//! Insert > Illustrations: a picture from a file.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Insert tab's Illustrations group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Illustrations", vec![Big { id: "ui.insertPicture", icon: "picture" }])
}
