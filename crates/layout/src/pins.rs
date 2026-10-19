//! Pinned nodes: the pass every algorithm's result goes through when a node has a [`Node::pin`].
//!
//! The algorithm first lays the whole graph out as usual, which says where each node belongs
//! relative to its neighbours. Then every pinned node goes to its pin, and the free nodes are
//! placed breadth first out from the pinned ones: each at its nearest placed neighbour's position
//! plus the offset the algorithm put between the two, so a child of a pinned node still lands
//! below it in a top-to-bottom layout. Free nodes with no path to a pinned node keep their own
//! arrangement and go to the right of everything placed. Finally each free node is pushed across
//! the flow until it overlaps no pinned node and no free node placed before it; pinned nodes never
//! move, even when they overlap each other. Edges whose ends moved by different amounts are
//! routed again between the final rectangles.
//!
//! [`Node::pin`]: crate::Node::pin

use std::collections::VecDeque;

use crate::geom::{Bounds, route_free};
use crate::{Algorithm, Graph, Notes, Options, Point, Rect, Sub, SubEdge};

/// Most pushes one free node gets while clearing overlaps before it jumps past everything placed.
const MAX_PUSHES: usize = 64;
/// Upper bound on rectangle comparisons while clearing overlaps; past it the remaining free
/// nodes go in a row beyond everything placed, which needs no comparisons.
const WORK_BUDGET: usize = 30_000_000;

/// Moves pinned nodes to their pins and the free nodes around them, and fixes the routes.
pub(crate) fn apply(graph: &Graph, opt: &Options, pos: &mut [Point], routes: &mut [Vec<Point>], notes: &mut Notes) {
    let n = graph.nodes.len().min(pos.len());
    let size = |v: usize| graph.nodes.get(v).map_or((0.0, 0.0), |node| (node.width, node.height));
    let rel: Vec<Point> = pos.iter().take(n).copied().collect();
    let rel_at = |v: usize| rel.get(v).copied().unwrap_or_default();

    // Neighbours by edges either way and by group membership (groups are kept together).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut link = |a: usize, b: usize| {
        if a != b && a < n && b < n {
            if let Some(l) = adj.get_mut(a) {
                l.push(b);
            }
            if let Some(l) = adj.get_mut(b) {
                l.push(a);
            }
        }
    };
    for e in &graph.edges {
        link(e.source, e.target);
    }
    for g in &graph.groups {
        for pair in g.members.windows(2) {
            if let [a, b] = pair {
                link(*a, *b);
            }
        }
    }
    for l in adj.iter_mut() {
        l.sort_unstable();
        l.dedup();
    }

    let mut at: Vec<Option<Point>> = (0..n).map(|v| graph.nodes.get(v).and_then(|node| node.pin)).collect();
    let mut queue: VecDeque<usize> = (0..n).filter(|&v| at.get(v).is_some_and(Option::is_some)).collect();
    // Free nodes in the order they were placed, which is also the order they clear overlaps in.
    let mut free: Vec<usize> = Vec::new();
    while let Some(m) = queue.pop_front() {
        let Some(Some(base)) = at.get(m).copied() else { continue };
        for &w in adj.get(m).map(Vec::as_slice).unwrap_or(&[]) {
            if at.get(w).is_some_and(Option::is_none) {
                let (rw, rm) = (rel_at(w), rel_at(m));
                if let Some(slot) = at.get_mut(w) {
                    *slot = Some(Point::new(base.x + rw.x - rm.x, base.y + rw.y - rm.y));
                }
                free.push(w);
                queue.push_back(w);
            }
        }
    }
    let rect_at = |v: usize, p: Point| {
        let (w, h) = size(v);
        Rect::new(p.x, p.y, w, h)
    };

    // Free nodes with no path to a pin: their arrangement, right of everything placed.
    let loose: Vec<usize> = (0..n).filter(|&v| at.get(v).is_some_and(Option::is_none)).collect();
    if !loose.is_empty() {
        let mut placed = Bounds::default();
        for (v, p) in at.iter().enumerate() {
            if let Some(p) = p {
                placed.add_rect(&rect_at(v, *p));
            }
        }
        let mut own = Bounds::default();
        for &v in &loose {
            own.add_rect(&rect_at(v, rel_at(v)));
        }
        let (placed, own) = (placed.rect(), own.rect());
        let (dx, dy) = (placed.right() + opt.component_spacing - own.x, placed.y - own.y);
        for &v in &loose {
            let r = rel_at(v);
            if let Some(slot) = at.get_mut(v) {
                *slot = Some(Point::new(r.x + dx, r.y + dy));
            }
        }
        free.extend(loose);
    }

    // Clear overlaps, pinned nodes first and never moved.
    let gap = opt.node_spacing;
    let across_x = !(opt.direction.is_horizontal() && matches!(opt.algorithm, Algorithm::Layered | Algorithm::Tree));
    let mut settled: Vec<Rect> = (0..n).filter_map(|v| graph.nodes.get(v).and_then(|node| node.pin).map(|p| rect_at(v, p))).collect();
    let mut work = 0usize;
    let mut over_budget = false;
    for &v in &free {
        let Some(Some(p)) = at.get(v).copied() else { continue };
        let mut b = rect_at(v, p);
        if over_budget {
            b = beyond(&settled, b, across_x, gap);
        } else {
            let mut pushes = 0;
            loop {
                work = work.saturating_add(settled.len());
                let Some(o) = settled.iter().find(|o| too_close(&b, o, gap)).copied() else { break };
                b = if pushes < MAX_PUSHES { push_past(b, &o, across_x, gap) } else { beyond(&settled, b, across_x, gap) };
                pushes += 1;
                if pushes > MAX_PUSHES || work > WORK_BUDGET {
                    b = beyond(&settled, b, across_x, gap);
                    break;
                }
            }
            if work > WORK_BUDGET {
                over_budget = true;
                notes.push("pins: too many nodes to clear one by one; the rest were placed in a row past the pinned ones");
            }
        }
        if let Some(slot) = at.get_mut(v) {
            *slot = Some(Point::new(b.x, b.y));
        }
        settled.push(b);
    }

    // Routes: moved along when both ends moved alike, otherwise routed again.
    let shift = |v: usize| {
        let (a, r) = (at.get(v).copied().flatten().unwrap_or_default(), rel_at(v));
        Point::new(a.x - r.x, a.y - r.y)
    };
    let final_pos: Vec<Point> = (0..n).map(|v| at.get(v).copied().flatten().unwrap_or_else(|| rel_at(v))).collect();
    let mut again = Sub { sizes: (0..n).map(size).collect(), edges: Vec::new(), order: Vec::new() };
    let mut again_ids: Vec<usize> = Vec::new();
    for (i, e) in graph.edges.iter().enumerate() {
        let (ds, dt) = (shift(e.source), shift(e.target));
        if e.source == e.target || ((ds.x - dt.x).abs() < 1e-9 && (ds.y - dt.y).abs() < 1e-9) {
            if let Some(route) = routes.get_mut(i) {
                for p in route.iter_mut() {
                    p.x += ds.x;
                    p.y += ds.y;
                }
            }
        } else if e.source < n && e.target < n {
            again.edges.push(SubEdge { s: e.source, t: e.target, weight: 1.0, min_len: 1 });
            again_ids.push(i);
        }
    }
    if !again.edges.is_empty() {
        let fresh = route_free(&again, &final_pos, opt.edge_spacing, opt.orthogonal_edges);
        for (i, route) in again_ids.into_iter().zip(fresh) {
            if let Some(slot) = routes.get_mut(i) {
                *slot = route;
            }
        }
    }
    for (slot, p) in pos.iter_mut().zip(final_pos) {
        *slot = p;
    }
}

