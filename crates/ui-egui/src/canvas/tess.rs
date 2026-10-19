//! Geometry for painting shapes, in world space and free of the model:
//!
//! - Filling outlines. egui only fills convex polygons itself, and several outlines (cloud,
//!   document, manual input) are concave, so fills go through this small ear-clipping
//!   triangulation into an [`egui::epaint::Mesh`].
//! - Gradient fills: the fill triangles cut into bands between colour stops (linear, exact, hard
//!   stops included) or subdivided until the colour changes little across a triangle (radial).
//! - Dash patterns: a polyline cut into dashes.
//! - Soft edges, blurred shadows and glows: an outline offset along its normals, and a strip
//!   between two such rings whose colour fades from the inner ring to the outer.

use egui::epaint::Mesh;
use egui::{Color32, Pos2, Vec2};

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

/// Most rings [`even_odd`] sorts into outlines and holes; past it each ring fills on its own.
const MAX_RINGS: usize = 256;

/// Whether `p` is inside the polygon `pts` (crossing count).
fn in_ring(p: Pos2, pts: &[Pos2]) -> bool {
    let n = pts.len();
    let mut inside = false;
    for i in 0..n {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + n - 1) % n)) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Whether segments `ab` and `cd` cross at a point inside both (touching at an end does not
/// count, so a bridge may start and end on polygon corners).
fn crosses(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    if a == c || a == d || b == c || b == d {
        return false;
    }
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn edges(pts: &[Pos2]) -> impl Iterator<Item = (Pos2, Pos2)> + '_ {
    let n = pts.len();
    (0..n).filter_map(move |i| Some((*pts.get(i)?, *pts.get((i + 1) % n)?)))
}

/// `outer` with `hole` cut in: the hole's rightmost point joined to the nearest point of
/// `outer` that the joining segment can reach without crossing `outer`, the hole itself or any
/// of `others`, giving one polygon that runs out along the bridge, round the hole and back.
/// `None` when no such point exists.
fn bridge(outer: &[Pos2], hole: &[Pos2], others: &[Vec<Pos2>]) -> Option<Vec<Pos2>> {
    let (mi, m) = hole.iter().copied().enumerate().max_by(|a, b| a.1.x.total_cmp(&b.1.x))?;
    let mut order: Vec<usize> = (0..outer.len()).collect();
    order.sort_by(|&i, &j| {
        let d = |k: usize| outer.get(k).map_or(f32::INFINITY, |q| q.distance_sq(m));
        d(i).total_cmp(&d(j))
    });
    let pi = order.into_iter().find(|&i| {
        let Some(&p) = outer.get(i) else { return false };
        !edges(outer).any(|(a, b)| crosses(m, p, a, b))
            && !edges(hole).any(|(a, b)| crosses(m, p, a, b))
            && !others.iter().any(|o| edges(o).any(|(a, b)| crosses(m, p, a, b)))
    })?;
    let p = *outer.get(pi)?;
    let mut out = Vec::with_capacity(outer.len() + hole.len() + 2);
    out.extend_from_slice(outer.get(..=pi)?);
    out.extend_from_slice(hole.get(mi..)?);
    out.extend_from_slice(hole.get(..=mi)?);
    out.push(p);
    out.extend_from_slice(outer.get(pi + 1..)?);
    Some(out)
}

/// The even-odd fill of closed `rings` as pieces of points and triangles: a ring inside an odd
/// number of others is a hole, cut into the smallest ring around it by bridging, so the fill
/// shows through it. A ring that cannot be bridged, or more rings or points than the limits,
/// fill each ring on its own (the fan fallback of [`triangulate`] still applies).
pub fn even_odd(rings: &[Vec<Pos2>]) -> Vec<(Vec<Pos2>, Vec<[u32; 3]>)> {
    let alone = |r: &Vec<Pos2>| (r.clone(), triangulate(r));
    let total: usize = rings.iter().map(Vec::len).sum();
    if rings.len() < 2 || rings.len() > MAX_RINGS || total > MAX_EAR_POINTS {
        return rings.iter().map(alone).collect();
    }
    let area: Vec<f32> = rings.iter().map(|r| area2(r).abs()).collect();
    let at = |i: usize| area.get(i).copied().unwrap_or(0.0);
    // The rings around each ring, judged by its first point; only larger rings can hold it.
    let around: Vec<Vec<usize>> = rings
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let Some(&p) = r.first() else { return Vec::new() };
            (0..rings.len()).filter(|&j| j != i && at(j) > at(i) && rings.get(j).is_some_and(|o| in_ring(p, o))).collect()
        })
        .collect();
    let depth = |i: usize| around.get(i).map_or(0, Vec::len);
    let parent = |i: usize| around.get(i).and_then(|a| a.iter().copied().min_by(|&x, &y| at(x).total_cmp(&at(y))));
    let mut out = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if depth(i) % 2 == 1 {
            continue;
        }
        let sign = area2(ring).signum();
        let mut holes: Vec<Vec<Pos2>> =
            (0..rings.len()).filter(|&h| depth(h) % 2 == 1 && parent(h) == Some(i)).filter_map(|h| rings.get(h).cloned()).collect();
        // Holes wind against the outline, rightmost first so each bridge reaches the outline.
        for h in &mut holes {
            if area2(h).signum() == sign {
                h.reverse();
            }
        }
        holes.sort_by(|a, b| {
            let right = |r: &Vec<Pos2>| r.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
            right(b).total_cmp(&right(a))
        });
        let mut merged = ring.clone();
        let mut left: Vec<Vec<Pos2>> = Vec::new();
        for (k, h) in holes.iter().enumerate() {
            let rest: Vec<Vec<Pos2>> = holes.get(k + 1..).unwrap_or_default().to_vec();
            match bridge(&merged, h, &rest) {
                Some(m) => merged = m,
                None => left.push(h.clone()),
            }
        }
        out.push((merged.clone(), triangulate(&merged)));
        // A hole that could not be cut in is drawn over as its own fill rather than lost.
        out.extend(left.iter().map(alone));
    }
    out
}

