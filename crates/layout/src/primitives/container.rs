//! The nested-container packer: a tree of containers packed from the inside out. Each
//! container's direct children (nodes, and containers already sized) are laid out layered with
//! the edges between their contents, the container is sized to that drawing plus its padding and
//! heading, and the top level is laid out the same way; positions then flow from the top down.

use super::{Claims, Nest, bad, size};
use crate::{Algorithm, Edge, Graph, LayoutError, Node, Options, Point, Resize};

/// Deepest nesting accepted.
const MAX_DEPTH: usize = 64;
/// Padding when a container names none.
const DEFAULT_PADDING: f64 = 20.0;

/// Places every node and sizes every container of `containers`.
pub(super) fn nested(graph: &Graph, containers: &[Nest], opt: &Options) -> Result<(Vec<Point>, Vec<Resize>), LayoutError> {
    const NAME: &str = "nested";
    let n = graph.nodes.len();
    let mut is_box = Claims::new(NAME, n);
    let mut in_box = Claims::new(NAME, n);
    // parent[v]: the container (index into `containers`) v sits directly in.
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut box_of: Vec<Option<usize>> = vec![None; n];
    for (c, nest) in containers.iter().enumerate() {
        is_box.claim(nest.id, "a container")?;
        if let Some(slot) = box_of.get_mut(nest.id) {
            *slot = Some(c);
        }
        for &p in [nest.padding, nest.header].iter().flatten() {
            if !(p.is_finite() && (0.0..=crate::MAX_SPACING).contains(&p)) {
                return Err(bad(NAME, format!("container node {} has an invalid padding or header", nest.id)));
            }
        }
    }
    for (c, nest) in containers.iter().enumerate() {
        for &m in &nest.members {
            in_box.claim(m, &format!("container node {}", nest.id))?;
            if let Some(slot) = parent.get_mut(m) {
                *slot = Some(c);
            }
        }
    }
    // Depth of every node (containers it is inside), refusing cycles and deep nesting.
    let id_of = |c: usize| containers.get(c).map_or(usize::MAX, |b| b.id);
    let mut depth = vec![0usize; n];
    for (v, d) in depth.iter_mut().enumerate() {
        let mut at = v;
        let mut k = 0;
        while let Some(c) = parent.get(at).copied().flatten() {
            k += 1;
            if k > MAX_DEPTH {
                return Err(bad(NAME, format!("node {v} is nested more than {MAX_DEPTH} deep, or containers hold each other")));
            }
            at = id_of(c);
        }
        *d = k;
    }

    // Edges lifted to the pair of siblings that hold their ends, keyed by the container they meet
    // in (`None` for the top level).
    let up = |v: usize| parent.get(v).copied().flatten().map(id_of);
    let mut lifted: Vec<(Option<usize>, usize, usize)> = Vec::new();
    for e in &graph.edges {
        let (mut a, mut b) = (e.source, e.target);
        if a >= n || b >= n || a == b {
            continue;
        }
        let d = |v: usize| depth.get(v).copied().unwrap_or(0);
        while d(a) > d(b) {
            a = up(a).unwrap_or(a);
        }
        while d(b) > d(a) {
            b = up(b).unwrap_or(b);
        }
        let mut guard = 0;
        while up(a) != up(b) && guard <= MAX_DEPTH {
            a = up(a).unwrap_or(a);
            b = up(b).unwrap_or(b);
            guard += 1;
        }
        // A two-way pair (a flow there and one back) is one relationship to the layout: the first
        // direction lifted sets the order, so the pair cannot tie the cycle breaker and put both
        // partners on one side.
        let scope = parent.get(a).copied().flatten();
        if a != b && up(a) == up(b) && !lifted.iter().any(|&(s, x, y)| s == scope && (x, y) == (b, a)) {
            lifted.push((scope, a, b));
        }
    }

    // Lifted edges by the container they meet in (the last slot is the top level).
    let mut edges_in: Vec<Vec<(usize, usize)>> = vec![Vec::new(); containers.len() + 1];
    for (scope, a, b) in lifted {
        if let Some(list) = edges_in.get_mut(scope.unwrap_or(containers.len())) {
            list.push((a, b));
        }
    }
    let mut sizes: Vec<(f64, f64)> = (0..n).map(|v| size(graph, v)).collect();
    // Offset of every node from its container's top-left, or its top-level position.
    let mut offset = vec![Point::default(); n];
    let mut scratch = Scratch { index: vec![usize::MAX; n], options: Options { algorithm: Algorithm::Layered, ..*opt } };
    let top_slot = containers.len();

    // Containers from the deepest out, so every child container is sized before its parent.
    let mut by_depth: Vec<usize> = (0..containers.len()).collect();
    by_depth.sort_by_key(|&c| std::cmp::Reverse(depth.get(id_of(c)).copied().unwrap_or(0)));
    let mut resized = Vec::with_capacity(containers.len());
    for &c in &by_depth {
        let Some(nest) = containers.get(c) else { continue };
        let pad = nest.padding.unwrap_or(DEFAULT_PADDING);
        let head = nest.header.unwrap_or(0.0);
        let (w, h) = if nest.members.is_empty() {
            let (w, h) = size(graph, nest.id);
            (w.max(2.0 * pad), h.max(2.0 * pad + head))
        } else {
            let edges = edges_in.get(c).map(Vec::as_slice).unwrap_or(&[]);
            let (w, h) = scratch.place(&nest.members, edges, &sizes, &mut offset, Point::new(pad, pad + head))?;
            (w + 2.0 * pad, h + 2.0 * pad + head)
        };
        if let Some(slot) = sizes.get_mut(nest.id) {
            *slot = (w, h);
        }
        resized.push(Resize { node: nest.id, width: w, height: h });
    }
    let top: Vec<usize> = (0..n).filter(|&v| parent.get(v).copied().flatten().is_none()).collect();
    scratch.place(&top, edges_in.get(top_slot).map(Vec::as_slice).unwrap_or(&[]), &sizes, &mut offset, Point::default())?;

    // Absolute positions from the top down (shallow nodes first).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&v| depth.get(v).copied().unwrap_or(0));
    let mut pos = vec![Point::default(); n];
    for v in order {
        let o = offset.get(v).copied().unwrap_or_default();
        let base = up(v).and_then(|p| pos.get(p).copied()).unwrap_or_default();
        if let Some(slot) = pos.get_mut(v) {
            *slot = Point::new(base.x + o.x, base.y + o.y);
        }
    }
    resized.sort_by_key(|r| r.node);
    Ok((pos, resized))
}

