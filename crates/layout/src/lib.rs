//! Automatic layout for diagrams, independent of any diagram model.
//!
//! The caller describes a graph as plain geometry: nodes with a width and a height, directed
//! edges between node indices, and optional groups (clusters) of nodes. [`layout`] returns a
//! top-left position for every node, a polyline for every edge (from the source boundary to the
//! target boundary), a padded rectangle for every group and the overall bounds. Callers keep
//! their own ids and map them to indices; the diagram crate maps its shapes and connectors in
//! and applies positions and routes back, which is how the Cascade 3 `layout.*` commands and
//! the MCP `layout` option use this crate.
//!
//! Four algorithms are available, each implemented from the published literature:
//!
//! | Algorithm | Method |
//! |---|---|
//! | [`Algorithm::Layered`] | Sugiyama framework: greedy cycle breaking (Eades, Lin and Smyth), longest-path layering refined by network simplex (Gansner et al.), dummy nodes for long edges, barycenter sweeps with adjacent transposition, Brandes-Koepf coordinates |
//! | [`Algorithm::Tree`] | Tidy tree drawing (Reingold and Tilford, Walker, with the linear-time corrections of Buchheim, Juenger and Leipert); components that are not trees fall back to the layered layout |
//! | [`Algorithm::Radial`] | The tree drawn on concentric rings, each subtree owning an angular wedge proportional to its leaf count |
//! | [`Algorithm::Force`] | Fruchterman-Reingold with a seeded deterministic start and a cooling schedule, followed by node overlap removal |
//!
//! Disconnected components are laid out independently and packed in rows. Identical input and
//! options always give identical output. Hostile input (non-finite or huge sizes, out-of-range
//! indices, oversized graphs) is rejected with a [`LayoutError`]; every loop is bounded.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod force;
mod geom;
mod layered;
mod overlap;
mod tree;

use serde::{Deserialize, Serialize};

/// Largest number of nodes [`layout`] accepts.
pub const MAX_NODES: usize = 20_000;
/// Largest number of edges [`layout`] accepts.
pub const MAX_EDGES: usize = 100_000;
/// Largest number of groups [`layout`] accepts.
pub const MAX_GROUPS: usize = 10_000;
/// Largest total number of group memberships (summed over all groups) [`layout`] accepts.
pub const MAX_GROUP_MEMBERS: usize = 200_000;
/// Largest node width or height accepted.
pub const MAX_NODE_SIZE: f64 = 1.0e7;
/// Largest spacing or padding value accepted.
pub const MAX_SPACING: f64 = 1.0e6;
/// Largest edge weight accepted.
pub const MAX_WEIGHT: f64 = 1.0e6;
/// Largest edge `min_len` accepted.
pub const MAX_MIN_LEN: u32 = 100;
/// `Options::max_iterations` is clamped to this value.
pub const MAX_ITERATIONS: u32 = 10_000;

/// A point in diagram coordinates (y grows downward).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// A point at `(x, y)`.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle given by its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width (never negative).
    pub width: f64,
    /// Height (never negative).
    pub height: f64,
}

impl Rect {
    /// A rectangle with top-left corner `(x, y)` and the given size.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// Right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Centre point.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Whether the interiors of the two rectangles intersect by more than `eps` on both axes.
    pub fn overlaps(&self, other: &Rect, eps: f64) -> bool {
        let ox = self.right().min(other.right()) - self.x.max(other.x);
        let oy = self.bottom().min(other.bottom()) - self.y.max(other.y);
        ox > eps && oy > eps
    }
}

/// A node to place: only its size matters to the layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Width, finite and in `0..=MAX_NODE_SIZE`.
    pub width: f64,
    /// Height, finite and in `0..=MAX_NODE_SIZE`.
    pub height: f64,
}

impl Node {
    /// A node of the given size.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

fn default_weight() -> f64 {
    1.0
}

fn default_min_len() -> u32 {
    1
}

/// A directed edge between two nodes, referenced by index into [`Graph::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    /// Index of the source node.
    pub source: usize,
    /// Index of the target node.
    pub target: usize,
    /// How strongly the layered layout tries to keep this edge short (default 1).
    #[serde(default = "default_weight")]
    pub weight: f64,
    /// Minimum number of layers the edge spans in the layered layout (default 1; 0 is treated as 1).
    #[serde(default = "default_min_len")]
    pub min_len: u32,
}