/// Most dashes one polyline is cut into; past it the rest of the line is left solid, so a
/// hairline dash pattern on a huge outline cannot allocate without bound.
const MAX_DASHES: usize = 20_000;

/// Cut a polyline into dashes. `pattern` alternates dash and gap lengths (world units, each
/// positive); `closed` adds the closing segment. An empty or degenerate pattern gives the whole
/// line back as one piece.
pub fn dashes(pts: &[Pos2], closed: bool, pattern: &[f32]) -> Vec<Vec<Pos2>> {
    let mut line: Vec<Pos2> = pts.to_vec();
    if closed && let Some(&first) = pts.first() {
        line.push(first);
    }
    let total: f32 = pattern.iter().sum();
    if pattern.is_empty() || !total.is_finite() || total <= 1e-3 || pattern.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return vec![line];
    }
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut k = 0usize;
    let mut left = pattern.first().copied().unwrap_or(1.0);
    let mut on = true;
    let mut cur: Vec<Pos2> = line.first().copied().into_iter().collect();
    for (seg, w) in line.windows(2).enumerate() {
        let &[a, b] = w else { continue };
        let len = a.distance(b);
        if !len.is_finite() {
            continue;
        }
        let mut pos = 0.0f32;
        while len - pos > left {
            pos += left;
            let p = a + (b - a) * (pos / len);
            if on {
                push_new(&mut cur, p);
                out.push(std::mem::take(&mut cur));
                if out.len() >= MAX_DASHES {
                    // Bound reached: draw the rest of the line in one piece.
                    let mut rest = vec![p];
                    rest.extend(line.iter().skip(seg.saturating_add(1)));
                    out.push(rest);
                    return out;
                }
            } else {
                cur = vec![p];
            }
            on = !on;
            k = (k + 1) % pattern.len();
            left = pattern.get(k).copied().unwrap_or(1.0).max(1e-3);
        }
        left -= len - pos;
        if on {
            push_new(&mut cur, b);
        }
    }
    if on && cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Push `p` unless it repeats the last point (a dash ending exactly on a corner).
