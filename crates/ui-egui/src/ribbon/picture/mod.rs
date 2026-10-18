//! The contextual Picture tab, shown while a picture is selected: its adjustments, border and
//! crop.

use vizcraft_diagram::shape_fields::{Adjust, Recolor};

use super::Tab;
use crate::CraftApp;
use crate::ribbon::selected_shape;

groups! {
    adjust,
    styles,
    size,
}

/// The selected picture's adjustments.
pub(in crate::ribbon) fn adjust_of(app: &CraftApp) -> Option<Adjust> {
    selected_shape(app).and_then(|s| s.image.as_ref()).map(|i| i.adjust)
}

/// Whether the selected picture's recolour is `r`.
pub(in crate::ribbon) fn recolor_is(app: &CraftApp, r: &str) -> bool {
    adjust_of(app).is_some_and(|a| Some(a.recolor) == Recolor::parse(r))
}

fn has_picture_selected(app: &CraftApp) -> bool {
    selected_shape(app).is_some_and(|s| s.image.is_some())
}

/// The Picture tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Picture", has_picture_selected, groups())
}
