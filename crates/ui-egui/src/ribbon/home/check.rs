//! Home > Check (engineering workspace): validate the model against its rules and open the
//! Problems tab of the dock.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The engineering Home tab's Check group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Check",
        vec![
            Big { id: "validate.run", icon: "check" },
            Button { id: "ui.dock", icon: "problems", label: "Problems", size: Size::Big, params: |_| json!({"tab": "problems"}), checked: None },
        ],
    )
}
