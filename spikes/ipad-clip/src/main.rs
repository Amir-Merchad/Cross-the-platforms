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

type Last = Arc<Mutex<Option<String>>>;

/// Reconnects forever: the iPad closes the connection when it locks or sleeps.
#[tokio::main]
async fn main() {
    // Last text either side wrote, so an item never bounces back (kept across reconnects).
    let last: Last = Arc::new(Mutex::new(None));
    loop {
        if let Err(e) = session(&last).await { eprintln!("disconnected: {e}; retrying in 2 s"); }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn session(last: &Last) -> Result<(), String> {
    let mut mux = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd not reachable (Apple Devices installed?): {e:?}"))?;
    let dev = mux.get_devices().await.map_err(|e| format!("device list: {e:?}"))?.into_iter().next().ok_or("no iPad connected")?;
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), "ipad-clip-spike");

    let proxy = CoreDeviceProxy::connect(&provider).await.map_err(|e| format!("core device proxy (Developer Mode on? DDI mounted?): {e:?}"))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().map_err(|e| format!("software tunnel: {e:?}"))?.to_async_handle();
    let stream = adapter.connect(rsd_port).await.map_err(|e| format!("RSD connect: {e:?}"))?;
    let mut handshake = RsdHandshake::new(stream).await.map_err(|e| format!("RSD handshake: {e:?}"))?;
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
