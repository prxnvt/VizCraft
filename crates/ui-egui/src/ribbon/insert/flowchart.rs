//! Insert > Flowchart: the flowchart stencil as a gallery.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Insert tab's Flowchart group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Flowchart", vec![Gallery { set: "flowchart", rows: 3 }])
}
