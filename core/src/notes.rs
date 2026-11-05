//! The quick notes inbox: one Markdown file, newest first, each entry headed by time and device.
//! Note text is never logged. Written to a temp file and renamed, so a crash cannot cut the inbox.
//! A note that cannot be filed (file held by an editor or OneDrive, inbox full) goes to a spill file
//! next to it, so the text always survives somewhere.

use std::{fs, io::{self, Write}, path::{Path, PathBuf}, thread, time::Duration};

use serde::Serialize;

/// A note longer than this is refused, never truncated.
pub const MAX_NOTE_BYTES: usize = 256 * 1024;
/// Inbox.md (and the spill file) stop growing here; older notes are never deleted to make room.
pub const MAX_INBOX_BYTES: u64 = 8 * 1024 * 1024;
const HEAD: &str = "## ";
const SEP: &str = " · ";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note { pub stamp: String, pub device: String, pub text: String }

/// Where a note ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved { Inbox, Spill }

/// Why a note cannot be sent or filed at all, in plain words. Checked by the sender and again by the PC.
pub fn check(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() { Err("There is nothing to send.") }
    else if text.len() > MAX_NOTE_BYTES { Err("That note is too long to send. The limit is 256 KB.") }
    else { Ok(()) }
}

pub fn spill_path(inbox: &Path) -> PathBuf { inbox.with_file_name("Inbox-unsaved.md") }

fn one_line(s: &str) -> String { s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect() }

/// A body line that could be read as a header or as an escape gets a leading backslash.
fn escape(l: &str) -> String { if l.starts_with(HEAD) || l.starts_with('\\') { format!("\\{l}") } else { l.to_string() } }

fn entry(stamp: &str, device: &str, text: &str) -> String {
    let body: Vec<String> = text.trim_end().lines().map(escape).collect();
    format!("{HEAD}{}{SEP}{}\n\n{}\n\n", one_line(stamp), one_line(device), body.join("\n"))
}

/// Puts the note at the top of the file at `path` (created with its folder if missing). Over the size
/// cap it fails with `StorageFull` and changes nothing.
// ponytail: rewrites the whole file, fine up to the 8 MiB cap; a bigger inbox would need an append-only
// log plus a reader that reverses it.
pub fn append(path: &Path, stamp: &str, device: &str, text: &str) -> io::Result<()> {
    check(text).map_err(|m| io::Error::new(io::ErrorKind::InvalidInput, m))?;
    let entry = entry(stamp, device, text);
    let old = match fs::read_to_string(path) { Ok(s) => s, Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(), Err(e) => return Err(e) };
    if (old.len() + entry.len()) as u64 > MAX_INBOX_BYTES { return Err(io::Error::new(io::ErrorKind::StorageFull, "the notes file is full")); }
    if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, entry + &old)?;
    fs::rename(&tmp, path)
}

fn spill(inbox: &Path, stamp: &str, device: &str, text: &str) -> io::Result<()> {
    let p = spill_path(inbox);
    if fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MAX_INBOX_BYTES { return Err(io::Error::new(io::ErrorKind::StorageFull, "the spill file is full")); }
    if let Some(dir) = p.parent() { fs::create_dir_all(dir)?; }
    fs::OpenOptions::new().create(true).append(true).open(p)?.write_all(entry(stamp, device, text).as_bytes())
}

/// Files the note, retrying a failed write `tries` times `delay` apart; then, or at once when the inbox
/// is full, appends it to the spill file. Fails only when both are impossible.
pub fn save(inbox: &Path, stamp: &str, device: &str, text: &str, tries: u32, delay: Duration) -> io::Result<Saved> {
    check(text).map_err(|m| io::Error::new(io::ErrorKind::InvalidInput, m))?;
    for i in 0..tries.max(1) {
        match append(inbox, stamp, device, text) {
            Ok(()) => return Ok(Saved::Inbox),
            Err(e) if e.kind() == io::ErrorKind::StorageFull => break,
            Err(_) if i + 1 < tries => thread::sleep(delay),
            Err(_) => {}
        }
    }
    spill(inbox, stamp, device, text).map(|()| Saved::Spill)
}

