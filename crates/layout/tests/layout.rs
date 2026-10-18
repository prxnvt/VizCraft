//! Behavioural tests for vizcraft-layout: geometry invariants, determinism and hostile input.

use vizcraft_layout::{Algorithm, Direction, Edge, Graph, Group, Layout, LayoutError, Node, Options, Point, Rect, layout};

const EPS: f64 = 1e-6;
const ALGORITHMS: [Algorithm; 4] = [Algorithm::Layered, Algorithm::Tree, Algorithm::Radial, Algorithm::Force];
const DIRECTIONS: [Direction; 4] = [Direction::TopBottom, Direction::BottomTop, Direction::LeftRight, Direction::RightLeft];

/// Deterministic generator for test graphs (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn size(&mut self) -> f64 {
        20.0 + (self.next() % 80) as f64
    }
}

fn opts(algorithm: Algorithm) -> Options {
    Options { algorithm, ..Options::default() }
}

fn graph(sizes: &[(f64, f64)], edges: &[(usize, usize)]) -> Graph {
    Graph {
        nodes: sizes.iter().map(|&(w, h)| Node::new(w, h)).collect(),
        edges: edges.iter().map(|&(s, t)| Edge::new(s, t)).collect(),
        groups: Vec::new(),
    }
}

fn uniform(n: usize, edges: &[(usize, usize)]) -> Graph {
    graph(&vec![(60.0, 40.0); n], edges)
}

fn chain(n: usize) -> Graph {
    let edges: Vec<(usize, usize)> = (1..n).map(|i| (i - 1, i)).collect();
    uniform(n, &edges)
}

/// Complete binary tree in heap order: children of i are 2i+1 and 2i+2.
fn binary_tree(depth: u32) -> Graph {
    let n = (1usize << (depth + 1)) - 1;
    let edges: Vec<(usize, usize)> = (1..n).map(|i| ((i - 1) / 2, i)).collect();
    uniform(n, &edges)
}

fn random_graph(rng: &mut Rng, n: usize, m: usize) -> Graph {
    let sizes: Vec<(f64, f64)> = (0..n).map(|_| (rng.size(), rng.size())).collect();
    let edges: Vec<(usize, usize)> = (0..m).map(|_| (rng.below(n), rng.below(n))).collect();
    graph(&sizes, &edges)
}

fn rects(g: &Graph, l: &Layout) -> Vec<Rect> {
    g.nodes.iter().zip(&l.nodes).map(|(n, p)| Rect::new(p.x, p.y, n.width, n.height)).collect()
}

fn assert_no_overlap(g: &Graph, l: &Layout, what: &str) {
    let r = rects(g, l);
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            assert!(!r[i].overlaps(&r[j], EPS), "{what}: nodes {i} and {j} overlap: {:?} {:?}", r[i], r[j]);
        }
    }
}

/// Every pair is at least `gap` apart on some axis.
fn assert_spacing(g: &Graph, l: &Layout, gap: f64, what: &str) {
    let r = rects(g, l);
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            let (a, b) = (r[i], r[j]);
            let gx = (b.x - a.right()).max(a.x - b.right());
            let gy = (b.y - a.bottom()).max(a.y - b.bottom());
            assert!(gx >= gap - EPS || gy >= gap - EPS, "{what}: nodes {i} and {j} closer than {gap}: {a:?} {b:?}");
        }
    }
}

fn assert_finite(l: &Layout, what: &str) {
    let ok = |p: &Point| p.x.is_finite() && p.y.is_finite();
    assert!(l.nodes.iter().all(ok), "{what}: non-finite node position");
    assert!(l.edges.iter().flatten().all(ok), "{what}: non-finite route point");
    let b = l.bounds;
    assert!(b.width.is_finite() && b.height.is_finite() && b.width >= 0.0 && b.height >= 0.0, "{what}: bad bounds {b:?}");
}

fn on_boundary(r: &Rect, p: Point) -> bool {
    let inside = p.x >= r.x - EPS && p.x <= r.right() + EPS && p.y >= r.y - EPS && p.y <= r.bottom() + EPS;
    let near = (p.x - r.x).abs() < EPS || (p.x - r.right()).abs() < EPS || (p.y - r.y).abs() < EPS || (p.y - r.bottom()).abs() < EPS;
    inside && near
}

