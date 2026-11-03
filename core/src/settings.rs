//! Persisted settings. A missing, corrupt or invalid file never stops the app: defaults are used
//! and the bad file is kept as `settings.json.bak`.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::layout::{Attachment, validate};

/// Per-device return-edge tuning (see `edge::EdgeTracker`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration { pub gain: f64, pub min_push: f64, pub dwell_ms: u64 }

impl Default for Calibration { fn default() -> Self { Self { gain: 1.0, min_push: 96.0, dwell_ms: 300 } } }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub attachments: Vec<Attachment>,
    pub calibration: BTreeMap<String, Calibration>,
    pub entry_hold_ms: u64,
    pub start_with_windows: bool,
    /// Remote screen width in pointer units (iPad Air 13" ≈ 1366 points).
    pub remote_width: BTreeMap<String, f64>,
    /// Clipboard sync on/off (tray toggle and Clipboard tab).
    pub clipboard_sync: bool,
    /// Where received files go; `None` = `Downloads\Bridge`.
    pub receive_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { version: 1, attachments: Vec::new(), calibration: BTreeMap::new(), entry_hold_ms: 300, start_with_windows: true, remote_width: BTreeMap::new(), clipboard_sync: true, receive_dir: None }
    }
}

const DEFAULT_WIDTH: f64 = 1366.0;

impl Settings {
    pub fn calibration_for(&self, id: &str) -> Calibration { self.calibration.get(id).cloned().unwrap_or_default() }
    pub fn width_for(&self, id: &str) -> f64 { self.remote_width.get(id).copied().unwrap_or(DEFAULT_WIDTH) }

    /// Missing file → defaults; corrupt/invalid → backed up as `.bak`, defaults. Any other read
    /// error (locked by antivirus/OneDrive…) is an error, so a later Save can't overwrite real settings.
    pub fn try_load(path: &Path) -> Result<Settings, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => return Err(format!("can't read {}: {e}", path.display())),
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) if validate(&s.attachments).is_ok() => Ok(s),
            _ => {
                let mut bak = path.as_os_str().to_owned();
                bak.push(".bak");
                let _ = std::fs::rename(path, bak);
                Ok(Settings::default())
            }
        }
    }

    /// `try_load`, retrying briefly through transient locks; defaults only if the file stays unreadable.
    pub fn load(path: &Path) -> Settings {
        for _ in 0..5 {
            if let Ok(s) = Self::try_load(path) { return s }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Settings::default()
    }

    /// Validates, then writes atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        validate(&self.attachments)?;
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Attachment, Edge};
    use tempfile::tempdir;

    #[test] fn missing_file_gives_defaults() { let d = tempdir().unwrap(); assert_eq!(Settings::load(&d.path().join("s.json")), Settings::default()); }
    #[test] fn round_trip() {
        let d = tempdir().unwrap(); let p = d.path().join("s.json");
        let mut s = Settings::default();
        s.attachments.push(Attachment { device_id: "ipad".into(), monitor_id: "M1".into(), edge: Edge::Right });
        s.calibration.insert("ipad".into(), Calibration { gain: 1.4, min_push: 120.0, dwell_ms: 250 });
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
    }
    #[test] fn corrupt_file_is_backed_up_and_defaults_used() { // Review Focus 1
        let d = tempdir().unwrap(); let p = d.path().join("settings.json");
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        assert_eq!(std::fs::read_to_string(d.path().join("settings.json.bak")).unwrap(), "{not json");
    }
    #[test] fn older_file_missing_fields_keeps_known_ones() {
        let d = tempdir().unwrap(); let p = d.path().join("s.json");
        std::fs::write(&p, r#"{"version":1,"entry_hold_ms":500}"#).unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.entry_hold_ms, 500); assert!(s.start_with_windows);
        assert!(s.clipboard_sync);
    }
    #[test] fn clipboard_sync_defaults_on() { assert!(Settings::default().clipboard_sync); }
    #[test] fn invalid_layout_is_not_saved() {
        let d = tempdir().unwrap(); let p = d.path().join("s.json");
        let mut s = Settings::default();
        for id in ["a", "b"] { s.attachments.push(Attachment { device_id: id.into(), monitor_id: "M1".into(), edge: Edge::Right }); }
        assert!(s.save(&p).is_err()); assert!(!p.exists());
    }
    #[test] fn invalid_layout_on_disk_falls_back() {
        let d = tempdir().unwrap(); let p = d.path().join("settings.json");
        std::fs::write(&p, r#"{"attachments":[{"device_id":"a","monitor_id":"M1","edge":"Right"},{"device_id":"b","monitor_id":"M1","edge":"Right"}]}"#).unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        assert!(d.path().join("settings.json.bak").exists());
    }
    #[test] fn calibration_defaults_for_unknown_device() { assert_eq!(Settings::default().calibration_for("x"), Calibration { gain: 1.0, min_push: 96.0, dwell_ms: 300 }); }
    #[test] fn width_defaults_to_ipad_air_13() { assert_eq!(Settings::default().width_for("x"), 1366.0); }
    // Final review (re-graded): a read error that isn't "not found" (e.g. a sharing violation) must not
    // look like an empty file — the user's settings would be overwritten by the next Save.
    #[test] fn unreadable_path_is_reported_not_defaulted() {
        let d = tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::create_dir(&p).unwrap(); // reading a directory fails with an IO error other than NotFound
        assert!(Settings::try_load(&p).is_err());
        assert!(p.is_dir(), "nothing renamed or replaced");
    }
}
