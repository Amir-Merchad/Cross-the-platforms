//! Input routing: decides when local input goes to a device and turns it into HID reports.
//! Pure state machine; the Windows engine feeds it captured input and executes its actions.

use std::collections::HashMap;

use crate::{
    hid::{HeldInput, KeyboardReport, MouseReport, split_delta},
    layout::{Edge, Monitor, Rect},
    settings::Settings,
    space::{self, Beyond, Placement, Tracker, opposite},
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
/// A pause longer than this while pushing into an edge restarts the entry hold.
const HOLD_GAP_MS: u64 = 180;
/// How far inside the edge the Windows cursor lands on return.
const RETURN_INSET: i32 = 20;

struct Remote {
    device_id: String,
    tracker: Tracker,
    /// Where the Windows cursor lands on leaving: set when entered by an edge, `None` after a hotkey.
    return_to: Option<(i32, i32)>,
    held: HeldInput,
    pending: (i32, i32),
    next_due: u64,
}

pub struct Router {
    settings: Settings,
    monitors: Vec<Monitor>,
    /// The free layout in use: saved, or made from edge attachments for the current monitors.
    layout: Vec<Placement>,
    intervals: HashMap<String, u64>,
    /// (device id, since ms, last push ms) while the cursor pushes into an attached edge.
    hold: Option<(String, u64, u64)>,
    remote: Option<Remote>,
    cursor: (i32, i32),
}

/// `n` pixels past `p` in the direction of `edge`: into the screen that lies beyond that edge.
fn inset(edge: Edge, p: (i32, i32), n: i32) -> (i32, i32) {
    match edge { Edge::Left => (p.0 - n, p.1), Edge::Right => (p.0 + n, p.1), Edge::Top => (p.0, p.1 - n), Edge::Bottom => (p.0, p.1 + n) }
}

impl Router {
    pub fn new(settings: Settings, monitors: Vec<Monitor>) -> Self {
        let mut r = Self { settings, monitors, layout: Vec::new(), intervals: HashMap::new(), hold: None, remote: None, cursor: (0, 0) };
        r.relayout();
        r
    }
    pub fn set_settings(&mut self, s: Settings) { self.settings = s; self.hold = None; self.relayout(); }
    pub fn set_monitors(&mut self, m: Vec<Monitor>) { self.monitors = m; self.hold = None; self.relayout(); }
    pub fn set_interval(&mut self, host: &str, ms: u64) { self.intervals.insert(host.to_string(), ms); }
    pub fn active(&self) -> Option<&str> { self.remote.as_ref().map(|r| r.device_id.as_str()) }
    pub fn next_tick_ms(&self) -> Option<u64> {
        self.remote.as_ref().filter(|r| r.pending != (0, 0)).map(|r| r.next_due)
    }

    fn relayout(&mut self) {
        let s = &self.settings;
        self.layout = space::effective(&s.layout, &s.attachments, &self.monitors, |id| s.width_for(id));
    }

    /// Hotkey order: the saved layout's order, else the attachments' (a device on an unplugged screen keeps its number).
    fn device_at(&self, n: u8) -> Option<String> {
        let i = n as usize - 1;
        if self.settings.layout.is_empty() { self.settings.attachments.get(i).map(|a| a.device_id.clone()) } else { self.settings.layout.get(i).map(|p| p.device_id.clone()) }
    }

    pub fn on_input(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        match input {
            Input::Hotkey(0) | Input::Escape => self.leave(),
            Input::Hotkey(n) => self.hotkey(n, now_ms, connected),
            _ if self.remote.is_none() => self.local(input, now_ms, connected),
            _ => self.remote_input(input, now_ms, connected),
        }
    }

    pub fn tick(&mut self, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Some(r) = &self.remote else { return Vec::new() };
        if !connected(&r.device_id) { return self.leave() }
        if r.pending == (0, 0) || now_ms < r.next_due { return Vec::new() }
        let interval = self.interval_for(&r.device_id);
        let r = self.remote.as_mut().unwrap();
        r.next_due = now_ms + interval;
        flush(r)
    }

    fn interval_for(&self, id: &str) -> u64 { self.intervals.get(id).copied().unwrap_or(DEFAULT_INTERVAL_MS) }

    fn local(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Input::Move { dx, dy, cursor } = input else { return Vec::new() };
        self.cursor = cursor;
        let target = space::entry(&self.layout, &self.monitors, cursor, dx, dy)
            .filter(|(p, _, _)| connected(&p.device_id))
            .map(|(p, e, a)| (p.device_id.clone(), e, a));
        let Some((id, edge, along)) = target else { self.hold = None; return Vec::new() };
        match self.hold.clone() {
            // A continuing push (no pause longer than HOLD_GAP_MS) counts towards the hold.
            Some((held, since, last)) if held == id && now_ms.saturating_sub(last) <= HOLD_GAP_MS => {
                if now_ms.saturating_sub(since) >= self.settings.entry_hold_ms {
                    let back = inset(edge, cursor, RETURN_INSET);
                    return self.enter(&id, edge, along, Some(back), now_ms);
                }
                self.hold = Some((held, since, now_ms));
                Vec::new()
            }
            _ => { self.hold = Some((id, now_ms, now_ms)); Vec::new() }
        }
    }

    fn hotkey(&mut self, n: u8, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let Some(id) = self.device_at(n) else { return vec![Action::Notice(format!("no device on hotkey {n}"))] };
        if !connected(&id) { return vec![Action::Notice("That device is not connected".into())] }
        let mut out = if self.remote.is_some() { self.leave() } else { Vec::new() };
        let edge = self.layout.iter().find(|p| p.device_id == id).map(|p| space::home_edge(p, &self.monitors))
            .or_else(|| self.settings.attachments.iter().find(|a| a.device_id == id).map(|a| opposite(a.edge)))
            .unwrap_or(Edge::Left);
        out.extend(self.enter(&id, edge, 0.5, None, now_ms));
        out
    }

    /// Takes control of `id`, pinning its pointer against `edge` (the device's own edge) at `along`.
    fn enter(&mut self, id: &str, edge: Edge, along: f64, return_to: Option<(i32, i32)>, now_ms: u64) -> Vec<Action> {
        self.hold = None;
        let cal = self.settings.calibration_for(id);
        let width = self.settings.width_for(id);
        // A device on an unplugged screen still gets a shape, so hotkeys and Esc keep working.
        let rect = self.layout.iter().find(|p| p.device_id == id).map(|p| p.rect)
            .unwrap_or(Rect { left: 0, top: 0, right: width as i32, bottom: (width * 0.75) as i32 });
        let mut tracker = Tracker::new(&rect, width, cal.gain, cal.min_push, cal.dwell_ms);
        tracker.enter(edge, along, now_ms);
        let (px, py) = tracker.pin_delta(edge);
        let host = id.to_string();
        let mut out = vec![Action::Suppress(true)];
        out.extend(split_delta(px, py).into_iter().map(|(dx, dy)| Action::Mouse { host: host.clone(), report: MouseReport { dx, dy, ..Default::default() } }));
        out.push(Action::Active(Some(host.clone())));
        self.remote = Some(Remote { device_id: host, tracker, return_to, held: HeldInput::default(), pending: (0, 0), next_due: now_ms });
        out
    }

    fn remote_input(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let r = self.remote.as_mut().unwrap();
        let host = r.device_id.clone();
        match input {
            Input::Move { dx, dy, .. } => {
                if let Some((edge, along)) = r.tracker.on_move(dx, dy, now_ms) { return self.cross(edge, along, now_ms, connected) }
                let r = self.remote.as_mut().unwrap();
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

    /// A deliberate push left the active device by `edge` at `along`: Windows, the next device, or stay.
    fn cross(&mut self, edge: Edge, along: f64, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action> {
        let id = self.remote.as_ref().unwrap().device_id.clone();
        let Some(rect) = self.layout.iter().find(|p| p.device_id == id).map(|p| p.rect) else {
            // Unplugged screen: the edge that faced Windows still returns, to where the cursor was.
            let faced = self.settings.attachments.iter().find(|a| a.device_id == id).map(|a| opposite(a.edge));
            return if faced == Some(edge) { self.leave() } else { Vec::new() };
        };
        let p = space::point_on(&rect, edge, along);
        match space::what_is_beyond(edge, p, &self.monitors, &self.layout, &id) {
            Beyond::Monitor(_, q) => {
                self.remote.as_mut().unwrap().return_to = Some(inset(edge, q, RETURN_INSET));
                self.leave()
            }
            Beyond::Device(next) if connected(&next) => {
                let Some(d) = self.layout.iter().find(|d| d.device_id == next) else { return Vec::new() };
                let in_edge = opposite(edge);
                let a = space::along(&d.rect, in_edge, p);
                let back = self.remote.as_ref().unwrap().return_to;
                let mut out = self.leave_remote();
                out.extend(self.enter(&next, in_edge, a, back, now_ms).into_iter().filter(|x| *x != Action::Suppress(true)));
                out
            }
            _ => Vec::new(),
        }
    }

    /// Releases everything held on the active device, without giving input back to Windows.
    fn leave_remote(&mut self) -> Vec<Action> {
        let Some(mut r) = self.remote.take() else { return Vec::new() };
        let (k, m) = r.held.release_all();
        vec![Action::Keyboard { host: r.device_id.clone(), report: k }, Action::Mouse { host: r.device_id, report: m }]
    }

    /// Windows input first, then release everything held on the device.
    fn leave(&mut self) -> Vec<Action> {
        let Some(return_to) = self.remote.as_ref().map(|r| r.return_to) else { return Vec::new() };
        let place = return_to.unwrap_or(self.cursor);
        let mut out = vec![Action::Suppress(false)];
        out.extend(self.leave_remote());
        out.push(Action::PlaceCursor(place.0, place.1));
        out.push(Action::Active(None));
        out
    }
}

/// Emits pending motion as reports (with held buttons, so drags keep the button down).
fn flush(r: &mut Remote) -> Vec<Action> {
    let (dx, dy) = std::mem::take(&mut r.pending);
    let buttons = r.held.buttons();
    let host = &r.device_id;
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
        for t in [0, 100, 200] { assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, t, &up).is_empty()); }
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
        assert_eq!(a, vec![Action::Notice("That device is not connected".into())]); // never the raw id
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
    #[test] fn ipad_wifi_disconnect_returns_to_windows() { // Review Focus 1
        use crate::ipadhid::IPAD_HOST;
        let mut s = Settings::default();
        s.attachments = vec![Attachment { device_id: IPAD_HOST.into(), monitor_id: "M1".into(), edge: Edge::Right }];
        let mut r = Router::new(s, vec![Monitor { id: "M1".into(), rect: Rect { left: 0, top: 0, right: 1920, bottom: 1080 } }]);
        let a = r.on_input(Input::Hotkey(1), 0, &up);
        assert!(a.contains(&Action::Active(Some(IPAD_HOST.into()))));
        r.on_input(Input::Key { vk: 0x41, down: true }, 5, &up);
        let a = r.tick(10, &down);
        assert_eq!(a.first(), Some(&Action::Suppress(false)), "Windows gets input back first");
        assert!(a.contains(&Action::Keyboard { host: IPAD_HOST.into(), report: KeyboardReport::default() }), "held key released");
        assert_eq!(a.last(), Some(&Action::Active(None)));
        assert_eq!(r.active(), None);
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
        for t in [0, 100, 200, 300] { r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, t, &up); }
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
    // Free layout: the phone sits to the right of the iPad, which sits to the right of the screen.
    fn chain() -> Router {
        use crate::space::Placement;
        let mut s = Settings::default();
        s.layout = vec![
            Placement { device_id: "ipad".into(), rect: Rect { left: 1920, top: 0, right: 3286, bottom: 1024 } },
            Placement { device_id: "phone".into(), rect: Rect { left: 3286, top: 100, right: 3686, bottom: 900 } },
        ];
        Router::new(s, vec![Monitor { id: "M1".into(), rect: Rect { left: 0, top: 0, right: 1920, bottom: 1080 } }])
    }
    /// Pushes with `dx` for `n` packets 10 ms apart from `t`; returns every action.
    fn shove(r: &mut Router, dx: i32, n: u64, t: u64) -> Vec<Action> {
        (0..n).flat_map(|i| { let mut a = r.on_input(Input::Move { dx, dy: 0, cursor: (0, 0) }, t + i * 10, &up); a.extend(r.tick(t + i * 10, &up)); a }).collect()
    }
    #[test] fn pointer_walks_pc_ipad_phone_and_back() {
        let mut r = chain();
        for t in [0, 100, 200, 300] { r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, t, &up); }
        assert_eq!(r.active(), Some("ipad"));
        shove(&mut r, 20, 80, 2000); // across the iPad (1366 wide)
        let a = shove(&mut r, 5, 40, 3000);
        assert_eq!(r.active(), Some("phone"), "a push at the iPad's right edge enters the phone");
        assert!(!a.contains(&Action::Suppress(false)), "Windows never gets input back on the way");
        assert!(a.contains(&Action::Keyboard { host: "ipad".into(), report: KeyboardReport::default() }), "the iPad is let go of");
        shove(&mut r, -5, 40, 5000); // pinned at the phone's left edge, so this pushes back
        assert_eq!(r.active(), Some("ipad"));
        shove(&mut r, -20, 80, 7000);
        let a = shove(&mut r, -5, 40, 8000);
        assert_eq!(r.active(), None);
        assert!(a.contains(&Action::PlaceCursor(1899, 500)), "{a:?}");
    }
    #[test] fn an_edge_with_nothing_beyond_keeps_the_pointer() {
        let mut r = chain();
        r.on_input(Input::Hotkey(2), 0, &up); // the phone; nothing lies right of it
        assert_eq!(r.active(), Some("phone"));
        shove(&mut r, 20, 40, 2000);
        shove(&mut r, 5, 40, 3000);
        assert_eq!(r.active(), Some("phone"));
    }
    #[test] fn a_disconnected_neighbour_is_not_entered() {
        let mut r = chain();
        r.on_input(Input::Hotkey(1), 0, &up);
        let only_ipad = |id: &str| id == "ipad";
        for i in 0..80u64 { r.on_input(Input::Move { dx: 20, dy: 0, cursor: (0, 0) }, 2000 + i * 10, &only_ipad); }
        for i in 0..40u64 { r.on_input(Input::Move { dx: 5, dy: 0, cursor: (0, 0) }, 3000 + i * 10, &only_ipad); }
        assert_eq!(r.active(), Some("ipad"));
    }
    #[test] fn saved_layout_sets_hotkey_order() {
        let mut r = chain();
        assert!(r.on_input(Input::Hotkey(2), 0, &up).contains(&Action::Active(Some("phone".into()))));
    }
    // Final review I4: a pause at the edge restarts the hold.
    #[test] fn pause_at_edge_restarts_hold() {
        let mut r = r();
        r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up);
        let a = r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 1000, &up);
        assert!(a.is_empty(), "1 s pause then a nudge must not enter instantly");
        assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 1150, &up).is_empty());
        assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 1300, &up).contains(&Action::Suppress(true)));
    }
}