/// Reused state for laying out one container's children.
struct Scratch {
    /// Local index of every node in the children being laid out, `usize::MAX` elsewhere.
    index: Vec<usize>,
    options: Options,
}

impl Scratch {
    /// Lays `members` out layered with `edges` (between members), records each member's offset
    /// (`inset` plus its position) and returns the drawing's size.
    fn place(
        &mut self,
        members: &[usize],
        edges: &[(usize, usize)],
        sizes: &[(f64, f64)],
        offset: &mut [Point],
        inset: Point,
    ) -> Result<(f64, f64), LayoutError> {
        for (i, &m) in members.iter().enumerate() {
            if let Some(slot) = self.index.get_mut(m) {
                *slot = i;
            }
        }
        let local_of = |v: usize| self.index.get(v).copied().filter(|&i| i != usize::MAX);
        let local = Graph {
            nodes: members.iter().map(|&m| sizes.get(m).map_or(Node::new(0.0, 0.0), |&(w, h)| Node::new(w, h))).collect(),
            edges: edges.iter().filter_map(|&(a, b)| Some(Edge::new(local_of(a)?, local_of(b)?))).collect(),
            groups: Vec::new(),
        };
        for &m in members {
            if let Some(slot) = self.index.get_mut(m) {
                *slot = usize::MAX;
            }
        }
        let out = crate::layout(&local, &self.options)?;
        for (&m, p) in members.iter().zip(&out.nodes) {
            if let Some(slot) = offset.get_mut(m) {
                *slot = Point::new(inset.x + p.x, inset.y + p.y);
            }
        }
        Ok((out.bounds.width, out.bounds.height))
    }
}
