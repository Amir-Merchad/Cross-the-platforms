//! Objective pointer tests: the iPad's own screenshots replace a human watching.
//! Findings live in docs/spikes/ipad-pointer.md.

use super::*;
use idevice::core_device::{DataInclusionPolicy, GENERAL_PASTEBOARD, ImageFormat, PasteboardServiceClient, ScreenCaptureServiceClient};

const SHOTS: &str = r"C:\dev\_ref\tmp\pointer\shots";

/// RGB8 pixels from a PNG.
pub fn decode(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| format!("png: {e}"))?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("png too big")?];
    let info = r.next_frame(&mut buf).map_err(|e| format!("png: {e}"))?;
    let ch = info.color_type.samples();
    buf.truncate(info.buffer_size());
    let rgb = if ch == 3 { buf } else { buf.chunks(ch).flat_map(|p| if ch >= 3 { [p[0], p[1], p[2]] } else { [p[0]; 3] }).collect() };
    Ok((info.width as usize, info.height as usize, rgb))
}

/// Changed pixels and their bounding box. `threshold` is per channel.
#[derive(Debug, PartialEq)]
pub struct Change { pub pixels: usize, pub bbox: Option<(usize, usize, usize, usize)> }

pub fn changed(w: usize, a: &[u8], b: &[u8], threshold: u8) -> Change {
    let (mut pixels, mut bbox) = (0, None::<(usize, usize, usize, usize)>);
    for (i, (p, q)) in a.chunks(3).zip(b.chunks(3)).enumerate() {
        if p.iter().zip(q).any(|(x, y)| x.abs_diff(*y) > threshold) {
            let (x, y) = (i % w, i / w);
            pixels += 1;
            bbox = Some(bbox.map_or((x, y, x, y), |(l, t, r, bt)| (l.min(x), t.min(y), r.max(x), bt.max(y))));
        }
    }
    Change { pixels, bbox }
}

pub fn diff_files(a: &str, b: &str) -> Result<Change, String> {
    let read = |p: &str| std::fs::read(p).map_err(|e| format!("{p}: {e}")).and_then(|d| decode(&d));
    let ((w, h, pa), (w2, h2, pb)) = (read(a)?, read(b)?);
    if (w, h) != (w2, h2) { return Err(format!("sizes differ: {w}x{h} and {w2}x{h2}")) }
    Ok(changed(w, &pa, &pb, 24))
}

pub fn diff_cmd(args: &[String]) -> ExitCode {
    let [a, b] = args else { eprintln!("diff <a.png> <b.png>"); return ExitCode::from(2) };
    match diff_files(a, b) {
        Ok(c) => { println!("{c:?}"); ExitCode::SUCCESS }
        Err(e) => { eprintln!("{e}"); ExitCode::FAILURE }
    }
}

/// Saves a PNG under SHOTS and returns its path. Takes the parts of Session it needs so reports can
/// flow on the raw client at the same time.
async fn capture(adapter: &mut AdapterHandle, handshake: &mut RsdHandshake, name: &str) -> Result<String, String> {
    let t = Instant::now();
    let mut c = ScreenCaptureServiceClient::<Io>::connect_rsd(adapter, handshake).await.map_err(|e| format!("screen capture service: {e:?}"))?;
    let png = c.take_screenshot(None, ImageFormat::Png).await.map_err(|e| format!("screenshot: {e:?}"))?;
    std::fs::create_dir_all(SHOTS).map_err(|e| e.to_string())?;
    let path = format!(r"{SHOTS}\{name}.png");
    std::fs::write(&path, &png).map_err(|e| e.to_string())?;
    println!("  shot {name}: {} KB in {:?}", png.len() / 1024, t.elapsed());
    Ok(path)
}

fn say_diff(label: &str, a: &str, b: &str) {
    match diff_files(a, b) {
        Ok(Change { pixels, bbox }) => println!("  DIFF {label}: {pixels} px, box {bbox:?}"),
        Err(e) => println!("  DIFF {label}: {e}"),
    }
}

/// iPad system log lines from the HID daemon, the one oracle that says what became of a report.
pub static LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn take_log() -> Vec<String> { std::mem::take(&mut *LOG.lock().unwrap()) }

pub async fn logtap() -> Res {
    use idevice::{os_trace_relay::OsTraceRelayClient, IdeviceService};
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd: {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("devices: {e:?}"))?.into_iter().next().ok_or("no iPad")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().map_err(|e| format!("addr: {e:?}"))?, HOST_NAME);
    let client = OsTraceRelayClient::connect(&provider).await.map_err(|e| format!("os trace: {e:?}"))?;
    let mut relay = client.start_trace(None).await.map_err(|e| format!("start trace: {e:?}"))?;
    println!("log tap running");
    let all = std::env::var_os("LOGALL").is_some();
    loop {
        let Ok(l) = relay.next().await else { println!("log tap ended"); return Ok(()) };
        let pointer = l.filename.contains("backboardd") && (l.message.contains("Pointer ") || l.message.contains("button number"));
        if all || pointer || l.filename.contains("dtuhidd") || l.image_name.contains("dtuhidd") {
            LOG.lock().unwrap().push(format!("{} [{}] {}", l.timestamp.format("%H:%M:%S%.3f"), l.filename.rsplit('/').next().unwrap_or(""), l.message));
        }
    }
}

