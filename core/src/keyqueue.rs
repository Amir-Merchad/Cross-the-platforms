//! Keyboard reports waiting for a host that's connected but hasn't subscribed to the keyboard
//! report yet (hosts subscribe to mouse and keyboard separately, keyboard sometimes seconds later).

use std::collections::{HashMap, VecDeque};

use crate::hid::KeyboardReport;

/// Reports older than this are dropped rather than typed late into whatever has focus by then.
const MAX_AGE_MS: u64 = 2000;
/// Per host. The oldest are dropped first, so the newest state (incl. the last release) survives.
const MAX_REPORTS: usize = 256;

#[derive(Default)]
pub struct KeyQueue { q: HashMap<String, VecDeque<(u64, KeyboardReport)>> }

impl KeyQueue {
    pub fn push(&mut self, host: &str, report: KeyboardReport, now_ms: u64) {
        let q = self.q.entry(host.to_string()).or_default();
        if q.len() == MAX_REPORTS { q.pop_front(); }
        q.push_back((now_ms, report));
    }

    /// Removes and returns this host's reports that are still fresh, oldest first.
    pub fn drain(&mut self, host: &str, now_ms: u64) -> Vec<KeyboardReport> {
        self.q.remove(host).map(|q| q.into_iter().filter(|(t, _)| now_ms.saturating_sub(*t) <= MAX_AGE_MS).map(|(_, r)| r).collect()).unwrap_or_default()
    }

    pub fn pending(&self, host: &str) -> usize { self.q.get(host).map_or(0, VecDeque::len) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::KeyboardReport;

    fn k(key: u8) -> KeyboardReport { KeyboardReport { modifiers: 0, keys: [key, 0, 0, 0, 0, 0] } }

    #[test] fn keeps_order_until_ready() {
        let mut q = KeyQueue::default();
        q.push("ipad", k(4), 0); q.push("ipad", k(0), 10); q.push("ipad", k(5), 20);
        assert_eq!(q.drain("ipad", 500), vec![k(4), k(0), k(5)]);
        assert!(q.drain("ipad", 600).is_empty());
    }
    #[test] fn drops_reports_older_than_2s() {
        let mut q = KeyQueue::default();
        q.push("ipad", k(4), 0); q.push("ipad", k(5), 1500);
        assert_eq!(q.drain("ipad", 2400), vec![k(5)]);
    }
    #[test] fn hosts_are_separate() {
        let mut q = KeyQueue::default();
        q.push("ipad", k(4), 0); q.push("s24", k(5), 0);
        assert_eq!(q.drain("s24", 10), vec![k(5)]);
        assert_eq!(q.pending("ipad"), 1);
    }
    #[test] fn bounded() {
        let mut q = KeyQueue::default();
        for i in 0..500 { q.push("ipad", k((i % 200) as u8), 0); }
        assert!(q.pending("ipad") <= 256);
    }
    #[test] fn ends_with_a_release_even_if_the_press_survives_but_its_release_was_bounded_away() {
        // If the cap cuts the tail, the host must not be left with a key held.
        let mut q = KeyQueue::default();
        for i in 0..300 { q.push("ipad", k(if i % 2 == 0 { 4 } else { 0 }), 0); }
        assert_eq!(q.drain("ipad", 10).last().unwrap().to_bytes(), [0; 8]);
    }
}
