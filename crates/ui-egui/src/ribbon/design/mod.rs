//! The Design tab: page layout and connector routing.

use super::Tab;

groups! {
    layout,
    connectors,
}

/// The Design tab.
pub(super) fn tab() -> Tab {
    Tab::new("Design", groups())
}
