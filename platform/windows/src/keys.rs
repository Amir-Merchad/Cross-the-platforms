//! Which key/button events the hooks swallow while input goes to the remote.
//! Pure so it can be tested; the hooks hold one `KeyGate` each.

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    PassToWindows,
    Remote,
    /// Ctrl+Alt+Q (0, while remote) or Ctrl+Alt+1…9 (in both states). The key is swallowed.
    Hotkey(u8),
}

pub struct KeyGate { swallowed: [bool; 256], hotkey_down: [bool; 256], ctrl: bool, alt: bool }

impl Default for KeyGate { fn default() -> Self { Self::new() } }

impl KeyGate {
    pub const fn new() -> Self { Self { swallowed: [false; 256], hotkey_down: [false; 256], ctrl: false, alt: false } }

    /// `vk` is a virtual-key code; mouse buttons use VK_LBUTTON=1, VK_RBUTTON=2, VK_MBUTTON=4.
    pub fn on_key(&mut self, vk: u16, down: bool, suppress: bool) -> Verdict {
        // Track modifiers ourselves: GetAsyncKeyState isn't updated for swallowed keys.
        match vk { 0x11 | 0xA2 | 0xA3 => self.ctrl = down, 0x12 | 0xA4 | 0xA5 => self.alt = down, _ => {} }
        let i = (vk & 0xFF) as usize;
        if down && self.ctrl && self.alt && (0x31..=0x39).contains(&vk) {
            self.hotkey_down[i] = true;
            return Verdict::Hotkey((vk - 0x30) as u8);
        }
        // The release of a hotkey key never reaches Windows or the device.
        if !down && std::mem::take(&mut self.hotkey_down[i]) { return Verdict::Remote }
        if !suppress { self.swallowed[i] = false; return Verdict::PassToWindows }
        if down {
            self.swallowed[i] = true;
            if self.ctrl && self.alt && vk == 0x51 { return Verdict::Hotkey(0) }
            Verdict::Remote
        } else if std::mem::take(&mut self.swallowed[i]) {
            Verdict::Remote
        } else {
            // pressed on Windows before crossing: Windows must see the release
            Verdict::PassToWindows
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LCTRL: u16 = 0xA2; const LALT: u16 = 0xA4; const LSHIFT: u16 = 0xA0; const Q: u16 = 0x51; const A: u16 = 0x41;

    #[test] fn passes_everything_when_not_suppressed() {
        let mut g = KeyGate::default();
        assert_eq!(g.on_key(A, true, false), Verdict::PassToWindows);
        assert_eq!(g.on_key(A, false, false), Verdict::PassToWindows);
    }
    #[test] fn ctrl_alt_q_escapes_while_suppressed() {
        let mut g = KeyGate::default();
        assert_eq!(g.on_key(LCTRL, true, true), Verdict::Remote);
        assert_eq!(g.on_key(LALT, true, true), Verdict::Remote);
        assert_eq!(g.on_key(Q, true, true), Verdict::Hotkey(0));
    }
    #[test] fn ctrl_alt_held_before_crossing_still_escape() {
        let mut g = KeyGate::default();
        g.on_key(LCTRL, true, false);
        g.on_key(LALT, true, false);
        assert_eq!(g.on_key(Q, true, true), Verdict::Hotkey(0));
    }
    #[test] fn q_alone_goes_remote() { let mut g = KeyGate::default(); assert_eq!(g.on_key(Q, true, true), Verdict::Remote); }
    #[test] fn key_held_before_crossing_releases_on_windows() {
        let mut g = KeyGate::default();
        g.on_key(LSHIFT, true, false);
        assert_eq!(g.on_key(LSHIFT, false, true), Verdict::PassToWindows);
    }
    #[test] fn key_pressed_on_remote_releases_on_remote() {
        let mut g = KeyGate::default();
        g.on_key(A, true, true);
        assert_eq!(g.on_key(A, false, true), Verdict::Remote);
    }
    #[test] fn swallowed_key_released_after_return_passes() {
        let mut g = KeyGate::default();
        g.on_key(A, true, true);
        assert_eq!(g.on_key(A, false, false), Verdict::PassToWindows);
        g.on_key(A, true, false);
        assert_eq!(g.on_key(A, false, true), Verdict::PassToWindows, "flag cleared after the up");
    }
    #[test] fn alt_released_after_escape_is_not_tracked_as_held() {
        let mut g = KeyGate::default();
        g.on_key(LCTRL, true, true); g.on_key(LALT, true, true); g.on_key(Q, true, true);
        g.on_key(LCTRL, false, false); g.on_key(LALT, false, false);
        assert_eq!(g.on_key(Q, true, true), Verdict::Remote);
    }
    #[test] fn ctrl_alt_digit_is_hotkey_when_local() {
        let mut g = KeyGate::default();
        g.on_key(0xA2, true, false); g.on_key(0xA4, true, false);
        assert_eq!(g.on_key(0x32, true, false), Verdict::Hotkey(2));
    }
    #[test] fn ctrl_alt_digit_is_hotkey_when_remote() {
        let mut g = KeyGate::default();
        g.on_key(0xA2, true, true); g.on_key(0xA4, true, true);
        assert_eq!(g.on_key(0x31, true, true), Verdict::Hotkey(1));
    }
    #[test] fn digit_alone_is_not_hotkey() { let mut g = KeyGate::default(); assert_eq!(g.on_key(0x31, true, false), Verdict::PassToWindows); }
    #[test] fn hotkey_key_up_is_swallowed() {
        let mut g = KeyGate::default();
        g.on_key(0xA2, true, false); g.on_key(0xA4, true, false); g.on_key(0x31, true, false);
        assert_ne!(g.on_key(0x31, false, false), Verdict::PassToWindows);
    }
    #[test] fn ctrl_alt_q_is_ignored_when_local() {
        let mut g = KeyGate::default();
        g.on_key(0xA2, true, false); g.on_key(0xA4, true, false);
        assert_eq!(g.on_key(0x51, true, false), Verdict::PassToWindows);
    }
}
