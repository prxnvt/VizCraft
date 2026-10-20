//! Shape operations: boolean operations on outlines, offset, join and trim. Every result is a
//! freeform shape ([`ShapeKind::Freeform`]) whose path is in its own frame.
//!
//! Outlines are flattened to polygons in world coordinates (rotation included) and handed to
//! `i_overlay` (MIT, pure Rust) for union, combine (exclusive or), fragment, intersect, subtract
//! and offset. Results come back as regions: an outer contour and its holes, wound in opposite
//! directions, so a hole is empty under the even-odd and the non-zero fill rule alike. Join and
//! trim work on the flattened lines themselves and give open paths.
//!
//! Hostile input stays cheap: at most [`MAX_INPUTS`] shapes and [`MAX_POINTS`] points go in, at
//! most [`MAX_PIECES`] pieces come out, and each result keeps at most
//! [`Freeform::MAX_ELS`] path commands.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

use crate::geom::{Path, PathEl, Point, Rect, distance_to_segment, point_in_polygon, ray_hit};
use crate::shape_fields::Freeform;
use crate::{MAX_EXTENT, Shape, ShapeKind};

/// Chord tolerance when flattening curved outlines, in world units.
pub const FLATTEN_TOLERANCE: f64 = 0.25;
/// Most shapes one operation takes.
pub const MAX_INPUTS: usize = 64;
/// Most outline points one operation takes, over all its inputs.
pub const MAX_POINTS: usize = 20_000;
/// Most pieces one operation gives.
pub const MAX_PIECES: usize = 256;
/// Largest offset distance, either way.
pub const MAX_OFFSET: f64 = 10_000.0;
/// How near two line ends must be for [`join`] to connect them, in world units.
pub const JOIN_TOLERANCE: f64 = 2.0;

/// One closed contour: its points, not repeating the first.
pub type Contour = Vec<[f64; 2]>;
/// One region: an outer contour and its holes.
pub type Region = Vec<Contour>;

/// The boolean operations that give one shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    /// Everything any input covers.
    Union,
    /// Exclusive or: what an odd number of inputs cover (overlaps become holes).
    Combine,
    /// What every input covers.
    Intersect,
    /// The first input minus the others.
    Subtract,
}

/// The inputs an operation may take: the first [`MAX_INPUTS`] shapes, stopping before the one that
/// would pass [`MAX_POINTS`] outline points.
fn capped<'a>(shapes: &[&'a Shape]) -> Vec<&'a Shape> {
    let mut total = 0usize;
    let mut out = Vec::new();
    for s in shapes.iter().take(MAX_INPUTS) {
        let n: usize = s.outline_world().flatten(FLATTEN_TOLERANCE).iter().map(|p| p.points.len()).sum();
        total = total.saturating_add(n);
        if total > MAX_POINTS {
            break;
        }
        out.push(*s);
    }
    out
}

/// The closed contours of a shape's outline in world coordinates. Open subpaths enclose nothing
/// and are left out.
pub fn contours(shape: &Shape) -> Vec<Contour> {
    shape
        .outline_world()
        .flatten(FLATTEN_TOLERANCE)
        .into_iter()
        .filter(|pl| pl.closed && pl.points.len() >= 3)
        .map(|pl| pl.points.iter().map(|p| [p.x, p.y]).collect())
        .collect()
}

/// A shape's area as regions with a consistent winding (outer one way, holes the other), so the
/// non-zero rule can combine it with others. The shape's own contours fill even-odd.
fn regions(shape: &Shape) -> Vec<Region> {
    let c = contours(shape);
    if c.is_empty() { Vec::new() } else { c.simplify_shape(FillRule::EvenOdd) }
}

fn flat(regions: Vec<Region>) -> Vec<Contour> {
    regions.into_iter().flatten().collect()
}

fn union_all(list: &[&Shape]) -> Vec<Region> {
    let all: Vec<Contour> = list.iter().flat_map(|s| flat(regions(s))).collect();
    if all.is_empty() { Vec::new() } else { all.simplify_shape(FillRule::NonZero) }
}

