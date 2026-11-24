//! Squarified treemap layout: splits a rectangle into tiles whose areas
//! match the sizes, keeping the tiles as square as possible.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w: w.max(0.0), h: h.max(0.0) }
    }

    pub fn area(&self) -> f32 {
        self.w * self.h
    }

    /// The rectangle made smaller by `d` on every side.
    pub fn shrink(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }

    /// The rectangle without a strip of height `h` at the top.
    pub fn below(&self, h: f32) -> Rect {
        let h = h.min(self.h);
        Rect::new(self.x, self.y + h, self.w, self.h - h)
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Lays out `sizes` (largest first gives the best result) in `area`.
/// Returns one rectangle per size, in the same order. Zero sizes get empty rectangles.
pub fn squarify(sizes: &[u64], area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::new(area.x, area.y, 0.0, 0.0); sizes.len()];
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if total <= 0.0 || area.area() <= 0.0 {
        return out;
    }
    // Areas in square pixels.
    let scale = f64::from(area.area()) / total;
    let areas: Vec<f64> = sizes.iter().map(|&s| s as f64 * scale).collect();

    let mut free = area;
    let mut start = 0;
    while start < areas.len() {
        if areas[start] <= 0.0 {
            start += 1;
            continue;
        }
        let side = f64::from(free.w.min(free.h));
        // Grow the row while the worst aspect ratio gets better.
        let mut end = start + 1;
        let mut row_sum = areas[start];
        let mut best = worst(&areas[start..end], row_sum, side);
        while end < areas.len() && areas[end] > 0.0 {
            let next = worst(&areas[start..=end], row_sum + areas[end], side);
            if next > best {
                break;
            }
            row_sum += areas[end];
            best = next;
            end += 1;
        }
        free = place_row(&areas[start..end], row_sum, free, &mut out[start..end]);
        start = end;
    }
    out
}

/// Worst aspect ratio of the tiles in a row laid along a side of length `side`.
fn worst(row: &[f64], sum: f64, side: f64) -> f64 {
    let (mut max, mut min) = (f64::MIN, f64::MAX);
    for &a in row {
        max = max.max(a);
        min = min.min(a);
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    (s2 * max / sum2).max(sum2 / (s2 * min))
}

/// Places a row along the shorter side of `free`; returns the space left.
fn place_row(row: &[f64], sum: f64, free: Rect, out: &mut [Rect]) -> Rect {
    if free.w >= free.h {
        // Column on the left side.
        let width = (sum / f64::from(free.h)) as f32;
        let width = width.min(free.w);
        let mut y = free.y;
        for (a, r) in row.iter().zip(out.iter_mut()) {
            let h = (a / sum) as f32 * free.h;
            *r = Rect::new(free.x, y, width, h);
            y += h;
        }
        Rect::new(free.x + width, free.y, free.w - width, free.h)
    } else {
        // Row along the top.
        let height = (sum / f64::from(free.w)) as f32;
        let height = height.min(free.h);
        let mut x = free.x;
        for (a, r) in row.iter().zip(out.iter_mut()) {
            let w = (a / sum) as f32 * free.w;
            *r = Rect::new(x, free.y, w, height);
            x += w;
        }
        Rect::new(free.x, free.y + height, free.w, free.h - height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: &Rect, b: &Rect) -> f32 {
        let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        w.max(0.0) * h.max(0.0)
    }

    #[test]
    fn areas_match_sizes_and_stay_inside() {
        let area = Rect::new(10.0, 20.0, 600.0, 400.0);
        let sizes = [600u64, 600, 400, 300, 200, 100, 100, 50, 0, 1];
        let total: u64 = sizes.iter().sum();
        let rects = squarify(&sizes, area);
        assert_eq!(rects.len(), sizes.len());
        for (s, r) in sizes.iter().zip(&rects) {
            let want = *s as f32 / total as f32 * area.area();
            assert!((r.area() - want).abs() < want * 0.01 + 0.5, "{s}: {} vs {want}", r.area());
            assert!(r.x >= area.x - 0.01 && r.y >= area.y - 0.01);
            assert!(r.x + r.w <= area.x + area.w + 0.05 && r.y + r.h <= area.y + area.h + 0.05);
        }
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(overlap(&rects[i], &rects[j]) < 0.5, "{i} and {j} overlap");
            }
        }
    }

    #[test]
    fn tiles_are_reasonably_square() {
        let rects = squarify(&[1; 16], Rect::new(0.0, 0.0, 400.0, 400.0));
        for r in rects {
            let ratio = r.w.max(r.h) / r.w.min(r.h);
            assert!(ratio < 2.0, "{r:?}");
        }
    }

    #[test]
    fn empty_input() {
        assert!(squarify(&[], Rect::new(0.0, 0.0, 10.0, 10.0)).is_empty());
        let r = squarify(&[0, 0], Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(r.iter().all(|r| r.area() == 0.0));
        let r = squarify(&[5], Rect::new(0.0, 0.0, 0.0, 10.0));
        assert_eq!(r[0].area(), 0.0);
    }
}
