//! Spreading overlapping parallel segments of different connectors apart.
//!
//! Each pass handles one orientation. Segments that lie on the same line and overlap form a group;
//! interior segments (both ends are bends) move, while first and last segments stay put and the
//! interior ones move off them. The group is spread across a channel that is free of every
//! expanded obstacle over the group's whole extent, so moving a segment, and stretching its two
//! perpendicular neighbours to follow, never enters an obstacle's clearance zone. A segment also
//! moves less than half way toward the far end of either neighbour, so neighbours never collapse
//! or reverse even when the segment beyond them moves too.

use crate::geom::{self, eps_for, near};
use crate::route::{Routed, Slide};
use crate::{Point, Rect};

/// Share of a neighbour's length a segment may eat into; below one half so that two segments
/// moving toward each other from both ends of a neighbour cannot meet.
const MAX_SHRINK: f64 = 0.45;

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    /// Horizontal segments, moved in y.
    Horizontal,
    /// Vertical segments, moved in x.
    Vertical,
}

impl Axis {
    /// The coordinate a segment of this orientation is moved along (constant along it).
    fn across(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.y,
            Axis::Vertical => p.x,
        }
    }

    /// The coordinate that varies along a segment of this orientation.
    fn along(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.x,
            Axis::Vertical => p.y,
        }
    }

    fn set_across(self, p: &mut Point, v: f64) {
        match self {
            Axis::Horizontal => p.y = v,
            Axis::Vertical => p.x = v,
        }
    }

    /// (along min, along max, across min, across max) of a rectangle.
    fn rect(self, r: &Rect) -> (f64, f64, f64, f64) {
        match self {
            Axis::Horizontal => (r.min.x, r.max.x, r.min.y, r.max.y),
            Axis::Vertical => (r.min.y, r.max.y, r.min.x, r.max.x),
        }
    }
}

struct Member {
    route: usize,
    seg: usize,
    across: f64,
    lo: f64,
    hi: f64,
    /// Across coordinates of the far ends of the two neighbouring segments.
    prev_far: f64,
    next_far: f64,
    /// Along coordinates where the two neighbouring segments join this one.
    prev_at: f64,
    next_at: f64,
    margin: f64,
    /// First and last segments carry the ends, so they move only when their ends may slide; fixed
    /// ones still take part in groups so that interior segments move off them.
    fixed: bool,
    /// For a first or last segment that may slide: the interval its across coordinate must stay
    /// in (the slide limit and the glued shape's side).
    slide: Option<(f64, f64)>,
    /// Obstacles this segment may overlap: the route's ignored ones and, for a sliding end
    /// segment, the shape it leaves.
    exempt: Vec<usize>,
    /// Room the segment needs on each side for the route's label, when the label sits on it.
    room: f64,
    /// Along the segment, the stretch the label covers, when the label sits on it.
    label_span: Option<(f64, f64)>,
    /// Obstacles the route passes through by right (a container holding an end): a label stays
    /// off their outlines even so.
    frames: Vec<usize>,
}

/// The segment of `pts` holding the middle of the route by length, where its label is drawn, and
/// that middle point.
fn middle_segment(pts: &[Point]) -> Option<(usize, Point)> {
    let len = |w: &[Point]| match w {
        [a, b] => ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt(),
        _ => 0.0,
    };
    let total: f64 = pts.windows(2).map(len).sum();
    if !(total.is_finite() && total > 0.0) {
        return None;
    }
    let mut run = 0.0;
    for (i, w) in pts.windows(2).enumerate() {
        let l = len(w);
        if run + l >= total / 2.0 {
            let [a, b] = w else { return None };
            let t = if l > 0.0 { (total / 2.0 - run) / l } else { 0.0 };
            return Some((i, Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))));
        }
        run += l;
    }
    None
}

/// Space kept between a label and a shape beside it.
const LABEL_GAP: f64 = 2.0;

/// Nudges every non-fallback route in place, horizontal segments first, then vertical ones.
pub(crate) fn nudge(routes: &mut [Routed], obstacles: &[Option<Rect>], spacing: f64) {
    for axis in [Axis::Horizontal, Axis::Vertical] {
        pass(routes, obstacles, spacing, axis);
    }
    for r in routes.iter_mut().filter(|r| r.margin.is_some()) {
        r.route.points = geom::simplify(&r.route.points);
    }
}

