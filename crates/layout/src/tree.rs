//! Tree layouts: the tidy tree drawing and the radial drawing.
//!
//! The tidy drawing follows Reingold and Tilford (1981) as generalised by Walker (1990), with
//! the corrections of Buchheim, Juenger and Leipert (2002) that make it linear-time. Both walks
//! run iteratively so deep trees cannot overflow the stack, and the separation between two
//! contour nodes uses their actual widths. The radial drawing places depth levels on concentric
//! rings and gives every subtree an angular wedge proportional to its number of leaves.
//!
//! Indices used for `[i]` access are local node indices of the sub graph (checked by the
//! caller) or positions in vectors built here, so they are in range by construction.

use crate::geom::{route_free, simplify};
use crate::{Notes, Options, Piece, Point, Sub, overlap};
use std::collections::VecDeque;

/// A rooted tree over the sub graph's nodes, children in edge order.
struct Rooted {
    root: usize,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    /// Breadth-first order from the root (parents before children).
    bfs: Vec<usize>,
    depth: Vec<usize>,
}

impl Rooted {
    fn from_children(n: usize, root: usize, children: Vec<Vec<usize>>) -> Option<Rooted> {
        let mut parent = vec![None; n];
        let mut depth = vec![0usize; n];
        let mut bfs = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        seen[root] = true;
        let mut queue = VecDeque::from([root]);
        while let Some(v) = queue.pop_front() {
            bfs.push(v);
            for &c in &children[v] {
                if seen[c] {
                    return None;
                }
                seen[c] = true;
                parent[c] = Some(v);
                depth[c] = depth[v] + 1;
                queue.push_back(c);
            }
        }
        (bfs.len() == n).then_some(Rooted { root, parent, children, bfs, depth })
    }
}

/// The directed tree, if the connected sub graph is one: exactly one node without incoming
/// edges, every other node with exactly one, and `n - 1` edges.
fn directed_tree(sub: &Sub) -> Option<Rooted> {
    let n = sub.sizes.len();
    if n == 0 || sub.edges.len() + 1 != n {
        return None;
    }
    let mut indeg = vec![0usize; n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &sub.edges {
        indeg[e.t] += 1;
        children[e.s].push(e.t);
    }
    if indeg.iter().any(|&d| d > 1) {
        return None;
    }
    let root = indeg.iter().position(|&d| d == 0)?;
    Rooted::from_children(n, root, children)
}

/// A breadth-first spanning tree that ignores edge directions, rooted at the first node without
/// incoming edges (or node 0).
fn spanning_tree(sub: &Sub) -> Option<Rooted> {
    let n = sub.sizes.len();
    if n == 0 {
        return None;
    }
    let mut indeg = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &sub.edges {
        indeg[e.t] += 1;
        adj[e.s].push(e.t);
        adj[e.t].push(e.s);
    }
    let root = indeg.iter().position(|&d| d == 0).unwrap_or(0);
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut seen = vec![false; n];
    seen[root] = true;
    let mut queue = VecDeque::from([root]);
    while let Some(v) = queue.pop_front() {
        for &w in &adj[v] {
            if !seen[w] {
                seen[w] = true;
                children[v].push(w);
                queue.push_back(w);
            }
        }
    }
    Rooted::from_children(n, root, children)
}

/// Tidy tree drawing, top to bottom. `None` when the sub graph is not a directed tree.
pub(crate) fn tidy(sub: &Sub, opt: &Options) -> Option<Piece> {
    let tree = directed_tree(sub)?;
    let x = walker(&tree, &sub.sizes, opt.node_spacing);

    let max_depth = tree.depth.iter().copied().max().unwrap_or(0);
    let mut level_h = vec![0.0f64; max_depth + 1];
    for (v, &d) in tree.depth.iter().enumerate() {
        level_h[d] = level_h[d].max(sub.sizes[v].1);
    }
    let mut level_top = vec![0.0f64; max_depth + 1];
    let mut y = 0.0;
    for d in 0..=max_depth {
        level_top[d] = y;
        y += level_h[d] + opt.layer_spacing;
    }
    let top = |v: usize| {
        let d = tree.depth[v];
        level_top[d] + (level_h[d] - sub.sizes[v].1) / 2.0
    };
    let pos: Vec<Point> = (0..sub.sizes.len()).map(|v| Point::new(x[v] - sub.sizes[v].0 / 2.0, top(v))).collect();
    let routes = sub
        .edges
        .iter()
        .map(|e| {
            let start = Point::new(x[e.s], top(e.s) + sub.sizes[e.s].1);
            let end = Point::new(x[e.t], top(e.t));
            if opt.orthogonal_edges {
                // Children share the parent's trunk and a bus in the middle of the channel.
                let d = tree.depth[e.s];
                let ym = level_top[d] + level_h[d] + opt.layer_spacing / 2.0;
                simplify(vec![start, Point::new(start.x, ym), Point::new(end.x, ym), end])
            } else {
                vec![start, end]
            }
        })
        .collect();
    Some(Piece { pos, routes })
}

/// Walker's algorithm with the Buchheim-Juenger-Leipert corrections. Returns centre x per node.
fn walker(tree: &Rooted, sizes: &[(f64, f64)], spacing: f64) -> Vec<f64> {
    let n = sizes.len();
    let mut w = Walker {
        tree,
        sizes,
        spacing,
        number: vec![0; n],
        prelim: vec![0.0; n],
        modifier: vec![0.0; n],
        shift: vec![0.0; n],
        change: vec![0.0; n],
        thread: vec![None; n],
        ancestor: (0..n).collect(),
        midpoint: vec![0.0; n],
    };
    for kids in &tree.children {
        for (i, &c) in kids.iter().enumerate() {
            w.number[c] = i;
        }
    }
    // First walk in post-order (reverse breadth-first order visits children before parents).
    for &v in tree.bfs.iter().rev() {
        let kids = &tree.children[v];
        let (Some(&first), Some(&last)) = (kids.first(), kids.last()) else { continue };
        let mut default_ancestor = first;
        for &c in kids {
            w.place_relative_to_left_sibling(c);
            default_ancestor = w.apportion(c, default_ancestor);
        }
        w.execute_shifts(v);
        w.midpoint[v] = (w.prelim[first] + w.prelim[last]) / 2.0;
    }
    w.place_relative_to_left_sibling(tree.root);

    // Second walk in pre-order: accumulate modifiers down the tree.
    let mut x = vec![0.0f64; n];
    let mut stack = vec![(tree.root, 0.0f64)];
    while let Some((v, m)) = stack.pop() {
        x[v] = w.prelim[v] + m;
        for &c in &tree.children[v] {
            stack.push((c, m + w.modifier[v]));
        }
    }
    x
}

struct Walker<'a> {
    tree: &'a Rooted,
    sizes: &'a [(f64, f64)],
    spacing: f64,
    number: Vec<usize>,
    prelim: Vec<f64>,
    modifier: Vec<f64>,
    shift: Vec<f64>,
    change: Vec<f64>,
    thread: Vec<Option<usize>>,
    ancestor: Vec<usize>,
    /// Midpoint of the children's preliminary positions, computed when the node's subtree is done.
    midpoint: Vec<f64>,
}

