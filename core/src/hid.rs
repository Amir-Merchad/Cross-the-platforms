//! HID report encoding for the BLE keyboard+mouse. Descriptor adapted from
//! Universal Bridge / windows-ble-hid (MIT, see core/THIRD_PARTY_NOTICES.md).

/// Composite descriptor: keyboard on report ID 1, mouse on report ID 2.
pub const REPORT_MAP: &[u8] = &[
    // Keyboard (Report ID 1): modifiers, reserved, 5 LED bits out, 6 key slots
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01,
    0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02,
    0x95, 0x01, 0x75, 0x08, 0x81, 0x01,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x05, 0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02,
    0x95, 0x01, 0x75, 0x03, 0x91, 0x01,
    0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x26, 0xDF, 0x00, 0x05, 0x07, 0x19, 0x00, 0x2A, 0xDF, 0x00,
    0x81, 0x00, 0xC0,
    // Mouse (Report ID 2): 3 buttons, dx/dy i16, wheel i8, AC pan i8
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x01, 0xA1, 0x00,
    0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02,
    0x95, 0x01, 0x75, 0x05, 0x81, 0x01,
    0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x16, 0x01, 0x80, 0x26, 0xFF, 0x7F, 0x75, 0x10, 0x95, 0x02,
    0x81, 0x06,
    0x09, 0x38, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01, 0x81, 0x06,
    0x05, 0x0C, 0x0A, 0x38, 0x02, 0x81, 0x06,
    0xC0, 0xC0,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseReport { pub buttons: u8, pub dx: i16, pub dy: i16, pub wheel: i8, pub pan: i8 }

impl MouseReport {
    pub fn to_bytes(&self) -> [u8; 7] {
        let [x0, x1] = self.dx.to_le_bytes();
        let [y0, y1] = self.dy.to_le_bytes();
        [self.buttons & 7, x0, x1, y0, y1, self.wheel as u8, self.pan as u8]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardReport { pub modifiers: u8, pub keys: [u8; 6] }

impl KeyboardReport {
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0] = self.modifiers;
        b[2..].copy_from_slice(&self.keys);
        b
    }
}

const MAX: i32 = 32767;

/// Splits a raw delta into reports that each fit the descriptor's i16 range and sum exactly.
pub fn split_delta(mut dx: i32, mut dy: i32) -> Vec<(i16, i16)> {
    let mut out = Vec::new();
    while dx != 0 || dy != 0 {
        let x = dx.clamp(-MAX, MAX);
        let y = dy.clamp(-MAX, MAX);
        out.push((x as i16, y as i16));
        dx -= x;
        dy -= y;
    }
    out
}

pub fn vk_to_modifier(vk: u16) -> Option<u8> {
    match vk {
        0xA2 => Some(0x01), 0xA0 => Some(0x02), 0xA4 => Some(0x04), 0x5B => Some(0x08),
        0xA3 => Some(0x10), 0xA1 => Some(0x20), 0xA5 => Some(0x40), 0x5C => Some(0x80),
        _ => None,
    }
}

/// Windows virtual-key → HID keyboard usage (layout-independent keys by VK).
pub fn vk_to_hid(vk: u16) -> Option<u8> {
    let u = match vk {
        0x41..=0x5A => 0x04 + (vk - 0x41) as u8,
        0x31..=0x39 => 0x1E + (vk - 0x31) as u8,
        0x30 => 0x27,
        0x70..=0x7B => 0x3A + (vk - 0x70) as u8,
        0x61..=0x69 => 0x59 + (vk - 0x61) as u8,
        0x0D => 0x28, 0x1B => 0x29, 0x08 => 0x2A, 0x09 => 0x2B, 0x20 => 0x2C,
        0xBD => 0x2D, 0xBB => 0x2E, 0xDB => 0x2F, 0xDD => 0x30, 0xDC => 0x31,
        0xBA => 0x33, 0xDE => 0x34, 0xC0 => 0x35, 0xBC => 0x36, 0xBE => 0x37, 0xBF => 0x38,
        0x14 => 0x39, 0x2C => 0x46, 0x91 => 0x47, 0x13 => 0x48, 0x2D => 0x49, 0x24 => 0x4A,
        0x21 => 0x4B, 0x2E => 0x4C, 0x23 => 0x4D, 0x22 => 0x4E, 0x27 => 0x4F, 0x25 => 0x50,
        0x28 => 0x51, 0x26 => 0x52, 0x90 => 0x53, 0x6F => 0x54, 0x6A => 0x55, 0x6D => 0x56,
        0x6B => 0x57, 0x60 => 0x62, 0x6E => 0x63, 0xE2 => 0x64, 0x5D => 0x65,
        _ => return None,
    };
    Some(u)
}

/// What is currently held on the remote side, so a switch or disconnect can release it all.
#[derive(Default)]
pub struct HeldInput { modifiers: u8, keys: [u8; 6], buttons: u8 }

impl HeldInput {
    fn report(&self) -> KeyboardReport { KeyboardReport { modifiers: self.modifiers, keys: self.keys } }

    pub fn key_down(&mut self, vk: u16) -> KeyboardReport {
        if let Some(m) = vk_to_modifier(vk) {
            self.modifiers |= m;
        } else if let Some(u) = vk_to_hid(vk)
            && !self.keys.contains(&u)
            // ponytail: keys beyond 6 are dropped, ErrorRollOver if a host ever needs it
            && let Some(slot) = self.keys.iter_mut().find(|k| **k == 0)
        {
            *slot = u;
        }
        self.report()
    }

    pub fn key_up(&mut self, vk: u16) -> KeyboardReport {
        if let Some(m) = vk_to_modifier(vk) {
            self.modifiers &= !m;
        } else if let Some(u) = vk_to_hid(vk) {
            for k in self.keys.iter_mut().filter(|k| **k == u) { *k = 0; }
        }
        self.report()
    }

    pub fn button(&mut self, mask: u8, down: bool) -> u8 {
        if down { self.buttons |= mask } else { self.buttons &= !mask }
        self.buttons
    }

    pub fn release_all(&mut self) -> (KeyboardReport, MouseReport) {
        *self = Self::default();
        (self.report(), MouseReport::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn split_small_delta_is_one_report() { assert_eq!(split_delta(5, -3), vec![(5, -3)]); }
    #[test] fn split_large_delta_sums_exactly() {
        let parts = split_delta(70_000, -40_000);
        assert!(parts.iter().all(|&(x, y)| (-32767..=32767).contains(&x) && (-32767..=32767).contains(&y)));
        assert_eq!(parts.iter().map(|p| p.0 as i32).sum::<i32>(), 70_000);
        assert_eq!(parts.iter().map(|p| p.1 as i32).sum::<i32>(), -40_000);
    }
    #[test] fn split_zero_is_empty() { assert!(split_delta(0, 0).is_empty()); }
    #[test] fn mouse_report_bytes() {
        assert_eq!(MouseReport { buttons: 1, dx: -1, dy: 2, wheel: -1, pan: 0 }.to_bytes(), [1, 0xFF, 0xFF, 2, 0, 0xFF, 0]);
    }
    #[test] fn keyboard_report_bytes() {
        assert_eq!(KeyboardReport { modifiers: 2, keys: [4, 0, 0, 0, 0, 0] }.to_bytes(), [2, 0, 4, 0, 0, 0, 0, 0]);
    }
    #[test] fn letter_a_maps_to_usage_4() { assert_eq!(vk_to_hid(0x41), Some(0x04)); }
    #[test] fn ctrl_is_modifier_not_key() { assert_eq!(vk_to_modifier(0xA2), Some(0x01)); assert_eq!(vk_to_hid(0xA2), None); }
    #[test] fn held_ctrl_a_then_release_all_is_zero() {
        let mut h = HeldInput::default();
        h.key_down(0xA2);
        let r = h.key_down(0x41);
        assert_eq!(r.modifiers, 0x01);
        assert_eq!(r.keys[0], 0x04);
        h.button(0x01, true);
        let (k, m) = h.release_all();
        assert_eq!(k.to_bytes(), [0; 8]);
        assert_eq!(m.to_bytes(), [0; 7]);
    }
    #[test] fn key_up_removes_key() {
        let mut h = HeldInput::default();
        h.key_down(0x41);
        assert_eq!(h.key_up(0x41).to_bytes(), [0; 8]);
    }
    #[test] fn seventh_key_is_ignored_not_panicking() {
        let mut h = HeldInput::default();
        for vk in 0x41..=0x47 { h.key_down(vk); }
        assert_eq!(h.key_down(0x48).keys.iter().filter(|&&k| k != 0).count(), 6);
    }
    #[test] fn report_map_declares_both_report_ids() {
        assert!(REPORT_MAP.windows(2).any(|w| w == [0x85, 1]));
        assert!(REPORT_MAP.windows(2).any(|w| w == [0x85, 2]));
    }
}
