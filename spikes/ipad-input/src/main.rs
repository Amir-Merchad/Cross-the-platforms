//! Spike: can Windows send keyboard, touch and scroll to an iPad over Wi-Fi with no Bluetooth and no
//! iPad app? Throwaway. Tunnel code copied from `platform/windows/src/ipad.rs`; the stream start and
//! HID use mirror idevice's `tools/src/coredevice_stream.rs` and `tools/src/hid.rs`.
//!
//!   ipad-input <check|keys|touch|scroll|latency [uhid|indigo|touch]|pointer> [--usb]
//!
//! Wi-Fi unless `--usb`; no silent fallback, because Wi-Fi is the question. The pairing record path is
//! handed to the library only: never read or print it here.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use idevice::{
    IdeviceService as _, ReadWrite, RemoteXpcClient, RsdService,
    core_device::{
        CallInfoBlob, DisplayServiceClient, build_liveness, build_screen_audio_offer,
        build_screen_video_offer, build_start_audio_parameters, build_start_video_parameters,
        hid::{
            ButtonState, DIGITIZER_SURFACE_MAIN_TOUCHSCREEN, DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE,
            HidSurface, IndigoHidClient, KeyboardUsage, MainKeyboardService, ScrollTarget,
            TOUCHSCREEN_STATE_CONTACT, TOUCHSCREEN_STATE_RELEASE, UniversalHidServiceClient,
            scroll_momentum, scroll_phase,
        },
    },
    core_device_proxy::CoreDeviceProxy,
    mobile_image_mounter::ImageMounter,
    remote_pairing::{RemotePairingClient, RpPairingFile, RpPairingSocket, connect_tls_psk_tunnel_native},
    rsd::RsdHandshake,
    tcp::handle::{AdapterHandle, StreamHandle, UdpSocketHandle},
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
    xpc::XPCObject,
};
use tokio::{task::JoinHandle, time::{sleep, timeout}};

type Io = Box<dyn ReadWrite>;
type Res = Result<(), String>;

const HOST_NAME: &str = "cross-the-platforms";
const RECORD: &str = "rp-pairing.plist";
const KEEPALIVE_IDLE: Duration = Duration::from_secs(10);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);
/// What Device Hub sends for screen sharing. idevice notes the device's own, larger mask breaks it.
const CLIENT_SUPPORTED_FEATURES: u64 = 140;
const UHID_SERVICE: &str = "com.apple.coredevice.hid.universalhidservice";
const UHID_FEATURE: &str = "com.apple.coredevice.feature.remote.universalhidservice";
const STOPPED: &str = "stopped by Ctrl+C";
const CENTRE: u16 = 32768; // Touch space is 0..=65535 on both axes.
const GREETING: &str = "hello bridge";

/// Set by Ctrl+C and checked between gestures. Cancelling a send mid-write would corrupt the XPC
/// framing that the cleanup still needs, so commands stop themselves. A second Ctrl+C exits hard.
static STOP: AtomicBool = AtomicBool::new(false);

async fn pause(ms: u64) -> Res {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        if STOP.load(Relaxed) { return Err(STOPPED.into()) }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() { return Ok(()) }
        sleep(left.min(Duration::from_millis(50))).await;
    }
}

async fn countdown(what: &str) -> Res {
    println!("{what}");
    for n in (1..=3).rev() {
        println!("  {n}...");
        pause(1000).await?;
    }
    Ok(())
}

/// A failed stage must not stop the later ones; Ctrl+C must.
fn note(r: Res) -> Res {
    match r {
        Err(e) if e == STOPPED => Err(e),
        Err(e) => { println!("  failed: {e}"); Ok(()) }
        Ok(()) => Ok(()),
    }
}

fn short(v: impl std::fmt::Debug, n: usize) -> String { format!("{v:?}").chars().take(n).collect() }

/// HID Keyboard page usage. Lowercase and space only: the iPad's layout decides what the rest mean.
fn usage(c: char) -> Option<u16> {
    match c {
        'a'..='z' => Some(0x04 + (c as u16 - 'a' as u16)),
        ' ' => Some(0x2C),
        _ => None,
    }
}

fn percentile(sorted: &[Duration], pct: usize) -> Duration { sorted[(sorted.len() - 1) * pct / 100] }

fn print_times(label: &str, times: &mut [Duration]) {
    if times.is_empty() { return }
    let row: Vec<String> = times.iter().map(|t| format!("{:.2}", t.as_secs_f64() * 1000.0)).collect();
    println!("{label}, ms each: {}", row.join(" "));
    times.sort();
    println!("{label}: min {:?}, median {:?}, p95 {:?}, max {:?}", times[0], percentile(times, 50), percentile(times, 95), times[times.len() - 1]);
}

