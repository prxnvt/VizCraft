//! Force-directed layout after Fruchterman and Reingold (1991): springs along edges pull with
//! d^2 / k, every pair of nodes repels with k^2 / d, and a linearly cooling temperature caps how
//! far a node moves per iteration. Small graphs compute repulsion over all pairs; larger ones use
//! the paper's grid variant (repulsion only within 2k). The start is seeded and deterministic,
//! and the result goes through overlap removal.
//!
//! Indices used for `[i]` access are local node indices (checked by the caller) or positions in
//! vectors built here, so they are in range by construction.

use crate::geom::route_free;
use crate::{Options, Piece, Point, Sub, overlap};

/// Graphs up to this many nodes compute repulsion over all pairs.
const EXACT_LIMIT: usize = 500;
/// Upper bound on iterations times per-iteration work.
const WORK_BUDGET: usize = 30_000_000;

/// SplitMix64 (Steele, Lea and Flood; Vigna): small, fast and good enough to scatter start positions.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub(crate) fn layout(sub: &Sub, opt: &Options) -> Piece {
    let n = sub.sizes.len();
    let mean_size = if n > 0 { sub.sizes.iter().map(|&(w, h)| (w + h) / 2.0).sum::<f64>() / n as f64 } else { 0.0 };
    let k = (mean_size + opt.node_spacing).max(1.0);
    let side = k * (n as f64).sqrt();
    let mut rng = SplitMix64(opt.seed);
    let mut p: Vec<(f64, f64)> = (0..n).map(|_| (rng.unit() * side, rng.unit() * side)).collect();

    let exact = n <= EXACT_LIMIT;
    let per_iteration = if exact { n * n / 2 } else { n * 30 } + sub.edges.len() + 1;
    let iterations = (opt.max_iterations as usize).min((WORK_BUDGET / per_iteration).max(1));
    let t0 = (side / 10.0).max(k);
    let k2 = k * k;
    let mut disp = vec![(0.0f64, 0.0f64); n];
    let mut cells: Vec<((i64, i64), usize)> = Vec::new();

    for it in 0..iterations {
        let t = t0 * (1.0 - it as f64 / iterations as f64) + 1e-3;
        disp.iter_mut().for_each(|d| *d = (0.0, 0.0));
        let repel = |i: usize, j: usize, disp: &mut [(f64, f64)]| {
            let (dx, dy) = (p[i].0 - p[j].0, p[i].1 - p[j].1);
            let dist = (dx * dx + dy * dy).sqrt();
            let (ux, uy, dist) = if dist < 1e-9 {
                // Coincident nodes: separate along a direction derived from their indices.
                let a = ((i.wrapping_mul(7919).wrapping_add(j.wrapping_mul(104_729))) % 360) as f64 * std::f64::consts::PI / 180.0;
                (a.cos(), a.sin(), 1e-3)
            } else {
                (dx / dist, dy / dist, dist)
            };
            let f = k2 / dist;
            disp[i].0 += ux * f;
            disp[i].1 += uy * f;
            disp[j].0 -= ux * f;
            disp[j].1 -= uy * f;
        };
        if exact {
            for i in 0..n {
                for j in i + 1..n {
                    repel(i, j, &mut disp);
                }
            }
        } else {
            let cell = 2.0 * k;
            cells.clear();
            cells.extend(p.iter().enumerate().map(|(i, &(x, y))| (((x / cell).floor() as i64, (y / cell).floor() as i64), i)));
            cells.sort_unstable();
            for &((cx, cy), i) in &cells {
                for gx in cx.saturating_sub(1)..=cx.saturating_add(1) {
                    for gy in cy.saturating_sub(1)..=cy.saturating_add(1) {
                        let start = cells.partition_point(|&(c, _)| c < (gx, gy));
                        for &(c, j) in &cells[start..] {
                            if c != (gx, gy) {
                                break;
                            }
                            if j <= i {
                                continue;
                            }
                            let (dx, dy) = (p[i].0 - p[j].0, p[i].1 - p[j].1);
                            if dx * dx + dy * dy < cell * cell {
                                repel(i, j, &mut disp);
                            }
                        }
                    }
                }
            }
        }
        for e in &sub.edges {
            let (dx, dy) = (p[e.t].0 - p[e.s].0, p[e.t].1 - p[e.s].1);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-9 {
                continue;
            }
            let f = dist * dist / k * e.weight.min(10.0);
            let (ux, uy) = (dx / dist, dy / dist);
            disp[e.s].0 += ux * f;
            disp[e.s].1 += uy * f;
            disp[e.t].0 -= ux * f;
            disp[e.t].1 -= uy * f;
        }
        for (q, d) in p.iter_mut().zip(&disp) {
            let len = (d.0 * d.0 + d.1 * d.1).sqrt();
            if len > 1e-12 && len.is_finite() {
                let step = len.min(t);
                q.0 += d.0 / len * step;
                q.1 += d.1 / len * step;
            }
        }
    }

    overlap::remove(&mut p, &sub.sizes, opt.node_spacing);
    let pos: Vec<Point> = p.iter().zip(&sub.sizes).map(|(&(cx, cy), &(w, h))| Point::new(cx - w / 2.0, cy - h / 2.0)).collect();
    let routes = route_free(sub, &pos, opt.edge_spacing, opt.orthogonal_edges);
    Piece { pos, routes }
}
