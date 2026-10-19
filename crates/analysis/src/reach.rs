//! Reachability: breadth-first walks with depths, and the elements no root reaches.

use std::collections::VecDeque;

use crate::graph::{Direction, Graph};

/// Every node reached from `starts` following edges in `dir`, at most `max_depth` steps away
/// (`None`: no limit), with its depth; starts are depth 0. Ascending by node, so in model order.
/// Starts out of range are ignored.
pub fn reach(g: &Graph, starts: &[usize], dir: Direction, max_depth: Option<u32>) -> Vec<(usize, u32)> {
    let depth = depths(g, starts, dir, max_depth);
    depth.iter().enumerate().filter_map(|(i, d)| d.map(|d| (i, d))).collect()
}

/// The depth of each node from `starts` (`None`: not reached).
pub(crate) fn depths(g: &Graph, starts: &[usize], dir: Direction, max_depth: Option<u32>) -> Vec<Option<u32>> {
    let mut depth: Vec<Option<u32>> = vec![None; g.len()];
    let mut queue = VecDeque::new();
    for &s in starts {
        if let Some(d) = depth.get_mut(s)
            && d.is_none()
        {
            *d = Some(0);
            queue.push_back(s);
        }
    }
    while let Some(u) = queue.pop_front() {
        let du = depth.get(u).copied().flatten().unwrap_or(0);
        if max_depth.is_some_and(|m| du >= m) {
            continue;
        }
        for (v, _) in g.step(u, dir) {
            if let Some(d) = depth.get_mut(v)
                && d.is_none()
            {
                *d = Some(du.saturating_add(1));
                queue.push_back(v);
            }
        }
    }
    depth
}

/// The edges a walk with these depths followed: those leaving a reached node that was expanded
/// (below `max_depth`) towards another reached node, in `dir`. Ascending.
pub fn walked_edges(g: &Graph, depth: &[Option<u32>], dir: Direction, max_depth: Option<u32>) -> Vec<usize> {
    let expanded = |n: usize| depth.get(n).copied().flatten().is_some_and(|d| max_depth.is_none_or(|m| d < m));
    let reached = |n: usize| depth.get(n).copied().flatten().is_some();
    let mut out: Vec<usize> = g
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| match dir {
            Direction::Downstream => expanded(e.from) && reached(e.to),
            Direction::Upstream => expanded(e.to) && reached(e.from),
            Direction::Both => (expanded(e.from) && reached(e.to)) || (expanded(e.to) && reached(e.from)),
        })
        .map(|(i, _)| i)
        .collect();
    out.sort_unstable();
    out
}

/// The nodes no root reaches downstream. With no roots, the nodes with no incoming edge are the
/// roots, so a cycle nothing feeds is unreachable. Ascending.
pub fn unreachable(g: &Graph, roots: &[usize]) -> Vec<usize> {
    let sources: Vec<usize>;
    let roots = if roots.is_empty() {
        sources = (0..g.len()).filter(|&n| g.inc.get(n).is_none_or(Vec::is_empty)).collect();
        &sources
    } else {
        roots
    };
    let depth = depths(g, roots, Direction::Downstream, None);
    depth.iter().enumerate().filter(|(_, d)| d.is_none()).map(|(i, _)| i).collect()
}

/// The roots [`unreachable`] uses for `roots`: the ones given, else the nodes with no incoming edge.
pub fn roots_of(g: &Graph, roots: &[usize]) -> Vec<usize> {
    if roots.is_empty() { (0..g.len()).filter(|&n| g.inc.get(n).is_none_or(Vec::is_empty)).collect() } else { roots.to_vec() }
}