/// Whether `a` and `b` are closer than `gap` on both axes (so they overlap or crowd each other).
fn too_close(a: &Rect, b: &Rect, gap: f64) -> bool {
    let h = gap / 2.0;
    let (a, b) = (Rect::new(a.x - h, a.y - h, a.width + gap, a.height + gap), Rect::new(b.x - h, b.y - h, b.width + gap, b.height + gap));
    a.overlaps(&b, 1e-9)
}

/// `b` moved across the flow just past `o`.
fn push_past(b: Rect, o: &Rect, across_x: bool, gap: f64) -> Rect {
    if across_x { Rect::new(o.right() + gap, b.y, b.width, b.height) } else { Rect::new(b.x, o.bottom() + gap, b.width, b.height) }
}

/// `b` moved across the flow past every rectangle in `settled`.
fn beyond(settled: &[Rect], b: Rect, across_x: bool, gap: f64) -> Rect {
    let far = settled.iter().fold(f64::NEG_INFINITY, |m, r| m.max(if across_x { r.right() } else { r.bottom() }));
    if !far.is_finite() {
        return b;
    }
    if across_x { Rect::new(b.x.max(far + gap), b.y, b.width, b.height) } else { Rect::new(b.x, b.y.max(far + gap), b.width, b.height) }
}

#[cfg(test)]
mod tests {
    use crate::{Algorithm, Direction, Edge, Graph, Node, Options, Point, Rect, layout};

    const ALGORITHMS: [Algorithm; 4] = [Algorithm::Layered, Algorithm::Tree, Algorithm::Radial, Algorithm::Force];