/// The interval a sliding end segment's across coordinate `c` may take: `limit` either way, and
/// within the side of the glued shape it leaves.
fn slide_range(slide: &Slide, c: f64, axis: Axis) -> (f64, f64) {
    let (mut lo, mut hi) = (c - slide.limit, c + slide.limit);
    if let Some(r) = &slide.rect {
        let (_, _, c0, c1) = axis.rect(r);
        lo = lo.max(c0);
        hi = hi.min(c1);
    }
    (lo.min(c), hi.max(c))
}

fn members(routes: &[Routed], axis: Axis) -> Vec<Member> {
    let mut out = Vec::new();
    for (ri, r) in routes.iter().enumerate() {
        let Some(margin) = r.margin else { continue };
        let pts = &r.route.points;
        let last = pts.len().saturating_sub(2);
        let labelled = r.label.and_then(|l| Some((middle_segment(pts)?, l)));
        for seg in 0..pts.len().saturating_sub(1) {
            let (Some(&a), Some(&b)) = (pts.get(seg), pts.get(seg + 1)) else { continue };
            if !near(axis.across(a), axis.across(b)) || near(axis.along(a), axis.along(b)) {
                continue;
            }
            let c = axis.across(a);
            // An end segment slides only when every end it carries may slide.
            let ends: Vec<Option<&Slide>> =
                [(seg == 0).then_some(r.start_slide.as_ref()), (seg == last).then_some(r.end_slide.as_ref())].into_iter().flatten().collect();
            let is_end = !ends.is_empty();
            let slide = if is_end && ends.iter().all(Option::is_some) {
                ends.iter().flatten().map(|s| slide_range(s, c, axis)).reduce(|x, y| (x.0.max(y.0), x.1.min(y.1)))
            } else {
                None
            };
            let fixed = is_end && slide.is_none_or(|(lo, hi)| hi - lo <= eps_for(lo, hi));
            let mut exempt = r.ignore.clone();
            if slide.is_some() {
                exempt.extend(ends.iter().flatten().filter_map(|s| s.index));
            }
            // An end has no neighbour beyond it; its own point stands in.
            let p = seg.checked_sub(1).and_then(|k| pts.get(k)).copied().unwrap_or(a);
            let n = pts.get(seg + 2).copied().unwrap_or(b);
            let (lo, hi) = (axis.along(a).min(axis.along(b)), axis.along(a).max(axis.along(b)));
            out.push(Member {
                route: ri,
                seg,
                across: c,
                lo,
                hi,
                prev_far: axis.across(p),
                next_far: axis.across(n),
                prev_at: axis.along(a),
                next_at: axis.along(b),
                margin,
                fixed,
                slide: if fixed { None } else { slide },
                exempt,
                room: match labelled {
                    // A label centred on a horizontal segment reaches half its height across it;
                    // on a vertical one, half its width.
                    Some(((k, _), (w, h))) if k == seg => match axis {
                        Axis::Horizontal => h / 2.0,
                        Axis::Vertical => w / 2.0,
                    },
                    _ => 0.0,
                },
                frames: r.ignore.clone(),
                label_span: match labelled {
                    Some(((k, at), (w, h))) if k == seg => {
                        let half = match axis {
                            Axis::Horizontal => w / 2.0,
                            Axis::Vertical => h / 2.0,
                        };
                        Some((axis.along(at) - half, axis.along(at) + half))
                    }
                    _ => None,
                },
            });
        }
    }
    out.sort_by(|a, b| a.across.total_cmp(&b.across).then(a.lo.total_cmp(&b.lo)).then(a.route.cmp(&b.route)).then(a.seg.cmp(&b.seg)));
    out
}

