//! The contextual notation tab, named after the pack of the active page's viewpoint (Flowchart,
//! say) and shown while the page has a view. Its controls are built from the pack's data: one
//! Insert menu per palette group, the viewpoint menu and the Check button.

use vizcraft_notation::Pack;

use super::Tab;
use crate::CraftApp;

groups! {
    insert,
    viewpoint,
    variant,
    check,
    key,
    steps,
}

/// The pack of the active page's viewpoint.
pub(super) fn active_pack(app: &CraftApp) -> Option<&'static Pack> {
    let vp = crate::palette::active_viewpoint(app)?;
    vizcraft_notation::catalog().pack(vp.split_once('.').map_or("", |(p, _)| p)).filter(|p| p.viewpoint(vp).is_some())
}

fn has_view(app: &CraftApp) -> bool {
    active_pack(app).is_some()
}

/// The notation tab: named after the active pack, or "Notation" while hidden.
pub(super) fn tab(app: &CraftApp) -> Tab {
    let name = active_pack(app).map_or("Notation", |p| p.name.as_str());
    Tab::contextual(name, has_view, groups())
}
