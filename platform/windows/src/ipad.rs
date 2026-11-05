//! iPad pasteboard over the CoreDevice pasteboard service, no app on the iPad. Wi-Fi via RemotePairing
//! (record made once over USB), USB as fallback. Ported from `spikes/ipad-clip`. Errors never carry
//! clipboard contents.

use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, path::{Path, PathBuf}, time::Duration};

use bridge_core::clipboard::{Content, Image, MAX_ITEM_BYTES, codec};
use idevice::{
    IdeviceService, ReadWrite, RemoteXpcClient, RsdService,
    core_device::{DataInclusionPolicy, GENERAL_PASTEBOARD, PasteboardServiceClient},
    core_device_proxy::CoreDeviceProxy,
    mobile_image_mounter::ImageMounter,
    remote_pairing::{RemotePairingClient, RpPairingFile, RpPairingSocket, connect_tls_psk_tunnel_native},
    rsd::RsdHandshake,
    tcp::handle::AdapterHandle,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};
use serde::Serialize;

const HOST_NAME: &str = "cross-the-platforms";
const RECORD: &str = "rp-pairing.plist";
/// iPad image types we read, best first. We always write PNG.
const IMAGE_UTIS: [&str; 3] = ["public.png", "public.jpeg", "public.tiff"];
const HTML_UTI: &str = "public.html";
const TEXT_UTIS: [&str; 3] = ["public.utf8-plain-text", "public.plain-text", "public.text"];
/// Pushes inline items up to this size and promise bigger ones (fetched on demand).
const INLINE_LIMIT: i64 = 256 * 1024;
/// Notices a silently dropped Wi-Fi link (iPad slept or left) instead of waiting forever.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(10);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);

/// Don't download what the size limit would throw away anyway.
fn worth_resolving(size: Option<i64>) -> bool { size.is_none_or(|s| s <= MAX_ITEM_BYTES as i64) }

/// Pushes at or below the change count seen when we subscribed describe the iPad's old clipboard:
/// replaying them on every reconnect would overwrite newer Windows copies.
fn is_fresh(change_count: Option<i64>, baseline: Option<i64>) -> bool {
    match (change_count, baseline) { (Some(c), Some(b)) => c > b, _ => true }
}

/// `%APPDATA%\CrossThePlatforms\ipad` — pairing record and developer disk image (never in git).
pub fn ipad_dir() -> PathBuf { crate::engine::settings_path().with_file_name("ipad") }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum IpadStatus { NotPaired, Connecting, Connected { wifi: bool }, Unavailable(String) }

/// Turns a connect/session error into something the owner can act on.
pub fn classify(err: &str) -> IpadStatus {
    let reason = if err.contains("pairing record") && err.contains("NotFound") {
        return IpadStatus::NotPaired;
    } else if err.contains("pair verify") && (err.contains("early eof") || err.contains("PairVerifyFailed")) {
        // The iPad holds another key for this pairing name: only a new pairing fixes it.
        return IpadStatus::NotPaired;
    } else if err.contains("just mounted") {
        return IpadStatus::Connecting;
    } else if err.contains("DDI mount over Wi-Fi") {
        "Plug the iPad in once by USB to finish setup after its reboot".to_string()
    } else if err.contains("ServiceNotFound") {
        "Developer disk image not mounted yet".to_string()
    } else if err.contains("_remotepairing._tcp not found") {
        "iPad not found on Wi-Fi (same network? awake?)".to_string()
    } else if err.contains("Developer Mode") {
        "iPad not reachable: Developer Mode on? (Settings → Privacy & Security)".to_string()
    } else {
        err.chars().take(120).collect()
    };
    IpadStatus::Unavailable(reason)
}

type Pasteboard = PasteboardServiceClient<Box<dyn ReadWrite>>;

/// A connected iPad, in two halves so reading (a long wait for the next push) never has to be
/// cancelled to write. Each half keeps the tunnel alive; it closes when both are dropped.
pub struct IpadLink { pub reader: IpadReader, pub writer: IpadWriter, pub wifi: bool }

pub struct IpadReader { _adapter: AdapterHandle, watcher: Pasteboard, resolver: Pasteboard, baseline: Option<i64> }

