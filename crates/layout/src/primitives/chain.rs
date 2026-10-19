//! Chains: the pipeline (steps along the flow with an optional parallel band between two of
//! them) and the two-row chain (forward steps with their compensations beside them).
//!
//! Gaps along the flow are the layer spacing; gaps across it (between band members and between
//! the two rows) are the node spacing.

use super::{Claims, Frame, Parallel, bad, size};
use crate::{Graph, LayoutError, Options, Point};

/// The pipeline: every node that is not in the parallel band is a step, in node order.
pub(super) fn pipeline(graph: &Graph, parallel: Option<&Parallel>, opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "pipeline";
    let n = graph.nodes.len();
    let frame = Frame { dir: opt.direction };
    let mut claims = Claims::new(NAME, n);
    let mut band: Vec<usize> = Vec::new();
    if let Some(p) = parallel {
        if p.members.is_empty() {
            return Err(bad(NAME, "the parallel band has no members"));
        }
        for &m in &p.members {
            band.push(claims.claim(m, "the parallel band")?);
        }
    }
    let steps = claims.unclaimed();
    let fan = match parallel {
        Some(p) => {
            let at = |v: usize, what: &str| {
                steps.iter().position(|&s| s == v).ok_or_else(|| bad(NAME, format!("the parallel band's `{what}` (node {v}) is not a step")))
            };
            let (i, j) = (at(p.from, "from")?, at(p.to, "to")?);
            if j != i + 1 {
                return Err(bad(NAME, "the parallel band's `from` and `to` must be consecutive steps"));
            }
            Some(i)
        }
        None => None,
    };
    let mut pos = vec![Point::default(); n];
    let mut along = 0.0;
    for (k, &v) in steps.iter().enumerate() {
        let s = size(graph, v);
        let (al, ac) = frame.extents(s);
        if let Some(slot) = pos.get_mut(v) {
            *slot = frame.place(along, -ac / 2.0, s);
        }
        along += al + opt.layer_spacing;
        if fan == Some(k) {
            let width = band.iter().map(|&m| frame.extents(size(graph, m)).0).fold(0.0, f64::max);
            let total: f64 =
                band.iter().map(|&m| frame.extents(size(graph, m)).1).sum::<f64>() + opt.node_spacing * band.len().saturating_sub(1) as f64;
            let mut across = -total / 2.0;
            for &m in &band {
                let s = size(graph, m);
                let (al, ac) = frame.extents(s);
                if let Some(slot) = pos.get_mut(m) {
                    *slot = frame.place(along + (width - al) / 2.0, across, s);
                }
                across += ac + opt.node_spacing;
            }
            along += width + opt.layer_spacing;
        }
    }
    Ok(pos)
}

/// The two-row chain: `forward` on the first row and `compensation[i]` beside `forward[i]` on
/// the second, each column as wide as its wider node.
pub(super) fn two_rows(graph: &Graph, forward: &[usize], compensation: &[Option<usize>], opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "chain";
    let n = graph.nodes.len();
    if forward.is_empty() {
        return Err(bad(NAME, "`forward` names no steps"));
    }
    if compensation.len() > forward.len() {
        return Err(bad(NAME, "`compensation` has more entries than `forward` has steps"));
    }
    let frame = Frame { dir: opt.direction };
    let mut claims = Claims::new(NAME, n);
    for &v in forward {
        claims.claim(v, "`forward`")?;
    }
    for &v in compensation.iter().flatten() {
        claims.claim(v, "`compensation`")?;
    }
    claims.require_all("a forward or compensation step")?;
    let ext = |v: usize| frame.extents(size(graph, v));
    let top = forward.iter().map(|&v| ext(v).1).fold(0.0, f64::max);
    let bottom_at = top + opt.node_spacing;
    let bottom = compensation.iter().flatten().map(|&v| ext(v).1).fold(0.0, f64::max);
    let mut pos = vec![Point::default(); n];
    let mut along = 0.0;
    for (i, &f) in forward.iter().enumerate() {
        let c = compensation.get(i).copied().flatten();
        let width = ext(f).0.max(c.map_or(0.0, |c| ext(c).0));
        let (al, ac) = ext(f);
        if let Some(slot) = pos.get_mut(f) {
            *slot = frame.place(along + (width - al) / 2.0, (top - ac) / 2.0, size(graph, f));
        }
        if let Some(c) = c {
            let (al, ac) = ext(c);
            if let Some(slot) = pos.get_mut(c) {
                *slot = frame.place(along + (width - al) / 2.0, bottom_at + (bottom - ac) / 2.0, size(graph, c));
            }
        }
        along += width + opt.layer_spacing;
    }
    Ok(pos)
}
