//! Notation > Check: the full validation group, so a page with a notation tab validates from that
//! tab: Check (`validate.run`), the rules to hold the page to, Live Checking, the Problems tab and
//! Next and Previous issue. The Process tab shows the same controls only on pages without a
//! notation tab.

use crate::ribbon::Group;

/// The notation tab's Check group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Check", crate::ribbon::process_validation_controls())
}
