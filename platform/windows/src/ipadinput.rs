//! iPad keyboard and mouse over Wi-Fi, no Bluetooth: a virtual mouse and keyboard on the iPad's
//! UniversalHID service, through the same tunnel kind the clipboard uses. Own thread and single-threaded
//! runtime, because idevice futures are not `Send`, and so the engine never waits on the network.
//!
//! No display stream: the proofs found input lands without one (docs/spikes/ipad-pointer.md).
//! Never logs keys. Errors carry no key or clipboard content.

use std::{path::PathBuf, thread, time::Duration};

use bridge_core::{
    hid::{KeyboardReport, MouseReport, split_delta},
    ipadhid::{KeyEvent, KeyTranslator, pointer_reports, scroll_reports},
};
use idevice::{
    ReadWrite, RemoteXpcClient, RsdService,
    core_device::hid::{KeyboardUsage, MainKeyboardError, MainKeyboardService, UniversalHidServiceClient},
    rsd::RsdHandshake,
    tcp::handle::{AdapterHandle, StreamHandle},
    xpc::XPCObject,
};
use serde::Serialize;
use tokio::{sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel}, time::timeout};

use crate::{diag, ipad::{self, IpadStatus}};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum IpadInputState { NotSetUp, Connecting, Ready, OnCall, NeedsUsb, NotFound, NeedsDeveloperMode }

#[derive(Clone, Debug, PartialEq)]
pub enum IpadCommand { Mouse(MouseReport), Keyboard(KeyboardReport), Enter, Leave, Stop }

pub struct IpadInputHandle { tx: UnboundedSender<IpadCommand>, thread: Option<thread::JoinHandle<()>> }

impl IpadInputHandle {
    /// Never blocks: an unbounded channel. Motion queued while the link is busy is summed on the
    /// service side, so a slow link loses detail, not time.
    pub fn send(&self, c: IpadCommand) { let _ = self.tx.send(c); }
}

impl Drop for IpadInputHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(IpadCommand::Stop);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

const KEEPALIVE_EVERY: Duration = Duration::from_millis(500);
/// A round trip is about 8 ms over Wi-Fi; a quarter second is a miss.
const KEEPALIVE_LIMIT: Duration = Duration::from_millis(250);
/// A write that cannot be queued in this long means the link is gone.
const WRITE_LIMIT: Duration = Duration::from_millis(500);
const CONNECT_LIMIT: Duration = Duration::from_secs(20);
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// Bounded cleanup: a dead tunnel must not hang Quit.
const CLEANUP_LIMIT: Duration = Duration::from_secs(2);

pub fn start(dir: PathBuf, on_state: impl Fn(IpadInputState) + Send + 'static) -> IpadInputHandle {
    let (tx, rx) = unbounded_channel();
    let thread = thread::Builder::new().name("ipad-input".into()).stack_size(16 << 20).spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        tokio::task::LocalSet::new().block_on(&rt, run(dir, rx, on_state));
        // A pending mDNS lookup on the blocking pool must not hold up Quit.
        rt.shutdown_background();
    }).expect("ipad-input thread");
    IpadInputHandle { tx, thread: Some(thread) }
}

/// What a connect or session error means for the owner. No raw errors reach the hub.
pub fn classify(err: &str) -> IpadInputState {
    let lower = err.to_lowercase();
    if lower.contains("call") && (lower.contains("phone") || lower.contains("voip") || lower.contains("facetime")) {
        return IpadInputState::OnCall;
    }
    match ipad::classify(err) {
        IpadStatus::NotPaired => IpadInputState::NotSetUp,
        IpadStatus::Connecting | IpadStatus::Connected { .. } => IpadInputState::Connecting,
        IpadStatus::Unavailable(m) if m.contains("USB") => IpadInputState::NeedsUsb,
        IpadStatus::Unavailable(m) if m.contains("not found on Wi-Fi") => IpadInputState::NotFound,
        IpadStatus::Unavailable(m) if m.contains("Developer Mode") => IpadInputState::NeedsDeveloperMode,
        // Disk image files missing on the PC: only the setup step over USB fetches them.
        IpadStatus::Unavailable(m) if m.contains("missing") && m.contains("ddi") => IpadInputState::NeedsUsb,
        IpadStatus::Unavailable(_) => IpadInputState::Connecting,
    }
}

