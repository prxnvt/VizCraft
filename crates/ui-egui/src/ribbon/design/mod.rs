//! The Design tab: page setup, themes and variants, page backgrounds and borders, page layout
//! and connector routing.

use super::{Group, Tab};

groups! {
    page_setup,
    themes,
    variants,
    backgrounds,
    layout,
    connectors,
}

/// The Design tab.
pub(super) fn tab() -> Tab {
    Tab::new("Design", groups())
}

/// The Layout group, for the engineering Home tab (which shows the same Re-Layout menu).
pub(in crate::ribbon) fn layout_group() -> Group {
    layout::group()
}
