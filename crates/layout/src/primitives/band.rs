//! Bands (swimlanes): every node names its band; the whole graph is laid out layered once so a
//! step that follows another in a different band still lands after it along the flow, then each
//! band packs its nodes into tracks across the flow and the bands stack with a gap.

use super::{Claims, bad, size};
use crate::{Algorithm, Graph, LayoutError, Options, Point};

/// Places the nodes of `bands`; with a horizontal direction the bands are rows and the flow runs
/// along them, with a vertical one they are columns.
pub(super) fn bands(graph: &Graph, bands: &[Vec<usize>], opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "bands";
    let n = graph.nodes.len();
    if bands.is_empty() {
        return Err(bad(NAME, "`bands` names no bands"));
    }
    let mut claims = Claims::new(NAME, n);
    for (b, band) in bands.iter().enumerate() {
        for &v in band {
            claims.claim(v, &format!("band {b}"))?;
        }
    }
    claims.require_all("in a band")?;
    let mut plain = graph.clone();
    for node in plain.nodes.iter_mut() {
        node.pin = None;
    }
    let flow = crate::layout(&plain, &Options { algorithm: Algorithm::Layered, ..*opt })?;
    let horizontal = opt.direction.is_horizontal();
    // (along, across) of a node: its position along the flow and its extent across it.
    let along_of = |v: usize| flow.nodes.get(v).map_or(0.0, |p| if horizontal { p.x } else { p.y });
    let cross_of = |v: usize| flow.nodes.get(v).map_or(0.0, |p| if horizontal { p.y } else { p.x });
    let extents = |v: usize| {
        let (w, h) = size(graph, v);
        if horizontal { (w, h) } else { (h, w) }
    };
    let gap = opt.node_spacing;
    let mut pos = vec![Point::default(); n];
    let mut offset = 0.0;
    for band in bands {
        let mut order: Vec<usize> = band.clone();
        order.sort_by(|&a, &b| cross_of(a).total_cmp(&cross_of(b)).then(a.cmp(&b)));
        // Tracks across the flow: the spans along it each holds, and its thickness.
        let mut tracks: Vec<(Vec<(f64, f64)>, f64)> = Vec::new();
        let mut track_of: Vec<(usize, usize)> = Vec::with_capacity(order.len());
        for &v in &order {
            let (al, ac) = extents(v);
            let (lo, hi) = (along_of(v), along_of(v) + al);
            let free = tracks.iter().position(|(spans, _)| spans.iter().all(|&(a, b)| hi + gap <= a || b + gap <= lo));
            let t = match free {
                Some(t) => t,
                None => {
                    tracks.push((Vec::new(), 0.0));
                    tracks.len() - 1
                }
            };
            if let Some((spans, thick)) = tracks.get_mut(t) {
                spans.push((lo, hi));
                *thick = thick.max(ac);
            }
            track_of.push((v, t));
        }
        let mut starts: Vec<f64> = Vec::with_capacity(tracks.len());
        let mut at = offset;
        for (_, thick) in &tracks {
            starts.push(at);
            at += thick + gap;
        }
        for (v, t) in track_of {
            let (_, ac) = extents(v);
            let start = starts.get(t).copied().unwrap_or(offset);
            let thick = tracks.get(t).map_or(0.0, |t| t.1);
            let across = start + (thick - ac) / 2.0;
            if let Some(slot) = pos.get_mut(v) {
                *slot = if horizontal { Point::new(along_of(v), across) } else { Point::new(across, along_of(v)) };
            }
        }
        // Bands are twice the node spacing apart (tracks inside a band once), so the band edges
        // read clearly; an empty band still takes a gap, so band k is always the k-th across.
        offset = if tracks.is_empty() { offset + gap } else { at + gap };
    }
    Ok(pos)
}
