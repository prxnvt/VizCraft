//! Home > Clipboard: paste, cut and copy.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Home tab's Clipboard group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Clipboard",
        vec![Big { id: "edit.paste", icon: "paste" }, Stack(vec![Small { id: "edit.cut", icon: "cut" }, Small { id: "edit.copy", icon: "copy" }])],
    )
}