fn surface_kind(s: &HidSurface) -> &'static str {
    match (s.primary_usage_page, s.primary_usage) {
        (Some(1), Some(1)) => "POINTER",
        (Some(1), Some(2)) => "MOUSE",
        (Some(1), Some(6)) => "keyboard",
        (Some(0x0D), _) => "digitizer",
        _ => "other",
    }
}

fn is_pointer(s: &HidSurface) -> bool { matches!(surface_kind(s), "POINTER" | "MOUSE") }

// ---- Virtual mouse: idevice only builds a keyboard, but `createService` takes any descriptor. ----

/// Boot mouse with report ID 1: 3 buttons, then relative X, Y and wheel. Closing `C0 C0` left off.
const MOUSE_DESCRIPTOR: [u8; 52] = [
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01,
    0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x03, 0x81, 0x06,
];
/// The eight vendor bytes idevice's keyboard descriptor ends with (timestamp data).
const VENDOR_TAIL: [u8; 17] = [0x06, 0x00, 0xFF, 0x0A, 0x02, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x08, 0x81, 0x02];

fn mouse_descriptor(vendor_tail: bool) -> Vec<u8> {
    let mut d = MOUSE_DESCRIPTOR.to_vec();
    d.push(0xC0);
    if vendor_tail { d.extend_from_slice(&VENDOR_TAIL) }
    d.push(0xC0);
    d
}

fn mouse_report(dx: i8, dy: i8, vendor_tail: bool) -> Vec<u8> {
    let mut r = vec![0x01, 0, dx as u8, dy as u8, 0];
    if vendor_tail {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        r.extend_from_slice(&nanos.to_le_bytes()[..6]);
        r.extend_from_slice(&[0, 0]);
    }
    r
}

fn uhid_request(payload: XPCObject) -> XPCObject {
    idevice::xpc!({ "featureIdentifier": UHID_FEATURE, "messageType": "Request", "payload": payload })
}

/// Same shape as idevice's `build_main_keyboard_create_request`, as a mouse.
fn mouse_create_request(descriptor: Vec<u8>) -> XPCObject {
    const ID: u64 = 0x1_0000_2002;
    const VENDOR: i64 = 0x05AC;
    const PRODUCT: i64 = 0x0269;
    const NAME: &str = "ctp spike mouse";
    uhid_request(idevice::xpc!({
        "createService": {
            "_0": {
                "DeviceUsagePairs": [{ "DeviceUsage": 2_i64, "DeviceUsagePage": 1_i64 }],
                "PrimaryUsage": 2_u64,
                "PrimaryUsagePage": 1_u64,
                "Product": NAME,
                "ProductID": PRODUCT,
                "VendorID": VENDOR,
                "_CoreDevice_codablePropertyStorage": {
                    "Manufacturer": { "string": "idevice" },
                    "Product": { "string": NAME },
                    "ProductID": { "int": PRODUCT },
                    "VendorID": { "int": VENDOR },
                    "PrimaryUsage": { "int": 2_i64 },
                    "PrimaryUsagePage": { "int": 1_i64 },
                    "DeviceUsagePairs": {
                        "array": [{ "dictionary": { "DeviceUsage": { "int": 2_i64 }, "DeviceUsagePage": { "int": 1_i64 } } }],
                    },
                    "Transport": { "string": "USB" },
                    "ReportDescriptor": { "data": descriptor },
                    "UniversalControlVirtualService": { "bool": true },
                    "_ServiceID": { "uint": ID },
                },
                "_ServiceID": ID,
            },
        },
    }))
}

fn service_request(operation: &'static str, id: u64) -> XPCObject {
    uhid_request(idevice::xpc!({ operation: { "_0": id } }))
}

fn send_request(id: u64, report: Vec<u8>) -> XPCObject {
    uhid_request(idevice::xpc!({ "send": { "_0": report, "_1": id } }))
}

// ---- Session ----

struct Session {
    adapter: AdapterHandle,
    handshake: RsdHandshake,
    display: DisplayServiceClient<Io>,
    hid: UniversalHidServiceClient<Io>,
    indigo: IndigoHidClient<Io>,
    keyboard: Option<MainKeyboardService>,
    /// Indigo keys currently down, released on exit.
    held: Vec<u64>,
    raw: Option<RemoteXpcClient<StreamHandle>>,
    mouse: Option<u64>,
    rtp: Arc<AtomicU64>,
    pumps: Vec<JoinHandle<()>>,
}

