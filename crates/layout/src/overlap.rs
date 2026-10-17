//! Node overlap removal for the force and radial layouts.
//!
//! Three stages, each cheaper to reason about than the last is to look at:
//!
//! 1. Pairwise pushes along the axis of least penetration (a few bounded passes), which fix the
//!    small overlaps a force layout leaves while keeping its shape.
//! 2. Uniform scaling of the centres about their centroid by the smallest factor that separates
//!    every remaining overlapping pair. Scaling never creates a new overlap, because pairs that
//!    are already apart only move further apart.
//! 3. If that would need an absurd factor, a row placement in reading order, which cannot overlap.

/// Most push passes.
const PUSH_PASSES: usize = 30;
/// Largest uniform scale factor accepted before falling back to rows.
const MAX_SCALE: f64 = 1.0e3;

/// Moves the centres `c` (sizes in `sizes`) so no two rectangles overlap, aiming for `spacing`
/// between them.
pub(crate) fn remove(c: &mut [(f64, f64)], sizes: &[(f64, f64)], spacing: f64) {
    let n = c.len().min(sizes.len());
    if n < 2 {
        return;
    }
    let c = &mut c[..n];
    let sizes = &sizes[..n];
    if push(c, sizes, spacing) {
        return;
    }
    for _ in 0..3 {
        match needed_scale(c, sizes, spacing) {
            Some(s) if s <= 1.0 => return,
            Some(s) if s <= MAX_SCALE => scale(c, s * (1.0 + 1e-9) + 1e-12),
            _ => break,
        }
    }
    if !any_overlap(c, sizes) {
        return;
    }
    rows(c, sizes, spacing);
}

fn by_left(c: &[(f64, f64)], sizes: &[(f64, f64)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..c.len()).collect();
    order.sort_by(|&a, &b| (c[a].0 - sizes[a].0 / 2.0).total_cmp(&(c[b].0 - sizes[b].0 / 2.0)).then(a.cmp(&b)));
    order
}

/// Required extra separation (x, y) for a pair; both positive means they are too close.
fn penetration(c: &[(f64, f64)], sizes: &[(f64, f64)], i: usize, j: usize, gap: f64) -> (f64, f64, f64, f64) {
    let dx = c[j].0 - c[i].0;
    let dy = c[j].1 - c[i].1;
    let nx = (sizes[i].0 + sizes[j].0) / 2.0 + gap - dx.abs();
    let ny = (sizes[i].1 + sizes[j].1) / 2.0 + gap - dy.abs();
    (nx, ny, dx, dy)
}

/// Pairwise push passes. Returns true when a pass found nothing to fix.
fn push(c: &mut [(f64, f64)], sizes: &[(f64, f64)], spacing: f64) -> bool {
    let n = c.len();
    for _ in 0..PUSH_PASSES {
        let order = by_left(c, sizes);
        let mut moved = false;
        for a in 0..n {
            let i = order[a];
            for &j in &order[a + 1..] {
                if c[j].0 - sizes[j].0 / 2.0 > c[i].0 + sizes[i].0 / 2.0 + spacing {
                    break;
                }
                let (nx, ny, dx, dy) = penetration(c, sizes, i, j, spacing);
                if nx > 1e-9 && ny > 1e-9 {
                    moved = true;
                    if nx <= ny {
                        let s = if dx > 0.0 || (dx == 0.0 && i < j) { 1.0 } else { -1.0 };
                        c[i].0 -= s * nx / 2.0;
                        c[j].0 += s * nx / 2.0;
                    } else {
                        let s = if dy > 0.0 || (dy == 0.0 && i < j) { 1.0 } else { -1.0 };
                        c[i].1 -= s * ny / 2.0;
                        c[j].1 += s * ny / 2.0;
                    }
                }
            }
        }
        if !moved {
            return true;
        }
    }
    false
}

/// Smallest uniform scale that separates every pair closer than `spacing`; `None` when two
/// centres coincide (scaling cannot separate them).
fn needed_scale(c: &[(f64, f64)], sizes: &[(f64, f64)], spacing: f64) -> Option<f64> {
    let order = by_left(c, sizes);
    let mut s = 1.0f64;
    for (a, &i) in order.iter().enumerate() {
        for &j in &order[a + 1..] {
            if c[j].0 - sizes[j].0 / 2.0 >= c[i].0 + sizes[i].0 / 2.0 + spacing {
                break;
            }
            let (nx, ny, dx, dy) = penetration(c, sizes, i, j, spacing);
            if nx > 1e-9 && ny > 1e-9 {
                let tx = (sizes[i].0 + sizes[j].0) / 2.0 + spacing;
                let ty = (sizes[i].1 + sizes[j].1) / 2.0 + spacing;
                let fx = if dx.abs() > 1e-9 { tx / dx.abs() } else { f64::INFINITY };
                let fy = if dy.abs() > 1e-9 { ty / dy.abs() } else { f64::INFINITY };
                let f = fx.min(fy);
                if !f.is_finite() {
                    return None;
                }
                s = s.max(f);
            }
        }
    }
    Some(s)
}

fn scale(c: &mut [(f64, f64)], s: f64) {
    let n = c.len() as f64;
    let mx = c.iter().map(|p| p.0).sum::<f64>() / n;
    let my = c.iter().map(|p| p.1).sum::<f64>() / n;
    for p in c.iter_mut() {
        p.0 = mx + (p.0 - mx) * s;
        p.1 = my + (p.1 - my) * s;
    }
}

/// Whether any two rectangles overlap (no spacing required).
pub(crate) fn any_overlap(c: &[(f64, f64)], sizes: &[(f64, f64)]) -> bool {
    let order = by_left(c, sizes);
    for (a, &i) in order.iter().enumerate() {
        for &j in &order[a + 1..] {
            if c[j].0 - sizes[j].0 / 2.0 >= c[i].0 + sizes[i].0 / 2.0 {
                break;
            }
            let (nx, ny, _, _) = penetration(c, sizes, i, j, 0.0);
            if nx > 1e-9 && ny > 1e-9 {
                return true;
            }
        }
    }
    false
}

/// Last resort: rows in reading order of the current positions.
fn rows(c: &mut [(f64, f64)], sizes: &[(f64, f64)], spacing: f64) {
    let mut order: Vec<usize> = (0..c.len()).collect();
    order.sort_by(|&a, &b| c[a].1.total_cmp(&c[b].1).then(c[a].0.total_cmp(&c[b].0)).then(a.cmp(&b)));
    let area: f64 = sizes.iter().map(|s| (s.0 + spacing) * (s.1 + spacing)).sum();
    let widest = sizes.iter().map(|s| s.0).fold(0.0, f64::max);
    let target = widest.max(area.sqrt() * 1.2);
    let (mut x, mut y, mut row_h) = (0.0f64, 0.0f64, 0.0f64);
    for i in order {
        let (w, h) = sizes[i];
        if x > 0.0 && x + w > target {
            x = 0.0;
            y += row_h + spacing;
            row_h = 0.0;
        }
        c[i] = (x + w / 2.0, y + h / 2.0);
        x += w + spacing;
        row_h = row_h.max(h);
    }
}
