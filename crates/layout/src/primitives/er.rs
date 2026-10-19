//! The entity-relationship drawing: entities layered in the flow direction with parents (the
//! referenced side, edge sources) before children, orthogonal edges, gaps wide enough for two
//! crow's-foot ends and an end label between neighbours, and each entity's satellites (Chen
//! attributes) on a ring around it, clear of its edges.
//!
//! Satellites are placed by [`ring_around`]: their centres sit on a rectangle around the owner,
//! spaced evenly clockwise, far enough out that no satellite touches the owner and far enough
//! apart that no two touch, and none across from a side the owner's relationships leave from, so
//! the axis between two related entities stays clear for the relationship and its label. The owner is laid out as a node grown by the ring's
//! thickness, so neighbouring rings never meet either.

use serde::{Deserialize, Serialize};

use super::{Claims, bad, size};
use crate::{Algorithm, Edge, Graph, LayoutError, Node, Options, Point};

/// One entity and the nodes drawn around it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Satellite<T = usize> {
    /// The entity.
    pub owner: T,
    /// The nodes around it, clockwise from the top.
    #[serde(default)]
    pub members: Vec<T>,
}

/// Line width the gaps assume: the ER connector types' width.
const LINE_WIDTH: f64 = 1.5;

/// Room left for an end label beside an end (`N`, `0..*`, `P`), in world units.
const END_LABEL: f64 = 28.0;

/// Room left for the relationship's own label between the two ends.
const MID_LABEL: f64 = 24.0;

/// The least gap between entities, across and along the flow: at each end the largest end
/// marker (`8 + 2.5 * lw` long) and an end label beside it, and the relationship's label between.
pub const MIN_GAP: f64 = 2.0 * (8.0 + 2.5 * LINE_WIDTH + END_LABEL) + MID_LABEL;

/// Space between an owner and its satellites when there is room.
const RING_GAP: f64 = 30.0;

/// Space between two satellites along the ring.
const RING_SPACING: f64 = 10.0;

/// Most satellites around one owner.
pub const MAX_SATELLITES: usize = 1024;

/// Where `members` (sizes) go around an owner of size `owner`: each member's top-left relative to
/// the owner's top-left, and how far the ring reaches beyond the owner's frame on every side.
pub fn ring(owner: (f64, f64), members: &[(f64, f64)]) -> (Vec<Point>, f64) {
    ring_around(owner, members, [false; 4])
}