/// The result of `op` on `shapes` (in order; subtract keeps the first), as regions. Empty when
/// the result covers nothing.
pub fn boolean(op: BoolOp, shapes: &[&Shape]) -> Vec<Region> {
    let list = capped(shapes);
    let Some((first, rest)) = list.split_first() else { return Vec::new() };
    match op {
        BoolOp::Union => union_all(&list),
        BoolOp::Combine => {
            let all: Vec<Contour> = list.iter().flat_map(|s| flat(regions(s))).collect();
            if all.is_empty() { Vec::new() } else { all.simplify_shape(FillRule::EvenOdd) }
        }
        BoolOp::Intersect => {
            let mut acc = regions(first);
            for s in rest {
                if acc.is_empty() {
                    break;
                }
                acc = flat(acc).overlay(&flat(regions(s)), OverlayRule::Intersect, FillRule::NonZero);
            }
            acc
        }
        BoolOp::Subtract => {
            let acc = regions(first);
            let cut = flat(union_all(rest));
            if acc.is_empty() || cut.is_empty() {
                return acc;
            }
            flat(acc).overlay(&cut, OverlayRule::Difference, FillRule::NonZero)
        }
    }
}

/// Every piece the outlines of `shapes` cut each other into: each separate region covered by
/// exactly the same inputs, in input order. Two overlapping rectangles give three.
pub fn fragment(shapes: &[&Shape]) -> Vec<Region> {
    let list = capped(shapes);
    // Each entry is an area covered by one set of inputs; a new input splits every entry into
    // the part inside it and the part outside, and adds what it covers that no entry does.
    let mut areas: Vec<Vec<Contour>> = Vec::new();
    let mut covered: Vec<Contour> = Vec::new();
    for s in &list {
        let b = flat(regions(s));
        if b.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        for a in &areas {
            let inside = flat(a.overlay(&b, OverlayRule::Intersect, FillRule::NonZero));
            let outside = flat(a.overlay(&b, OverlayRule::Difference, FillRule::NonZero));
            next.extend([inside, outside].into_iter().filter(|x| !x.is_empty()));
        }
        let fresh = if covered.is_empty() { b.clone() } else { flat(b.overlay(&covered, OverlayRule::Difference, FillRule::NonZero)) };
        if !fresh.is_empty() {
            next.push(fresh);
        }
        covered = if covered.is_empty() { b } else { flat(covered.overlay(&b, OverlayRule::Union, FillRule::NonZero)) };
        areas = next;
        if areas.len() > MAX_PIECES {
            areas.truncate(MAX_PIECES);
        }
    }
    // An area may be several separate regions (a ring cut in two): each is its own piece.
    let mut out: Vec<Region> = areas.into_iter().flat_map(|a| a.simplify_shape(FillRule::NonZero)).filter(|r| !r.is_empty()).collect();
    out.truncate(MAX_PIECES);
    out
}

/// The outline of `shape` moved out by `distance` (in by a negative one), with mitred corners.
/// Empty when an inward offset swallows the shape.
pub fn offset(shape: &Shape, distance: f64) -> Vec<Region> {
    let d = if distance.is_finite() { distance.clamp(-MAX_OFFSET, MAX_OFFSET) } else { 0.0 };
    let r = regions(shape);
    if r.is_empty() {
        return r;
    }
    if d.abs() < 1e-9 {
        return r;
    }
    // A mitre down to a 10 degree interior angle: rectangles stay rectangles.
    let style = OutlineStyle::new(d).line_join(LineJoin::Miter(10f64.to_radians()));
    r.outline(&style)
}

/// Regions as one path in world coordinates: each contour a closed subpath.
pub fn regions_path(regions: &[Region]) -> Path {
    let mut path = Path::new();
    for c in regions.iter().flatten() {
        let mut pts = c.iter().map(|p| Point::new(p[0], p[1]));
        let Some(first) = pts.next() else { continue };
        path.move_to(first);
        for p in pts {
            path.line_to(p);
        }
        path.close();
    }
    path
}

