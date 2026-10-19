//! Grids: the matrix (rows by columns with equal cells) and the timeline (events on one axis
//! with equal spacing and their labels beside them).

use super::{Claims, Frame, bad, size};
use crate::{Graph, LayoutError, Options, Point};

/// The matrix: every cell as wide as the widest node and as tall as the tallest, nodes centred
/// in their cells, cells `node_spacing` apart. Exactly one of `rows` and `cols` gives the cells.
pub(super) fn matrix(
    graph: &Graph,
    rows: Option<&[Vec<Option<usize>>]>,
    cols: Option<&[Vec<Option<usize>>]>,
    opt: &Options,
) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "matrix";
    let n = graph.nodes.len();
    // Every filled cell as (row, column, node).
    let cells: Vec<(usize, usize, usize)> = match (rows, cols) {
        (Some(rows), None) => {
            rows.iter().enumerate().flat_map(|(r, row)| row.iter().enumerate().filter_map(move |(c, v)| v.map(|v| (r, c, v)))).collect()
        }
        (None, Some(cols)) => {
            cols.iter().enumerate().flat_map(|(c, col)| col.iter().enumerate().filter_map(move |(r, v)| v.map(|v| (r, c, v)))).collect()
        }
        _ => return Err(bad(NAME, "give exactly one of `rows` and `cols`")),
    };
    let mut claims = Claims::new(NAME, n);
    for &(_, _, v) in &cells {
        claims.claim(v, "a cell")?;
    }
    claims.require_all("in a cell")?;
    let cw = (0..n).map(|v| size(graph, v).0).fold(0.0, f64::max);
    let ch = (0..n).map(|v| size(graph, v).1).fold(0.0, f64::max);
    let mut pos = vec![Point::default(); n];
    for (r, c, v) in cells {
        let (w, h) = size(graph, v);
        if let Some(slot) = pos.get_mut(v) {
            *slot = Point::new(c as f64 * (cw + opt.node_spacing) + (cw - w) / 2.0, r as f64 * (ch + opt.node_spacing) + (ch - h) / 2.0);
        }
    }
    Ok(pos)
}

/// The timeline: events centred at equal steps along the flow, each label centred beside its
/// event on the far side of the axis (below it for a horizontal timeline).
pub(super) fn timeline(graph: &Graph, events: Option<&[usize]>, labels: &[Option<usize>], opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "timeline";
    let n = graph.nodes.len();
    let frame = Frame { dir: opt.direction };
    let mut claims = Claims::new(NAME, n);
    let events: Vec<usize> = match events {
        Some(e) => {
            let mut out = Vec::with_capacity(e.len());
            for &v in e {
                out.push(claims.claim(v, "`events`")?);
            }
            for &v in labels.iter().flatten() {
                claims.claim(v, "`labels`")?;
            }
            claims.require_all("an event or a label")?;
            out
        }
        None => {
            for &v in labels.iter().flatten() {
                claims.claim(v, "`labels`")?;
            }
            claims.unclaimed()
        }
    };
    if labels.len() > events.len() {
        return Err(bad(NAME, "`labels` has more entries than there are events"));
    }
    let ext = |v: usize| frame.extents(size(graph, v));
    let all = || events.iter().chain(labels.iter().flatten());
    let pitch = all().map(|&v| ext(v).0).fold(0.0, f64::max) + opt.node_spacing;
    let row = events.iter().map(|&v| ext(v).1).fold(0.0, f64::max);
    let mut pos = vec![Point::default(); n];
    for (i, &e) in events.iter().enumerate() {
        let centre = i as f64 * pitch + pitch / 2.0;
        let (al, ac) = ext(e);
        if let Some(slot) = pos.get_mut(e) {
            *slot = frame.place(centre - al / 2.0, (row - ac) / 2.0, size(graph, e));
        }
        if let Some(l) = labels.get(i).copied().flatten() {
            let (al, _) = ext(l);
            if let Some(slot) = pos.get_mut(l) {
                *slot = frame.place(centre - al / 2.0, row + opt.node_spacing / 2.0, size(graph, l));
            }
        }
    }
    Ok(pos)
}
