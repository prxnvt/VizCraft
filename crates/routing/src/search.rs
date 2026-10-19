//! The sparse orthogonal grid and the A* search over (grid point, heading) states.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::Point;

/// Headings, encoded so that `d ^ 1` is the opposite heading.
pub(crate) const RIGHT: usize = 0;
pub(crate) const LEFT: usize = 1;
pub(crate) const DOWN: usize = 2;
pub(crate) const UP: usize = 3;

/// A rectilinear grid of candidate route points. Node `j * nx + i` sits at `(xs[i], ys[j])`.
/// Horizontal edge `j * (nx - 1) + i` joins nodes i and i + 1 of row j; vertical edge
/// `j * nx + i` joins rows j and j + 1 of column i.
pub(crate) struct Grid {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub node_blocked: Vec<bool>,
    pub h_blocked: Vec<bool>,
    pub v_blocked: Vec<bool>,
    /// Crossings with earlier connectors on each horizontal edge (empty when not penalised).
    pub h_cross: Vec<u32>,
    /// Crossings with earlier connectors on each vertical edge (empty when not penalised).
    pub v_cross: Vec<u32>,
    /// Horizontal edges lying on an earlier connector's run (empty when not penalised).
    pub h_along: Vec<bool>,
    /// Vertical edges lying on an earlier connector's run (empty when not penalised).
    pub v_along: Vec<bool>,
}

impl Grid {
    fn nx(&self) -> usize {
        self.xs.len()
    }

    fn nodes(&self) -> usize {
        self.xs.len().saturating_mul(self.ys.len())
    }

    /// Position of a node, if it exists.
    pub fn point(&self, node: usize) -> Option<Point> {
        let nx = self.nx();
        if nx == 0 {
            return None;
        }
        Some(Point::new(*self.xs.get(node % nx)?, *self.ys.get(node / nx)?))
    }

    /// The node at exactly `p`, if `p` lies on the grid.
    pub fn node_at(&self, p: Point) -> Option<usize> {
        let i = self.xs.partition_point(|&x| x < p.x);
        let j = self.ys.partition_point(|&y| y < p.y);
        // Exact comparison on purpose: terminals are inserted into the grid verbatim.
        let hit = self.xs.get(i).is_some_and(|&x| x.total_cmp(&p.x) == Ordering::Equal)
            && self.ys.get(j).is_some_and(|&y| y.total_cmp(&p.y) == Ordering::Equal);
        hit.then(|| j.saturating_mul(self.nx()).saturating_add(i))
    }

    fn blocked(&self, node: usize) -> bool {
        self.node_blocked.get(node).copied().unwrap_or(true)
    }

    /// The neighbour of `node` in heading `dir`: (node, edge length, crossings on the edge,
    /// whether the edge lies on an earlier connector's run).
    fn step(&self, node: usize, dir: usize) -> Option<(usize, f64, u32, bool)> {
        let nx = self.nx();
        if nx == 0 {
            return None;
        }
        let (i, j) = (node % nx, node / nx);
        let (next, len, blocked, cross, along) = match dir {
            RIGHT => {
                let e = j * (nx - 1) + i;
                (node + 1, self.xs.get(i + 1)? - self.xs.get(i)?, *self.h_blocked.get(e)?, self.h_cross.get(e), self.h_along.get(e))
            }
            LEFT => {
                let e = j * (nx - 1) + i.checked_sub(1)?;
                (node - 1, self.xs.get(i)? - self.xs.get(i - 1)?, *self.h_blocked.get(e)?, self.h_cross.get(e), self.h_along.get(e))
            }
            DOWN => {
                let e = node;
                (node + nx, self.ys.get(j + 1)? - self.ys.get(j)?, *self.v_blocked.get(e)?, self.v_cross.get(e), self.v_along.get(e))
            }
            _ => {
                let e = node.checked_sub(nx)?;
                (e, self.ys.get(j)? - self.ys.get(j.checked_sub(1)?)?, *self.v_blocked.get(e)?, self.v_cross.get(e), self.v_along.get(e))
            }
        };
        if blocked || self.blocked(next) {
            return None;
        }
        Some((next, len, cross.copied().unwrap_or(0), along.copied().unwrap_or(false)))
    }
}

/// Heap entry ordered so that `BinaryHeap` pops the smallest estimate first. Costs are always
/// finite (inputs are sanitised), and `total_cmp` gives a total order regardless, so the order is
/// deterministic. Ties prefer the deeper state, then the lower state index.
struct Entry {
    f: f64,
    g: f64,
    state: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.total_cmp(&self.f).then_with(|| self.g.total_cmp(&other.g)).then_with(|| other.state.cmp(&self.state))
    }
}

/// A lower bound on the bends needed to reach a target offset `(dx, dy)` while heading `dir`,
/// given that the route never reverses on the spot.
fn min_bends(dir: usize, dx: f64, dy: f64) -> u32 {
    let (along, perp) = match dir {
        RIGHT => (dx, dy),
        LEFT => (-dx, dy),
        DOWN => (dy, dx),
        _ => (-dy, dx),
    };
    if perp == 0.0 {
        if along >= 0.0 { 0 } else { 2 }
    } else if along >= 0.0 {
        1
    } else {
        2
    }
}

/// A place the search may start from: a grid node, the heading it must leave in (`None` for
/// any) and the cost already spent reaching it (the stub out of the shape).
pub(crate) struct Source {
    pub node: usize,
    pub dir: Option<usize>,
    pub cost: f64,
}

