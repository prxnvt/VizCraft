//! The Home tab: clipboard, font, paragraph, tools, shape styles, arrange, editing and undo. With
//! the engineering workspace (`ui.preset`) it is arranged for modelling instead: clipboard,
//! text, quick insert, layout, check, arrange, quick styles and tools.

use super::{Control, Group, Tab};
use crate::CraftApp;

mod check;
mod layers;
mod layout;
mod quick_insert;
mod text;

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

/// The engineering arrangement, left to right. Editing and Undo stay on the quick access toolbar.
fn engineering_groups() -> Vec<Group> {
    vec![
        clipboard::group(),
        text::group(),
        quick_insert::group(),
        layout::group(),
        check::group(),
        arrange::group(),
        Group::new("Styles", vec![Control::Custom(shape_styles::quick_styles)]),
        tools::group(),
    ]
}

/// The Home tab, in the arrangement of the workspace preset (office, or none chosen yet, is
/// today's).
pub(super) fn tab(app: &CraftApp) -> Tab {
    Tab::new("Home", if crate::preset::engineering(app) { engineering_groups() } else { groups() })
}
