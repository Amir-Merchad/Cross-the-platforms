//! The engine thread: captured input → `Router` → BLE HID reports or the iPad's Wi-Fi input service,
//! plus connection upkeep. Bluetooth is optional: capture, hotkeys and the iPad run without it.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{atomic::Ordering, mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};

use bridge_core::{
    ipadhid::IPAD_HOST,
    keyqueue::KeyQueue,
    names::display_name,
    router::{Action, Input, Router},
    settings::Settings,
};
use serde::Serialize;
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

use crate::{capture, ipadinput::{self, IpadCommand, IpadInputHandle, IpadInputState}, monitors, peripheral::HidPeripheral};

pub enum Command { Reload(Box<Settings>), SwitchTo(Option<String>), Stop }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HostStatus {
    pub id: String,
    pub name: Option<String>,
    pub connected: bool,
    pub interval_ms: u64,
    /// Notify timings while this host is active (0 otherwise).
    pub notify_avg_ms: f64,
    pub notify_max_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub advertising: bool,
    pub hosts: Vec<HostStatus>,
    pub active: Option<String>,
    pub notice: Option<String>,
    /// The Wi-Fi iPad's input link; `None` before the service reports.
    pub ipad: Option<IpadInputState>,
    /// The owner switched Bluetooth off in Settings.
    pub bluetooth_off: bool,
}

/// The tray's one line about what is happening.
pub fn status_line(s: &Status) -> String {
    let name = |id: &str| display_name(s.hosts.iter().find(|h| h.id == id).and_then(|h| h.name.as_deref()));
    match (&s.active, &s.notice) {
        (Some(id), _) => format!("Controlling {}", name(id)),
        (None, Some(n)) => n.clone(),
        (None, None) if !s.advertising && !s.bluetooth_off => "Starting Bluetooth…".into(),
        (None, None) => format!("{} device(s) connected", s.hosts.iter().filter(|h| h.connected).count()),
    }
}

/// `%APPDATA%\CrossThePlatforms\settings.json`
pub fn settings_path() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("CrossThePlatforms").join("settings.json")
}

pub struct EngineHandle { tx: mpsc::Sender<Command>, thread: Option<thread::JoinHandle<()>> }

impl EngineHandle {
    pub fn send(&self, c: Command) { let _ = self.tx.send(c); }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
        capture::unhook();
    }
}

const CONNECTION_POLL: Duration = Duration::from_millis(200);
const MONITOR_POLL: Duration = Duration::from_secs(2);
const CLOCK_LOG: Duration = Duration::from_secs(60);
const STATUS_REFRESH: Duration = Duration::from_secs(1);
const ADVERTISE_RETRY: Duration = Duration::from_secs(5);
const COMMAND_LATENCY: Duration = Duration::from_millis(50);

/// What the watcher thread last saw. Cloned by the input loop every poll.
#[derive(Clone, Default)]
struct Snapshot {
    ids: Vec<String>,
    up: HashSet<String>,
    intervals: HashMap<String, u64>,
    names: HashMap<String, Option<String>>,
    keyboard_ready: HashSet<String>,
    advertising: bool,
}

struct Watcher { stop: std::sync::Arc<std::sync::atomic::AtomicBool>, thread: Option<thread::JoinHandle<()>> }

