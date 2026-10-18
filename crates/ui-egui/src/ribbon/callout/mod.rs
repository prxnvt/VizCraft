//! The contextual Callout tab, shown while a callout is selected: its style, leader and the
//! side of the target the leader meets.

use vizcraft_diagram::container::{self as ct, Container};

use super::Tab;
use crate::CraftApp;
use crate::ribbon::selected_shape;

groups! {
    style,
    leader,
}

/// The selected callout's settings.
pub(in crate::ribbon) fn selected_callout(app: &CraftApp) -> Option<&Container> {
    selected_shape(app).filter(|s| ct::is_callout(s)).and_then(|s| s.container.as_ref())
}

fn has_callout_selected(app: &CraftApp) -> bool {
    selected_callout(app).is_some()
}

/// The Callout tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Callout", has_callout_selected, groups())
}