impl Edge {
    /// An edge from `source` to `target` with weight 1 and minimum length 1.
    pub const fn new(source: usize, target: usize) -> Self {
        Self { source, target, weight: 1.0, min_len: 1 }
    }
}

/// A cluster of nodes that should stay together; the layout returns its padded bounding box.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Group {
    /// Indices of the member nodes.
    pub members: Vec<usize>,
    /// Space between the members' bounding box and the returned group rectangle.
    #[serde(default)]
    pub padding: f64,
}

/// The input graph.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Graph {
    /// Nodes, referenced by index.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// Directed edges between node indices. Self-loops and parallel edges are allowed.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Optional clusters.
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// Which layout algorithm to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Algorithm {
    /// Layered (hierarchical) drawing for directed graphs; the general-purpose choice.
    #[default]
    Layered,
    /// Tidy tree drawing for forests; other components use the layered drawing.
    Tree,
    /// Tree drawn on concentric rings around the root.
    Radial,
    /// Force-directed drawing for undirected-looking graphs.
    Force,
}

/// The direction edges flow in for the layered and tree layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Sources at the top, edges pointing down.
    #[default]
    TopBottom,
    /// Sources at the bottom, edges pointing up.
    BottomTop,
    /// Sources at the left, edges pointing right.
    LeftRight,
    /// Sources at the right, edges pointing left.
    RightLeft,
}

impl Direction {
    /// Whether layers are stacked horizontally (edges flow along x).
    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::LeftRight | Direction::RightLeft)
    }
}

/// Layout options. Every field has a sensible default, so `Options::default()` works.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    /// Algorithm to run.
    pub algorithm: Algorithm,
    /// Flow direction (layered and tree layouts).
    pub direction: Direction,
    /// Minimum gap between neighbouring nodes in a layer (or between any nodes for force and radial).
    pub node_spacing: f64,
    /// Gap between consecutive layers (tree depths, radial rings).
    pub layer_spacing: f64,
    /// Gap between parallel edge segments and between edge ports on a node side.
    pub edge_spacing: f64,
    /// Gap between packed disconnected components.
    pub component_spacing: f64,
    /// Route edges as axis-aligned polylines instead of straight segments.
    pub orthogonal_edges: bool,
    /// Seed for the force layout's initial positions.
    pub seed: u64,
    /// Iteration cap for the force layout (also bounds the crossing-reduction passes); clamped to `1..=MAX_ITERATIONS`.
    pub max_iterations: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::Layered,
            direction: Direction::TopBottom,
            node_spacing: 40.0,
            layer_spacing: 60.0,
            edge_spacing: 12.0,
            component_spacing: 60.0,
            orthogonal_edges: false,
            seed: 1,
            max_iterations: 300,
        }
    }
}

/// The result of [`layout`]. All coordinates are normalised so that `bounds` starts at the origin.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    /// Top-left corner of every node, in input order.
    pub nodes: Vec<Point>,
    /// Polyline of every edge, in input order, from the source boundary to the target boundary.
    pub edges: Vec<Vec<Point>>,
    /// Padded bounding rectangle of every group, in input order (zero-sized for an empty group).
    pub groups: Vec<Rect>,
    /// Bounding box of nodes, edges and groups.
    pub bounds: Rect,
    /// Human-readable remarks about fallbacks taken (for example a tree layout requested for a graph that is not a forest).
    pub notes: Vec<String>,
}

