//! Insert > Pages: New Page, as a blank page (the main button), a background page or a copy of
//! the active page.

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Insert tab's Pages group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Pages",
        vec![Split {
            id: "page.add",
            icon: "new",
            big: true,
            menu: vec![
                ("Blank Page", "page.add", json!({})),
                ("Background Page", "page.add", json!({"background": true})),
                ("Duplicate Page", "page.duplicate", json!({})),
            ],
        }],
    )
}
