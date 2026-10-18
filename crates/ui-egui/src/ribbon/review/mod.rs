//! The Review tab: no groups yet, so it is hidden.

use super::Tab;

groups! {}

/// The Review tab.
pub(super) fn tab() -> Tab {
    Tab::new("Review", groups())
}
