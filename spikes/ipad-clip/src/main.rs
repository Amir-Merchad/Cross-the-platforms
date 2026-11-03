//! Spike 1: two-way text clipboard sync Windows <-> iPad over the CoreDevice pasteboard service.
//! Throwaway. USB flow mirrors idevice/tools/src/coredevice_pasteboard.rs; the Wi-Fi flow mirrors
//! idevice/ffi/src/tunnel_provider.rs (RemotePairing -> TLS-PSK tunnel -> RSD).
//! Logs lengths only, never contents.
//!
//!   ipad-clip                       USB (Apple Devices' usbmux)
//!   ipad-clip --rp-pair <file>      one-time, over USB: create a RemotePairing record for Wi-Fi
//!   ipad-clip --wifi <file>         Wi-Fi: find the iPad via mDNS, verify that record, sync

use std::{net::SocketAddr, sync::{Arc, Mutex}, time::Duration};

use idevice::{
    IdeviceService, RemoteXpcClient, RsdService,
    core_device::{GENERAL_PASTEBOARD, PasteboardServiceClient},
    core_device_proxy::CoreDeviceProxy,
    remote_pairing::{RemotePairingClient, RpPairingFile, RpPairingSocket, connect_tls_psk_tunnel_native},
    rsd::RsdHandshake,
    tcp::handle::AdapterHandle,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};

type Last = Arc<Mutex<Option<String>>>;
const HOST_NAME: &str = "cross-the-platforms";

enum Mode { Usb, Wifi(String) }

/// Reconnects forever: the iPad can drop the connection when it locks, sleeps or leaves Wi-Fi.
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned());
    if let Some(file) = flag("--rp-pair") {
        match rp_pair_usb(&file).await {
            Ok(()) => eprintln!("RemotePairing record saved to {file} (keep it out of git)"),
            Err(e) => eprintln!("pairing failed: {e}"),
        }
        return;
    }
    let mode = flag("--wifi").map(Mode::Wifi).unwrap_or(Mode::Usb);
    // Last text either side wrote, so an item never bounces back (kept across reconnects).
    let last: Last = Arc::new(Mutex::new(None));
    loop {
        if let Err(e) = session(&last, &mode).await { eprintln!("disconnected: {e}; retrying in 2 s"); }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// USB: usbmux -> CoreDeviceProxy tunnel -> RSD.
async fn connect_usb() -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd not reachable (Apple Devices installed?): {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad connected")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), "ipad-clip-spike");
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
        found.ok_or_else(|| "iPad's _remotepairing._tcp not found on the network (same Wi-Fi? iPad awake?)".to_string())
    }).await.map_err(|e| e.to_string())?
}

/// Wi-Fi: RemotePairing verify -> TLS-PSK tunnel -> RSD (same steps as idevice's FFI tunnel provider).
async fn connect_wifi(file: &str) -> Result<(AdapterHandle, RsdHandshake), String> {
    let mut rpf = RpPairingFile::read_from_file(file).await.map_err(|e| format!("pairing record {file}: {e:?} (run --rp-pair first)"))?;
    let addr = discover().await?;
    eprintln!("found iPad at {addr}");
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

/// One-time over USB: RemotePairing through the untrusted tunnel service (idevice's `rppairing pair`).
async fn rp_pair_usb(file: &str) -> Result<(), String> {
    let (mut adapter, handshake) = connect_usb().await?;
    let port = handshake.services.get("com.apple.internal.dt.coredevice.untrusted.tunnelservice").ok_or("untrusted tunnel service not found")?.port;
    let stream = adapter.connect(port).await.map_err(|e| format!("tunnel service connect: {e}"))?;
    let mut conn = RemoteXpcClient::new(stream).await.map_err(|e| format!("xpc: {e:?}"))?;
    conn.do_handshake().await.map_err(|e| format!("xpc handshake: {e:?}"))?;
    let _ = conn.recv_root().await;
    let mut rpf = RpPairingFile::generate(HOST_NAME);
    let mut rpc = RemotePairingClient::new(conn, HOST_NAME);
    eprintln!("pairing... confirm on the iPad if asked");
    rpc.connect(&mut rpf, async || "000000".to_string()).await.map_err(|e| format!("pairing: {e:?}"))?;
    rpf.write_to_file(file).await.map_err(|e| format!("write {file}: {e:?}"))
}

async fn session(last: &Last, mode: &Mode) -> Result<(), String> {
    let (mut adapter, mut handshake) = match mode { Mode::Usb => connect_usb().await?, Mode::Wifi(f) => connect_wifi(f).await? };
    let mut watcher = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (watch): {e:?}"))?;
    let mut setter = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake).await.map_err(|e| format!("pasteboard (set): {e:?}"))?;
    watcher.set_change_notifications(true, GENERAL_PASTEBOARD, None).await.map_err(|e| format!("AUTONOTIFY: {e:?}"))?;
    eprintln!("connected; syncing text both ways (Ctrl+C to stop)");

    let ipad_to_windows = async {
        let mut win = arboard::Clipboard::new().map_err(|e| format!("windows clipboard: {e}"))?;
        loop {
            let snap = watcher.recv_push().await.map_err(|e| format!("push stream ended: {e:?}"))?;
            let Some(text) = snap.text().filter(|t| !t.is_empty()) else { continue };
            let mut l = last.lock().unwrap();
            if l.as_deref() == Some(&text) { continue }
            match win.set_text(text.clone()) {
                Ok(()) => eprintln!("iPad -> Windows: {} bytes", text.len()),
                Err(e) => eprintln!("Windows set failed: {e}"),
            }
            *l = Some(text);
        }
    };

    let windows_to_ipad = async {
        let mut win = arboard::Clipboard::new().map_err(|e| format!("windows clipboard: {e}"))?;
        let mut tick = tokio::time::interval(Duration::from_millis(300));
        loop {
            tick.tick().await;
            let Ok(text) = win.get_text() else { continue };
            if text.is_empty() { continue }
            let changed = { let l = last.lock().unwrap(); l.as_deref() != Some(&text) };
            if !changed { continue }
            // Mark synced before sending: the iPad's echo push can arrive before set_text returns,
            // and writing it back would replace Windows' rich formats (HTML/RTF) with plain text.
            let prev = last.lock().unwrap().replace(text.clone());
            if let Err(e) = setter.set_text(&text, GENERAL_PASTEBOARD).await {
                *last.lock().unwrap() = prev;
                return Err(format!("iPad set failed: {e:?}"));
            }
            eprintln!("Windows -> iPad: {} bytes", text.len());
        }
    };

    // Either direction failing means the link is gone: drop both and reconnect.
    tokio::select! {
        r = ipad_to_windows => r,
        r = windows_to_ipad => r,
    }
}
