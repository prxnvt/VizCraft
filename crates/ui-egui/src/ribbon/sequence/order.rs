//! Sequence > Order: the selected message, note or fragment one step up or down, and the selected
//! lifeline one column left or right (`sequence.moveUp`, `moveDown`, `moveLeft`, `moveRight`;
//! Alt+arrows).

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Sequence tab's Order group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Order",
        vec![
            Stack(vec![Small { id: "sequence.moveUp", icon: "chevronUp" }, Small { id: "sequence.moveDown", icon: "chevronDown" }]),
            Stack(vec![Small { id: "sequence.moveLeft", icon: "chevronLeft" }, Small { id: "sequence.moveRight", icon: "chevronRight" }]),
        ],
    )
}
