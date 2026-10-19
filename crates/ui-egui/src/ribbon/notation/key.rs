//! Notation > Key: a generated key (legend) of what the page draws. Insert builds it, Refresh
//! rebuilds it in place after the drawing changed (the key is never refreshed on its own), Remove
//! takes it away. Each runs a `key.*` command.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The notation tab's Key group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Key",
        vec![Big { id: "key.insert", icon: "legend" }, Stack(vec![Small { id: "key.refresh", icon: "refresh" }, Small { id: "key.remove", icon: "delete" }])],
    )
}
