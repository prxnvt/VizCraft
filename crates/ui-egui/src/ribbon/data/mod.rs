//! The Data tab: shape data.

use super::Tab;

groups! {
    show_hide,
}

/// The Data tab.
pub(super) fn tab() -> Tab {
    Tab::new("Data", groups())
}
