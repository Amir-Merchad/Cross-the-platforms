//! Which key/button events the hooks swallow while input goes to the remote.
//! Pure so it can be tested; the hooks hold one `KeyGate` each.

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    PassToWindows,
    Remote,
    /// Ctrl+Alt+Q (0, while remote) or Ctrl+Alt+1…n (in both states). The key is swallowed.
    Hotkey(u8),
    /// Swallowed and not forwarded: hotkey repeats/releases, repeats of a key held on Windows.
    Drop,
}

pub struct KeyGate {
    swallowed: [bool; 256],
    hotkey_down: [bool; 256],
    /// Pressed while local: its release belongs to Windows even if it arrives while remote.
    local_down: [bool; 256],
    ctrl: bool,
    lalt: bool,
    /// Right Alt. AltGr arrives as LCtrl + RAlt, so hotkeys require the left Alt and no right Alt.
    ralt: bool,
    hotkeys: u8,
}

impl Default for KeyGate { fn default() -> Self { Self::new() } }

impl KeyGate {
    pub const fn new() -> Self {
        Self { swallowed: [false; 256], hotkey_down: [false; 256], local_down: [false; 256], ctrl: false, lalt: false, ralt: false, hotkeys: 9 }
    }

    /// Only Ctrl+Alt+1…`n` are hotkeys (one per placed device); higher digits reach Windows.
    pub fn set_hotkey_count(&mut self, n: u8) { self.hotkeys = n.min(9); }

    /// `vk` is a virtual-key code; mouse buttons use VK_LBUTTON=1, VK_RBUTTON=2, VK_MBUTTON=4.
    pub fn on_key(&mut self, vk: u16, down: bool, suppress: bool) -> Verdict {
        // Track modifiers ourselves: GetAsyncKeyState isn't updated for swallowed keys.
        match vk { 0x11 | 0xA2 | 0xA3 => self.ctrl = down, 0x12 | 0xA4 => self.lalt = down, 0xA5 => self.ralt = down, _ => {} }
        let combo = self.ctrl && self.lalt && !self.ralt;
        let i = (vk & 0xFF) as usize;
        if down && self.hotkey_down[i] { return Verdict::Drop } // auto-repeat of a held hotkey
        if down && combo && (0x31..0x31 + self.hotkeys as u16).contains(&vk) {
            self.hotkey_down[i] = true;
            return Verdict::Hotkey((vk - 0x30) as u8);
        }
        // The release of a hotkey key never reaches Windows or the device.
        if !down && std::mem::take(&mut self.hotkey_down[i]) { return Verdict::Drop }
        if !suppress {
            self.local_down[i] = down;
            self.swallowed[i] = false;
            return Verdict::PassToWindows;
        }
        if down {
            // Auto-repeat of a key held before crossing: keep its release for Windows.
            if self.local_down[i] { return Verdict::Drop }
            self.swallowed[i] = true;
            if combo && vk == 0x51 { return Verdict::Hotkey(0) }
            Verdict::Remote
        } else if std::mem::take(&mut self.swallowed[i]) {
            Verdict::Remote
        } else {
            // pressed on Windows before crossing: Windows must see the release
            self.local_down[i] = false;
            Verdict::PassToWindows
        }
    }
}

/// Tells raw keyboard input the hook never saw (Windows skips LL hooks while our own window has focus)
/// from input the hook let through. A key the hook swallowed never reaches raw input at all.
pub struct HookWitness { seen: [[u64; 2]; 256] }

impl Default for HookWitness { fn default() -> Self { Self::new() } }

/// The hook runs before raw input is generated; anything older than this wasn't this event.
const WITNESS_WINDOW_MS: u64 = 100;

impl HookWitness {
    pub const fn new() -> Self { Self { seen: [[u64::MAX; 2]; 256] } }

    pub fn hook_saw(&mut self, vk: u16, down: bool, now_ms: u64) { self.seen[(vk & 0xFF) as usize][down as usize] = now_ms; }

    /// True when a device is in control and this raw key event never went through the hook.
    pub fn raw_was_skipped(&self, vk: u16, down: bool, now_ms: u64, suppress: bool) -> bool {
        if !suppress { return false }
        let t = self.seen[(vk & 0xFF) as usize][down as usize];
        t == u64::MAX || now_ms.saturating_sub(t) > WITNESS_WINDOW_MS
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
    // Final review I1: auto-repeat of a key held before crossing must not move its release to the device.
    #[test] fn repeat_of_locally_held_key_is_dropped_and_release_goes_to_windows() {
        let mut g = KeyGate::default();
        g.on_key(LSHIFT, true, false);
        assert_eq!(g.on_key(LSHIFT, true, true), Verdict::Drop, "repeat while remote");
        assert_eq!(g.on_key(LSHIFT, false, true), Verdict::PassToWindows);
    }
    // Final review I2: holding a hotkey must not fire it again on every repeat.
    #[test] fn hotkey_repeat_fires_once() {
        let mut g = KeyGate::default();
        g.on_key(LCTRL, true, false); g.on_key(LALT, true, false);
        assert_eq!(g.on_key(0x31, true, false), Verdict::Hotkey(1));
        assert_eq!(g.on_key(0x31, true, false), Verdict::Drop);
    }
    // Final review I3: AltGr arrives as LCtrl + RAlt and must keep typing characters.
    #[test] fn altgr_digit_is_not_a_hotkey() {
        let mut g = KeyGate::default();
        g.on_key(LCTRL, true, false); g.on_key(0xA5, true, false);
        assert_eq!(g.on_key(0x37, true, false), Verdict::PassToWindows, "AltGr+7 is a curly brace on German layouts");
    }
    #[test] fn altgr_q_goes_to_device_not_windows() {
        let mut g = KeyGate::default();
        g.on_key(LCTRL, true, true); g.on_key(0xA5, true, true);
        assert_eq!(g.on_key(Q, true, true), Verdict::Remote, "AltGr+Q is '@' on German layouts");
    }
    #[test] fn digit_beyond_placed_devices_is_not_swallowed() {
        let mut g = KeyGate::default();
        g.set_hotkey_count(2);
        g.on_key(LCTRL, true, false); g.on_key(LALT, true, false);
        assert_eq!(g.on_key(0x33, true, false), Verdict::PassToWindows);
    }
    // Hook-skip safety net: Windows skips our LL hook while our own window has focus; raw input still arrives.
    #[test] fn key_seen_by_hook_is_not_skipped() {
        let mut w = HookWitness::default();
        w.hook_saw(0x41, true, 1000);
        assert!(!w.raw_was_skipped(0x41, true, 1010, true));
    }
    #[test] fn key_unseen_while_remote_is_skipped() {
        assert!(HookWitness::default().raw_was_skipped(0x41, true, 1000, true));
    }
    #[test] fn key_unseen_while_local_is_left_alone() {
        assert!(!HookWitness::default().raw_was_skipped(0x41, true, 1000, false));
    }
    #[test] fn old_sighting_does_not_count() {
        let mut w = HookWitness::default();
        w.hook_saw(0x41, true, 0);
        assert!(w.raw_was_skipped(0x41, true, 500, true));
    }
    #[test] fn press_and_release_are_tracked_separately() {
        let mut w = HookWitness::default();
        w.hook_saw(0x41, true, 1000);
        assert!(w.raw_was_skipped(0x41, false, 1010, true));
    }
}
