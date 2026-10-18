//! The Insert tab: stencil galleries and the connector and text tools.

use super::Tab;

groups! {
    basic_shapes,
    flowchart,
    diagram_parts,
}

/// The Insert tab.
pub(super) fn tab() -> Tab {
    Tab::new("Insert", groups())
}
