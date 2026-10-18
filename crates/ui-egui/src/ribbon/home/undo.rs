//! Home > Undo: undo and redo.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Home tab's Undo group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Undo", vec![Big { id: "edit.undo", icon: "undo" }, Big { id: "edit.redo", icon: "redo" }])
}