pub struct IpadWriter { _adapter: AdapterHandle, setter: Pasteboard }

impl IpadLink {
    /// Wi-Fi first (needs the pairing record), then USB. Mounts the developer disk image when the
    /// pasteboard service is missing (after an iPad reboot); the caller reconnects after that.
    pub async fn connect(dir: &Path) -> Result<IpadLink, String> {
        let (adapter, handshake, wifi) = connect_tunnel(dir).await?;
        Self::open(dir, adapter, handshake, wifi).await
    }

    async fn open(dir: &Path, mut adapter: AdapterHandle, mut handshake: RsdHandshake, wifi: bool) -> Result<IpadLink, String> {
        let mut watcher = match Pasteboard::connect_rsd(&mut adapter, &mut handshake).await {
            Ok(w) => w,
            Err(e) if format!("{e:?}").contains("ServiceNotFound") => {
                mount(dir, &mut adapter, &mut handshake, wifi).await?;
                // The handshake lists the services from before the mount: start over.
                return Err(JUST_MOUNTED.into());
            }
            Err(e) => return Err(format!("pasteboard (watch): {e:?}")),
        };
        let setter = Pasteboard::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (set): {e:?}"))?;
        let mut resolver = Pasteboard::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (resolve): {e:?}"))?;
        let baseline = resolver.get_with_policy(GENERAL_PASTEBOARD, DataInclusionPolicy::AllPromised).await.map_err(|e| format!("pasteboard baseline: {e:?}"))?.change_count;
        watcher.set_change_notifications(true, GENERAL_PASTEBOARD, Some(DataInclusionPolicy::Threshold(INLINE_LIMIT))).await.map_err(|e| format!("AUTONOTIFY: {e:?}"))?;
        Ok(IpadLink { reader: IpadReader { _adapter: adapter.clone(), watcher, resolver, baseline }, writer: IpadWriter { _adapter: adapter, setter }, wifi })
    }
}

fn snap_html(snap: &idevice::core_device::PasteboardSnapshot) -> Option<String> {
    use idevice::core_device::PasteboardPayload::Inline;
    snap.items.iter().flat_map(|i| &i.data).find_map(|e| match &e.payload { Inline(b) if e.uti == HTML_UTI => String::from_utf8(b.clone()).ok(), _ => None })
}

/// Formatting only when there is some: an empty or blank HTML part is dropped.
fn rich_or_text(text: String, html: Option<String>) -> Content {
    match html.filter(|h| !h.trim().is_empty()) { Some(html) => Content::Rich { text, html }, None => Content::Text(text) }
}

impl IpadReader {
    /// Waits for the next iPad copy that has text or a readable image.
    pub async fn next_change(&mut self) -> Result<Content, String> {
        loop {
            let snap = self.watcher.recv_push().await.map_err(|e| format!("push stream ended: {e:?}"))?;
            if !is_fresh(snap.change_count, self.baseline) { continue }
            // Small items arrive inline, big ones as promises: text first, then images, either way.
            let promised = snap.promised_items();
            let find = |utis: &[&str]| utis.iter().find_map(|u| promised.iter().find(|p| p.uti == *u && worth_resolving(p.size))).cloned();
            let text = match snap.text().filter(|t| !t.is_empty()) {
                Some(t) => Some(t),
                None => match find(&TEXT_UTIS) {
                    Some(p) => self.resolver.resolve(GENERAL_PASTEBOARD, p.item_index, &p.uti).await.map_err(|e| format!("resolve text: {e:?}"))?
                        .and_then(|b| String::from_utf8(b).ok()).filter(|t| !t.is_empty()),
                    None => None,
                },
            };
            if let Some(text) = text {
                // Formatting is a bonus: any trouble with the HTML and the plain text still goes through.
                let html = match snap_html(&snap) {
                    Some(h) => Some(h),
                    None => match find(&[HTML_UTI]) {
                        Some(p) => self.resolver.resolve(GENERAL_PASTEBOARD, p.item_index, &p.uti).await.ok().flatten().and_then(|b| String::from_utf8(b).ok()),
                        None => None,
                    },
                };
                return Ok(rich_or_text(text, html));
            }
            let encoded = match snap.image() {
                Some((uti, bytes)) => Some((uti.to_string(), bytes)),
                None => match find(&IMAGE_UTIS) {
                    Some(p) => self.resolver.resolve(GENERAL_PASTEBOARD, p.item_index, &p.uti).await.map_err(|e| format!("resolve image: {e:?}"))?.map(|b| (p.uti, b)),
                    None => None,
                },
            };
            let Some((uti, bytes)) = encoded else { continue };
            match decode_off_thread(bytes).await {
                Ok(img) => return Ok(Content::Image(img)),
                Err(e) => crate::diag::log(format_args!("iPad image not decodable ({uti}): {e}")),
            }
        }
    }

}