fn assert_routes_attached(g: &Graph, l: &Layout, what: &str) {
    let r = rects(g, l);
    for (i, (e, route)) in g.edges.iter().zip(&l.edges).enumerate() {
        assert!(route.len() >= 2, "{what}: edge {i} has a route of {} points", route.len());
        let (first, last) = (route[0], route[route.len() - 1]);
        assert!(on_boundary(&r[e.source], first), "{what}: edge {i} does not start on its source: {first:?} {:?}", r[e.source]);
        assert!(on_boundary(&r[e.target], last), "{what}: edge {i} does not end on its target: {last:?} {:?}", r[e.target]);
    }
}

fn assert_inside_bounds(g: &Graph, l: &Layout, what: &str) {
    let b = l.bounds;
    assert!(b.x.abs() < EPS && b.y.abs() < EPS, "{what}: bounds not at origin");
    for r in rects(g, l) {
        assert!(r.x >= -EPS && r.y >= -EPS && r.right() <= b.right() + EPS && r.bottom() <= b.bottom() + EPS, "{what}: node outside bounds");
    }
    for p in l.edges.iter().flatten() {
        assert!(p.x >= -EPS && p.y >= -EPS && p.x <= b.right() + EPS && p.y <= b.bottom() + EPS, "{what}: route outside bounds");
    }
}

/// Proper crossings between straight segments of different edges.
fn geometric_crossings(l: &Layout) -> usize {
    fn orient(a: Point, b: Point, c: Point) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    let segs: Vec<(usize, Point, Point)> = l.edges.iter().enumerate().flat_map(|(i, r)| r.windows(2).map(move |w| (i, w[0], w[1]))).collect();
    let mut count = 0;
    for a in 0..segs.len() {
        for b in a + 1..segs.len() {
            let (ia, p1, p2) = segs[a];
            let (ib, q1, q2) = segs[b];
            if ia == ib {
                continue;
            }
            let d1 = orient(q1, q2, p1);
            let d2 = orient(q1, q2, p2);
            let d3 = orient(p1, p2, q1);
            let d4 = orient(p1, p2, q2);
            if d1 * d2 < -EPS && d3 * d4 < -EPS {
                count += 1;
            }
        }
    }
    count
}

/// Crossings of a two-layer drawing given the node order along x of each layer.
fn two_layer_crossings(edges: &[(usize, usize)], xpos: &dyn Fn(usize) -> f64) -> usize {
    let mut c = 0;
    for (i, &(a, b)) in edges.iter().enumerate() {
        for &(c2, d) in &edges[i + 1..] {
            let s = (xpos(a) - xpos(c2)) * (xpos(b) - xpos(d));
            if s < 0.0 {
                c += 1;
            }
        }
    }
    c
}

#[test]
fn empty_graph_and_single_node() {
    for alg in ALGORITHMS {
        let l = layout(&Graph::default(), &opts(alg)).unwrap();
        assert!(l.nodes.is_empty() && l.edges.is_empty());
        assert_eq!(l.bounds, Rect::default());

        let g = uniform(1, &[]);
        let l = layout(&g, &opts(alg)).unwrap();
        assert_eq!(l.nodes, vec![Point::new(0.0, 0.0)]);
        assert_eq!(l.bounds, Rect::new(0.0, 0.0, 60.0, 40.0));
    }
}