/// A shape's outline as polylines in world coordinates, with whether each is closed.
pub fn lines(shape: &Shape) -> Vec<(Vec<Point>, bool)> {
    shape.outline_world().flatten(FLATTEN_TOLERANCE).into_iter().filter(|pl| pl.points.len() >= 2).map(|pl| (pl.points, pl.closed)).collect()
}

/// The polylines of `shapes`, open ones as they are and closed ones repeating their first point,
/// within the input caps.
fn all_lines(shapes: &[&Shape]) -> Vec<Vec<Point>> {
    capped(shapes)
        .iter()
        .flat_map(|s| lines(s))
        .map(|(mut pts, closed)| {
            if closed && let Some(f) = pts.first().copied() {
                pts.push(f);
            }
            pts
        })
        .collect()
}

/// Join the lines of `shapes` end to end: each chain grows from its ends by the nearest line
/// whose start or end lies within `tolerance` (turned round when needed). Returns the chains as
/// one path in world coordinates, and whether every chain closed on itself. `None` with no lines.
pub fn join(shapes: &[&Shape], tolerance: f64) -> Option<(Path, bool)> {
    let tol = if tolerance.is_finite() { tolerance.clamp(0.0, 1000.0) } else { JOIN_TOLERANCE };
    let mut pool: Vec<Vec<Point>> = all_lines(shapes);
    if pool.is_empty() {
        return None;
    }
    let mut chains: Vec<Vec<Point>> = Vec::new();
    while !pool.is_empty() {
        let mut chain = pool.remove(0);
        while let (Some(&head), Some(&tail)) = (chain.first(), chain.last()) {
            // The nearest free line end to either end of the chain.
            let mut best: Option<(f64, usize, bool, bool)> = None;
            for (i, l) in pool.iter().enumerate() {
                let (Some(&a), Some(&b)) = (l.first(), l.last()) else { continue };
                for (d, at_tail, flip) in [
                    (tail.distance(a), true, false),
                    (tail.distance(b), true, true),
                    (head.distance(b), false, false),
                    (head.distance(a), false, true),
                ] {
                    if d <= tol && best.is_none_or(|x| d < x.0) {
                        best = Some((d, i, at_tail, flip));
                    }
                }
            }
            let Some((_, i, at_tail, flip)) = best else { break };
            let mut l = pool.remove(i);
            if flip {
                l.reverse();
            }
            if at_tail {
                chain.extend(l.into_iter().skip(1));
            } else {
                l.pop();
                l.extend(chain);
                chain = l;
            }
        }
        chains.push(chain);
    }
    let closes = |c: &Vec<Point>| c.len() >= 4 && c.first().zip(c.last()).is_some_and(|(a, b)| a.distance(*b) <= tol.max(1e-9));
    let closed = chains.iter().all(closes);
    let mut path = Path::new();
    for c in &chains {
        let mut pts = c.clone();
        if closed {
            pts.pop();
        }
        let mut it = pts.into_iter();
        let Some(first) = it.next() else { continue };
        path.move_to(first);
        for p in it {
            path.line_to(p);
        }
        if closed {
            path.close();
        }
    }
    Some((path, closed))
}

