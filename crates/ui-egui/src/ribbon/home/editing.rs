//! Home > Editing: select all and the Layers menu.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Home tab's Editing group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Editing", vec![Big { id: "edit.selectAll", icon: "selectAll" }, super::layers::control()])
}
