//! The Review tab: comments, proofing, language and shape reports.

use super::Tab;

groups! {
    comments,
    proofing,
    language,
    reporting,
}

/// The Review tab.
pub(super) fn tab() -> Tab {
    Tab::new("Review", groups())
}
