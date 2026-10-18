//! Review > Proofing: the Spelling dialog.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Review tab's Proofing group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Proofing", vec![Big { id: "ui.spelling", icon: "spelling" }])
}
