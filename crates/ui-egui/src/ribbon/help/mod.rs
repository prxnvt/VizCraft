//! The Help tab: the command list, About and settings.

use super::Tab;

groups! {
    help,
    settings,
}

/// The Help tab.
pub(super) fn tab() -> Tab {
    Tab::new("Help", groups())
}
