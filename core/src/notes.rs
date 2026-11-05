//! The quick notes inbox: one Markdown file, newest first, each entry headed by time and device.
//! Note text is never logged. Written to a temp file and renamed, so a crash cannot cut the inbox.

use std::{fs, io, path::Path};

use serde::Serialize;

/// A note longer than this is dropped, not truncated.
pub const MAX_NOTE_BYTES: usize = 256 * 1024;
const HEAD: &str = "## ";
const SEP: &str = " · ";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note { pub stamp: String, pub device: String, pub text: String }

fn one_line(s: &str) -> String { s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect() }

/// Puts the note at the top of the file at `path` (created with its folder if missing).
pub fn append(path: &Path, stamp: &str, device: &str, text: &str) -> io::Result<()> {
    let body: Vec<String> = text.trim_end().lines().map(|l| if l.starts_with(HEAD) { format!("\\{l}") } else { l.to_string() }).collect();
    let entry = format!("{HEAD}{}{SEP}{}\n\n{}\n\n", one_line(stamp), one_line(device), body.join("\n"));
    let old = match fs::read_to_string(path) { Ok(s) => s, Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(), Err(e) => return Err(e) };
    if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, entry + &old)?;
    fs::rename(&tmp, path)
}

/// Every note in the file, newest first. A missing or unreadable file is an empty inbox.
pub fn read(path: &Path) -> Vec<Note> {
    let Ok(s) = fs::read_to_string(path) else { return Vec::new() };
    let mut out: Vec<Note> = Vec::new();
    for line in s.lines() {
        if let Some((stamp, device)) = line.strip_prefix(HEAD).and_then(|h| h.split_once(SEP)) {
            out.push(Note { stamp: stamp.into(), device: device.into(), text: String::new() });
        } else if let Some(n) = out.last_mut() {
            n.text.push_str(line.strip_prefix('\\').filter(|l| l.starts_with(HEAD)).unwrap_or(line));
            n.text.push('\n');
        }
    }
    for n in &mut out { n.text = n.text.trim().to_string(); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_is_appended_with_time_and_device() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Bridge").join("Inbox.md");
        append(&p, "2026-11-05 14:32", "Pixel 8", "buy milk").unwrap();
        append(&p, "2026-11-05 15:01", "iPad", "line one\n## not a heading\nline three").unwrap();
        let file = fs::read_to_string(&p).unwrap();
        assert!(file.starts_with("## 2026-11-05 15:01 · iPad\n"), "newest first");
        assert!(file.contains("## 2026-11-05 14:32 · Pixel 8\n\nbuy milk"));
        let n = read(&p);
        assert_eq!(n.len(), 2);
        assert_eq!((n[0].device.as_str(), n[0].text.as_str()), ("iPad", "line one\n## not a heading\nline three"));
        assert_eq!((n[1].stamp.as_str(), n[1].text.as_str()), ("2026-11-05 14:32", "buy milk"));
    }

    #[test]
    fn note_survives_restart() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        append(&p, "2026-11-05 09:00", "Pixel 8", "first").unwrap();
        // A restart is a fresh process: nothing but the file remains.
        append(&p, "2026-11-06 09:00", "Pixel 8", "second").unwrap();
        let n = read(&p);
        assert_eq!(n.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert!(!p.with_extension("md.tmp").exists());
        assert!(read(&d.path().join("missing.md")).is_empty());
    }
}
