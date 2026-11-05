//! Commands the settings window calls (see `src/lib/api.ts`).

use std::path::PathBuf;

use bridge_core::{cliphistory::Entry, clipboard::Content, identity::Peer, layout::Monitor, settings::Settings};
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
pub fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    let settings = settings.keeping_owned(&state.settings.lock().unwrap());
    settings.save(&settings_path())?;
    let autostart = app.autolaunch();
    let _ = if settings.start_with_windows { autostart.enable() } else { autostart.disable() };
    if let Some(e) = state.engine.lock().unwrap().as_ref() { e.send(Command::Reload(Box::new(settings.clone()))); }
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
pub fn reachable_peers(app: AppHandle) -> Result<Vec<String>, String> { Ok(transfer(&app)?.reachable()) }

#[tauri::command]
pub fn pair_ipad(state: State<AppState>) {
    if let Some(c) = state.clip.lock().unwrap().as_ref() { c.send(ClipCommand::PairIpad); }
}

#[derive(serde::Serialize)]
pub struct Invite { qr: String, expires_in_s: u64 }

fn transfer(app: &AppHandle) -> Result<std::sync::Arc<ctp_windows::transfer::TransferHandle>, String> {
    crate::transfer(app).ok_or_else(|| "file transfer isn't running (see Diagnostics)".into())
}

/// One-step iPad pairing: a fresh files pairing code goes straight onto the iPad's clipboard (over the
/// USB pairing's link), for Bridge on the iPad to paste. The answer arrives as a clipboard status notice.
#[tauri::command]
pub fn invite_to_ipad(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let qr = transfer(&app)?.invite().to_qr_text();
    match state.clip.lock().unwrap().as_ref() {
        Some(c) => { c.send(ClipCommand::InviteToIpad(qr)); Ok(()) }
        None => Err("clipboard sync isn't running".into()),
    }
}

/// A fresh one-time pairing code (valid 5 minutes) as QR text.
#[tauri::command]
pub fn pair_invite(app: AppHandle) -> Result<Invite, String> { Ok(Invite { qr: transfer(&app)?.invite().to_qr_text(), expires_in_s: 300 }) }

#[tauri::command]
pub fn paired_peers(app: AppHandle) -> Result<Vec<Peer>, String> { Ok(transfer(&app)?.peers()) }

/// Files waiting for a closed iPad, by peer id; the page asks on load because the outbox survives a restart.
#[tauri::command]
pub fn queued_files(app: AppHandle) -> Result<std::collections::HashMap<String, usize>, String> { Ok(transfer(&app)?.waiting().into_iter().collect()) }

#[tauri::command]
pub fn forget_peer(app: AppHandle, id: String) -> Result<(), String> {
    transfer(&app)?.forget(&id);
    crate::peers_changed(&app);
    Ok(())
}

/// Starts in the background; progress and the outcome arrive as `transfer` events.
#[tauri::command]
pub fn send_files(app: AppHandle, peer_id: String, paths: Vec<String>) -> Result<(), String> {
    transfer(&app)?.send_files(peer_id, paths.into_iter().map(PathBuf::from).collect());
    Ok(())
}

#[tauri::command]
pub fn receive_dir(state: State<AppState>) -> String { crate::receive_dir(&state.settings.lock().unwrap()).to_string_lossy().into_owned() }

#[tauri::command]
pub fn set_receive_dir(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap().clone();
    settings.receive_dir = Some(path.clone());
    settings.save(&settings_path())?;
    *state.settings.lock().unwrap() = settings;
    transfer(&app)?.set_receive_dir(PathBuf::from(path));
    Ok(())
}

#[tauri::command]
pub fn open_receive_dir(state: State<AppState>) -> Result<(), String> {
    let dir = crate::receive_dir(&state.settings.lock().unwrap());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_bluetooth_settings() -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg("ms-settings:bluetooth").spawn().map(|_| ()).map_err(|e| e.to_string())
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

#[derive(serde::Serialize)]
pub struct HistoryList { enabled: bool, entries: Vec<Entry> }

#[tauri::command]
pub fn history_list(state: State<AppState>, query: String) -> HistoryList {
    let h = state.history.lock().unwrap();
    HistoryList { enabled: h.enabled(), entries: h.search(&query).into_iter().cloned().collect() }
}

/// Puts the item back on the clipboard; the clipboard service then syncs it like any copy.
#[tauri::command]
pub fn history_copy(state: State<AppState>, id: u64) -> Result<(), String> {
    let c: Content = state.history.lock().unwrap().content(id).ok_or("That item is gone.")?;
    ctp_windows::clipboard::write(&c)
}

#[tauri::command]
pub fn history_pin(state: State<AppState>, id: u64, pinned: bool) { state.history.lock().unwrap().pin(id, pinned); }

#[tauri::command]
pub fn history_delete(state: State<AppState>, id: u64) { state.history.lock().unwrap().delete(id); }

#[tauri::command]
pub fn history_clear(state: State<AppState>) { state.history.lock().unwrap().clear(); }

#[tauri::command]
pub fn history_enable(state: State<AppState>, on: bool) { state.history.lock().unwrap().set_enabled(on); }
