//! Cross the Platforms — Windows hub. Lives in the tray and hosts the input engine.
#![windows_subsystem = "windows"]

mod commands;
mod tray;

use std::sync::Mutex;

use bridge_core::settings::Settings;
use ctp_windows::engine::{self, EngineHandle, Status, settings_path};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub struct AppState {
    pub engine: Mutex<Option<EngineHandle>>,
    pub status: Mutex<Status>,
    pub settings: Mutex<Settings>,
}

fn main() {
    let settings = Settings::load(&settings_path());
    tauri::Builder::default()
        // A second launch just opens the existing settings window.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(AppState { engine: Mutex::new(None), status: Mutex::new(Status::default()), settings: Mutex::new(settings.clone()) })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings, commands::save_settings, commands::get_status, commands::list_monitors, commands::switch_to
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
            Ok(())
        })
        // Closing the window hides it; the app keeps running in the tray.
        .on_window_event(|w, e| if let WindowEvent::CloseRequested { api, .. } = e { api.prevent_close(); let _ = w.hide(); })
        .run(tauri::generate_context!())
        .expect("error while running Cross the Platforms");
}