impl IpadWriter {
    pub async fn write(&mut self, c: &Content) -> Result<(), String> {
        match c {
            // Plain text only for now: set_image would send public.html with no text type, and a plain field
            // (or Bridge's own iPad app) would get nothing. Formatted paste waits on spikes/ipad-input htmlset.
            Content::Text(t) | Content::Rich { text: t, .. } => self.setter.set_text(t, GENERAL_PASTEBOARD).await,
            Content::Image(i) => {
                let img = i.clone();
                let png = tokio::task::spawn_blocking(move || codec::encode_png(&img)).await.map_err(|e| e.to_string())??;
                self.setter.set_image(&png, "public.png", GENERAL_PASTEBOARD).await
            }
        }
        .map_err(|e| format!("iPad write: {e:?}"))
    }
}

/// Image codecs take seconds on big screenshots: keep them off the async thread.
async fn decode_off_thread(bytes: Vec<u8>) -> Result<Image, String> {
    tokio::task::spawn_blocking(move || codec::decode(&bytes)).await.map_err(|e| e.to_string())?
}

#[derive(Debug)]
pub struct DdiFiles { pub image: PathBuf, pub trust_cache: PathBuf, pub manifest: PathBuf }

/// The personalized developer disk image files in `<dir>\ddi`; the error names the first missing one.
pub fn ddi_files(dir: &Path) -> Result<DdiFiles, String> {
    let ddi = dir.join("ddi");
    let f = |name: &str| { let p = ddi.join(name); if p.is_file() { Ok(p) } else { Err(format!("missing {}", p.display())) } };
    Ok(DdiFiles { image: f("Image.dmg")?, trust_cache: f("Image.dmg.trustcache")?, manifest: f("BuildManifest.plist")? })
}

/// iPads unmount the developer disk image on every reboot; the pasteboard and HID services need it.
/// Over Wi-Fi it mounts through the tunnel (proof 6: about 2 s after a restart). Over USB it mounts
/// through lockdown, because the tunnel route timed out on USB.
pub async fn mount(dir: &Path, adapter: &mut AdapterHandle, handshake: &mut RsdHandshake, wifi: bool) -> Result<(), String> {
    if wifi {
        mount_ddi(dir, adapter, handshake).await.map_err(|e| format!("DDI mount over Wi-Fi failed: {e}"))
    } else {
        mount_usb(dir).await
    }
}

/// The error text a caller returns after mounting, so it reconnects with a fresh service list.
pub const JUST_MOUNTED: &str = "developer disk image just mounted; reconnecting";

/// Image, trust cache and build manifest.
type DdiBytes = (Vec<u8>, Vec<u8>, Vec<u8>);

fn read_ddi(dir: &Path) -> Result<DdiBytes, String> {
    let files = ddi_files(dir)?;
    let read = |p: &Path, name: &str| std::fs::read(p).map_err(|e| format!("disk image file {name}: {e}"));
    Ok((read(&files.image, "image")?, read(&files.trust_cache, "trust cache")?, read(&files.manifest, "manifest")?))
}

async fn mount_ddi(dir: &Path, adapter: &mut AdapterHandle, handshake: &mut RsdHandshake) -> Result<(), String> {
    let (image, trust_cache, manifest) = read_ddi(dir)?;
    let chip_id = handshake.properties.get("UniqueChipID").and_then(|v| v.as_unsigned_integer()).ok_or("iPad didn't report its chip id")?;
    let mut mounter = ImageMounter::connect_rsd(adapter, handshake).await.map_err(|e| format!("image mounter: {e:?}"))?;
    mounter.mount_personalized_rsd(adapter, handshake, image, trust_cache, &manifest, None, chip_id).await.map_err(|e| format!("DDI mount: {e:?}"))?;
    crate::diag::log("iPad developer disk image mounted over the tunnel");
    Ok(())
}

