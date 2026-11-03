//! iPad pasteboard over the CoreDevice pasteboard service, no app on the iPad. Wi-Fi via RemotePairing
//! (record made once over USB), USB as fallback. Ported from `spikes/ipad-clip`. Errors never carry
//! clipboard contents.

use std::{net::SocketAddr, path::{Path, PathBuf}, time::Duration};

use bridge_core::clipboard::{Content, codec};
use idevice::{
    IdeviceService, ReadWrite, RemoteXpcClient, RsdService,
    core_device::{GENERAL_PASTEBOARD, PasteboardServiceClient},
    core_device_proxy::CoreDeviceProxy,
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

/// `%APPDATA%\CrossThePlatforms\ipad` — pairing record and developer disk image (never in git).
pub fn ipad_dir() -> PathBuf { crate::engine::settings_path().with_file_name("ipad") }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum IpadStatus { NotPaired, Connecting, Connected { wifi: bool }, Unavailable(String) }

/// Turns a connect/session error into something the owner can act on.
pub fn classify(err: &str) -> IpadStatus {
    let reason = if err.contains("pairing record") && err.contains("NotFound") {
        return IpadStatus::NotPaired;
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

pub struct IpadLink {
    // Keeps the tunnel alive for the two service connections.
    _adapter: AdapterHandle,
    watcher: Pasteboard,
    setter: Pasteboard,
    wifi: bool,
}

impl IpadLink {
    /// Wi-Fi first (needs the pairing record), then USB.
    pub async fn connect(dir: &Path) -> Result<IpadLink, String> {
        let record = dir.join(RECORD);
        let wifi_err = if record.exists() {
            match connect_wifi(&record).await {
                Ok((a, h)) => return Self::open(a, h, true).await,
                Err(e) => e,
            }
        } else {
            "pairing record: NotFound".to_string()
        };
        match connect_usb().await {
            Ok((a, h)) => Self::open(a, h, false).await,
            Err(_) => Err(wifi_err),
        }
    }

    async fn open(mut adapter: AdapterHandle, mut handshake: RsdHandshake, wifi: bool) -> Result<IpadLink, String> {
        let mut watcher = Pasteboard::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (watch): {e:?}"))?;
        let setter = Pasteboard::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (set): {e:?}"))?;
        watcher.set_change_notifications(true, GENERAL_PASTEBOARD, None).await.map_err(|e| format!("AUTONOTIFY: {e:?}"))?;
        Ok(IpadLink { _adapter: adapter, watcher, setter, wifi })
    }

    pub fn is_wifi(&self) -> bool { self.wifi }

    /// Waits for the next iPad copy that has text or a readable image.
    pub async fn next_change(&mut self) -> Result<Content, String> {
        loop {
            let snap = self.watcher.recv_push().await.map_err(|e| format!("push stream ended: {e:?}"))?;
            if let Some(t) = snap.text().filter(|t| !t.is_empty()) { return Ok(Content::Text(t)) }
            let promised = snap.promised_items();
            let Some(p) = IMAGE_UTIS.iter().find_map(|u| promised.iter().find(|p| p.uti == *u)) else { continue };
            let bytes = self.setter.resolve(GENERAL_PASTEBOARD, p.item_index, &p.uti).await.map_err(|e| format!("resolve image: {e:?}"))?;
            match bytes.map(|b| codec::decode(&b)) {
                Some(Ok(img)) => return Ok(Content::Image(img)),
                Some(Err(e)) => crate::diag::log(format_args!("iPad image not decodable ({}): {e}", p.uti)),
                None => {}
            }
        }
    }

    pub async fn write(&mut self, c: &Content) -> Result<(), String> {
        match c {
            Content::Text(t) => self.setter.set_text(t, GENERAL_PASTEBOARD).await,
            Content::Image(i) => self.setter.set_image(&codec::encode_png(i)?, "public.png", GENERAL_PASTEBOARD).await,
        }
        .map_err(|e| format!("iPad write: {e:?}"))
    }
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

/// Finds the iPad's `_remotepairing._tcp` service (its port changes) via mDNS.
async fn discover() -> Result<SocketAddr, String> {
    tokio::task::spawn_blocking(|| {
        let mdns = mdns_sd::ServiceDaemon::new().map_err(|e| format!("mDNS: {e}"))?;
        let rx = mdns.browse("_remotepairing._tcp.local.").map_err(|e| format!("mDNS browse: {e}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
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
        found.ok_or_else(|| "iPad's _remotepairing._tcp not found on the network".to_string())
    }).await.map_err(|e| e.to_string())?
}

/// Wi-Fi: RemotePairing verify → TLS-PSK tunnel → RSD.
async fn connect_wifi(record: &Path) -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut rpf = RpPairingFile::read_from_file(record).await.map_err(|e| format!("pairing record: {e:?}"))?;
    let addr = discover().await?;
    let stream = tokio::net::TcpStream::connect(addr).await.map_err(|e| format!("connect: {e}"))?;
    let mut rpc = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_NAME);
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pair verify: {e:?}"))?;
    let tunnel_port = rpc.create_tcp_listener().await.map_err(|e| format!("tunnel listener: {e:?}"))?;
    let tunnel_stream = tokio::net::TcpStream::connect(SocketAddr::new(addr.ip(), tunnel_port)).await.map_err(|e| format!("tunnel connect: {e}"))?;
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

/// One-time over USB: creates the RemotePairing record Wi-Fi needs (confirm on the iPad if asked).
pub async fn pair_usb(dir: &Path) -> Result<(), String> {
    let (mut adapter, handshake) = connect_usb().await?;
    let port = handshake.services.get("com.apple.internal.dt.coredevice.untrusted.tunnelservice").ok_or("untrusted tunnel service not found")?.port;
    let stream = adapter.connect(port).await.map_err(|e| format!("tunnel service connect: {e}"))?;
    let mut conn = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
    conn.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
    let _ = conn.recv_root().await;
    let mut rpf = RpPairingFile::generate(HOST_NAME);
    let mut rpc = RemotePairingClient::new(conn, HOST_NAME);
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pairing: {e:?}"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    rpf.write_to_file(dir.join(RECORD)).await.map_err(|e| format!("write pairing record: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        eprintln!("connected, wifi={}", link.is_wifi());
        link.write(&Content::Text("ctp test".into())).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(20), link.next_change()).await.expect("no push within 20 s").unwrap();
        assert_eq!(got, Content::Text("ctp test".into()));
    }
    #[test] fn anything_else_is_passed_through_short() { assert!(matches!(classify("weird"), IpadStatus::Unavailable(_))); }
}
