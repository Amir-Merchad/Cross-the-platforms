//! Reports for the iPad's UniversalHID daemon over Wi-Fi, with no Bluetooth. The daemon ignores
//! descriptors and picks a fixed format by report ID: see `docs/spikes/ipad-pointer.md`.

use crate::hid::{KeyboardReport, MouseReport};

/// The one Wi-Fi iPad's host id, beside the Bluetooth host ids.
pub const IPAD_HOST: &str = "ipad-wifi";

/// Report 5: `05 dx dy`, eight zeros, `buttons`, seven zeros. Bit 0 primary, bit 1 secondary.
const POINTER_ID: u8 = 5;
const POINTER_LEN: usize = 19;
const BUTTONS_AT: usize = 11;
/// Report 7: `07 00 00 x y`, eight zeros. Signed 8-bit.
const SCROLL_ID: u8 = 7;
const SCROLL_LEN: usize = 13;
/// About 2 px per unit on the iPad; proof 1 suggests 16 per wheel notch as a start.
pub const SCROLL_PER_NOTCH: i32 = 16;

fn pointer(dx: i8, dy: i8, buttons: u8) -> Vec<u8> {
    let mut r = vec![0u8; POINTER_LEN];
    (r[0], r[1], r[2], r[BUTTONS_AT]) = (POINTER_ID, dx as u8, dy as u8, buttons);
    r
}

/// Motion split into signed 8-bit steps that sum exactly, each carrying `buttons` so a drag holds.
/// A report with no motion still yields one report, which is how button changes travel.
/// Only the low three button bits are sent, as the Bluetooth path does.
pub fn pointer_reports(r: &MouseReport) -> Vec<Vec<u8>> {
    let buttons = r.buttons & 7;
    let (mut dx, mut dy) = (r.dx as i32, r.dy as i32);
    let mut out = vec![];
    loop {
        let (x, y) = (dx.clamp(-128, 127), dy.clamp(-128, 127));
        out.push(pointer(x as i8, y as i8, buttons));
        dx -= x;
        dy -= y;
        if dx == 0 && dy == 0 { return out }
    }
}

/// Report 7. `pan` goes in byte 3 and `wheel` in byte 4. Negative `wheel` scrolls the page down,
/// the same sign as a Windows wheel.
pub fn scroll_report(wheel: i8, pan: i8) -> Vec<u8> {
    let mut r = vec![0u8; SCROLL_LEN];
    (r[0], r[3], r[4]) = (SCROLL_ID, pan as u8, wheel as u8);
    r
}

