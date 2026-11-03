//! Input routing: decides when local input goes to a device and turns it into HID reports.
//! Pure state machine; the Windows engine feeds it captured input and executes its actions.

use std::collections::HashMap;

use crate::{
    edge::{EdgeEvent, EdgeTracker},
    hid::{HeldInput, KeyboardReport, MouseReport, split_delta},
    layout::{Attachment, Edge, Monitor, Rect, entry, side},
    settings::Settings,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// Raw mouse delta plus the local cursor position (used while local for edge entry).
    Move { dx: i32, dy: i32, cursor: (i32, i32) },
    Button { mask: u8, down: bool },
    Wheel { v: i8, h: i8 },
    Key { vk: u16, down: bool },
    /// 0 = back to Windows, 1..=9 = n-th attachment in settings order.
    Hotkey(u8),
    /// Esc ×3 emergency return.
    Escape,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Suppress(bool),
    Mouse { host: String, report: MouseReport },
    Keyboard { host: String, report: KeyboardReport },
    PlaceCursor(i32, i32),
    Active(Option<String>),
    Notice(String),
}

/// Pacing used until the engine reports the host's real connection interval.
const DEFAULT_INTERVAL_MS: u64 = 8;
/// How far inside the edge the Windows cursor lands on return.
const RETURN_INSET: i32 = 20;

struct Remote {
    attachment: Attachment,
    rect: Option<Rect>,
    by_edge: bool,
    tracker: EdgeTracker,
    held: HeldInput,
    pending: (i32, i32),
    next_due: u64,
}

pub struct Router {
    settings: Settings,
    monitors: Vec<Monitor>,
    intervals: HashMap<String, u64>,
    /// (device id, since ms) while the cursor pushes into an attached edge.
    hold: Option<(String, u64)>,
    remote: Option<Remote>,
    cursor: (i32, i32),
}

impl Router {
    pub fn new(settings: Settings, monitors: Vec<Monitor>) -> Self {
        Self { settings, monitors, intervals: HashMap::new(), hold: None, remote: None, cursor: (0, 0) }
    }
    pub fn set_settings(&mut self, s: Settings) { self.settings = s; self.hold = None; }
    pub fn set_monitors(&mut self, m: Vec<Monitor>) { self.monitors = m; self.hold = None; }
    pub fn set_interval(&mut self, host: &str, ms: u64) { self.intervals.insert(host.to_string(), ms); }
    pub fn active(&self) -> Option<&str> { self.remote.as_ref().map(|r| r.attachment.device_id.as_str()) }
    pub fn next_tick_ms(&self) -> Option<u64> {
        self.remote.as_ref().filter(|r| r.pending != (0, 0)).map(|r| r.next_due)
    }

    pub fn on_input(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        match input {
            Input::Hotkey(0) | Input::Escape => self.leave(),
            Input::Hotkey(n) => self.hotkey(n, now_ms, connected),
            _ if self.remote.is_none() => self.local(input, now_ms, connected),
            _ => self.remote_input(input, now_ms),
        }
    }

    pub fn tick(&mut self, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Some(r) = &self.remote else { return Vec::new() };
        if !connected(&r.attachment.device_id) { return self.leave() }
        if r.pending == (0, 0) || now_ms < r.next_due { return Vec::new() }
        let interval = self.interval_for(&r.attachment.device_id);
        let r = self.remote.as_mut().unwrap();
        r.next_due = now_ms + interval;
        flush(r)
    }

    fn interval_for(&self, id: &str) -> u64 { self.intervals.get(id).copied().unwrap_or(DEFAULT_INTERVAL_MS) }

    fn local(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Input::Move { dx, dy, cursor } = input else { return Vec::new() };
        self.cursor = cursor;
        let target = entry(&self.settings.attachments, &self.monitors, cursor, dx, dy)
            .filter(|a| connected(&a.device_id))
            .cloned();
        let Some(a) = target else { self.hold = None; return Vec::new() };
        match &self.hold {
            Some((id, since)) if *id == a.device_id => {
                if now_ms.saturating_sub(*since) >= self.settings.entry_hold_ms { self.enter(a, true, now_ms) } else { Vec::new() }
            }
            _ => { self.hold = Some((a.device_id, now_ms)); Vec::new() }
        }
    }

    fn hotkey(&mut self, n: u8, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Some(a) = self.settings.attachments.get(n as usize - 1).cloned() else {
            return vec![Action::Notice(format!("no device on hotkey {n}"))];
        };
        if !connected(&a.device_id) { return vec![Action::Notice(format!("{} is not connected", a.device_id))] }
        let mut out = if self.remote.is_some() { self.leave() } else { Vec::new() };
        out.extend(self.enter(a, false, now_ms));
        out
    }

