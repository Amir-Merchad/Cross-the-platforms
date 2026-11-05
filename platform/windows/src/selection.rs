//! Explorer's selected files, through Shell COM, for Ctrl+Alt+F. COM calls into Explorer can hang,
//! so this runs on its own thread, never the engine's. Logs nothing about the files.

use std::{path::PathBuf, thread};

use windows::{
    Win32::{
        Foundation::HWND,
        System::{Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IServiceProvider}, Variant::VARIANT},
        UI::{
            Shell::{IFolderView2, IShellBrowser, IShellWindows, IWebBrowserApp, SID_STopLevelBrowser, SIGDN_FILESYSPATH, ShellWindows},
            WindowsAndMessaging::{FindWindowExW, GetForegroundWindow},
        },
    },
    core::{Interface, Result, w},
};

pub const NOTHING_SELECTED: &str = "Nothing is selected in Explorer, so nothing was sent.";
pub const SENDING: &str = "Sending the selected files.";
const NOT_RUNNING: &str = "File sending is not running, so nothing was sent.";

fn outcome(files: usize) -> &'static str { if files == 0 { NOTHING_SELECTED } else { SENDING } }

/// Sends the selection of the Explorer window in front to `peer`, then says what happened.
pub fn send_selected(peer: String, say: impl FnOnce(&'static str) + Send + 'static) {
    let _ = thread::Builder::new().name("ctp-selection".into()).spawn(move || {
        let files = selected_in(unsafe { GetForegroundWindow() });
        let line = outcome(files.len());
        say(if files.is_empty() || crate::transfer::send_files_to(peer, files) { line } else { NOT_RUNNING });
    });
}

/// The real files selected in Explorer window `top`. Anything else (another app, folders, items with
/// no path such as a phone's storage) is left out, so the answer can be empty.
fn selected_in(top: HWND) -> Vec<PathBuf> {
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let files = shell_selection(top).unwrap_or_default();
        if init.is_ok() { CoUninitialize(); }
        files.into_iter().filter(|p| p.is_file()).collect()
    }
}

unsafe fn shell_selection(top: HWND) -> Result<Vec<PathBuf>> {
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
        // Windows 11 tabs share the top window; its first tab child is the tab in front.
        let tab = FindWindowExW(Some(top), None, w!("ShellTabWindowClass"), None).ok();
        for i in 0..windows.Count()? {
            let Ok(item) = windows.Item(&VARIANT::from(i)) else { continue };
            let Ok(app) = item.cast::<IWebBrowserApp>() else { continue };
            if app.HWND().map(|h| h.0) != Ok(top.0 as isize) { continue }
            let browser: IShellBrowser = item.cast::<IServiceProvider>()?.QueryService(&SID_STopLevelBrowser)?;
            if tab.is_some_and(|t| browser.GetWindow() != Ok(t)) { continue }
            let view: IFolderView2 = browser.QueryActiveShellView()?.cast()?;
            let Ok(items) = view.GetSelection(false) else { return Ok(Vec::new()) };
            let mut out = Vec::new();
            for n in 0..items.GetCount()? {
                let Ok(path) = items.GetItemAt(n)?.GetDisplayName(SIGDN_FILESYSPATH) else { continue };
                out.push(PathBuf::from(path.to_string().unwrap_or_default()));
                CoTaskMemFree(Some(path.0 as _));
            }
            return Ok(out);
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn a_window_that_is_not_explorer_selects_nothing() { // Review Focus 5
        assert!(selected_in(HWND::default()).is_empty());
    }
    #[test] fn nothing_selected_says_so() {
        assert_eq!(outcome(0), NOTHING_SELECTED);
        assert_eq!(outcome(3), SENDING);
    }
}
