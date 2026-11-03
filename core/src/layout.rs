//! Monitor layout: which device sits on which monitor edge, and when the pointer enters it.

use serde::{Deserialize, Serialize};

use crate::edge::Side;

/// Which edge of a Windows monitor a device is attached to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge { Left, Right, Top, Bottom }

/// Right/bottom exclusive (Windows convention).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Monitor { pub id: String, pub rect: Rect }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment { pub device_id: String, pub monitor_id: String, pub edge: Edge }

/// One device per edge, and each device attached once.
pub fn validate(attachments: &[Attachment]) -> Result<(), String> {
    for (i, a) in attachments.iter().enumerate() {
        for b in &attachments[i + 1..] {
            if a.monitor_id == b.monitor_id && a.edge == b.edge {
                return Err(format!("two devices on the {:?} edge of {}", a.edge, a.monitor_id));
            }
            if a.device_id == b.device_id {
                return Err(format!("device {} is attached twice", a.device_id));
            }
        }
    }
    Ok(())
}

/// The remote edge that faces Windows, as the edge tracker's side.
/// Device to the right/below → its left/top faces Windows → "toward Windows" is negative.
pub fn side(edge: Edge) -> Side {
    match edge { Edge::Right | Edge::Bottom => Side::Left, Edge::Left | Edge::Top => Side::Right }
}

/// The attachment the pointer is pushing into, if any.
pub fn entry<'a>(attachments: &'a [Attachment], monitors: &[Monitor], cursor: (i32, i32), dx: i32, dy: i32) -> Option<&'a Attachment> {
    let (x, y) = cursor;
    attachments.iter().find(|a| {
        let Some(m) = monitors.iter().find(|m| m.id == a.monitor_id) else { return false };
        let r = m.rect;
        if x < r.left || x >= r.right || y < r.top || y >= r.bottom { return false }
        let pushing = match a.edge {
            Edge::Right => x == r.right - 1 && dx > 0,
            Edge::Left => x == r.left && dx < 0,
            Edge::Top => y == r.top && dy < 0,
            Edge::Bottom => y == r.bottom - 1 && dy > 0,
        };
        // A monitor beyond this edge at the cursor's position takes the pointer instead.
        let neighbour = monitors.iter().any(|o| o.id != m.id && match a.edge {
            Edge::Right => o.rect.left == r.right && (o.rect.top..o.rect.bottom).contains(&y),
            Edge::Left => o.rect.right == r.left && (o.rect.top..o.rect.bottom).contains(&y),
            Edge::Top => o.rect.bottom == r.top && (o.rect.left..o.rect.right).contains(&x),
            Edge::Bottom => o.rect.top == r.bottom && (o.rect.left..o.rect.right).contains(&x),
        });
        pushing && !neighbour
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::Side;

    fn mon(id: &str, l: i32, t: i32, r: i32, b: i32) -> Monitor { Monitor { id: id.into(), rect: Rect { left: l, top: t, right: r, bottom: b } } }
    fn att(d: &str, m: &str, e: Edge) -> Attachment { Attachment { device_id: d.into(), monitor_id: m.into(), edge: e } }

    #[test] fn right_edge_push_enters() {
        let a = [att("ipad", "M1", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
        assert_eq!(entry(&a, &m, (1919, 500), 3, 0).map(|x| x.device_id.as_str()), Some("ipad"));
    }
    #[test] fn moving_inward_at_edge_does_not_enter() {
        let a = [att("ipad", "M1", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
        assert!(entry(&a, &m, (1919, 500), -3, 0).is_none());
    }
    #[test] fn left_edge_enters_left_device() {
        let a = [att("ipad", "M1", Edge::Right), att("s24", "M1", Edge::Left)]; let m = [mon("M1", 0, 0, 1920, 1080)];
        assert_eq!(entry(&a, &m, (0, 10), -2, 0).unwrap().device_id, "s24");
    }
    #[test] fn top_and_bottom_edges() {
        let a = [att("up", "M1", Edge::Top), att("down", "M1", Edge::Bottom)]; let m = [mon("M1", 0, 0, 1920, 1080)];
        assert_eq!(entry(&a, &m, (500, 0), 0, -2).unwrap().device_id, "up");
        assert_eq!(entry(&a, &m, (500, 1079), 0, 2).unwrap().device_id, "down");
    }
    #[test] fn neighbouring_monitor_wins_over_device() { // Review Focus 5
        let a = [att("ipad", "M1", Edge::Right)];
        let m = [mon("M1", 0, 0, 1920, 1080), mon("M2", 1920, 0, 3840, 1080)];
        assert!(entry(&a, &m, (1919, 500), 3, 0).is_none());
    }
    #[test] fn neighbour_only_covering_part_of_edge() {
        let a = [att("ipad", "M1", Edge::Right)];
        let m = [mon("M1", 0, 0, 1920, 1080), mon("M2", 1920, 0, 3840, 400)];
        assert!(entry(&a, &m, (1919, 200), 3, 0).is_none());
        assert!(entry(&a, &m, (1919, 800), 3, 0).is_some());
    }
    #[test] fn missing_monitor_disables_attachment() { // Review Focus 2
        let a = [att("ipad", "GONE", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
        assert!(entry(&a, &m, (1919, 500), 3, 0).is_none());
    }
    #[test] fn same_edge_twice_is_invalid() { // Review Focus 4
        let e = validate(&[att("a", "M1", Edge::Right), att("b", "M1", Edge::Right)]).unwrap_err();
        assert!(e.contains("Right") && e.contains("M1"));
    }
    #[test] fn same_device_twice_is_invalid() {
        assert!(validate(&[att("a", "M1", Edge::Right), att("a", "M1", Edge::Left)]).is_err());
    }
    #[test] fn distinct_edges_are_valid() { assert!(validate(&[att("a", "M1", Edge::Right), att("b", "M1", Edge::Left)]).is_ok()); }
    #[test] fn side_faces_windows() {
        assert_eq!(side(Edge::Right), Side::Left);
        assert_eq!(side(Edge::Left), Side::Right);
        assert_eq!(side(Edge::Top), Side::Right);
        assert_eq!(side(Edge::Bottom), Side::Left);
    }
}