impl Walker<'_> {
    fn distance(&self, a: usize, b: usize) -> f64 {
        (self.sizes[a].0 + self.sizes[b].0) / 2.0 + self.spacing
    }

    fn left_sibling(&self, v: usize) -> Option<usize> {
        let p = self.tree.parent[v]?;
        let i = self.number[v].checked_sub(1)?;
        self.tree.children[p].get(i).copied()
    }

    fn leftmost_sibling(&self, v: usize) -> usize {
        self.tree.parent[v].and_then(|p| self.tree.children[p].first().copied()).unwrap_or(v)
    }

    fn next_left(&self, v: usize) -> Option<usize> {
        self.tree.children[v].first().copied().or(self.thread[v])
    }

    fn next_right(&self, v: usize) -> Option<usize> {
        self.tree.children[v].last().copied().or(self.thread[v])
    }

    /// The part of the first walk that depends on the left sibling. Running it just before
    /// `apportion` keeps the order of the recursive formulation.
    fn place_relative_to_left_sibling(&mut self, v: usize) {
        let leaf = self.tree.children[v].is_empty();
        if let Some(l) = self.left_sibling(v) {
            self.prelim[v] = self.prelim[l] + self.distance(l, v);
            if !leaf {
                self.modifier[v] = self.prelim[v] - self.midpoint[v];
            }
        } else {
            self.prelim[v] = if leaf { 0.0 } else { self.midpoint[v] };
        }
    }

    fn apportion(&mut self, v: usize, default_ancestor: usize) -> usize {
        let Some(w) = self.left_sibling(v) else { return default_ancestor };
        let mut default_ancestor = default_ancestor;
        let (mut vip, mut vop) = (v, v);
        let mut vim = w;
        let mut vom = self.leftmost_sibling(vip);
        let mut sip = self.modifier[vip];
        let mut sop = self.modifier[vop];
        let mut sim = self.modifier[vim];
        let mut som = self.modifier[vom];
        // Each step descends one level, so the loop is bounded by the tree height.
        while let (Some(nim), Some(nip)) = (self.next_right(vim), self.next_left(vip)) {
            vim = nim;
            vip = nip;
            if let Some(x) = self.next_left(vom) {
                vom = x;
            }
            if let Some(x) = self.next_right(vop) {
                vop = x;
            }
            self.ancestor[vop] = v;
            let shift = (self.prelim[vim] + sim) - (self.prelim[vip] + sip) + self.distance(vim, vip);
            if shift > 0.0 {
                let a = self.pick_ancestor(vim, v, default_ancestor);
                self.move_subtree(a, v, shift);
                sip += shift;
                sop += shift;
            }
            sim += self.modifier[vim];
            sip += self.modifier[vip];
            som += self.modifier[vom];
            sop += self.modifier[vop];
        }
        if let Some(nr) = self.next_right(vim)
            && self.next_right(vop).is_none()
        {
            self.thread[vop] = Some(nr);
            self.modifier[vop] += sim - sop;
        }
        if let Some(nl) = self.next_left(vip)
            && self.next_left(vom).is_none()
        {
            self.thread[vom] = Some(nl);
            self.modifier[vom] += sip - som;
            default_ancestor = v;
        }
        default_ancestor
    }

    fn pick_ancestor(&self, vim: usize, v: usize, default_ancestor: usize) -> usize {
        let a = self.ancestor[vim];
        if self.tree.parent[a].is_some() && self.tree.parent[a] == self.tree.parent[v] { a } else { default_ancestor }
    }

    fn move_subtree(&mut self, wm: usize, wp: usize, shift: f64) {
        let subtrees = self.number[wp].saturating_sub(self.number[wm]).max(1) as f64;
        self.change[wp] -= shift / subtrees;
        self.shift[wp] += shift;
        self.change[wm] += shift / subtrees;
        self.prelim[wp] += shift;
        self.modifier[wp] += shift;
    }

    fn execute_shifts(&mut self, v: usize) {
        let mut shift = 0.0;
        let mut change = 0.0;
        for &c in self.tree.children[v].iter().rev() {
            self.prelim[c] += shift;
            self.modifier[c] += shift;
            change += self.change[c];
            shift += self.shift[c] + change;
        }
    }
}

