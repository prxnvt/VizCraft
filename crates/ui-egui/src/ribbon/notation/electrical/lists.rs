//! Electrical > Lists: the wire list (one row per wire segment, from and to as device and pin)
//! and the cable, terminal and I/O lists, each opening the Tables dock on it (`ui.tables`), where
//! Export writes CSV, XLSX or Markdown; and the page's key, which on an electrical page lives here
//! instead of in a Key group of its own.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

fn list(label: &'static str, params: serde_json::Value) -> MenuEntry {
    MenuEntry { label, id: "ui.tables", params, checked: None }
}

/// The Lists group.
pub(super) fn group() -> Group {
    Group::new(
        "Lists",
        vec![Stack(vec![
            Menu {
                label: "Lists",
                icon: "wireList",
                big: false,
                menu: vec![
                    list("Wire List", json!({"view": "list", "subject": "connections", "kind": "wire"})),
                    list("Cable List", json!({"view": "list", "type": "electrical.Cable"})),
                    list("Terminal Plan", json!({"view": "list", "type": "electrical.Terminal"})),
                    list("I/O List", json!({"view": "list", "type": "electrical.IoPoint"})),
                ],
            },
            Small { id: "key.insert", icon: "legend" },
            Small { id: "key.refresh", icon: "refresh" },
        ])],
    )
}
