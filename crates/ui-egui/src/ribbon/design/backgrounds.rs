//! Design > Backgrounds: page backgrounds, and borders with a title block.

use serde_json::json;
use vizcraft_diagram::decor::{self, BackgroundKind, BorderKind, Decoration};

use crate::CraftApp;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The decoration (background and border) of the active page.
fn decoration(app: &CraftApp) -> Decoration {
    decor::decoration(&app.session.doc, app.session.doc.active_index())
}

fn backgrounds() -> Vec<MenuEntry> {
    let entry = |label, kind: &str| MenuEntry::new(label, "design.setBackground", json!({"kind": kind}));
    vec![
        entry("No Background", "none").checked(|a| decoration(a).background == BackgroundKind::None),
        entry("Solid", "solid").checked(|a| decoration(a).background == BackgroundKind::Solid),
        entry("Gradient", "gradient").checked(|a| decoration(a).background == BackgroundKind::Gradient),
        entry("Grid Pattern", "pattern").checked(|a| decoration(a).background == BackgroundKind::Pattern),
    ]
}

fn borders() -> Vec<MenuEntry> {
    let entry = |label, kind: &str| MenuEntry::new(label, "design.setBorderTitle", json!({"kind": kind}));
    vec![
        entry("No Border", "none").checked(|a| decoration(a).border == BorderKind::None),
        entry("Frame", "frame").checked(|a| decoration(a).border == BorderKind::Frame),
        entry("Banner", "banner").checked(|a| decoration(a).border == BorderKind::Banner),
        entry("Margin", "margin").checked(|a| decoration(a).border == BorderKind::Margin),
    ]
}

/// The Design tab's Backgrounds group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Backgrounds",
        vec![
            Menu { label: "Backgrounds", icon: "fill", big: true, menu: backgrounds() },
            Menu { label: "Borders & Titles", icon: "shape", big: true, menu: borders() },
        ],
    )
}