#[test]
fn layered_dag_edges_go_strictly_down() {
    // A small DAG with a long edge, a diamond and parallel paths.
    let edges = [(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (0, 4), (2, 5), (5, 4), (1, 5)];
    let g = graph(&[(80.0, 30.0), (40.0, 60.0), (50.0, 50.0), (120.0, 20.0), (60.0, 40.0), (30.0, 30.0)], &edges);
    for orthogonal in [false, true] {
        let o = Options { orthogonal_edges: orthogonal, ..opts(Algorithm::Layered) };
        let l = layout(&g, &o).unwrap();
        let r = rects(&g, &l);
        for (i, (e, route)) in g.edges.iter().zip(&l.edges).enumerate() {
            assert!(r[e.source].bottom() < r[e.target].y, "edge {i} not strictly downward");
            for w in route.windows(2) {
                assert!(w[1].y >= w[0].y - EPS, "edge {i} route goes up: {route:?}");
            }
            assert!((route[0].y - r[e.source].bottom()).abs() < EPS);
            assert!((route[route.len() - 1].y - r[e.target].y).abs() < EPS);
            if orthogonal {
                for w in route.windows(2) {
                    assert!((w[0].x - w[1].x).abs() < EPS || (w[0].y - w[1].y).abs() < EPS, "edge {i} not orthogonal: {route:?}");
                }
            }
        }
        assert_no_overlap(&g, &l, "layered dag");
        assert_spacing(&g, &l, 40.0, "layered dag");
        assert_routes_attached(&g, &l, "layered dag");
        assert_inside_bounds(&g, &l, "layered dag");
    }
}

#[test]
fn layered_chain_and_binary_tree_have_no_crossings() {
    for g in [chain(12), binary_tree(4)] {
        let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
        assert_eq!(geometric_crossings(&l), 0);
        assert_no_overlap(&g, &l, "planar");
    }
    // A chain is drawn as a straight vertical line.
    let l = layout(&chain(6), &opts(Algorithm::Layered)).unwrap();
    assert!(l.nodes.iter().all(|p| (p.x - l.nodes[0].x).abs() < EPS));
}

#[test]
fn layered_reduces_crossings() {
    // Two layers wired as a reversed permutation plus noise: input order crosses heavily.
    let mut rng = Rng(7);
    let k = 8;
    let mut edges: Vec<(usize, usize)> = (0..k).map(|i| (i, k + (k - 1 - i))).collect();
    for _ in 0..6 {
        edges.push((rng.below(k), k + rng.below(k)));
    }
    let g = uniform(2 * k, &edges);
    let input = two_layer_crossings(&edges, &|v| v as f64);
    let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
    let placed = two_layer_crossings(&edges, &|v| l.nodes[v].x);
    assert!(placed < input, "crossings not reduced: {placed} vs {input}");

    // Interleaved pairs (0-5, 1-4 ...) admit a crossing-free drawing; the layout finds it.
    let edges = [(0, 7), (1, 6), (2, 5), (3, 4)];
    let g = uniform(8, &edges);
    let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
    assert_eq!(two_layer_crossings(&edges, &|v| l.nodes[v].x), 0);
}

#[test]
fn layered_cycles_and_reversed_edges_point_the_right_way() {
    let edges = [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1), (1, 0)];
    let g = uniform(4, &edges);
    for orthogonal in [false, true] {
        let o = Options { orthogonal_edges: orthogonal, ..opts(Algorithm::Layered) };
        let l = layout(&g, &o).unwrap();
        assert_routes_attached(&g, &l, "cycle");
        assert_no_overlap(&g, &l, "cycle");
    }
}

#[test]
fn self_loops_and_parallel_edges() {
    let g = uniform(3, &[(0, 1), (0, 1), (1, 1), (1, 1), (1, 2), (2, 1)]);
    for alg in ALGORITHMS {
        for orthogonal in [false, true] {
            let o = Options { orthogonal_edges: orthogonal, ..opts(alg) };
            let l = layout(&g, &o).unwrap();
            assert_routes_attached(&g, &l, "loops");
            assert_ne!(l.edges[0], l.edges[1], "{alg:?}: parallel edges overlap");
            assert_ne!(l.edges[2], l.edges[3], "{alg:?}: nested loops overlap");
            assert_eq!(l.edges[2].len(), 4);
            assert_inside_bounds(&g, &l, "loops");
        }
    }
}

#[test]
fn tree_layout_of_binary_tree_is_symmetric_and_spaced() {
    let depth = 4;
    let g = binary_tree(depth);
    let o = opts(Algorithm::Tree);
    let l = layout(&g, &o).unwrap();
    assert!(l.notes.is_empty(), "{:?}", l.notes);
    let cx = |v: usize| l.nodes[v].x + 30.0;
    let root = cx(0);
    for d in 1..=depth {
        let first = (1usize << d) - 1;
        let count = 1usize << d;
        for k in 0..count / 2 {
            let (a, b) = (first + k, first + count - 1 - k);
            assert!(((cx(a) - root) + (cx(b) - root)).abs() < 1e-6, "depth {d}: {a} and {b} not mirrored");
            assert!((l.nodes[a].y - l.nodes[b].y).abs() < EPS);
        }
    }
    // Parents are centred over their children.
    for p in 0..(1usize << depth) - 1 {
        assert!((cx(p) - (cx(2 * p + 1) + cx(2 * p + 2)) / 2.0).abs() < 1e-6);
    }
    assert_eq!(geometric_crossings(&l), 0);
    assert_spacing(&g, &l, o.node_spacing.min(o.layer_spacing), "tree");
    assert_routes_attached(&g, &l, "tree");
}

