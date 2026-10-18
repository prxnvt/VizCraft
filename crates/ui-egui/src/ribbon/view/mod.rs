//! The View tab: zoom, what the canvas shows, visual aids and appearance.

use super::Tab;

groups! {
    zoom,
    show,
    visual_aids,
    appearance,
}

/// The View tab.
pub(super) fn tab() -> Tab {
    Tab::new("View", groups())
}
