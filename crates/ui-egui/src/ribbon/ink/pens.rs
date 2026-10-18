//! Ink > Pens: the pen's colour and thickness.

use crate::ribbon::Group;

/// The Ink tab's Pens group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Pens", super::format())
}