#[test]
fn tree_layout_handles_uneven_trees_and_falls_back() {
    let mut rng = Rng(11);
    let n = 200;
    let sizes: Vec<(f64, f64)> = (0..n).map(|_| (rng.size(), rng.size())).collect();
    let edges: Vec<(usize, usize)> = (1..n).map(|i| (rng.below(i), i)).collect();
    let g = graph(&sizes, &edges);
    for orthogonal in [false, true] {
        let o = Options { orthogonal_edges: orthogonal, ..opts(Algorithm::Tree) };
        let l = layout(&g, &o).unwrap();
        assert!(l.notes.is_empty());
        assert_no_overlap(&g, &l, "uneven tree");
        assert_spacing(&g, &l, 40.0, "uneven tree");
        assert_routes_attached(&g, &l, "uneven tree");
    }
    // A diamond is not a tree: layered fallback with a note.
    let g = uniform(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
    let l = layout(&g, &opts(Algorithm::Tree)).unwrap();
    assert!(!l.notes.is_empty());
    assert_no_overlap(&g, &l, "tree fallback");
    assert_routes_attached(&g, &l, "tree fallback");
}

#[test]
fn directions_are_transforms() {
    let edges = [(0, 1), (0, 2), (1, 3), (2, 3), (0, 3), (3, 4)];
    let sizes = [(80.0, 30.0), (40.0, 60.0), (50.0, 50.0), (120.0, 20.0), (60.0, 40.0)];
    let swapped: Vec<(f64, f64)> = sizes.iter().map(|&(w, h)| (h, w)).collect();
    let g = graph(&sizes, &edges);
    let gs = graph(&swapped, &edges);
    for alg in [Algorithm::Layered, Algorithm::Tree] {
        for orthogonal in [false, true] {
            let run = |g: &Graph, direction| layout(g, &Options { direction, orthogonal_edges: orthogonal, ..opts(alg) }).unwrap();
            let tb = run(&g, Direction::TopBottom);
            let bt = run(&g, Direction::BottomTop);
            let lr = run(&g, Direction::LeftRight);
            let rl = run(&g, Direction::RightLeft);
            let tb_swapped = run(&gs, Direction::TopBottom);
            let h = tb.bounds.height;
            for (v, (w, ht)) in sizes.iter().copied().enumerate() {
                assert!((bt.nodes[v].x - tb.nodes[v].x).abs() < EPS);
                assert!((bt.nodes[v].y - (h - tb.nodes[v].y - ht)).abs() < EPS);
                assert!((lr.nodes[v].x - tb_swapped.nodes[v].y).abs() < EPS);
                assert!((lr.nodes[v].y - tb_swapped.nodes[v].x).abs() < EPS);
                assert!((rl.nodes[v].x - (lr.bounds.width - lr.nodes[v].x - w)).abs() < EPS);
                assert!((rl.nodes[v].y - lr.nodes[v].y).abs() < EPS);
            }
            for (a, b) in tb.edges.iter().zip(&bt.edges) {
                for (p, q) in a.iter().zip(b) {
                    assert!((p.x - q.x).abs() < EPS && (q.y - (h - p.y)).abs() < EPS);
                }
            }
            let r = rects(&g, &lr);
            for e in &g.edges {
                assert!(r[e.source].right() < r[e.target].x, "{alg:?}: LR edge not rightward");
            }
            let r = rects(&g, &rl);
            for e in &g.edges {
                assert!(r[e.source].x > r[e.target].right(), "{alg:?}: RL edge not leftward");
            }
            let r = rects(&g, &bt);
            for e in &g.edges {
                assert!(r[e.source].y > r[e.target].bottom(), "{alg:?}: BT edge not upward");
            }
        }
    }
}

#[test]
fn radial_and_force_do_not_overlap() {
    let mut rng = Rng(3);
    let tree_edges: Vec<(usize, usize)> = (1..120).map(|i| (rng.below(i), i)).collect();
    let sizes: Vec<(f64, f64)> = (0..120).map(|_| (rng.size(), rng.size())).collect();
    let tree = graph(&sizes, &tree_edges);
    let general = random_graph(&mut rng, 150, 300);
    for alg in [Algorithm::Radial, Algorithm::Force] {
        for g in [&tree, &general, &binary_tree(5)] {
            let l = layout(g, &opts(alg)).unwrap();
            assert_no_overlap(g, &l, "radial/force");
            assert_routes_attached(g, &l, "radial/force");
            assert_inside_bounds(g, &l, "radial/force");
            assert_finite(&l, "radial/force");
        }
    }
    // Radial rings: the root of a tree sits at the centre of its children.
    let star = uniform(7, &[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6)]);
    let l = layout(&star, &opts(Algorithm::Radial)).unwrap();
    let c = |v: usize| Point::new(l.nodes[v].x + 30.0, l.nodes[v].y + 20.0);
    let d: Vec<f64> = (1..7).map(|v| ((c(v).x - c(0).x).powi(2) + (c(v).y - c(0).y).powi(2)).sqrt()).collect();
    assert!(d.iter().all(|&x| (x - d[0]).abs() < 1e-6 && x > 0.0), "{d:?}");
}