    fn enter(&mut self, a: Attachment, by_edge: bool, now_ms: u64) -> Vec<Action> {
        self.hold = None;
        let cal = self.settings.calibration_for(&a.device_id);
        let mut tracker = EdgeTracker::new(side(a.edge), self.settings.width_for(&a.device_id), cal.gain, cal.min_push, cal.dwell_ms);
        tracker.enter(now_ms);
        let host = a.device_id.clone();
        let vertical = matches!(a.edge, Edge::Top | Edge::Bottom);
        let pin = tracker.pin_delta();
        let rect = self.monitors.iter().find(|m| m.id == a.monitor_id).map(|m| m.rect);
        let mut out = vec![Action::Suppress(true)];
        let (px, py) = if vertical { (0, pin) } else { (pin, 0) };
        out.extend(split_delta(px, py).into_iter().map(|(dx, dy)| Action::Mouse { host: host.clone(), report: MouseReport { dx, dy, ..Default::default() } }));
        out.push(Action::Active(Some(host)));
        self.remote = Some(Remote { attachment: a, rect, by_edge, tracker, held: HeldInput::default(), pending: (0, 0), next_due: now_ms });
        out
    }

    fn remote_input(&mut self, input: Input, now_ms: u64) -> Vec<Action> {
        let r = self.remote.as_mut().unwrap();
        let host = r.attachment.device_id.clone();
        match input {
            Input::Move { dx, dy, .. } => {
                let axis = if matches!(r.attachment.edge, Edge::Top | Edge::Bottom) { dy } else { dx };
                if r.tracker.on_move(axis, now_ms) == EdgeEvent::Return { return self.leave() }
                r.pending.0 += dx;
                r.pending.1 += dy;
                Vec::new()
            }
            Input::Button { mask, down } => {
                let mut out = flush(r);
                let buttons = r.held.button(mask, down);
                out.push(Action::Mouse { host, report: MouseReport { buttons, ..Default::default() } });
                out
            }
            Input::Wheel { v, h } => {
                let mut out = flush(r);
                out.push(Action::Mouse { host, report: MouseReport { buttons: r.held.buttons(), wheel: v, pan: h, ..Default::default() } });
                out
            }
            Input::Key { vk, down } => {
                let mut out = flush(r);
                let report = if down { r.held.key_down(vk) } else { r.held.key_up(vk) };
                out.push(Action::Keyboard { host, report });
                out
            }
            Input::Hotkey(_) | Input::Escape => unreachable!("handled in on_input"),
        }
    }

    /// Windows input first, then release everything held on the device.
    fn leave(&mut self) -> Vec<Action> {
        let Some(mut r) = self.remote.take() else { return Vec::new() };
        let host = r.attachment.device_id.clone();
        let (k, m) = r.held.release_all();
        let (x, y) = self.cursor;
        let place = match (r.by_edge, r.rect) {
            (true, Some(rect)) => match r.attachment.edge {
                Edge::Right => (rect.right - 1 - RETURN_INSET, y),
                Edge::Left => (rect.left + RETURN_INSET, y),
                Edge::Top => (x, rect.top + RETURN_INSET),
                Edge::Bottom => (x, rect.bottom - 1 - RETURN_INSET),
            },
            _ => (x, y),
        };
        vec![
            Action::Suppress(false),
            Action::Keyboard { host: host.clone(), report: k },
            Action::Mouse { host, report: m },
            Action::PlaceCursor(place.0, place.1),
            Action::Active(None),
        ]
    }
}