/// [`ring`] keeping the sides in `blocked` (top, right, bottom, left) clear: the sides the owner's
/// relationships leave from. No member's centre lies across from a blocked side (within the
/// owner's half size plus half the biggest member of its middle), so the axis from the owner to
/// a related entity stays open for the relationship and its label; members go round the other
/// sides and the corners, spaced evenly, and the ring grows when they need more room.
pub fn ring_around(owner: (f64, f64), members: &[(f64, f64)], blocked: [bool; 4]) -> (Vec<Point>, f64) {
    let (w, h) = (finite(owner.0), finite(owner.1));
    if members.is_empty() {
        return (Vec::new(), 0.0);
    }
    let n = members.len().min(MAX_SATELLITES);
    let biggest = members.iter().take(n).map(|&(a, b)| finite(a).max(finite(b))).fold(0.0, f64::max);
    // Half of the stretch kept clear in front of a blocked side, measured along the ring.
    let half_clear = [w / 2.0 + biggest / 2.0, h / 2.0 + biggest / 2.0, w / 2.0 + biggest / 2.0, h / 2.0 + biggest / 2.0];
    let kept: f64 = blocked.iter().zip(half_clear).filter(|(b, _)| **b).map(|(_, e)| 2.0 * e).sum();
    // Centres run around a rectangle `gap + biggest / 2` outside the frame; along it, neighbours
    // need twice the satellite size so they stay apart even where the path turns a corner.
    let need = 2.0 * (biggest + RING_SPACING) * n as f64;
    let path = |gap: f64| 4.0 * (w + h) / 2.0 + 8.0 * (gap + biggest / 2.0) - kept;
    let mut gap = RING_GAP;
    if (n > 1 || kept > 0.0) && path(gap) < need {
        gap += (need - path(gap)) / 8.0;
    }
    let (hx, hy) = (w / 2.0 + gap + biggest / 2.0, h / 2.0 + gap + biggest / 2.0);
    let perimeter = 4.0 * (hx + hy);
    // The open stretches of the path, from the top centre clockwise.
    let centres = [0.0, hx + hy, 2.0 * hx + 2.0 * hy, 3.0 * hx + 3.0 * hy];
    let mut cuts: Vec<(f64, f64)> = Vec::new();
    for ((&b, &c), e) in blocked.iter().zip(&centres).zip(half_clear) {
        if b {
            if c == 0.0 {
                cuts.push((0.0, e));
                cuts.push((perimeter - e, perimeter));
            } else {
                cuts.push((c - e, c + e));
            }
        }
    }
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut open: Vec<(f64, f64)> = Vec::new();
    let mut at = 0.0;
    for (c0, c1) in cuts {
        if c0 > at {
            open.push((at, c0));
        }
        at = at.max(c1);
    }
    if at < perimeter {
        open.push((at, perimeter));
    }
    let total: f64 = open.iter().map(|(a, b)| b - a).sum();
    let step = if open.is_empty() || total.is_nan() || total <= 0.0 { perimeter / n as f64 } else { total / n as f64 };
    let mut out = Vec::with_capacity(n);
    for (i, &(mw, mh)) in members.iter().take(n).enumerate() {
        // Distance along the path from the top centre, clockwise: with nothing blocked the first
        // member sits at the top centre; otherwise each sits in the middle of its share of the
        // open stretches.
        let d = if blocked.iter().any(|b| *b) && total > 0.0 {
            let mut left = (i as f64 + 0.5) * step;
            let mut d = 0.0;
            for (a, b) in &open {
                if left <= b - a {
                    d = a + left;
                    break;
                }
                left -= b - a;
            }
            d
        } else {
            (i as f64 * step).rem_euclid(perimeter)
        };
        let (cx, cy) = along(d, hx, hy);
        out.push(Point::new(w / 2.0 + cx - finite(mw) / 2.0, h / 2.0 + cy - finite(mh) / 2.0));
    }
    (out, gap + biggest)
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, crate::MAX_NODE_SIZE) } else { 0.0 }
}

/// The point `d` along the rectangle of half-sizes `hx`, `hy` centred on the origin, starting at
/// the top centre and running clockwise.
fn along(d: f64, hx: f64, hy: f64) -> (f64, f64) {
    let legs = [(hx, (1.0, 0.0)), (2.0 * hy, (0.0, 1.0)), (2.0 * hx, (-1.0, 0.0)), (2.0 * hy, (0.0, -1.0)), (hx, (1.0, 0.0))];
    let (mut x, mut y) = (0.0, -hy);
    let mut left = d;
    let mut corner = [(hx, -hy), (hx, hy), (-hx, hy), (-hx, -hy), (0.0, -hy)].into_iter();
    for (len, (dx, dy)) in legs {
        if left <= len {
            return (x + dx * left, y + dy * left);
        }
        left -= len;
        if let Some((cx, cy)) = corner.next() {
            (x, y) = (cx, cy);
        }
    }
    (x, y)
}