/// Throws the media away. Left unread, the datagrams queue without limit for as long as a session
/// runs. Video also gets RTCP liveness, which idevice says tells the device the receiver is alive.
fn pump(udp: UdpSocketHandle, video_ssrc: Option<u32>, rtp: Arc<AtomicU64>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut last = Instant::now();
        while let Ok(dg) = udp.recv().await {
            rtp.fetch_add(1, Relaxed);
            if let Some(ssrc) = video_ssrc && last.elapsed() >= Duration::from_secs(1) {
                last = Instant::now();
                let _ = udp.send_to(dg.source_port, build_liveness(ssrc, "ctp", &[])).await;
            }
        }
    })
}

impl Session {
    async fn open(dir: &Path, usb: bool) -> Result<Session, String> {
        for attempt in 0..2 {
            let (mut adapter, mut handshake) = if usb { connect_usb().await? } else { connect_wifi(&dir.join(RECORD)).await? };
            println!("tunnel up over {}", if usb { "USB" } else { "Wi-Fi" });
            match DisplayServiceClient::<Io>::connect_rsd(&mut adapter, &mut handshake).await {
                Ok(display) => return Session::start(adapter, handshake, display).await,
                Err(e) if attempt == 0 && format!("{e:?}").contains("ServiceNotFound") => {
                    println!("display service not listed: the developer disk image is not mounted, mounting it");
                    if usb { mount_usb(dir).await.map_err(|e| format!("USB mount failed: {e}"))?; } else { mount_ddi(dir, &mut adapter, &mut handshake).await.map_err(|e| format!("DDI mount failed: {e}. Plug in USB and rerun with --usb"))?; }
                    println!("mounted; reconnecting because the service list is now stale");
                    let _ = adapter.close().await;
                    sleep(Duration::from_secs(2)).await;
                }
                Err(e) => return Err(format!("display service: {e:?}")),
            }
        }
        Err("display service still missing after mounting the disk image".into())
    }

