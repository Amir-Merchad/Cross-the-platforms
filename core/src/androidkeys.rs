//! Keyboard for Android over Wi-Fi (roadmap step 3): HID usages, as the router produces them, to Android
//! `KeyEvent` key codes and meta state, for a helper on the phone that injects key events (the scrcpy
//! route). Key codes are Android's public constants. No device has checked this table yet.

use crate::{hid::KeyboardReport, ipadhid::{KeyEvent, KeyTranslator}};

/// Android `KeyEvent.META_*` bits.
pub const META_SHIFT_ON: u32 = 0x1;
pub const META_ALT_ON: u32 = 0x2;
pub const META_CTRL_ON: u32 = 0x1000;
pub const META_META_ON: u32 = 0x10000;

/// HID keyboard usage → Android key code. None for keys Android has no code for.
pub fn keycode(usage: u8) -> Option<u32> {
    Some(match usage {
        0x04..=0x1D => 29 + (usage - 0x04) as u32,     // A..Z → KEYCODE_A (29)..KEYCODE_Z (54)
        0x1E..=0x26 => 8 + (usage - 0x1E) as u32,      // 1..9 → KEYCODE_1 (8)..KEYCODE_9 (16)
        0x27 => 7,                                     // 0
        0x28 => 66, 0x29 => 111, 0x2A => 67, 0x2B => 61, 0x2C => 62, // Enter, Esc, Backspace (DEL), Tab, Space
        0x2D => 69, 0x2E => 70, 0x2F => 71, 0x30 => 72, 0x31 => 73,  // - = [ ] \
        0x33 => 74, 0x34 => 75, 0x35 => 68, 0x36 => 55, 0x37 => 56, 0x38 => 76, // ; ' ` , . /
        0x39 => 115,                                   // Caps Lock
        0x3A..=0x45 => 131 + (usage - 0x3A) as u32,    // F1..F12 → 131..142
        0x46 => 120, 0x47 => 116, 0x48 => 121,         // Print Screen (SYSRQ), Scroll Lock, Pause (BREAK)
        0x49 => 124, 0x4A => 122, 0x4B => 92, 0x4C => 112, 0x4D => 123, 0x4E => 93, // Insert, Home, PgUp, Delete (FORWARD_DEL), End, PgDn
        0x4F => 22, 0x50 => 21, 0x51 => 20, 0x52 => 19, // Right, Left, Down, Up (DPAD)
        0x53 => 143,                                   // Num Lock
        0x54 => 154, 0x55 => 155, 0x56 => 156, 0x57 => 157, 0x58 => 160, // numpad / * - + Enter
        0x59..=0x61 => 145 + (usage - 0x59) as u32,    // numpad 1..9 → 145..153
        0x62 => 144, 0x63 => 158,                      // numpad 0, numpad .
        0x64 => 73,                                    // ISO extra key (German < >): backslash code, layout decides
        0x65 => 82,                                    // Application (menu)
        0xE0 => 113, 0xE1 => 59, 0xE2 => 57, 0xE3 => 117, // left Ctrl, Shift, Alt, Meta
        0xE4 => 114, 0xE5 => 60, 0xE6 => 58, 0xE7 => 118, // right Ctrl, Shift, Alt, Meta
        _ => return None,
    })
}

/// The meta state for HID modifier bits (bit 0 left Ctrl … bit 7 right GUI), left and right alike.
pub fn meta_state(modifiers: u8) -> u32 {
    let has = |bits: u8| modifiers & bits != 0;
    let mut m = 0;
    if has(0x11) { m |= META_CTRL_ON }
    if has(0x22) { m |= META_SHIFT_ON }
    if has(0x44) { m |= META_ALT_ON }
    if has(0x88) { m |= META_META_ON }
    m
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AndroidKey { pub down: bool, pub keycode: u32, pub meta: u32 }

/// Router keyboard snapshots → Android key events, with the meta state at the time of each event.
#[derive(Default)]
pub struct AndroidKeys { keys: KeyTranslator, modifiers: u8 }

impl AndroidKeys {
    pub fn apply(&mut self, r: &KeyboardReport) -> Vec<AndroidKey> {
        self.modifiers = r.modifiers;
        let meta = meta_state(r.modifiers);
        self.keys.apply(r).into_iter().filter_map(|e| Self::event(e, meta)).collect()
    }

    /// Lets go of everything held, for leaving the phone or a dropped link.
    pub fn release_all(&mut self) -> Vec<AndroidKey> {
        self.modifiers = 0;
        self.keys.release_all().into_iter().filter_map(|e| Self::event(e, 0)).collect()
    }

    fn event(e: KeyEvent, meta: u32) -> Option<AndroidKey> {
        let (down, usage) = match e { KeyEvent::Down(u) => (true, u), KeyEvent::Up(u) => (false, u) };
        keycode(usage).map(|keycode| AndroidKey { down, keycode, meta })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(modifiers: u8, k: u8) -> KeyboardReport { KeyboardReport { modifiers, keys: [k, 0, 0, 0, 0, 0] } }

    #[test] fn letters_digits_and_common_keys() {
        assert_eq!(keycode(0x04), Some(29)); // A
        assert_eq!(keycode(0x1D), Some(54)); // Z
        assert_eq!(keycode(0x1E), Some(8));  // 1
        assert_eq!(keycode(0x27), Some(7));  // 0
        assert_eq!(keycode(0x28), Some(66)); // Enter
        assert_eq!(keycode(0x2A), Some(67)); // Backspace
        assert_eq!(keycode(0x4C), Some(112)); // Delete
        assert_eq!(keycode(0x3A), Some(131)); // F1
        assert_eq!(keycode(0x45), Some(142)); // F12
        assert_eq!(keycode(0x52), Some(19)); // Up
        assert_eq!(keycode(0xFF), None);
    }
    #[test] fn every_key_the_router_can_send_has_a_code() {
        for vk in 0..=255u16 {
            if let Some(u) = crate::hid::vk_to_hid(vk) { assert!(keycode(u).is_some(), "usage {u:#x} from vk {vk:#x}") }
        }
    }
    #[test] fn modifiers_on_either_side_set_the_same_meta() {
        assert_eq!(meta_state(0x02), META_SHIFT_ON);
        assert_eq!(meta_state(0x20), META_SHIFT_ON);
        assert_eq!(meta_state(0x11), META_CTRL_ON);
        assert_eq!(meta_state(0x0C), META_ALT_ON | META_META_ON);
    }
    #[test] fn ctrl_c_goes_down_with_ctrl_held() {
        let mut k = AndroidKeys::default();
        assert_eq!(k.apply(&kb(0x01, 0)), vec![AndroidKey { down: true, keycode: 113, meta: META_CTRL_ON }]);
        assert_eq!(k.apply(&kb(0x01, 0x06)), vec![AndroidKey { down: true, keycode: 31, meta: META_CTRL_ON }]);
        assert_eq!(k.apply(&kb(0x01, 0)), vec![AndroidKey { down: false, keycode: 31, meta: META_CTRL_ON }]);
    }
    #[test] fn release_all_lets_go_once() {
        let mut k = AndroidKeys::default();
        k.apply(&kb(0x02, 0x04));
        assert_eq!(k.release_all(), vec![AndroidKey { down: false, keycode: 29, meta: 0 }, AndroidKey { down: false, keycode: 59, meta: 0 }]);
        assert!(k.release_all().is_empty());
    }
}