fn push_new(line: &mut Vec<Pos2>, p: Pos2) {
    if line.last() != Some(&p) {
        line.push(p);
    }
}

/// Lengthen an open polyline by `d` at both ends along its end segments (square line caps).
pub fn extend_ends(line: &mut [Pos2], d: f32) {
    let n = line.len();
    if n < 2 {
        return;
    }
    let dir = |from: Pos2, to: Pos2| -> Vec2 {
        let v = to - from;
        let l = v.length();
        if l > 1e-6 { v / l } else { Vec2::ZERO }
    };
    if let (Some(&a), Some(&b)) = (line.first(), line.get(1)) {
        let v = dir(b, a);
        if let Some(p) = line.first_mut() {
            *p = a + v * d;
        }
    }
    if let (Some(&a), Some(&b)) = (line.get(n - 2), line.get(n - 1)) {
        let v = dir(a, b);
        if let Some(p) = line.get_mut(n - 1) {
            *p = b + v * d;
        }
    }
}

/// The closed ring `pts` moved `d` outwards along its vertex normals (negative moves inwards).
/// Corners are mitred up to four times `d`, so sharp points do not shoot off.
pub fn offset_ring(pts: &[Pos2], d: f32) -> Vec<Pos2> {
    let n = pts.len();
    if n < 3 || !d.is_finite() {
        return pts.to_vec();
    }
    // Outward is to the left of travel for a negative (clockwise on screen) signed area.
    let side = if area2(pts) > 0.0 { 1.0 } else { -1.0 };
    let at = |i: usize| pts.get(i % n).copied().unwrap_or(Pos2::ZERO);
    let normal = |a: Pos2, b: Pos2| -> Vec2 {
        let v = b - a;
        let l = v.length();
        if l > 1e-6 { Vec2::new(v.y, -v.x) / l * side } else { Vec2::ZERO }
    };
    (0..n)
        .map(|i| {
            let (prev, p, next) = (at(i + n - 1), at(i), at(i + 1));
            let (n1, n2) = (normal(prev, p), normal(p, next));
            let m = n1 + n2;
            let ml = m.length();
            if ml < 1e-6 {
                return p + n1 * d;
            }
            let m = m / ml;
            // Mitre length 1 / cos(half angle), limited.
            let cos = m.dot(if n1 == Vec2::ZERO { n2 } else { n1 }).max(0.25);
            p + m * (d / cos)
        })
        .collect()
}

/// Whether `inner` is a usable inset of `ring`: the same winding and a real area.
pub fn inset_ok(ring: &[Pos2], inner: &[Pos2]) -> bool {
    let (a, b) = (area2(ring), area2(inner));
    let n = ring.len();
    // An inset past the middle can keep its winding while every edge runs backwards.
    let edges_agree = n == inner.len()
        && (0..n).all(|i| match (ring.get(i), ring.get((i + 1) % n), inner.get(i), inner.get((i + 1) % n)) {
            (Some(&p), Some(&q), Some(&r), Some(&u)) => (q - p).dot(u - r) >= 0.0,
            _ => false,
        });
    edges_agree && a.signum() == b.signum() && b.abs() > 1e-3 && b.abs() < a.abs()
}

/// A strip between two rings of equal length (`inner[i]` pairs with `outer[i]`), coloured
/// `c_in` along `inner` and `c_out` along `outer`; closed around.
pub fn strip(inner: &[Pos2], outer: &[Pos2], c_in: Color32, c_out: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let n = inner.len().min(outer.len());
    if n < 2 {
        return mesh;
    }
    mesh.reserve_vertices(2 * n);
    mesh.reserve_triangles(2 * n);
    for (a, b) in inner.iter().zip(outer) {
        mesh.colored_vertex(*a, c_in);
        mesh.colored_vertex(*b, c_out);
    }
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        mesh.add_triangle(2 * i, 2 * i + 1, 2 * j);
        mesh.add_triangle(2 * j, 2 * i + 1, 2 * j + 1);
    }
    mesh
}

