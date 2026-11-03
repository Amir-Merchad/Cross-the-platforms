//! Explorer "Send to" entries, one per paired device: `Bridge – <name>.lnk` in
//! `%APPDATA%\Microsoft\Windows\SendTo`, running `ctp-app.exe --send-to <peer id> <files…>`.

use std::path::{Path, PathBuf};

use bridge_core::{identity::Peer, names::safe_name};
use windows::{
    Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile},
        UI::Shell::{IShellLinkW, ShellLink},
    },
    core::{HSTRING, Interface},
};

const PREFIX: &str = "Bridge – ";
pub const FLAG: &str = "--send-to";

pub fn sendto_shortcut_args(peer_id: &str) -> String { format!("{FLAG} {peer_id}") }

/// `[.., "--send-to", id, files…]` → `(id, files)`.
pub fn parse_send_to(args: &[String]) -> Option<(String, Vec<PathBuf>)> {
    let i = args.iter().position(|a| a == FLAG)?;
    let id = args.get(i + 1)?.clone();
    let files: Vec<PathBuf> = args[i + 2..].iter().map(PathBuf::from).collect();
    (!files.is_empty()).then_some((id, files))
}

fn sendto_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("Microsoft").join("Windows").join("SendTo")
}

/// Replaces our shortcuts with one per paired peer. Only touches files we named.
pub fn refresh(peers: &[Peer], exe: &Path) -> Result<(), String> {
    let dir = sendto_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with(PREFIX) && n.ends_with(".lnk") { let _ = std::fs::remove_file(e.path()); }
        }
    }
    for p in peers { create_link(&safe_name(&dir, &format!("{PREFIX}{}.lnk", p.name)), exe, &sendto_shortcut_args(&p.id), &p.name)?; }
    Ok(())
}

fn create_link(at: &Path, exe: &Path, args: &str, name: &str) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); // already-initialized is fine
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        link.SetPath(&HSTRING::from(exe.as_os_str())).map_err(|e| e.to_string())?;
        link.SetArguments(&HSTRING::from(args)).map_err(|e| e.to_string())?;
        link.SetDescription(&HSTRING::from(format!("Send to {name} with Cross the Platforms"))).map_err(|e| e.to_string())?;
        link.cast::<IPersistFile>().map_err(|e| e.to_string())?.Save(&HSTRING::from(at.as_os_str()), true).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn shortcut_args() { assert_eq!(sendto_shortcut_args("abc123"), "--send-to abc123"); }
    #[test] fn parses_explorer_invocation() {
        let a: Vec<String> = ["ctp-app.exe", "--send-to", "abc", r"C:\x\a b.pdf", r"C:\y.mp4"].map(String::from).into();
        assert_eq!(parse_send_to(&a), Some(("abc".into(), vec![PathBuf::from(r"C:\x\a b.pdf"), PathBuf::from(r"C:\y.mp4")])));
    }
    #[test] fn no_files_is_nothing() {
        let a: Vec<String> = ["ctp-app.exe", "--send-to", "abc"].map(String::from).into();
        assert_eq!(parse_send_to(&a), None);
        assert_eq!(parse_send_to(&["ctp-app.exe".to_string()]), None);
    }
}
