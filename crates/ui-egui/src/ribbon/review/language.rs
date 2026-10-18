//! Review > Language: the Proofing Language menu. A language applies to the selected shapes; with
//! nothing selected it becomes the document's default. The row for the language in force is
//! checked.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// The language in force for the selection, else for the document, and whether the selection
/// sets it itself.
fn current(app: &CraftApp) -> (String, bool) {
    let doc = &app.session.doc;
    let own = doc.page().and_then(|p| app.session.sel.primary.as_deref().and_then(|id| p.shape(id))).map(|sh| sh.language.clone());
    match own {
        Some(own) => (vizcraft_engine::proof::effective_language(&doc.language, &own).to_string(), !own.is_empty()),
        None => (vizcraft_engine::proof::effective_language(&doc.language, "").to_string(), true),
    }
}

fn is(app: &CraftApp, tag: &str) -> bool {
    current(app).0.eq_ignore_ascii_case(tag)
}

fn entry(label: &'static str, tag: &'static str) -> MenuEntry {
    MenuEntry::new(label, "proof.setLanguage", json!({"language": tag}))
}

/// The Review tab's Language group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Language",
        vec![Menu {
            label: "Proofing Language",
            icon: "language",
            big: true,
            menu: vec![
                entry("English (United States)", "en-US").checked(|a| is(a, "en-US")),
                entry("English (United Kingdom)", "en-GB").checked(|a| is(a, "en-GB")),
                entry("French (France)", "fr-FR").checked(|a| is(a, "fr-FR")),
                entry("German (Germany)", "de-DE").checked(|a| is(a, "de-DE")),
                entry("Spanish (Spain)", "es-ES").checked(|a| is(a, "es-ES")),
                entry("Do Not Check Spelling", "none").checked(|a| is(a, "none")),
                entry("Use the Document Language", "default").checked(|a| !current(a).1),
            ],
        }],
    )
}