/// A mesh of `pts` filled with `tris` in one colour.
pub fn solid(pts: &[Pos2], tris: &[[u32; 3]], c: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(pts.len());
    mesh.reserve_triangles(tris.len());
    for p in pts {
        mesh.colored_vertex(*p, c);
    }
    for [a, b, c] in tris {
        mesh.add_triangle(*a, *b, *c);
    }
    mesh
}

/// One gradient colour stop: offset 0 to 1 and an unmultiplied RGBA colour.
pub type Stop = (f32, [u8; 4]);

fn lerp_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let mut out = [0u8; 4];
    for ((o, x), y) in out.iter_mut().zip(a).zip(b) {
        *o = (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn to_color(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// The colour at `t` (sorted stops; before the first and after the last the end colours hold).
fn color_at(stops: &[Stop], t: f32) -> [u8; 4] {
    let Some(&(o0, c0)) = stops.first() else { return [0; 4] };
    if t.is_nan() || t <= o0 {
        return c0;
    }
    for w in stops.windows(2) {
        if let [(oa, ca), (ob, cb)] = w
            && t <= *ob
        {
            let span = ob - oa;
            return if span > 1e-6 { lerp_rgba(*ca, *cb, (t - oa) / span) } else { *cb };
        }
    }
    stops.last().map_or(c0, |s| s.1)
}

/// Clip a convex polygon (with gradient parameters) to `t >= lo` (`keep_above`) or `t <= lo`.
fn clip(poly: &[(Pos2, f32)], lo: f32, keep_above: bool) -> Vec<(Pos2, f32)> {
    let inside = |t: f32| if keep_above { t >= lo } else { t <= lo };
    let mut out = Vec::with_capacity(poly.len() + 2);
    let n = poly.len();
    for i in 0..n {
        let (Some(&(pa, ta)), Some(&(pb, tb))) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
        if inside(ta) {
            out.push((pa, ta));
        }
        if inside(ta) != inside(tb) && (tb - ta).abs() > 1e-9 {
            let f = (lo - ta) / (tb - ta);
            out.push((pa + (pb - pa) * f, lo));
        }
    }
    out
}

/// Most triangles one gradient fill makes.
const MAX_GRADIENT_TRIS: usize = 60_000;

/// A linear gradient fill: `t(p)` must be affine (a projection onto the gradient axis). Each fill
/// triangle is cut into the bands between stops, and every band's vertices take the colour that
/// band interpolates, so colours are exact and hard stops (two stops at one offset) stay sharp.
pub fn linear_gradient(pts: &[Pos2], tris: &[[u32; 3]], t: impl Fn(Pos2) -> f32, stops: &[Stop]) -> Mesh {
    let mut mesh = Mesh::default();
    let Some(&(first, c_first)) = stops.first() else { return mesh };
    let (last, c_last) = stops.last().copied().unwrap_or((first, c_first));
    // Bands: below the first stop, between neighbours, above the last.
    let mut bands: Vec<(f32, f32, [u8; 4], [u8; 4])> = vec![(f32::NEG_INFINITY, first, c_first, c_first)];
    for w in stops.windows(2) {
        if let [(oa, ca), (ob, cb)] = w
            && ob > oa
        {
            bands.push((*oa, *ob, *ca, *cb));
        }
    }
    bands.push((last, f32::INFINITY, c_last, c_last));
    let at = |i: u32| pts.get(i as usize).copied().unwrap_or(Pos2::ZERO);
    for tri in tris {
        let poly: Vec<(Pos2, f32)> = tri.iter().map(|&i| (at(i), t(at(i)))).collect();
        for &(lo, hi, ca, cb) in &bands {
            if mesh.indices.len() / 3 >= MAX_GRADIENT_TRIS {
                return mesh;
            }
            let piece = clip(&clip(&poly, lo, true), hi, false);
            if piece.len() < 3 {
                continue;
            }
            let base = mesh.vertices.len() as u32;
            let span = hi - lo;
            for (p, tv) in &piece {
                let f = if span.is_finite() && span > 1e-6 { (tv - lo) / span } else { 0.0 };
                mesh.colored_vertex(*p, to_color(lerp_rgba(ca, cb, f)));
            }
            for k in 1..piece.len() as u32 - 1 {
                mesh.add_triangle(base, base + k, base + k + 1);
            }
        }
    }
    mesh
}

fn pos2_mean(a: Pos2, b: Pos2, c: Pos2) -> Pos2 {
    Pos2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
}

/// A gradient fill whose parameter `t(p)` is not affine (radial): triangles are split at their
/// midpoints until `t` varies by less than a twentieth across each, then coloured per vertex.
pub fn curved_gradient(pts: &[Pos2], tris: &[[u32; 3]], t: impl Fn(Pos2) -> f32, stops: &[Stop]) -> Mesh {
    fn split(out: &mut Vec<[Pos2; 3]>, tri: [Pos2; 3], t: &dyn Fn(Pos2) -> f32, depth: u32) {
        let [a, b, c] = tri;
        let (ab, bc, ca) = (a.lerp(b, 0.5), b.lerp(c, 0.5), c.lerp(a, 0.5));
        // The corners alone can agree while the middle differs (a square around a radial centre).
        let mid = pos2_mean(a, b, c);
        let ts = [a, b, c, ab, bc, ca, mid].map(t);
        let spread = ts.iter().copied().fold(f32::MIN, f32::max) - ts.iter().copied().fold(f32::MAX, f32::min);
        if depth == 0 || spread.is_nan() || spread <= 0.05 || out.len() >= MAX_GRADIENT_TRIS {
            out.push(tri);
            return;
        }
        for t2 in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            split(out, t2, t, depth - 1);
        }
    }
    let at = |i: u32| pts.get(i as usize).copied().unwrap_or(Pos2::ZERO);
    let mut small: Vec<[Pos2; 3]> = Vec::new();
    for [a, b, c] in tris {
        split(&mut small, [at(*a), at(*b), at(*c)], &t, 6);
    }
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(small.len() * 3);
    mesh.reserve_triangles(small.len());
    for tri in small {
        let base = mesh.vertices.len() as u32;
        for p in tri {
            mesh.colored_vertex(p, to_color(color_at(stops, t(p))));
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    mesh
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
    fn a_ring_inside_another_is_a_hole_and_islands_fill_again() {
        let sq = |x0: f32, y0: f32, x1: f32, y1: f32| vec![pos2(x0, y0), pos2(x1, y0), pos2(x1, y1), pos2(x0, y1)];
        let rings = vec![sq(0.0, 0.0, 100.0, 100.0), sq(20.0, 20.0, 50.0, 50.0), sq(60.0, 60.0, 80.0, 80.0), sq(30.0, 30.0, 40.0, 40.0)];
        let pieces = even_odd(&rings);
        let total: f32 = pieces.iter().map(|(p, t)| area_of(p, t)).sum();
        // The square minus two holes, plus the island inside the first hole.
        assert!((total - (10000.0 - 900.0 - 400.0 + 100.0)).abs() < 1e-2, "{total}");
        assert_eq!(pieces.len(), 2, "one bridged outline and the island");
        // One ring, or junk, fills as before.
        assert_eq!(even_odd(&rings[..1]).len(), 1);
        assert!(!even_odd(&[vec![pos2(f32::NAN, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)], sq(0.0, 0.0, 1.0, 1.0)]).is_empty());
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

    #[test]
    fn dashes_follow_the_pattern_around_corners() {
        let line = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        let d = dashes(&line, false, &[4.0, 2.0]);
        // 20 units: dashes at 0-4, 6-10, 12-16, 18-20.
        assert_eq!(d.len(), 4, "{d:?}");
        assert_eq!(d[1], vec![pos2(6.0, 0.0), pos2(10.0, 0.0)]);
        assert_eq!(d[3], vec![pos2(10.0, 8.0), pos2(10.0, 10.0)]);
        let square = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        let total: f32 = dashes(&square, true, &[1.0, 1.0]).iter().map(|p| p.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>()).sum();
        assert!((total - 20.0).abs() < 1e-3, "{total}");
        // Degenerate and hostile patterns draw solid and stay bounded.
        assert_eq!(dashes(&line, false, &[]).len(), 1);
        assert_eq!(dashes(&line, false, &[0.0, 0.0]).len(), 1);
        assert_eq!(dashes(&line, false, &[f32::NAN]).len(), 1);
        let long = [pos2(0.0, 0.0), pos2(1.0e7, 0.0)];
        assert!(dashes(&long, false, &[0.01, 0.01]).len() <= MAX_DASHES + 1);
    }

    #[test]
    fn square_caps_lengthen_open_lines() {
        let mut l = vec![pos2(0.0, 0.0), pos2(10.0, 0.0)];
        extend_ends(&mut l, 2.0);
        assert_eq!(l, vec![pos2(-2.0, 0.0), pos2(12.0, 0.0)]);
    }

    #[test]
    fn rings_offset_outwards_in_both_windings() {
        let mut sq = vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        for _ in 0..2 {
            let out = offset_ring(&sq, 1.0);
            assert!((area2(&out).abs() / 2.0 - 144.0).abs() < 1e-2, "{out:?}");
            let inner = offset_ring(&sq, -1.0);
            assert!(inset_ok(&sq, &inner));
            assert!(!inset_ok(&sq, &offset_ring(&sq, -6.0)), "an inset past the middle turns inside out");
            sq.reverse();
        }
        let m = strip(&sq, &offset_ring(&sq, 2.0), Color32::RED, Color32::TRANSPARENT);
        assert_eq!((m.vertices.len(), m.indices.len()), (8, 24));
    }

    #[test]
    fn linear_gradients_cut_bands_with_exact_colours() {
        let sq = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        let tris = triangulate(&sq);
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        // A hard stop at the middle: left half black, right half white, nothing grey.
        let stops = [(0.0, black), (0.5, black), (0.5, white), (1.0, white)];
        let m = linear_gradient(&sq, &tris, |p| p.x / 10.0, &stops);
        assert!(!m.vertices.is_empty());
        for v in &m.vertices {
            let want = if v.pos.x < 5.0 - 1e-3 {
                Color32::BLACK
            } else if v.pos.x > 5.0 + 1e-3 {
                Color32::WHITE
            } else {
                v.color
            };
            assert_eq!(v.color, want, "{v:?}");
        }
        let area: f32 = m
            .indices
            .chunks(3)
            .map(|t| cross(m.vertices[t[0] as usize].pos, m.vertices[t[1] as usize].pos, m.vertices[t[2] as usize].pos).abs() / 2.0)
            .sum();
        assert!((area - 100.0).abs() < 1e-2, "{area}");
        // A smooth gradient puts grey in the middle.
        let m = linear_gradient(&sq, &tris, |p| p.x / 10.0, &[(0.0, black), (1.0, white)]);
        assert!(m.vertices.iter().any(|v| v.pos.x == 0.0 && v.color == Color32::BLACK));
        let r = curved_gradient(&sq, &tris, |p| (p - pos2(5.0, 5.0)).length() / 5.0, &[(0.0, white), (1.0, black)]);
        assert!(r.indices.len() / 3 > tris.len(), "the radial fill is subdivided");
        assert!(curved_gradient(&sq, &tris, |_| f32::NAN, &[]).indices.len() / 3 == tris.len());
    }
}
