//! Home > Font: family, size and size steps, bold, italic, underline, strikethrough, letter case,
//! highlight and colour of the selected text.

use serde_json::{Value, json};
use vizcraft_diagram::{Style, TextCase};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{FONT_SIZES, Group, MenuEntry, Size, color32, selected_style, short_num};

/// Families offered by the font field. Inter and JetBrains Mono draw on the canvas; other names
/// are kept for exports and draw in the nearest shipped face.
pub const FONT_FAMILIES: &[&str] = &["Inter", "JetBrains Mono", "Arial", "Helvetica", "Georgia", "Times New Roman", "Verdana", "Courier New"];

/// The selected style's flag flipped, so the toggle turns it off when every selected shape
/// shows it and on otherwise. With nothing selected the engine answers "select a shape first".
fn flip(app: &CraftApp, key: &str, get: fn(&Style) -> bool) -> Value {
    json!({ key: !selected_style(app).is_some_and(get) })
}

/// A toggle running `text.setFormat` for one flag.
fn format_toggle(icon: &'static str, label: &'static str, params: fn(&CraftApp) -> Value, checked: fn(&CraftApp) -> bool) -> crate::ribbon::Control {
    Button { id: "text.setFormat", icon, label, size: Size::Icon, params, checked: Some(checked) }
}

/// An icon-only dropdown in a ribbon row: `tip` names it, `menu` lists its entries.
fn icon_menu(app: &mut CraftApp, ui: &mut egui::Ui, icon: &str, tip: &str, menu: &[MenuEntry]) {
    crate::widgets::menu_button(ui, app, icon, "", tip, false, |ui, app| {
        for e in menu {
            let checked = e.checked.map(|f| f(app));
            crate::widgets::menu_item(ui, app, e.label, e.id, e.params.clone(), checked);
        }
    });
}

fn case_is(app: &CraftApp, case: TextCase) -> bool {
    selected_style(app).is_some_and(|s| s.text_case == case)
}

/// The Change Case menu.
fn case_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    // Sentence and toggle case rewrite the text once and leave no state to show; the other three
    // are a style the text keeps.
    let entries = [
        MenuEntry::new("Sentence case", "text.setCase", json!({"case": "sentence"})),
        MenuEntry::new("lower case", "text.setCase", json!({"case": "lower"})).checked(|a| case_is(a, TextCase::Lower)),
        MenuEntry::new("UPPER CASE", "text.setCase", json!({"case": "upper"})).checked(|a| case_is(a, TextCase::Upper)),
        MenuEntry::new("Capitalize Each Word", "text.setCase", json!({"case": "title"})).checked(|a| case_is(a, TextCase::Title)),
        MenuEntry::new("tOGGLE cASE", "text.setCase", json!({"case": "toggle"})),
    ];
    icon_menu(app, ui, "textCase", "Change Case", &entries);
}

/// The Home tab's Font group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Font",
        vec![Stack(vec![
            Row(vec![
                Combo {
                    id: "text.setFormat",
                    param: "fontFamily",
                    name: "Font",
                    width: 92.0,
                    items: |_| FONT_FAMILIES.iter().map(|f| f.to_string()).collect(),
                    current: |a| selected_style(a).map(|s| if s.font_family.is_empty() { "Inter".to_string() } else { s.font_family.clone() }).unwrap_or_default(),
                },
                Combo {
                    id: "text.setFormat",
                    param: "size",
                    name: "Font Size",
                    width: 52.0,
                    items: |_| FONT_SIZES.iter().map(|s| format!("{s} pt")).collect(),
                    current: |a| selected_style(a).map(|s| format!("{} pt", short_num(s.font_size))).unwrap_or_default(),
                },
            ]),
            Row(vec![
                format_toggle("bold", "Bold", |a| flip(a, "bold", |s| s.bold), |a| selected_style(a).is_some_and(|s| s.bold)),
                format_toggle("italic", "Italic", |a| flip(a, "italic", |s| s.italic), |a| selected_style(a).is_some_and(|s| s.italic)),
                format_toggle("underline", "Underline", |a| flip(a, "underline", |s| s.underline), |a| selected_style(a).is_some_and(|s| s.underline)),
                format_toggle("strikethrough", "Strikethrough", |a| flip(a, "strikethrough", |s| s.strikethrough), |a| selected_style(a).is_some_and(|s| s.strikethrough)),
                Button { id: "text.grow", icon: "fontGrow", label: "Increase Font Size", size: Size::Icon, params: |_| json!({"step": 1}), checked: None },
                Button { id: "text.grow", icon: "fontShrink", label: "Decrease Font Size", size: Size::Icon, params: |_| json!({"step": -1}), checked: None },
            ]),
            Row(vec![
                Custom(case_menu),
                Colors {
                    id: "text.setFormat",
                    param: "highlight",
                    icon: "highlight",
                    name: "Text Highlight Color",
                    show_label: false,
                    none: Some("No Highlight"),
                    current: |a| selected_style(a).map(|s| color32(s.highlight)),
                },
                Colors {
                    id: "text.setFormat",
                    param: "color",
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
