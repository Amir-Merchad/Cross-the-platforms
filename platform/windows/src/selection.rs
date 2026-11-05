//! Explorer's selected files, through Shell COM, for Ctrl+Alt+F. COM calls into Explorer can hang,
//! so this runs on its own thread, never the engine's, and one at a time. Logs nothing about the files.

use std::{path::PathBuf, sync::atomic::{AtomicBool, Ordering}, thread};

use windows::{
    Win32::{
        Foundation::HWND,
        System::{Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IServiceProvider}, Variant::VARIANT},
        UI::{
            Shell::{IFolderView2, IShellBrowser, IShellWindows, IWebBrowserApp, SID_STopLevelBrowser, SIGDN_FILESYSPATH, ShellWindows},
            WindowsAndMessaging::{FindWindowExW, GetForegroundWindow},
        },
    },
    core::{Interface, w},
};

pub const NOTHING_SELECTED: &str = "Nothing is selected in Explorer, so nothing was sent.";
pub const NOT_ANSWERING: &str = "Explorer is not answering, so nothing was sent.";

/// A worker is asking Explorer. It stays set while Explorer hangs, so presses never pile up threads.
static BUSY: AtomicBool = AtomicBool::new(false);

fn answer(files: Vec<PathBuf>) -> Result<Vec<PathBuf>, &'static str> { if files.is_empty() { Err(NOTHING_SELECTED) } else { Ok(files) } }

/// Finds the selection of the Explorer window in front, or a plain message saying why there is none.
pub fn find_selected(done: impl FnOnce(Result<Vec<PathBuf>, &'static str>) + Send + 'static) {
    if BUSY.swap(true, Ordering::SeqCst) { return done(Err(NOT_ANSWERING)) }
    let worker = thread::Builder::new().name("ctp-selection".into()).spawn(move || {
        let files = selected_in(unsafe { GetForegroundWindow() });
        BUSY.store(false, Ordering::SeqCst);
        done(answer(files));
    });
    if worker.is_err() { BUSY.store(false, Ordering::SeqCst); }
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

unsafe fn shell_selection(top: HWND) -> windows::core::Result<Vec<PathBuf>> {
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
        assert_eq!(answer(Vec::new()), Err(NOTHING_SELECTED));
        assert_eq!(answer(vec![PathBuf::from("a")]), Ok(vec![PathBuf::from("a")]));
    }
    #[test] fn a_press_while_explorer_is_busy_says_so() {
        BUSY.store(true, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        find_selected(move |r| tx.send(r).unwrap());
        assert_eq!(rx.try_recv(), Ok(Err(NOT_ANSWERING)), "answered at once, no second worker");
        BUSY.store(false, Ordering::SeqCst);
    }
}