#[test]
fn radial_fans_a_lone_childs_subtree_out_beyond_it() {
    // 0 -> 1, and 1 -> 2, 3: the root's only child owns the whole circle. Its children must land
    // beyond it, seen from the root, not beside the root (which put the sample flowchart's
    // shapes in one row with the root).
    let chain = uniform(4, &[(0, 1), (1, 2), (1, 3)]);
    let l = layout(&chain, &opts(Algorithm::Radial)).unwrap();
    let c = |v: usize| Point::new(l.nodes[v].x + 30.0, l.nodes[v].y + 20.0);
    let out = Point::new(c(1).x - c(0).x, c(1).y - c(0).y);
    for v in [2, 3] {
        let along = (c(v).x - c(1).x) * out.x + (c(v).y - c(1).y) * out.y;
        assert!(along > 0.0, "child {v} of the lone child points back towards the root: {along}");
    }
}

#[test]
fn components_are_packed_apart_and_groups_get_rects() {
    let mut g = uniform(7, &[(0, 1), (1, 2), (3, 4), (5, 6)]);
    g.groups = vec![Group { members: vec![0, 3], padding: 10.0 }, Group { members: vec![], padding: 5.0 }];
    for alg in ALGORITHMS {
        let o = opts(alg);
        let l = layout(&g, &o).unwrap();
        assert_no_overlap(&g, &l, "components");
        let r = rects(&g, &l);
        // Nodes of different components are at least component spacing apart on some axis
        // unless they share a group unit; 5-6 is its own unit.
        for &a in &[5usize, 6] {
            for b in 0..5 {
                let gx = (r[b].x - r[a].right()).max(r[a].x - r[b].right());
                let gy = (r[b].y - r[a].bottom()).max(r[a].y - r[b].bottom());
                assert!(gx >= o.component_spacing - EPS || gy >= o.component_spacing - EPS, "{alg:?}: {a} {b}");
            }
        }
        let gr = l.groups[0];
        for &m in &[0usize, 3] {
            assert!(gr.x <= r[m].x - 10.0 + EPS && gr.y <= r[m].y - 10.0 + EPS);
            assert!(gr.right() >= r[m].right() + 10.0 - EPS && gr.bottom() >= r[m].bottom() + 10.0 - EPS);
        }
        assert_eq!(l.groups[1], Rect::default());
        assert_inside_bounds(&g, &l, "components");
    }
}

#[test]
fn deterministic_across_runs() {
    let mut rng = Rng(99);
    let g = random_graph(&mut rng, 80, 160);
    for alg in ALGORITHMS {
        for direction in DIRECTIONS {
            let o = Options { direction, seed: 42, ..opts(alg) };
            let a = layout(&g, &o).unwrap();
            let b = layout(&g.clone(), &o).unwrap();
            assert_eq!(a, b, "{alg:?} {direction:?} not deterministic");
        }
    }
}