/// Two missed round trips in a row mark the link down; one success resets the count.
#[derive(Default)]
pub struct Keepalive { misses: u8 }

impl Keepalive {
    /// Returns true when the link is down.
    pub fn tick(&mut self, ok: bool) -> bool {
        self.misses = if ok { 0 } else { self.misses.saturating_add(1) };
        self.misses >= 2
    }
}

/// One thing to send, after summing.
#[derive(Clone, Debug, PartialEq)]
pub enum Out { Move { dx: i32, dy: i32, buttons: u8 }, Mouse(MouseReport), Keys(KeyboardReport), Enter, Leave }

/// Commands waiting while the link is busy. Consecutive moves with the same buttons are summed;
/// button changes, scrolls and keys keep their place and order, and are never dropped.
#[derive(Default)]
pub struct Pending { items: Vec<Out> }

impl Pending {
    pub fn push(&mut self, c: IpadCommand) {
        let out = match c {
            IpadCommand::Mouse(r) if r.wheel == 0 && r.pan == 0 => {
                if let Some(Out::Move { dx, dy, buttons }) = self.items.last_mut()
                    && *buttons == r.buttons {
                    *dx += r.dx as i32;
                    *dy += r.dy as i32;
                    return;
                }
                Out::Move { dx: r.dx as i32, dy: r.dy as i32, buttons: r.buttons }
            }
            IpadCommand::Mouse(r) => Out::Mouse(r),
            IpadCommand::Keyboard(r) => Out::Keys(r),
            IpadCommand::Enter => Out::Enter,
            IpadCommand::Leave => Out::Leave,
            IpadCommand::Stop => return,
        };
        self.items.push(out);
    }

    pub fn take(&mut self) -> Vec<Out> { std::mem::take(&mut self.items) }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
}

/// The reports for one summed item: motion first, then scroll.
pub fn reports_for(out: &Out) -> Vec<Vec<u8>> {
    match out {
        Out::Move { dx, dy, buttons } => {
            let steps = split_delta(*dx, *dy);
            if steps.is_empty() { return pointer_reports(&MouseReport { buttons: *buttons, ..Default::default() }) }
            steps.into_iter().flat_map(|(dx, dy)| pointer_reports(&MouseReport { dx, dy, buttons: *buttons, ..Default::default() })).collect()
        }
        Out::Mouse(r) => {
            let mut v = if r.dx != 0 || r.dy != 0 { pointer_reports(&MouseReport { wheel: 0, pan: 0, ..*r }) } else { vec![] };
            v.extend(scroll_reports(r));
            if v.is_empty() { v = pointer_reports(r) }
            v
        }
        _ => vec![],
    }
}

/// Grows from 2 s to 30 s; a success starts it over.
fn next_backoff(d: Duration) -> Duration { (d * 2).min(BACKOFF_MAX) }

async fn run(dir: PathBuf, mut rx: UnboundedReceiver<IpadCommand>, on_state: impl Fn(IpadInputState)) {
    let mut reported = None;
    // Reports only changes, and says whether it was one.
    let mut report = |s: IpadInputState| { let new = reported != Some(s); if new { reported = Some(s); on_state(s) } new };
    let mut backoff = BACKOFF_MIN;
    let _ = report(IpadInputState::Connecting);
    loop {
        // Stop wins over a slow connect, so Quit never waits on the network.
        let attempt = tokio::select! {
            r = timeout(CONNECT_LIMIT, Session::open(&dir)) => r.unwrap_or_else(|_| Err("iPad didn't answer in time".into())),
            () = until_stop(&mut rx) => return,
        };
        let wait = match attempt {
            Ok(mut s) => {
                diag::log("iPad input connected over Wi-Fi");
                backoff = BACKOFF_MIN;
                let _ = report(IpadInputState::Ready);
                let end = s.serve(&mut rx).await;
                let _ = timeout(CLEANUP_LIMIT * 2, s.close()).await;
                match end {
                    End::Stop => return,
                    End::Down(e) => {
                        diag::log(format_args!("iPad input link down: {e}"));
                        let _ = report(classify(&e));
                        BACKOFF_MIN
                    }
                }
            }
            Err(e) if e == ipad::JUST_MOUNTED => { let _ = report(IpadInputState::Connecting); Duration::from_millis(500) }
            Err(e) => {
                if report(classify(&e)) { diag::log(format_args!("iPad input not connected: {e}")); }
                let w = backoff;
                backoff = next_backoff(backoff);
                w
            }
        };
        // Wait, dropping input meant for a link that is not there (the router only routes to a
        // connected host, so this is the brief window around a drop). Stop still ends at once.
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Err(_) => break,
                Ok(None) | Ok(Some(IpadCommand::Stop)) => return,
                Ok(Some(_)) => {}
            }
        }
    }
}

