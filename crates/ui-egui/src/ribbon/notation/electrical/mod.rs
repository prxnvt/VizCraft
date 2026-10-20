//! The Electrical groups of the notation tab, shown after the generic notation groups while
//! the active page's pack is `electrical`: Standard (symbol set and designation format), Ladder
//! (the Ladder tool and Number Wires), Lines (the wire, bus and signal lines the Wire tool draws),
//! Devices (link to coil, terminal strip, cable, I/O, footprints) and Lists (wire, cable,
//! terminal and I/O lists in the Tables dock).

mod devices;
mod ladder;
mod lists;
mod standard;

use crate::ribbon::Group;

/// The Electrical groups, left to right.
pub(in crate::ribbon) fn groups() -> Vec<Group> {
    vec![standard::group(), ladder::group(), devices::group(), lists::group()]
}

/// The generic notation groups these replace on an electrical page.
pub(in crate::ribbon) const HIDES: &[&str] = &["Steps", "Key"];
