//! Commands the settings window calls (see `src/lib/api.ts`).

use bridge_core::{layout::Monitor, settings::Settings};
use ctp_windows::engine::{Command, Status, settings_path};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::AppState;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings { state.settings.lock().unwrap().clone() }

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    settings.save(&settings_path())?;
    let autostart = app.autolaunch();
    let _ = if settings.start_with_windows { autostart.enable() } else { autostart.disable() };
    if let Some(e) = state.engine.lock().unwrap().as_ref() { e.send(Command::Reload(settings.clone())); }
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status { state.status.lock().unwrap().clone() }

#[tauri::command]
pub fn list_monitors() -> Vec<Monitor> { ctp_windows::monitors::list() }

#[tauri::command]
pub fn switch_to(state: State<AppState>, device_id: Option<String>) {
    if let Some(e) = state.engine.lock().unwrap().as_ref() { e.send(Command::SwitchTo(device_id)); }
}