enum End { Stop, Down(String) }

/// Returns on Stop or a closed channel; input that arrives meanwhile has no link to go to.
async fn until_stop(rx: &mut UnboundedReceiver<IpadCommand>) {
    while let Some(c) = rx.recv().await { if c == IpadCommand::Stop { return } }
}

const UHID_SERVICE: &str = "com.apple.coredevice.hid.universalhidservice";
const UHID_FEATURE: &str = "com.apple.coredevice.feature.remote.universalhidservice";

/// One tunnel, a HID client for the keyboard and the keepalive, and a raw client for the mouse
/// (idevice builds only a keyboard; `createService` takes any descriptor).
struct Session {
    adapter: AdapterHandle,
    hid: UniversalHidServiceClient<Box<dyn ReadWrite>>,
    raw: RemoteXpcClient<StreamHandle>,
    keyboard: Option<MainKeyboardService>,
    mouse: Option<u64>,
    keys: KeyTranslator,
    buttons: u8,
}

impl Session {
    async fn open(dir: &std::path::Path) -> Result<Session, String> {
        let (mut adapter, mut handshake, wifi) = ipad::connect_tunnel(dir).await?;
        let hid = match UniversalHidServiceClient::connect_rsd(&mut adapter, &mut handshake).await {
            Ok(h) => h,
            Err(e) if format!("{e:?}").contains("ServiceNotFound") => {
                let r = ipad::mount(dir, &mut adapter, &mut handshake, wifi).await;
                let _ = timeout(CLEANUP_LIMIT, adapter.close()).await;
                r?;
                return Err(ipad::JUST_MOUNTED.into());
            }
            Err(e) => { let _ = timeout(CLEANUP_LIMIT, adapter.close()).await; return Err(format!("universal HID: {e:?}")) }
        };
        let mut s = match open_raw(&mut adapter, &handshake).await {
            Ok(raw) => Session { adapter, hid, raw, keyboard: None, mouse: None, keys: KeyTranslator::default(), buttons: 0 },
            Err(e) => { let _ = timeout(CLEANUP_LIMIT, adapter.close()).await; return Err(e) }
        };
        if let Err(e) = s.create_devices().await { s.close().await; return Err(e) }
        Ok(s)
    }

    async fn create_devices(&mut self) -> Result<(), String> {
        // A session that died with the link may have left its mouse behind, maybe with a button down.
        // Unproven whether the iPad removes it by itself, so remove ours by name first.
        if let Ok(Ok(list)) = timeout(Duration::from_secs(2), self.hid.list_connected_services()).await {
            for stale in list.iter().filter(|s| s.product.as_deref() == Some(MOUSE_NAME)) {
                let _ = timeout(WRITE_LIMIT, self.raw.send_object(service_request("removeService", stale.service_id), false)).await;
            }
        }
        self.keyboard = Some(self.hid.create_main_keyboard().await.map_err(|e| format!("create virtual keyboard: {e}"))?);
        self.raw.send_object(mouse_create_request(), true).await.map_err(|e| format!("create mouse: {e:?}"))?;
        let reply = timeout(Duration::from_secs(3), self.raw.recv()).await.map_err(|_| "create mouse: no reply".to_string())?.map_err(|e| format!("create mouse reply: {e:?}"))?;
        let id = reply.as_dictionary().and_then(|d| d.get("serviceID")).and_then(plist::Value::as_unsigned_integer).filter(|id| *id != 0).ok_or("iPad refused the virtual mouse")?;
        self.mouse = Some(id);
        Ok(())
    }