/// Places every node: owners and free entities layered, satellites on their owner's ring.
pub(super) fn er(graph: &Graph, satellites: &[Satellite], opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "er";
    let n = graph.nodes.len();
    let mut members = Claims::new(NAME, n);
    let mut owners = Claims::new(NAME, n);
    for s in satellites {
        owners.claim(s.owner, "an owner")?;
        if s.members.len() > MAX_SATELLITES {
            return Err(bad(NAME, format!("node {} has more than {MAX_SATELLITES} satellites", s.owner)));
        }
        for &m in &s.members {
            members.claim(m, &format!("a satellite of node {}", s.owner))?;
        }
    }
    for s in satellites {
        if members.taken.get(s.owner).copied().unwrap_or(false) {
            return Err(bad(NAME, format!("node {} is both an owner and a satellite", s.owner)));
        }
    }
    // The ring of every owner, first kept off the sides its relationships will likely leave from
    // (toward the layers before and after it), so the layout leaves room for it.
    let is_member = |v: usize| members.taken.get(v).copied().unwrap_or(false);
    let core_edges: Vec<(usize, usize)> =
        graph.edges.iter().filter(|e| e.source != e.target && !is_member(e.source) && !is_member(e.target)).map(|e| (e.source, e.target)).collect();
    // Sides as (top, right, bottom, left); a source's edges leave toward the layers after it.
    let (before, after) = match opt.direction {
        crate::Direction::TopBottom => (0, 2),
        crate::Direction::BottomTop => (2, 0),
        crate::Direction::LeftRight => (3, 1),
        crate::Direction::RightLeft => (1, 3),
    };
    let mut predicted = vec![[false; 4]; n];
    for &(a, b) in &core_edges {
        if let Some(p) = predicted.get_mut(a).and_then(|p| p.get_mut(after)) {
            *p = true;
        }
        if let Some(p) = predicted.get_mut(b).and_then(|p| p.get_mut(before)) {
            *p = true;
        }
    }
    let sizes_of = |s: &Satellite| -> Vec<(f64, f64)> { s.members.iter().map(|&m| size(graph, m)).collect() };
    let mut rings: Vec<Option<(Vec<Point>, f64)>> = vec![None; n];
    for s in satellites {
        if let Some(slot) = rings.get_mut(s.owner) {
            *slot = Some(ring_around(size(graph, s.owner), &sizes_of(s), predicted.get(s.owner).copied().unwrap_or_default()));
        }
    }
    // The entities: every node that is not a satellite, grown by its ring.
    let core: Vec<usize> = members.unclaimed();
    let mut local_of = vec![usize::MAX; n];
    for (i, &v) in core.iter().enumerate() {
        if let Some(slot) = local_of.get_mut(v) {
            *slot = i;
        }
    }
    let thick = |v: usize| rings.get(v).and_then(|r| r.as_ref()).map_or(0.0, |r| r.1);
    let local = Graph {
        nodes: core
            .iter()
            .map(|&v| {
                let (w, h) = size(graph, v);
                let t = thick(v);
                let mut node = Node::new(w + 2.0 * t, h + 2.0 * t);
                node.order = graph.nodes.get(v).and_then(|x| x.order);
                node
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter_map(|e| {
                let (a, b) = (*local_of.get(e.source)?, *local_of.get(e.target)?);
                (a != usize::MAX && b != usize::MAX && a != b).then_some(Edge { source: a, target: b, ..*e })
            })
            .collect(),
        groups: Vec::new(),
    };
    let options = Options {
        algorithm: Algorithm::Layered,
        orthogonal_edges: true,
        node_spacing: opt.node_spacing.max(MIN_GAP),
        layer_spacing: opt.layer_spacing.max(MIN_GAP),
        ..*opt
    };
    let out = crate::layout(&local, &options)?;
    let mut pos = vec![Point::default(); n];
    for (&v, p) in core.iter().zip(&out.nodes) {
        let t = thick(v);
        if let Some(slot) = pos.get_mut(v) {
            *slot = Point::new(p.x + t, p.y + t);
        }
    }
    // Now that the entities are placed, keep clear the sides their relationships really use: the
    // side facing each related entity.
    let centre = |v: usize| {
        let (w, h) = size(graph, v);
        pos.get(v).map(|p| (p.x + w / 2.0, p.y + h / 2.0))
    };
    let mut used = vec![[false; 4]; n];
    for &(a, b) in &core_edges {
        let (Some(ca), Some(cb)) = (centre(a), centre(b)) else { continue };
        let (dx, dy) = (cb.0 - ca.0, cb.1 - ca.1);
        let side = if dx.abs() >= dy.abs() {
            if dx >= 0.0 { 1 } else { 3 }
        } else if dy >= 0.0 {
            2
        } else {
            0
        };
        if let Some(u) = used.get_mut(a).and_then(|u| u.get_mut(side)) {
            *u = true;
        }
        if let Some(u) = used.get_mut(b).and_then(|u| u.get_mut((side + 2) % 4)) {
            *u = true;
        }
    }
    for s in satellites {
        if let (Some(slot), Some(u)) = (rings.get_mut(s.owner), used.get(s.owner))
            && predicted.get(s.owner) != Some(u)
        {
            *slot = Some(ring_around(size(graph, s.owner), &sizes_of(s), *u));
        }
    }
    for s in satellites {
        let Some((offsets, _)) = rings.get(s.owner).and_then(|r| r.as_ref()) else { continue };
        let at = pos.get(s.owner).copied().unwrap_or_default();
        for (&m, o) in s.members.iter().zip(offsets) {
            if let Some(slot) = pos.get_mut(m) {
                *slot = Point::new(at.x + o.x, at.y + o.y);
            }
        }
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Primitive;
    use crate::{Direction, Rect, layout_primitive};

    /// Three entities (0 parent of 1, 1 parent of 2, two edges 0 to 1) and five attributes.
    fn schema() -> Graph {
        let mut nodes = vec![Node::new(200.0, 120.0), Node::new(180.0, 90.0), Node::new(160.0, 100.0)];
        nodes.extend((0..5).map(|_| Node::new(100.0, 40.0)));
        let edges = vec![Edge::new(0, 1), Edge::new(0, 1), Edge::new(1, 2), Edge::new(0, 3), Edge::new(0, 4), Edge::new(1, 5)];
        Graph { nodes, edges, groups: Vec::new() }
    }

    fn rects(g: &Graph, out: &crate::Layout) -> Vec<Rect> {
        g.nodes.iter().zip(&out.nodes).map(|(n, p)| Rect::new(p.x, p.y, n.width, n.height)).collect()
    }

    fn gap(a: &Rect, b: &Rect) -> f64 {
        let dx = (b.x - a.right()).max(a.x - b.right());
        let dy = (b.y - a.bottom()).max(a.y - b.bottom());
        dx.max(dy)
    }

    #[test]
    fn entities_layer_parents_first_with_wide_gaps_and_satellites_on_rings() {
        let g = schema();
        let p = Primitive::Er { satellites: vec![Satellite { owner: 0, members: vec![3, 4] }, Satellite { owner: 1, members: vec![5, 6, 7] }] };
        for direction in [Direction::TopBottom, Direction::LeftRight] {
            let out = layout_primitive(&g, &p, &Options { direction, ..Options::default() }).unwrap();
            let r = rects(&g, &out);
            for (i, a) in r.iter().enumerate() {
                for (j, b) in r.iter().enumerate().skip(i + 1) {
                    assert!(!a.overlaps(b, 1e-6), "{direction:?}: {i} and {j} overlap");
                }
            }
            for (i, j) in [(0, 1), (1, 2), (0, 2)] {
                assert!(gap(&r[i], &r[j]) >= MIN_GAP - 1e-6, "{direction:?}: entities {i} and {j} are too close");
            }
            let along = |v: usize| if direction.is_horizontal() { r[v].x } else { r[v].y };
            assert!(along(0) < along(1) && along(1) < along(2), "{direction:?}: parents come first");
            for (owner, ms) in [(0, &[3, 4][..]), (1, &[5, 6, 7][..])] {
                for &m in ms {
                    let d = gap(&r[owner], &r[m]);
                    assert!((RING_GAP - 1e-6..200.0).contains(&d), "{direction:?}: satellite {m} is {d} from its owner");
                }
            }
            // Orthogonal edges between entities.
            for e in out.edges.iter().take(3) {
                assert!(e.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6), "{e:?}");
            }
        }
    }

    #[test]
    fn chen_attributes_stay_off_the_axis_between_related_entities() {
        // Two related entities with two attributes each, in both flows: no attribute lies on the
        // straight line between the entities' facing sides, where the relationship runs.
        let mut nodes = vec![Node::new(140.0, 56.0), Node::new(140.0, 56.0)];
        nodes.extend((0..4).map(|_| Node::new(110.0, 44.0)));
        let edges = vec![Edge::new(0, 1), Edge::new(0, 2), Edge::new(0, 3), Edge::new(1, 4), Edge::new(1, 5)];
        let g = Graph { nodes, edges, groups: Vec::new() };
        let p = Primitive::Er { satellites: vec![Satellite { owner: 0, members: vec![2, 3] }, Satellite { owner: 1, members: vec![4, 5] }] };
        for direction in [Direction::TopBottom, Direction::LeftRight] {
            let out = layout_primitive(&g, &p, &Options { direction, ..Options::default() }).unwrap();
            let r = rects(&g, &out);
            let lane = if direction.is_horizontal() {
                let y = r[0].y + r[0].height / 2.0;
                Rect::new(r[0].right(), y - 20.0, r[1].x - r[0].right(), 40.0)
            } else {
                let x = r[0].x + r[0].width / 2.0;
                Rect::new(x - 20.0, r[0].bottom(), 40.0, r[1].y - r[0].bottom())
            };
            assert!(lane.width > 0.0 && lane.height > 0.0, "{direction:?}: the entities are apart along the flow");
            for m in 2..6 {
                assert!(!r[m].overlaps(&lane, 1e-6), "{direction:?}: attribute {m} {:?} sits on the axis {lane:?}", r[m]);
                for o in 0..6 {
                    assert!(o == m || !r[m].overlaps(&r[o], 1e-6), "{direction:?}: {m} and {o} overlap");
                }
            }
        }
        // Blocking every side still places every member clear of the owner and of each other.
        let (pts, _) = ring_around((100.0, 60.0), &[(80.0, 30.0); 6], [true; 4]);
        let rs: Vec<Rect> = pts.iter().map(|p| Rect::new(p.x, p.y, 80.0, 30.0)).collect();
        for (i, a) in rs.iter().enumerate() {
            assert!(!a.overlaps(&Rect::new(0.0, 0.0, 100.0, 60.0), 1e-6));
            for b in rs.iter().skip(i + 1) {
                assert!(!a.overlaps(b, 1e-6), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_crowded_ring_grows_until_no_two_satellites_touch() {
        let sizes = vec![(120.0, 40.0); 24];
        let (pts, thick) = ring((100.0, 60.0), &sizes);
        let rs: Vec<Rect> = pts.iter().map(|p| Rect::new(p.x, p.y, 120.0, 40.0)).collect();
        let owner = Rect::new(0.0, 0.0, 100.0, 60.0);
        for (i, a) in rs.iter().enumerate() {
            assert!(!a.overlaps(&owner, 1e-6), "{i} touches the owner");
            assert!(a.x >= -thick - 1e-6 && a.right() <= 100.0 + thick + 1e-6, "{i} past the ring");
            for (j, b) in rs.iter().enumerate().skip(i + 1) {
                assert!(!a.overlaps(b, 1e-6), "{i} and {j} touch");
            }
        }
        assert_eq!(ring((1.0, 1.0), &[]), (Vec::new(), 0.0));
        let (one, _) = ring((f64::NAN, f64::INFINITY), &[(f64::NAN, 5.0)]);
        assert!(one.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn bad_roles_are_refused() {
        let g = schema();
        let cases = [
            vec![Satellite { owner: 9, members: vec![3] }],
            vec![Satellite { owner: 0, members: vec![30] }],
            vec![Satellite { owner: 0, members: vec![3, 3] }],
            vec![Satellite { owner: 0, members: vec![0] }],
            vec![Satellite { owner: 0, members: vec![3] }, Satellite { owner: 0, members: vec![4] }],
            vec![Satellite { owner: 0, members: vec![3] }, Satellite { owner: 3, members: vec![4] }],
        ];
        for satellites in cases {
            let p = Primitive::Er { satellites: satellites.clone() };
            assert!(matches!(layout_primitive(&g, &p, &Options::default()), Err(LayoutError::InvalidRoles { .. })), "{satellites:?}");
        }
        let none = Primitive::Er { satellites: Vec::new() };
        assert!(layout_primitive(&g, &none, &Options::default()).is_ok(), "no satellites: every node is an entity");
        let json: Primitive<String> = serde_json::from_str(r#"{"primitive": "er", "roles": {}}"#).unwrap();
        assert_eq!(json, Primitive::Er { satellites: Vec::new() });
        assert!(serde_json::from_str::<Primitive<String>>(r#"{"primitive": "er", "roles": {"satellites": 5}}"#).is_err());
    }
}
