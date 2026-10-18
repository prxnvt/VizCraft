//! The Insert tab: pages, stencil galleries and the connector and text tools.

use super::Tab;

groups! {
    pages,
    links,
    basic_shapes,
    flowchart,
    diagram_parts,
    text,
}

/// The Insert tab.
pub(super) fn tab() -> Tab {
    Tab::new("Insert", groups())
}
