//! What a German keyboard types on an iPad that reads every keyboard as US (proof 8 in
//! `docs/spikes/ipad-pointer.md`). Pure: `ipadhid::KeyTranslator` turns the answer into key events.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardLayout { #[default] Same, GermanToUs }

impl KeyboardLayout {
    /// Anything unknown, from a newer build say, changes nothing.
    pub fn parse(s: &str) -> Self { if s == "german-to-us" { Self::GermanToUs } else { Self::Same } }
}

/// One press on the iPad: left-side modifier bits and a usage.
pub type Chord = (u8, u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Goes down with the key and up with it, under the modifiers already held, so it repeats.
    Hold(u8),
    /// Pressed and released at once, under exactly these modifiers. Empty types nothing.
    Tap(&'static [Chord]),
}

const SHIFT: u8 = 0x02;
const OPTION: u8 = 0x04;

/// The dead key that puts umlauts on a US iPad.
const DIAERESIS: Chord = (OPTION, 0x18);

/// The chords for a German key at this level: 0 plain, 1 Shift, 2 AltGr. Keys marked the same as on
/// a US keyboard are left out. ² and ³ have no US sequence, so they type nothing.
fn german_key(usage: u8, level: u8) -> Option<&'static [Chord]> {
    Some(match (usage, level) {
        (0x34, 0) => &[DIAERESIS, (0, 0x04)],       // ä
        (0x34, 1) => &[DIAERESIS, (SHIFT, 0x04)],   // Ä
        (0x33, 0) => &[DIAERESIS, (0, 0x12)],       // ö
        (0x33, 1) => &[DIAERESIS, (SHIFT, 0x12)],   // Ö
        (0x2F, 0) => &[DIAERESIS, (0, 0x18)],       // ü
        (0x2F, 1) => &[DIAERESIS, (SHIFT, 0x18)],   // Ü
        (0x2D, 0) => &[(OPTION, 0x16)],             // ß
        (0x2D, 1) => &[(SHIFT, 0x38)],              // ?
        (0x2D, 2) => &[(0, 0x31)],                  // backslash
        (0x1F, 1) => &[(SHIFT, 0x34)],              // "
        (0x1F | 0x20, 2) => &[],                    // ² ³
        (0x20, 1) => &[(OPTION, 0x23)],             // §
        (0x23, 1) => &[(SHIFT, 0x24)],              // &
        (0x24, 1) => &[(0, 0x38)],                  // /
        (0x24, 2) => &[(SHIFT, 0x2F)],              // {
        (0x25, 1) => &[(SHIFT, 0x26)],              // (
        (0x25, 2) => &[(0, 0x2F)],                  // [
        (0x26, 1) => &[(SHIFT, 0x27)],              // )
        (0x26, 2) => &[(0, 0x30)],                  // ]
        (0x27, 1) => &[(0, 0x2E)],                  // =
        (0x27, 2) => &[(SHIFT, 0x30)],              // }
        (0x2E, 0) => &[(OPTION, 0x08)],             // ´, proof 8
        (0x2E, 1) => &[(0, 0x35)],                  // `
        (0x30, 0) => &[(SHIFT, 0x2E)],              // +
        (0x30, 1) => &[(SHIFT, 0x25)],              // *
        (0x30, 2) => &[(SHIFT, 0x35)],              // ~
        (0x31 | 0x32, 0) => &[(SHIFT, 0x20)],       // #
        (0x31 | 0x32, 1) => &[(0, 0x34)],           // '
        (0x35, 0) => &[(SHIFT, 0x23)],              // ^
        (0x35, 1) => &[(OPTION | SHIFT, 0x25)],     // °
        (0x36, 1) => &[(0, 0x33)],                  // ;
        (0x37, 1) => &[(SHIFT, 0x33)],              // :
        (0x38, 0) => &[(0, 0x2D)],                  // -
        (0x38, 1) => &[(SHIFT, 0x2D)],              // _
        (0x64, 0) => &[(SHIFT, 0x36)],              // <
        (0x64, 1) => &[(SHIFT, 0x37)],              // >
        (0x64, 2) => &[(SHIFT, 0x31)],              // |
        (0x14, 2) => &[(SHIFT, 0x1F)],              // @
        (0x08, 2) => &[(OPTION | SHIFT, 0x1F)],     // €
        (0x10, 2) => &[(OPTION, 0x10)],             // µ
        _ => return None,
    })
}

