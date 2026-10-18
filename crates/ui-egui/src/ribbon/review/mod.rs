//! The Review tab: comments.

use super::Tab;

groups! {
    comments,
}

/// The Review tab.
pub(super) fn tab() -> Tab {
    Tab::new("Review", groups())
}
