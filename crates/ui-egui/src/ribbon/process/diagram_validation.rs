//! Process > Diagram Validation: check the whole document against its rules.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Process tab's Diagram Validation group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Diagram Validation", vec![Big { id: "validate.run", icon: "check" }])
}
