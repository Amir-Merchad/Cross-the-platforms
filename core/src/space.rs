//! Free layout: monitors and devices as rectangles in one desktop space (Windows virtual-screen
//! pixels). Screens connect where their edges touch. See docs/superpowers/specs/2026-11-05-free-layout-design.md.

use serde::{Deserialize, Serialize};

use crate::layout::{Attachment, Edge, Monitor, Rect};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement { pub device_id: String, pub rect: Rect }

/// Default device height as a share of its width: an iPad in landscape.
const ASPECT: (i32, i32) = (3, 4);

fn overlap(a0: i32, a1: i32, b0: i32, b1: i32) -> bool { a0.max(b0) < a1.min(b1) }

/// The edge of `a` that `b` touches, if their spans overlap along it. Corners alone do not touch.
pub fn touching(a: &Rect, b: &Rect) -> Option<Edge> {
    let vertical = overlap(a.top, a.bottom, b.top, b.bottom);
    let horizontal = overlap(a.left, a.right, b.left, b.right);
    if vertical && a.right == b.left { Some(Edge::Right) }
    else if vertical && a.left == b.right { Some(Edge::Left) }
    else if horizontal && a.bottom == b.top { Some(Edge::Bottom) }
    else if horizontal && a.top == b.bottom { Some(Edge::Top) }
    else { None }
}

fn intersects(a: &Rect, b: &Rect) -> bool { overlap(a.left, a.right, b.left, b.right) && overlap(a.top, a.bottom, b.top, b.bottom) }

/// Each device once, with a real size, never overlapping another device or a monitor.
pub fn validate(layout: &[Placement], monitors: &[Monitor]) -> Result<(), String> {
    for (i, p) in layout.iter().enumerate() {
        if p.rect.right <= p.rect.left || p.rect.bottom <= p.rect.top { return Err(format!("{} has no size", p.device_id)) }
        for q in &layout[i + 1..] {
            if p.device_id == q.device_id { return Err(format!("device {} is placed twice", p.device_id)) }
            if intersects(&p.rect, &q.rect) { return Err(format!("{} overlaps {}", p.device_id, q.device_id)) }
        }
        if let Some(m) = monitors.iter().find(|m| intersects(&p.rect, &m.rect)) { return Err(format!("{} overlaps screen {}", p.device_id, m.id)) }
    }
    Ok(())
}

/// The layout to use: the saved one, or one made from edge attachments. A made-up placement covers
/// its monitor's whole edge, so an old settings file crosses exactly where it did before. `width`
/// gives each device's depth in desk pixels. Attachments to a missing monitor are left out.
pub fn effective(saved: &[Placement], attachments: &[Attachment], monitors: &[Monitor], width: impl Fn(&str) -> f64) -> Vec<Placement> {
    if !saved.is_empty() { return saved.to_vec() }
    attachments.iter().filter_map(|a| {
        let m = monitors.iter().find(|m| m.id == a.monitor_id)?.rect;
        let depth = (width(&a.device_id).round() as i32).max(1);
        let rect = match a.edge {
            Edge::Right => Rect { left: m.right, right: m.right + depth, ..m },
            Edge::Left => Rect { left: m.left - depth, right: m.left, ..m },
            Edge::Top => Rect { top: m.top - depth * ASPECT.0 / ASPECT.1, bottom: m.top, ..m },
            Edge::Bottom => Rect { top: m.bottom, bottom: m.bottom + depth * ASPECT.0 / ASPECT.1, ..m },
        };
        Some(Placement { device_id: a.device_id.clone(), rect })
    }).collect()
}

/// A new placement for a device dropped beside `rect` on `edge`, centred along it, `width` wide.
pub fn beside(rect: &Rect, edge: Edge, width: i32) -> Rect {
    let h = width * ASPECT.0 / ASPECT.1;
    let (cx, cy) = ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
    match edge {
        Edge::Right => Rect { left: rect.right, right: rect.right + width, top: cy - h / 2, bottom: cy - h / 2 + h },
        Edge::Left => Rect { left: rect.left - width, right: rect.left, top: cy - h / 2, bottom: cy - h / 2 + h },
        Edge::Top => Rect { left: cx - width / 2, right: cx - width / 2 + width, top: rect.top - h, bottom: rect.top },
        Edge::Bottom => Rect { left: cx - width / 2, right: cx - width / 2 + width, top: rect.bottom, bottom: rect.bottom + h },
    }
}