/// Over lockdown on USB, as the spike's working mounter does. The pairing record goes to the library
/// only: never read or printed here.
pub async fn mount_usb(dir: &Path) -> Result<(), String> {
    use idevice::{lockdown::LockdownClient, provider::IdeviceProvider};
    let (image, trust_cache, manifest) = read_ddi(dir)?;
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd not reachable (Apple Devices installed?): {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad on USB")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().map_err(|e| format!("usbmuxd address: {e:?}"))?, HOST_NAME);
    let mut lockdown = LockdownClient::connect(&provider).await.map_err(|e| format!("lockdown: {e:?}"))?;
    let pairing = provider.get_pairing_file().await.map_err(|e| format!("lockdown pairing: {e:?}"))?;
    lockdown.start_session(&pairing).await.map_err(|e| format!("lockdown session: {e:?}"))?;
    let chip_id = lockdown.get_value(Some("UniqueChipID"), None).await.map_err(|e| format!("chip id: {e:?}"))?.as_unsigned_integer().ok_or("iPad didn't report its chip id")?;
    let mut mounter = ImageMounter::connect(&provider).await.map_err(|e| format!("image mounter: {e:?}"))?;
    mounter.mount_personalized(&provider, image, trust_cache, &manifest, None, chip_id).await.map_err(|e| format!("DDI mount: {e:?}"))?;
    crate::diag::log("iPad developer disk image mounted over USB");
    Ok(())
}

/// USB: usbmux → CoreDeviceProxy tunnel → RSD.
async fn connect_usb() -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd not reachable (Apple Devices installed?): {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad on USB")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().map_err(|e| format!("usbmuxd address: {e:?}"))?, HOST_NAME);
    let proxy = CoreDeviceProxy::connect(&provider).await.map_err(|e| format!("core device proxy (Developer Mode on? DDI mounted?): {e:?}"))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().map_err(|e| format!("software tunnel: {e:?}"))?.to_async_handle();
    let stream = adapter.connect(rsd_port).await.map_err(|e| format!("RSD connect: {e:?}"))?;
    let handshake = RsdHandshake::new(stream).await.map_err(|e| format!("RSD handshake: {e:?}"))?;
    Ok((adapter, handshake))
}

// ---- Discovery: the iPad's `_remotepairing._tcp` port changes, and multicast fails on the owner's PC ----

const LAST_IP: &str = "ipad-last-ip.txt";
const NOT_FOUND: &str = "iPad's _remotepairing._tcp not found on the network";

pub fn save_last_ip(dir: &Path, ip: IpAddr) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(LAST_IP), ip.to_string())
}

pub fn load_last_ip(dir: &Path) -> Option<IpAddr> {
    std::fs::read_to_string(dir.join(LAST_IP)).ok()?.trim().parse().ok()
}

/// Saved address by unicast, then a multicast browse, then a unicast sweep of the local /24, 32 at a
/// time. The address found is saved for next time.
pub async fn discover(dir: &Path) -> Result<SocketAddr, String> {
    let found = async {
        if let Some(ip) = load_last_ip(dir)
            && let Some(a) = unicast_lookup(ip, 3, Duration::from_millis(500)).await { return Some(a) }
        if let Ok(a) = browse(Duration::from_secs(4)).await { return Some(a) }
        sweep().await
    }.await;
    let addr = found.ok_or_else(|| NOT_FOUND.to_string())?;
    let _ = save_last_ip(dir, addr.ip());
    Ok(addr)
}

