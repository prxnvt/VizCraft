//! The Design tab: page setup, page layout and connector routing.

use super::Tab;

groups! {
    page_setup,
    layout,
    connectors,
}

/// The Design tab.
pub(super) fn tab() -> Tab {
    Tab::new("Design", groups())
}