#[test]
fn hostile_input_is_rejected() {
    let bad_size = [f64::NAN, f64::INFINITY, -1.0, 1e300];
    for s in bad_size {
        let g = graph(&[(s, 10.0)], &[]);
        assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidNodeSize { node: 0 }));
        let g = graph(&[(10.0, 10.0), (10.0, s)], &[]);
        assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidNodeSize { node: 1 }));
    }
    let g = uniform(2, &[(0, 1), (0, 2)]);
    assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidEdgeEndpoint { edge: 1 }));
    let g = uniform(2, &[(usize::MAX, 0)]);
    assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidEdgeEndpoint { edge: 0 }));
    let mut g = uniform(2, &[(0, 1)]);
    g.edges[0].weight = f64::NAN;
    assert!(matches!(layout(&g, &Options::default()), Err(LayoutError::InvalidEdgeAttribute { edge: 0 })));
    g.edges[0].weight = 1.0;
    g.edges[0].min_len = u32::MAX;
    assert!(matches!(layout(&g, &Options::default()), Err(LayoutError::InvalidEdgeAttribute { edge: 0 })));
    let mut g = uniform(2, &[]);
    g.groups = vec![Group { members: vec![0, 5], padding: 0.0 }];
    assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidGroupMember { group: 0 }));
    g.groups = vec![Group { members: vec![0], padding: f64::NAN }];
    assert_eq!(layout(&g, &Options::default()), Err(LayoutError::InvalidGroupPadding { group: 0 }));
    let g = uniform(2, &[]);
    for o in [
        Options { node_spacing: f64::NAN, ..Options::default() },
        Options { layer_spacing: -3.0, ..Options::default() },
        Options { edge_spacing: f64::INFINITY, ..Options::default() },
        Options { component_spacing: 1e300, ..Options::default() },
    ] {
        assert!(matches!(layout(&g, &o), Err(LayoutError::InvalidOption { .. })));
    }
    let g = uniform(vizcraft_layout::MAX_NODES + 1, &[]);
    assert!(matches!(layout(&g, &Options::default()), Err(LayoutError::TooManyNodes { .. })));
    let mut g = uniform(2, &[]);
    g.edges = vec![Edge::new(0, 1); vizcraft_layout::MAX_EDGES + 1];
    assert!(matches!(layout(&g, &Options::default()), Err(LayoutError::TooManyEdges { .. })));
}

#[test]
fn extreme_but_valid_input_never_panics() {
    // Zero sizes, zero spacing, maximum sizes, maximum min_len, zero iterations.
    let zero = Options { node_spacing: 0.0, layer_spacing: 0.0, edge_spacing: 0.0, component_spacing: 0.0, max_iterations: 0, ..Options::default() };
    let mut big = uniform(3, &[(0, 1), (1, 2), (2, 0)]);
    for n in big.nodes.iter_mut() {
        *n = Node::new(vizcraft_layout::MAX_NODE_SIZE, vizcraft_layout::MAX_NODE_SIZE);
    }
    big.edges[0].min_len = vizcraft_layout::MAX_MIN_LEN;
    big.edges[1].weight = 0.0;
    let tiny = graph(&[(0.0, 0.0); 6], &[(0, 1), (1, 2), (0, 0), (3, 4), (4, 3), (5, 5)]);
    for alg in ALGORITHMS {
        for direction in DIRECTIONS {
            for o in
                [Options { algorithm: alg, direction, ..zero }, Options { algorithm: alg, direction, max_iterations: u32::MAX, ..Options::default() }]
            {
                for g in [&big, &tiny] {
                    let l = layout(g, &o).unwrap();
                    assert_finite(&l, "extreme");
                    assert_no_overlap(g, &l, "extreme");
                }
            }
        }
    }
}

#[test]
fn random_graphs_never_panic_and_never_overlap() {
    let mut rng = Rng(2024);
    for round in 0..120 {
        let n = 1 + rng.below(25);
        let m = rng.below(50);
        let mut g = random_graph(&mut rng, n, m);
        for e in g.edges.iter_mut() {
            e.min_len = rng.below(4) as u32;
            e.weight = rng.below(5) as f64;
        }
        if rng.below(3) == 0 {
            g.groups = vec![Group { members: (0..rng.below(4)).map(|_| rng.below(n)).collect(), padding: 4.0 }];
        }
        let alg = ALGORITHMS[round % 4];
        let o = Options {
            algorithm: alg,
            direction: DIRECTIONS[(round / 4) % 4],
            orthogonal_edges: round % 3 == 0,
            seed: round as u64,
            ..Options::default()
        };
        let l = layout(&g, &o).unwrap();
        let what = format!("round {round} {alg:?}");
        assert_eq!(l.nodes.len(), n);
        assert_eq!(l.edges.len(), m);
        assert_finite(&l, &what);
        assert_no_overlap(&g, &l, &what);
        assert_routes_attached(&g, &l, &what);
        assert_inside_bounds(&g, &l, &what);
        if matches!(alg, Algorithm::Layered) {
            assert_spacing(&g, &l, o.node_spacing.min(o.layer_spacing), &what);
        }
    }
}