/// Why [`layout`] refused its input.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayoutError {
    /// More nodes than [`MAX_NODES`].
    #[error("too many nodes: {count} (at most {max})")]
    TooManyNodes {
        /// Nodes given.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// More edges than [`MAX_EDGES`].
    #[error("too many edges: {count} (at most {max})")]
    TooManyEdges {
        /// Edges given.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// More groups than [`MAX_GROUPS`] or more memberships than [`MAX_GROUP_MEMBERS`].
    #[error("too many groups or group members: {count} (at most {max})")]
    TooManyGroups {
        /// Groups or memberships given.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// A node width or height is negative, not finite or larger than [`MAX_NODE_SIZE`].
    #[error("node {node} has an invalid size")]
    InvalidNodeSize {
        /// Index of the node.
        node: usize,
    },
    /// An edge references a node index that does not exist.
    #[error("edge {edge} references a node that does not exist")]
    InvalidEdgeEndpoint {
        /// Index of the edge.
        edge: usize,
    },
    /// An edge weight is negative, not finite or too large, or `min_len` exceeds [`MAX_MIN_LEN`].
    #[error("edge {edge} has an invalid weight or minimum length")]
    InvalidEdgeAttribute {
        /// Index of the edge.
        edge: usize,
    },
    /// A group references a node index that does not exist.
    #[error("group {group} references a node that does not exist")]
    InvalidGroupMember {
        /// Index of the group.
        group: usize,
    },
    /// A group padding is negative, not finite or too large.
    #[error("group {group} has an invalid padding")]
    InvalidGroupPadding {
        /// Index of the group.
        group: usize,
    },
    /// An option value is negative, not finite or too large.
    #[error("invalid option: {name}")]
    InvalidOption {
        /// Name of the option.
        name: &'static str,
    },
}

/// Lay out `graph` with `options`.
///
/// Returns an error for invalid or oversized input; never panics.
pub fn layout(graph: &Graph, options: &Options) -> Result<Layout, LayoutError> {
    validate(graph, options)?;
    let mut opt = *options;
    opt.max_iterations = opt.max_iterations.clamp(1, MAX_ITERATIONS);
    Ok(Orchestrator::new(graph, &opt).run())
}

fn size_ok(v: f64) -> bool {
    v.is_finite() && (0.0..=MAX_NODE_SIZE).contains(&v)
}

fn spacing_ok(v: f64) -> bool {
    v.is_finite() && (0.0..=MAX_SPACING).contains(&v)
}

fn validate(graph: &Graph, options: &Options) -> Result<(), LayoutError> {
    let n = graph.nodes.len();
    if n > MAX_NODES {
        return Err(LayoutError::TooManyNodes { count: n, max: MAX_NODES });
    }
    if graph.edges.len() > MAX_EDGES {
        return Err(LayoutError::TooManyEdges { count: graph.edges.len(), max: MAX_EDGES });
    }
    if graph.groups.len() > MAX_GROUPS {
        return Err(LayoutError::TooManyGroups { count: graph.groups.len(), max: MAX_GROUPS });
    }
    let members: usize = graph.groups.iter().fold(0usize, |acc, g| acc.saturating_add(g.members.len()));
    if members > MAX_GROUP_MEMBERS {
        return Err(LayoutError::TooManyGroups { count: members, max: MAX_GROUP_MEMBERS });
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        if !size_ok(node.width) || !size_ok(node.height) {
            return Err(LayoutError::InvalidNodeSize { node: i });
        }
    }
    for (i, e) in graph.edges.iter().enumerate() {
        if e.source >= n || e.target >= n {
            return Err(LayoutError::InvalidEdgeEndpoint { edge: i });
        }
        if !e.weight.is_finite() || e.weight < 0.0 || e.weight > MAX_WEIGHT || e.min_len > MAX_MIN_LEN {
            return Err(LayoutError::InvalidEdgeAttribute { edge: i });
        }
    }
    for (i, g) in graph.groups.iter().enumerate() {
        if g.members.iter().any(|&m| m >= n) {
            return Err(LayoutError::InvalidGroupMember { group: i });
        }
        if !spacing_ok(g.padding) {
            return Err(LayoutError::InvalidGroupPadding { group: i });
        }
    }
    let checks: [(&'static str, f64); 4] = [
        ("nodeSpacing", options.node_spacing),
        ("layerSpacing", options.layer_spacing),
        ("edgeSpacing", options.edge_spacing),
        ("componentSpacing", options.component_spacing),
    ];
    for (name, v) in checks {
        if !spacing_ok(v) {
            return Err(LayoutError::InvalidOption { name });
        }
    }
    Ok(())
}

/// A connected piece of the input with local node indices, handed to one algorithm.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sub {
    /// Node sizes (width, height) in the frame the algorithm works in.
    pub sizes: Vec<(f64, f64)>,
    /// Edges between local indices; never self-loops.
    pub edges: Vec<SubEdge>,
}

/// An edge of a [`Sub`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct SubEdge {
    pub s: usize,
    pub t: usize,
    pub weight: f64,
    pub min_len: u32,
}

/// What an algorithm returns for a [`Sub`]: top-left positions and one route per sub edge.
#[derive(Debug, Clone, Default)]
pub(crate) struct Piece {
    pub pos: Vec<Point>,
    pub routes: Vec<Vec<Point>>,
}

/// Remarks collected while laying out, deduplicated and kept in first-seen order.
#[derive(Debug, Default)]
pub(crate) struct Notes(Vec<String>);

impl Notes {
    pub fn push(&mut self, note: &str) {
        if !self.0.iter().any(|n| n == note) {
            self.0.push(note.to_string());
        }
    }
}

/// Disjoint-set forest with path compression, iterative so deep chains cannot overflow the stack.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while let Some(&p) = self.parent.get(root) {
            if p == root {
                break;
            }
            root = p;
        }
        let mut cur = x;
        while let Some(&p) = self.parent.get(cur) {
            if p == root {
                break;
            }
            if let Some(slot) = self.parent.get_mut(cur) {
                *slot = root;
            }
            cur = p;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        // Attach the larger root under the smaller one so labels do not depend on edge order.
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        if lo != hi
            && let Some(slot) = self.parent.get_mut(hi)
        {
            *slot = lo;
        }
    }

    /// A dense label per element, numbered in order of each set's first element.
    fn labels(&mut self) -> (Vec<usize>, usize) {
        let n = self.parent.len();
        let mut label_of_root = vec![usize::MAX; n];
        let mut labels = vec![0usize; n];
        let mut next = 0usize;
        for (v, label) in labels.iter_mut().enumerate() {
            let r = self.find(v);
            if let Some(slot) = label_of_root.get_mut(r) {
                if *slot == usize::MAX {
                    *slot = next;
                    next += 1;
                }
                *label = *slot;
            }
        }
        (labels, next)
    }
}