async fn browse(limit: Duration) -> Result<SocketAddr, String> {
    tokio::task::spawn_blocking(move || {
        let mdns = mdns_sd::ServiceDaemon::new().map_err(|e| format!("mDNS: {e}"))?;
        let rx = mdns.browse("_remotepairing._tcp.local.").map_err(|e| format!("mDNS browse: {e}"))?;
        let deadline = std::time::Instant::now() + limit;
        let mut found = None;
        while let Ok(ev) = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
                let addrs = info.get_addresses();
                if let Some(ip) = addrs.iter().find(|a| a.is_ipv4()).or(addrs.iter().next()) {
                    found = Some(SocketAddr::new(*ip, info.get_port()));
                    break;
                }
            }
        }
        let _ = mdns.shutdown();
        found.ok_or_else(|| NOT_FOUND.to_string())
    }).await.map_err(|e| e.to_string())?
}

/// The PC's address on the route to the internet. Nothing is sent: a UDP connect only picks a route.
fn local_ipv4() -> Option<Ipv4Addr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:53").ok()?;
    match s.local_addr().ok()?.ip() { IpAddr::V4(v) if !v.is_loopback() && !v.is_unspecified() => Some(v), _ => None }
}

/// Every other host in `ip`'s /24, nearest first, so a DHCP neighbour is tried early.
pub fn subnet_hosts(ip: Ipv4Addr) -> Vec<Ipv4Addr> {
    let [a, b, c, me] = ip.octets();
    let mut hosts: Vec<u8> = (1..=254).filter(|h| *h != me).collect();
    hosts.sort_by_key(|h| (*h as i16 - me as i16).abs());
    hosts.into_iter().map(|h| Ipv4Addr::new(a, b, c, h)).collect()
}

async fn sweep() -> Option<SocketAddr> {
    let hosts = subnet_hosts(local_ipv4()?);
    for chunk in hosts.chunks(32) {
        let mut set = tokio::task::JoinSet::new();
        for h in chunk { let ip = IpAddr::V4(*h); set.spawn(async move { unicast_lookup(ip, 1, Duration::from_millis(400)).await }); }
        while let Some(r) = set.join_next().await {
            if let Ok(Some(a)) = r { set.abort_all(); return Some(a) }
        }
    }
    None
}

/// One owner name in a DNS message and the offset after it. Follows compression pointers.
fn dns_name(m: &[u8], mut i: usize) -> Option<(String, usize)> {
    let (mut name, mut end, mut hops) = (String::new(), None, 0);
    loop {
        let len = *m.get(i)? as usize;
        if len == 0 { return Some((name, end.unwrap_or(i + 1))) }
        if len & 0xC0 == 0xC0 {
            end.get_or_insert(i + 2);
            hops += 1;
            if hops > 16 { return None }
            i = (len & 0x3F) << 8 | *m.get(i + 1)? as usize;
        } else {
            name.push_str(&String::from_utf8_lossy(m.get(i + 1..i + 1 + len)?));
            name.push('.');
            i += 1 + len;
        }
    }
}

/// The port in the first `_remotepairing._tcp` SRV record of a DNS reply.
fn remotepairing_port(m: &[u8]) -> Option<u16> {
    let word = |at: usize| Some(u16::from_be_bytes([*m.get(at)?, *m.get(at + 1)?]));
    let mut i = 12;
    for _ in 0..word(4)? { i = dns_name(m, i)?.1 + 4 }
    for _ in 0..word(6)? as u32 + word(8)? as u32 + word(10)? as u32 {
        let (name, at) = dns_name(m, i)?;
        let rdata = at + 10;
        if word(at)? == 33 && name.contains("_remotepairing._tcp") { return word(rdata + 4) }
        i = rdata + word(at + 8)? as usize;
    }
    None
}

/// A PTR query for `_remotepairing._tcp.local` sent straight to the iPad's port 5353. The iPad answers
/// this even where multicast browsing finds nothing.
async fn unicast_lookup(ip: IpAddr, tries: u32, wait: Duration) -> Option<SocketAddr> {
    let mut q = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in ["_remotepairing", "_tcp", "local"] { q.push(label.len() as u8); q.extend_from_slice(label.as_bytes()) }
    q.extend_from_slice(&[0, 0, 12, 0, 1]); // Root, PTR, IN.
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let mut buf = [0u8; 4096];
    for _ in 0..tries {
        socket.send_to(&q, (ip, 5353)).await.ok()?;
        if let Ok(Ok((n, from))) = tokio::time::timeout(wait, socket.recv_from(&mut buf)).await
            && from.ip() == ip
            && let Some(port) = remotepairing_port(&buf[..n]) { return Some(SocketAddr::new(ip, port)) }
    }
    None
}

