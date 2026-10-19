//! Strongly connected components (iterative Tarjan) and the cycles they make.

use crate::graph::Graph;
use crate::result::Group;

/// The strongly connected components of a graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Components {
    /// The component index of each node.
    pub of: Vec<usize>,
    /// The nodes of each component, ascending; components ordered by their first node, so both
    /// are in model order.
    pub groups: Vec<Vec<usize>>,
}

/// The strongly connected components of `g`, by Tarjan's algorithm with an explicit stack, so a
/// chain of any length cannot overflow the call stack.
pub fn strongly_connected(g: &Graph) -> Components {
    const UNSET: usize = usize::MAX;
    let n = g.len();
    let mut disc = vec![UNSET; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut raw: Vec<Vec<usize>> = Vec::new();
    let mut time = 0usize;
    // (node, position in its out list)
    let mut call: Vec<(usize, usize)> = Vec::new();
    for root in 0..n {
        if disc.get(root).copied() != Some(UNSET) {
            continue;
        }
        let visit = |v: usize, disc: &mut Vec<usize>, low: &mut Vec<usize>, on: &mut Vec<bool>, st: &mut Vec<usize>, t: &mut usize| {
            if let (Some(d), Some(l), Some(o)) = (disc.get_mut(v), low.get_mut(v), on.get_mut(v)) {
                *d = *t;
                *l = *t;
                *o = true;
            }
            *t += 1;
            st.push(v);
        };
        visit(root, &mut disc, &mut low, &mut on_stack, &mut stack, &mut time);
        call.push((root, 0));
        while let Some(&mut (u, ref mut pos)) = call.last_mut() {
            let next = g.out.get(u).and_then(|o| o.get(*pos)).copied();
            if let Some((v, _)) = next {
                *pos += 1;
                if disc.get(v).copied() == Some(UNSET) {
                    visit(v, &mut disc, &mut low, &mut on_stack, &mut stack, &mut time);
                    call.push((v, 0));
                } else if on_stack.get(v).copied().unwrap_or(false) {
                    let dv = disc.get(v).copied().unwrap_or(UNSET);
                    if let Some(lu) = low.get_mut(u) {
                        *lu = (*lu).min(dv);
                    }
                }
                continue;
            }
            call.pop();
            let lu = low.get(u).copied().unwrap_or(0);
            if let Some(&(p, _)) = call.last()
                && let Some(lp) = low.get_mut(p)
            {
                *lp = (*lp).min(lu);
            }
            if disc.get(u).copied() == Some(lu) {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    if let Some(o) = on_stack.get_mut(w) {
                        *o = false;
                    }
                    comp.push(w);
                    if w == u {
                        break;
                    }
                }
                comp.sort_unstable();
                raw.push(comp);
            }
        }
    }
    raw.sort_by_key(|c| c.first().copied().unwrap_or(usize::MAX));
    let mut of = vec![0usize; n];
    for (ci, c) in raw.iter().enumerate() {
        for &v in c {
            if let Some(slot) = of.get_mut(v) {
                *slot = ci;
            }
        }
    }
    Components { of, groups: raw }
}

/// Whether node `n` has an edge to itself.
pub fn self_loop(g: &Graph, n: usize) -> bool {
    g.out.get(n).is_some_and(|o| o.iter().any(|&(t, _)| t == n))
}

/// The cycles of `g`: every strongly connected component of two or more nodes, and every node
/// with an edge to itself, each with the edges inside it, in model order.
pub fn cycles(g: &Graph) -> Vec<Group> {
    let c = strongly_connected(g);
    let mut out = Vec::new();
    for (ci, nodes) in c.groups.iter().enumerate() {
        let cyclic = nodes.len() > 1 || nodes.first().is_some_and(|&v| self_loop(g, v));
        if !cyclic {
            continue;
        }
        let edges = nodes.iter().flat_map(|&v| g.out.get(v).into_iter().flatten()).filter(|&&(t, _)| c.of.get(t) == Some(&ci)).map(|&(_, e)| e);
        let mut edges: Vec<usize> = edges.collect();
        edges.sort_unstable();
        out.push(Group {
            label: format!("Cycle {}", out.len() + 1),
            elements: g.node_ids(nodes.iter().copied()),
            relationships: g.relationship_ids(edges),
        });
    }
    out
}