/// A place the search may finish at: a grid node, the heading it must arrive with (`None` for
/// any) and the cost still to pay after it (the stub into the shape).
pub(crate) struct Target {
    pub node: usize,
    pub dir: Option<usize>,
    pub cost: f64,
}

/// What a successful search found: the node path and which source and target it joins.
pub(crate) struct Found {
    pub nodes: Vec<usize>,
    pub source: usize,
    pub target: usize,
}

/// Finds the cheapest node path from any of `sources` to any of `targets`, minimising the
/// sources' and targets' own costs plus length plus `bend` per bend plus `crossing` per crossing
/// plus `overlap` per unit of length run on top of an earlier connector.
/// Ties go to the earlier source and target, so the result is deterministic. Returns `None` when
/// no path exists.
pub(crate) fn search(grid: &Grid, sources: &[Source], targets: &[Target], bend: f64, crossing: f64, overlap: f64) -> Option<Found> {
    let nodes = grid.nodes();
    let usable = |n: usize| n < nodes && !grid.blocked(n);
    let goal = nodes.checked_mul(4)?;
    let states = goal.checked_add(1)?;
    // Targets by node, and their positions for the heuristic.
    let mut at_node: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut target_points: Vec<Point> = Vec::new();
    for (k, t) in targets.iter().enumerate() {
        if usable(t.node)
            && let Some(p) = grid.point(t.node)
        {
            at_node.entry(t.node).or_default().push(k);
            target_points.push(p);
        }
    }
    if target_points.is_empty() {
        return None;
    }
    let mut best = vec![f64::INFINITY; states];
    let mut parent = vec![usize::MAX; states];
    let mut origin: HashMap<usize, usize> = HashMap::new();
    let mut goal_target = usize::MAX;
    let mut heap = BinaryHeap::new();
    let h = |node: usize, dir: usize| -> f64 {
        grid.point(node).map_or(0.0, |p| {
            target_points.iter().fold(f64::INFINITY, |acc, t| {
                let (dx, dy) = (t.x - p.x, t.y - p.y);
                acc.min(dx.abs() + dy.abs() + bend * f64::from(min_bends(dir, dx, dy)))
            })
        })
    };
    for (k, src) in sources.iter().enumerate() {
        if !usable(src.node) || !src.cost.is_finite() {
            continue;
        }
        let dirs: &[usize] = match src.dir {
            Some(RIGHT) => &[RIGHT],
            Some(LEFT) => &[LEFT],
            Some(DOWN) => &[DOWN],
            Some(_) => &[UP],
            None => &[RIGHT, LEFT, DOWN, UP],
        };
        for &d in dirs {
            let s = src.node * 4 + d;
            if let Some(b) = best.get_mut(s)
                && src.cost < *b
            {
                *b = src.cost;
                origin.insert(s, k);
                heap.push(Entry { f: src.cost + h(src.node, d), g: src.cost, state: s });
            }
        }
    }
    // Reopening states is allowed, so cap the work explicitly to guarantee termination.
    let max_pops = states.saturating_mul(8).saturating_add(64);
    let mut pops = 0usize;
    while let Some(Entry { g, state, .. }) = heap.pop() {
        pops += 1;
        if pops > max_pops {
            return None;
        }
        if state == goal {
            let (nodes, first) = reconstruct(&parent, goal, states)?;
            let source = *origin.get(&first)?;
            return Some(Found { nodes, source, target: goal_target });
        }
        if best.get(state).is_none_or(|&b| g > b) {
            continue;
        }
        let (node, dir) = (state / 4, state % 4);
        if let Some(here) = at_node.get(&node) {
            for &k in here {
                let Some(t) = targets.get(k) else { continue };
                let extra = match t.dir {
                    None => Some(0.0),
                    Some(rd) if rd == dir => Some(0.0),
                    Some(rd) if rd == dir ^ 1 => None,
                    Some(_) => Some(bend),
                };
                if let Some(e) = extra
                    && let Some(b) = best.get_mut(goal)
                    && g + e + t.cost < *b
                {
                    *b = g + e + t.cost;
                    goal_target = k;
                    if let Some(p) = parent.get_mut(goal) {
                        *p = state;
                    }
                    heap.push(Entry { f: *b, g: *b, state: goal });
                }
            }
        }
        for nd in [RIGHT, LEFT, DOWN, UP] {
            if nd == dir ^ 1 {
                continue;
            }
            let Some((next, len, cross, along)) = grid.step(node, nd) else { continue };
            let ng = g + len + if nd == dir { 0.0 } else { bend } + crossing * f64::from(cross) + if along { overlap * len } else { 0.0 };
            let ns = next * 4 + nd;
            if let Some(b) = best.get_mut(ns)
                && ng < *b
            {
                *b = ng;
                if let Some(p) = parent.get_mut(ns) {
                    *p = state;
                }
                origin.remove(&ns);
                heap.push(Entry { f: ng + h(next, nd), g: ng, state: ns });
            }
        }
    }
    None
}

/// Walks the parent links back from the goal and returns the node sequence from the start, with
/// the first state of the path (a source state).
fn reconstruct(parent: &[usize], goal: usize, states: usize) -> Option<(Vec<usize>, usize)> {
    let mut out = Vec::new();
    let mut s = *parent.get(goal)?;
    let mut first = s;
    // A path visits each state at most once, so this bound only trips on a corrupted parent chain.
    for _ in 0..states {
        if s == usize::MAX {
            out.reverse();
            out.dedup();
            return Some((out, first));
        }
        out.push(s / 4);
        first = s;
        s = *parent.get(s)?;
    }
    None
}
