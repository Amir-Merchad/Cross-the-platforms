//! Return-edge tracking for a remote screen whose pointer position can't be read
//! (spec §5.1 "pin + clamp").

/// Which edge of the remote screen touches Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEvent { Stay, Return }

pub struct EdgeTracker { side: Side, width: f64, gain: f64, threshold: f64, est: f64, overflow: f64 }

impl EdgeTracker {
    /// `gain` and `threshold` are calibration knobs (iPadOS/Android pointer acceleration differ).
    pub fn new(side: Side, width: f64, gain: f64, threshold: f64) -> Self {
        Self { side, width, gain, threshold, est: 0.0, overflow: 0.0 }
    }

    /// Raw dx to send on entry so the remote pointer is pinned against the Windows-side edge.
    pub fn pin_delta(&self) -> i32 {
        let push = (self.width * 2.0) as i32;
        match self.side { Side::Left => -push, Side::Right => push }
    }

    pub fn enter(&mut self) { self.est = 0.0; self.overflow = 0.0; }

    pub fn on_move(&mut self, dx: i32) -> EdgeEvent {
        // d < 0 means "toward Windows"
        let d = dx as f64 * self.gain * if self.side == Side::Left { 1.0 } else { -1.0 };
        if self.est == 0.0 && d < 0.0 { self.overflow -= d } else { self.overflow = 0.0 }
        self.est = (self.est + d).clamp(0.0, self.width);
        if self.overflow >= self.threshold { EdgeEvent::Return } else { EdgeEvent::Stay }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> EdgeTracker { let mut e = EdgeTracker::new(Side::Left, 1000.0, 1.0, 50.0); e.enter(); e }

    #[test] fn pin_delta_points_toward_windows() {
        assert_eq!(EdgeTracker::new(Side::Left, 1000.0, 1.0, 50.0).pin_delta(), -2000);
        assert_eq!(EdgeTracker::new(Side::Right, 1000.0, 1.0, 50.0).pin_delta(), 2000);
    }
    #[test] fn pushing_past_edge_returns() { let mut e = t(); assert_eq!(e.on_move(-30), EdgeEvent::Stay); assert_eq!(e.on_move(-30), EdgeEvent::Return); }
    #[test] fn moving_away_resets_overflow() { let mut e = t(); e.on_move(-40); e.on_move(5); e.on_move(-5); assert_eq!(e.on_move(-40), EdgeEvent::Stay); }
    #[test] fn must_travel_back_before_overflow_counts() { let mut e = t(); e.on_move(300); assert_eq!(e.on_move(-320), EdgeEvent::Stay); assert_eq!(e.on_move(-50), EdgeEvent::Return); }
    #[test] fn far_edge_clamps() { let mut e = t(); e.on_move(5000); e.on_move(-1000); assert_eq!(e.on_move(-50), EdgeEvent::Return); }
    #[test] fn right_side_mirrors() { let mut e = EdgeTracker::new(Side::Right, 1000.0, 1.0, 50.0); e.enter(); assert_eq!(e.on_move(60), EdgeEvent::Return); }
}
