//! Sequence > Layout: Lay Out packs the lifelines from the left margin and redraws every row
//! (`sequence.layout`; the Inspector's Sequence group sets the row height and column gap).

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Sequence tab's Layout group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Layout", vec![Big { id: "sequence.layout", icon: "layout" }])
}
