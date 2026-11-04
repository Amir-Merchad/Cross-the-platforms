//! Diagnostic log: `%APPDATA%\CrossThePlatforms\engine.log`. Timings, ids and event kinds only —
//! never key codes, text or clipboard contents.

use std::{fs::OpenOptions, io::Write, sync::{Mutex, OnceLock}, time::Instant};

static LOG: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

pub fn log(msg: impl std::fmt::Display) {
    let start = START.get_or_init(Instant::now);
    let file = LOG.get_or_init(|| {
        let path = crate::engine::settings_path().with_file_name("engine.log");
        Mutex::new(OpenOptions::new().create(true).append(true).open(path).ok())
    });
    if let Ok(mut g) = file.lock() && let Some(f) = g.as_mut() {
        let _ = writeln!(f, "{:>9} ms  {msg}", start.elapsed().as_millis());
    }
}

/// Wall-clock marker, so a Windows Bluetooth crash time can be matched to this log.
pub fn clock_line(unix: u64, hosts: usize, active: bool) -> String { format!("clock unix={unix} hosts={hosts} active={active}") }

/// Foreground window's exe and elevation: an elevated foreground hides keys from a non-elevated hook (UIPI).
pub fn foreground() -> String {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return format!("pid {pid} (cannot open)") };
        let (mut buf, mut n) = ([0u16; 260], 260u32);
        let exe = match QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n) {
            Ok(()) => String::from_utf16_lossy(&buf[..n as usize]).rsplit('\\').next().unwrap_or_default().to_string(),
            Err(_) => "?".into(),
        };
        let mut elevated = "?".to_string();
        let mut tok = HANDLE::default();
        if OpenProcessToken(p, TOKEN_QUERY, &mut tok).is_ok() {
            let (mut e, mut len) = (TOKEN_ELEVATION::default(), 0);
            if GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as _), size_of::<TOKEN_ELEVATION>() as u32, &mut len).is_ok() {
                elevated = (e.TokenIsElevated != 0).to_string();
            }
            let _ = CloseHandle(tok);
        }
        let _ = CloseHandle(p);
        format!("pid {pid} {exe} elevated={elevated}")
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    #[test] fn clock_line_lets_a_crash_time_be_matched_to_the_log() {
        assert_eq!(clock_line(1_793_800_000, 2, true), "clock unix=1793800000 hosts=2 active=true");
    }
}