/// Splits the graph into pieces and units, runs the algorithm per piece and packs the results.
struct Orchestrator<'a> {
    graph: &'a Graph,
    opt: &'a Options,
    pos: Vec<Point>,
    routes: Vec<Vec<Point>>,
    notes: Notes,
    /// Scratch map from global node index to local index within the piece being laid out.
    local: Vec<usize>,
}

impl<'a> Orchestrator<'a> {
    fn new(graph: &'a Graph, opt: &'a Options) -> Self {
        let n = graph.nodes.len();
        Self { graph, opt, pos: vec![Point::default(); n], routes: vec![Vec::new(); graph.edges.len()], notes: Notes::default(), local: vec![0; n] }
    }

    fn node_rect(&self, v: usize) -> Rect {
        let p = self.pos.get(v).copied().unwrap_or_default();
        let size = self.graph.nodes.get(v).copied().unwrap_or(Node::new(0.0, 0.0));
        Rect::new(p.x, p.y, size.width, size.height)
    }

    fn run(mut self) -> Layout {
        let graph = self.graph;
        let n = graph.nodes.len();
        if n == 0 {
            return self.finish();
        }
        let force = self.opt.algorithm == Algorithm::Force;

        let mut uf = UnionFind::new(n);
        for e in &graph.edges {
            uf.union(e.source, e.target);
        }
        let (edge_labels, edge_count) = uf.labels();
        // Groups join units so their members are packed next to each other.
        for g in &graph.groups {
            for pair in g.members.windows(2) {
                if let [a, b] = pair {
                    uf.union(*a, *b);
                }
            }
        }
        let (unit_labels, unit_count) = uf.labels();
        // The force layout handles a whole unit at once (group members attract each other); the
        // other algorithms need edge-connected pieces.
        let (piece_of, piece_count) = if force { (unit_labels.clone(), unit_count) } else { (edge_labels, edge_count) };

        let mut piece_nodes: Vec<Vec<usize>> = vec![Vec::new(); piece_count];
        for (v, &p) in piece_of.iter().enumerate() {
            if let Some(list) = piece_nodes.get_mut(p) {
                list.push(v);
            }
        }
        let mut piece_edges: Vec<Vec<usize>> = vec![Vec::new(); piece_count];
        for (i, e) in graph.edges.iter().enumerate() {
            if let Some(list) = piece_of.get(e.source).and_then(|&p| piece_edges.get_mut(p)) {
                list.push(i);
            }
        }
        let mut piece_extra: Vec<Vec<(usize, usize)>> = vec![Vec::new(); piece_count];
        if force {
            for g in &graph.groups {
                for pair in g.members.windows(2) {
                    if let [a, b] = pair
                        && a != b
                        && let Some(list) = piece_of.get(*a).and_then(|&p| piece_extra.get_mut(p))
                    {
                        list.push((*a, *b));
                    }
                }
            }
        }

        let mut piece_bounds = vec![Rect::default(); piece_count];
        for p in 0..piece_count {
            let nodes = piece_nodes.get(p).map(Vec::as_slice).unwrap_or(&[]);
            let edges = piece_edges.get(p).map(Vec::as_slice).unwrap_or(&[]);
            let extra = piece_extra.get(p).map(Vec::as_slice).unwrap_or(&[]);
            let b = self.layout_piece(nodes, edges, extra);
            if let Some(slot) = piece_bounds.get_mut(p) {
                *slot = b;
            }
        }

        // Pack pieces inside each unit, then the units themselves.
        let mut unit_pieces: Vec<Vec<usize>> = vec![Vec::new(); unit_count];
        for (p, nodes) in piece_nodes.iter().enumerate() {
            if let Some(list) = nodes.first().and_then(|&v| unit_labels.get(v)).and_then(|&u| unit_pieces.get_mut(u)) {
                list.push(p);
            }
        }
        let mut group_unit: Vec<Vec<usize>> = vec![Vec::new(); unit_count];
        for (gi, g) in graph.groups.iter().enumerate() {
            if let Some(list) = g.members.first().and_then(|&v| unit_labels.get(v)).and_then(|&u| group_unit.get_mut(u)) {
                list.push(gi);
            }
        }
        let spacing = self.opt.component_spacing;
        let mut unit_bounds = vec![Rect::default(); unit_count];
        for (u, pieces) in unit_pieces.iter().enumerate() {
            let boxes: Vec<Rect> = pieces.iter().map(|&p| piece_bounds.get(p).copied().unwrap_or_default()).collect();
            let offsets = geom::pack(&boxes, spacing);
            let mut acc = geom::Bounds::default();
            for ((&p, off), b) in pieces.iter().zip(&offsets).zip(&boxes) {
                self.translate(piece_nodes.get(p).map(Vec::as_slice).unwrap_or(&[]), piece_edges.get(p).map(Vec::as_slice).unwrap_or(&[]), *off);
                acc.add_rect(&Rect::new(b.x + off.x, b.y + off.y, b.width, b.height));
            }
            for &gi in group_unit.get(u).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(r) = self.group_rect(gi) {
                    acc.add_rect(&r);
                }
            }
            if let Some(slot) = unit_bounds.get_mut(u) {
                *slot = acc.rect();
            }
        }
        let offsets = geom::pack(&unit_bounds, spacing);
        for (pieces, off) in unit_pieces.iter().zip(&offsets) {
            for &p in pieces {
                self.translate(piece_nodes.get(p).map(Vec::as_slice).unwrap_or(&[]), piece_edges.get(p).map(Vec::as_slice).unwrap_or(&[]), *off);
            }
        }
        self.finish()
    }

    fn translate(&mut self, nodes: &[usize], edges: &[usize], off: Point) {
        for &v in nodes {
            if let Some(p) = self.pos.get_mut(v) {
                p.x += off.x;
                p.y += off.y;
            }
        }
        for &e in edges {
            if let Some(route) = self.routes.get_mut(e) {
                for p in route.iter_mut() {
                    p.x += off.x;
                    p.y += off.y;
                }
            }
        }
    }

    fn group_rect(&self, gi: usize) -> Option<Rect> {
        let g = self.graph.groups.get(gi)?;
        let mut acc = geom::Bounds::default();
        for &m in &g.members {
            acc.add_rect(&self.node_rect(m));
        }
        let r = acc.get()?;
        Some(Rect::new(r.x - g.padding, r.y - g.padding, r.width + 2.0 * g.padding, r.height + 2.0 * g.padding))
    }

    /// Lays out one piece in local coordinates and returns its bounds (nodes and routes).
    fn layout_piece(&mut self, nodes: &[usize], edges: &[usize], extra: &[(usize, usize)]) -> Rect {
        let graph = self.graph;
        for (i, &v) in nodes.iter().enumerate() {
            if let Some(slot) = self.local.get_mut(v) {
                *slot = i;
            }
        }
        // Endpoints outside this piece cannot occur (pieces are unions of connected components);
        // the check keeps the algorithms' indexing safe regardless.
        let local = |v: usize| self.local.get(v).copied().filter(|&i| nodes.get(i) == Some(&v));
        let mut sub =
            Sub { sizes: nodes.iter().map(|&v| graph.nodes.get(v).map(|n| (n.width, n.height)).unwrap_or((0.0, 0.0))).collect(), edges: Vec::new() };
        let mut ids: Vec<Option<usize>> = Vec::new();
        let mut loops: Vec<usize> = Vec::new();
        for &e in edges {
            let Some(ed) = graph.edges.get(e) else { continue };
            if ed.source == ed.target {
                loops.push(e);
                continue;
            }
            let (Some(s), Some(t)) = (local(ed.source), local(ed.target)) else { continue };
            sub.edges.push(SubEdge { s, t, weight: ed.weight, min_len: ed.min_len });
            ids.push(Some(e));
        }
        for &(a, b) in extra {
            let (Some(s), Some(t)) = (local(a), local(b)) else { continue };
            sub.edges.push(SubEdge { s, t, weight: 1.0, min_len: 1 });
            ids.push(None);
        }

        let piece = if nodes.len() <= 1 {
            Piece { pos: vec![Point::default(); nodes.len()], routes: vec![Vec::new(); sub.edges.len()] }
        } else {
            self.run_algorithm(&sub)
        };

        for (&v, p) in nodes.iter().zip(&piece.pos) {
            if let Some(slot) = self.pos.get_mut(v) {
                *slot = *p;
            }
        }
        for (id, route) in ids.iter().zip(piece.routes) {
            if let Some(slot) = id.and_then(|e| self.routes.get_mut(e)) {
                *slot = route;
            }
        }
        self.add_self_loops(&loops);

        let mut acc = geom::Bounds::default();
        for &v in nodes {
            acc.add_rect(&self.node_rect(v));
        }
        for &e in edges {
            for p in self.routes.get(e).map(Vec::as_slice).unwrap_or(&[]) {
                acc.add_point(*p);
            }
        }
        acc.rect()
    }

    fn run_algorithm(&mut self, sub: &Sub) -> Piece {
        let opt = self.opt;
        match opt.algorithm {
            Algorithm::Layered => directed(sub, opt, |s| layered::layout(s, opt, &mut self.notes)),
            Algorithm::Tree => directed(sub, opt, |s| match tree::tidy(s, opt) {
                Some(p) => p,
                None => {
                    self.notes.push("tree: a component is not a tree (forest); it was laid out with the layered algorithm");
                    layered::layout(s, opt, &mut self.notes)
                }
            }),
            Algorithm::Radial => tree::radial(sub, opt, &mut self.notes),
            Algorithm::Force => force::layout(sub, opt),
        }
    }

    /// Draws every self-loop as a small rectangular loop on the side of its node that edges do not
    /// use (the right side for vertical flow, the bottom side for horizontal flow).
    fn add_self_loops(&mut self, loops: &[usize]) {
        let opt = self.opt;
        let directed_layout = matches!(opt.algorithm, Algorithm::Layered | Algorithm::Tree);
        let bottom_side = directed_layout && opt.direction.is_horizontal();
        // Count loops per node so nested loops get growing sizes.
        let mut per_node: Vec<(usize, usize)> = Vec::new();
        for &e in loops {
            if let Some(ed) = self.graph.edges.get(e) {
                per_node.push((ed.source, e));
            }
        }
        per_node.sort_unstable();
        let mut i = 0;
        while i < per_node.len() {
            let node = per_node.get(i).map(|p| p.0).unwrap_or(0);
            let mut j = i;
            while per_node.get(j).is_some_and(|p| p.0 == node) {
                j += 1;
            }
            let count = j - i;
            let rect = self.node_rect(node);
            let base = opt.edge_spacing.max(4.0).min((opt.node_spacing * 0.45 / count as f64).max(2.0));
            for k in 0..count {
                let Some(&(_, e)) = per_node.get(i + k) else { continue };
                let route = geom::self_loop(&rect, k, count, base, bottom_side);
                if let Some(slot) = self.routes.get_mut(e) {
                    *slot = route;
                }
            }
            i = j;
        }
    }

    fn finish(mut self) -> Layout {
        let mut acc = geom::Bounds::default();
        for v in 0..self.pos.len() {
            acc.add_rect(&self.node_rect(v));
        }
        for route in &self.routes {
            for p in route {
                acc.add_point(*p);
            }
        }
        let mut groups: Vec<Option<Rect>> = Vec::with_capacity(self.graph.groups.len());
        for gi in 0..self.graph.groups.len() {
            let r = self.group_rect(gi);
            if let Some(r) = &r {
                acc.add_rect(r);
            }
            groups.push(r);
        }
        let b = acc.rect();
        let (dx, dy) = (-b.x, -b.y);
        for p in self.pos.iter_mut() {
            p.x += dx;
            p.y += dy;
        }
        for route in self.routes.iter_mut() {
            for p in route.iter_mut() {
                p.x += dx;
                p.y += dy;
            }
        }
        let groups = groups
            .into_iter()
            .map(|g| match g {
                Some(r) => Rect::new(r.x + dx, r.y + dy, r.width, r.height),
                None => Rect::default(),
            })
            .collect();
        Layout { nodes: self.pos, edges: self.routes, groups, bounds: Rect::new(0.0, 0.0, b.width, b.height), notes: self.notes.0 }
    }
}

