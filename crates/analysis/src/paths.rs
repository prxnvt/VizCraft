//! Paths: the shortest between two nodes, and the longest through the whole graph.

use std::collections::VecDeque;

use crate::graph::{Direction, Graph};
use crate::scc::{self, Components};

/// A path: its nodes in walk order and the edges between them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// Nodes, first to last.
    pub nodes: Vec<usize>,
    /// Edge indices, one fewer than the nodes for a simple path.
    pub edges: Vec<usize>,
}

/// The longest path, and whether the graph had cycles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LongestPath {
    /// The path; on a cyclic graph it runs through the condensation, so each cycle counts as one
    /// step and the nodes are the ends of its edges.
    pub path: Path,
    /// The graph has a cycle, so the path was found on the condensation.
    pub cyclic: bool,
}

/// The shortest path (fewest edges) from `from` to `to` following edges in `dir`, breadth-first;
/// `None` when there is none. From a node to itself is the one-node path.
pub fn shortest_path(g: &Graph, from: usize, to: usize, dir: Direction) -> Option<Path> {
    if from >= g.len() || to >= g.len() {
        return None;
    }
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; g.len()];
    let mut seen = vec![false; g.len()];
    let mut queue = VecDeque::from([from]);
    if let Some(s) = seen.get_mut(from) {
        *s = true;
    }
    while let Some(u) = queue.pop_front() {
        if u == to {
            break;
        }
        for (v, e) in g.step(u, dir) {
            if let Some(s) = seen.get_mut(v)
                && !*s
            {
                *s = true;
                if let Some(p) = prev.get_mut(v) {
                    *p = Some((u, e));
                }
                queue.push_back(v);
            }
        }
    }
    if !seen.get(to).copied().unwrap_or(false) {
        return None;
    }
    let mut path = Path { nodes: vec![to], edges: Vec::new() };
    let mut at = to;
    while at != from {
        let (p, e) = prev.get(at).copied().flatten()?;
        path.nodes.push(p);
        path.edges.push(e);
        at = p;
    }
    path.nodes.reverse();
    path.edges.reverse();
    Some(path)
}

/// Longest-path layers on the condensation: for each component, the most edges on a path from
/// any source component, with the edge that got it there.
struct Layers {
    comps: Components,
    dist: Vec<u32>,
    via: Vec<Option<usize>>,
    cyclic: bool,
}

fn layers_of(g: &Graph) -> Layers {
    let comps = scc::strongly_connected(g);
    let k = comps.groups.len();
    let cyclic = comps.groups.iter().any(|c| c.len() > 1) || (0..g.len()).any(|n| scc::self_loop(g, n));
    let comp = |n: usize| comps.of.get(n).copied().unwrap_or(0);
    // Edges between components, by source component; in-degrees for Kahn's sort.
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut indeg = vec![0usize; k];
    for (i, e) in g.edges.iter().enumerate() {
        let (a, b) = (comp(e.from), comp(e.to));
        if a == b {
            continue;
        }
        if let Some(o) = out.get_mut(a) {
            o.push(i);
        }
        if let Some(d) = indeg.get_mut(b) {
            *d += 1;
        }
    }
    let mut dist = vec![0u32; k];
    let mut via: Vec<Option<usize>> = vec![None; k];
    let mut queue: VecDeque<usize> = (0..k).filter(|&c| indeg.get(c) == Some(&0)).collect();
    while let Some(a) = queue.pop_front() {
        let da = dist.get(a).copied().unwrap_or(0);
        for &e in out.get(a).into_iter().flatten() {
            let b = g.edges.get(e).map_or(0, |e| comp(e.to));
            if let (Some(db), Some(vb)) = (dist.get_mut(b), via.get_mut(b))
                && da + 1 > *db
            {
                *db = da + 1;
                *vb = Some(e);
            }
            if let Some(d) = indeg.get_mut(b) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    queue.push_back(b);
                }
            }
        }
    }
    Layers { comps, dist, via, cyclic }
}

/// The longest-path layer of every node: the most edges on a path to it from a node with no
/// incoming edge, each cycle counting as one node. Sources are layer 0.
pub fn layers(g: &Graph) -> Vec<u32> {
    let l = layers_of(g);
    (0..g.len()).map(|n| l.comps.of.get(n).and_then(|&c| l.dist.get(c)).copied().unwrap_or(0)).collect()
}

/// The longest path through `g` counted in edges, by a topological sort (Kahn's algorithm) of its
/// condensation; `cyclic` says the graph had cycles. Ties go to the earliest node in model order.
pub fn longest_path(g: &Graph) -> LongestPath {
    let l = layers_of(g);
    let mut end = None;
    let mut best = 0u32;
    for (c, &d) in l.dist.iter().enumerate() {
        if d > best {
            best = d;
            end = Some(c);
        }
    }
    let mut edges = Vec::new();
    let mut at = end;
    while let Some(c) = at {
        let Some(e) = l.via.get(c).copied().flatten() else { break };
        edges.push(e);
        // The walk strictly lowers the layer, so it ends; the bound is a guard all the same.
        if edges.len() > g.edges.len() {
            break;
        }
        at = g.edges.get(e).and_then(|e| l.comps.of.get(e.from)).copied();
    }
    edges.reverse();
    let mut nodes: Vec<usize> = Vec::new();
    for &e in &edges {
        let Some(edge) = g.edges.get(e) else { continue };
        for n in [edge.from, edge.to] {
            if nodes.last() != Some(&n) {
                nodes.push(n);
            }
        }
    }
    if nodes.is_empty() && !g.is_empty() {
        nodes.push(0);
    }
    LongestPath { path: Path { nodes, edges }, cyclic: l.cyclic }
}
