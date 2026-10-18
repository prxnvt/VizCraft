//! Insert > Links: add a link to the selected shape (the Link dialog), follow its first link, and
//! remove it. Screen tips are set in the dialog.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Insert tab's Links group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Links",
        vec![
            Big { id: "link.add", icon: "link" },
            Stack(vec![Small { id: "link.follow", icon: "link" }, Small { id: "link.remove", icon: "close" }]),
        ],
    )
}
