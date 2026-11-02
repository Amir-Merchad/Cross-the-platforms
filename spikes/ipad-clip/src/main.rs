//! Spike 1: two-way text clipboard sync Windows <-> iPad over the CoreDevice pasteboard service.
//! Throwaway. Connection flow mirrors idevice/tools/src/coredevice_pasteboard.rs.
//! Logs lengths only, never contents.

use std::{sync::{Arc, Mutex}, time::Duration};

use idevice::{
    IdeviceService, RsdService,
    core_device::{GENERAL_PASTEBOARD, PasteboardServiceClient},
    core_device_proxy::CoreDeviceProxy,
    rsd::RsdHandshake,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};

#[tokio::main]
async fn main() {
    let mut mux = UsbmuxdConnection::default().await.expect("usbmuxd not reachable (install Apple Devices; or set USBMUXD_SOCKET_ADDRESS=127.0.0.1:27015)");
    let dev = mux.get_devices().await.expect("device list").into_iter().next().expect("no iPad connected");
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), "ipad-clip-spike");

    let proxy = CoreDeviceProxy::connect(&provider).await.expect("no core device proxy (Developer Mode on? DDI mounted?)");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().expect("no software tunnel").to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.expect("RSD handshake");
    let mut watcher = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake).await.expect("pasteboard (watch)");
    let mut setter = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake).await.expect("pasteboard (set)");
    eprintln!("connected; syncing text both ways (Ctrl+C to stop)");

    // Last text either side wrote, so an item never bounces back.
    let last: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let last_w = last.clone();
    tokio::spawn(async move {
        watcher.set_change_notifications(true, GENERAL_PASTEBOARD, None).await.expect("AUTONOTIFY");
        let mut win = arboard::Clipboard::new().expect("windows clipboard");
        loop {
            let snap = match watcher.recv_push().await { Ok(s) => s, Err(e) => { eprintln!("push stream ended: {e:?}"); std::process::exit(1) } };
            let Some(text) = snap.text().filter(|t| !t.is_empty()) else { continue };
            let mut l = last_w.lock().unwrap();
            if l.as_deref() == Some(&text) { continue }
            match win.set_text(text.clone()) {
                Ok(()) => eprintln!("iPad -> Windows: {} bytes", text.len()),
                Err(e) => eprintln!("Windows set failed: {e}"),
            }
            *l = Some(text);
        }
    });

    let mut win = arboard::Clipboard::new().expect("windows clipboard");
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
        match setter.set_text(&text, GENERAL_PASTEBOARD).await {
            Ok(()) => eprintln!("Windows -> iPad: {} bytes", text.len()),
            Err(e) => { eprintln!("iPad set failed: {e:?}"); *last.lock().unwrap() = prev }
        }
    }
}
