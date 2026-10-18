//! The Developer tab: Shape Design (shape operations, the shape name, behaviour and protection)
//! and Show/Hide (the Document Stencil pane and the Drawing Explorer).

use super::Tab;

groups! {
    shape_design,
    show_hide,
}

/// The Developer tab.
pub(super) fn tab() -> Tab {
    Tab::new("Developer", groups())
}
