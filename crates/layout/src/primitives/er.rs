//! The entity-relationship drawing: entities layered in the flow direction with parents (the
//! referenced side, edge sources) before children, orthogonal edges, gaps wide enough for two
//! crow's-foot ends and an end label between neighbours, and each entity's satellites (Chen
//! attributes) on a ring around it, clear of its edges.
//!
//! Satellites are placed by [`ring`]: their centres sit on a rectangle around the owner, spaced
//! evenly from the top centre clockwise, far enough out that no satellite touches the owner and
//! far enough apart that no two touch. The owner is laid out as a node grown by the ring's
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
    let (w, h) = (finite(owner.0), finite(owner.1));
    if members.is_empty() {
        return (Vec::new(), 0.0);
    }
    let n = members.len().min(MAX_SATELLITES);
    let biggest = members.iter().take(n).map(|&(a, b)| finite(a).max(finite(b))).fold(0.0, f64::max);
    // Centres run around a rectangle `gap + biggest / 2` outside the frame; along it, neighbours
    // need twice the satellite size so they stay apart even where the path turns a corner.
    let need = 2.0 * (biggest + RING_SPACING) * n as f64;
    let path = |gap: f64| 4.0 * (w + h) / 2.0 + 8.0 * (gap + biggest / 2.0);
    let mut gap = RING_GAP;
    if n > 1 && path(gap) < need {
        gap += (need - path(gap)) / 8.0;
    }
    let (hx, hy) = (w / 2.0 + gap + biggest / 2.0, h / 2.0 + gap + biggest / 2.0);
    let perimeter = 4.0 * (hx + hy);
    let step = perimeter / n as f64;
    let mut out = Vec::with_capacity(n);
    for (i, &(mw, mh)) in members.iter().take(n).enumerate() {
        // Distance along the path from the top centre, clockwise.
        let d = (i as f64 * step).rem_euclid(perimeter);
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
    // The ring of every owner.
    let mut rings: Vec<Option<(Vec<Point>, f64)>> = vec![None; n];
    for s in satellites {
        let sizes: Vec<(f64, f64)> = s.members.iter().map(|&m| size(graph, m)).collect();
        if let Some(slot) = rings.get_mut(s.owner) {
            *slot = Some(ring(size(graph, s.owner), &sizes));
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
