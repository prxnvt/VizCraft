//! Design > Backgrounds: page backgrounds, and borders with a title block.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

fn backgrounds() -> Vec<MenuEntry> {
    [("No Background", "none"), ("Solid", "solid"), ("Gradient", "gradient"), ("Grid Pattern", "pattern")]
        .into_iter()
        .map(|(label, kind)| (label, "design.setBackground", json!({"kind": kind})))
        .collect()
}

fn borders() -> Vec<MenuEntry> {
    [("No Border", "none"), ("Frame", "frame"), ("Banner", "banner"), ("Margin", "margin")]
        .into_iter()
        .map(|(label, kind)| (label, "design.setBorderTitle", json!({"kind": kind})))
        .collect()
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
