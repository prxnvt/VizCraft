//! Home > Undo: undo and redo, a small stack of icon buttons (the label shows as the tooltip) so
//! the Home tab fits in 1280 px.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Home tab's Undo group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Undo",
        vec![Stack(vec![
            Button { id: "edit.undo", icon: "undo", label: "Undo", size: Size::Icon, params: |_| json!({}), checked: None },
            Button { id: "edit.redo", icon: "redo", label: "Redo", size: Size::Icon, params: |_| json!({}), checked: None },
        ])],
    )
}