pub fn opposite(e: Edge) -> Edge { match e { Edge::Left => Edge::Right, Edge::Right => Edge::Left, Edge::Top => Edge::Bottom, Edge::Bottom => Edge::Top } }

/// The point on `edge` of `r` at `along` (0 at the top or left end, 1 at the other).
pub fn point_on(r: &Rect, edge: Edge, along: f64) -> (i32, i32) {
    let along = along.clamp(0.0, 1.0);
    let x = r.left + ((r.right - 1 - r.left) as f64 * along).round() as i32;
    let y = r.top + ((r.bottom - 1 - r.top) as f64 * along).round() as i32;
    match edge { Edge::Left => (r.left, y), Edge::Right => (r.right - 1, y), Edge::Top => (x, r.top), Edge::Bottom => (x, r.bottom - 1) }
}

/// Where `p` lies along `edge` of `r`, as 0..=1.
pub fn along(r: &Rect, edge: Edge, p: (i32, i32)) -> f64 {
    let span = |a: i32, b: i32, v: i32| if b - 1 > a { (v - a) as f64 / (b - 1 - a) as f64 } else { 0.5 };
    match edge { Edge::Left | Edge::Right => span(r.top, r.bottom, p.1), Edge::Top | Edge::Bottom => span(r.left, r.right, p.0) }.clamp(0.0, 1.0)
}

/// Just beyond `edge` of `r` at point `p`: the pixel a screen there must contain.
fn beyond(edge: Edge, p: (i32, i32)) -> (i32, i32) {
    match edge { Edge::Left => (p.0 - 1, p.1), Edge::Right => (p.0 + 1, p.1), Edge::Top => (p.0, p.1 - 1), Edge::Bottom => (p.0, p.1 + 1) }
}

fn contains(r: &Rect, p: (i32, i32)) -> bool { p.0 >= r.left && p.0 < r.right && p.1 >= r.top && p.1 < r.bottom }

/// What lies just beyond a point on a screen's edge.
#[derive(Clone, Debug, PartialEq)]
pub enum Beyond { Monitor(String, (i32, i32)), Device(String), Nothing }

/// Monitors win over devices, as a neighbouring monitor already wins over an attached device.
pub fn what_is_beyond(edge: Edge, p: (i32, i32), monitors: &[Monitor], layout: &[Placement], except: &str) -> Beyond {
    let q = beyond(edge, p);
    if let Some(m) = monitors.iter().find(|m| contains(&m.rect, q)) { return Beyond::Monitor(m.id.clone(), q) }
    match layout.iter().find(|d| d.device_id != except && contains(&d.rect, q)) {
        Some(d) => Beyond::Device(d.device_id.clone()),
        None => Beyond::Nothing,
    }
}

/// The device the cursor pushes into from a monitor edge, the device's edge it enters by, and where along it.
pub fn entry<'a>(layout: &'a [Placement], monitors: &[Monitor], cursor: (i32, i32), dx: i32, dy: i32) -> Option<(&'a Placement, Edge, f64)> {
    let m = monitors.iter().find(|m| contains(&m.rect, cursor))?;
    let r = m.rect;
    let (x, y) = cursor;
    let edge = [(Edge::Right, x == r.right - 1 && dx > 0), (Edge::Left, x == r.left && dx < 0), (Edge::Top, y == r.top && dy < 0), (Edge::Bottom, y == r.bottom - 1 && dy > 0)]
        .into_iter().find(|(_, pushing)| *pushing)?.0;
    match what_is_beyond(edge, cursor, monitors, layout, "") {
        Beyond::Device(id) => {
            let d = layout.iter().find(|d| d.device_id == id)?;
            let in_edge = opposite(edge);
            Some((d, in_edge, along(&d.rect, in_edge, beyond(edge, cursor))))
        }
        _ => None,
    }
}

/// Where a hotkey-entered device's pointer starts: the edge facing a monitor if one touches, else its left.
pub fn home_edge(d: &Placement, monitors: &[Monitor]) -> Edge {
    monitors.iter().find_map(|m| touching(&d.rect, &m.rect)).unwrap_or(Edge::Left)
}

