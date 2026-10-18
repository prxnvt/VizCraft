//! The Process tab: diagram validation.

use super::Tab;

groups! {
    diagram_validation,
}

/// The Process tab.
pub(super) fn tab() -> Tab {
    Tab::new("Process", groups())
}
