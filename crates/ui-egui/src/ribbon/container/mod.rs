//! The contextual Container tab, shown while a container, lane set or lane is selected: styles,
//! heading, size, membership and swimlanes.

use vizcraft_diagram::container::{self as ct, Container};

use super::Tab;
use crate::CraftApp;
use crate::ribbon::selected_shape;

pub(in crate::ribbon) use styles::grid;

groups! {
    styles,
    heading,
    size,
    membership,
    swimlanes,
}

/// The selected container's settings (the primary selected shape first).
pub(in crate::ribbon) fn selected_container(app: &CraftApp) -> Option<&Container> {
    selected_shape(app).filter(|s| ct::is_container(s)).and_then(|s| s.container.as_ref())
}

fn has_container_selected(app: &CraftApp) -> bool {
    selected_container(app).is_some()
}

/// The Container tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Container", has_container_selected, groups())
}
