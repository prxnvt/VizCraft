//! Obstacle-avoiding connector routes, built on `vizcraft-routing`.
//!
//! Connectors with `avoid` set keep their bends in `waypoints`; [`refresh`] recomputes them after
//! every document change so they stay clear of the shapes. Routing is too costly to redo on every
//! frame, which is why the result is stored rather than derived like the other route styles.

use crate::model::{Document, Page};

/// Recompute the stored bends of every connector that avoids shapes, on every page. The engine
/// calls this after each command that changes the document, inside the same undo step.
pub fn refresh(_doc: &mut Document) {
    // Contract stub for Cascade 3: the routing piece fills this in.
}

/// Route the connectors named by `ids` (every connector on the page when `None`) around the
/// page's shapes now, storing the bends in their `waypoints`.
pub fn reroute(_page: &mut Page, _ids: Option<&[String]>) {
    // Contract stub for Cascade 3: the routing piece fills this in.
}