    async fn start(mut adapter: AdapterHandle, mut handshake: RsdHandshake, mut display: DisplayServiceClient<Io>) -> Result<Session, String> {
        let audio_udp = adapter.bind_udp(0).await.map_err(|e| format!("bind audio: {e:?}"))?;
        let video_udp = adapter.bind_udp(0).await.map_err(|e| format!("bind video: {e:?}"))?;
        let (receiver, sender) = (adapter.host_ip().to_string(), adapter.peer_ip().to_string());
        // Strings mirror the Device Hub offer idevice captured, which the device accepted.
        let call_info = CallInfoBlob {
            call_id: 0, client_version: 1, device_type: "Mac17,7".into(), framework_version: "2205.3.1".into(),
            os_version: "25F71".into(), device_name: None, audio_device_uid: None,
        };
        let session_id = uuid::Uuid::new_v4();
        let call_id = || uuid::Uuid::new_v4().to_string().to_uppercase();

        // Audio first: it sets up the screen-sharing session the video joins.
        let offer = build_screen_audio_offer(&call_id(), &call_info).map_err(|e| format!("audio offer: {e:?}"))?;
        let params = build_start_audio_parameters(&receiver, audio_udp.local_port(), &sender, 50000, offer, CLIENT_SUPPORTED_FEATURES, session_id);
        if let Err(e) = display.start_media_stream(params).await { println!("audio stream refused, trying video alone: {}", format!("{e:?}").chars().take(120).collect::<String>()); }
        println!("audio stream started");

        let ssrc = uuid::Uuid::new_v4().as_u128() as u32;
        let offer = build_screen_video_offer(&call_id(), &call_info, ssrc).map_err(|e| format!("video offer: {e:?}"))?;
        let params = build_start_video_parameters(&receiver, video_udp.local_port(), &sender, 50001, offer, CLIENT_SUPPORTED_FEATURES, 1, session_id);
        if let Err(e) = display.start_media_stream(params).await {
            let _ = display.stop_media_stream().await;
            return Err(format!("video stream: {e:?}"));
        }
        println!("video stream started, input gate should be open");

        let rtp = Arc::new(AtomicU64::new(0));
        let pumps = vec![pump(audio_udp, None, rtp.clone()), pump(video_udp, Some(ssrc), rtp.clone())];
        sleep(Duration::from_millis(300)).await; // idevice: surfaces need a moment to re-match the stream.

        let clients = async {
            let hid = UniversalHidServiceClient::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("universal HID: {e:?}"))?;
            let indigo = IndigoHidClient::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("indigo HID: {e:?}"))?;
            Ok::<_, String>((hid, indigo))
        }.await;
        let (hid, indigo) = match clients {
            Ok(c) => c,
            Err(e) => { let _ = display.stop_media_stream().await; return Err(e) }
        };
        println!("HID services connected");
        Ok(Session { adapter, handshake, display, hid, indigo, keyboard: None, held: vec![], raw: None, mouse: None, rtp, pumps })
    }

    /// Always runs, Ctrl+C included. Each step is bounded so a dead tunnel cannot hang the exit.
    async fn finish(mut self) {
        const LIMIT: Duration = Duration::from_secs(3);
        println!("cleaning up");
        for key in std::mem::take(&mut self.held) {
            let _ = timeout(LIMIT, self.indigo.send_keyboard(key, ButtonState::Up)).await;
        }
        if let Some(mut kb) = self.keyboard.take() {
            match timeout(LIMIT, self.hid.remove_main_keyboard(&mut kb)).await {
                Ok(Ok(())) => println!("virtual keyboard removed"),
                Ok(Err(e)) => println!("virtual keyboard removal failed: {e}"),
                Err(_) => println!("virtual keyboard removal timed out"),
            }
        }
        let _ = timeout(LIMIT, self.remove_mouse()).await;
        match timeout(LIMIT, self.display.stop_media_stream()).await {
            Ok(Ok(_)) => println!("display stream stopped"),
            Ok(Err(e)) => println!("display stream stop failed: {e:?}"),
            Err(_) => println!("display stream stop timed out"),
        }
        self.pumps.iter().for_each(JoinHandle::abort);
        let _ = timeout(LIMIT, self.adapter.close()).await;
    }

    async fn surfaces(&mut self) -> Result<Vec<HidSurface>, String> {
        self.hid.list_connected_services().await.map_err(|e| format!("HID surface list: {e:?}"))
    }

    async fn check(&mut self) -> Res {
        let services: Vec<&String> = self.handshake.services.keys().filter(|k| ["hid", "display", "pointer", "mouse"].iter().any(|w| k.to_lowercase().contains(w))).collect();
        println!("RSD services of interest: {services:?}");
        match self.display.get_media_support_info().await {
            Ok(v) => println!("media support: {}", short(v, 600)),
            Err(e) => println!("media support query failed: {e:?}"),
        }
        let list = self.surfaces().await?;
        println!("HID surfaces ({}):", list.len());
        for s in &list {
            println!("  [{}] {} page 0x{:X} usage 0x{:X} ({})", s.service_id, s.product.as_deref().unwrap_or("(unnamed)"), s.primary_usage_page.unwrap_or(0), s.primary_usage.unwrap_or(0), surface_kind(s));
        }
        let has = |id| list.iter().any(|s| s.service_id == id);
        println!("main touchscreen ({DIGITIZER_SURFACE_MAIN_TOUCHSCREEN}): {}", has(DIGITIZER_SURFACE_MAIN_TOUCHSCREEN));
        println!("touchscreen gesture ({DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE}): {}", has(DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE));
        let pointers: Vec<u64> = list.iter().filter(|s| is_pointer(s)).map(|s| s.service_id).collect();
        println!("pointer or mouse surface: {}", if pointers.is_empty() { "NONE listed (the pointer command still tries a virtual mouse)".to_string() } else { format!("YES {pointers:?}") });
        println!("waiting 3 s to see media arrive");
        pause(3000).await?;
        let packets = self.rtp.load(Relaxed);
        println!("media packets received: {packets}{}", if packets == 0 { " (stream may not really be active: input would be dropped)" } else { "" });
        match self.display.get_media_stream_server_status().await {
            Ok(v) => println!("stream status: {}", short(v, 300)),
            Err(e) => println!("stream status failed: {e:?}"),
        }
        Ok(())
    }

    async fn ensure_keyboard(&mut self) -> Res {
        if self.keyboard.is_none() {
            self.keyboard = Some(self.hid.create_main_keyboard().await.map_err(|e| format!("create virtual keyboard: {e}"))?);
        }
        Ok(())
    }

    async fn uhid_event(&mut self, key: KeyboardUsage, down: bool) -> Res {
        let kb = self.keyboard.as_mut().ok_or("no virtual keyboard")?;
        let r = if down { self.hid.main_keyboard_key_down(kb, key).await } else { self.hid.main_keyboard_key_up(kb, key).await };
        r.map_err(|e| format!("virtual keyboard: {e}"))
    }

    async fn indigo_event(&mut self, key: u64, state: ButtonState) -> Res {
        self.indigo.send_keyboard(key, state).await.map_err(|e| format!("indigo key: {e:?}"))?;
        if state == ButtonState::Down { self.held.push(key) } else { self.held.retain(|k| *k != key) }
        Ok(())
    }

    async fn type_uhid(&mut self, text: &str) -> Res {
        self.ensure_keyboard().await?;
        for c in text.chars().filter_map(usage) {
            let key = KeyboardUsage::new(c).map_err(|e| e.to_string())?;
            self.uhid_event(key, true).await?;
            self.uhid_event(key, false).await?;
            pause(30).await?;
        }
        Ok(())
    }

    async fn type_indigo(&mut self, text: &str) -> Res {
        for c in text.chars().filter_map(usage) {
            self.indigo_event(c as u64, ButtonState::Down).await?;
            self.indigo_event(c as u64, ButtonState::Up).await?;
            pause(30).await?;
        }
        Ok(())
    }

    /// Two routes exist and the source doesn't say which reaches a real iPad, so the text says which landed.
    async fn keys(&mut self) -> Res {
        countdown("focus a text field on the iPad (Notes works)").await?;
        println!("route 1: virtual keyboard, typing \"{GREETING} uhid \"");
        note(self.type_uhid(&format!("{GREETING} uhid ")).await)?;
        pause(1000).await?;
        println!("route 2: indigo events, typing \"{GREETING} indigo \"");
        note(self.type_indigo(&format!("{GREETING} indigo ")).await)?;
        pause(500).await?;
        println!("look at the field: each phrase that appears is a route that works");
        Ok(())
    }

    async fn touch(&mut self) -> Res {
        countdown("about to tap the centre of the screen, then swipe right to left").await?;
        self.hid.tap(CENTRE, CENTRE).await.map_err(|e| format!("tap: {e:?}"))?;
        println!("tap sent");
        pause(1500).await?;
        self.hid.drag(49152, CENTRE, 16384, CENTRE, 30, 16).await.map_err(|e| format!("swipe: {e:?}"))?;
        println!("swipe sent");
        Ok(())
    }

    async fn crown(&mut self, target: ScrollTarget, delta: f64) -> Res {
        let none = scroll_momentum::UNDEFINED;
        let err = |e| format!("indigo scroll: {e:?}");
        self.indigo.send_scroll((0.0, 0.0, 0.0), scroll_phase::BEGAN, none, target).await.map_err(err)?;
        for _ in 0..20 {
            self.indigo.send_scroll((0.0, delta, 0.0), scroll_phase::CHANGED, none, target).await.map_err(err)?;
            sleep(Duration::from_millis(16)).await;
        }
        self.indigo.send_scroll((0.0, 0.0, 0.0), scroll_phase::ENDED, none, target).await.map_err(err)
    }

    async fn scroll(&mut self) -> Res {
        countdown("open a long page (Settings or Safari); three routes run in turn").await?;
        for (name, target) in [("A: indigo scroll, crown target", ScrollTarget::DigitalCrown), ("B: indigo scroll, dial target", ScrollTarget::Dial)] {
            println!("{name}: down, then back");
            note(self.crown(target, 10.0).await)?;
            pause(1500).await?;
            note(self.crown(target, -10.0).await)?;
            pause(2000).await?;
        }
        println!("C: touch drag: down, then back");
        self.hid.drag(CENTRE, 49152, CENTRE, 16384, 30, 16).await.map_err(|e| format!("drag: {e:?}"))?;
        pause(1500).await?;
        self.hid.drag(CENTRE, 16384, CENTRE, 49152, 30, 16).await.map_err(|e| format!("drag: {e:?}"))?;
        println!("say which of A, B, C moved the page");
        Ok(())
    }

    async fn latency(&mut self, kind: &str) -> Res {
        const EVENTS: u16 = 50;
        let mut times = Vec::with_capacity(EVENTS as usize);
        match kind {
            "uhid" | "indigo" => {
                if kind == "uhid" { self.ensure_keyboard().await? }
                countdown("focus a text field: expect exactly 25 letters x").await?;
                let code = usage('x').unwrap();
                let key = KeyboardUsage::new(code).map_err(|e| e.to_string())?;
                for i in 0..EVENTS {
                    let down = i % 2 == 0;
                    let t = Instant::now();
                    if kind == "uhid" {
                        self.uhid_event(key, down).await?;
                    } else {
                        self.indigo_event(code as u64, if down { ButtonState::Down } else { ButtonState::Up }).await?;
                    }
                    times.push(t.elapsed());
                    if !down { pause(30).await? }
                }
            }
            "touch" => {
                countdown("a slow drag across the middle of the screen").await?;
                let mut x = 16384;
                for _ in 0..EVENTS {
                    let t = Instant::now();
                    let r = self.hid.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x, CENTRE, None).await;
                    times.push(t.elapsed());
                    r.map_err(|e| format!("touch: {e:?}"))?;
                    if STOP.load(Relaxed) { break }
                    x += 640;
                    sleep(Duration::from_millis(16)).await;
                }
                self.hid.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x, CENTRE, None).await.map_err(|e| format!("touch release: {e:?}"))?;
            }
            other => return Err(format!("latency takes uhid, indigo or touch, not {other}")),
        }
        print_times("send (leaves this PC, not arrival on the iPad)", &mut times);
        let mut trips = Vec::with_capacity(20);
        for _ in 0..20 {
            pause(20).await?;
            let t = Instant::now();
            self.surfaces().await?;
            trips.push(t.elapsed());
        }
        print_times("round trip to the iPad HID daemon and back", &mut trips);
        println!("media packets flowing beside this: {}", self.rtp.load(Relaxed));
        Ok(())
    }

    async fn raw_client(&mut self) -> Result<&mut RemoteXpcClient<StreamHandle>, String> {
        if self.raw.is_none() {
            let port = self.handshake.services.get(UHID_SERVICE).ok_or("universal HID service not listed")?.port;
            let stream = self.adapter.connect(port).await.map_err(|e| format!("connect: {e}"))?;
            let mut raw = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
            raw.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
            self.raw = Some(raw);
        }
        Ok(self.raw.as_mut().unwrap())
    }

    async fn remove_mouse(&mut self) {
        let Some(id) = self.mouse.take() else { return };
        if let Some(raw) = self.raw.as_mut() {
            let _ = raw.send_object(service_request("resetGestureState", id), false).await;
            match raw.send_object(service_request("removeService", id), false).await {
                Ok(()) => println!("  virtual mouse removed"),
                Err(e) => println!("  virtual mouse removal failed: {e:?}"),
            }
        }
    }

    /// A small square: 80 reports over about 1.3 s.
    async fn square(&mut self, mut send: impl AsyncFnMut(&mut Self, i8, i8) -> Res) -> Res {
        for (dx, dy) in [(8, 0), (0, 8), (-8, 0), (0, -8)] {
            for _ in 0..20 {
                send(self, dx, dy).await?;
                pause(16).await?;
            }
        }
        Ok(())
    }

    async fn virtual_mouse(&mut self, vendor_tail: bool) -> Res {
        let raw = self.raw_client().await?;
        raw.send_object(mouse_create_request(mouse_descriptor(vendor_tail)), true).await.map_err(|e| format!("create: {e:?}"))?;
        let reply = raw.recv().await.map_err(|e| format!("create reply: {e:?}"))?;
        let Some(id) = reply.as_dictionary().and_then(|d| d.get("serviceID")).and_then(plist::Value::as_unsigned_integer).filter(|id| *id != 0) else {
            return Err(format!("iPad refused the mouse: {}", short(reply, 400)));
        };
        self.mouse = Some(id);
        println!("  iPad accepted the mouse, service {id}");
        let list = self.surfaces().await?;
        let visible = list.iter().find(|s| s.service_id == id);
        println!("  in the surface list: {}", visible.map_or("no".to_string(), |s| format!("yes ({})", surface_kind(s))));
        pause(1000).await?;
        println!("  moving: watch for a round pointer tracing a square");
        let moved = self.square(async |s, dx, dy| {
            let raw = s.raw.as_mut().ok_or("no client")?;
            raw.send_object(send_request(id, mouse_report(dx, dy, vendor_tail)), false).await.map_err(|e| format!("report: {e:?}"))
        }).await;
        pause(1000).await?;
        self.remove_mouse().await;
        moved
    }

    /// Nothing here can see the iPad's screen: the owner's eyes are the result, so stages are lettered.
    async fn pointer(&mut self) -> Res {
        countdown("watch the iPad for a round pointer; stages are 3 s apart").await?;
        let list = self.surfaces().await?;
        let pointers: Vec<u64> = list.iter().filter(|s| is_pointer(s)).map(|s| s.service_id).collect();

        println!("A: send_report with a boot mouse report to listed pointer surfaces: {pointers:?}");
        for id in pointers {
            note(self.square(async |s, dx, dy| s.hid.send_report(id, vec![0, dx as u8, dy as u8, 0]).await.map_err(|e| format!("report: {e:?}"))).await)?;
        }
        pause(3000).await?;

        println!("B: 19-byte gesture/pointer report on the gesture surface, small relative steps");
        note(self.square(async |s, dx, dy| s.hid.send_digitizer(dx as i32, dy as i32, DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE, None).await.map_err(|e| format!("report: {e:?}"))).await)?;
        pause(3000).await?;

        println!("C: same report, absolute 16-bit positions around the centre");
        let (mut x, mut y) = (CENTRE as i32, CENTRE as i32);
        note(self.square(async |s, dx, dy| {
            x += dx as i32 * 50;
            y += dy as i32 * 50;
            s.hid.send_digitizer(x, y, DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE, None).await.map_err(|e| format!("report: {e:?}"))
        }).await)?;
        pause(3000).await?;

        println!("D: send_vendor_defined with a boot mouse report, Generic Desktop mouse usage");
        note(self.square(async |s, dx, dy| s.indigo.send_vendor_defined(1, 2, 1, vec![0, dx as u8, dy as u8, 0]).await.map_err(|e| format!("vendor event: {e:?}"))).await)?;
        pause(3000).await?;

        println!("E: createService with a virtual USB mouse");
        note(self.virtual_mouse(false).await)?;
        pause(3000).await?;

        println!("F: virtual mouse with the keyboard's vendor timestamp bytes");
        note(self.virtual_mouse(true).await)?;

        println!("done. If no stage moved a pointer, nothing here works and iPad input over Wi-Fi is touch only");
        Ok(())
    }
}