/// No return in the first second after entering (the entry flick may still be arriving).
const GRACE_MS: u64 = 1000;
/// A pause longer than this ends a push; the next one starts fresh.
const MAX_GAP_MS: u64 = 180;

/// The remote pointer on both axes, estimated from sent motion, in the device's own pointer units.
/// Leaving needs a deliberate push against an edge, with the rules of `edge::EdgeTracker`.
pub struct Tracker {
    w: f64, h: f64, gain: f64, min_push: f64, dwell_ms: u64,
    est: (f64, f64), entered: u64,
    /// (edge, started ms, last packet ms, outward counts since start)
    push: Option<(Edge, u64, u64, f64)>,
}

impl Tracker {
    /// `width` in pointer units; the height follows the placement's shape.
    pub fn new(rect: &Rect, width: f64, gain: f64, min_push: f64, dwell_ms: u64) -> Self {
        let h = width * (rect.bottom - rect.top) as f64 / (rect.right - rect.left).max(1) as f64;
        Self { w: width, h, gain, min_push, dwell_ms, est: (0.0, 0.0), entered: 0, push: None }
    }

    /// Starts at `along` on `edge`.
    pub fn enter(&mut self, edge: Edge, along: f64, now_ms: u64) {
        let a = along.clamp(0.0, 1.0);
        self.est = match edge { Edge::Left => (0.0, a * self.h), Edge::Right => (self.w, a * self.h), Edge::Top => (a * self.w, 0.0), Edge::Bottom => (a * self.w, self.h) };
        self.entered = now_ms;
        self.push = None;
    }

    /// Raw motion that pins the remote pointer against `edge`.
    pub fn pin_delta(&self, edge: Edge) -> (i32, i32) {
        let (w, h) = ((self.w * 2.0) as i32, (self.h * 2.0) as i32);
        match edge { Edge::Left => (-w, 0), Edge::Right => (w, 0), Edge::Top => (0, -h), Edge::Bottom => (0, h) }
    }

    /// Where the estimate sits along `edge`, 0..=1.
    pub fn along(&self, edge: Edge) -> f64 {
        match edge { Edge::Left | Edge::Right => self.est.1 / self.h.max(1.0), Edge::Top | Edge::Bottom => self.est.0 / self.w.max(1.0) }.clamp(0.0, 1.0)
    }

