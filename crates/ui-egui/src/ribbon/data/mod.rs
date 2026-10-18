//! The Data tab: shape data, external data and data graphics.

use super::Tab;

groups! {
    show_hide,
    external_data,
    display_data,
}

/// The Data tab.
pub(super) fn tab() -> Tab {
    Tab::new("Data", groups())
}
