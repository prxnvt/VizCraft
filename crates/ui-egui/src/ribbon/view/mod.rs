//! The View tab: zoom, panes and appearance.

use super::Tab;

groups! {
    zoom,
    show,
    appearance,
}

/// The View tab.
pub(super) fn tab() -> Tab {
    Tab::new("View", groups())
}
