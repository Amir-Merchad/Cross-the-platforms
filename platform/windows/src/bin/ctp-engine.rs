//! Headless engine runner for testing without the UI. Uses `%APPDATA%\CrossThePlatforms\settings.json`
//! (or the path given as the first argument). Ctrl+C to stop.

fn main() {
    let path = std::env::args().nth(1).map(Into::into).unwrap_or_else(ctp_windows::engine::settings_path);
    let settings = bridge_core::settings::Settings::load(&path);
    eprintln!("settings: {} ({} attachments)", path.display(), settings.attachments.len());
    for m in ctp_windows::monitors::list() { eprintln!("monitor {} {:?}", m.id, m.rect); }
    let engine = ctp_windows::engine::start(settings, |s| eprintln!("status: {}", serde_json::to_string(&s).unwrap_or_default()));
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = ctrlc_like(tx);
    let _ = rx.recv();
    drop(engine); // input back to Windows, device released, advertising stopped
}

/// Waits for Enter as a portable "stop" (Ctrl+C would kill the process before the engine can clean up).
fn ctrlc_like(tx: std::sync::mpsc::Sender<()>) -> std::thread::JoinHandle<()> {
    eprintln!("press Enter to stop cleanly");
    std::thread::spawn(move || { let mut s = String::new(); let _ = std::io::stdin().read_line(&mut s); let _ = tx.send(()); })
}
