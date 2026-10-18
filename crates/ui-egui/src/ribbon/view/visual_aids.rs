//! View > Visual Aids: AutoConnect, snapping, and the Snap and Glue dialog.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The View tab's Visual Aids group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Visual Aids",
        vec![Stack(vec![
            Toggle { id: "view.autoConnect", icon: "autoConnect", checked: |a| a.session.view.aids.auto_connect },
            Toggle { id: "view.snap", icon: "snap", checked: |a| a.session.view.aids.snap },
            Small { id: "view.snapSettings", icon: "settings" },
        ])],
    )
}
