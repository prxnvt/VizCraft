//! Drawings around a centre: hub and spoke, concentric rings, and the N-channel voter (channels
//! side by side feeding one node after them).
//!
//! Circle placements measure every node by its half diagonal, so a radius that keeps those
//! circles apart keeps the rectangles apart whatever their angle.

use std::f64::consts::{PI, TAU};

use super::{Claims, Frame, bad, size};
use crate::{Graph, LayoutError, Options, Point};

/// Half the diagonal of node `v`: the radius of the circle around its centre that holds it.
fn reach(graph: &Graph, v: usize) -> f64 {
    let (w, h) = size(graph, v);
    w.hypot(h) / 2.0
}

/// The radius at which `count` nodes of reach `r` spaced evenly on a circle stay `gap` apart.
fn ring_radius(count: usize, r: f64, gap: f64) -> f64 {
    if count < 2 { 0.0 } else { (2.0 * r + gap) / (2.0 * (PI / count as f64).sin()).max(1e-12) }
}

/// Puts `ring` on a circle of `radius` around the origin, from the top clockwise.
fn on_circle(graph: &Graph, ring: &[usize], radius: f64, pos: &mut [Point]) {
    let count = ring.len().max(1) as f64;
    for (i, &v) in ring.iter().enumerate() {
        let angle = -PI / 2.0 + TAU * i as f64 / count;
        let (w, h) = size(graph, v);
        if let Some(slot) = pos.get_mut(v) {
            *slot = Point::new(radius * angle.cos() - w / 2.0, radius * angle.sin() - h / 2.0);
        }
    }
}

/// Puts the centre node `v` on the origin.
fn centre(graph: &Graph, v: usize, pos: &mut [Point]) {
    let (w, h) = size(graph, v);
    if let Some(slot) = pos.get_mut(v) {
        *slot = Point::new(-w / 2.0, -h / 2.0);
    }
}

/// Hub and spoke: every node but the hub on one circle around it, `layer_spacing` clear of the
/// hub and `node_spacing` clear of each other.
pub(super) fn hub_spoke(graph: &Graph, hub: usize, opt: &Options) -> Result<Vec<Point>, LayoutError> {
    let n = graph.nodes.len();
    let mut claims = Claims::new("hubSpoke", n);
    claims.claim(hub, "`hub`")?;
    let spokes = claims.unclaimed();
    let r = spokes.iter().map(|&v| reach(graph, v)).fold(0.0, f64::max);
    let radius = (reach(graph, hub) + r + opt.layer_spacing).max(ring_radius(spokes.len(), r, opt.node_spacing));
    let mut pos = vec![Point::default(); n];
    centre(graph, hub, &mut pos);
    on_circle(graph, &spokes, radius, &mut pos);
    Ok(pos)
}

/// Concentric rings: each ring `layer_spacing` clear of the one inside it (or of the centre) and
/// its nodes `node_spacing` clear of each other.
pub(super) fn rings(graph: &Graph, center: usize, rings: &[Vec<usize>], opt: &Options) -> Result<Vec<Point>, LayoutError> {
    let n = graph.nodes.len();
    let mut claims = Claims::new("rings", n);
    claims.claim(center, "`center`")?;
    for (k, ring) in rings.iter().enumerate() {
        for &v in ring {
            claims.claim(v, &format!("ring {k}"))?;
        }
    }
    claims.require_all("the centre or on a ring")?;
    let mut pos = vec![Point::default(); n];
    centre(graph, center, &mut pos);
    let mut outer = reach(graph, center);
    for ring in rings.iter().filter(|r| !r.is_empty()) {
        let r = ring.iter().map(|&v| reach(graph, v)).fold(0.0, f64::max);
        let radius = (outer + r + opt.layer_spacing).max(ring_radius(ring.len(), r, opt.node_spacing));
        on_circle(graph, ring, radius, &mut pos);
        outer = radius + r;
    }
    Ok(pos)
}

/// The N-channel voter: each channel a chain along the flow, the channels side by side across
/// it, and the voter after the longest channel, centred across all of them.
pub(super) fn voter(graph: &Graph, channels: &[Vec<usize>], voter: usize, opt: &Options) -> Result<Vec<Point>, LayoutError> {
    const NAME: &str = "voter";
    let n = graph.nodes.len();
    if channels.is_empty() {
        return Err(bad(NAME, "`channels` names no channels"));
    }
    let mut claims = Claims::new(NAME, n);
    claims.claim(voter, "`voter`")?;
    for (k, channel) in channels.iter().enumerate() {
        if channel.is_empty() {
            return Err(bad(NAME, format!("channel {k} is empty")));
        }
        for &v in channel {
            claims.claim(v, &format!("channel {k}"))?;
        }
    }
    claims.require_all("the voter or in a channel")?;
    let frame = Frame { dir: opt.direction };
    let ext = |v: usize| frame.extents(size(graph, v));
    let mut pos = vec![Point::default(); n];
    let mut across = 0.0;
    let mut longest: f64 = 0.0;
    for channel in channels {
        let width = channel.iter().map(|&v| ext(v).1).fold(0.0, f64::max);
        let mut along = 0.0;
        for &v in channel {
            let (al, ac) = ext(v);
            if let Some(slot) = pos.get_mut(v) {
                *slot = frame.place(along, across + (width - ac) / 2.0, size(graph, v));
            }
            along += al + opt.layer_spacing;
        }
        longest = longest.max(along);
        across += width + opt.node_spacing;
    }
    let total = (across - opt.node_spacing).max(0.0);
    let (_, ac) = ext(voter);
    if let Some(slot) = pos.get_mut(voter) {
        *slot = frame.place(longest, (total - ac) / 2.0, size(graph, voter));
    }
    Ok(pos)
}
