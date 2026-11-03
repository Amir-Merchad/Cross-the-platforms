//! Clipboard sync rules, shared by every app: which endpoints an item goes to, and when it doesn't.
//! Items are compared by content hash, so an endpoint reporting back what we just wrote to it is
//! dropped (no bouncing), and the newest copy anywhere wins. Contents are never logged.

use std::collections::{HashMap, VecDeque};

use sha2::{Digest, Sha256};

/// Writes per endpoint whose echo we still expect. Enough for an app that sets the clipboard several
/// times in a row; an echo that never comes just ages out.
const PENDING_ECHOES: usize = 4;

/// Items bigger than this (text bytes, or width × height × 4 for images) are not synced.
pub const MAX_ITEM_BYTES: usize = 64 * 1024 * 1024;

/// Canonical image: RGBA8, row-major, so the same picture hashes the same whatever it was encoded as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content { Text(String), Image(Image) }

impl Content {
    pub fn size(&self) -> usize {
        match self { Content::Text(t) => t.len(), Content::Image(i) => i.rgba.len() }
    }
    pub fn kind(&self) -> &'static str {
        match self { Content::Text(_) => "text", Content::Image(_) => "image" }
    }
    fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        match self {
            Content::Text(t) => { h.update([0]); h.update(t.as_bytes()); }
            Content::Image(i) => { h.update([1]); h.update(i.width.to_le_bytes()); h.update(i.height.to_le_bytes()); h.update(&i.rgba); }
        }
        h.finalize().into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endpoint { Windows, Ipad, Peer(String) }

#[derive(Clone, Debug, PartialEq)]
pub struct Item { pub id: u64, pub origin: Endpoint, pub content: Content }

pub struct Hub {
    endpoints: Vec<Endpoint>,
    last: Option<[u8; 32]>,
    /// Hashes written to each endpoint whose echo hasn't come back yet: a late echo of an older write
    /// must not overwrite a newer copy elsewhere.
    pending: HashMap<Endpoint, VecDeque<[u8; 32]>>,
    next_id: u64,
    paused: bool,
    skipped: u64,
}

impl Hub {
    pub fn new(endpoints: Vec<Endpoint>) -> Self { Self { endpoints, last: None, pending: HashMap::new(), next_id: 1, paused: false, skipped: 0 } }

    /// Paired phones come and go; the fixed endpoints stay.
    pub fn set_endpoints(&mut self, endpoints: Vec<Endpoint>) {
        self.pending.retain(|e, _| endpoints.contains(e));
        self.endpoints = endpoints;
    }

    pub fn set_paused(&mut self, p: bool) { self.paused = p; }

    pub fn skipped_too_large(&self) -> u64 { self.skipped }

    /// `from` reported a clipboard change; returns what to write where.
    pub fn observe(&mut self, from: Endpoint, content: Content) -> Vec<(Endpoint, Item)> {
        if matches!(&content, Content::Text(t) if t.is_empty()) { return Vec::new() }
        let hash = content.hash();
        if let Some(q) = self.pending.get_mut(&from) && let Some(i) = q.iter().position(|h| *h == hash) {
            q.remove(i);
            return Vec::new();
        }
        if self.last == Some(hash) { return Vec::new() }
        // Seen even while paused, so resuming never replays it.
        self.last = Some(hash);
        if self.paused { return Vec::new() }
        if content.size() > MAX_ITEM_BYTES { self.skipped += 1; return Vec::new() }
        let item = Item { id: self.next_id, origin: from.clone(), content };
        self.next_id += 1;
        let targets: Vec<Endpoint> = self.endpoints.iter().filter(|e| **e != from).cloned().collect();
        for e in &targets {
            let q = self.pending.entry(e.clone()).or_default();
            if q.len() == PENDING_ECHOES { q.pop_front(); }
            q.push_back(hash);
        }
        targets.into_iter().map(|e| (e, item.clone())).collect()
    }
}

/// PNG/JPEG/TIFF ⇄ canonical `Image`. Lossless both ways for PNG, so our own PNG echoing back hashes the same.
pub mod codec {
    use super::Image;

