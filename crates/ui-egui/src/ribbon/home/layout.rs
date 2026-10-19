//! Home > Layout (engineering workspace): the Design tab's Layout group, so re-laying out the
//! page in its notation's layout is one click from Home.

use crate::ribbon::Group;

/// The engineering Home tab's Layout group.
pub(in crate::ribbon) fn group() -> Group {
    crate::ribbon::design::layout_group()
}