impl Watcher {
    fn start(p: std::sync::Arc<HidPeripheral>, watched: std::sync::Arc<Mutex<Vec<String>>>, out: std::sync::Arc<Mutex<Snapshot>>) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::Builder::new().name("ctp-ble-watch".into()).spawn(move || {
            let mut names: HashMap<String, Option<String>> = HashMap::new();
            let mut last_adv = Instant::now() - MONITOR_POLL;
            let mut advertising = true;
            while !flag.load(Ordering::SeqCst) {
                let started = Instant::now();
                let mut ids: Vec<String> = p.hosts();
                for id in watched.lock().unwrap().iter() { if !ids.contains(id) { ids.push(id.clone()) } }
                let up: HashSet<String> = ids.iter().filter(|id| p.connected(id)).cloned().collect();
                let intervals = up.iter().map(|id| (id.clone(), p.report_interval_ms(id))).collect();
                for id in &up { names.entry(id.clone()).or_insert_with(|| p.host_name(id)); }
                let keyboard_ready = up.iter().filter(|id| p.keyboard_ready(id)).cloned().collect();
                if last_adv.elapsed() >= MONITOR_POLL { last_adv = Instant::now(); advertising = p.ensure_advertising(!up.is_empty()); }
                *out.lock().unwrap() = Snapshot { ids, up, intervals, names: names.clone(), keyboard_ready, advertising };
                let took = started.elapsed();
                if took > Duration::from_millis(100) { crate::diag::log(format_args!("ble watcher pass took {} ms (off the input thread)", took.as_millis())); }
                thread::sleep(CONNECTION_POLL.saturating_sub(took));
            }
        }).expect("spawn ble watcher");
        Self { stop, thread: Some(thread) }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

/// Where a host's reports go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sink { Ipad, Ble }

pub fn sink_for(host: &str) -> Sink { if host == IPAD_HOST { Sink::Ipad } else { Sink::Ble } }

/// Whether to run the Bluetooth peripheral at all.
pub fn wants_peripheral(s: &Settings) -> bool { s.bluetooth_enabled }

/// Starts the peripheral off the input thread, retrying until it comes up or is no longer wanted.
/// Each failure is reported so the Desk can say why.
struct PeripheralStarter { rx: mpsc::Receiver<Result<std::sync::Arc<HidPeripheral>, String>>, stop: std::sync::Arc<std::sync::atomic::AtomicBool> }

impl PeripheralStarter {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        thread::Builder::new().name("ctp-ble-start".into()).spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match HidPeripheral::start() {
                    Ok(p) => { let _ = tx.send(Ok(std::sync::Arc::new(p))); return }
                    Err(e) => if tx.send(Err(e.message().to_string())).is_err() { return },
                }
                let until = Instant::now() + ADVERTISE_RETRY;
                while Instant::now() < until && !flag.load(Ordering::SeqCst) { thread::sleep(Duration::from_millis(100)) }
            }
        }).expect("spawn ble starter");
        Self { rx, stop }
    }
}

impl Drop for PeripheralStarter {
    fn drop(&mut self) { self.stop.store(true, Ordering::SeqCst) }
}

/// The running peripheral and its watcher; dropped together, watcher first.
struct Ble { watcher: Watcher, peripheral: std::sync::Arc<HidPeripheral> }

/// Bluetooth is off, starting, or running.
enum BleState { Off, Starting(PeripheralStarter), Running(Ble) }

impl BleState {
    fn for_settings(s: &Settings) -> Self { if wants_peripheral(s) { BleState::Starting(PeripheralStarter::start()) } else { BleState::Off } }
    fn peripheral(&self) -> Option<&HidPeripheral> { match self { BleState::Running(b) => Some(&b.peripheral), _ => None } }
}

pub fn start(settings: Settings, on_status: impl Fn(Status) + Send + 'static) -> EngineHandle {
    let (tx, rx) = mpsc::channel();
    let thread = thread::spawn(move || run(settings, rx, on_status));
    EngineHandle { tx, thread: Some(thread) }
}