// ---- Tunnel, from platform/windows/src/ipad.rs ----

async fn mount_ddi(dir: &Path, adapter: &mut AdapterHandle, handshake: &mut RsdHandshake) -> Res {
    let ddi = dir.join("ddi");
    let read = |name: &str| std::fs::read(ddi.join(name)).map_err(|e| format!("{name}: {e}"));
    let (image, trust_cache, manifest) = (read("Image.dmg")?, read("Image.dmg.trustcache")?, read("BuildManifest.plist")?);
    let chip_id = handshake.properties.get("UniqueChipID").and_then(|v| v.as_unsigned_integer()).ok_or("iPad didn't report its chip id")?;
    let mut mounter = ImageMounter::connect_rsd(adapter, handshake).await.map_err(|e| format!("image mounter: {e:?}"))?;
    mounter.mount_personalized_rsd(adapter, handshake, image, trust_cache, &manifest, None, chip_id).await.map_err(|e| format!("{e:?}"))
}

/// Mounts over lockdown, as the working mounter tool does: the RSD route timed out on this iPad.
async fn mount_usb(dir: &Path) -> Res {
    use idevice::{IdeviceService, lockdown::LockdownClient, provider::IdeviceProvider};
    let ddi = dir.join("ddi");
    let read = |name: &str| std::fs::read(ddi.join(name)).map_err(|e| format!("{name}: {e}"));
    let (image, trust_cache, manifest) = (read("Image.dmg")?, read("Image.dmg.trustcache")?, read("BuildManifest.plist")?);
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd: {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad on USB")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().map_err(|e| format!("usbmuxd address: {e:?}"))?, HOST_NAME);
    let mut lockdown = LockdownClient::connect(&provider).await.map_err(|e| format!("lockdown: {e:?}"))?;
    let pairing = provider.get_pairing_file().await.map_err(|e| format!("pairing record: {e:?}"))?;
    lockdown.start_session(&pairing).await.map_err(|e| format!("lockdown session: {e:?}"))?;
    let chip_id = lockdown.get_value(Some("UniqueChipID"), None).await.map_err(|e| format!("chip id: {e:?}"))?.as_unsigned_integer().ok_or("no chip id")?;
    let mut mounter = ImageMounter::connect(&provider).await.map_err(|e| format!("image mounter: {e:?}"))?;
    mounter.mount_personalized(&provider, image, trust_cache, &manifest, None, chip_id).await.map_err(|e| format!("{e:?}"))
}

async fn connect_usb() -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd not reachable (Apple Devices installed?): {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad on USB")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().map_err(|e| format!("usbmuxd address: {e:?}"))?, HOST_NAME);
    let proxy = CoreDeviceProxy::connect(&provider).await.map_err(|e| format!("core device proxy (Developer Mode on?): {e:?}"))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().map_err(|e| format!("software tunnel: {e:?}"))?.to_async_handle();
    let stream = adapter.connect(rsd_port).await.map_err(|e| format!("RSD connect: {e:?}"))?;
    let handshake = RsdHandshake::new(stream).await.map_err(|e| format!("RSD handshake: {e:?}"))?;
    Ok((adapter, handshake))
}

async fn discover() -> Result<SocketAddr, String> {
    tokio::task::spawn_blocking(|| {
        let mdns = mdns_sd::ServiceDaemon::new().map_err(|e| format!("mDNS: {e}"))?;
        let rx = mdns.browse("_remotepairing._tcp.local.").map_err(|e| format!("mDNS browse: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut found = None;
        while let Ok(ev) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
                let addrs = info.get_addresses();
                if let Some(ip) = addrs.iter().find(|a| a.is_ipv4()).or(addrs.iter().next()) {
                    found = Some(SocketAddr::new(*ip, info.get_port()));
                    break;
                }
            }
        }
        let _ = mdns.shutdown();
        found.ok_or_else(|| "iPad not found on Wi-Fi: unlock it, keep it awake, same network".to_string())
    }).await.map_err(|e| e.to_string())?
}

async fn connect_wifi(record: &Path) -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut rpf = RpPairingFile::read_from_file(record).await.map_err(|e| format!("pairing record: {e:?}"))?;
    println!("looking for the iPad on Wi-Fi");
    let addr = discover().await?;
    println!("found it at {addr}");
    let stream = tokio::net::TcpStream::connect(addr).await.map_err(|e| format!("connect: {e}"))?;
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_NAME);
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pair verify: {e:?}"))?;
    let tunnel_port = rpc.create_tcp_listener().await.map_err(|e| format!("tunnel listener: {e:?}"))?;
    let tunnel_stream = tokio::net::TcpStream::connect(SocketAddr::new(addr.ip(), tunnel_port)).await.map_err(|e| format!("tunnel connect: {e}"))?;
    let keepalive = socket2::TcpKeepalive::new().with_time(KEEPALIVE_IDLE).with_interval(KEEPALIVE_INTERVAL);
    socket2::SockRef::from(&tunnel_stream).set_tcp_keepalive(&keepalive).map_err(|e| format!("keepalive: {e}"))?;
    let tunnel = connect_tls_psk_tunnel_native(tunnel_stream, rpc.encryption_key()).await.map_err(|e| format!("TLS-PSK tunnel: {e:?}"))?;
    let client_ip: std::net::IpAddr = tunnel.info.client_address.parse().map_err(|e| format!("{e}"))?;
    let server_ip: std::net::IpAddr = tunnel.info.server_address.parse().map_err(|e| format!("{e}"))?;
    let (mtu, rsd_port) = (tunnel.info.mtu as usize, tunnel.info.server_rsd_port);
    let mut adapter = idevice::tcp::adapter::Adapter::new(Box::new(tunnel.into_inner()), client_ip, server_ip);
    adapter.set_mss(mtu.min(8192) - 60);
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.map_err(|e| format!("RSD connect: {e}"))?;
    let handshake = RsdHandshake::new(stream).await.map_err(|e| format!("RSD handshake: {e:?}"))?;
    Ok((adapter, handshake))
}

