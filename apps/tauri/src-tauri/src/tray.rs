//! Tray icon and menu: status, switch per connected device, back to Windows, settings, quit.

use ctp_windows::engine::{Command, Status};
use tauri::{
    AppHandle, Manager,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::AppState;

pub const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip(format!("Bridge: {}", status_line(&Status::default())))
        .menu(&build_menu(app, &Status::default())?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Rebuilds the menu from the latest engine status.
pub fn refresh(app: &AppHandle, status: &Status) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, status)) {
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(format!("Bridge: {}", status_line(status))));
    }
}

fn status_line(s: &Status) -> String {
    let name = |id: &str| s.hosts.iter().find(|h| h.id == id).and_then(|h| h.name.clone()).unwrap_or_else(|| id.to_string());
    match (&s.active, &s.notice) {
        (Some(id), _) => format!("Controlling {}", name(id)),
        (None, Some(n)) => n.clone(),
        (None, None) if !s.advertising => "Starting Bluetooth…".into(),
        (None, None) => format!("{} device(s) connected", s.hosts.iter().filter(|h| h.connected).count()),
    }
}

fn build_menu(app: &AppHandle, s: &Status) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "status", status_line(s), false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for h in s.hosts.iter().filter(|h| h.connected) {
        let label = format!("Switch to {}", h.name.clone().unwrap_or_else(|| h.id.clone()));
        menu.append(&MenuItem::with_id(app, format!("switch:{}", h.id), label, s.active.as_ref() != Some(&h.id), None::<&str>)?)?;
    }
    menu.append(&MenuItem::with_id(app, "back", "Back to Windows", s.active.is_some(), None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let clip_on = app.state::<AppState>().settings.lock().unwrap().clipboard_sync;
    menu.append(&CheckMenuItem::with_id(app, "clipboard", "Clipboard sync", true, clip_on, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Open Bridge", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "settings" => show_window(app),
        "clipboard" => {
            let on = !state.settings.lock().unwrap().clipboard_sync;
            let _ = crate::commands::set_clipboard_sync_now(app, on);
        }
        "back" => { if let Some(e) = state.engine.lock().unwrap().as_ref() { e.send(Command::SwitchTo(None)); } }
        "quit" => {
            // Engine drop returns input to Windows and stops advertising before the process exits.
            drop(state.engine.lock().unwrap().take());
            drop(state.clip.lock().unwrap().take());
            drop(state.transfer.lock().unwrap().take()); // closes the endpoint so peers see us leave
            app.exit(0);
        }
        other => if let Some(host) = other.strip_prefix("switch:")
            && let Some(e) = state.engine.lock().unwrap().as_ref()
        {
            e.send(Command::SwitchTo(Some(host.to_string())));
        },
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
