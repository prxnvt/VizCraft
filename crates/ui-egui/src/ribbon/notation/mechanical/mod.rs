//! The groups the mechanical, annotation, reliability and IDEF0 packs add to the notation tab,
//! after its generic groups: Mechanism on mechanical pages, Reliability Analysis and FMEA on
//! reliability pages, Function Model on IDEF0 pages. The Sheet tab's Annotate group is here too
//! ([`annotate`]), since it inserts the annotation pack's symbols.

pub(in crate::ribbon) mod annotate;
mod idef0;
mod mech;
mod reliability;

use crate::ribbon::Group;

/// The groups of pack `pack_id` (one of the four this module serves).
pub(in crate::ribbon) fn groups(pack_id: &str) -> Vec<Group> {
    match pack_id {
        "mechanical" => vec![mech::group()],
        "reliability" => vec![reliability::analyze(), reliability::fmea()],
        "idef0" => vec![idef0::group()],
        // Annotation pages use the Sheet tab's Annotate group; a second copy here would show the
        // same group twice on screen.
        _ => Vec::new(),
    }
}