/// The debug-build futures here overflow Windows' 1 MB main-thread stack.
fn main() -> ExitCode {
    let thread = std::thread::Builder::new().stack_size(64 << 20).spawn(|| {
        tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(16 << 20).build().expect("tokio runtime").block_on(run())
    });
    thread.expect("main thread").join().unwrap_or(ExitCode::FAILURE)
}

async fn run() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usb = args.iter().any(|a| a == "--usb");
    args.retain(|a| a != "--usb");
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if !["check", "keys", "touch", "scroll", "latency", "pointer"].contains(&cmd) {
        eprintln!("usage: ipad-input <check|keys|touch|scroll|latency [uhid|indigo|touch]|pointer> [--usb]");
        return ExitCode::from(2);
    }
    let Some(appdata) = std::env::var_os("APPDATA") else {
        eprintln!("APPDATA is not set");
        return ExitCode::FAILURE;
    };
    let dir = PathBuf::from(appdata).join("CrossThePlatforms").join("ipad");

    tokio::spawn(async {
        while tokio::signal::ctrl_c().await.is_ok() {
            if STOP.swap(true, Relaxed) { std::process::exit(130) }
            println!("Ctrl+C: finishing this gesture, then cleaning up (again to quit at once)");
        }
    });

    let mut session = match timeout(Duration::from_secs(90), Session::open(&dir, usb)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => { eprintln!("FAILED: {e}"); return ExitCode::FAILURE }
        Err(_) => { eprintln!("FAILED: no connection within 90 s"); return ExitCode::FAILURE }
    };
    let result = match cmd {
        "check" => session.check().await,
        "keys" => session.keys().await,
        "touch" => session.touch().await,
        "scroll" => session.scroll().await,
        "latency" => session.latency(args.get(1).map(String::as_str).unwrap_or("uhid")).await,
        _ => session.pointer().await,
    };
    session.finish().await;
    match result {
        Ok(()) => { println!("finished"); ExitCode::SUCCESS }
        Err(e) if e == STOPPED => { println!("{e}"); ExitCode::SUCCESS }
        Err(e) => { eprintln!("FAILED: {e}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_usages_and_mouse_reports() {
        assert_eq!(usage('a'), Some(0x04));
        assert_eq!(usage('z'), Some(0x1D));
        assert_eq!(usage(' '), Some(0x2C));
        assert_eq!(usage('A'), None);
        assert_eq!(mouse_report(-1, 2, false), [1, 0, 0xFF, 2, 0]);
        assert_eq!(mouse_report(0, 0, true).len(), 13);
        assert_eq!(mouse_descriptor(true).len(), mouse_descriptor(false).len() + VENDOR_TAIL.len());
    }
}
