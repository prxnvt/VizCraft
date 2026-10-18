//! Insert > Pages: New Page, as a blank page (the main button), a background page or a copy of
//! the active page.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The Insert tab's Pages group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Pages",
        vec![Split {
            id: "page.add",
            icon: "new",
            big: true,
            menu: vec![
                MenuEntry::new("Blank Page", "page.add", json!({})),
                MenuEntry::new("Background Page", "page.add", json!({"background": true})),
                MenuEntry::new("Duplicate Page", "page.duplicate", json!({})),
            ],
        }],
    )
}