/// Radial drawing: the root at the centre, depth `d` on ring `d`, wedges proportional to leaf
/// counts. Graphs that are not trees use a breadth-first spanning tree.
pub(crate) fn radial(sub: &Sub, opt: &Options, notes: &mut Notes) -> Piece {
    let n = sub.sizes.len();
    let tree = match directed_tree(sub) {
        Some(t) => t,
        None => {
            notes.push("radial: a component is not a tree; its rings follow a breadth-first spanning tree");
            match spanning_tree(sub) {
                Some(t) => t,
                None => return Piece { pos: vec![Point::default(); n], routes: vec![Vec::new(); sub.edges.len()] },
            }
        }
    };
    let mut leaves = vec![0.0f64; n];
    for &v in tree.bfs.iter().rev() {
        leaves[v] = if tree.children[v].is_empty() { 1.0 } else { tree.children[v].iter().map(|&c| leaves[c]).sum() };
    }
    let tau = std::f64::consts::TAU;
    let mut wedge_start = vec![0.0f64; n];
    let mut wedge = vec![0.0f64; n];
    wedge[tree.root] = tau;
    for &v in &tree.bfs {
        // Below the root, a node's children fan out across at most a half circle centred on the
        // node's own direction. Without the cap a lone child of the root owns the whole circle,
        // and its children land beside the root (at its height) instead of beyond the child.
        let span = if v == tree.root { wedge[v] } else { wedge[v].min(std::f64::consts::PI) };
        let mut start = wedge_start[v] + (wedge[v] - span) / 2.0;
        for &c in &tree.children[v] {
            let size = if leaves[v] > 0.0 { span * leaves[c] / leaves[v] } else { 0.0 };
            wedge_start[c] = start;
            wedge[c] = size;
            start += size;
        }
    }
    let diag = |v: usize| {
        let (w, h) = sub.sizes[v];
        (w * w + h * h).sqrt()
    };
    let max_depth = tree.depth.iter().copied().max().unwrap_or(0);
    let mut extent = vec![0.0f64; max_depth + 1];
    // Ring radius needed so neighbouring wedges on the ring keep their nodes' bounding circles
    // apart: a chord of 2 r sin(theta / 2) must cover the diagonal plus spacing.
    let mut angular_need = vec![0.0f64; max_depth + 1];
    for (v, &d) in tree.depth.iter().enumerate() {
        extent[d] = extent[d].max(diag(v));
        if d > 0 {
            let half = (wedge[v].min(std::f64::consts::PI) / 2.0).sin();
            if half > 1e-12 {
                angular_need[d] = angular_need[d].max((diag(v) + opt.node_spacing) / (2.0 * half));
            }
        }
    }
    let mut radius = vec![0.0f64; max_depth + 1];
    for d in 1..=max_depth {
        let radial_need = radius[d - 1] + (extent[d - 1] + extent[d]) / 2.0 + opt.layer_spacing;
        radius[d] = radial_need.max(angular_need[d]);
    }
    let mut centers: Vec<(f64, f64)> = (0..n)
        .map(|v| {
            let r = radius[tree.depth[v]];
            // Start at twelve o'clock and go clockwise.
            let a = wedge_start[v] + wedge[v] / 2.0 - std::f64::consts::FRAC_PI_2;
            (r * a.cos(), r * a.sin())
        })
        .collect();
    overlap::remove(&mut centers, &sub.sizes, opt.node_spacing);
    let pos: Vec<Point> = centers.iter().zip(&sub.sizes).map(|(&(cx, cy), &(w, h))| Point::new(cx - w / 2.0, cy - h / 2.0)).collect();
    let routes = route_free(sub, &pos, opt.edge_spacing, opt.orthogonal_edges);
    Piece { pos, routes }
}
