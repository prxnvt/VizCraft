//! The Data tab: no groups yet, so it is hidden.

use super::Tab;

groups! {}

/// The Data tab.
pub(super) fn tab() -> Tab {
    Tab::new("Data", groups())
}
