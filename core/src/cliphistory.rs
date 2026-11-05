//! Clipboard history on this device only: 50 unpinned items, pins that never expire, images kept as PNG
//! files under a size limit. Never logged, never synced. A bad history file starts empty and is kept
//! as `history.json.bak`; a file that can't be written just means nothing is remembered.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::clipboard::{Content, codec};

pub const MAX_UNPINNED: usize = 50;
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_TOTAL_IMAGE_BYTES: u64 = 200 * 1024 * 1024;
/// A huge text or HTML copy is synced but not remembered.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind { Text, Image }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    pub text: Option<String>,
    /// HTML of a formatted copy; entries from before this existed load without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Image file name inside the history folder.
    pub file: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// Device it was copied on.
    pub from: String,
    /// Unix milliseconds.
    pub at: u64,
    pub pinned: bool,
    hash: String,
}

/// An item ready to store: hashing and PNG encoding done before taking any lock.
pub enum Prepared { Text { hash: String, text: String, html: Option<String> }, Image { hash: String, width: u32, height: u32, png: Vec<u8> } }

fn hex(d: impl AsRef<[u8]>) -> String { d.as_ref().iter().map(|b| format!("{b:02x}")).collect() }

/// `None` for what history never keeps: empty or huge text, an image that won't encode.
pub fn prepare(c: &Content) -> Option<Prepared> {
    match c {
        Content::Text(t) if t.is_empty() || t.len() > MAX_TEXT_BYTES => None,
        Content::Text(t) => Some(Prepared::Text { hash: hex(Sha256::new().chain_update(b"t").chain_update(t.as_bytes()).finalize()), text: t.clone(), html: None }),
        Content::Rich { text, html } if text.len() + html.len() > MAX_TEXT_BYTES => prepare(&Content::Text(text.clone())), // the text alone may still fit
        Content::Rich { text, .. } if text.is_empty() => None,
        Content::Rich { text, html } => Some(Prepared::Text {
            hash: hex(Sha256::new().chain_update(b"r").chain_update((text.len() as u64).to_le_bytes()).chain_update(text.as_bytes()).chain_update(html.as_bytes()).finalize()),
            text: text.clone(), html: Some(html.clone()),
        }),
        Content::Image(i) => {
            let hash = hex(Sha256::new().chain_update(b"i").chain_update(i.width.to_le_bytes()).chain_update(i.height.to_le_bytes()).chain_update(&i.rgba).finalize());
            Some(Prepared::Image { hash, width: i.width, height: i.height, png: codec::encode_png(i).ok()? })
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Stored { enabled: bool, next_id: u64, entries: Vec<Entry> }
impl Default for Stored { fn default() -> Self { Self { enabled: true, next_id: 1, entries: Vec::new() } } }

/// `saving` is off when a file we couldn't read or move aside is still in the way: writing would destroy it.
pub struct History { dir: PathBuf, s: Stored, max_image: u64, max_total: u64, saving: bool }

impl History {
    pub fn open(dir: impl Into<PathBuf>) -> Self { Self::open_with(dir, MAX_IMAGE_BYTES, MAX_TOTAL_IMAGE_BYTES) }

    pub fn open_with(dir: impl Into<PathBuf>, max_image: u64, max_total: u64) -> Self {
        let dir = dir.into();
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("history.json");
        let aside = || {
            let bak = dir.join("history.json.bak");
            let _ = fs::remove_file(&bak);
            fs::rename(&file, bak).is_ok()
        };
        let (mut saving, mut s) = (true, Stored::default());
        match fs::read(&file) {
            Ok(b) => match serde_json::from_slice(&b) { Ok(v) => s = v, Err(_) => saving = aside() },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => saving = aside(),
        }
        Self { dir, s, max_image, max_total, saving }
    }

    pub fn enabled(&self) -> bool { self.s.enabled }
    pub fn entries(&self) -> &[Entry] { &self.s.entries }

    pub fn set_enabled(&mut self, on: bool) { self.s.enabled = on; self.save(); }

    fn save(&self) {
        if !self.saving { return }
        let tmp = self.dir.join("history.json.tmp");
        if let Ok(b) = serde_json::to_vec(&self.s) && fs::write(&tmp, b).is_ok() { let _ = fs::rename(tmp, self.dir.join("history.json")); }
    }

    fn drop_file(&self, e: &Entry) { if let Some(f) = &e.file { let _ = fs::remove_file(self.dir.join(f)); } }

    /// Newest first. Returns whether anything was stored.
    pub fn add(&mut self, from: &str, item: Prepared, at: u64) -> bool {
        if !self.s.enabled { return false }
        let (hash, kind) = match &item { Prepared::Text { hash, .. } => (hash.clone(), Kind::Text), Prepared::Image { hash, .. } => (hash.clone(), Kind::Image) };
        if let Some(i) = self.s.entries.iter().position(|e| e.hash == hash) {
            let mut e = self.s.entries.remove(i);
            e.at = at;
            e.from = from.into();
            self.s.entries.insert(0, e);
            self.save();
            return true;
        }
        let id = self.s.next_id;
        let mut e = Entry { id, kind, text: None, html: None, file: None, width: 0, height: 0, bytes: 0, from: from.into(), at, pinned: false, hash };
        match item {
            Prepared::Text { text, html, .. } => { e.bytes = (text.len() + html.as_ref().map_or(0, String::len)) as u64; e.text = Some(text); e.html = html; }
            Prepared::Image { width, height, png, .. } => {
                if png.len() as u64 > self.max_image || self.pinned_image_bytes() + png.len() as u64 > self.max_total { return false }
                if !self.saving { return false }
                let name = format!("img-{id}.png");
                if fs::write(self.dir.join(&name), &png).is_err() { return false }
                (e.file, e.width, e.height, e.bytes) = (Some(name), width, height, png.len() as u64);
            }
        }
        self.s.next_id += 1;
        self.s.entries.insert(0, e);
        self.trim();
        self.save();
        true
    }

    fn pinned_image_bytes(&self) -> u64 { self.s.entries.iter().filter(|e| e.pinned && e.file.is_some()).map(|e| e.bytes).sum() }

    /// Oldest unpinned go first, by count and by total image size.
    fn trim(&mut self) {
        while self.s.entries.iter().filter(|e| !e.pinned).count() > MAX_UNPINNED { self.remove_oldest_unpinned(|_| true); }
        while self.s.entries.iter().filter(|e| e.file.is_some()).map(|e| e.bytes).sum::<u64>() > self.max_total {
            if !self.remove_oldest_unpinned(|e| e.file.is_some()) { break }
        }
    }

    fn remove_oldest_unpinned(&mut self, f: impl Fn(&Entry) -> bool) -> bool {
        let Some(i) = self.s.entries.iter().rposition(|e| !e.pinned && f(e)) else { return false };
        let e = self.s.entries.remove(i);
        self.drop_file(&e);
        true
    }

    pub fn pin(&mut self, id: u64, pinned: bool) {
        let room = self.max_total;
        let held = self.pinned_image_bytes();
        // Pinning an image that would take pinned images over the total is refused: pins can't be trimmed.
        if let Some(e) = self.s.entries.iter_mut().find(|e| e.id == id) && !(pinned && !e.pinned && e.file.is_some() && held + e.bytes > room) { e.pinned = pinned; }
        self.trim();
        self.save();
    }

    pub fn delete(&mut self, id: u64) {
        if let Some(i) = self.s.entries.iter().position(|e| e.id == id) { let e = self.s.entries.remove(i); self.drop_file(&e); }
        self.save();
    }

    /// Everything, pins too.
    pub fn clear(&mut self) {
        for e in std::mem::take(&mut self.s.entries) { self.drop_file(&e); }
        self.save();
    }

    /// Text containing `q`, ignoring case. An empty query lists everything.
    pub fn search(&self, q: &str) -> Vec<&Entry> {
        let q = q.to_lowercase();
        self.s.entries.iter().filter(|e| q.is_empty() || e.text.as_ref().is_some_and(|t| t.to_lowercase().contains(&q))).collect()
    }

    /// What to put back on the clipboard.
    pub fn content(&self, id: u64) -> Option<Content> {
        let e = self.s.entries.iter().find(|e| e.id == id)?;
        match (&e.text, &e.file) {
            (Some(t), _) => Some(match &e.html { Some(html) => Content::Rich { text: t.clone(), html: html.clone() }, None => Content::Text(t.clone()) }),
            (_, Some(f)) => codec::decode(&fs::read(self.dir.join(f)).ok()?).ok().map(Content::Image),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::Image;

    fn text(t: &str) -> Prepared { prepare(&Content::Text(t.into())).unwrap() }
    fn hist() -> (History, tempfile::TempDir) { let d = tempfile::tempdir().unwrap(); (History::open(d.path().join("h")), d) }

    #[test] fn old_history_entries_load() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("history.json"), br#"{"enabled":true,"next_id":2,"entries":[{"id":1,"kind":"text","text":"old","file":null,"width":0,"height":0,"bytes":3,"from":"PC","at":1,"pinned":false,"hash":"h"}]}"#).unwrap();
        let h = History::open(d.path());
        assert_eq!(h.entries().len(), 1);
        assert_eq!(h.content(1), Some(Content::Text("old".into())));
    }
    #[test] fn formatted_copy_keeps_its_html_and_the_limit_counts_it() { // Review Focus 3
        let (mut h, _d) = hist();
        let rich = Content::Rich { text: "bold".into(), html: "<b>bold</b>".into() };
        assert!(h.add("PC", prepare(&rich).unwrap(), 1));
        let again = History::open(h.dir.clone());
        assert_eq!(again.content(again.entries()[0].id), Some(rich));
        assert!(matches!(prepare(&Content::Rich { text: "a".into(), html: "x".repeat(MAX_TEXT_BYTES) }), Some(Prepared::Text { html: None, .. })), "text alone fits, so it is kept");
        assert!(prepare(&Content::Rich { text: "x".repeat(MAX_TEXT_BYTES + 1), html: "<b></b>".into() }).is_none());
    }
    #[test] fn oldest_unpinned_goes_at_51() {
        let (mut h, _d) = hist();
        for i in 0..51 { h.add("PC", text(&format!("clip {i}")), i); }
        assert_eq!(h.entries().len(), 50);
        assert_eq!(h.entries()[0].text.as_deref(), Some("clip 50"));
        assert_eq!(h.entries().last().unwrap().text.as_deref(), Some("clip 1"));
    }
    #[test] fn pins_survive_trimming() {
        let (mut h, _d) = hist();
        h.add("PC", text("keep me"), 0);
        let id = h.entries()[0].id;
        h.pin(id, true);
        for i in 0..60 { h.add("PC", text(&format!("n{i}")), i + 1); }
        assert_eq!(h.entries().iter().filter(|e| !e.pinned).count(), 50);
        assert!(h.entries().iter().any(|e| e.pinned && e.text.as_deref() == Some("keep me")));
    }
    #[test] fn duplicate_moves_to_top() {
        let (mut h, _d) = hist();
        h.add("PC", text("a"), 1);
        h.add("PC", text("b"), 2);
        h.add("iPad", text("a"), 3);
        let t: Vec<_> = h.entries().iter().map(|e| e.text.clone().unwrap()).collect();
        assert_eq!(t, ["a", "b"]);
        assert_eq!((h.entries()[0].from.as_str(), h.entries()[0].at), ("iPad", 3));
    }
    #[test] fn search_matches_text_case_insensitive() {
        let (mut h, _d) = hist();
        h.add("PC", text("Hello World"), 1);
        h.add("PC", text("other"), 2);
        assert_eq!(h.search("wORLD").len(), 1);
        assert_eq!(h.search("").len(), 2);
        assert!(h.search("zzz").is_empty());
    }
    #[test] fn corrupt_file_starts_empty_and_keeps_bak() { // Review Focus 4
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("history.json"), b"{not json").unwrap();
        let mut h = History::open(d.path());
        assert!(h.entries().is_empty());
        assert_eq!(fs::read(d.path().join("history.json.bak")).unwrap(), b"{not json");
        assert!(h.add("PC", text("fresh"), 1));
        assert_eq!(History::open(d.path()).entries().len(), 1);
    }
    #[test] fn unreadable_file_is_kept_aside_like_a_corrupt_one() { // Review Focus 4, non-parse read error
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("history.json")).unwrap(); // reading it fails, and it is not a parse error
        let mut h = History::open(d.path());
        assert!(h.entries().is_empty());
        assert!(d.path().join("history.json.bak").exists(), "moved aside");
        assert!(h.add("PC", text("fresh"), 1));
        assert_eq!(History::open(d.path()).entries().len(), 1);
    }
    #[test] fn file_that_cannot_be_moved_is_left_alone_and_nothing_is_saved() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("history.json")).unwrap();
        fs::create_dir_all(d.path().join("history.json.bak").join("x")).unwrap(); // blocks the move
        let mut h = History::open(d.path());
        assert!(h.add("PC", text("kept in memory"), 1));
        assert!(d.path().join("history.json").is_dir(), "original untouched");
        assert!(!d.path().join("history.json.tmp").exists());
    }
    #[test] fn pinned_images_never_push_the_total_over_the_cap() { // Review Focus 3
        let d = tempfile::tempdir().unwrap();
        let img = |n: u8| prepare(&Content::Image(Image { width: 1, height: 1, rgba: vec![n, 0, 0, 255] })).unwrap();
        let size = match img(1) { Prepared::Image { png, .. } => png.len() as u64, _ => unreachable!() };
        let mut h = History::open_with(d.path(), 1000, size * 2);
        for n in 1..=2 { assert!(h.add("PC", img(n), n as u64)); let id = h.entries()[0].id; h.pin(id, true); }
        assert!(!h.add("PC", img(3), 3), "no room even after evicting every unpinned image");
        assert_eq!(h.entries().len(), 2);
        let total: u64 = h.entries().iter().map(|e| e.bytes).sum();
        assert!(total <= size * 2);
        assert_eq!(fs::read_dir(d.path()).unwrap().filter(|f| f.as_ref().unwrap().file_name().to_string_lossy().ends_with(".png")).count(), 2);
    }
    #[test] fn clear_removes_pinned_entries_too() {
        let (mut h, _d) = hist();
        h.add("PC", text("pinned"), 1);
        let id = h.entries()[0].id;
        h.pin(id, true);
        h.clear();
        assert!(h.entries().is_empty());
    }
    #[test] fn oversize_image_is_skipped() { // Review Focus 3
        let (mut h, _d) = hist();
        let big = Prepared::Image { hash: "x".into(), width: 1, height: 1, png: vec![0; MAX_IMAGE_BYTES as usize + 1] };
        assert!(!h.add("PC", big, 1));
        assert!(h.entries().is_empty());
        assert!(prepare(&Content::Text("x".repeat(MAX_TEXT_BYTES + 1))).is_none());
    }
    #[test] fn images_are_files_and_total_is_capped() {
        let d = tempfile::tempdir().unwrap();
        let img = |n: u8| prepare(&Content::Image(Image { width: 1, height: 1, rgba: vec![n, 0, 0, 255] })).unwrap();
        let size = match img(1) { Prepared::Image { png, .. } => png.len() as u64, _ => unreachable!() };
        let mut h = History::open_with(d.path(), 1000, size * 2 + 1);
        for n in 1..=3 { h.add("PC", img(n), n as u64); }
        assert_eq!(h.entries().len(), 2);
        assert_eq!(fs::read_dir(d.path()).unwrap().filter(|f| f.as_ref().unwrap().file_name().to_string_lossy().ends_with(".png")).count(), 2);
        let id = h.entries()[0].id;
        assert!(matches!(h.content(id), Some(Content::Image(i)) if i.width == 1));
    }
    #[test] fn switch_off_stores_nothing_and_clear_empties() {
        let (mut h, _d) = hist();
        h.add("PC", text("a"), 1);
        h.set_enabled(false);
        assert!(!h.add("PC", text("b"), 2));
        h.clear();
        assert!(h.entries().is_empty());
        assert!(!History::open(h.dir.clone()).enabled());
    }
}
