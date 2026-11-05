//! The quick notes inbox on this PC: `Documents\Bridge\Inbox.md`. Note text is never logged.

use std::{path::{Path, PathBuf}, sync::Mutex};

use bridge_core::notes;
use windows::Win32::{System::{Com::CoTaskMemFree, SystemInformation::GetLocalTime}, UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath}};

static WRITE: Mutex<()> = Mutex::new(());

fn inbox_in(documents: &Path) -> PathBuf { documents.join("Bridge").join("Inbox.md") }

/// The real Documents folder (it may be redirected to OneDrive), else `%USERPROFILE%\Documents`.
pub fn inbox_path() -> PathBuf {
    let known = unsafe {
        SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok().map(|p| {
            let s = p.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(p.as_ptr().cast()));
            s
        })
    };
    inbox_in(&known.flatten().unwrap_or_else(|| PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())).join("Documents")))
}

fn local_stamp() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute)
}

/// Files a note from `device` at the top of the inbox.
pub fn record(device: &str, text: &str) -> std::io::Result<()> {
    let _g = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    notes::append(&inbox_path(), &local_stamp(), device, text)
}

pub fn list() -> Vec<notes::Note> { notes::read(&inbox_path()) }

#[cfg(test)]
mod tests {
    #[test]
    fn inbox_lives_in_documents_bridge() {
        let p = super::inbox_in(std::path::Path::new(r"C:\Users\me\Documents"));
        assert_eq!(p, std::path::PathBuf::from(r"C:\Users\me\Documents\Bridge\Inbox.md"));
    }
}