/// What a key at this German position does, given the modifiers held. Positions are real key
/// positions, the usages a German keyboard sends.
// ponytail: `hid::vk_to_hid` gives positions only while Windows itself is on US. With Windows on
// German the virtual keys differ (ü arrives as 0x33, Z as 0x1D). Carry the scan code if that is needed.
pub fn german_to_us(usage: u8, modifiers: u8) -> Press {
    let held = |bits: u8| modifiers & bits != 0;
    let (shift, altgr) = (held(0x22), held(0x40));
    let shortcut = held(0x88) || held(0x04) || (held(0x11) && !altgr);
    match usage {
        0x1C => return Press::Hold(0x1D),
        0x1D => return Press::Hold(0x1C),
        _ if shortcut => return Press::Hold(usage),
        _ => {}
    }
    let level = if altgr { 2 } else { shift as u8 };
    match german_key(usage, level) {
        None => Press::Hold(usage),
        Some(&[(mods, out)]) if !altgr && mods == if shift { SHIFT } else { 0 } => Press::Hold(out),
        Some(chords) => Press::Tap(chords),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hid::KeyboardReport, ipadhid::{KeyEvent::{self, Down, Up}, KeyTranslator}};

    const SHIFT: u8 = 0x02;
    const LCTRL: u8 = 0x01;
    const CMD: u8 = 0x08;
    const ALTGR: u8 = 0x40;
    const OPTION: u8 = 0xE2;
    const LSHIFT: u8 = 0xE1;
    const A: u8 = 0x04;
    const O: u8 = 0x12;
    const S: u8 = 0x16;
    const U: u8 = 0x18;
    const Y: u8 = 0x1C;
    const Z: u8 = 0x1D;
    const AE: u8 = 0x34;
    const OE: u8 = 0x33;
    const UE: u8 = 0x2F;
    const SZ: u8 = 0x2D;

    fn report(modifiers: u8, keys: &[u8]) -> KeyboardReport {
        let mut r = KeyboardReport { modifiers, keys: [0; 6] };
        r.keys[..keys.len()].copy_from_slice(keys);
        r
    }
    fn german() -> KeyTranslator {
        let mut t = KeyTranslator::default();
        assert!(t.set_layout(KeyboardLayout::GermanToUs).is_empty());
        t
    }
    /// Every usage that went down came up, in order, and nothing is left held.
    fn assert_balanced(t: &KeyTranslator, events: &[KeyEvent]) {
        let mut down = vec![];
        for e in events {
            match *e {
                Down(u) => { assert!(!down.contains(&u), "{u:#x} pressed twice"); down.push(u) }
                Up(u) => { assert!(down.contains(&u), "{u:#x} released but not down"); down.retain(|d| *d != u) }
            }
        }
        assert!(down.is_empty(), "still down: {down:x?}");
        assert!(t.is_empty());
    }
    /// The events one key makes, with these modifiers already down.
    fn typed(modifiers: u8, key: u8) -> Vec<KeyEvent> {
        let mut t = german();
        t.apply(&report(modifiers, &[]));
        t.apply(&report(modifiers, &[key]))
    }
    fn tap(mods: &[u8], key: u8) -> Vec<KeyEvent> {
        let mut e: Vec<_> = mods.iter().map(|m| Down(*m)).collect();
        e.extend([Down(key), Up(key)]);
        e.extend(mods.iter().rev().map(|m| Up(*m)));
        e
    }

    #[test] fn umlaut_becomes_dead_key_sequence() {
        for (key, vowel) in [(AE, A), (OE, O), (UE, U)] {
            let mut t = german();
            let e = t.apply(&report(0, &[key]));
            assert_eq!(e, [tap(&[OPTION], U), tap(&[], vowel)].concat());
            assert!(t.apply(&report(0, &[])).is_empty(), "the key's release sends nothing more");
            assert_balanced(&t, &e);
        }
    }
    #[test] fn sharp_s_is_option_s() {
        let mut t = german();
        assert_eq!(t.apply(&report(0, &[SZ])), tap(&[OPTION], S));
    }
    #[test] fn shift_gives_capital_umlaut() {
        let mut t = german();
        let mut e = t.apply(&report(SHIFT, &[]));
        assert_eq!(e, [Down(LSHIFT)]);
        let press = t.apply(&report(SHIFT, &[AE]));
        assert_eq!(press, [Up(LSHIFT), Down(OPTION), Down(U), Up(U), Up(OPTION), Down(LSHIFT), Down(A), Up(A)], "Option+u alone, then Shift+a");
        e.extend(press);
        e.extend(t.apply(&report(SHIFT, &[])));
        e.extend(t.apply(&report(0, &[])));
        assert_balanced(&t, &e);
    }
    #[test] fn right_shift_gives_capital_umlaut_too() {
        assert_eq!(typed(0x20, UE), [Up(0xE5), Down(OPTION), Down(U), Up(U), Up(OPTION), Down(LSHIFT), Down(U), Up(U), Up(LSHIFT), Down(0xE5)]);
    }
    #[test] fn z_and_y_swap() {
        let mut t = german();
        assert_eq!(t.apply(&report(0, &[Y])), [Down(Z)], "the German Z sits where US has Y");
        assert_eq!(t.apply(&report(0, &[])), [Up(Z)]);
        assert_eq!(t.apply(&report(0, &[Z])), [Down(Y)]);
        assert_eq!(t.apply(&report(0, &[])), [Up(Y)]);
        assert_eq!(typed(SHIFT, Y), [Down(Z)], "Shift stays held: capital Z, and it repeats");
    }
    #[test] fn shortcuts_keep_the_german_letter_and_nothing_else_moves() {
        assert_eq!(typed(CMD, Y), [Down(Z)], "Cmd+Z undoes from the key marked Z");
        assert_eq!(typed(CMD, AE), [Down(AE)]);
        assert_eq!(typed(LCTRL, 0x24), [Down(0x24)], "Ctrl+7 is not AltGr");
    }
    #[test] fn same_changes_nothing() {
        let mut same = KeyTranslator::default();
        assert!(same.set_layout(KeyboardLayout::Same).is_empty());
        let mut plain = KeyTranslator::default();
        for r in [report(SHIFT, &[]), report(SHIFT, &[AE]), report(ALTGR, &[Y, 0x14]), report(0, &[Z]), report(0, &[])] {
            assert_eq!(same.apply(&r), plain.apply(&r));
        }
        assert_eq!(same.apply(&report(0, &[Y])), [Down(Y)], "no swap");
        assert_eq!(KeyboardLayout::parse("same"), KeyboardLayout::Same);
        assert_eq!(KeyboardLayout::parse("klingon-to-us"), KeyboardLayout::Same);
        assert_eq!(KeyboardLayout::parse("german-to-us"), KeyboardLayout::GermanToUs);
    }
    #[test] fn symbols_land_where_the_german_key_shows_them() {
        let (sh, opt) = (LSHIFT, OPTION);
        // (held, German key, modifiers the iPad must see, usage it must see)
        let table: &[(u8, u8, &[u8], u8)] = &[
            (SHIFT, 0x1F, &[sh], 0x34),       // "
            (SHIFT, 0x20, &[opt], 0x23),      // section sign
            (SHIFT, 0x23, &[sh], 0x24),       // &
            (SHIFT, 0x24, &[], 0x38),         // /
            (SHIFT, 0x25, &[sh], 0x26),       // (
            (SHIFT, 0x26, &[sh], 0x27),       // )
            (SHIFT, 0x27, &[], 0x2E),         // =
            (SHIFT, 0x2D, &[sh], 0x38),       // ?
            (0, 0x2E, &[opt], 0x08),          // acute, proof 8
            (SHIFT, 0x2E, &[], 0x35),         // grave
            (0, 0x30, &[sh], 0x2E),           // +
            (SHIFT, 0x30, &[sh], 0x25),       // *
            (0, 0x31, &[sh], 0x20),           // #, as Windows reports the key
            (0, 0x32, &[sh], 0x20),           // #, as the ISO usage
            (SHIFT, 0x31, &[], 0x34),         // apostrophe
            (0, 0x35, &[sh], 0x23),           // ^
            (SHIFT, 0x35, &[opt, sh], 0x25),  // degree
            (SHIFT, 0x36, &[], 0x33),         // ;
            (SHIFT, 0x37, &[sh], 0x33),       // :
            (0, 0x38, &[], 0x2D),             // -
            (SHIFT, 0x38, &[sh], 0x2D),       // _
            (0, 0x64, &[sh], 0x36),           // <
            (SHIFT, 0x64, &[sh], 0x37),       // >
            (ALTGR, 0x14, &[sh], 0x1F),       // @
            (ALTGR, 0x08, &[opt, sh], 0x1F),  // euro
            (ALTGR, 0x10, &[opt], 0x10),      // micro
            (ALTGR, 0x24, &[sh], 0x2F),       // {
            (ALTGR, 0x25, &[], 0x2F),         // [
            (ALTGR, 0x26, &[], 0x30),         // ]
            (ALTGR, 0x27, &[sh], 0x30),       // }
            (ALTGR, 0x2D, &[], 0x31),         // backslash
            (ALTGR, 0x30, &[sh], 0x35),       // ~
            (ALTGR, 0x64, &[sh], 0x31),       // |
        ];
        for (m, key, mods, out) in table.iter().copied() {
            let e = typed(m, key);
            let at = e.iter().position(|x| *x == Down(out)).unwrap_or_else(|| panic!("key {key:#x} with {m:#x}: {e:x?}"));
            let mut on: Vec<u8> = (0..8).filter(|b| m & (1 << b) != 0).map(|b| 0xE0 + b).collect();
            for x in &e[..at] { match *x { Down(u) => on.push(u), Up(u) => on.retain(|d| *d != u) } }
            on.sort();
            let mut want = mods.to_vec();
            want.sort();
            assert_eq!(on, want, "modifiers under key {key:#x} with {m:#x}");
        }
    }
    #[test] fn windows_altgr_is_lctrl_and_right_alt() {
        assert_eq!(typed(LCTRL | ALTGR, 0x14), [Up(0xE0), Up(0xE6), Down(LSHIFT), Down(0x1F), Up(0x1F), Up(LSHIFT), Down(0xE0), Down(0xE6)]);
    }
    #[test] fn altgr_two_and_three_have_no_us_sequence() {
        assert!(typed(ALTGR, 0x1F).is_empty(), "superscript 2 types nothing");
        assert!(typed(ALTGR, 0x20).is_empty(), "superscript 3 types nothing");
    }
    #[test] fn keys_outside_the_table_pass_through() {
        assert_eq!(typed(0, A), [Down(A)]);
        assert_eq!(typed(SHIFT, 0x1E), [Down(0x1E)], "! is the same key");
        assert_eq!(typed(0, 0x28), [Down(0x28)], "Enter");
        assert_eq!(typed(ALTGR, A), [Down(A)]);
    }
    #[test] fn a_key_held_under_a_sequence_is_let_go_first() {
        let mut t = german();
        let mut e = t.apply(&report(0, &[A]));
        let press = t.apply(&report(0, &[A, AE]));
        assert_eq!(press, [vec![Up(A)], tap(&[OPTION], U), tap(&[], A)].concat(), "the a of the umlaut is really pressed");
        e.extend(press);
        e.extend(t.apply(&report(0, &[AE])));
        e.extend(t.apply(&report(0, &[])));
        assert_balanced(&t, &e);
    }
    #[test] fn a_modifier_released_with_the_key_held_leaves_nothing_down() {
        let mut t = german();
        let mut e = t.apply(&report(SHIFT, &[]));
        e.extend(t.apply(&report(SHIFT, &[0x24]))); // / lifts Shift for the press, then puts it back
        e.extend(t.apply(&report(0, &[0x24]))); // Shift up while the key is still down
        e.extend(t.apply(&report(0, &[])));
        assert_balanced(&t, &e);

        let mut t = german();
        let mut e = t.apply(&report(SHIFT, &[Y]));
        e.extend(t.apply(&report(0, &[Y])));
        e.extend(t.apply(&report(0, &[])));
        assert_eq!(e, [Down(LSHIFT), Down(Z), Up(LSHIFT), Up(Z)]);
        assert_balanced(&t, &e);
    }
    #[test] fn altgr_released_after_a_sequence_is_balanced() {
        let mut t = german();
        let mut e = t.apply(&report(LCTRL | ALTGR, &[]));
        e.extend(t.apply(&report(LCTRL | ALTGR, &[0x14])));
        e.extend(t.apply(&report(ALTGR, &[0x14])));
        e.extend(t.apply(&report(0, &[])));
        assert_balanced(&t, &e);
    }
    #[test] fn leaving_lets_go_of_everything() {
        let mut t = german();
        let mut e = t.apply(&report(SHIFT, &[AE, Y]));
        e.extend(t.release_all());
        assert_balanced(&t, &e);
        assert!(t.release_all().is_empty());
        assert!(t.apply(&report(0, &[])).is_empty(), "the router's zero report after leaving sends nothing");
    }
    #[test] fn a_sequence_does_not_fire_twice_from_a_repeated_snapshot() {
        let mut t = german();
        t.apply(&report(0, &[AE]));
        assert!(t.apply(&report(0, &[AE])).is_empty());
        assert_eq!(t.apply(&report(0, &[AE, A])), [Down(A)], "another key does not retype it");
    }
    #[test] fn changing_the_layout_lets_go_first() {
        let mut t = german();
        let mut e = t.apply(&report(SHIFT, &[Y]));
        e.extend(t.set_layout(KeyboardLayout::Same));
        assert_balanced(&t, &e);
        assert_eq!(t.apply(&report(0, &[Y])), [Down(Y)], "now unchanged");
        assert!(t.set_layout(KeyboardLayout::Same).is_empty(), "no change, nothing let go");
    }
}
