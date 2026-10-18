//! Insert > Diagram Parts: the connector and text tools, containers (with their style gallery),
//! callouts and swimlanes as big buttons.

use serde_json::json;
use vizcraft_diagram::container::{CALLOUT_STYLES, CONTAINER_STYLES};

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

/// One menu entry per preset: `id` with `{"style": name}`.
fn styles(id: &'static str, presets: &'static [vizcraft_diagram::container::Preset]) -> Vec<MenuEntry> {
    presets.iter().map(|p| (p.label, id, json!({"style": p.name}))).collect()
}

/// The Insert tab's Diagram Parts group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Diagram Parts",
        vec![
            Button { id: "ui.tool", icon: "connector", label: "Connector", size: Size::Big, params: |_| json!({"tool": "connector"}), checked: None },
            Button { id: "ui.tool", icon: "text", label: "Text", size: Size::Big, params: |_| json!({"tool": "text"}), checked: None },
            // Around the selection, else an empty one in the middle of the page.
            Split { id: "container.add", icon: "container", big: true, menu: styles("container.add", CONTAINER_STYLES) },
            Split { id: "callout.add", icon: "callout", big: true, menu: styles("callout.add", CALLOUT_STYLES) },
            Split {
                id: "lane.addSet",
                icon: "swimlanes",
                big: true,
                menu: vec![
                    ("Horizontal Lanes", "lane.addSet", json!({"orientation": "horizontal"})),
                    ("Vertical Lanes", "lane.addSet", json!({"orientation": "vertical"})),
                    ("Two Lanes", "lane.addSet", json!({"lanes": 2})),
                    ("Four Lanes", "lane.addSet", json!({"lanes": 4})),
                ],
            },
        ],
    )
}
