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