/// One Wi-Fi pair verify, step by step, beside the iPad's own log (needs USB for the log). The record
/// path goes to the library only. Full log to wifi-<tag>.log, pairing lines printed.
pub async fn wifidiag(dir: &Path, record: &str, tag: &str) -> Res {
    tokio::spawn(async { if let Err(e) = logtap().await { println!("log tap failed: {e}") } });
    pause(2500).await?;
    let addr: SocketAddr = std::env::var("IPAD_ADDR").map_err(|_| "set IPAD_ADDR")?.parse().map_err(|e| format!("IPAD_ADDR: {e}"))?;
    let mut rpf = RpPairingFile::read_from_file(dir.join(record)).await.map_err(|e| format!("pairing record: {e:?}"))?;
    take_log();
    let t = Instant::now();
    let stream = tokio::net::TcpStream::connect(addr).await.map_err(|e| format!("connect: {e}"))?;
    println!("{:?} tcp connected from {:?}", t.elapsed(), stream.local_addr());
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_NAME);
    match timeout(Duration::from_secs(15), rpc.attempt_pair_verify()).await {
        Ok(Ok(v)) => {
            println!("{:?} handshake ok: {}", t.elapsed(), short(v, 1200));
            match timeout(Duration::from_secs(15), rpc.validate_pairing(&mut rpf)).await {
                Ok(r) => println!("{:?} validate_pairing: {r:?}", t.elapsed()),
                Err(_) => println!("{:?} validate_pairing: timed out", t.elapsed()),
            }
        }
        Ok(Err(e)) => println!("{:?} handshake failed: {e:?}", t.elapsed()),
        Err(_) => println!("{:?} handshake timed out", t.elapsed()),
    }
    pause(5000).await?;
    let log = take_log();
    let path = format!("C:/dev/_ref/tmp/pointer/wifi-{tag}.log");
    let _ = std::fs::write(&path, log.join("
"));
    println!("{} log lines to {path}", log.len());
    let hit = |l: &&String| { let l = l.to_lowercase(); ["remotepairing", "remoted", "coredevice", "rppairing", "pair verify", "pairverify"].iter().any(|w| l.contains(w)) };
    for l in log.iter().filter(hit).take(150) { println!("{}", l.chars().take(260).collect::<String>()); }
    Ok(())
}

/// Over USB: pairs under a new identifier into `file`, never touching rp-pairing.plist. The identifier
/// is a hash of the host name, and pairing again under a known one leaves the iPad holding the old key.
pub async fn rppair(dir: &Path, file: &str, host: &str) -> Res {
    let path = dir.join(file);
    if path.exists() || file == RECORD { return Err(format!("refusing to overwrite {file}")) }
    tokio::spawn(async { if let Err(e) = logtap().await { println!("log tap failed: {e}") } });
    pause(2500).await?;
    take_log();
    let (mut adapter, handshake) = connect_usb().await?;
    let port = handshake.services.get("com.apple.internal.dt.coredevice.untrusted.tunnelservice").ok_or("untrusted tunnel service not found")?.port;
    let stream = adapter.connect(port).await.map_err(|e| format!("tunnel service connect: {e}"))?;
    let mut conn = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
    conn.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
    let _ = conn.recv_root().await;
    let mut rpf = RpPairingFile::generate(host);
    let mut rpc = RemotePairingClient::new(conn, host);
    println!("pairing as {host}, confirm on the iPad if asked");
    let r = timeout(Duration::from_secs(60), rpc.connect(&mut rpf, async || "000000".to_string())).await;
    match &r {
        Ok(Ok(())) => { rpf.write_to_file(&path).await.map_err(|e| format!("write: {e:?}"))?; println!("paired, wrote {}", path.display()) }
        Ok(Err(e)) => println!("pairing failed: {e:?}"),
        Err(_) => println!("pairing timed out"),
    }
    pause(3000).await?;
    let log = take_log();
    let _ = std::fs::write("C:/dev/_ref/tmp/pointer/wifi-rppair.log", log.join("
"));
    for l in log.iter().filter(|l| l.contains("[remotepairingd") && !l.contains("nw_") && !l.contains("SecItem") && !l.contains("agent")).take(70) { println!("{}", l.chars().take(220).collect::<String>()); }
    Ok(())
}

/// (report id, ok|fail|none) per report the daemon logged, in order.
fn verdicts_of(log: &[String]) -> Vec<(u8, &'static str)> {
    let mut out: Vec<(u8, &'static str)> = vec![];
    for l in log {
        if let Some((_, n)) = l.rsplit_once("report type identifier ") {
            if let Ok(n) = n.trim().parse::<u8>() { out.push((n, "ok")) }
        } else if l.contains("Failed to decode") {
            if let Some(last) = out.last_mut() { last.1 = "fail" }
        } else if l.contains("No gesture for report") {
            if let Some(last) = out.last_mut() { last.1 = "none" }
        }
    }
    out
}

fn verdicts(log: &[String]) -> Vec<(u8, &'static str)> {
    let _ = std::fs::write("C:/dev/_ref/tmp/pointer/ids.log", log.join("\n"));
    verdicts_of(log)
}

/// Pointer positions in pixels from backboardd's log, oldest first, with the button mask. Each
/// position is logged twice, in pixels then rotated into points: `(x, y)` then `((2048-y)/2, x/2)`.
fn positions(log: &[String]) -> Vec<(f64, f64, String)> {
    let all: Vec<(f64, f64, String)> = log.iter().filter_map(|l| {
        let (_, rest) = l.split_once("Pointer ")?;
        let (xy, rest) = rest.split_once(" (global)")?;
        let (x, y) = xy.split_once(',')?;
        let mask = rest.split_once("buttonMask:").map_or("", |m| m.1.split_whitespace().next().unwrap_or("")).to_string();
        Some((x.parse().ok()?, y.parse().ok()?, mask))
    }).collect();
    all.windows(2).filter(|w| (w[1].1 - w[0].0 / 2.0).abs() < 1.0 && (w[1].0 - (2048.0 - w[0].1) / 2.0).abs() < 1.0).map(|w| w[0].clone()).collect()
}

fn track(log: &[String]) -> Option<((f64, f64), (f64, f64), usize, String)> {
    let pts = positions(log);
    let (a, b) = (pts.first()?, pts.last()?);
    Some(((a.0, a.1), (b.0, b.1), pts.len(), b.2.clone()))
}

/// The UniversalHID pointer report: `05 dx dy`, eight zeros, buttons, seven zeros. Found by sweeping
/// IDs, lengths and bytes against dtuhidd and backboardd logs. Bit 0 of buttons is the primary click.
pub fn r05(dx: i8, dy: i8, buttons: u8) -> Vec<u8> {
    let mut r = vec![0u8; 19];
    (r[0], r[1], r[2], r[11]) = (5, dx as u8, dy as u8, buttons);
    r
}

/// What a virtual service claims to be. `extra` goes into the codable storage as `{tag: value}`.
#[derive(Clone)]
pub struct Spec {
    pub usages: Vec<(i64, i64)>, // (page, usage), the first is primary
    pub vendor: i64,
    pub product: i64,
    pub transport: &'static str,
    pub hint: Option<&'static str>,
    pub descriptor: Option<Vec<u8>>,
    pub extra: Vec<(&'static str, &'static str, XPCObject)>,
}

impl Default for Spec {
    fn default() -> Self {
        Spec { usages: vec![(1, 2)], vendor: 0x05AC, product: 0x0269, transport: "USB", hint: None, descriptor: Some(mouse_descriptor(false)), extra: vec![] }
    }
}

fn tagged(tag: &str, v: XPCObject) -> XPCObject {
    let mut d = idevice::xpc::Dictionary::new();
    d.insert(tag.into(), v);
    XPCObject::Dictionary(d)
}

fn create_request(s: &Spec) -> XPCObject {
    const ID: u64 = 0x1_0000_2003;
    const NAME: &str = "ctp spike mouse";
    let (page, usage) = s.usages[0];
    let pairs: Vec<XPCObject> = s.usages.iter().map(|(p, u)| idevice::xpc!({ "DeviceUsage": *u, "DeviceUsagePage": *p })).collect();
    let boxed: Vec<XPCObject> = s.usages.iter().map(|(p, u)| idevice::xpc!({ "dictionary": { "DeviceUsage": { "int": *u }, "DeviceUsagePage": { "int": *p } } })).collect();
    let mut storage = idevice::xpc!({
        "Manufacturer": { "string": "idevice" },
        "Product": { "string": NAME },
        "ProductID": { "int": s.product },
        "VendorID": { "int": s.vendor },
        "PrimaryUsage": { "int": usage },
        "PrimaryUsagePage": { "int": page },
        "Transport": { "string": s.transport },
        "UniversalControlVirtualService": { "bool": true },
        "_ServiceID": { "uint": ID },
    });
    let mut service = idevice::xpc!({
        "PrimaryUsage": usage as u64,
        "PrimaryUsagePage": page as u64,
        "Product": NAME,
        "ProductID": s.product,
        "VendorID": s.vendor,
        "_ServiceID": ID,
    });
    let (XPCObject::Dictionary(st), XPCObject::Dictionary(sv)) = (&mut storage, &mut service) else { unreachable!() };
    st.insert("DeviceUsagePairs".into(), tagged("array", XPCObject::Array(boxed)));
    sv.insert("DeviceUsagePairs".into(), XPCObject::Array(pairs));
    if let Some(d) = &s.descriptor { st.insert("ReportDescriptor".into(), tagged("data", XPCObject::Data(d.clone()))); }
    if let Some(h) = s.hint {
        st.insert("DeviceTypeHint".into(), tagged("string", XPCObject::String(h.into())));
        sv.insert("DeviceTypeHint".into(), XPCObject::String(h.into()));
    }
    for (k, tag, v) in &s.extra { st.insert((*k).into(), tagged(tag, v.clone())); }
    sv.insert("_CoreDevice_codablePropertyStorage".into(), storage);
    let mut create = idevice::xpc::Dictionary::new();
    create.insert("_0".into(), service);
    let mut payload = idevice::xpc::Dictionary::new();
    payload.insert("createService".into(), XPCObject::Dictionary(create));
    uhid_request(XPCObject::Dictionary(payload))
}

/// The report the crate calls gesture/pointer, with buttons where it writes zeros.
pub fn r13(dx: i32, dy: i32, buttons: u16) -> Vec<u8> {
    let mut r = idevice::core_device::hid::build_digitizer_report(dx, dy, None);
    r[9..11].copy_from_slice(&buttons.to_le_bytes());
    r
}

pub type Fmt = fn(i32, i32, u16) -> Vec<u8>;

/// Boot-style collection descriptor around report 0x13's layout.
pub fn desc13() -> Vec<u8> {
    let mut d = vec![
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x13, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31,
        0x17, 0x00, 0x00, 0x00, 0x80, 0x27, 0xFF, 0xFF, 0xFF, 0x7F, 0x75, 0x20, 0x95, 0x02, 0x81, 0x06,
        0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x00, 0x25, 0x01, 0x95, 0x10, 0x75, 0x01, 0x81, 0x02, 0xC0,
    ];
    d.extend_from_slice(&VENDOR_TAIL);
    d.push(0xC0);
    d
}

async fn motion(raw: &mut RemoteXpcClient<StreamHandle>, id: u64, fmt: Fmt, dx: i32, dy: i32, ms: u64) -> Res {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if STOP.load(Relaxed) { return Err(STOPPED.into()) }
        raw.send_object(send_request(id, fmt(dx, dy, 0)), false).await.map_err(|e| format!("report: {e:?}"))?;
        sleep(Duration::from_millis(8)).await;
    }
    Ok(())
}

impl Session {
    pub async fn shot(&mut self, name: &str) -> Result<String, String> { capture(&mut self.adapter, &mut self.handshake, name).await }

    pub async fn create(&mut self, s: &Spec) -> Result<u64, String> {
        let raw = self.raw_client().await?;
        raw.send_object(create_request(s), true).await.map_err(|e| format!("create: {e:?}"))?;
        let reply = timeout(Duration::from_secs(3), raw.recv()).await.map_err(|_| "create: no reply".to_string())?.map_err(|e| format!("create reply: {e:?}"))?;
        let Some(id) = reply.as_dictionary().and_then(|d| d.get("serviceID")).and_then(plist::Value::as_unsigned_integer).filter(|id| *id != 0) else {
            return Err(format!("refused: {}", short(reply, 400)));
        };
        self.mouse = Some(id);
        println!("  created service {id}");
        Ok(id)
    }

    /// One report that asks for a reply, then the service as the iPad now describes it.
    async fn readback(&mut self, id: u64, target: u64, fmt: Fmt) -> Res {
        let raw = self.raw.as_mut().ok_or("no client")?;
        raw.send_object(send_request(target, fmt(0, 0, 0)), true).await.map_err(|e| format!("report: {e:?}"))?;
        match timeout(Duration::from_millis(500), raw.recv()).await {
            Ok(r) => println!("  reply to report: {}", short(r, 200)),
            Err(_) => println!("  reply to report: none in 500 ms"),
        }
        raw.send_object(uhid_request(idevice::xpc!({ "connectedServices": XPCObject::Dictionary(Default::default()) })), true).await.map_err(|e| format!("send: {e:?}"))?;
        let v = timeout(Duration::from_secs(2), raw.recv()).await.map_err(|_| "no list".to_string())?.map_err(|e| format!("recv: {e:?}"))?;
        let list = v.as_dictionary().and_then(|d| d.get("connectedServices")).and_then(plist::Value::as_array).cloned().unwrap_or_default();
        let mine = list.iter().find(|s| s.as_dictionary().and_then(|d| d.get("_ServiceID")).and_then(plist::Value::as_unsigned_integer) == Some(id));
        println!("  {} services; mine: {}", list.len(), mine.map_or("MISSING".to_string(), |m| short(m.as_dictionary().unwrap().keys().collect::<Vec<_>>(), 200)));
        Ok(())
    }

    /// Before, with the service up, during 2 s one way, during 2 s back, after.
    /// `target` overrides where reports go (a built-in surface while this service keeps the pointer up).
    pub async fn stage(&mut self, name: &str, spec: &Spec, fmt: Fmt, step: i32, target: Option<u64>) -> Res {
        println!("== {name}");
        let before = self.shot(&format!("{name}-0before")).await?;
        let id = self.create(spec).await?;
        let dest = target.unwrap_or(id);
        pause(1500).await?;
        let up = self.shot(&format!("{name}-1up")).await?;
        say_diff("service up vs before", &before, &up);
        let mut last = up.clone();
        for (tag, dx) in [("2right", step), ("3left", -step)] {
            let raw = self.raw.as_mut().ok_or("no client")?;
            let shot_name = format!("{name}-{tag}");
            let (adapter, handshake) = (&mut self.adapter, &mut self.handshake);
            let (m, during) = tokio::join!(motion(raw, dest, fmt, dx, dx / 2, 2000), async {
                sleep(Duration::from_millis(300)).await;
                capture(adapter, handshake, &shot_name).await
            });
            m?;
            let during = during?;
            say_diff(&format!("{tag} vs previous"), &last, &during);
            last = during;
        }
        self.readback(id, dest, fmt).await?;
        pause(300).await?;
        let after = self.shot(&format!("{name}-4after")).await?;
        say_diff("after vs up", &up, &after);
        self.remove_mouse().await;
        pause(1500).await
    }

    /// Every field the iPad reports for its HID surfaces, descriptors included.
    async fn dump(&mut self) -> Res {
        let raw = self.raw_client().await?;
        raw.send_object(uhid_request(idevice::xpc!({ "connectedServices": XPCObject::Dictionary(Default::default()) })), true).await.map_err(|e| format!("send: {e:?}"))?;
        let v = raw.recv().await.map_err(|e| format!("recv: {e:?}"))?;
        let path = r"C:\dev\_ref\tmp\pointer\surfaces.xml";
        plist::to_file_xml(path, &v).map_err(|e| e.to_string())?;
        println!("wrote {path}");
        Ok(())
    }

    async fn stages(&mut self, which: &str) -> Res {
        let plain = Spec::default();
        let d13 = Spec { descriptor: Some(desc13()), ..Spec::default() };
        match which {
            "base" => note(self.stage("e-id1", &plain, |x, y, _| mouse_report(x as i8, y as i8, false), 20, None).await),
            "r13" => {
                note(self.stage("r13-own", &d13, r13, 20, None).await)?;
                note(self.stage("r13-plaindesc", &plain, r13, 20, None).await)?;
                note(self.stage("r13-to1281", &plain, r13, 20, Some(DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE)).await)
            }
            other => Err(format!("unknown stage set {other}")),
        }
    }

    /// Home button, as idevice's hid tool presses it.
    pub async fn home(&mut self) -> Res {
        self.indigo.send_button(0x0C, 0x40, ButtonState::Down).await.map_err(|e| format!("home: {e:?}"))?;
        sleep(Duration::from_millis(80)).await;
        self.indigo.send_button(0x0C, 0x40, ButtonState::Up).await.map_err(|e| format!("home: {e:?}"))?;
        pause(1200).await
    }

    /// For each report ID and length, zero-filled so nothing acts: does the daemon decode it?
    async fn ids(&mut self) -> Res {
        let id = self.create(&Spec::default()).await?;
        pause(1000).await?;
        let mut sent = vec![];
        let raw = self.raw.as_mut().ok_or("no client")?;
        for rid in 1..=255u16 { sent.push((rid as u8, 24usize)); let mut r = vec![0u8; 24]; r[0] = rid as u8; raw.send_object(send_request(id, r), false).await.map_err(|e| format!("report: {e:?}"))?; pause(15).await?; }
        pause(2500).await?;
        let verdicts = verdicts(&take_log());
        let known: Vec<u8> = verdicts.iter().filter(|(_, v)| *v != "none").map(|(i, _)| *i).collect();
        println!("  ids with a gesture ({}): {:02X?}", known.len(), known);
        let mut ok: std::collections::BTreeMap<u8, Vec<usize>> = Default::default();
        for rid in known {
            for len in 1..=72usize {
                let raw = self.raw.as_mut().ok_or("no client")?;
                let mut r = vec![0u8; len];
                r[0] = rid;
                raw.send_object(send_request(id, r), false).await.map_err(|e| format!("report: {e:?}"))?;
                pause(25).await?;
                // A verdict for this exact send, in order: logs arrive late, so wait for the line.
                let mut got = None;
                for _ in 0..40 {
                    let v = verdicts_of(&take_log());
                    if let Some(x) = v.into_iter().next_back() { got = Some(x); break }
                    pause(25).await?;
                }
                if got.is_some_and(|(_, v)| v == "ok") { ok.entry(rid).or_default().push(len) }
            }
            println!("  id {rid:02X}: decodes at lengths {:?}", ok.get(&rid));
        }
        Ok(())
    }

    /// `raw <hex>...`: sends each report 30 times, 8 ms apart, on a virtual mouse, and prints what the
    /// daemon said. All log lines go to raw.log.
    async fn raw_reports(&mut self, reports: &[String]) -> Res {
        let id = self.create(&Spec::default()).await?;
        pause(1200).await?;
        let mut all = take_log();
        for hex in reports {
            let bytes: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
            println!("-- {hex}");
            let raw = self.raw.as_mut().ok_or("no client")?;
            for _ in 0..30 {
                raw.send_object(send_request(id, bytes.clone()), false).await.map_err(|e| format!("report: {e:?}"))?;
                sleep(Duration::from_millis(8)).await;
            }
            pause(1500).await?;
            let log = take_log();
            let mut seen = std::collections::BTreeMap::<String, u32>::new();
            for l in log.iter().filter(|l| !l.contains("] [0x") && l.contains("[dtuhidd]")) { *seen.entry(l.split_once(' ').map_or(l.as_str(), |x| x.1).chars().take(170).collect()).or_default() += 1; }
            for (l, n) in seen.iter().take(25) { println!("  {n}x {l}"); }
            all.extend(log);
        }
        let _ = std::fs::write("C:/dev/_ref/tmp/pointer/raw.log", all.join("\n"));
        Ok(())
    }

    pub async fn pointer_report(&mut self, id: u64, dx: i8, dy: i8, buttons: u8) -> Res {
        let raw = self.raw.as_mut().ok_or("no client")?;
        raw.send_object(send_request(id, r05(dx, dy, buttons)), false).await.map_err(|e| format!("report: {e:?}"))
    }

    /// From the bottom-left corner, a known count of steady reports. The log lags and misses too much
    /// to steer by, so it only verifies. Not the top edge: that pulls down Notification Centre.
    /// Coordinates are landscape pixels, 2732 x 2048.
    /// ponytail: open loop at one speed; a real driver would just send the user's mouse deltas.
    async fn goto(&mut self, id: u64, target: (f64, f64)) -> Res {
        const PX_PER_REPORT: f64 = 18.0; // At 16 units a report, measured.
        for _ in 0..220 {
            self.pointer_report(id, -100, 100, 0).await?;
            sleep(Duration::from_millis(8)).await;
        }
        for (n, dx, dy) in [(target.0, 16, 0), (2048.0 - target.1, 0, -16)] {
            for _ in 0..(n / PX_PER_REPORT).round() as usize {
                self.pointer_report(id, dx, dy, 0).await?;
                sleep(Duration::from_millis(8)).await;
            }
        }
        Ok(())
    }

    /// Moves the pointer both ways across the screen, hovers an icon and clicks it. Proof is
    /// backboardd's own position log plus screenshots of what the pointer did to the screen.
    async fn demo(&mut self, target: (f64, f64), tag: &str) -> Res {
        self.home().await?;
        let base = self.shot(&format!("{tag}-0home")).await?;
        let id = self.create(&Spec::default()).await?;
        pause(1200).await?;
        take_log();
        let mut trace = vec![];
        let mut last = base.clone();
        for (i, p) in [(600.0, 1500.0), (2000.0, 1500.0), (1000.0, 900.0)].into_iter().enumerate() {
            self.goto(id, p).await?;
            let shot = self.shot(&format!("{tag}-waypoint{i}")).await?;
            say_diff(&format!("waypoint {i} {p:?} vs previous"), &last, &shot);
            last = shot;
        }
        self.goto(id, target).await?;
        pause(500).await?;
        let hover = self.shot(&format!("{tag}-1hover")).await?;
        say_diff("hover vs home", &base, &hover);
        self.pointer_report(id, 0, 0, 1).await?;
        pause(70).await?;
        self.pointer_report(id, 0, 0, 0).await?;
        pause(3500).await?;
        let log = take_log();
        println!("  button lines: {:?}", log.iter().filter(|l| l.contains("button number")).map(|l| l.split_once("->").map_or("", |x| x.1).trim().to_string()).collect::<Vec<_>>());
        trace.extend(log);
        let clicked = self.shot(&format!("{tag}-2clicked")).await?;
        say_diff("clicked vs hover", &hover, &clicked);
        self.remove_mouse().await;
        self.home().await?;
        let end = self.shot(&format!("{tag}-3home")).await?;
        say_diff("home again vs home", &base, &end);
        let _ = std::fs::write(format!("C:/dev/_ref/tmp/pointer/{tag}-trace.log"), trace.join("\n"));
        let pts = positions(&trace);
        println!("  {} positions logged; x {:.0}..{:.0}, y {:.0}..{:.0}", pts.len(),
            pts.iter().map(|p| p.0).fold(f64::MAX, f64::min), pts.iter().map(|p| p.0).fold(f64::MIN, f64::max),
            pts.iter().map(|p| p.1).fold(f64::MAX, f64::min), pts.iter().map(|p| p.1).fold(f64::MIN, f64::max));
        Ok(())
    }

    /// Pulls down Spotlight by touch (touch axes are portrait, so landscape down is X) and types into
    /// it with the virtual keyboard, so nothing is kept.
    async fn typed(&mut self, tag: &str) -> Res {
        self.home().await?;
        self.hid.drag(42000, CENTRE, 20000, CENTRE, 20, 16).await.map_err(|e| format!("swipe: {e:?}"))?;
        pause(1500).await?;
        let open = self.shot(&format!("{tag}-spotlight")).await?;
        self.type_uhid(GREETING).await?;
        pause(1500).await?;
        let typed = self.shot(&format!("{tag}-typed")).await?;
        say_diff("typed vs spotlight", &open, &typed);
        self.home().await?;
        self.home().await
    }

    /// Report 05, 19 bytes: one byte at a time set to `value`, the rest zero, then zeros to release.
    async fn bytes(&mut self, value: u8) -> Res {
        let id = self.create(&Spec::default()).await?;
        pause(1200).await?;
        for pos in 1..19usize {
            let raw = self.raw.as_mut().ok_or("no client")?;
            take_log();
            let mut r = vec![0u8; 19];
            r[0] = 5;
            r[pos] = value;
            for _ in 0..25 {
                raw.send_object(send_request(id, r.clone()), false).await.map_err(|e| format!("report: {e:?}"))?;
                sleep(Duration::from_millis(8)).await;
            }
            let mut z = vec![0u8; 19];
            z[0] = 5;
            raw.send_object(send_request(id, z), false).await.map_err(|e| format!("report: {e:?}"))?;
            pause(700).await?;
            let log = take_log();
            println!("  byte {pos:2}: {:?} buttons {:?}", track(&log), log.iter().filter(|l| l.contains("button number")).count());
        }
        Ok(())
    }

    pub async fn run_probe(&mut self, args: &[String], dir: &Path) -> Res {
        tokio::spawn(async { if let Err(e) = logtap().await { println!("log tap failed: {e}") } });
        pause(800).await?;
        match args[0].as_str() {
            "home" => self.home().await,
            "typed" => self.typed(args.get(1).map_or("typed", |s| s.as_str())).await,
            "ids" => self.ids().await,
            "raw" => self.raw_reports(&args[1..]).await,
            "demo" => self.demo((1980.0, 1898.0), args.get(1).map_or("demo", |s| s.as_str())).await,
            "bytes" => self.bytes(args.get(1).and_then(|v| v.parse().ok()).unwrap_or(16)).await,
            "shot" => self.shot(args.get(1).map_or("shot", |s| s.as_str())).await.map(|_| ()),
            "dump" => self.dump().await,
            "do" => self.steps(&args[1..]).await,
            "scroll" => self.scroll7(args.get(1).map_or("scroll", |s| s.as_str())).await,
            "rightdrag" => self.rightdrag(args.get(1).map_or("rd", |s| s.as_str())).await,
            "streamtime" => match args[1].as_str() {
                "warm" => self.streamtime_warm(args.get(2).and_then(|v| v.parse().ok()).unwrap_or(50)).await,
                _ => self.streamtime_pointer().await,
            },
            "callcheck" => self.callcheck(args.get(1).and_then(|v| v.parse().ok()).unwrap_or(180)).await,
            "twotunnels" => self.twotunnels(dir).await,
            "idle" => self.idle(args.get(1).map_or("rtt", |s| s.as_str()), args.get(2).and_then(|v| v.parse().ok()).unwrap_or(300)).await,
            "keyboard" => self.keyboard(args.get(1).map_or("kb", |s| s.as_str())).await,
            "probe" => self.stages(args.get(1).map_or("", |s| s.as_str())).await,
            other => Err(format!("unknown command {other}")),
        }
    }
}

// ---- Proofs for the Wi-Fi build. Results: docs/spikes/ipad-pointer.md ----

/// Report 7, 13 bytes: `07 00 00 x y`, then eight zeros. `x` and `y` are signed 8-bit. From the daemon's
/// `IOHIDEventCreateScrollEvent` call: bits 0x10 and 0x18 of the collection, which starts after the ID.
pub fn r07(x: i8, y: i8) -> Vec<u8> {
    let mut r = vec![0u8; 13];
    (r[0], r[3], r[4]) = (7, x as u8, y as u8);
    r
}

const BACKSPACE: u16 = 0x2A;

fn hex(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)).collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Result<T, String> { s.and_then(|v| v.parse().ok()).ok_or("bad number in step".to_string()) }

fn record(dir: &Path) -> PathBuf { dir.join(if dir.join(SPIKE_RECORD).exists() { SPIKE_RECORD } else { RECORD }) }

fn ms(d: Duration) -> String { format!("{:.0} ms", d.as_secs_f64() * 1000.0) }

impl Session {
    async fn ensure_mouse(&mut self) -> Result<u64, String> {
        if let Some(id) = self.mouse { return Ok(id) }
        let id = self.create(&Spec::default()).await?;
        pause(1200).await?;
        Ok(id)
    }

    async fn send_raw(&mut self, id: u64, report: Vec<u8>) -> Res {
        self.raw.as_mut().ok_or("no client")?.send_object(send_request(id, report), false).await.map_err(|e| format!("report: {e:?}"))
    }

    /// All down in order, held, all up in reverse.
    async fn press(&mut self, keys: &[u16], hold: u64) -> Res {
        self.ensure_keyboard().await?;
        for k in keys { self.uhid_event(KeyboardUsage::new(*k).map_err(|e| e.to_string())?, true).await? }
        pause(hold).await?;
        for k in keys.iter().rev() { self.uhid_event(KeyboardUsage::new(*k).map_err(|e| e.to_string())?, false).await? }
        pause(40).await
    }

    async fn backspaces(&mut self, n: usize) -> Res {
        for _ in 0..n { self.press(&[BACKSPACE], 15).await? }
        Ok(())
    }

    /// Spotlight by touch, from Home.
    async fn spotlight(&mut self) -> Res {
        self.hid.drag(42000, CENTRE, 20000, CENTRE, 20, 16).await.map_err(|e| format!("swipe: {e:?}"))?;
        pause(1500).await
    }

    /// A small script for looking around, one process per look:
    /// home, spot, type:abc, key:E1+05, hold:0E,1500, shot:name, wait:ms, mouse, goto:x,y,
    /// click:bits, move:dx,dy,n[,buttons], rep:hex,n,ms, tap:x,y (touch, 0..65535).
    async fn steps(&mut self, steps: &[String]) -> Res {
        for step in steps {
            let (op, arg) = step.split_once(':').unwrap_or((step, ""));
            let mut a = arg.split(',');
            match op {
                "home" => self.home().await?,
                "spot" => self.spotlight().await?,
                "type" => self.type_uhid(&arg.replace('_', " ")).await?,
                "key" => { let keys: Vec<u16> = arg.split('+').map(|k| u16::from_str_radix(k, 16).map_err(|e| e.to_string())).collect::<Result<_, _>>()?; self.press(&keys, 40).await? }
                "hold" => { let k = u16::from_str_radix(a.next().unwrap_or(""), 16).map_err(|e| e.to_string())?; self.press(&[k], num(a.next())?).await? }
                "shot" => { self.shot(arg).await?; }
                "wait" => pause(num(Some(arg))?).await?,
                "stop" => println!("  stop: {}", short(self.display.stop_media_stream().await, 200)),
                "rtp" => println!("  media so far {}, status {}", self.rtp.load(Relaxed), short(self.display.get_media_stream_server_status().await, 200)),
                "start" => self.pumps = open_stream(&mut self.adapter, &mut self.display, &self.rtp).await?,
                "mouse" => { self.ensure_mouse().await?; }
                "goto" => { let id = self.ensure_mouse().await?; self.goto(id, (num(a.next())?, num(a.next())?)).await? }
                "click" => {
                    let id = self.ensure_mouse().await?;
                    self.pointer_report(id, 0, 0, num(Some(arg))?).await?;
                    pause(70).await?;
                    self.pointer_report(id, 0, 0, 0).await?;
                }
                "move" => {
                    let id = self.ensure_mouse().await?;
                    let (dx, dy, n): (i8, i8, u32) = (num(a.next())?, num(a.next())?, num(a.next())?);
                    let buttons: u8 = a.next().map_or(Ok(0), |b| num(Some(b)))?;
                    for _ in 0..n { self.pointer_report(id, dx, dy, buttons).await?; pause(8).await? }
                }
                "rep" => {
                    let id = self.ensure_mouse().await?;
                    let report = hex(a.next().unwrap_or(""))?;
                    let (n, gap): (u32, u64) = (num(a.next())?, num(a.next())?);
                    for _ in 0..n { self.send_raw(id, report.clone()).await?; pause(gap).await? }
                }
                "tap" => self.hid.tap(num(a.next())?, num(a.next())?).await.map_err(|e| format!("tap: {e:?}"))?,
                other => return Err(format!("unknown step {other}")),
            }
        }
        Ok(())
    }

    /// Proof 1. Open Weather first, on its widget. Page down and back up, then the hourly strip.
    async fn scroll7(&mut self, tag: &str) -> Res {
        let id = self.ensure_mouse().await?;
        self.goto(id, (1000.0, 1500.0)).await?;
        pause(1500).await?;
        let mut last = self.shot(&format!("{tag}-0")).await?;
        for (name, x, y, n) in [("down", 0, -16, 10), ("up", 0, 16, 10), ("left", -1, 0, 30), ("right", 1, 0, 30)] {
            for _ in 0..n { self.send_raw(id, r07(x, y)).await?; pause(16).await? }
            pause(900).await?;
            let shot = self.shot(&format!("{tag}-{name}")).await?;
            say_diff(name, &last, &shot);
            last = shot;
        }
        Ok(())
    }

    /// Proof 2. Right click the Files icon, read the menu, close it with Home. Then hold the primary
    /// button across empty Home Screen: a page swipe is a drag with the button held.
    async fn rightdrag(&mut self, tag: &str) -> Res {
        self.home().await?;
        let id = self.ensure_mouse().await?;
        self.goto(id, (1980.0, 1898.0)).await?;
        pause(400).await?;
        let hover = self.shot(&format!("{tag}-0hover")).await?;
        self.pointer_report(id, 0, 0, 2).await?;
        pause(70).await?;
        self.pointer_report(id, 0, 0, 0).await?;
        pause(1500).await?;
        let menu = self.shot(&format!("{tag}-1rightclick")).await?;
        say_diff("right click vs hover", &hover, &menu);
        self.home().await?;
        self.home().await?;
        self.goto(id, (1762.0, 960.0)).await?;
        pause(400).await?;
        let before = self.shot(&format!("{tag}-2dragstart")).await?;
        self.pointer_report(id, 0, 0, 1).await?;
        pause(120).await?;
        for _ in 0..60 { self.pointer_report(id, -16, 0, 1).await?; pause(8).await? }
        let held = self.shot(&format!("{tag}-3dragheld")).await?;
        say_diff("held vs start", &before, &held);
        self.pointer_report(id, 0, 0, 0).await?;
        pause(1200).await?;
        let after = self.shot(&format!("{tag}-4dragend")).await?;
        say_diff("released vs start", &before, &after);
        self.home().await
    }

    async fn restart_stream(&mut self) -> Res {
        let _ = self.display.stop_media_stream().await;
        self.pumps.iter().for_each(JoinHandle::abort);
        self.pumps = open_stream(&mut self.adapter, &mut self.display, &self.rtp).await?;
        Ok(())
    }

    /// Proof 3, tunnel and keyboard kept: stream off, then a to z, one every `step` ms, start the moment
    /// the stream is asked for. The first letter on screen dates the first accepted report.
    /// Needs Spotlight open (`do home spot`).
    async fn streamtime_warm(&mut self, step: u64) -> Res {
        self.ensure_keyboard().await?;
        for run in 1..=5 {
            let _ = self.display.stop_media_stream().await;
            self.pumps.iter().for_each(JoinHandle::abort);
            pause(1500).await?;
            let Session { adapter, display, hid, keyboard, rtp, pumps, .. } = self;
            let kb = keyboard.as_mut().ok_or("no keyboard")?;
            let t = Instant::now();
            let start = async {
                let r = open_stream(adapter, display, rtp).await;
                (r, t.elapsed())
            };
            let letters = async {
                for c in 'a'..='z' {
                    let key = KeyboardUsage::new(usage(c).unwrap()).map_err(|e| e.to_string())?;
                    hid.main_keyboard_key_down(kb, key).await.map_err(|e| format!("key: {e}"))?;
                    hid.main_keyboard_key_up(kb, key).await.map_err(|e| format!("key: {e}"))?;
                    sleep(Duration::from_millis(step)).await;
                }
                Ok::<_, String>(t.elapsed())
            };
            let ((started, took), sent) = tokio::join!(start, letters);
            *pumps = started?;
            println!("run {run}: stream up after {}, letters done after {:?}, one each {step} ms", ms(took), sent.map(ms));
            pause(800).await?;
            self.shot(&format!("st-warm{run}")).await?;
            self.backspaces(30).await?;
        }
        Ok(())
    }

    /// Proof 3, new mouse: one report `delay` after createService, on a quiet Home Screen.
    async fn streamtime_pointer(&mut self) -> Res {
        self.home().await?;
        for delay in [0, 100, 300, 600, 1200] {
            self.remove_mouse().await;
            pause(3000).await?;
            let quiet = self.shot(&format!("st-p{delay}-a")).await?;
            let mut c = ScreenCaptureServiceClient::<Io>::connect_rsd(&mut self.adapter, &mut self.handshake).await.map_err(|e| format!("capture: {e:?}"))?;
            let t = Instant::now();
            let id = self.create(&Spec::default()).await?;
            let made = t.elapsed();
            pause(delay).await?;
            self.pointer_report(id, 60, 0, 0).await?;
            sleep(Duration::from_millis(100)).await;
            let png = c.take_screenshot(None, ImageFormat::Png).await.map_err(|e| format!("screenshot: {e:?}"))?;
            let path = format!(r"{SHOTS}\st-p{delay}-b.png");
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
            println!("createService took {}, report {delay} ms later", ms(made));
            say_diff("one report vs quiet", &quiet, &path);
        }
        Ok(())
    }

    /// Proof 5. Tunnel one holds the stream and pointer; tunnel two does pasteboard reads. Only the
    /// change count is printed, the pasteboard's content never is.
    async fn twotunnels(&mut self, dir: &Path) -> Res {
        let id = self.ensure_mouse().await?;
        self.goto(id, (1366.0, 1024.0)).await?;
        let before = self.rtp.load(Relaxed);
        println!("opening the second tunnel");
        let (mut adapter, mut handshake) = connect_wifi(&record(dir)).await.map_err(|e| format!("second tunnel: {e}"))?;
        println!("second tunnel up");
        let mut watcher = PasteboardServiceClient::<Io>::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard: {e:?}"))?;
        let mut reader = PasteboardServiceClient::<Io>::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard: {e:?}"))?;
        watcher.set_change_notifications(true, GENERAL_PASTEBOARD, Some(DataInclusionPolicy::AllPromised)).await.map_err(|e| format!("notifications: {e:?}"))?;
        let (mut trips, mut reads, mut failed) = (vec![], vec![], 0);
        for phase in ["first stream", "stream restarted with tunnel two up"] {
            for i in 0..60 {
                self.pointer_report(id, if i % 2 == 0 { 30 } else { -30 }, 0, 0).await?;
                let t = Instant::now();
                self.surfaces().await?;
                trips.push(t.elapsed());
                let t = Instant::now();
                match reader.get_with_policy(GENERAL_PASTEBOARD, DataInclusionPolicy::AllPromised).await {
                    Ok(s) => { reads.push(t.elapsed()); if i % 20 == 0 { println!("  {phase}: change count {:?}", s.change_count) } }
                    Err(e) => { failed += 1; println!("  pasteboard read failed: {}", short(e, 80)) }
                }
                if i == 30 {
                    let a = self.shot(&format!("tt-{}a", phase.len())).await?;
                    for _ in 0..20 { self.pointer_report(id, 16, 8, 0).await?; pause(8).await? }
                    let b = self.shot(&format!("tt-{}b", phase.len())).await?;
                    say_diff("pointer moved during the pasteboard reads", &a, &b);
                }
                pause(250).await?;
            }
            if phase == "first stream" {
                self.restart_stream().await?;
                pause(500).await?;
            }
        }
        print_times("tunnel one, HID round trip", &mut trips);
        print_times("tunnel two, pasteboard read", &mut reads);
        println!("failed reads {failed}, media packets during the test {}", self.rtp.load(Relaxed) - before);
        let _ = adapter.close().await;
        pause(1000).await?;
        self.surfaces().await?;
        println!("tunnel one still answers after tunnel two closed");
        Ok(())
    }

    /// One lone report and a shot, compared with the last shot before it. The pointer stays drawn, so a
    /// report that landed moves it. Nothing of ours but the keepalive precedes the report.
    async fn lone_report(&mut self, id: u64, name: &str, dx: i8, before: &str) -> Result<String, String> {
        self.pointer_report(id, dx, 0, 0).await?;
        sleep(Duration::from_millis(100)).await;
        let seen = self.shot(name).await?;
        say_diff(name, before, &seen);
        Ok(seen)
    }

    /// Proof 7. `rtt`: connectedServices every 500 ms. `zero`: an empty report 5 every 2 s. `none`.
    async fn idle(&mut self, mode: &str, secs: u64) -> Res {
        self.home().await?;
        let id = self.ensure_mouse().await?;
        self.goto(id, (1366.0, 1024.0)).await?;
        pause(3000).await?;
        let start = self.shot(&format!("idle-{mode}-0")).await?;
        let control = self.lone_report(id, &format!("idle-{mode}-control"), 60, &start).await?;
        println!("idling {secs} s, keepalive {mode}");
        let (end, mut worst, mut tick) = (Instant::now() + Duration::from_secs(secs), Duration::ZERO, 0u64);
        while Instant::now() < end {
            match mode {
                "rtt" => { let t = Instant::now(); self.surfaces().await?; worst = worst.max(t.elapsed()); pause(500).await? }
                "zero" => { self.pointer_report(id, 0, 0, 0).await?; pause(2000).await? }
                _ => pause(1000).await?,
            }
            tick += 1;
        }
        println!("{tick} ticks, worst round trip {}", ms(worst));
        let first = self.lone_report(id, &format!("idle-{mode}-first"), -60, &control).await?;
        self.lone_report(id, &format!("idle-{mode}-second"), -60, &first).await.map(|_| ())
    }

    /// Proof 8. Everything lands in Spotlight, so nothing is kept. Enter is never sent.
    async fn keyboard(&mut self, tag: &str) -> Res {
        let (shift, ctrl, alt, cmd, ralt) = (0xE1u16, 0xE0u16, 0xE2u16, 0xE3u16, 0xE6u16);
        self.home().await?;
        self.spotlight().await?;
        self.ensure_keyboard().await?;
        let k = |c: char| usage(c).unwrap();
        // Shift: expect aBc.
        self.type_uhid("a").await?;
        self.press(&[shift, k('b')], 40).await?;
        self.type_uhid("c").await?;
        pause(600).await?;
        self.shot(&format!("{tag}-1shift")).await?;
        self.backspaces(4).await?;
        // Arrows: abc, Left twice, x, Right twice, w: expect axbcw.
        self.type_uhid("abc").await?;
        self.press(&[0x50], 40).await?;
        self.press(&[0x50], 40).await?;
        self.type_uhid("x").await?;
        self.press(&[0x4F], 40).await?;
        self.press(&[0x4F], 40).await?;
        self.type_uhid("w").await?;
        pause(600).await?;
        self.shot(&format!("{tag}-2arrows")).await?;
        self.backspaces(7).await?;
        // Ctrl+A goes to the line start: expect qxv.
        self.type_uhid("xv").await?;
        self.press(&[ctrl, k('a')], 40).await?;
        self.type_uhid("q").await?;
        pause(600).await?;
        self.shot(&format!("{tag}-3ctrl")).await?;
        // Cmd+A selects all, the next letter replaces it: expect m.
        self.press(&[cmd, k('a')], 40).await?;
        pause(500).await?;
        self.shot(&format!("{tag}-4cmd-selected")).await?;
        self.type_uhid("m").await?;
        pause(600).await?;
        self.shot(&format!("{tag}-4cmd")).await?;
        self.backspaces(3).await?;
        // Option and right Option: the layout decides.
        for key in [(alt, k('l')), (alt, 0x22), (ralt, k('q')), (ralt, k('e'))] { self.press(&[key.0, key.1], 40).await? }
        pause(600).await?;
        self.shot(&format!("{tag}-5option")).await?;
        self.backspaces(8).await?;
        // A held key: repeats are the iPad's own, one down and one up go out.
        self.press(&[k('k')], 2000).await?;
        pause(600).await?;
        self.shot(&format!("{tag}-6held")).await?;
        self.press(&[BACKSPACE], 2500).await?;
        pause(400).await?;
        self.shot(&format!("{tag}-6held-cleared")).await?;
        // The keys under ae, oe, ue, sz on a German keyboard, then shifted, then z and y and the ISO key.
        for key in [0x34, 0x33, 0x2F, 0x2D] { self.press(&[key], 40).await? }
        self.press(&[0x2C], 40).await?;
        for key in [0x34, 0x33, 0x2F] { self.press(&[shift, key], 40).await? }
        self.press(&[0x2C], 40).await?;
        for key in [k('z'), k('y'), 0x64] { self.press(&[key], 40).await? }
        pause(800).await?;
        self.shot(&format!("{tag}-7german")).await?;
        self.backspaces(16).await?;
        // Esc.
        self.type_uhid("esc").await?;
        pause(600).await?;
        let open = self.shot(&format!("{tag}-8before-esc")).await?;
        self.press(&[0x29], 40).await?;
        pause(1200).await?;
        let esc = self.shot(&format!("{tag}-8esc")).await?;
        say_diff("Esc vs before", &open, &esc);
        self.home().await?;
        self.home().await?;
        // F keys: the ones with no lasting effect. F5 to F10 can start dictation, change Focus or play
        // media, so they are not sent. F1 and F2, F11 and F12 undo each other.
        for (name, key) in [("f1", 0x3A), ("f2", 0x3B), ("f3", 0x3C), ("f4", 0x3D), ("f11", 0x44), ("f12", 0x45)] {
            self.press(&[key], 40).await?;
            sleep(Duration::from_millis(150)).await;
            self.shot(&format!("{tag}-9{name}")).await?;
            pause(1500).await?;
        }
        self.home().await?;
        self.home().await
    }

    /// Proof 4. A small wiggle, a round trip and a media count twice a second while the owner starts
    /// and ends a call. If the media stops for 3 s, tries to restart the stream every 10 s. No clicks.
    async fn callcheck(&mut self, secs: u64) -> Res {
        let id = self.ensure_mouse().await?;
        self.goto(id, (1366.0, 1024.0)).await?;
        println!("start the call whenever you like, the run lasts {secs} s");
        let (start, mut seen, mut quiet, mut flip) = (Instant::now(), self.rtp.load(Relaxed), 0u32, 1i8);
        let mut tick = 0u64;
        while start.elapsed() < Duration::from_secs(secs) {
            flip = -flip;
            let report = self.pointer_report(id, flip * 12, 0, 0).await;
            let t = Instant::now();
            let trip = match timeout(Duration::from_secs(3), self.surfaces()).await {
                Ok(Ok(_)) => ms(t.elapsed()),
                Ok(Err(e)) => short(e, 60),
                Err(_) => "no reply in 3 s".into(),
            };
            let now = self.rtp.load(Relaxed);
            println!("{:6.1}s report {} trip {trip} media +{}", start.elapsed().as_secs_f64(), if report.is_ok() { "ok" } else { "ERR" }, now - seen);
            quiet = if now == seen { quiet + 1 } else { 0 };
            seen = now;
            if tick % 20 == 0 {
                println!("  status {}", timeout(Duration::from_secs(3), self.display.get_media_stream_server_status()).await.map_or("no reply".into(), |v| short(v, 200)));
                if let Err(e) = self.shot(&format!("call-{tick:04}")).await { println!("  shot failed: {e}") }
            }
            if quiet >= 6 && quiet % 20 == 6 && std::env::var_os("NOSTREAM").is_none() {
                match timeout(Duration::from_secs(5), self.restart_stream()).await {
                    Ok(r) => println!("  restart: {}", short(r, 160)),
                    Err(_) => println!("  restart: no reply in 5 s"),
                }
            }
            tick += 1;
            pause(500).await?;
        }
        Ok(())
    }
}

/// Proof 3 from nothing: no tunnel, no stream. Needs Spotlight open (`do home spot`). SETTLE_MS changes
/// the wait after the stream starts.
pub async fn streamtime_cold(dir: &Path, step: u64) -> Res {
    for run in 1..=5 {
        let t = Instant::now();
        let (mut adapter, mut handshake) = connect_wifi(&record(dir)).await?;
        let tunnel = t.elapsed();
        let display = DisplayServiceClient::<Io>::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("display: {e:?}"))?;
        let mut s = Session::start(adapter, handshake, display).await?;
        let stream = t.elapsed();
        s.ensure_keyboard().await?;
        let keyboard = t.elapsed();
        for c in 'a'..='z' {
            s.press(&[usage(c).unwrap()], 0).await?;
            pause(step).await?;
        }
        println!("run {run}: tunnel {}, stream and HID {}, keyboard {}, first letter sent then, one each {step} ms", ms(tunnel), ms(stream), ms(keyboard));
        pause(800).await?;
        s.shot(&format!("st-cold{run}")).await?;
        s.backspaces(30).await?;
        s.finish().await;
        pause(2000).await?;
    }
    Ok(())
}

/// Proof 6, on a restarted iPad, unlocked once. Lockdown over Wi-Fi first, with the record Apple Devices
/// keeps (path to the library only), then the tunnel. `dry` stops before anything is mounted.
pub async fn wifimount(dir: &Path, dry: bool) -> Res {
    use idevice::{lockdown::LockdownClient, pairing_file::PairingFile, provider::TcpProvider};
    let ddi = dir.join("ddi");
    let read = |name: &str| std::fs::read(ddi.join(name)).map_err(|e| format!("{name}: {e}"));
    let (image, trust_cache, manifest) = (read("Image.dmg")?, read("Image.dmg.trustcache")?, read("BuildManifest.plist")?);
    let ip: std::net::IpAddr = std::fs::read_to_string(dir.join("ipad-last-ip.txt")).map_err(|e| format!("last ip: {e}"))?.trim().parse().map_err(|e| format!("last ip: {e}"))?;
    const LIMIT: Duration = Duration::from_secs(180);

    let lockdown = async {
        let found = std::fs::read_dir(r"C:\ProgramData\Apple\Lockdown").map_err(|e| format!("lockdown folder: {e}"))?.flatten().map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "plist") && p.file_stem().is_some_and(|n| n != "SystemConfiguration")).ok_or("no lockdown record")?;
        let pairing_file = match PairingFile::read_from_file(&found) {
            Ok(f) => f,
            Err(_) => {
                let udid = found.file_stem().unwrap().to_string_lossy().to_string();
                UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd: {e:?}"))?.get_pair_record(&udid).await.map_err(|e| format!("pair record: {e:?}"))?
            }
        };
        let provider = TcpProvider { addr: ip, scope_id: None, pairing_file: pairing_file.clone(), label: HOST_NAME.into() };
        let mut lockdown = LockdownClient::connect(&provider).await.map_err(|e| format!("lockdown connect: {e:?}"))?;
        lockdown.start_session(&pairing_file).await.map_err(|e| format!("lockdown session: {e:?}"))?;
        let chip_id = lockdown.get_value(Some("UniqueChipID"), None).await.map_err(|e| format!("chip id: {e:?}"))?.as_unsigned_integer().ok_or("no chip id")?;
        let mut mounter = ImageMounter::connect(&provider).await.map_err(|e| format!("image mounter: {e:?}"))?;
        let mounted = mounter.copy_devices().await.map_err(|e| format!("copy devices: {e:?}"))?.len();
        println!("lockdown over Wi-Fi reached the mounter, {mounted} images mounted");
        if dry || mounted > 0 { return Ok(()) }
        mounter.mount_personalized(&provider, image.clone(), trust_cache.clone(), &manifest, None, chip_id).await.map_err(|e| format!("mount: {e:?}"))
    };
    let t = Instant::now();
    let by_lockdown: Res = timeout(LIMIT, lockdown).await.unwrap_or(Err("timed out".into()));
    println!("lockdown route after {}: {by_lockdown:?}", ms(t.elapsed()));
    if by_lockdown.is_ok() || dry { return by_lockdown }

    let t = Instant::now();
    let (mut adapter, mut handshake) = connect_wifi(&record(dir)).await?;
    let by_tunnel = timeout(LIMIT, mount_ddi(dir, &mut adapter, &mut handshake)).await.unwrap_or(Err("timed out".into()));
    println!("tunnel route after {}: {by_tunnel:?}", ms(t.elapsed()));
    let _ = adapter.close().await;
    by_tunnel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_box() {
        let a = vec![0u8; 3 * 4 * 3];
        let mut b = a.clone();
        b[(4 + 1) * 3] = 200;
        b[(2 * 4 + 3) * 3 + 2] = 200;
        assert_eq!(changed(4, &a, &b, 24), Change { pixels: 2, bbox: Some((1, 1, 3, 2)) });
        assert_eq!(changed(4, &a, &a, 24).pixels, 0);
    }

    #[test]
    fn pointer_track() {
        let log = ["t [backboardd] Pointer 46,2046 (global) buttonMask:11 edges:bottom", "t [backboardd] Pointer 1,23 (global) buttonMask:11", "t [backboardd] Pointer 100,2000 (global) buttonMask:0", "t [backboardd] Pointer 24,50 (global) buttonMask:0"].map(String::from);
        assert_eq!(track(&log), Some(((46.0, 2046.0), (100.0, 2000.0), 2, "0".into())));
        assert_eq!(r05(-1, 2, 1)[..3], [5, 0xFF, 2]);
        assert_eq!((r05(0, 0, 1).len(), r05(0, 0, 1)[11]), (19, 1));
    }

    #[test]
    fn log_verdicts() {
        let log = ["x report type identifier 1", "y Failed to decode report", "x report type identifier 2", "x report type identifier 9", "No gesture for report ID 9"].map(String::from);
        assert_eq!(verdicts_of(&log), [(1, "fail"), (2, "ok"), (9, "none")]);
    }

    #[test]
    fn report_7_carries_x_and_y() {
        assert_eq!(r07(1, -1), [7, 0, 0, 1, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn report_13_carries_buttons() {
        let r = r13(1, -1, 1);
        assert_eq!((r.len(), r[0], r[9]), (19, 0x13, 1));
    }
}
