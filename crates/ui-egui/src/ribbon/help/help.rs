//! Help > Help: the command list and About.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Help tab's Help group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Help", vec![Big { id: "ui.commands", icon: "commands" }, Big { id: "help.about", icon: "info" }])
}
