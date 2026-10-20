//! IDEF0 > Function Model: Decompose the selected box into a child diagram (`idef0.decompose`),
//! Renumber Boxes in staircase order (`idef0.renumber`) and recompute the boundary ICOM Codes
//! (`idef0.icom`).

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Function Model group.
pub(super) fn group() -> Group {
    Group::new(
        "Function Model",
        vec![
            Big { id: "idef0.decompose", icon: "decompose" },
            Stack(vec![Small { id: "idef0.renumber", icon: "renumberBoxes" }, Small { id: "idef0.icom", icon: "icomCodes" }]),
        ],
    )
}
