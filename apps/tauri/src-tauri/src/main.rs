//! Cross the Platforms — Windows hub. Lives in the tray and hosts the input engine.
#![windows_subsystem = "windows"]

mod commands;
mod tray;

use std::{path::PathBuf, sync::{Arc, Mutex}};

use bridge_core::{identity::PeerKind, node::Event, settings::Settings};
use ctp_windows::{
    clipsync::{self, ClipCommand, ClipHandle, ClipStatus},
    diag,
    engine::{self, EngineHandle, Status, settings_path},
    sendto,
    transfer::{self, TransferHandle},
};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub struct AppState {
    pub engine: Mutex<Option<EngineHandle>>,
    pub status: Mutex<Status>,
    pub settings: Mutex<Settings>,
    pub clip: Mutex<Option<ClipHandle>>,
    pub clip_status: Mutex<ClipStatus>,
    pub transfer: Mutex<Option<Arc<TransferHandle>>>,
}

pub fn receive_dir(s: &Settings) -> PathBuf { s.receive_dir.clone().map(PathBuf::from).unwrap_or_else(transfer::default_receive_dir) }

pub fn transfer(app: &AppHandle) -> Option<Arc<TransferHandle>> { app.state::<AppState>().transfer.lock().unwrap().clone() }

/// After pairing or forgetting: phones for clipboard text, and the Explorer "Send to" entries.
pub fn peers_changed(app: &AppHandle) {
    let Some(t) = transfer(app) else { return };
    let peers = t.peers();
    let phones = peers.iter().filter(|p| p.kind == PeerKind::Android).map(|p| p.id.clone()).collect();
    if let Some(c) = app.state::<AppState>().clip.lock().unwrap().as_ref() { c.send(ClipCommand::Phones(phones)); }
    std::thread::spawn(move || {
        let Ok(exe) = std::env::current_exe() else { return };
        if let Err(e) = sendto::refresh(&peers, &exe) { diag::log(format_args!("Send to shortcuts: {e}")); }
    });
}

/// `ctp-app.exe --send-to <id> <files…>` (Explorer "Send to"), at startup or from a second launch.
fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    let Some((id, files)) = sendto::parse_send_to(args) else { return false };
    if let Some(t) = transfer(app) { t.send_files(id, files); }
    tray::show_window(app);
    true
}

fn main() {
    let settings = Settings::load(&settings_path());
    tauri::Builder::default()
        // A second launch opens the existing window, or hands its "Send to" files to us.
        .plugin(tauri_plugin_single_instance::init(|app, args, _| { if !handle_args(app, &args) { tray::show_window(app) } }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            engine: Mutex::new(None), status: Mutex::new(Status::default()), settings: Mutex::new(settings.clone()),
            clip: Mutex::new(None), clip_status: Mutex::new(ClipStatus::default()), transfer: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings, commands::save_settings, commands::get_status, commands::list_monitors, commands::switch_to,
            commands::get_clip_status, commands::set_clipboard_sync, commands::pair_ipad,
            commands::pair_invite, commands::paired_peers, commands::forget_peer, commands::send_files,
            commands::receive_dir, commands::set_receive_dir, commands::open_receive_dir
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::create(&handle)?;
            if settings.start_with_windows { let _ = handle.autolaunch().enable(); }
            let on_status = {
                let handle = handle.clone();
                // Runs on the engine thread: never block on the main thread here (Quit joins the engine there).
                move |s: Status| {
                    *handle.state::<AppState>().status.lock().unwrap() = s.clone();
                    let (h, tray_status) = (handle.clone(), s.clone());
                    let _ = handle.run_on_main_thread(move || tray::refresh(&h, &tray_status));
                    let _ = handle.emit("status", s);
                }
            };
            *app.state::<AppState>().engine.lock().unwrap() = Some(engine::start(settings.clone(), on_status));

            let on_transfer = {
                let handle = handle.clone();
                move |e: Event| match e {
                    // Clipboard text from a phone goes to the clipboard service, never to the window.
                    Event::Clip { from, text } => {
                        if let Some(c) = handle.state::<AppState>().clip.lock().unwrap().as_ref() { c.send(ClipCommand::FromPhone { id: from, text }); }
                    }
                    e => {
                        if matches!(e, Event::Paired(_)) { peers_changed(&handle); }
                        let _ = handle.emit("transfer", e);
                    }
                }
            };
            match TransferHandle::start(receive_dir(&settings), on_transfer) {
                Ok(t) => *app.state::<AppState>().transfer.lock().unwrap() = Some(Arc::new(t)),
                Err(e) => diag::log(format_args!("transfer service didn't start: {e}")),
            }

            let on_clip = {
                let handle = handle.clone();
                move |s: ClipStatus| {
                    *handle.state::<AppState>().clip_status.lock().unwrap() = s.clone();
                    let _ = handle.emit("clip-status", s);
                }
            };
            let to_phone: clipsync::ToPhone = {
                let handle = handle.clone();
                Box::new(move |id, text| if let Some(t) = transfer(&handle) { t.send_clip(id, text) })
            };
            *app.state::<AppState>().clip.lock().unwrap() = Some(clipsync::start(settings.clipboard_sync, to_phone, on_clip));
            peers_changed(&handle);
            handle_args(&handle, &std::env::args().collect::<Vec<_>>());
            Ok(())
        })
        // Closing the window hides it; the app keeps running in the tray.
        .on_window_event(|w, e| if let WindowEvent::CloseRequested { api, .. } = e { api.prevent_close(); let _ = w.hide(); })
        .run(tauri::generate_context!())
        .expect("error while running Cross the Platforms");
}
