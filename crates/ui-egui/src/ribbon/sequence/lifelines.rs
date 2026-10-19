//! Sequence > Lifelines: a lifeline or an actor right of the selected one (`ui.sequenceLifeline`,
//! which then edits its name), and a note over the selected lifeline or beside the selected
//! message (`ui.sequenceNote`).

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

/// The Sequence tab's Lifelines group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Lifelines",
        vec![
            Button { id: "ui.sequenceLifeline", icon: "lifeline", label: "Lifeline", size: Size::Big, params: |_| json!({}), checked: None },
            Stack(vec![
                Button { id: "ui.sequenceLifeline", icon: "actorLifeline", label: "Actor", size: Size::Small, params: |_| json!({"actor": true}), checked: None },
                Small { id: "ui.sequenceNote", icon: "note" },
            ]),
        ],
    )
}
