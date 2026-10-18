//! The Home tab: clipboard, font, tools, shape styles, arrange, editing and undo.

use super::Tab;

mod layers;

groups! {
    clipboard,
    font,
    tools,
    shape_styles,
    arrange,
    editing,
    undo,
}

/// The Home tab.
pub(super) fn tab() -> Tab {
    Tab::new("Home", groups())
}
