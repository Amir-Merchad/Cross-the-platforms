//! The engine thread: captured input → `Router` → BLE HID reports, plus connection upkeep.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

use bridge_core::{
    router::{Action, Input, Router},
    settings::Settings,
};
use serde::Serialize;
use windows::{Devices::Bluetooth::BluetoothLEPreferredConnectionParametersRequest, Win32::UI::WindowsAndMessaging::SetCursorPos};

use crate::{capture, monitors, peripheral::HidPeripheral};

pub enum Command { Reload(Settings), SwitchTo(Option<String>), Stop }

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
const STATUS_REFRESH: Duration = Duration::from_secs(1);
const ADVERTISE_RETRY: Duration = Duration::from_secs(5);
const COMMAND_LATENCY: Duration = Duration::from_millis(50);

pub fn start(settings: Settings, on_status: impl Fn(Status) + Send + 'static) -> EngineHandle {
    let (tx, rx) = mpsc::channel();
    let thread = thread::spawn(move || run(settings, rx, on_status));
    EngineHandle { tx, thread: Some(thread) }
}

fn run(settings: Settings, commands: mpsc::Receiver<Command>, on_status: impl Fn(Status)) {
    let mut status = Status::default();
    // Advertise first; retry until the adapter is available (Bluetooth off, adapter busy…).
    let peripheral = loop {
        match HidPeripheral::start() {
            Ok(p) => break p,
            Err(e) => {
                status.notice = Some(format!("Bluetooth not ready: {}", e.message()));
                on_status(status.clone());
                if let Ok(Command::Stop) = commands.recv_timeout(ADVERTISE_RETRY) { return }
            }
        }
    };
    status.advertising = true;
    status.notice = None;

    let (input_tx, input_rx) = mpsc::channel::<Input>();
    let mut capture = capture::start(input_tx);
    let mut router = Router::new(settings.clone(), monitors::list());
    let mut settings = settings;
    let clock = Instant::now();
    let now_ms = || clock.elapsed().as_millis() as u64;

    let mut connected: HashSet<String> = HashSet::new();
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    let mut intervals: HashMap<String, u64> = HashMap::new();
    let mut fast: HashMap<String, BluetoothLEPreferredConnectionParametersRequest> = HashMap::new();
    let (mut last_conn, mut last_mon, mut last_status) = (Instant::now() - CONNECTION_POLL, Instant::now(), Instant::now());
    let mut reported = Status::default();

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
        while let Ok(c) = commands.try_recv() {
            match c {
                Command::Stop => break 'outer,
                Command::Reload(s) => { router.set_settings(s.clone()); settings = s; }
                Command::SwitchTo(None) => actions.extend(router.on_input(Input::Hotkey(0), now_ms(), &is_up)),
                Command::SwitchTo(Some(id)) => match settings.attachments.iter().position(|a| a.device_id == id) {
                    Some(i) => actions.extend(router.on_input(Input::Hotkey(i as u8 + 1), now_ms(), &is_up)),
                    None => status.notice = Some(format!("{id} is not placed in the layout yet")),
                },
            }
        }
        actions.extend(router.tick(now_ms(), &is_up));

        if last_conn.elapsed() >= CONNECTION_POLL {
            last_conn = Instant::now();
            let mut ids: HashSet<String> = peripheral.hosts().into_iter().collect();
            ids.extend(settings.attachments.iter().map(|a| a.device_id.clone()));
            for id in &ids {
                let up = peripheral.connected(id);
                if up && !connected.contains(id) {
                    let ms = peripheral.report_interval_ms(id);
                    intervals.insert(id.clone(), ms);
                    router.set_interval(id, ms);
                    if let Some(req) = peripheral.request_fast(id) { fast.insert(id.clone(), req); }
                    names.entry(id.clone()).or_insert_with(|| peripheral.host_name(id));
                }
                if up { connected.insert(id.clone()); } else { connected.remove(id); fast.remove(id); }
            }
            actions.extend(router.tick(now_ms(), &|id: &str| connected.contains(id)));
            status.hosts = ids.iter().filter(|id| connected.contains(*id) || settings.attachments.iter().any(|a| &a.device_id == *id))
                .map(|id| HostStatus { id: id.clone(), name: names.get(id).cloned().flatten(), connected: connected.contains(id), interval_ms: intervals.get(id).copied().unwrap_or(0), notify_avg_ms: 0.0, notify_max_ms: 0.0 })
                .collect();
            status.hosts.sort_by(|a, b| a.id.cmp(&b.id));
        }
        if last_mon.elapsed() >= MONITOR_POLL {
            last_mon = Instant::now();
            router.set_monitors(monitors::list());
        }

        for a in actions {
            match a {
                Action::Suppress(on) => capture::SUPPRESS.store(on, Ordering::SeqCst),
                Action::Mouse { host, report } => { peripheral.send_mouse(&host, report); }
                Action::Keyboard { host, report } => { peripheral.send_keyboard(&host, report); }
                Action::PlaceCursor(x, y) => unsafe { let _ = SetCursorPos(x, y); },
                Action::Active(a) => { status.active = a; status.notice = None; }
                Action::Notice(n) => status.notice = Some(n),
            }
        }

        if status.active.is_some() && last_status.elapsed() >= STATUS_REFRESH {
            last_status = Instant::now();
            let (_, avg, max) = peripheral.take_stats();
            if let Some(h) = status.hosts.iter_mut().find(|h| Some(&h.id) == status.active.as_ref()) { h.notify_avg_ms = avg; h.notify_max_ms = max; }
        }
        if status != reported { reported = status.clone(); on_status(status.clone()); }
    }

    // Shutdown: input back to Windows before anything else, then release the device, then stop advertising.
    capture.stop();
    for a in router.on_input(Input::Hotkey(0), now_ms(), &|_: &str| true) {
        match a {
            Action::Mouse { host, report } => { peripheral.send_mouse(&host, report); }
            Action::Keyboard { host, report } => { peripheral.send_keyboard(&host, report); }
            _ => {}
        }
    }
    drop(peripheral);
}
