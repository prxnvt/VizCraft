//! View > Views: presentation mode.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The View tab's Views group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Views",
        vec![Button { id: "ui.present", icon: "present", label: "Presentation", size: Size::Icon, params: |_| json!({}), checked: Some(|a| a.presenting) }],
    )
}
