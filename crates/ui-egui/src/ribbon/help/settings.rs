//! Help > Settings: preferences.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Help tab's Settings group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Settings", vec![Big { id: "app.preferences", icon: "settings" }])
}
