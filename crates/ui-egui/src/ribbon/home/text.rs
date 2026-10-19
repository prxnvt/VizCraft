//! Home > Text (engineering workspace): the font and size, bold, italic and underline, and the
//! horizontal alignment, reusing the Font and Paragraph controls, with a launcher that opens the
//! Inspector's Format tab for everything else.

use serde_json::json;

use super::{font, paragraph};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The engineering Home tab's Text group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Text",
        vec![Stack(vec![
            Row(vec![font::family_combo(), font::size_combo()]),
            Row(vec![font::bold(), font::italic(), font::underline()]),
            Row(vec![
                paragraph::align_left(),
                paragraph::align_center(),
                paragraph::align_right(),
                Button {
                    id: "ui.inspector",
                    icon: "taskPanes",
                    label: "Text Format",
                    size: Size::Icon,
                    params: |_| json!({"tab": "format"}),
                    checked: None,
                },
            ]),
        ])],
    )
}
