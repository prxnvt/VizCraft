//! Insert > Basic Shapes: the basic stencil as a gallery.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Insert tab's Basic Shapes group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Basic Shapes", vec![Gallery { set: "basic", rows: 3 }])
}
