//! Clipboard between every paired device, with no hub: the device where something was copied sends it to
//! each paired device itself, and receivers never forward. Duplicates (the PC's hub relaying what a phone
//! already sent straight to the iPad) and echoes (a received clip showing up as a local change) are
//! dropped here. Holds a hash and a time only, never the text.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

/// The same text again within this long is a duplicate or an echo, not a new copy.
pub const REPEAT_WINDOW_MS: u64 = 10_000;

fn digest(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

#[derive(Default)]
pub struct ClipMesh { last: Option<(u64, u64)> }

impl ClipMesh {
    fn repeat(&self, text: &str, now_ms: u64) -> bool {
        self.last.is_some_and(|(h, t)| h == digest(text) && now_ms.saturating_sub(t) < REPEAT_WINDOW_MS)
    }

    /// A clip arrived from a peer. True: put it on this device's clipboard. False: seen already.
    pub fn on_remote(&mut self, text: &str, now_ms: u64) -> bool {
        if text.is_empty() || self.repeat(text, now_ms) { return false }
        self.last = Some((digest(text), now_ms));
        true
    }

    /// This device's clipboard changed. True: send it to every paired peer. False: empty, our own echo
    /// of a received clip, or the same copy again.
    pub fn on_local(&mut self, text: &str, now_ms: u64) -> bool {
        if text.is_empty() || self.repeat(text, now_ms) {
            // Seen again: keep the window open from now, so a slow echo still matches.
            if !text.is_empty() { self.last = Some((digest(text), now_ms)) }
            return false;
        }
        self.last = Some((digest(text), now_ms));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn a_new_copy_goes_out_once() {
        let mut m = ClipMesh::default();
        assert!(m.on_local("notes", 0));
        assert!(!m.on_local("notes", 50), "the watcher firing twice sends once");
    }
    #[test] fn a_received_clip_does_not_echo_back() {
        let mut m = ClipMesh::default();
        assert!(m.on_remote("from phone", 0));
        assert!(!m.on_local("from phone", 200), "writing it to the clipboard is not a new copy");
    }
    #[test] fn the_same_clip_by_two_routes_lands_once() {
        let mut m = ClipMesh::default();
        assert!(m.on_remote("x", 0), "straight from the phone");
        assert!(!m.on_remote("x", 300), "relayed by the PC");
        assert!(m.on_remote("y", 400));
    }
    #[test] fn copying_the_same_text_later_is_a_new_copy() {
        let mut m = ClipMesh::default();
        assert!(m.on_local("again", 0));
        assert!(m.on_local("again", REPEAT_WINDOW_MS + 1));
        assert!(m.on_remote("again", 3 * REPEAT_WINDOW_MS));
    }
    #[test] fn empty_text_is_never_sent_or_applied() {
        let mut m = ClipMesh::default();
        assert!(!m.on_local("", 0));
        assert!(!m.on_remote("", 0));
    }
    #[test] fn a_slow_echo_still_matches() {
        let mut m = ClipMesh::default();
        assert!(m.on_remote("t", 0));
        assert!(!m.on_local("t", 9_000));
        assert!(!m.on_local("t", 18_000), "each sighting renews the window");
    }
}