// ---- Pairing ----

/// Saved beside the record: the host name it was paired under. Not secret.
const HOST_FILE: &str = "rp-pairing-host.txt";
/// The spike's own working pairing (`ipad-input rppair`), used until Bridge pairs again.
const SPIKE_RECORD: &str = "rp-pairing-spike.plist";
const SPIKE_HOST: &str = "cross-the-platforms-input";

/// The identifier is a hash of the host name, and the iPad keeps the first key it saw for an
/// identifier. So every pairing needs a name it has never seen (spike, "Why the key differs").
pub fn pairing_host_name(seed: &str) -> String {
    let clean: String = seed.chars().filter(char::is_ascii_alphanumeric).take(16).collect();
    format!("{HOST_NAME}-{clean}")
}

/// Which record to use and the name to verify with. Only file names and the saved host name are read.
/// Bridge's own record with a saved name wins; then the spike's working record; then the old record
/// under the old fixed name, which the iPad rejects but which still says "pair again" plainly.
pub fn choose_record(dir: &Path) -> Option<(PathBuf, String)> {
    let record = dir.join(RECORD);
    let host = std::fs::read_to_string(dir.join(HOST_FILE)).ok().map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
    match host {
        Some(h) if record.exists() => Some((record, h)),
        _ if dir.join(SPIKE_RECORD).exists() => Some((dir.join(SPIKE_RECORD), SPIKE_HOST.to_string())),
        _ if record.exists() => Some((record, HOST_NAME.to_string())),
        _ => None,
    }
}

/// Wi-Fi: RemotePairing verify → TLS-PSK tunnel → RSD.
async fn connect_wifi(dir: &Path, record: &Path, host: &str) -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut rpf = RpPairingFile::read_from_file(record).await.map_err(|e| format!("pairing record: {e:?}"))?;
    let addr = discover(dir).await?;
    let stream = tokio::net::TcpStream::connect(addr).await.map_err(|e| format!("connect: {e}"))?;
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), host);
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pair verify: {e:?}"))?;
    let tunnel_port = rpc.create_tcp_listener().await.map_err(|e| format!("tunnel listener: {e:?}"))?;
    let tunnel_stream = tokio::net::TcpStream::connect(SocketAddr::new(addr.ip(), tunnel_port)).await.map_err(|e| format!("tunnel connect: {e}"))?;
    // Each input report is a small write; Nagle would hold it behind the previous unacknowledged one.
    tunnel_stream.set_nodelay(true).map_err(|e| format!("nodelay: {e}"))?;
    let keepalive = socket2::TcpKeepalive::new().with_time(KEEPALIVE_IDLE).with_interval(KEEPALIVE_INTERVAL);
    socket2::SockRef::from(&tunnel_stream).set_tcp_keepalive(&keepalive).map_err(|e| format!("keepalive: {e}"))?;
    let tunnel = connect_tls_psk_tunnel_native(tunnel_stream, rpc.encryption_key()).await.map_err(|e| format!("TLS-PSK tunnel: {e:?}"))?;
    let client_ip: IpAddr = tunnel.info.client_address.parse().map_err(|e| format!("{e}"))?;
    let server_ip: IpAddr = tunnel.info.server_address.parse().map_err(|e| format!("{e}"))?;
    let (mtu, rsd_port) = (tunnel.info.mtu as usize, tunnel.info.server_rsd_port);
    let mut adapter = idevice::tcp::adapter::Adapter::new(Box::new(tunnel.into_inner()), client_ip, server_ip);
    adapter.set_mss(mtu.min(8192) - 60);
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.map_err(|e| format!("RSD connect: {e}"))?;
    let handshake = RsdHandshake::new(stream).await.map_err(|e| format!("RSD handshake: {e:?}"))?;
    Ok((adapter, handshake))
}