/// Where segments `a`-`b` and `c`-`d` cross: the parameter along the first, strictly inside it
/// (ends excluded so lines meeting end to end are not cut).
fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<f64> {
    let r = b - a;
    let s = d - c;
    let den = r.cross(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let q = c - a;
    let t = q.cross(s) / den;
    let u = q.cross(r) / den;
    const E: f64 = 1e-9;
    (t > E && t < 1.0 - E && (-E..=1.0 + E).contains(&u)).then_some(t)
}

/// Most segments [`trim`] compares, over all its inputs; past it the rest are left whole.
const MAX_TRIM_SEGMENTS: usize = 4000;

/// Cut the lines of `shapes` wherever they cross each other (or themselves), giving each piece
/// as an open polyline in world coordinates, in input order. A closed outline is cut open.
pub fn trim(shapes: &[&Shape]) -> Vec<Vec<Point>> {
    let lines = all_lines(shapes);
    let mut segs: Vec<(usize, usize, Point, Point)> = Vec::new();
    for (li, l) in lines.iter().enumerate() {
        for (si, w) in l.windows(2).enumerate() {
            if let (Some(&a), Some(&b)) = (w.first(), w.get(1)) {
                segs.push((li, si, a, b));
            }
        }
    }
    segs.truncate(MAX_TRIM_SEGMENTS);
    // The cut parameters on each segment, from every other segment that crosses it.
    let mut cuts: Vec<Vec<f64>> = vec![Vec::new(); segs.len()];
    for (i, &(li, si, a, b)) in segs.iter().enumerate() {
        for &(lj, sj, c, d) in &segs {
            // A segment does not cut itself or its neighbours on the same line.
            if li == lj && si.abs_diff(sj) <= 1 {
                continue;
            }
            if let (Some(t), Some(list)) = (crossing(a, b, c, d), cuts.get_mut(i)) {
                list.push(t);
            }
        }
    }
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let mut line_at = usize::MAX;
    for (i, &(li, _, a, b)) in segs.iter().enumerate() {
        if li != line_at {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur = vec![a];
            line_at = li;
        }
        let mut ts = cuts.get(i).cloned().unwrap_or_default();
        ts.sort_by(f64::total_cmp);
        for t in ts {
            let p = a.lerp(b, t);
            cur.push(p);
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur = vec![p];
            if out.len() >= MAX_PIECES {
                return out;
            }
        }
        cur.push(b);
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out.truncate(MAX_PIECES);
    out
}

/// An open polyline as a path.
pub fn polyline_path(points: &[Point]) -> Path {
    Path::polyline(points)
}

/// A freeform shape drawing `world` (a path in world coordinates): its frame fitted to the path's
/// bounds, the path stored in that frame, unrotated, and the style, text, data, layers and parent
/// of `template`. An open path gets no fill. `None` for an empty path.
pub fn freeform(template: &Shape, id: String, world: &Path, closed: bool) -> Option<Shape> {
    let b = world.bounds()?;
    if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let local = world.transformed(&crate::Transform::translate(-b.x0, -b.y0));
    let mut f = Freeform::new(&local, closed);
    f.els.truncate(Freeform::MAX_ELS);
    if f.els.is_empty() {
        return None;
    }
    let mut s = Shape {
        id,
        kind: ShapeKind::Freeform,
        x: b.x0,
        y: b.y0,
        w: b.width().clamp(0.0, MAX_EXTENT),
        h: b.height().clamp(0.0, MAX_EXTENT),
        style: template.style.clone(),
        text: template.text.clone(),
        data: template.data.clone(),
        layers: template.layers.clone(),
        parent: template.parent.clone(),
        path: Some(f),
        ..Shape::default()
    };
    if !closed {
        s.style.fill.a = 0;
        s.style.gradient = None;
    }
    s.connection_points = outline_points(&s);
    s.sanitize();
    Some(s)
}

/// Connection points where the outline meets the rays from the frame centre to the top, right,
/// bottom and left, normalised to the frame (the frame's side midpoints when a ray misses).
pub fn outline_points(s: &Shape) -> Vec<Point> {
    let lines = s.local_outline().flatten(FLATTEN_TOLERANCE);
    let (w, h) = (s.w.max(1e-9), s.h.max(1e-9));
    let c = Point::new(s.w / 2.0, s.h / 2.0);
    [Point::new(0.0, -1.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0), Point::new(-1.0, 0.0)]
        .into_iter()
        .map(|dir| {
            let fallback = Point::new(c.x + dir.x * s.w / 2.0, c.y + dir.y * s.h / 2.0);
            let hit = ray_hit(c, dir, &lines).unwrap_or(fallback);
            Point::new((hit.x / w).clamp(0.0, 1.0), (hit.y / h).clamp(0.0, 1.0))
        })
        .collect()
}

/// The area a path encloses under the even-odd rule, from its flattened closed subpaths: the
/// absolute sum of their signed areas, which is exact when holes wind against their outer
/// contour (as every result here does).
pub fn area(path: &Path) -> f64 {
    let mut total = 0.0;
    for pl in path.flatten(FLATTEN_TOLERANCE).into_iter().filter(|pl| pl.closed) {
        let pts = &pl.points;
        let n = pts.len();
        let mut a = 0.0;
        for i in 0..n {
            if let (Some(p), Some(q)) = (pts.get(i), pts.get((i + 1) % n)) {
                a += p.x * q.y - q.x * p.y;
            }
        }
        total += a / 2.0;
    }
    total.abs()
}

/// Whether `p` is inside `path` under the even-odd rule (inside an odd number of its closed
/// subpaths).
pub fn contains_even_odd(path: &Path, p: Point) -> bool {
    path.flatten(FLATTEN_TOLERANCE).iter().filter(|pl| pl.closed && point_in_polygon(p, &pl.points)).count() % 2 == 1
}

/// Whether `p` lies within `tol` of an open path's lines.
pub fn near_line(path: &Path, p: Point, tol: f64) -> bool {
    path.flatten(FLATTEN_TOLERANCE).iter().any(|pl| pl.segments().any(|(a, b)| distance_to_segment(p, a, b) <= tol))
}

/// The bounds of regions, `None` when empty.
pub fn regions_bounds(regions: &[Region]) -> Option<Rect> {
    Rect::bounding(regions.iter().flatten().flatten().map(|p| Point::new(p[0], p[1])))
}

/// The draw.io (JGraph) wrapper attribute that carries a freeform outline as JSON.
pub const PATH_ATTR: &str = "vizcraftPath";

/// A freeform shape's outline as JSON for [`PATH_ATTR`]; `None` for any other shape.
pub fn path_json(shape: &Shape) -> Option<String> {
    let f = shape.path.as_ref().filter(|_| shape.kind == ShapeKind::Freeform)?;
    serde_json::to_string(f).ok()
}

/// Make `shape` the freeform that [`path_json`] wrote; junk JSON leaves it as it is.
pub fn set_path_json(shape: &mut Shape, json: &str) {
    if let Ok(mut f) = serde_json::from_str::<Freeform>(json) {
        f.sanitize();
        if !f.els.is_empty() {
            shape.kind = ShapeKind::Freeform;
            shape.stencil.clear();
            shape.path = Some(f);
        }
    }
}

/// Whether a path has any `Close` command (a closed outline).
pub fn is_closed(path: &Path) -> bool {
    path.els.iter().any(|e| matches!(e, PathEl::Close))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape::new(id, ShapeKind::Rectangle, x, y, w, h)
    }

    fn line(id: &str, a: Point, b: Point) -> Shape {
        let world = Path::polyline(&[a, b]);
        freeform(&Shape::default(), id.into(), &world, false).unwrap()
    }

    #[test]
    fn union_of_two_overlapping_rectangles_is_their_area() {
        let (a, b) = (rect("a", 0.0, 0.0, 100.0, 100.0), rect("b", 50.0, 50.0, 100.0, 100.0));
        let r = boolean(BoolOp::Union, &[&a, &b]);
        assert_eq!(r.len(), 1);
        let got = area(&regions_path(&r));
        assert!((got - 17_500.0).abs() < 175.0, "{got}");
    }

    #[test]
    fn subtract_leaves_a_hole() {
        let (a, b) = (rect("a", 0.0, 0.0, 100.0, 100.0), rect("b", 25.0, 25.0, 50.0, 50.0));
        let path = regions_path(&boolean(BoolOp::Subtract, &[&a, &b]));
        assert!(contains_even_odd(&path, Point::new(10.0, 10.0)));
        assert!(!contains_even_odd(&path, Point::new(50.0, 50.0)));
        assert!((area(&path) - 7_500.0).abs() < 1.0);
    }

    #[test]
    fn combine_intersect_and_fragment() {
        let (a, b) = (rect("a", 0.0, 0.0, 100.0, 100.0), rect("b", 50.0, 50.0, 100.0, 100.0));
        let x = regions_path(&boolean(BoolOp::Combine, &[&a, &b]));
        assert!(!contains_even_odd(&x, Point::new(75.0, 75.0)) && contains_even_odd(&x, Point::new(10.0, 10.0)));
        let i = regions_path(&boolean(BoolOp::Intersect, &[&a, &b]));
        assert!((area(&i) - 2_500.0).abs() < 1.0);
        let pieces = fragment(&[&a, &b]);
        assert_eq!(pieces.len(), 3);
        let total: f64 = pieces.iter().map(|p| area(&regions_path(std::slice::from_ref(p)))).sum();
        assert!((total - 17_500.0).abs() < 1.0, "{total}");
    }

    #[test]
    fn offset_grows_the_bounds_by_the_distance() {
        let a = rect("a", 0.0, 0.0, 100.0, 60.0);
        let b = regions_bounds(&offset(&a, 10.0)).unwrap();
        assert!((b.x0 + 10.0).abs() < 0.5 && (b.y0 + 10.0).abs() < 0.5 && (b.x1 - 110.0).abs() < 0.5 && (b.y1 - 70.0).abs() < 0.5, "{b:?}");
        let inner = regions_bounds(&offset(&a, -10.0)).unwrap();
        assert!((inner.x0 - 10.0).abs() < 0.5 && (inner.y1 - 50.0).abs() < 0.5, "{inner:?}");
        assert!(offset(&a, -40.0).is_empty());
    }

    #[test]
    fn join_and_trim_open_lines() {
        let l1 = line("l1", Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let l2 = line("l2", Point::new(101.0, 0.0), Point::new(100.0, 100.0));
        let (path, closed) = join(&[&l1, &l2], JOIN_TOLERANCE).unwrap();
        assert!(!closed);
        assert_eq!(path.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 1);
        let cross = line("x", Point::new(50.0, -50.0), Point::new(50.0, 50.0));
        let pieces = trim(&[&l1, &cross]);
        assert_eq!(pieces.len(), 4, "{pieces:?}");
    }

    #[test]
    fn rotation_is_part_of_the_outline() {
        let mut a = rect("a", 0.0, 0.0, 100.0, 20.0);
        a.angle_deg = 90.0;
        let b = regions_bounds(&boolean(BoolOp::Union, &[&a])).unwrap();
        assert!((b.width() - 20.0).abs() < 0.5 && (b.height() - 100.0).abs() < 0.5, "{b:?}");
    }

    #[test]
    fn freeform_fits_its_frame_and_follows_a_resize() {
        let world = Path::polygon(&[Point::new(10.0, 20.0), Point::new(110.0, 20.0), Point::new(60.0, 80.0)]);
        let mut s = freeform(&rect("t", 0.0, 0.0, 1.0, 1.0), "f".into(), &world, true).unwrap();
        assert_eq!((s.x, s.y, s.w, s.h), (10.0, 20.0, 100.0, 60.0));
        assert_eq!(s.outline_world().bounds(), world.bounds());
        s.w = 200.0;
        assert!((s.bounds().width() - 200.0).abs() < 1e-9);
        let open = freeform(&rect("t", 0.0, 0.0, 1.0, 1.0), "o".into(), &Path::polyline(&[Point::ZERO, Point::new(5.0, 5.0)]), false).unwrap();
        assert_eq!(open.style.fill.a, 0);
        assert!(!is_closed(&open.local_outline()));
    }

    #[test]
    fn hostile_inputs_stay_bounded() {
        let mut junk = rect("j", f64::NAN, 1e300, f64::INFINITY, -5.0);
        junk.sanitize();
        let empty: Vec<&Shape> = Vec::new();
        assert!(boolean(BoolOp::Union, &empty).is_empty());
        let _ = boolean(BoolOp::Subtract, &[&junk, &junk]);
        let _ = fragment(&[&junk, &junk, &junk]);
        let _ = offset(&junk, f64::NAN);
        let _ = offset(&junk, -1e300);
        assert!(join(&empty, 1.0).is_none());
        let many: Vec<Shape> = (0..100).map(|i| rect(&format!("r{i}"), f64::from(i), 0.0, 50.0, 50.0)).collect();
        let refs: Vec<&Shape> = many.iter().collect();
        assert!(fragment(&refs).len() <= MAX_PIECES);
        assert!(trim(&refs).len() <= MAX_PIECES);
    }
}