    /// Sends until the link drops or Stop. Each wake drains everything queued, sums it, then sends.
    async fn serve(&mut self, rx: &mut UnboundedReceiver<IpadCommand>) -> End {
        let mut alive = Keepalive::default();
        let mut tick = tokio::time::interval(KEEPALIVE_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut pending = Pending::default();
        loop {
            tokio::select! {
                c = rx.recv() => {
                    let Some(c) = c else { return End::Stop };
                    if c == IpadCommand::Stop { return End::Stop }
                    pending.push(c);
                    while let Ok(c) = rx.try_recv() {
                        if c == IpadCommand::Stop { return End::Stop }
                        pending.push(c);
                    }
                    for out in pending.take() {
                        if let Err(e) = self.send(&out).await { return End::Down(e) }
                    }
                }
                _ = tick.tick() => {
                    let ok = matches!(timeout(KEEPALIVE_LIMIT, self.hid.list_connected_services()).await, Ok(Ok(_)));
                    if alive.tick(ok) { return End::Down("keepalive missed twice".into()) }
                }
            }
        }
    }

    async fn send(&mut self, out: &Out) -> Result<(), String> {
        match out {
            Out::Enter => Ok(()),
            Out::Leave => self.release().await,
            Out::Keys(r) => {
                for ev in self.keys.apply(r) { self.key(ev).await? }
                Ok(())
            }
            Out::Move { buttons, .. } | Out::Mouse(MouseReport { buttons, .. }) => {
                self.buttons = *buttons;
                for report in reports_for(out) { self.report(report).await? }
                Ok(())
            }
        }
    }

    async fn report(&mut self, report: Vec<u8>) -> Result<(), String> {
        let id = self.mouse.ok_or("no virtual mouse")?;
        match timeout(WRITE_LIMIT, self.raw.send_object(send_request(id, report), false)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(format!("mouse report: {e:?}")),
            Err(_) => Err("mouse report timed out".into()),
        }
    }

    async fn key(&mut self, ev: KeyEvent) -> Result<(), String> {
        let kb = self.keyboard.as_mut().ok_or("no virtual keyboard")?;
        let (usage, down) = match ev { KeyEvent::Down(u) => (u, true), KeyEvent::Up(u) => (u, false) };
        // Usages the iPad keyboard does not take are skipped, not fatal.
        let Ok(usage) = KeyboardUsage::new(usage as u16) else { return Ok(()) };
        let send = async { if down { self.hid.main_keyboard_key_down(kb, usage).await } else { self.hid.main_keyboard_key_up(kb, usage).await } };
        match timeout(WRITE_LIMIT, send).await {
            Ok(Ok(())) | Ok(Err(MainKeyboardError::KeyAlreadyPressed | MainKeyboardError::KeyNotPressed | MainKeyboardError::InvalidUsage)) => Ok(()),
            Ok(Err(e)) => Err(format!("virtual keyboard: {e}")),
            Err(_) => Err("virtual keyboard timed out".into()),
        }
    }

    /// Everything held goes up on the iPad: keys, then buttons. Review Focus 5.
    async fn release(&mut self) -> Result<(), String> {
        for ev in self.keys.release_all() { self.key(ev).await? }
        if let Some(kb) = self.keyboard.as_mut() {
            let _ = timeout(WRITE_LIMIT, self.hid.reset_main_keyboard(kb)).await;
        }
        if std::mem::take(&mut self.buttons) != 0 {
            for r in pointer_reports(&MouseReport::default()) { self.report(r).await? }
        }
        Ok(())
    }

    /// Lets go of everything, removes the virtual devices, closes the tunnel. Each step is bounded.
    async fn close(mut self) {
        let _ = timeout(CLEANUP_LIMIT, self.release()).await;
        if let Some(mut kb) = self.keyboard.take() {
            let _ = timeout(CLEANUP_LIMIT, self.hid.remove_main_keyboard(&mut kb)).await;
        }
        if let Some(id) = self.mouse.take() {
            let _ = timeout(CLEANUP_LIMIT, self.raw.send_object(service_request("resetGestureState", id), false)).await;
            let _ = timeout(CLEANUP_LIMIT, self.raw.send_object(service_request("removeService", id), false)).await;
        }
        let _ = timeout(CLEANUP_LIMIT, self.adapter.close()).await;
    }
}

async fn open_raw(adapter: &mut AdapterHandle, handshake: &RsdHandshake) -> Result<RemoteXpcClient<StreamHandle>, String> {
    let port = handshake.services.get(UHID_SERVICE).ok_or("universal HID service not listed")?.port;
    let stream = adapter.connect(port).await.map_err(|e| format!("HID connect: {e}"))?;
    let mut raw = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
    raw.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
    Ok(raw)
}

fn uhid_request(payload: XPCObject) -> XPCObject {
    idevice::xpc!({ "featureIdentifier": UHID_FEATURE, "messageType": "Request", "payload": payload })
}

fn service_request(operation: &'static str, id: u64) -> XPCObject {
    uhid_request(idevice::xpc!({ operation: { "_0": id } }))
}

fn send_request(id: u64, report: Vec<u8>) -> XPCObject {
    uhid_request(idevice::xpc!({ "send": { "_0": report, "_1": id } }))
}

/// Boot mouse descriptor, report ID 1. The daemon ignores it and decodes by report ID (5 for the
/// pointer, 7 for scroll); it only has to make the iPad add a pointer device. As the spike's `Spec::default`.
const MOUSE_DESCRIPTOR: [u8; 54] = [
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01,
    0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x03, 0x81, 0x06, 0xC0, 0xC0,
];

const MOUSE_NAME: &str = "Cross the Platforms mouse";

/// The proven virtual mouse: Apple vendor and product ids, USB transport, a Universal Control service.
fn mouse_create_request() -> XPCObject {
    const ID: u64 = 0x1_0000_2003;
    const VENDOR: i64 = 0x05AC;
    const PRODUCT: i64 = 0x0269;
    uhid_request(idevice::xpc!({
        "createService": {
            "_0": {
                "DeviceUsagePairs": [{ "DeviceUsage": 2_i64, "DeviceUsagePage": 1_i64 }],
                "PrimaryUsage": 2_u64,
                "PrimaryUsagePage": 1_u64,
                "Product": MOUSE_NAME,
                "ProductID": PRODUCT,
                "VendorID": VENDOR,
                "_CoreDevice_codablePropertyStorage": {
                    "Manufacturer": { "string": "idevice" },
                    "Product": { "string": MOUSE_NAME },
                    "ProductID": { "int": PRODUCT },
                    "VendorID": { "int": VENDOR },
                    "PrimaryUsage": { "int": 2_i64 },
                    "PrimaryUsagePage": { "int": 1_i64 },
                    "DeviceUsagePairs": {
                        "array": [{ "dictionary": { "DeviceUsage": { "int": 2_i64 }, "DeviceUsagePage": { "int": 1_i64 } } }],
                    },
                    "Transport": { "string": "USB" },
                    "ReportDescriptor": { "data": MOUSE_DESCRIPTOR.to_vec() },
                    "UniversalControlVirtualService": { "bool": true },
                    "_ServiceID": { "uint": ID },
                },
                "_ServiceID": ID,
            },
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn classify_stream_error() {
        assert_eq!(classify("display: the device is on a phone or VoIP call"), IpadInputState::OnCall);
        assert_eq!(classify("pairing record: NotFound"), IpadInputState::NotSetUp);
        assert_eq!(classify("pair verify: early eof"), IpadInputState::NotSetUp);
        assert_eq!(classify("iPad's _remotepairing._tcp not found on the network"), IpadInputState::NotFound);
        assert_eq!(classify("core device proxy (Developer Mode on? DDI mounted?): X"), IpadInputState::NeedsDeveloperMode);
        assert_eq!(classify("DDI mount over Wi-Fi failed: X"), IpadInputState::NeedsUsb);
        assert_eq!(classify(ipad::JUST_MOUNTED), IpadInputState::Connecting);
        assert_eq!(classify("keepalive missed twice"), IpadInputState::Connecting);
    }
    #[test] fn missing_disk_image_files_ask_for_usb() {
        assert_eq!(classify("missing C:\\x\\ipad\\ddi\\Image.dmg"), IpadInputState::NeedsUsb);
    }
    #[test] fn missed_keepalives_mark_down() { // Review Focus 1
        let mut k = Keepalive::default();
        assert!(!k.tick(false));
        assert!(!k.tick(true), "a success resets");
        assert!(!k.tick(false));
        assert!(k.tick(false), "two misses in a row");
    }
    #[test] fn motion_is_summed_while_busy() {
        let mut p = Pending::default();
        let m = |dx, dy, buttons| IpadCommand::Mouse(MouseReport { dx, dy, buttons, ..Default::default() });
        p.push(m(3, 1, 0));
        p.push(m(4, -2, 0));
        p.push(m(0, 0, 1));
        p.push(IpadCommand::Keyboard(KeyboardReport { modifiers: 0, keys: [4, 0, 0, 0, 0, 0] }));
        p.push(m(5, 5, 1));
        p.push(m(1, 1, 1));
        p.push(m(0, 0, 0));
        assert_eq!(p.take(), vec![
            Out::Move { dx: 7, dy: -1, buttons: 0 },
            Out::Move { dx: 0, dy: 0, buttons: 1 },
            Out::Keys(KeyboardReport { modifiers: 0, keys: [4, 0, 0, 0, 0, 0] }),
            Out::Move { dx: 6, dy: 6, buttons: 1 },
            Out::Move { dx: 0, dy: 0, buttons: 0 },
        ]);
        assert!(p.is_empty());
    }
    #[test] fn press_and_release_are_never_merged_away() {
        let mut p = Pending::default();
        let m = |buttons| IpadCommand::Mouse(MouseReport { buttons, ..Default::default() });
        p.push(m(1));
        p.push(m(0));
        assert_eq!(p.take().len(), 2, "a quick click keeps its down and its up");
    }
    #[test] fn scroll_keeps_its_place() {
        let mut p = Pending::default();
        p.push(IpadCommand::Mouse(MouseReport { dx: 1, ..Default::default() }));
        p.push(IpadCommand::Mouse(MouseReport { wheel: -1, ..Default::default() }));
        p.push(IpadCommand::Mouse(MouseReport { dx: 1, ..Default::default() }));
        assert_eq!(p.take().len(), 3);
    }
    #[test] fn summed_motion_beyond_i16_still_sums_exactly() {
        let r = reports_for(&Out::Move { dx: 70_000, dy: -3, buttons: 1 });
        assert_eq!(r.iter().map(|b| b[1] as i8 as i32).sum::<i32>(), 70_000);
        assert_eq!(r.iter().map(|b| b[2] as i8 as i32).sum::<i32>(), -3);
        assert!(r.iter().all(|b| b[0] == 5 && b[11] == 1));
    }
    #[test] fn a_button_change_sends_one_report() {
        let r = reports_for(&Out::Move { dx: 0, dy: 0, buttons: 2 });
        assert_eq!(r.len(), 1);
        assert_eq!(r[0][11], 2);
    }
    #[test] fn wheel_becomes_report_7() {
        let r = reports_for(&Out::Mouse(MouseReport { wheel: 1, buttons: 1, ..Default::default() }));
        assert_eq!(r, vec![bridge_core::ipadhid::scroll_report(16, 0)]);
    }
    #[test] fn backoff_grows_to_thirty_seconds() {
        assert_eq!(next_backoff(BACKOFF_MIN), Duration::from_secs(4));
        assert_eq!(next_backoff(Duration::from_secs(20)), BACKOFF_MAX);
        assert_eq!(next_backoff(BACKOFF_MAX), BACKOFF_MAX);
    }
    #[test] fn mouse_descriptor_closes_its_collections() {
        assert_eq!(MOUSE_DESCRIPTOR.iter().filter(|b| **b == 0xA1).count(), 2);
        assert_eq!(&MOUSE_DESCRIPTOR[MOUSE_DESCRIPTOR.len() - 2..], &[0xC0, 0xC0]);
    }
}
