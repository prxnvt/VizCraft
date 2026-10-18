//! The Design tab: page setup, themes and variants, page backgrounds and borders, page layout
//! and connector routing.

use super::Tab;

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
