//! Windows clipboard: read/write text and images, and a change watcher (`AddClipboardFormatListener`,
//! no polling). Writing triggers the watcher too; the core `Hub` drops that echo by content hash.

use std::{borrow::Cow, cell::RefCell, sync::mpsc::Sender, thread, time::Duration};

use bridge_core::clipboard::{Content, Image};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{DataExchange::AddClipboardFormatListener, LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

const BUSY_ATTEMPTS: u32 = 5;
const BUSY_DELAY: Duration = Duration::from_millis(50);

/// Another app holding the clipboard open is common and brief; anything else won't fix itself.
fn with_retry<T>(attempts: u32, delay: Duration, mut f: impl FnMut() -> Result<T, arboard::Error>) -> Result<T, arboard::Error> {
    let mut n = 1;
    loop {
        match f() {
            Err(arboard::Error::ClipboardOccupied) if n < attempts => { n += 1; thread::sleep(delay); }
            r => return r,
        }
    }
}

/// Text if there is any, else an image, else `None` (files, RTF-only, empty, or still busy).
pub fn read() -> Option<Content> {
    let mut cb = with_retry(BUSY_ATTEMPTS, BUSY_DELAY, arboard::Clipboard::new).ok()?;
    if let Ok(t) = with_retry(BUSY_ATTEMPTS, BUSY_DELAY, || cb.get_text()) && !t.is_empty() { return Some(Content::Text(t)) }
    let img = with_retry(BUSY_ATTEMPTS, BUSY_DELAY, || cb.get_image()).ok()?;
    Some(Content::Image(Image { width: img.width as u32, height: img.height as u32, rgba: img.bytes.into_owned() }))
}

pub fn write(c: &Content) -> Result<(), String> {
    let mut cb = with_retry(BUSY_ATTEMPTS, BUSY_DELAY, arboard::Clipboard::new).map_err(|e| e.to_string())?;
    match c {
        Content::Text(t) => with_retry(BUSY_ATTEMPTS, BUSY_DELAY, || cb.set_text(t.as_str())),
        Content::Image(i) => with_retry(BUSY_ATTEMPTS, BUSY_DELAY, || cb.set_image(arboard::ImageData {
            width: i.width as usize, height: i.height as usize, bytes: Cow::Borrowed(&i.rgba),
        })),
    }
    .map_err(|e| e.to_string())
}

thread_local! { static WATCH_TX: RefCell<Option<Sender<Content>>> = const { RefCell::new(None) }; }

pub struct WatchHandle { thread_id: u32, thread: Option<thread::JoinHandle<()>> }

impl WatchHandle {
    pub fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            unsafe { let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)); }
            let _ = t.join();
        }
    }
}

impl Drop for WatchHandle { fn drop(&mut self) { self.stop() } }

/// Sends the clipboard's content on every change, from its own thread.
pub fn watch(tx: Sender<Content>) -> WatchHandle {
    let (id_tx, id_rx) = std::sync::mpsc::channel();
    let thread = thread::spawn(move || unsafe {
        WATCH_TX.with(|t| *t.borrow_mut() = Some(tx));
        let hinst = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: w!("ctp-clipboard"), ..Default::default() };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(Default::default(), w!("ctp-clipboard"), w!(""), Default::default(), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst.into()), None).unwrap();
        if let Err(e) = AddClipboardFormatListener(hwnd) { crate::diag::log(format_args!("clipboard listener failed: {e}")); }
        let _ = id_tx.send(GetCurrentThreadId());
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() { DispatchMessageW(&msg); }
        let _ = DestroyWindow(hwnd);
    });
    // The id must be known before stop() can post WM_QUIT; the thread sends it once its queue exists.
    let thread_id = id_rx.recv().unwrap_or(0);
    WatchHandle { thread_id, thread: Some(thread) }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if let Some(c) = read() { WATCH_TX.with(|t| if let Some(tx) = t.borrow().as_ref() { let _ = tx.send(c); }); }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn retry_gives_up_after_attempts() { // Review Focus 2
        let mut n = 0;
        let r: Result<(), _> = with_retry(5, Duration::ZERO, || { n += 1; Err(arboard::Error::ClipboardOccupied) });
        assert!(r.is_err()); assert_eq!(n, 5);
    }
    #[test] fn retry_stops_on_other_errors() {
        let mut n = 0;
        let _ = with_retry(5, Duration::ZERO, || { n += 1; Err::<(), _>(arboard::Error::ContentNotAvailable) });
        assert_eq!(n, 1);
    }
    // These use the real Windows clipboard: run locally with `--include-ignored`.
    #[test] #[ignore] fn text_round_trip() {
        write(&Content::Text("héllo 👋 مرحبا".into())).unwrap();
        assert_eq!(read(), Some(Content::Text("héllo 👋 مرحبا".into())));
    }
    #[test] #[ignore] fn image_round_trip_is_identical() { // Review Focus 1: Windows echo of an iPad image
        let img = Image { width: 2, height: 2, rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255] };
        write(&Content::Image(img.clone())).unwrap();
        assert_eq!(read(), Some(Content::Image(img)));
    }
    #[test] #[ignore] fn empty_clipboard_reads_none() { arboard::Clipboard::new().unwrap().clear().unwrap(); assert_eq!(read(), None); }
}
