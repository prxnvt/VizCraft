//! Single points of failure: articulation points and bridges of the undirected view.
//!
//! Directions are ignored: a node whose removal splits the rest apart is a cut point however its
//! links point. The walk tracks the edge it arrived by, not the parent node, so two links between
//! the same pair are two paths and neither is a bridge.

use crate::graph::Graph;

/// Cut points and bridges of a graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cuts {
    /// Articulation points, ascending.
    pub points: Vec<usize>,
    /// Bridges (edge indices), ascending.
    pub bridges: Vec<usize>,
}

/// The articulation points and bridges of `g`'s undirected view, by an iterative lowlink walk.
pub fn cuts(g: &Graph) -> Cuts {
    const UNSET: usize = usize::MAX;
    let n = g.len();
    let mut disc = vec![UNSET; n];
    let mut low = vec![0usize; n];
    let mut point = vec![false; n];
    let mut bridge = vec![false; g.edges.len()];
    let mut time = 0usize;
    // (node, edge it was reached by, position in its undirected neighbour list)
    let mut call: Vec<(usize, Option<usize>, usize)> = Vec::new();
    let neighbours = |u: usize, pos: usize| -> Option<(usize, usize)> {
        let out = g.out.get(u).map_or(0, Vec::len);
        if pos < out { g.out.get(u).and_then(|o| o.get(pos)).copied() } else { g.inc.get(u).and_then(|i| i.get(pos - out)).copied() }
    };
    for root in 0..n {
        if disc.get(root).copied() != Some(UNSET) {
            continue;
        }
        let mut root_children = 0usize;
        if let (Some(d), Some(l)) = (disc.get_mut(root), low.get_mut(root)) {
            *d = time;
            *l = time;
        }
        time += 1;
        call.push((root, None, 0));
        while let Some(&mut (u, via, ref mut pos)) = call.last_mut() {
            if let Some((v, e)) = neighbours(u, *pos) {
                *pos += 1;
                if Some(e) == via || v == u {
                    continue;
                }
                let dv = disc.get(v).copied().unwrap_or(UNSET);
                if dv == UNSET {
                    if let (Some(d), Some(l)) = (disc.get_mut(v), low.get_mut(v)) {
                        *d = time;
                        *l = time;
                    }
                    time += 1;
                    if u == root {
                        root_children += 1;
                    }
                    call.push((v, Some(e), 0));
                } else if let Some(lu) = low.get_mut(u) {
                    *lu = (*lu).min(dv);
                }
                continue;
            }
            call.pop();
            let Some(&(p, _, _)) = call.last() else { continue };
            let lu = low.get(u).copied().unwrap_or(0);
            if let Some(lp) = low.get_mut(p) {
                *lp = (*lp).min(lu);
            }
            let dp = disc.get(p).copied().unwrap_or(0);
            if lu > dp
                && let Some(b) = via.and_then(|e| bridge.get_mut(e))
            {
                *b = true;
            }
            if p != root
                && lu >= dp
                && let Some(c) = point.get_mut(p)
            {
                *c = true;
            }
        }
        if root_children >= 2
            && let Some(c) = point.get_mut(root)
        {
            *c = true;
        }
    }
    Cuts {
        points: point.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| i).collect(),
        bridges: bridge.iter().enumerate().filter(|(_, b)| **b).map(|(i, _)| i).collect(),
    }
}

/// The articulation points of `g`'s undirected view, ascending.
pub fn articulation_points(g: &Graph) -> Vec<usize> {
    cuts(g).points
}

/// The bridges of `g`'s undirected view (edge indices), ascending.
pub fn bridges(g: &Graph) -> Vec<usize> {
    cuts(g).bridges
}
