//! The contextual notation tab, named after the pack of the active page's viewpoint (Flowchart,
//! say) and shown while the page has a view. Its controls are built from the pack's data: one
//! Insert menu per palette group, the viewpoint menu and the Check button. A pack with groups of
//! its own ([`pack_groups`]) shows them after these while its viewpoint is active.

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

mod fluidpower;

/// The generic groups a notation pack leaves out of its tab, by label (fluid power has no key
/// and no numbered flow steps), so its own groups fit.
fn pack_hides(pack_id: &str) -> &'static [&'static str] {
    match pack_id {
        "fluidpower" => &["Key", "Steps"],
        _ => &[],
    }
}

/// The groups a notation pack adds to the tab after the generic ones, by pack id.
pub(in crate::ribbon) fn pack_groups(pack_id: &str) -> Vec<crate::ribbon::Group> {
    match pack_id {
        "fluidpower" => fluidpower::groups(),
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
    let hidden = pack.map(|p| pack_hides(&p.id)).unwrap_or_default();
    all.retain(|g| !hidden.contains(&g.label));
    all.extend(pack.map(|p| pack_groups(&p.id)).unwrap_or_default());
    Tab::contextual(name, has_view, all)
}
