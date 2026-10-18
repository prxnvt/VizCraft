//! The Review tab: proofing, language and shape reports.

use super::Tab;

groups! {
    proofing,
    language,
    reporting,
}

/// The Review tab.
pub(super) fn tab() -> Tab {
    Tab::new("Review", groups())
}
