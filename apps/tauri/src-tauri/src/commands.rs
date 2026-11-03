//! Commands the settings window calls (see `src/lib/api.ts`).

use bridge_core::{layout::Monitor, settings::Settings};
use ctp_windows::{
    clipsync::{ClipCommand, ClipStatus},
    engine::{Command, Status, settings_path},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::AppState;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings { state.settings.lock().unwrap().clone() }

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<AppState>, mut settings: Settings) -> Result<(), String> {
    // The clipboard toggle is owned by `set_clipboard_sync` (tray + Clipboard tab): an older draft can't undo it.
    settings.clipboard_sync = state.settings.lock().unwrap().clipboard_sync;
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

#[tauri::command]
pub fn get_clip_status(state: State<AppState>) -> ClipStatus { state.clip_status.lock().unwrap().clone() }

#[tauri::command]
pub fn set_clipboard_sync(app: AppHandle, on: bool) -> Result<(), String> { set_clipboard_sync_now(&app, on) }

#[tauri::command]
pub fn pair_ipad(state: State<AppState>) {
    if let Some(c) = state.clip.lock().unwrap().as_ref() { c.send(ClipCommand::PairIpad); }
}

/// Saves the toggle, tells the clipboard service and refreshes the tray check mark.
pub fn set_clipboard_sync_now(app: &AppHandle, on: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut settings = state.settings.lock().unwrap().clone();
    settings.clipboard_sync = on;
    settings.save(&settings_path())?;
    *state.settings.lock().unwrap() = settings;
    if let Some(c) = state.clip.lock().unwrap().as_ref() { c.send(ClipCommand::SetEnabled(on)); }
    let status = state.status.lock().unwrap().clone();
    crate::tray::refresh(app, &status);
    Ok(())
}