/// The interval the member's segment may move within: free of every expanded obstacle over the
/// segment's extent, and short of the far ends of its neighbours.
fn channel(m: &Member, obstacles: &[Option<Rect>], axis: Axis) -> (f64, f64) {
    let c = m.across;
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for (k, r) in obstacles.iter().enumerate() {
        let Some(r) = r else { continue };
        if m.exempt.contains(&k) {
            continue;
        }
        let (a0, a1, c0, c1) = axis.rect(&r.expanded(m.margin));
        // Only obstacles whose interior overlaps the closed extent matter; one that merely touches
        // an end leaves the stretched neighbour on its boundary, which is allowed.
        if !(a0 < m.hi && a1 > m.lo) {
            continue;
        }
        let e = eps_for(c0, c1);
        if c1 <= c + e {
            lo = lo.max(c1.min(c));
        } else if c0 >= c - e {
            hi = hi.min(c0.max(c));
        } else {
            // The segment already runs through this obstacle (only possible through rounding);
            // leave it where it is rather than make things worse.
            return (c, c);
        }
    }
    let first = m.seg == 0;
    for (k, far) in [m.prev_far, m.next_far].into_iter().enumerate() {
        // A sliding end segment has no neighbour on its end's side: the slide range bounds it.
        let at_end = m.slide.is_some() && ((k == 0 && first) || (k == 1 && near(far, c)));
        if at_end {
            continue;
        }
        let room = (far - c).abs() * MAX_SHRINK;
        if far < c {
            lo = lo.max(c - room);
        } else {
            hi = hi.min(c + room);
        }
    }
    if let Some((s0, s1)) = m.slide {
        lo = lo.max(s0);
        hi = hi.min(s1);
    }
    (lo, hi)
}

/// Every route's label box as it stands: (route, box).
fn label_boxes(routes: &[Routed]) -> Vec<(usize, Rect)> {
    routes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.margin.is_some())
        .filter_map(|(i, r)| {
            let (w, h) = r.label?;
            let (_, at) = middle_segment(&r.route.points)?;
            Some((i, Rect::new(at.x - w / 2.0, at.y - h / 2.0, at.x + w / 2.0, at.y + h / 2.0)))
        })
        .collect()
}

fn pass(routes: &mut [Routed], obstacles: &[Option<Rect>], spacing: f64, axis: Axis) {
    let all = members(routes, axis);
    let labels = label_boxes(routes);
    let mut moves: Vec<(usize, usize, f64)> = Vec::new();
    let mut label_moves: Vec<(usize, usize, f64)> = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let Some(first) = all.get(i) else { break };
        let mut reach = first.hi;
        let mut j = i + 1;
        while let Some(m) = all.get(j) {
            if !near(m.across, first.across) || m.lo >= reach - eps_for(m.lo, reach) {
                break;
            }
            reach = reach.max(m.hi);
            j += 1;
        }
        if let Some(group) = all.get(i..j) {
            if group.len() > 1 {
                place(group, obstacles, spacing, axis, routes, &mut moves);
            } else if let Some(m) = group.first()
                && let Some(v) = clear_label(m, obstacles, &labels, axis)
            {
                label_moves.push((m.route, m.seg, v));
            }
        }
        i = j;
    }
    let apply = |routes: &mut [Routed], (route, seg, v): (usize, usize, f64)| {
        if let Some(r) = routes.get_mut(route) {
            for k in [seg, seg + 1] {
                if let Some(p) = r.route.points.get_mut(k) {
                    axis.set_across(p, v);
                }
            }
        }
    };
    for m in moves {
        apply(routes, m);
    }
    // Moving a segment for its label stretches its neighbours; a move that lays one of them on
    // another route's run is undone.
    for (route, seg, v) in label_moves {
        let Some(old) = routes.get(route).map(|r| r.route.points.clone()) else { continue };
        apply(routes, (route, seg, v));
        let clashes = routes.get(route).is_some_and(|r| {
            routes.iter().enumerate().any(|(j, o)| j != route && o.margin.is_some() && runs_meet(&r.route.points, &o.route.points, spacing))
        });
        if clashes && let Some(r) = routes.get_mut(route) {
            r.route.points = old;
        }
    }
}