#[test]
fn large_graphs_finish() {
    // 10k nodes with mostly local edges plus some long and backward ones.
    let mut rng = Rng(5);
    let n = 10_000;
    let mut edges = Vec::new();
    for i in 0..n {
        for _ in 0..2 {
            let j = i + 1 + rng.below(4);
            if j < n {
                edges.push((i, j));
            }
        }
        if rng.below(50) == 0 {
            edges.push((i, rng.below(n)));
        }
    }
    let g = uniform(n, &edges);
    let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
    assert_finite(&l, "large layered");
    assert_eq!(l.edges.iter().filter(|r| r.len() < 2).count(), 0);

    let tree_edges: Vec<(usize, usize)> = (1..n).map(|i| (i.saturating_sub(1 + rng.below(30)), i)).collect();
    let tree = uniform(n, &tree_edges);
    let l = layout(&tree, &opts(Algorithm::Tree)).unwrap();
    assert!(l.notes.is_empty());
    assert_finite(&l, "large tree");

    // A deep chain must not overflow the stack in any algorithm that walks trees.
    let deep = chain(n);
    for alg in [Algorithm::Tree, Algorithm::Radial, Algorithm::Layered] {
        let l = layout(&deep, &opts(alg)).unwrap();
        assert_finite(&l, "deep chain");
    }

    // A medium random graph with arbitrary edges through layered and force.
    let g = random_graph(&mut rng, 1_000, 2_500);
    for alg in [Algorithm::Layered, Algorithm::Force] {
        let l = layout(&g, &opts(alg)).unwrap();
        assert_finite(&l, "random 1000");
    }
    let isolated = uniform(n, &[]);
    let l = layout(&isolated, &opts(Algorithm::Force)).unwrap();
    assert_finite(&l, "isolated");
}

#[test]
fn json_round_trip_with_defaults() {
    let g: Graph =
        serde_json::from_str(r#"{"nodes":[{"width":10,"height":10},{"width":10,"height":10}],"edges":[{"source":0,"target":1}]}"#).unwrap();
    assert_eq!(g.edges[0].weight, 1.0);
    assert_eq!(g.edges[0].min_len, 1);
    let o: Options = serde_json::from_str(r#"{"algorithm":"radial","direction":"leftRight","orthogonalEdges":true}"#).unwrap();
    assert_eq!(o.algorithm, Algorithm::Radial);
    assert_eq!(o.direction, Direction::LeftRight);
    assert_eq!(o.node_spacing, Options::default().node_spacing);
    let l = layout(&g, &o).unwrap();
    let back: Layout = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
    assert_eq!(back, l);
}

/// Maximum-size random input through every algorithm; run with `--release -- --ignored`.
#[test]
#[ignore]
fn stress_maximum_size() {
    let mut rng = Rng(77);
    let g = random_graph(&mut rng, vizcraft_layout::MAX_NODES, vizcraft_layout::MAX_EDGES);
    for alg in ALGORITHMS {
        let start = std::time::Instant::now();
        let l = layout(&g, &opts(alg)).unwrap();
        assert_finite(&l, "stress");
        eprintln!("{alg:?}: {:?}", start.elapsed());
    }
}

#[test]
fn layered_layering_keeps_edges_short() {
    // Longest path would put 5 and 6 at the top; network simplex moves them next to 4.
    let g = uniform(7, &[(0, 1), (1, 2), (2, 3), (3, 4), (5, 6), (6, 4)]);
    let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
    assert!((l.nodes[5].y - l.nodes[2].y).abs() < EPS, "{:?}", l.nodes);
    assert!((l.nodes[6].y - l.nodes[3].y).abs() < EPS, "{:?}", l.nodes);
    // min_len is honoured: three layer gaps (the two dummy layers have no height).
    let mut g = uniform(2, &[(0, 1)]);
    g.edges[0].min_len = 3;
    let l = layout(&g, &opts(Algorithm::Layered)).unwrap();
    assert!(l.nodes[1].y - l.nodes[0].y >= 40.0 + 3.0 * 60.0 - EPS, "{:?}", l.nodes);
}