/// The scroll reports for a router report's wheel and pan, in notches, scaled for the iPad.
pub fn scroll_reports(r: &MouseReport) -> Vec<Vec<u8>> {
    if r.wheel == 0 && r.pan == 0 { return vec![] }
    let (mut v, mut h) = (r.wheel as i32 * SCROLL_PER_NOTCH, r.pan as i32 * SCROLL_PER_NOTCH);
    let mut out = vec![];
    while v != 0 || h != 0 {
        let (y, x) = (v.clamp(-127, 127), h.clamp(-127, 127));
        out.push(scroll_report(y as i8, x as i8));
        v -= y;
        h -= x;
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEvent { Down(u8), Up(u8) }

/// The iPad takes key down and up events, and the router sends snapshots of held keys. This compares
/// snapshots. Modifier bits become usages `0xE0` to `0xE7`.
#[derive(Default)]
pub struct KeyTranslator { held: Vec<u8> }

fn usages(r: &KeyboardReport) -> Vec<u8> {
    let mods = (0..8).filter(|b| r.modifiers & (1 << b) != 0).map(|b| 0xE0 + b);
    let keys = r.keys.iter().copied().filter(|k| *k != 0);
    let mut out: Vec<u8> = vec![];
    for u in mods.chain(keys) { if !out.contains(&u) { out.push(u) } }
    out
}

impl KeyTranslator {
    /// Releases first, then presses: modifiers before keys, so Shift is down before the letter.
    pub fn apply(&mut self, r: &KeyboardReport) -> Vec<KeyEvent> {
        let now = usages(r);
        let mut out: Vec<KeyEvent> = self.held.iter().filter(|u| !now.contains(u)).map(|u| KeyEvent::Up(*u)).collect();
        out.extend(now.iter().filter(|u| !self.held.contains(u)).map(|u| KeyEvent::Down(*u)));
        self.held = now;
        out
    }

    /// Lets go of everything held, keys before modifiers. A second call yields nothing.
    pub fn release_all(&mut self) -> Vec<KeyEvent> {
        let mut held = std::mem::take(&mut self.held);
        held.sort_by_key(|u| (*u >= 0xE0, *u));
        held.into_iter().map(KeyEvent::Up).collect()
    }

    pub fn is_empty(&self) -> bool { self.held.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn pointer_report_layout() {
        let r = pointer_reports(&MouseReport { dx: 5, dy: -3, buttons: 1, ..Default::default() });
        assert_eq!(r.len(), 1);
        let mut want = vec![0u8; 19];
        (want[0], want[1], want[2], want[11]) = (5, 5, 0xFD, 1);
        assert_eq!(r[0], want);
    }
    #[test] fn large_motion_splits_to_i8() {
        let r = pointer_reports(&MouseReport { dx: 300, dy: -200, buttons: 1, ..Default::default() });
        assert_eq!(r.iter().map(|b| b[1] as i8 as i32).sum::<i32>(), 300);
        assert_eq!(r.iter().map(|b| b[2] as i8 as i32).sum::<i32>(), -200);
        assert!(r.iter().all(|b| b.len() == 19 && b[11] == 1), "every step holds the button");
    }
    #[test] fn extreme_motion_sums_exactly() {
        let r = pointer_reports(&MouseReport { dx: i16::MIN, dy: i16::MAX, ..Default::default() });
        assert_eq!(r.iter().map(|b| b[1] as i8 as i32).sum::<i32>(), i16::MIN as i32);
        assert_eq!(r.iter().map(|b| b[2] as i8 as i32).sum::<i32>(), i16::MAX as i32);
    }
    #[test] fn a_button_change_alone_is_one_report() {
        let r = pointer_reports(&MouseReport { buttons: 2, ..Default::default() });
        assert_eq!(r.len(), 1);
        assert_eq!((r[0][1], r[0][2], r[0][11]), (0, 0, 2), "right click is bit 1");
    }
    #[test] fn scroll_report_layout() {
        assert_eq!(scroll_report(-16, 3), vec![7, 0, 0, 3, 0xF0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    #[test] fn one_notch_is_sixteen_units() {
        assert_eq!(scroll_reports(&MouseReport { wheel: -1, ..Default::default() }), vec![scroll_report(-16, 0)]);
        assert!(scroll_reports(&MouseReport { dx: 4, ..Default::default() }).is_empty());
    }
    #[test] fn many_notches_split_and_sum() {
        let r = scroll_reports(&MouseReport { wheel: 10, pan: -9, ..Default::default() });
        assert_eq!(r.iter().map(|b| b[4] as i8 as i32).sum::<i32>(), 160);
        assert_eq!(r.iter().map(|b| b[3] as i8 as i32).sum::<i32>(), -144);
    }
    #[test] fn press_and_release_yield_events() {
        let mut t = KeyTranslator::default();
        assert_eq!(t.apply(&KeyboardReport { modifiers: 0, keys: [4, 0, 0, 0, 0, 0] }), vec![KeyEvent::Down(4)]);
        assert_eq!(t.apply(&KeyboardReport::default()), vec![KeyEvent::Up(4)]);
    }
    #[test] fn modifier_bits_become_usages() {
        let mut t = KeyTranslator::default();
        assert_eq!(t.apply(&KeyboardReport { modifiers: 0x02, keys: [0; 6] }), vec![KeyEvent::Down(0xE1)]);
        assert_eq!(t.apply(&KeyboardReport { modifiers: 0x80, keys: [0; 6] }), vec![KeyEvent::Up(0xE1), KeyEvent::Down(0xE7)]);
    }
    #[test] fn shift_goes_down_before_the_letter() {
        let mut t = KeyTranslator::default();
        assert_eq!(t.apply(&KeyboardReport { modifiers: 0x02, keys: [5, 0, 0, 0, 0, 0] }), vec![KeyEvent::Down(0xE1), KeyEvent::Down(5)]);
    }
    #[test] fn unchanged_snapshot_yields_nothing() {
        let mut t = KeyTranslator::default();
        let r = KeyboardReport { modifiers: 1, keys: [4, 0, 0, 0, 0, 0] };
        t.apply(&r);
        assert!(t.apply(&r).is_empty(), "no repeat from Bluetooth-style resends");
    }
    #[test] fn release_all_lets_go_of_everything() { // Review Focus 5
        let mut t = KeyTranslator::default();
        t.apply(&KeyboardReport { modifiers: 0x02, keys: [4, 0, 0, 0, 0, 0] });
        assert_eq!(t.release_all(), vec![KeyEvent::Up(4), KeyEvent::Up(0xE1)]);
        assert!(t.release_all().is_empty());
        assert!(t.is_empty());
        assert_eq!(t.apply(&KeyboardReport::default()), vec![], "a zero report after release sends nothing twice");
    }
}
