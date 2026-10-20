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

mod electrical;

/// The groups a pack adds after the generic notation groups while it is the active page's pack.
/// One arm per pack.
pub(super) fn pack_groups(pack_id: &str) -> Vec<crate::ribbon::Group> {
    match pack_id {
        "electrical" => electrical::groups(),
        _ => Vec::new(),
    }
}

/// The generic notation groups a pack's own groups replace (by label), so its tab fits. One arm
/// per pack that needs it.
fn pack_hides(pack_id: &str) -> &'static [&'static str] {
    match pack_id {
        "electrical" => electrical::HIDES,
        _ => &[],
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
    if let Some(p) = pack {
        let hides = pack_hides(&p.id);
        all.retain(|g| !hides.contains(&g.label));
        all.extend(pack_groups(&p.id));
    }
    Tab::contextual(name, has_view, all)
}
