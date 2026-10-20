//! The contextual notation tab, named after the pack of the active page's viewpoint (Flowchart,
//! say) and shown while the page has a view. Its controls are built from the pack's data: one
//! Insert menu per palette group, the viewpoint menu and the Check button.

use vizcraft_notation::Pack;

use super::{Group, Tab};
use crate::CraftApp;

mod process;

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

pub(super) fn has_view(app: &CraftApp) -> bool {
    active_pack(app).is_some()
}

/// The groups particular to one pack, shown after the generic notation groups while that pack's
/// page is active. Each wave 4 pack adds its own arm.
pub(super) fn pack_groups(pack_id: &str) -> Vec<Group> {
    match pack_id {
        "process" => process::groups(),
        _ => Vec::new(),
    }
}

/// The generic notation groups a pack's page leaves out to make room for its own.
pub(super) fn pack_hides(pack_id: &str) -> &'static [&'static str] {
    match pack_id {
        "process" => process::HIDES,
        _ => &[],
    }
}

/// The notation tab: named after the active pack, or "Notation" while hidden.
pub(super) fn tab(app: &CraftApp) -> Tab {
    let pack = active_pack(app);
    let name = pack.map_or("Notation", |p| p.name.as_str());
    let mut groups = groups();
    if let Some(p) = pack {
        let hides = pack_hides(&p.id);
        groups.retain(|g| !hides.contains(&g.label));
        groups.extend(pack_groups(&p.id));
    }
    Tab::contextual(name, has_view, groups)
}