    fn rects(g: &Graph, out: &crate::Layout) -> Vec<Rect> {
        g.nodes.iter().zip(&out.nodes).map(|(n, p)| Rect::new(p.x, p.y, n.width, n.height)).collect()
    }

    /// A small tree (0 -> 1, 2; 1 -> 3, 4; 2 -> 5) plus a loose node 6.
    fn tree() -> Graph {
        Graph {
            nodes: vec![Node::new(80.0, 40.0); 7],
            edges: [(0, 1), (0, 2), (1, 3), (1, 4), (2, 5)].iter().map(|&(s, t)| Edge::new(s, t)).collect(),
            groups: Vec::new(),
        }
    }

    #[test]
    fn a_pinned_node_keeps_its_position_under_every_algorithm_and_nothing_overlaps_it() {
        for algorithm in ALGORITHMS {
            for direction in [Direction::TopBottom, Direction::LeftRight] {
                let mut g = tree();
                // Pin the root far from the origin and a leaf right where its sibling would go.
                g.nodes[0] = g.nodes[0].pinned(Point::new(1000.0, 500.0));
                g.nodes[4] = g.nodes[4].pinned(Point::new(1000.0, 640.0));
                let opts = Options { algorithm, direction, ..Options::default() };
                let out = layout(&g, &opts).unwrap();
                assert_eq!(out.nodes[0], Point::new(1000.0, 500.0), "{algorithm:?} {direction:?}");
                assert_eq!(out.nodes[4], Point::new(1000.0, 640.0), "{algorithm:?} {direction:?}");
                let r = rects(&g, &out);
                for i in 0..r.len() {
                    for j in i + 1..r.len() {
                        assert!(!r[i].overlaps(&r[j], 1e-6), "{algorithm:?} {direction:?}: {i} and {j} overlap");
                    }
                }
                // Routes start and end on their nodes' boundaries after the move.
                for (e, route) in g.edges.iter().zip(&out.edges) {
                    let (a, b) = (r[e.source], r[e.target]);
                    let (p, q) = (route[0], route[route.len() - 1]);
                    let on = |r: &Rect, p: Point| p.x >= r.x - 1e-6 && p.x <= r.right() + 1e-6 && p.y >= r.y - 1e-6 && p.y <= r.bottom() + 1e-6;
                    assert!(on(&a, p) && on(&b, q), "{algorithm:?}: route of {e:?} detached");
                }
            }
        }
    }

    #[test]
    fn free_nodes_follow_their_pinned_neighbour() {
        let mut g = tree();
        g.nodes[0] = g.nodes[0].pinned(Point::new(-300.0, -200.0));
        let out = layout(&g, &Options::default()).unwrap();
        assert_eq!(out.nodes[0], Point::new(-300.0, -200.0));
        assert!(out.nodes[1].y > -200.0 && out.nodes[2].y > -200.0, "children below the pinned root");
        // The loose node goes right of everything placed.
        let right = (0..6).map(|v| out.nodes[v].x + 80.0).fold(f64::MIN, f64::max);
        assert!(out.nodes[6].x >= right);
        assert!(out.bounds.x <= -300.0 && out.bounds.y <= -200.0, "a pinned layout is not normalised");
    }

    #[test]
    fn hostile_pins_are_refused() {
        for bad in [f64::NAN, f64::INFINITY, 1.0e12] {
            let mut g = tree();
            g.nodes[1].pin = Some(Point::new(bad, 0.0));
            assert!(matches!(layout(&g, &Options::default()), Err(crate::LayoutError::InvalidPin { node: 1 })));
        }
    }

    #[test]
    fn keep_order_draws_three_siblings_in_model_order_where_crossing_reduction_ties() {
        for algorithm in [Algorithm::Layered, Algorithm::Tree] {
            let edges = [(0, 1), (0, 2), (0, 3)];
            let plain =
                Graph { nodes: vec![Node::new(60.0, 30.0); 4], edges: edges.iter().map(|&(s, t)| Edge::new(s, t)).collect(), groups: Vec::new() };
            let opts = Options { algorithm, ..Options::default() };
            let out = layout(&plain, &opts).unwrap();
            assert!(out.nodes[1].x < out.nodes[2].x && out.nodes[2].x < out.nodes[3].x, "{algorithm:?}: edge order without model order");
            // Created in the order 2, 3, 1.
            let mut ordered = plain.clone();
            for (v, o) in [(0, 0), (2, 1), (3, 2), (1, 3)] {
                ordered.nodes[v] = ordered.nodes[v].ordered(o);
            }
            let out = layout(&ordered, &opts).unwrap();
            assert!(out.nodes[2].x < out.nodes[3].x && out.nodes[3].x < out.nodes[1].x, "{algorithm:?}: model order kept");
        }
    }
}
