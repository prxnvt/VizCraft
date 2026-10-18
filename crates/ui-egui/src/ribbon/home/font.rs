//! Home > Font: size, bold, italic and colour of the selected text.

use serde_json::{Value, json};
use vizcraft_diagram::Style;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{FONT_SIZES, Group, Size, color32, selected_style, short_num};

/// The selected style's flag flipped, so the toggle turns it off when every selected shape
/// shows it and on otherwise. With nothing selected the engine answers "select a shape first".
fn flip(app: &CraftApp, key: &str, get: fn(&Style) -> bool) -> Value {
    json!({ key: !selected_style(app).is_some_and(get) })
}

/// The Home tab's Font group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Font",
        vec![Stack(vec![
            Combo {
                id: "shape.setStyle",
                param: "fontSize",
                name: "Font Size",
                width: 72.0,
                items: |_| FONT_SIZES.iter().map(|s| format!("{s} pt")).collect(),
                current: |a| selected_style(a).map(|s| format!("{} pt", short_num(s.font_size))).unwrap_or_default(),
            },
            Row(vec![
                Button {
                    id: "shape.setStyle",
                    icon: "bold",
                    label: "Bold",
                    size: Size::Icon,
                    params: |a| flip(a, "bold", |s| s.bold),
                    checked: Some(|a| selected_style(a).is_some_and(|s| s.bold)),
                },
                Button {
                    id: "shape.setStyle",
                    icon: "italic",
                    label: "Italic",
                    size: Size::Icon,
                    params: |a| flip(a, "italic", |s| s.italic),
                    checked: Some(|a| selected_style(a).is_some_and(|s| s.italic)),
                },
                Colors {
                    id: "shape.setStyle",
                    param: "fontColor",
                    icon: "fontColor",
                    name: "Font Color",
                    show_label: false,
                    none: None,
                    current: |a| selected_style(a).map(|s| color32(s.font_color)),
                },
            ]),
        ])],
    )
}
