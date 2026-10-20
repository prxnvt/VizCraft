//! The fluid power groups of the notation tab, shown after the generic ones while the page's
//! viewpoint is a `fluidpower` one: Symbols (the valve dialog, pressure valves, power unit),
//! Lines (the Pipe tool armed with a line type), Codes (renumber, code form, port labels,
//! component list), Sequence (the displacement-step chart) and Analyze (the pressure trace).
//! Every control runs a `fluid.*` command or `ui.tool` by id.

mod analyze;
mod codes;
mod lines;
mod sequence;
mod symbols;

use crate::ribbon::Group;

/// The fluid power groups, left to right.
pub(in crate::ribbon) fn groups() -> Vec<Group> {
    vec![symbols::group(), lines::group(), codes::group(), sequence::group(), analyze::group()]
}
