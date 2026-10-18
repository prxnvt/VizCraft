//! The Model tab: work on the model behind the diagram, not on one drawing of it.

use super::Tab;

groups! {
    model,
}

/// The Model tab.
pub(super) fn tab() -> Tab {
    Tab::new("Model", groups())
}
