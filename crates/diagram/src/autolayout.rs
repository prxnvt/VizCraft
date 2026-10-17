//! Automatic placement of shapes, built on `vizcraft-layout`.
//!
//! The page's shapes become layout nodes (sized by their bounding boxes) and the connectors glued
//! at both ends to two of them become layout edges. The `layout.*` commands and the Mermaid and
//! CSV importers all place shapes through [`layout_items`], so they agree on what a layout does.

use std::collections::{HashMap, HashSet};

pub use vizcraft_layout::{Algorithm, Direction, Options};

use crate::geom::Point;
use crate::model::Page;

/// Where a whole-page layout puts its top-left corner, so the drawing does not touch the page edge.
pub const PAGE_MARGIN: f64 = 40.0;

/// Lay out the shapes named by `ids` (or every shape on the page when `None`) with `opts`.
///
/// A selection keeps its previous top-left corner; a whole page starts at [`PAGE_MARGIN`].
/// Connectors between moved shapes lose their stored bends, because those were drawn for the old
/// positions (connectors that avoid shapes get new ones from the engine's routing refresh).
/// Returns the layout's notes (for example a tree layout falling back to layered), or the reason
/// the layout was refused.
pub fn layout_items(page: &mut Page, ids: Option<&[String]>, opts: &Options) -> Result<Vec<String>, String> {
    let wanted: Option<HashSet<&str>> = ids.map(|ids| ids.iter().map(String::as_str).collect());
    let chosen: Vec<usize> =
        page.shapes.iter().enumerate().filter(|(_, s)| wanted.as_ref().is_none_or(|w| w.contains(s.id.as_str()))).map(|(i, _)| i).collect();
    if chosen.is_empty() {
        return Ok(Vec::new());
    }
    let mut node_of: HashMap<&str, usize> = HashMap::new();
    let mut graph = vizcraft_layout::Graph::default();
    for (n, &i) in chosen.iter().enumerate() {
        if let Some(s) = page.shapes.get(i) {
            let b = s.bounds();
            node_of.insert(s.id.as_str(), n);
            graph.nodes.push(vizcraft_layout::Node { width: (b.x1 - b.x0).max(1.0), height: (b.y1 - b.y0).max(1.0) });
        }
    }
    for c in &page.connectors {
        let (Some(a), Some(b)) = (c.start.shape().and_then(|s| node_of.get(s)), c.end.shape().and_then(|s| node_of.get(s))) else { continue };
        if a != b {
            graph.edges.push(vizcraft_layout::Edge::new(*a, *b));
        }
    }
    let origin = if ids.is_some() {
        chosen
            .iter()
            .filter_map(|&i| page.shapes.get(i))
            .map(|s| s.bounds())
            .fold(None, |acc: Option<Point>, b| Some(acc.map_or(Point::new(b.x0, b.y0), |p| Point::new(p.x.min(b.x0), p.y.min(b.y0)))))
    } else {
        None
    }
    .unwrap_or(Point::new(PAGE_MARGIN, PAGE_MARGIN));
    let out = vizcraft_layout::layout(&graph, opts).map_err(|e| e.to_string())?;
    let mut moved: HashSet<String> = HashSet::new();
    for (n, &i) in chosen.iter().enumerate() {
        let (Some(pos), Some(s)) = (out.nodes.get(n), page.shapes.get_mut(i)) else { continue };
        let b = s.bounds();
        // Move the frame so its bounding box (which differs from the frame when rotated) lands on
        // the layout position.
        s.x += origin.x + pos.x - b.x0;
        s.y += origin.y + pos.y - b.y0;
        s.sanitize();
        moved.insert(s.id.clone());
    }
    for c in &mut page.connectors {
        if c.start.shape().is_some_and(|s| moved.contains(s)) || c.end.shape().is_some_and(|s| moved.contains(s)) {
            c.waypoints.clear();
        }
    }
    Ok(out.notes)
}
