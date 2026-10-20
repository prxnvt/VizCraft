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

pub(in crate::ribbon) mod mechanical;

/// The groups pack `pack_id` adds after the generic notation groups while it is active.
pub(in crate::ribbon) fn pack_groups(pack_id: &str) -> Vec<super::Group> {
    match pack_id {
        "mechanical" | "annotation" | "reliability" | "idef0" => mechanical::groups(pack_id),
        _ => Vec::new(),
    }
}

/// The pack of the active page's viewpoint.
pub(super) fn active_pack(app: &CraftApp) -> Option<&'static Pack> {
    let vp = crate::palette::active_viewpoint(app)?;
    vizcraft_notation::catalog().pack(vp.split_once('.').map_or("", |(p, _)| p)).filter(|p| p.viewpoint(vp).is_some())
}

pub(super) fn has_view(app: &CraftApp) -> bool {
    active_pack(app).is_some()
}

/// The notation tab: named after the active pack, or "Notation" while hidden.
pub(super) fn tab(app: &CraftApp) -> Tab {
    let pack = active_pack(app);
    let name = pack.map_or("Notation", |p| p.name.as_str());
    let mut all = groups();
    all.extend(pack_groups(pack.map_or("", |p| p.id.as_str())));
    Tab::contextual(name, has_view, all)
}
