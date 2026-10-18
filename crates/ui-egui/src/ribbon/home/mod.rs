//! The Home tab: clipboard, font, paragraph, tools, shape styles, arrange, editing and undo.

use super::Tab;

mod layers;

groups! {
    clipboard,
    font,
    paragraph,
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
