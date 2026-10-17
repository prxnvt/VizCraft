//! Filling shape outlines. egui only fills convex polygons itself, and several outlines (cloud,
//! document, manual input) are concave, so fills go through this small ear-clipping
//! triangulation into an [`egui::epaint::Mesh`].

use egui::Pos2;

/// Above this many points a polygon is filled as a fan instead: ear clipping is quadratic, and
/// only a hostile file produces outlines this dense.
const MAX_EAR_POINTS: usize = 2048;

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Twice the signed area (positive when the points run counter-clockwise in a y-up frame).
fn area2(pts: &[Pos2]) -> f32 {
    let n = pts.len();
    (0..n).map(|i| pts.get(i).zip(pts.get((i + 1) % n)).map_or(0.0, |(a, b)| a.x * b.y - b.x * a.y)).sum()
}

/// Whether `p` lies inside triangle `abc` (wound with sign `s`) or on its edges. A point on an
/// edge must block the ear too: a reflex vertex lying exactly on the cut would otherwise let the
/// triangle cover space outside the polygon. Points at a corner of the triangle do not count.
fn inside(p: Pos2, a: Pos2, b: Pos2, c: Pos2, s: f32) -> bool {
    if p == a || p == b || p == c {
        return false;
    }
    cross(a, b, p) * s >= 0.0 && cross(b, c, p) * s >= 0.0 && cross(c, a, p) * s >= 0.0
}

/// Triangles covering the simple polygon `pts`, as index triples. A degenerate polygon gives
/// none; one that is not simple (crossing edges from a hand-made file) falls back to a fan for
/// the part ear clipping cannot resolve, so something is always drawn and nothing loops.
pub fn triangulate(pts: &[Pos2]) -> Vec<[u32; 3]> {
    let n = pts.len();
    if n < 3 || pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Vec::new();
    }
    let a2 = area2(pts);
    if a2.abs() < 1e-9 {
        return Vec::new();
    }
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(n - 2);
    if n > MAX_EAR_POINTS {
        fan(&idx, &mut out);
        return out;
    }
    let s = a2.signum();
    let at = |i: usize| pts.get(i).copied().unwrap_or(Pos2::ZERO);
    let mut i = 0usize;
    let mut misses = 0usize;
    while idx.len() > 3 {
        let m = idx.len();
        if misses > m {
            // No ear left: not a simple polygon. Fan what remains.
            fan(&idx, &mut out);
            return out;
        }
        let (Some(&ia), Some(&ib), Some(&ic)) = (idx.get((i + m - 1) % m), idx.get(i % m), idx.get((i + 1) % m)) else { break };
        let (a, b, c) = (at(ia), at(ib), at(ic));
        let turn = cross(a, b, c) * s;
        if turn.abs() <= f32::EPSILON * (a.x.abs() + a.y.abs() + 1.0) {
            // Collinear or repeated point: drop it without a triangle.
            idx.remove(i % m);
            misses = 0;
        } else if turn > 0.0 && !idx.iter().any(|&j| j != ia && j != ib && j != ic && inside(at(j), a, b, c, s)) {
            out.push([ia as u32, ib as u32, ic as u32]);
            idx.remove(i % m);
            misses = 0;
        } else {
            i += 1;
            misses += 1;
        }
        if !idx.is_empty() {
            i %= idx.len();
        }
    }
    if let [a, b, c] = idx[..] {
        out.push([a as u32, b as u32, c as u32]);
    }
    out
}

fn fan(idx: &[usize], out: &mut Vec<[u32; 3]>) {
    let Some(&first) = idx.first() else { return };
    for w in idx.windows(2).skip(1) {
        if let [b, c] = w {
            out.push([first as u32, *b as u32, *c as u32]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn area_of(pts: &[Pos2], tris: &[[u32; 3]]) -> f32 {
        tris.iter()
            .map(|t| {
                let p = |i: u32| pts[i as usize];
                cross(p(t[0]), p(t[1]), p(t[2])).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn convex_and_concave_polygons_are_covered_exactly() {
        let square = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        let t = triangulate(&square);
        assert_eq!(t.len(), 2);
        assert!((area_of(&square, &t) - 100.0).abs() < 1e-3);
        // An L shape (concave), both windings.
        let mut l = vec![pos2(0.0, 0.0), pos2(20.0, 0.0), pos2(20.0, 10.0), pos2(10.0, 10.0), pos2(10.0, 20.0), pos2(0.0, 20.0)];
        for _ in 0..2 {
            let t = triangulate(&l);
            assert_eq!(t.len(), 4);
            assert!((area_of(&l, &t) - 300.0).abs() < 1e-3, "{l:?} {t:?} {}", area_of(&l, &t));
            l.reverse();
        }
    }

    #[test]
    fn degenerate_and_hostile_input_never_loops() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[pos2(0.0, 0.0), pos2(1.0, 1.0)]).is_empty());
        assert!(triangulate(&[pos2(0.0, 0.0), pos2(1.0, 1.0), pos2(2.0, 2.0)]).is_empty());
        assert!(triangulate(&[pos2(f32::NAN, 0.0), pos2(1.0, 0.0), pos2(0.0, 1.0)]).is_empty());
        // A bow tie crosses itself; it still gets triangles and terminates.
        let bow = [pos2(0.0, 0.0), pos2(10.0, 10.0), pos2(10.0, 0.0), pos2(0.0, 10.0)];
        let _ = triangulate(&bow);
        // Repeated points.
        let rep = [pos2(0.0, 0.0), pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        assert!((area_of(&rep, &triangulate(&rep)) - 100.0).abs() < 1e-3);
    }
}
