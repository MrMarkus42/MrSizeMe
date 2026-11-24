//! Ring diagram layout: the chosen folder in the middle, its contents as
//! rings around it. Angles are in turns (0 = top, clockwise, 1 = full circle).

use crate::tree::{Measure, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub id: NodeId,
    /// Ring number, 1 = innermost ring.
    pub depth: u16,
    pub start: f32,
    pub end: f32,
    /// The segment of ring 1 this one belongs to (for colours).
    pub top: u32,
}

impl Segment {
    pub fn span(&self) -> f32 {
        self.end - self.start
    }

    pub fn contains_angle(&self, turn: f32) -> bool {
        turn >= self.start && turn < self.end
    }
}

/// Lays out up to `max_depth` rings below `root`. Parts thinner than
/// `min_span` turns are left out (their space stays empty).
pub fn layout(tree: &Tree, root: NodeId, max_depth: u16, measure: Measure, min_span: f32) -> Vec<Segment> {
    let mut out = Vec::new();
    // (folder, start, end, depth of its children, top index)
    let mut stack = vec![(root, 0.0f32, 1.0f32, 1u16, u32::MAX)];
    while let Some((id, start, end, depth, top)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let total = tree.value(id, measure);
        if total == 0 {
            continue;
        }
        let mut kids: Vec<NodeId> =
            tree.children(id).iter().copied().filter(|&c| tree.value(c, measure) > 0).collect();
        kids.sort_by_key(|&c| std::cmp::Reverse(tree.value(c, measure)));
        let span = end - start;
        let mut a = start;
        for c in kids {
            let part = span * (tree.value(c, measure) as f64 / total as f64) as f32;
            let seg_top = if depth == 1 { out.len() as u32 } else { top };
            if part >= min_span {
                out.push(Segment { id: c, depth, start: a, end: (a + part).min(end), top: seg_top });
                if tree.node(c).is_dir() {
                    stack.push((c, a, (a + part).min(end), depth + 1, seg_top));
                }
            }
            a += part;
        }
    }
    out
}

/// The angle (in turns) of a point relative to the centre: 0 = straight
/// up, growing clockwise (screen y grows downwards).
pub fn turn_of(dx: f32, dy: f32) -> f32 {
    (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, NO_PARENT, Node, ROOT};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn sample() -> Tree {
        let file = |name: &str, size| {
            let mut n = Node::new(OsStr::new(name).into(), NO_PARENT, Kind::File);
            n.size = size;
            n
        };
        let mut t = Tree::new(PathBuf::from("/x"), Kind::Dir);
        let first =
            t.add_children(ROOT, [Node::new(OsStr::new("d").into(), NO_PARENT, Kind::Dir), file("a", 250)]);
        t.add_children(first, [file("b", 500), file("c", 250), file("tiny", 0)]);
        t.sum_up();
        t
    }

    #[test]
    fn spans_follow_sizes() {
        let t = sample();
        let segs = layout(&t, ROOT, 6, Measure::Size, 0.0);
        assert_eq!(segs.len(), 4); // d, a, b, c (empty file left out)
        let d = segs.iter().find(|s| t.node(s.id).name_lossy() == "d").unwrap();
        assert!((d.span() - 0.75).abs() < 1e-5);
        assert_eq!(d.depth, 1);
        let b = segs.iter().find(|s| t.node(s.id).name_lossy() == "b").unwrap();
        assert_eq!(b.depth, 2);
        assert!((b.span() - 0.5).abs() < 1e-5);
        assert!(b.start >= d.start && b.end <= d.end + 1e-6);
        assert_eq!(segs[b.top as usize].id, d.id);
    }

    #[test]
    fn depth_limit_and_thin_parts() {
        let t = sample();
        assert_eq!(layout(&t, ROOT, 1, Measure::Size, 0.0).len(), 2);
        // Counting files: d holds 3 of 4 files.
        let by_count = layout(&t, ROOT, 1, Measure::Files, 0.0);
        assert!((by_count[0].span() - 0.75).abs() < 1e-5);
        assert_eq!(layout(&t, ROOT, 6, Measure::Size, 0.3).len(), 2); // only d and b
    }

    #[test]
    fn angles() {
        assert!((turn_of(0.0, -1.0) - 0.0).abs() < 1e-6); // up
        assert!((turn_of(1.0, 0.0) - 0.25).abs() < 1e-6); // right
        assert!((turn_of(0.0, 1.0) - 0.5).abs() < 1e-6); // down
        assert!((turn_of(-1.0, 0.0) - 0.75).abs() < 1e-6); // left
    }
}