/// A tunnel to the iPad: Wi-Fi with the pairing record first, then USB. The `bool` is `wifi`. With
/// neither, the Wi-Fi error wins, because it says what to fix.
pub async fn connect_tunnel(dir: &Path) -> Result<(AdapterHandle, RsdHandshake, bool), String> {
    let wifi_err = match choose_record(dir) {
        Some((record, host)) => match connect_wifi(dir, &record, &host).await {
            Ok((a, h)) => return Ok((a, h, true)),
            Err(e) => e,
        },
        None => "pairing record: NotFound".to_string(),
    };
    match connect_usb().await {
        Ok((a, h)) => Ok((a, h, false)),
        Err(_) => Err(wifi_err),
    }
}

/// Over USB: creates the RemotePairing record Wi-Fi needs (tap Trust on the iPad if asked), under a
/// name the iPad has never seen. The old record is replaced only once the new one is made.
pub async fn pair_usb(dir: &Path) -> Result<(), String> {
    let (mut adapter, handshake) = connect_usb().await?;
    let port = handshake.services.get("com.apple.internal.dt.coredevice.untrusted.tunnelservice").ok_or("untrusted tunnel service not found")?.port;
    let stream = adapter.connect(port).await.map_err(|e| format!("tunnel service connect: {e}"))?;
    let mut conn = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
    conn.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
    let _ = conn.recv_root().await;
    let seed = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let host = pairing_host_name(&seed);
    let mut rpf = RpPairingFile::generate(&host);
    let mut rpc = RemotePairingClient::new(conn, &host);
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pairing: {e:?}"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    rpf.write_to_file(dir.join(RECORD)).await.map_err(|e| format!("write pairing record: {e:?}"))?;
    std::fs::write(dir.join(HOST_FILE), &host).map_err(|e| format!("write pairing name: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn ipad_html_is_kept_only_when_there_is_some() {
        assert_eq!(rich_or_text("a".into(), Some("<b>a</b>".into())), Content::Rich { text: "a".into(), html: "<b>a</b>".into() });
        assert_eq!(rich_or_text("a".into(), Some("  ".into())), Content::Text("a".into()));
        assert_eq!(rich_or_text("a".into(), None), Content::Text("a".into()));
    }

    // Review Focus 4: every way the iPad can be unreachable gets a reason the owner can act on.
    #[test] fn missing_record_means_not_paired() { assert_eq!(classify("pairing record: NotFound"), IpadStatus::NotPaired); }
    #[test] fn service_not_found_means_ddi() {
        assert_eq!(classify("pasteboard (watch): ServiceNotFound"), IpadStatus::Unavailable("Developer disk image not mounted yet".into()));
    }
    #[test] fn not_on_network_is_explained() {
        assert!(matches!(classify("iPad's _remotepairing._tcp not found on the network"), IpadStatus::Unavailable(m) if m.contains("Wi-Fi")));
    }
    #[test] fn developer_mode_off_is_explained() {
        assert!(matches!(classify("core device proxy (Developer Mode on? DDI mounted?): InvalidService"), IpadStatus::Unavailable(m) if m.contains("Developer Mode")));
    }
    // Device check: iPad on Wi-Fi (or USB), record in `ipad_dir()`. Our own write comes back as a change push.
    #[tokio::test] #[ignore] async fn live_write_echoes_back() {
        let mut link = IpadLink::connect(&ipad_dir()).await.unwrap();
        eprintln!("connected, wifi={}", link.wifi);
        link.writer.write(&Content::Text("ctp test".into())).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(20), link.reader.next_change()).await.expect("no push within 20 s").unwrap();
        assert_eq!(got, Content::Text("ctp test".into()));
    }
    #[test] fn fresh_mount_means_connecting() { assert_eq!(classify(JUST_MOUNTED), IpadStatus::Connecting); }
    #[test] fn wifi_mount_refused_asks_for_usb() {
        assert!(matches!(classify("DDI mount over Wi-Fi failed: X"), IpadStatus::Unavailable(m) if m.contains("USB")));
    }
    #[test] fn ddi_files_names_what_is_missing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("ddi")).unwrap();
        std::fs::write(d.path().join("ddi/Image.dmg"), b"x").unwrap();
        let e = ddi_files(d.path()).unwrap_err();
        assert!(e.contains("Image.dmg.trustcache"), "{e}");
    }
    #[test] fn oversized_promises_are_not_fetched() { // Final review I6
        assert!(worth_resolving(Some(1000)));
        assert!(worth_resolving(None));
        assert!(!worth_resolving(Some(bridge_core::clipboard::MAX_ITEM_BYTES as i64 + 1)));
    }
    #[test] fn pushes_up_to_the_baseline_are_stale() { // Final review I8
        assert!(!is_fresh(Some(7), Some(7)));
        assert!(!is_fresh(Some(6), Some(7)));
        assert!(is_fresh(Some(8), Some(7)));
        assert!(is_fresh(None, Some(7)), "no counter: can't tell, keep it");
        assert!(is_fresh(Some(1), None));
    }
    #[test] fn each_pairing_gets_a_new_name() {
        let (a, b) = (pairing_host_name("a"), pairing_host_name("b"));
        assert_ne!(a, b);
        assert!(a.starts_with("cross-the-platforms") && b.starts_with("cross-the-platforms"));
        assert_ne!(a, HOST_NAME, "never the old fixed name the iPad already holds a key for");
    }
    #[test] fn saved_address_round_trips() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load_last_ip(d.path()), None);
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        save_last_ip(d.path(), ip).unwrap();
        assert_eq!(load_last_ip(d.path()), Some(ip));
    }
    #[test] fn bad_signature_means_pair_again() {
        assert_eq!(classify("pair verify: early eof"), IpadStatus::NotPaired);
        assert_eq!(classify("pair verify: PairVerifyFailed"), IpadStatus::NotPaired);
    }
    #[test] fn record_choice_prefers_a_named_pairing() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(choose_record(d.path()), None);
        std::fs::write(d.path().join(RECORD), b"x").unwrap();
        assert_eq!(choose_record(d.path()).unwrap().1, HOST_NAME, "old record keeps the old name");
        std::fs::write(d.path().join(SPIKE_RECORD), b"x").unwrap();
        assert_eq!(choose_record(d.path()).unwrap(), (d.path().join(SPIKE_RECORD), SPIKE_HOST.to_string()));
        std::fs::write(d.path().join(HOST_FILE), "cross-the-platforms-abc\n").unwrap();
        assert_eq!(choose_record(d.path()).unwrap(), (d.path().join(RECORD), "cross-the-platforms-abc".to_string()));
    }
    #[test] fn sweep_covers_the_subnet_nearest_first() {
        let hosts = subnet_hosts(Ipv4Addr::new(192, 168, 1, 3));
        assert_eq!(hosts.len(), 253);
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 3)));
        assert_eq!(&hosts[..2], &[Ipv4Addr::new(192, 168, 1, 2), Ipv4Addr::new(192, 168, 1, 4)]);
        assert!(hosts.iter().all(|h| h.octets()[..3] == [192, 168, 1] && h.octets()[3] != 0 && h.octets()[3] != 255));
    }
    #[test] fn srv_port_from_a_compressed_reply() { // Ported from the spike.
        let mut m = vec![0, 0, 0x84, 0, 0, 1, 0, 1, 0, 0, 0, 2];
        m.extend_from_slice(b"\x0e_remotepairing\x04_tcp\x05local\0\0\x0c\0\x01");
        m.extend_from_slice(&[0xC0, 12, 0, 12, 0, 1, 0, 0, 0, 9, 0, 2, 0xC0, 12]);
        m.extend_from_slice(b"\x0e_apple-mobdev2\x04_tcp\x05local\0\0\x21\0\x01\0\0\0\x09\0\x08\0\0\0\0\x7e\xf2\xc0\x0c");
        m.extend_from_slice(&[1, b'i', 0xC0, 12, 0, 33, 0, 1, 0, 0, 0, 9, 0, 8, 0, 0, 0, 0, 0xC0, 0, 0xC0, 12]);
        assert_eq!(remotepairing_port(&m), Some(49152));
        assert_eq!(remotepairing_port(&m[..40]), None);
    }
    #[test] fn anything_else_is_passed_through_short() { assert!(matches!(classify("weird"), IpadStatus::Unavailable(_))); }
}
