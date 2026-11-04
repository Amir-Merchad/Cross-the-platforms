//! Objective pointer tests: the iPad's own screenshots replace a human watching.
//! Findings live in docs/spikes/ipad-pointer.md.

use super::*;
use idevice::core_device::{ImageFormat, ScreenCaptureServiceClient};

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
        if all || l.filename.contains("dtuhidd") || l.image_name.contains("dtuhidd") {
            LOG.lock().unwrap().push(format!("{} [{}] {}", l.timestamp.format("%H:%M:%S%.3f"), l.filename.rsplit('/').next().unwrap_or(""), l.message));
        }
    }
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
    let _ = std::fs::write(r"C:\dev\_ref	mp\pointer\ids.log", log.join("
"));
    verdicts_of(log)
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
            for l in log.iter().filter(|l| !l.contains("] [0x")) { *seen.entry(l.split_once(' ').map_or(l.as_str(), |x| x.1).chars().take(170).collect()).or_default() += 1; }
            for (l, n) in seen.iter().take(25) { println!("  {n}x {l}"); }
            all.extend(log);
        }
        let _ = std::fs::write(r"C:\dev\_ref	mp\pointeraw.log", all.join("
"));
        Ok(())
    }

    pub async fn run_probe(&mut self, args: &[String]) -> Res {
        tokio::spawn(async { if let Err(e) = logtap().await { println!("log tap failed: {e}") } });
        pause(800).await?;
        match args[0].as_str() {
            "home" => self.home().await,
            "ids" => self.ids().await,
            "raw" => self.raw_reports(&args[1..]).await,
            "shot" => self.shot(args.get(1).map_or("shot", |s| s.as_str())).await.map(|_| ()),
            "dump" => self.dump().await,
            "probe" => self.stages(args.get(1).map_or("", |s| s.as_str())).await,
            other => Err(format!("{other}: not built yet")),
        }
    }
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
    fn log_verdicts() {
        let log = ["x report type identifier 1", "y Failed to decode report", "x report type identifier 2", "x report type identifier 9", "No gesture for report ID 9"].map(String::from);
        assert_eq!(verdicts_of(&log), [(1, "fail"), (2, "ok"), (9, "none")]);
    }

    #[test]
    fn report_13_carries_buttons() {
        let r = r13(1, -1, 1);
        assert_eq!((r.len(), r[0], r[9]), (19, 0x13, 1));
    }
}