    /// The edge a deliberate push just left by, and where along it.
    pub fn on_move(&mut self, dx: i32, dy: i32, now_ms: u64) -> Option<(Edge, f64)> {
        let (ddx, ddy) = (dx as f64 * self.gain, dy as f64 * self.gain);
        // Outward at an edge the estimate already sits on; the stronger axis wins.
        let x_out = match () { _ if self.est.0 <= 0.0 && ddx < 0.0 => Some(Edge::Left), _ if self.est.0 >= self.w && ddx > 0.0 => Some(Edge::Right), _ => None };
        let y_out = match () { _ if self.est.1 <= 0.0 && ddy < 0.0 => Some(Edge::Top), _ if self.est.1 >= self.h && ddy > 0.0 => Some(Edge::Bottom), _ => None };
        let out = match (x_out, y_out) {
            (Some(x), Some(y)) => Some(if ddx.abs() >= ddy.abs() { (x, ddx.abs()) } else { (y, ddy.abs()) }),
            (Some(x), None) if ddx.abs() >= ddy.abs() => Some((x, ddx.abs())),
            (None, Some(y)) if ddy.abs() >= ddx.abs() => Some((y, ddy.abs())),
            _ => None,
        };
        self.est = ((self.est.0 + ddx).clamp(0.0, self.w), (self.est.1 + ddy).clamp(0.0, self.h));
        let Some((edge, d)) = out else { self.push = None; return None };
        if now_ms < self.entered + GRACE_MS { self.push = None; return None }
        match self.push {
            Some((e, start, last, dist)) if e == edge && now_ms.saturating_sub(last) <= MAX_GAP_MS => {
                let dist = dist + d;
                if now_ms.saturating_sub(start) >= self.dwell_ms && dist >= self.min_push { self.push = None; return Some((edge, self.along(edge))) }
                self.push = Some((e, start, now_ms, dist));
            }
            // The first outward packet only starts the push: it may be a flick from mid-screen.
            _ => self.push = Some((edge, now_ms, now_ms, 0.0)),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect { Rect { left: l, top: t, right: r, bottom: b } }
    fn mon(id: &str, r: Rect) -> Monitor { Monitor { id: id.into(), rect: r } }
    fn place(id: &str, r: Rect) -> Placement { Placement { device_id: id.into(), rect: r } }
    fn att(d: &str, m: &str, e: Edge) -> Attachment { Attachment { device_id: d.into(), monitor_id: m.into(), edge: e } }
    const M1: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    #[test] fn touching_needs_a_shared_span() {
        assert_eq!(touching(&M1, &rect(1920, 500, 3000, 1500)), Some(Edge::Right));
        assert_eq!(touching(&rect(1920, 500, 3000, 1500), &M1), Some(Edge::Left));
        assert_eq!(touching(&M1, &rect(100, 1080, 500, 1400)), Some(Edge::Bottom));
        assert_eq!(touching(&M1, &rect(1920, 1080, 3000, 1500)), None, "a corner alone does not touch");
        assert_eq!(touching(&M1, &rect(1921, 0, 3000, 1080)), None, "a gap does not touch");
    }
    #[test] fn old_attachments_cover_the_whole_edge() {
        let l = effective(&[], &[att("ipad", "M1", Edge::Right), att("s24", "M1", Edge::Left), att("x", "GONE", Edge::Top)], &[mon("M1", M1)], |_| 1366.0);
        assert_eq!(l, vec![place("ipad", rect(1920, 0, 3286, 1080)), place("s24", rect(-1366, 0, 0, 1080))]);
        assert!(validate(&l, &[mon("M1", M1)]).is_ok());
    }
    #[test] fn a_saved_layout_wins() {
        let saved = [place("ipad", rect(1920, 0, 3000, 800))];
        assert_eq!(effective(&saved, &[att("s24", "M1", Edge::Left)], &[mon("M1", M1)], |_| 1366.0), saved.to_vec());
    }
    #[test] fn overlaps_and_duplicates_are_invalid() {
        let m = [mon("M1", M1)];
        assert!(validate(&[place("a", rect(1920, 0, 3000, 800)), place("b", rect(2900, 0, 4000, 800))], &m).is_err());
        assert!(validate(&[place("a", rect(1900, 0, 3000, 800))], &m).is_err(), "over a monitor");
        assert!(validate(&[place("a", rect(1920, 0, 3000, 800)), place("a", rect(3000, 0, 4000, 800))], &m).is_err());
        assert!(validate(&[place("a", rect(1920, 0, 1920, 800))], &m).is_err(), "no size");
        assert!(validate(&[place("a", rect(1920, 0, 3000, 800)), place("b", rect(3000, 0, 4000, 800))], &m).is_ok(), "touching is fine");
    }
    #[test] fn more_than_four_devices_fit() {
        let m = [mon("M1", M1)];
        let l: Vec<Placement> = (0..6).map(|i| place(&format!("d{i}"), rect(1920 + i * 1000, 0, 2920 + i * 1000, 750))).collect();
        assert!(validate(&l, &m).is_ok());
    }
    #[test] fn entry_maps_the_cursor_along_the_device_edge() {
        let l = [place("ipad", rect(1920, 540, 3286, 1564))];
        let m = [mon("M1", M1)];
        let (d, e, a) = entry(&l, &m, (1919, 800), 3, 0).unwrap();
        assert_eq!((d.device_id.as_str(), e), ("ipad", Edge::Left));
        assert!((a - 260.0 / 1023.0).abs() < 1e-9);
        assert!(entry(&l, &m, (1919, 300), 3, 0).is_none(), "above the device: nothing there");
        assert!(entry(&l, &m, (1919, 800), -3, 0).is_none(), "moving inward");
    }
    #[test] fn a_monitor_beyond_the_edge_wins() {
        let l = [place("ipad", rect(1920, 0, 3286, 1080))];
        let m = [mon("M1", M1), mon("M2", rect(1920, 0, 3840, 400))];
        assert!(entry(&l, &m, (1919, 200), 3, 0).is_none());
    }
    #[test] fn devices_beside_devices() {
        let l = [place("ipad", rect(1920, 0, 3286, 1024)), place("phone", rect(3286, 200, 3686, 1000))];
        let m = [mon("M1", M1)];
        assert_eq!(what_is_beyond(Edge::Right, (3285, 500), &m, &l, "ipad"), Beyond::Device("phone".into()));
        assert_eq!(what_is_beyond(Edge::Right, (3285, 100), &m, &l, "ipad"), Beyond::Nothing);
        assert_eq!(what_is_beyond(Edge::Left, (1920, 500), &m, &l, "ipad"), Beyond::Monitor("M1".into(), (1919, 500)));
    }
    #[test] fn points_and_along_agree() {
        let r = rect(100, 200, 301, 401);
        for e in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            let p = point_on(&r, e, 0.25);
            assert!((along(&r, e, p) - 0.25).abs() < 0.01, "{e:?}");
        }
        assert_eq!(point_on(&r, Edge::Right, 1.0), (300, 400));
    }
    #[test] fn home_edge_faces_a_monitor() {
        let m = [mon("M1", M1)];
        assert_eq!(home_edge(&place("up", rect(0, -768, 1024, 0)), &m), Edge::Bottom);
        assert_eq!(home_edge(&place("far", rect(5000, 0, 6000, 750)), &m), Edge::Left);
    }
    #[test] fn beside_centres_on_the_edge() {
        assert_eq!(beside(&M1, Edge::Right, 1000), rect(1920, 165, 2920, 915));
        assert_eq!(touching(&M1, &beside(&M1, Edge::Top, 800)), Some(Edge::Top));
    }

    // Tracker: width 1000, height 750, gain 1, min push 96, dwell 300.
    fn t(edge: Edge, along: f64) -> Tracker { let mut k = Tracker::new(&rect(0, 0, 1000, 750), 1000.0, 1.0, 96.0, 300); k.enter(edge, along, 0); k }
    fn push(k: &mut Tracker, dx: i32, dy: i32, n: u64, from: u64) -> Option<(Edge, f64)> {
        (0..n).filter_map(|i| k.on_move(dx, dy, from + i * 10)).last()
    }

    #[test] fn deliberate_push_back_leaves_where_it_entered() {
        let mut k = t(Edge::Left, 0.5);
        let (e, a) = push(&mut k, -5, 0, 32, 2000).unwrap();
        assert_eq!(e, Edge::Left);
        assert!((a - 0.5).abs() < 1e-9);
    }
    #[test] fn no_leaving_during_the_grace_second() { let mut k = t(Edge::Left, 0.5); assert_eq!(push(&mut k, -5, 0, 60, 100), None); }
    #[test] fn short_or_weak_pushes_stay() {
        let mut k = t(Edge::Left, 0.5);
        assert_eq!(push(&mut k, -20, 0, 20, 2000), None, "under the dwell");
        let mut k = t(Edge::Left, 0.5);
        assert_eq!(push(&mut k, -1, 0, 40, 2000), None, "under the minimum push");
    }
    #[test] fn crossing_to_the_far_side_needs_the_travel() {
        let mut k = t(Edge::Left, 0.5);
        assert_eq!(push(&mut k, 10, 0, 99, 2000), None, "990 of 1000: not at the far edge yet");
        assert_eq!(push(&mut k, 5, 0, 34, 3000).map(|r| r.0), Some(Edge::Right));
    }
    #[test] fn the_far_edge_crossing_reports_the_estimated_point() {
        let mut k = t(Edge::Left, 0.0);
        k.on_move(0, 375, 2000); // down to the middle
        k.on_move(2000, 0, 2010); // across, clamped at the right edge
        let (e, a) = push(&mut k, 5, 0, 32, 2020).unwrap();
        assert_eq!(e, Edge::Right);
        assert!((a - 0.5).abs() < 1e-9);
    }
    #[test] fn pushing_down_leaves_by_the_bottom() {
        let mut k = t(Edge::Top, 0.3);
        k.on_move(0, 5000, 2000);
        assert_eq!(push(&mut k, 0, 5, 32, 2010).map(|r| r.0), Some(Edge::Bottom));
    }
    #[test] fn sideways_drift_does_not_count_as_a_push() {
        let mut k = t(Edge::Left, 0.5);
        assert_eq!(push(&mut k, -1, 8, 40, 2000), None, "mostly vertical motion along the left edge");
    }
    #[test] fn pin_points_at_the_entry_edge() {
        let k = t(Edge::Left, 0.5);
        assert_eq!(k.pin_delta(Edge::Left), (-2000, 0));
        assert_eq!(k.pin_delta(Edge::Bottom), (0, 1500));
    }
}
