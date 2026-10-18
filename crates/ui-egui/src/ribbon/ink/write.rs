//! Ink > Write: the pens, the eraser and lasso select.

use crate::ribbon::Group;

/// The Ink tab's Write group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Write", super::tools())
}
