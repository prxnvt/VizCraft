//! The Generate tab: preview, generate and check generated files, and pick the target.

use super::Tab;

groups! {
    generate,
    targets,
}

/// The Generate tab.
pub(super) fn tab() -> Tab {
    Tab::new("Generate", groups())
}