fn run(mut settings: Settings, commands: mpsc::Receiver<Command>, on_status: impl Fn(Status)) {
    let mut status = Status { bluetooth_off: !wants_peripheral(&settings), ..Status::default() };
    // Capture, router and hotkeys start at once; Bluetooth comes up in the background, if wanted.
    let mut ble = BleState::for_settings(&settings);
    let (ipad_tx, ipad_states) = mpsc::channel::<IpadInputState>();
    let ipad: IpadInputHandle = ipadinput::start(crate::ipad::ipad_dir(), move |s| { let _ = ipad_tx.send(s); });

    let (input_tx, input_rx) = mpsc::channel::<Input>();
    let mut capture = capture::start(input_tx);
    let mut router = Router::new(settings.clone(), monitors::list());
    router.set_interval(IPAD_HOST, IPAD_INTERVAL_MS);
    capture::set_hotkey_count(settings.placed_ids().len() as u8);
    let clock = Instant::now();
    let now_ms = || clock.elapsed().as_millis() as u64;

    let mut connected: HashSet<String> = HashSet::new();
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    let mut intervals: HashMap<String, u64> = HashMap::new();
    let mut last_clock = Instant::now() - CLOCK_LOG;
    let (mut last_conn, mut last_mon, mut last_status) = (Instant::now() - CONNECTION_POLL, Instant::now(), Instant::now());
    // Bluetooth bookkeeping runs on its own thread: WinRT calls there can take tens of ms, and every
    // ms the input loop waits is a visible hitch of the remote pointer. The loop only reads snapshots.
    let watched: std::sync::Arc<Mutex<Vec<String>>> = std::sync::Arc::new(Mutex::new(settings.placed_ids()));
    let snapshot: std::sync::Arc<Mutex<Snapshot>> = std::sync::Arc::default();
    let mut reported = Status::default();
    // Keyboard reports for a host that hasn't subscribed to the keyboard report yet.
    let mut keys = KeyQueue::default();

    'outer: loop {
        let wait = router.next_tick_ms().map(|t| Duration::from_millis(t.saturating_sub(now_ms()))).unwrap_or(COMMAND_LATENCY).min(COMMAND_LATENCY);
        let mut actions = Vec::new();
        let is_up = |id: &str| connected.contains(id);
        match input_rx.recv_timeout(wait) {
            Ok(i) => {
                actions.extend(router.on_input(i, now_ms(), &is_up));
                while let Ok(i) = input_rx.try_recv() { actions.extend(router.on_input(i, now_ms(), &is_up)); }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let mut ble_gone = false;
        while let Ok(c) = commands.try_recv() {
            match c {
                Command::Stop => break 'outer,
                Command::Reload(s) => {
                    let s = *s;
                    if wants_peripheral(&s) != wants_peripheral(&settings) {
                        crate::diag::log(format_args!("bluetooth switched {}", if wants_peripheral(&s) { "on" } else { "off" }));
                        // Off drops the watcher, then the peripheral: advertising stops, hosts disconnect.
                        ble = BleState::for_settings(&s);
                        if !wants_peripheral(&s) {
                            *snapshot.lock().unwrap() = Snapshot::default();
                            ble_gone = true;
                            status.notice = None;
                        }
                        status.bluetooth_off = !wants_peripheral(&s);
                    }
                    capture::set_hotkey_count(s.placed_ids().len() as u8);
                    router.set_settings(s.clone());
                    *watched.lock().unwrap() = s.placed_ids();
                    settings = s;
                }
                Command::SwitchTo(None) => actions.extend(router.on_input(Input::Hotkey(0), now_ms(), &is_up)),
                Command::SwitchTo(Some(id)) => match settings.placed_ids().iter().position(|d| *d == id) {
                    Some(i) => actions.extend(router.on_input(Input::Hotkey(i as u8 + 1), now_ms(), &is_up)),
                    None => status.notice = Some(format!("{} is not placed yet", display_name(names.get(&id).and_then(|n| n.as_deref())))),
                },
            }
        }
        actions.extend(router.tick(now_ms(), &is_up));
        if ble_gone {
            // Bluetooth hosts went with the peripheral; the tick returns input to Windows if one had it.
            connected.retain(|id| id == IPAD_HOST);
            actions.extend(router.tick(now_ms(), &|id: &str| connected.contains(id)));
        }

        // The iPad's link: Ready is connected. Losing it makes the router's tick return input to Windows.
        let mut ipad_changed = false;
        while let Ok(st) = ipad_states.try_recv() {
            if status.ipad != Some(st) { crate::diag::log(format_args!("iPad input {st:?}")); }
            status.ipad = Some(st);
            ipad_changed = true;
        }
        if ipad_changed {
            if status.ipad == Some(IpadInputState::Ready) { connected.insert(IPAD_HOST.to_string()); } else { connected.remove(IPAD_HOST); }
            actions.extend(router.tick(now_ms(), &|id: &str| connected.contains(id)));
        }

        if let BleState::Starting(starter) = &ble {
            let mut started = None;
            while let Ok(r) = starter.rx.try_recv() {
                match r {
                    Ok(p) => started = Some(p),
                    Err(e) => status.notice = Some(format!("Bluetooth not ready: {e}")),
                }
            }
            if let Some(p) = started {
                crate::diag::log("bluetooth peripheral started");
                status.advertising = true;
                status.notice = None;
                ble = BleState::Running(Ble { watcher: Watcher::start(p.clone(), watched.clone(), snapshot.clone()), peripheral: p });
            }
        }

        if last_conn.elapsed() >= CONNECTION_POLL {
            last_conn = Instant::now();
            let snap = snapshot.lock().unwrap().clone();
            for id in &snap.ids {
                let up = snap.up.contains(id);
                if up && !connected.contains(id) {
                    crate::diag::log(format_args!("{id} connected; interval {} ms; keyboard subscribed: {}", snap.intervals.get(id).copied().unwrap_or(0), snap.keyboard_ready.contains(id)));
                }
                if !up && connected.contains(id) { crate::diag::log(format_args!("{id} disconnected")); }
                if up { connected.insert(id.clone()); } else { connected.remove(id); }
            }
            for (id, &ms) in &snap.intervals {
                if intervals.get(id) != Some(&ms) {
                    if intervals.contains_key(id) { crate::diag::log(format_args!("{id} interval now {ms} ms")); }
                    intervals.insert(id.clone(), ms);
                    router.set_interval(id, ms);
                }
            }
            for (id, n) in &snap.names { names.insert(id.clone(), n.clone()); }
            actions.extend(router.tick(now_ms(), &|id: &str| connected.contains(id)));
            if let Some(p) = ble.peripheral() {
                for id in &snap.keyboard_ready {
                    if keys.pending(id) > 0 {
                        for r in keys.drain(id, now_ms()) { p.send_keyboard(id, r); }
                        status.notice = None;
                    }
                }
            }
            status.hosts = snap.ids.iter().filter(|id| connected.contains(*id) || settings.placed_ids().contains(*id))
                .map(|id| HostStatus { id: id.clone(), name: names.get(id).cloned().flatten(), connected: connected.contains(id), interval_ms: intervals.get(id).copied().unwrap_or(0), notify_avg_ms: 0.0, notify_max_ms: 0.0 })
                .collect();
            if let Some(h) = ipad_host(status.ipad, &settings) { status.hosts.push(h) }
            status.hosts.sort_by(|a, b| a.id.cmp(&b.id));
            if let BleState::Running(_) = ble {
                status.advertising = snap.advertising;
                if !status.advertising && status.notice.is_none() { status.notice = Some("Bluetooth advertising restarting…".into()) }
            } else {
                status.advertising = false;
            }
        }
        if last_clock.elapsed() >= CLOCK_LOG {
            last_clock = Instant::now();
            let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            crate::diag::log(crate::diag::clock_line(unix, status.hosts.iter().filter(|h| h.connected).count(), status.active.is_some()));
        }
        if last_mon.elapsed() >= MONITOR_POLL {
            last_mon = Instant::now();
            router.set_monitors(monitors::list());
        }

        for a in actions {
            match a {
                Action::Suppress(on) => {
                    if on { capture::release_own_focus(); }
                    capture::SUPPRESS.store(on, Ordering::SeqCst);
                }
                Action::Mouse { host, report } => match sink_for(&host) {
                    Sink::Ipad => ipad.send(IpadCommand::Mouse(report)),
                    Sink::Ble => { if let Some(p) = ble.peripheral() { p.send_mouse(&host, report); } }
                },
                Action::Keyboard { host, report } if sink_for(&host) == Sink::Ipad => ipad.send(IpadCommand::Keyboard(report)),
                Action::Keyboard { host, report } => {
                    // Keep order: once something is queued for this host, queue behind it.
                    if keys.pending(&host) > 0 || !ble.peripheral().is_some_and(|p| p.send_keyboard(&host, report)) {
                        keys.push(&host, report, now_ms());
                        status.notice = Some(format!("Keyboard on {} is still connecting…", display_name(names.get(&host).and_then(|n| n.as_deref()))));
                    }
                }
                Action::PlaceCursor(x, y) => unsafe { let _ = SetCursorPos(x, y); },
                Action::Active(a) => {
                    crate::diag::log(format_args!("active -> {a:?}"));
                    if status.active.as_deref() == Some(IPAD_HOST) && a.as_deref() != Some(IPAD_HOST) { ipad.send(IpadCommand::Leave); }
                    if a.as_deref() == Some(IPAD_HOST) && status.active.as_deref() != Some(IPAD_HOST) { ipad.send(IpadCommand::Enter); }
                    status.active = a;
                    status.notice = None;
                }
                Action::Notice(n) => status.notice = Some(n),
            }
        }

        if status.active.is_some() && last_status.elapsed() >= STATUS_REFRESH {
            last_status = Instant::now();
            let (_, avg, max) = ble.peripheral().map_or((0, 0.0, 0.0), |p| p.take_stats());
            if let Some(h) = status.hosts.iter_mut().find(|h| Some(&h.id) == status.active.as_ref()) { h.notify_avg_ms = avg; h.notify_max_ms = max; }
        }
        if status != reported { reported = status.clone(); on_status(status.clone()); }
    }

    // Shutdown: input back to Windows before anything else, then release the device, then stop advertising.
    capture.stop();
    for a in router.on_input(Input::Hotkey(0), now_ms(), &|_: &str| true) {
        match a {
            Action::Mouse { host, report } if sink_for(&host) == Sink::Ipad => ipad.send(IpadCommand::Mouse(report)),
            Action::Keyboard { host, report } if sink_for(&host) == Sink::Ipad => ipad.send(IpadCommand::Keyboard(report)),
            Action::Mouse { host, report } => { if let Some(p) = ble.peripheral() { p.send_mouse(&host, report); } }
            Action::Keyboard { host, report } => { if let Some(p) = ble.peripheral() { p.send_keyboard(&host, report); } }
            Action::Active(None) if status.active.as_deref() == Some(IPAD_HOST) => ipad.send(IpadCommand::Leave),
            _ => {}
        }
    }
    drop(ipad); // releases everything held on the iPad, removes its virtual devices, closes the tunnel
    if let BleState::Running(Ble { watcher, peripheral }) = ble {
        drop(watcher); // joins the watcher, releasing its handle on the peripheral
        drop(peripheral);
    }
}

/// The iPad's pointer pacing: reports are tiny and the link takes them at about 8 ms.
const IPAD_INTERVAL_MS: u64 = 8;

/// The Wi-Fi iPad as a host: listed once its service has reported, or when it is placed.
fn ipad_host(state: Option<IpadInputState>, settings: &Settings) -> Option<HostStatus> {
    let placed = settings.placed_ids().iter().any(|d| d == IPAD_HOST);
    if state.is_none() && !placed { return None }
    if state.is_some_and(|s| s == IpadInputState::NotSetUp) && !placed { return None }
    let connected = state == Some(IpadInputState::Ready);
    Some(HostStatus { id: IPAD_HOST.into(), name: Some("iPad".into()), connected, interval_ms: if connected { IPAD_INTERVAL_MS } else { 0 }, notify_avg_ms: 0.0, notify_max_ms: 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: Option<&str>) -> HostStatus {
        HostStatus { id: "AA:BB".into(), name: name.map(Into::into), connected: true, interval_ms: 0, notify_avg_ms: 0.0, notify_max_ms: 0.0 }
    }
    fn line(name: Option<&str>) -> String { status_line(&Status { advertising: true, hosts: vec![host(name)], active: Some("AA:BB".into()), notice: None, ..Status::default() }) }

    #[test] fn ipad_wifi_goes_to_the_service() {
        assert_eq!(sink_for(IPAD_HOST), Sink::Ipad);
        assert_eq!(sink_for("AA:BB:CC:DD:EE:FF"), Sink::Ble);
    }
    #[test] fn bluetooth_switch_decides_the_peripheral() { // Review Focus 3
        assert!(wants_peripheral(&Settings::default()));
        assert!(!wants_peripheral(&Settings { bluetooth_enabled: false, ..Settings::default() }));
    }
    #[test] fn bluetooth_off_does_not_say_starting() {
        let s = Status { bluetooth_off: true, ..Status::default() };
        assert_eq!(status_line(&s), "0 device(s) connected");
        assert_eq!(status_line(&Status::default()), "Starting Bluetooth…");
    }
    #[test] fn ipad_host_is_connected_only_when_ready() {
        let s = Settings::default();
        assert_eq!(ipad_host(None, &s), None, "nothing to show before the service reports");
        assert_eq!(ipad_host(Some(IpadInputState::NotSetUp), &s), None, "not set up and not placed: hidden");
        assert!(!ipad_host(Some(IpadInputState::NotFound), &s).unwrap().connected);
        assert!(ipad_host(Some(IpadInputState::Ready), &s).unwrap().connected);
        let placed = Settings { attachments: vec![bridge_core::layout::Attachment { device_id: IPAD_HOST.into(), monitor_id: "M1".into(), edge: bridge_core::layout::Edge::Right }], ..Settings::default() };
        assert!(ipad_host(None, &placed).is_some(), "placed: always listed");
    }

    #[test] fn status_line_never_shows_a_raw_id_or_a_blank_name() {
        assert_eq!(line(Some("iPad Air")), "Controlling iPad Air");
        assert_eq!(line(None), "Controlling Unnamed device");
        assert_eq!(line(Some(" ")), "Controlling Unnamed device");
    }
}