/// Every note in the file, newest first. A missing or unreadable file is an empty inbox.
pub fn read(path: &Path) -> Vec<Note> {
    let Ok(s) = fs::read_to_string(path) else { return Vec::new() };
    let mut out: Vec<Note> = Vec::new();
    for line in s.lines() {
        if let Some((stamp, device)) = line.strip_prefix(HEAD).and_then(|h| h.split_once(SEP)) {
            out.push(Note { stamp: stamp.into(), device: device.into(), text: String::new() });
        } else if let Some(n) = out.last_mut() {
            n.text.push_str(line.strip_prefix('\\').unwrap_or(line));
            n.text.push('\n');
        }
    }
    for n in &mut out { n.text = n.text.trim().to_string(); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: Duration = Duration::ZERO;

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
        // A restart keeps nothing but the file: read it back cold, then keep filing on top of it.
        let before = read(&p);
        assert_eq!(before, [Note { stamp: "2026-11-05 09:00".into(), device: "Pixel 8".into(), text: "first".into() }]);
        append(&p, "2026-11-06 09:00", "Pixel 8", "second").unwrap();
        let after = read(&p);
        assert_eq!(after.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(after[1], before[0]);
        assert!(!p.with_extension("md.tmp").exists());
        assert!(read(&d.path().join("missing.md")).is_empty());
    }

    #[test]
    fn heading_and_backslash_lines_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        let text = "## 2026-01-01 00:00 · fake\n\\## already escaped\n\\plain\nend";
        append(&p, "2026-11-05 09:00", "iPad", text).unwrap();
        let n = read(&p);
        assert_eq!(n.len(), 1, "the body never makes a second note");
        assert_eq!(n[0].text, text);
    }

    #[test]
    fn device_name_with_separator_or_newline_stays_one_header() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        append(&p, "2026-11-05 09:00", "Amir · Pixel\n## sneaky", "hi").unwrap();
        let n = read(&p);
        assert_eq!(n.len(), 1);
        assert_eq!((n[0].stamp.as_str(), n[0].device.as_str(), n[0].text.as_str()), ("2026-11-05 09:00", "Amir · Pixel ## sneaky", "hi"));
    }

    #[test]
    fn oversize_and_empty_notes_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        assert!(check(&"x".repeat(MAX_NOTE_BYTES + 1)).is_err() && check("  \n").is_err() && check(&"x".repeat(MAX_NOTE_BYTES)).is_ok());
        assert!(save(&p, "t", "d", &"x".repeat(MAX_NOTE_BYTES + 1), 1, NOW).is_err());
        assert!(save(&p, "t", "d", " ", 1, NOW).is_err());
        assert!(!p.exists() && !spill_path(&p).exists(), "a refused note leaves no file");
    }

    #[test]
    fn a_failed_write_goes_to_the_spill_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        fs::create_dir(&p).unwrap(); // the inbox cannot be read or replaced
        assert_eq!(save(&p, "2026-11-05 09:00", "iPad", "keep me", 3, NOW).unwrap(), Saved::Spill);
        let n = read(&spill_path(&p));
        assert_eq!((n.len(), n[0].text.as_str()), (1, "keep me"));
    }

    #[test]
    fn a_full_inbox_spills_and_never_loses_older_notes() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Inbox.md");
        fs::write(&p, "x".repeat(MAX_INBOX_BYTES as usize - 10)).unwrap();
        assert_eq!(save(&p, "2026-11-05 09:00", "iPad", "late", 1, NOW).unwrap(), Saved::Spill);
        assert_eq!(fs::read_to_string(&p).unwrap().len() as u64, MAX_INBOX_BYTES - 10, "inbox untouched");
        assert_eq!(read(&spill_path(&p))[0].text, "late");
    }
}