/// Whether two polylines have axis-aligned segments closer than `spacing` side by side,
/// overlapping along them by more than `spacing`.
fn runs_meet(a: &[Point], b: &[Point], spacing: f64) -> bool {
    let gap = (spacing - 0.5).max(0.5);
    let over = |a0: f64, a1: f64, b0: f64, b1: f64| a0.max(a1).min(b0.max(b1)) - a0.min(a1).max(b0.min(b1)) > spacing;
    a.windows(2).any(|s| {
        b.windows(2).any(|t| {
            let ([p, q], [u, v]) = (s, t) else { return false };
            (near(p.y, q.y) && near(u.y, v.y) && (p.y - u.y).abs() < gap && over(p.x, q.x, u.x, v.x))
                || (near(p.x, q.x) && near(u.x, v.x) && (p.x - u.x).abs() < gap && over(p.y, q.y, u.y, v.y))
        })
    })
}

/// Where a lone segment carrying its route's label must move so the label clears the shapes
/// and the other routes' labels beside it, or `None` when it already does or cannot. The segment stays in the channel its
/// line keeps clear of shapes and short of its neighbours' far ends, as nudging always does.
fn clear_label(m: &Member, obstacles: &[Option<Rect>], labels: &[(usize, Rect)], axis: Axis) -> Option<f64> {
    let (s0, s1) = m.label_span?;
    if m.fixed || m.room <= 0.0 {
        return None;
    }
    let c = m.across;
    let pad = m.room + LABEL_GAP;
    let mut label_zones: Vec<(f64, f64)> = Vec::new();
    let (mut reach_lo, mut reach_hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for (k, r) in obstacles.iter().enumerate() {
        let Some(r) = r else { continue };
        let (a0, a1, c0, c1) = axis.rect(r);
        if m.exempt.contains(&k) {
            // A frame the line may cross: the label keeps off its two sides along the segment.
            if m.frames.contains(&k) && a1 > s0 && a0 < s1 {
                label_zones.extend([(c0 - pad, c0 + pad), (c1 - pad, c1 + pad)]);
            }
            continue;
        }
        if a1 > s0 && a0 < s1 {
            label_zones.push((c0 - pad, c1 + pad));
        }
        // The line itself keeps its margin over its whole extent, and cannot jump past a shape.
        let (e0, e1, d0, d1) = axis.rect(&r.expanded(m.margin));
        if e0 < m.hi && e1 > m.lo {
            if d1 <= c + eps_for(d1, c) {
                reach_lo = reach_lo.max(d1);
            } else if d0 >= c - eps_for(d0, c) {
                reach_hi = reach_hi.min(d0);
            } else {
                return None;
            }
        }
    }
    // Other routes' labels beside this one's stretch are kept off too.
    for (_, r) in labels.iter().filter(|(i, _)| *i != m.route) {
        let (a0, a1, c0, c1) = axis.rect(r);
        if a1 > s0 && a0 < s1 {
            label_zones.push((c0 - pad, c1 + pad));
        }
    }
    let inside = |v: f64| label_zones.iter().any(|(z0, z1)| v > *z0 + eps_for(*z0, v) && v < *z1 - eps_for(v, *z1));
    if !inside(c) {
        return None;
    }
    let (mut lo, mut hi) = (reach_lo, reach_hi);
    for far in [m.prev_far, m.next_far] {
        let room = (far - c).abs() * MAX_SHRINK;
        if far < c {
            lo = lo.max(c - room);
        } else {
            hi = hi.min(c + room);
        }
    }
    if let Some((a, b)) = m.slide {
        lo = lo.max(a);
        hi = hi.min(b);
    }
    label_zones
        .iter()
        .flat_map(|(z0, z1)| [*z0, *z1])
        .filter(|v| v.is_finite() && *v >= lo && *v <= hi && !inside(*v))
        .min_by(|a, b| (a - c).abs().total_cmp(&(b - c).abs()))
}

/// Groups larger than this skip the pairwise score (it costs the square of the group size) and are
/// ordered by the neighbour rule alone.
const MAX_SCORED: usize = 64;

/// Which side of `other` segment `m` must take so the two do not cross, from where their
/// neighbours turn off (`c` is the shared line): -1 for the smaller side, 1 for the larger, 0 when
/// neither says. A neighbour that turns off strictly inside the other's extent crosses it unless
/// its segment lies on the side it turns toward; each such end votes.
fn side_vote(m: &Member, other: &Member, c: f64) -> i64 {
    if std::ptr::eq(m, other) {
        return 0;
    }
    let turn = |far: f64| -> i64 {
        if near(far, c) {
            0
        } else if far < c {
            -1
        } else {
            1
        }
    };
    let inside = |at: f64, s: &Member| at > s.lo + eps_for(s.lo, at) && at < s.hi - eps_for(at, s.hi);
    let mut vote: i64 = 0;
    for (at, far) in [(m.prev_at, m.prev_far), (m.next_at, m.next_far)] {
        if inside(at, other) {
            vote += turn(far);
        }
    }
    for (at, far) in [(other.prev_at, other.prev_far), (other.next_at, other.next_far)] {
        if inside(at, m) {
            vote -= turn(far);
        }
    }
    vote.signum()
}

fn place(group: &[Member], obstacles: &[Option<Rect>], spacing: f64, axis: Axis, routes: &[Routed], moves: &mut Vec<(usize, usize, f64)>) {
    let c = group.first().map_or(0.0, |m| m.across);
    // A label on a segment needs its neighbours further off than the plain spacing: clear of its
    // box, and of a neighbour's label box beside it.
    let mut rooms: Vec<f64> = group.iter().map(|m| m.room).filter(|r| r.is_finite() && *r > 0.0).collect();
    rooms.sort_by(|a, b| b.total_cmp(a));
    let base = spacing;
    let spacing = match rooms.as_slice() {
        [] => spacing,
        [one] => spacing.max(one + spacing / 2.0),
        [a, b, ..] => spacing.max(a + b + spacing / 2.0),
    };
    let has_fixed = group.iter().any(|m| m.fixed);
    // Order members so that a segment whose neighbours lie toward smaller coordinates sits on the
    // smaller side, which avoids many needless crossings; the pairwise score below settles the
    // cases that rule gets wrong, and ties go by route order.
    let order: Vec<&Member> = group.iter().filter(|m| !m.fixed).collect();
    if order.is_empty() || (!has_fixed && order.len() < 2) {
        return;
    }
    let scores: Vec<i64> = order.iter().map(|m| if order.len() <= MAX_SCORED { order.iter().map(|o| side_vote(m, o, c)).sum() } else { 0 }).collect();
    let mut order: Vec<(&Member, i64)> = order.into_iter().zip(scores).collect();
    order.sort_by(|(a, sa), (b, sb)| {
        sa.cmp(sb).then((a.prev_far + a.next_far).total_cmp(&(b.prev_far + b.next_far))).then(a.route.cmp(&b.route)).then(a.seg.cmp(&b.seg))
    });
    let order: Vec<&Member> = order.into_iter().map(|(m, _)| m).collect();
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for m in &order {
        let (l, h) = channel(m, obstacles, axis);
        lo = lo.max(l);
        hi = hi.min(h);
    }
    if lo > c || hi < c {
        return;
    }
    let k = order.len();
    let positions: Vec<f64> = if has_fixed {
        // The fixed segments keep the shared line, so the movable ones take free slots beside it,
        // nearest first, alternating sides; with no room for a label's spacing, the plain one.
        let slots_at = |spacing: f64| -> Vec<f64> {
            (1..=k)
                .flat_map(|i| {
                    let d = i as f64 * spacing;
                    [c + d, c - d]
                })
                .filter(|&v| v >= lo && v <= hi)
                .take(k)
                .collect()
        };
        let mut slots = slots_at(spacing);
        if slots.len() < k {
            slots = slots_at(base);
        }
        if slots.len() < k {
            return;
        }
        slots.sort_by(f64::total_cmp);
        slots
    } else {
        let gaps = (k - 1) as f64;
        let width = gaps * spacing;
        let (start, step) = if hi - lo >= width {
            let centre = c.max(lo + width / 2.0).min(hi - width / 2.0);
            (centre - width / 2.0, spacing)
        } else if hi - lo > eps_for(lo, hi) {
            (lo, (hi - lo) / gaps)
        } else {
            return;
        };
        (0..k).map(|i| start + i as f64 * step).collect()
    };
    for (m, v) in order.iter().zip(positions) {
        if routes.get(m.route).is_some() && v.is_finite() {
            moves.push((m.route, m.seg, v));
        }
    }
}