    pub fn encode_png(img: &Image) -> Result<Vec<u8>, String> {
        let buf = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).ok_or("image size doesn't match its pixels")?;
        let mut out = Vec::new();
        buf.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Image, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
        Ok(Image { width: img.width(), height: img.height(), rgba: img.into_raw() })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample() -> Image { Image { width: 3, height: 2, rgba: (0..24).map(|i| (i * 10) as u8).collect() } }
        #[test] fn png_round_trip_is_identical() { // Review Focus 1: echo of our own PNG hashes the same
            assert_eq!(decode(&encode_png(&sample()).unwrap()).unwrap(), sample());
        }
        #[test] fn decodes_jpeg_to_rgba() {
            let mut jpg = Vec::new();
            image::RgbImage::new(4, 4).write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Jpeg).unwrap();
            let img = decode(&jpg).unwrap();
            assert_eq!((img.width, img.height, img.rgba.len()), (4, 4, 64));
        }
        #[test] fn garbage_is_an_error_not_a_panic() { assert!(decode(b"not an image").is_err()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> Hub { Hub::new(vec![Endpoint::Windows, Endpoint::Ipad]) }
    fn t(s: &str) -> Content { Content::Text(s.into()) }

    #[test] fn windows_copy_goes_to_ipad_only() {
        let out = hub().observe(Endpoint::Windows, t("hi"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, Endpoint::Ipad);
        assert_eq!(out[0].1.origin, Endpoint::Windows);
        assert_eq!(out[0].1.content, t("hi"));
    }
    #[test] fn echo_from_target_is_dropped() { // Review Focus 1
        let mut h = hub();
        h.observe(Endpoint::Windows, t("hi"));
        assert!(h.observe(Endpoint::Ipad, t("hi")).is_empty());
    }
    #[test] fn same_content_twice_is_sent_once() {
        let mut h = hub();
        h.observe(Endpoint::Windows, t("a"));
        assert!(h.observe(Endpoint::Windows, t("a")).is_empty());
    }
    #[test] fn last_writer_wins_and_a_can_come_back() {
        let mut h = hub();
        h.observe(Endpoint::Windows, t("a"));
        assert_eq!(h.observe(Endpoint::Ipad, t("b"))[0].0, Endpoint::Windows);
        assert_eq!(h.observe(Endpoint::Windows, t("a"))[0].0, Endpoint::Ipad, "a is new again after b");
    }
    #[test] fn ids_increase() {
        let mut h = hub();
        let a = h.observe(Endpoint::Windows, t("a"))[0].1.id;
        let b = h.observe(Endpoint::Windows, t("b"))[0].1.id;
        assert!(b > a);
    }
    #[test] fn empty_text_is_ignored() { assert!(hub().observe(Endpoint::Windows, t("")).is_empty()); }
    #[test] fn too_large_is_skipped_and_counted() { // Review Focus 3
        let mut h = hub();
        let big = Content::Image(Image { width: 4097, height: 4097, rgba: vec![0; 4097 * 4097 * 4] });
        assert!(h.observe(Endpoint::Windows, big).is_empty());
        assert_eq!(h.skipped_too_large(), 1);
    }
    #[test] fn paused_syncs_nothing_and_resume_replays_nothing() { // Review Focus 5
        let mut h = hub();
        h.set_paused(true);
        assert!(h.observe(Endpoint::Windows, t("secret")).is_empty());
        h.set_paused(false);
        assert!(h.observe(Endpoint::Ipad, t("secret")).is_empty(), "copied while paused counts as seen");
        assert_eq!(h.observe(Endpoint::Windows, t("next")).len(), 1);
    }
    #[test] fn rapid_copies_dont_undo_the_newest() { // Final review I1
        let mut h = hub();
        assert_eq!(h.observe(Endpoint::Windows, t("A")).len(), 1);
        assert_eq!(h.observe(Endpoint::Windows, t("B")).len(), 1);
        assert!(h.observe(Endpoint::Ipad, t("A")).is_empty(), "late echo of A must not overwrite B on Windows");
        assert!(h.observe(Endpoint::Ipad, t("B")).is_empty());
    }
    #[test] fn three_endpoints_fan_out_to_both_others() {
        let mut h = Hub::new(vec![Endpoint::Windows, Endpoint::Ipad, Endpoint::Peer("s24".into())]);
        let to: Vec<_> = h.observe(Endpoint::Ipad, t("x")).into_iter().map(|(e, _)| e).collect();
        assert_eq!(to, vec![Endpoint::Windows, Endpoint::Peer("s24".into())]);
    }
    #[test] fn phones_can_join_later() {
        let mut h = Hub::new(vec![Endpoint::Windows, Endpoint::Ipad]);
        h.set_endpoints(vec![Endpoint::Windows, Endpoint::Ipad, Endpoint::Peer("s24".into())]);
        let to: Vec<_> = h.observe(Endpoint::Windows, t("y")).into_iter().map(|(e, _)| e).collect();
        assert_eq!(to, vec![Endpoint::Ipad, Endpoint::Peer("s24".into())]);
        h.set_endpoints(vec![Endpoint::Windows, Endpoint::Ipad]);
        let to: Vec<_> = h.observe(Endpoint::Windows, t("z")).into_iter().map(|(e, _)| e).collect();
        assert_eq!(to, vec![Endpoint::Ipad]);
    }
}