/// Emits pending motion as reports (with held buttons, so drags keep the button down).
fn flush(r: &mut Remote) -> Vec<Action> {
    let (dx, dy) = std::mem::take(&mut r.pending);
    let buttons = r.held.buttons();
    let host = &r.attachment.device_id;
    split_delta(dx, dy).into_iter()
        .map(|(dx, dy)| Action::Mouse { host: host.clone(), report: MouseReport { buttons, dx, dy, ..Default::default() } })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Attachment, Edge, Monitor, Rect};
    use crate::settings::Settings;

    fn r() -> Router {
        let mut s = Settings::default();
        s.attachments = vec![
            Attachment { device_id: "ipad".into(), monitor_id: "M1".into(), edge: Edge::Right },
            Attachment { device_id: "s24".into(), monitor_id: "M1".into(), edge: Edge::Left },
        ];
        Router::new(s, vec![Monitor { id: "M1".into(), rect: Rect { left: 0, top: 0, right: 1920, bottom: 1080 } }])
    }
    fn up(_: &str) -> bool { true }
    fn down(_: &str) -> bool { false }

    #[test] fn hold_at_edge_enters_after_300ms() {
        let mut r = r();
        assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up).is_empty());
        let a = r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 300, &up);
        assert_eq!(a.first(), Some(&Action::Suppress(true)));
        assert!(a.contains(&Action::Active(Some("ipad".into()))));
        assert_eq!(r.active(), Some("ipad"));
    }
    #[test] fn leaving_edge_resets_hold() {
        let mut r = r();
        r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up);
        r.on_input(Input::Move { dx: -3, dy: 0, cursor: (1900, 500) }, 100, &up);
        assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 350, &up).is_empty());
    }
    #[test] fn edge_to_disconnected_device_does_not_enter() {
        let mut r = r();
        r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &down);
        let a = r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 400, &down);
        assert!(!a.contains(&Action::Suppress(true)));
        assert_eq!(r.active(), None);
    }
    #[test] fn hotkey_enters_connected_device() {
        let mut r = r();
        let a = r.on_input(Input::Hotkey(2), 0, &up);
        assert!(a.contains(&Action::Active(Some("s24".into()))));
    }
    #[test] fn hotkey_for_disconnected_device_stays_local() { // Review Focus 3
        let mut r = r();
        let a = r.on_input(Input::Hotkey(1), 0, &down);
        assert!(!a.contains(&Action::Suppress(true)));
        assert!(matches!(a.as_slice(), [Action::Notice(_)]));
        assert_eq!(r.active(), None);
    }
    #[test] fn escape_leaves_with_suppress_false_first() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
        r.on_input(Input::Key { vk: 0xA0, down: true }, 10, &up); // Shift held on remote
        let a = r.on_input(Input::Escape, 20, &up);
        assert_eq!(a.first(), Some(&Action::Suppress(false)));
        assert!(a.iter().any(|x| matches!(x, Action::Keyboard { report, .. } if report.to_bytes() == [0; 8])));
        assert!(a.contains(&Action::Active(None)));
    }
    #[test] fn motion_is_paced_by_interval() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up); r.set_interval("ipad", 15);
        for t in 1..=10 { r.on_input(Input::Move { dx: 1, dy: 0, cursor: (0, 0) }, 2000 + t, &up); }
        let a = r.tick(2016, &up);
        let moves: Vec<_> = a.iter().filter_map(|x| match x { Action::Mouse { report, .. } => Some(report.dx), _ => None }).collect();
        assert_eq!(moves, vec![10]); // coalesced into one report
        assert!(r.tick(2020, &up).is_empty(), "next report not before 2031");
    }
    #[test] fn disconnect_on_tick_leaves() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
        let a = r.tick(100, &down);
        assert_eq!(a.first(), Some(&Action::Suppress(false)));
        assert_eq!(r.active(), None);
    }
    #[test] fn click_flushes_pending_motion_first() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
        r.on_input(Input::Move { dx: 5, dy: 0, cursor: (0, 0) }, 2000, &up);
        let a = r.on_input(Input::Button { mask: 1, down: true }, 2001, &up);
        let kinds: Vec<_> = a.iter().filter_map(|x| match x { Action::Mouse { report, .. } => Some((report.dx, report.buttons)), _ => None }).collect();
        assert_eq!(kinds, vec![(5, 0), (0, 1)]);
    }
    #[test] fn deliberate_push_back_returns() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up); // ipad on Right → its Left edge faces Windows
        let mut left = false;
        for i in 0..40u64 {
            let a = r.on_input(Input::Move { dx: -5, dy: 0, cursor: (0, 0) }, 2000 + i * 10, &up);
            a.iter().for_each(|x| left |= *x == Action::Active(None));
            r.tick(2000 + i * 10, &up);
        }
        assert!(left);
    }
    #[test] fn monitor_unplugged_keeps_hotkeys_working() { // Review Focus 2
        let mut r = r(); r.set_monitors(vec![]);
        assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 400, &up).is_empty());
        assert!(r.on_input(Input::Hotkey(1), 500, &up).contains(&Action::Active(Some("ipad".into()))));
    }
    #[test] fn leaving_edge_entry_places_cursor_inside() {
        let mut r = r();
        r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up);
        r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 300, &up);
        assert!(r.on_input(Input::Escape, 400, &up).contains(&Action::PlaceCursor(1899, 500)));
    }
    #[test] fn wheel_keeps_held_buttons() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
        r.on_input(Input::Button { mask: 1, down: true }, 10, &up);
        let a = r.on_input(Input::Wheel { v: -1, h: 0 }, 20, &up);
        assert!(a.iter().any(|x| matches!(x, Action::Mouse { report, .. } if report.buttons == 1 && report.wheel == -1)));
    }
    #[test] fn hotkey_zero_returns_to_windows() {
        let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
        assert!(r.on_input(Input::Hotkey(0), 10, &up).contains(&Action::Active(None)));
    }
}
