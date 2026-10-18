//! Process > Diagram Validation: check the whole document, choose the rules it is held to, switch
//! live checking, and step through the issues. Every control runs a `validate.*` command.

use serde_json::json;
use vizcraft_diagram::Conformance;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item};

/// The conformance profile in effect for the document (what `validate.setProfile` with no params
/// returns). Worked out once per document revision and kept on the app: the Live Checking toggle
/// asks every frame and the answer walks the model.
fn profile(app: &CraftApp) -> Conformance {
    let catalog = vizcraft_notation::catalog();
    let key = (app.session.rev(), std::ptr::from_ref(catalog) as usize);
    if let Some((rev, cat, p)) = app.profile_cache.borrow().as_ref()
        && (*rev, *cat) == key
    {
        return p.clone();
    }
    let p = vizcraft_notation::rules::profile(&app.session.doc, catalog);
    *app.profile_cache.borrow_mut() = Some((key.0, key.1, p.clone()));
    p
}

fn live_on(app: &CraftApp) -> bool {
    profile(app).enabled
}

/// The Rules menu: Off, then one row per ruleset of the catalog (built-in and loaded packs), the
/// ones the document is held to checked. Drawn by hand because the rows are the catalog's, which
/// loading a pack changes.
fn rules_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "rules", "Rules", "Rules", false, |ui, app| {
        let p = profile(app);
        menu_item(ui, app, "Off", "validate.setProfile", json!({"enabled": false}), Some(!p.enabled));
        ui.separator();
        for (_, r) in vizcraft_notation::catalog().rulesets() {
            let on = p.enabled && p.rulesets.contains(&r.id);
            menu_item(ui, app, &r.label, "validate.setProfile", json!({"rulesets": [r.id]}), Some(on));
        }
    });
}

/// The Process tab's Diagram Validation group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Diagram Validation",
        vec![
            Big { id: "validate.run", icon: "check" },
            Stack(vec![
                Custom(rules_menu),
                Button {
                    id: "validate.setProfile",
                    icon: "liveChecking",
                    label: "Live Checking",
                    size: Size::Small,
                    params: |a| json!({"enabled": !live_on(a)}),
                    checked: Some(live_on),
                },
            ]),
            Stack(vec![Small { id: "validate.next", icon: "nextIssue" }, Small { id: "validate.previous", icon: "prevIssue" }]),
        ],
    )
}
