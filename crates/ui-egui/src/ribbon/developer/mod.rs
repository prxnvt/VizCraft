//! The Developer tab: no groups yet, so it is hidden.

use super::Tab;

groups! {}

/// The Developer tab.
pub(super) fn tab() -> Tab {
    Tab::new("Developer", groups())
}
