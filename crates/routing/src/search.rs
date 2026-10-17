//! The sparse orthogonal grid and the A* search over (grid point, heading) states.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

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

    /// The neighbour of `node` in heading `dir`: (node, edge length, crossings on the edge).
    fn step(&self, node: usize, dir: usize) -> Option<(usize, f64, u32)> {
        let nx = self.nx();
        if nx == 0 {
            return None;
        }
        let (i, j) = (node % nx, node / nx);
        let (next, len, blocked, cross) = match dir {
            RIGHT => {
                let e = j * (nx - 1) + i;
                (node + 1, self.xs.get(i + 1)? - self.xs.get(i)?, *self.h_blocked.get(e)?, self.h_cross.get(e))
            }
            LEFT => {
                let e = j * (nx - 1) + i.checked_sub(1)?;
                (node - 1, self.xs.get(i)? - self.xs.get(i - 1)?, *self.h_blocked.get(e)?, self.h_cross.get(e))
            }
            DOWN => {
                let e = node;
                (node + nx, self.ys.get(j + 1)? - self.ys.get(j)?, *self.v_blocked.get(e)?, self.v_cross.get(e))
            }
            _ => {
                let e = node.checked_sub(nx)?;
                (e, self.ys.get(j)? - self.ys.get(j.checked_sub(1)?)?, *self.v_blocked.get(e)?, self.v_cross.get(e))
            }
        };
        if blocked || self.blocked(next) {
            return None;
        }
        Some((next, len, cross.copied().unwrap_or(0)))
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

/// What the search needs to know about the two ends.
pub(crate) struct Ends {
    pub start: usize,
    /// Heading the route must leave the start in, or `None` for any.
    pub start_dir: Option<usize>,
    pub end: usize,
    /// Heading the route must arrive at the end with, or `None` for any.
    pub end_dir: Option<usize>,
}

/// Finds the cheapest node path from `ends.start` to `ends.end`, minimising length plus `bend`
/// per bend plus `crossing` per crossing. Returns `None` when no path exists.
pub(crate) fn search(grid: &Grid, ends: &Ends, bend: f64, crossing: f64) -> Option<Vec<usize>> {
    let nodes = grid.nodes();
    if ends.start >= nodes || ends.end >= nodes || grid.blocked(ends.start) || grid.blocked(ends.end) {
        return None;
    }
    let target = grid.point(ends.end)?;
    let goal = nodes.checked_mul(4)?;
    let states = goal.checked_add(1)?;
    let mut best = vec![f64::INFINITY; states];
    let mut parent = vec![usize::MAX; states];
    let mut heap = BinaryHeap::new();
    let h = |node: usize, dir: usize| -> f64 {
        grid.point(node).map_or(0.0, |p| {
            let (dx, dy) = (target.x - p.x, target.y - p.y);
            dx.abs() + dy.abs() + bend * f64::from(min_bends(dir, dx, dy))
        })
    };
    let starts: &[usize] = match ends.start_dir {
        Some(RIGHT) => &[RIGHT],
        Some(LEFT) => &[LEFT],
        Some(DOWN) => &[DOWN],
        Some(_) => &[UP],
        None => &[RIGHT, LEFT, DOWN, UP],
    };
    for &d in starts {
        let s = ends.start * 4 + d;
        if let Some(b) = best.get_mut(s) {
            *b = 0.0;
            heap.push(Entry { f: h(ends.start, d), g: 0.0, state: s });
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
            return reconstruct(&parent, goal, states);
        }
        if best.get(state).is_none_or(|&b| g > b) {
            continue;
        }
        let (node, dir) = (state / 4, state % 4);
        if node == ends.end {
            let extra = match ends.end_dir {
                None => Some(0.0),
                Some(rd) if rd == dir => Some(0.0),
                Some(rd) if rd == dir ^ 1 => None,
                Some(_) => Some(bend),
            };
            if let Some(e) = extra
                && let Some(b) = best.get_mut(goal)
                && g + e < *b
            {
                *b = g + e;
                if let Some(p) = parent.get_mut(goal) {
                    *p = state;
                }
                heap.push(Entry { f: g + e, g: g + e, state: goal });
            }
        }
        for nd in [RIGHT, LEFT, DOWN, UP] {
            if nd == dir ^ 1 {
                continue;
            }
            let Some((next, len, cross)) = grid.step(node, nd) else { continue };
            let ng = g + len + if nd == dir { 0.0 } else { bend } + crossing * f64::from(cross);
            let ns = next * 4 + nd;
            if let Some(b) = best.get_mut(ns)
                && ng < *b
            {
                *b = ng;
                if let Some(p) = parent.get_mut(ns) {
                    *p = state;
                }
                heap.push(Entry { f: ng + h(next, nd), g: ng, state: ns });
            }
        }
    }
    None
}

/// Walks the parent links back from the goal and returns the node sequence from the start.
fn reconstruct(parent: &[usize], goal: usize, states: usize) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut s = *parent.get(goal)?;
    // A path visits each state at most once, so this bound only trips on a corrupted parent chain.
    for _ in 0..states {
        if s == usize::MAX {
            out.reverse();
            out.dedup();
            return Some(out);
        }
        out.push(s / 4);
        s = *parent.get(s)?;
    }
    None
}
