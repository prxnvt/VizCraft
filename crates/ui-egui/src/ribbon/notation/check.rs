//! Notation > Check: validate the document against the pack's rules (`validate.run`).

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The notation tab's Check group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Check", vec![Big { id: "validate.run", icon: "check" }])
}