/// Runs a top-to-bottom algorithm in its own frame and maps the result to `opt.direction`.
///
/// For horizontal flow the node sizes are transposed first, so the algorithm always sees layers
/// stacked along y.
fn directed(sub: &Sub, opt: &Options, run: impl FnOnce(&Sub) -> Piece) -> Piece {
    let horizontal = opt.direction.is_horizontal();
    let frame;
    let input = if horizontal {
        frame = Sub { sizes: sub.sizes.iter().map(|&(w, h)| (h, w)).collect(), edges: sub.edges.clone() };
        &frame
    } else {
        sub
    };
    let mut piece = run(input);
    if opt.direction == Direction::TopBottom {
        return piece;
    }
    let mut acc = geom::Bounds::default();
    for (p, &(w, h)) in piece.pos.iter().zip(&input.sizes) {
        acc.add_rect(&Rect::new(p.x, p.y, w, h));
    }
    for route in &piece.routes {
        for p in route {
            acc.add_point(*p);
        }
    }
    let b = acc.rect();
    let flip = b.y + b.bottom();
    let map = |p: Point| match opt.direction {
        Direction::TopBottom => p,
        Direction::BottomTop => Point::new(p.x, flip - p.y),
        Direction::LeftRight => Point::new(p.y, p.x),
        Direction::RightLeft => Point::new(flip - p.y, p.x),
    };
    for (p, &(_, h)) in piece.pos.iter_mut().zip(&input.sizes) {
        // The node's far corner along the flow axis becomes its near corner after mirroring.
        *p = match opt.direction {
            Direction::TopBottom | Direction::LeftRight => map(*p),
            Direction::BottomTop | Direction::RightLeft => map(Point::new(p.x, p.y + h)),
        };
    }
    for route in piece.routes.iter_mut() {
        for p in route.iter_mut() {
            *p = map(*p);
        }
    }
    piece
}
