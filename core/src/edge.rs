//! Return-edge tracking for a remote screen whose pointer position can't be read
//! (spec §5.1 "pin + clamp").

/// Which edge of the remote screen touches Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEvent { Stay, Return }

/// No return in the first second after entering (the entry flick may still be arriving).
const GRACE_MS: u64 = 1000;
/// A pause longer than this ends a push; the next one starts fresh.
const MAX_GAP_MS: u64 = 180;

/// Estimates the remote pointer from sent deltas. Reaching the edge estimate is only a candidate:
/// returning needs a deliberate push, as in Universal Bridge's EdgeReturnGuard, because the remote's
/// pointer acceleration makes the estimate lag the real pointer.
pub struct EdgeTracker {
    side: Side, width: f64, gain: f64, min_push: f64, dwell_ms: u64,
    est: f64, entered: u64,
    /// (started ms, last packet ms, outward counts since start)
    push: Option<(u64, u64, f64)>,
}

impl EdgeTracker {
    /// `gain`, `min_push` (counts) and `dwell_ms` are calibration knobs.
    pub fn new(side: Side, width: f64, gain: f64, min_push: f64, dwell_ms: u64) -> Self {
        Self { side, width, gain, min_push, dwell_ms, est: 0.0, entered: 0, push: None }
    }

    /// Raw dx to send on entry so the remote pointer is pinned against the Windows-side edge.
    pub fn pin_delta(&self) -> i32 {
        let push = (self.width * 2.0) as i32;
        match self.side { Side::Left => -push, Side::Right => push }
    }

    pub fn enter(&mut self, now_ms: u64) { self.est = 0.0; self.entered = now_ms; self.push = None; }

    pub fn on_move(&mut self, dx: i32, now_ms: u64) -> EdgeEvent {
        // d < 0 means "toward Windows"
        let d = dx as f64 * self.gain * if self.side == Side::Left { 1.0 } else { -1.0 };
        let outward_at_edge = self.est == 0.0 && d < 0.0;
        self.est = (self.est + d).clamp(0.0, self.width);
        if !outward_at_edge || now_ms < self.entered + GRACE_MS { self.push = None; return EdgeEvent::Stay }
        match self.push {
            Some((start, last, dist)) if now_ms.saturating_sub(last) <= MAX_GAP_MS => {
                let dist = dist - d;
                if now_ms - start >= self.dwell_ms && dist >= self.min_push { self.push = None; return EdgeEvent::Return }
                self.push = Some((start, now_ms, dist));
            }
            // first outward packet only starts the push: it may be a flick from mid-screen
            _ => self.push = Some((now_ms, now_ms, 0.0)),
        }
        EdgeEvent::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // width 1000, gain 1, min push 96 counts, dwell 300 ms; entered at t=0
    fn t() -> EdgeTracker { let mut e = EdgeTracker::new(Side::Left, 1000.0, 1.0, 96.0, 300); e.enter(0); e }

    /// Steady outward push: `n` packets of `dx` every 10 ms starting at `from` ms. Returns the last event.
    /// Returns `Return` if any packet triggered a return.
    fn push(e: &mut EdgeTracker, dx: i32, n: u64, from: u64) -> EdgeEvent {
        let any = (0..n).map(|i| e.on_move(dx, from + i * 10)).fold(false, |a, ev| a || ev == EdgeEvent::Return);
        if any { EdgeEvent::Return } else { EdgeEvent::Stay }
    }

    #[test] fn pin_delta_points_toward_windows() {
        assert_eq!(EdgeTracker::new(Side::Left, 1000.0, 1.0, 96.0, 300).pin_delta(), -2000);
        assert_eq!(EdgeTracker::new(Side::Right, 1000.0, 1.0, 96.0, 300).pin_delta(), 2000);
    }
    #[test] fn deliberate_push_returns() {
        let mut e = t();
        // 32 packets × -5 over 310 ms after the grace period: dwell ≥ 300 ms and 155 counts ≥ 96
        assert_eq!(push(&mut e, -5, 32, 2000), EdgeEvent::Return);
    }
    #[test] fn no_return_during_entry_grace() {
        let mut e = t();
        assert_eq!(push(&mut e, -5, 60, 100), EdgeEvent::Stay, "pushes inside the first second never return");
    }
    #[test] fn short_push_does_not_return() {
        let mut e = t();
        assert_eq!(push(&mut e, -20, 20, 2000), EdgeEvent::Stay, "200 ms is under the 300 ms dwell even with 400 counts");
    }
    #[test] fn weak_push_does_not_return() {
        let mut e = t();
        assert_eq!(push(&mut e, -1, 40, 2000), EdgeEvent::Stay, "390 ms but only 39 counts");
    }
    #[test] fn first_flick_does_not_count() {
        let mut e = t();
        e.on_move(-5000, 2000); // fast flick arriving at the edge
        assert_eq!(push(&mut e, -3, 31, 2010), EdgeEvent::Stay, "93 counts after the flick is under 96");
    }
    #[test] fn gap_restarts_push() {
        let mut e = t();
        push(&mut e, -5, 20, 2000);
        assert_eq!(push(&mut e, -5, 20, 2400), EdgeEvent::Stay, "190+ ms pause restarts the dwell");
    }
    #[test] fn moving_away_resets() {
        let mut e = t();
        push(&mut e, -5, 20, 2000);
        e.on_move(10, 2200);
        e.on_move(-10, 2210);
        assert_eq!(push(&mut e, -5, 20, 2220), EdgeEvent::Stay);
    }
    #[test] fn must_travel_back_before_push_counts() {
        let mut e = t();
        e.on_move(300, 2000);
        assert_eq!(push(&mut e, -10, 31, 2010), EdgeEvent::Stay, "first 300 counts only travel back to the edge");
        assert_eq!(push(&mut e, -5, 32, 2320), EdgeEvent::Return);
    }
    #[test] fn far_edge_clamps() {
        let mut e = t();
        e.on_move(5000, 2000);
        e.on_move(-1000, 2010);
        assert_eq!(push(&mut e, -5, 32, 2020), EdgeEvent::Return);
    }
    #[test] fn right_side_mirrors() {
        let mut e = EdgeTracker::new(Side::Right, 1000.0, 1.0, 96.0, 300);
        e.enter(0);
        assert_eq!(push(&mut e, 5, 32, 2000), EdgeEvent::Return);
    }
}
