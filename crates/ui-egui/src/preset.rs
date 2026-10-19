//! Workspace presets (`ui.preset`): a named set of workspace defaults applied once, each of them
//! still the person's to change afterwards.
//!
//! - **Engineering**: rulers off, the Model Tree open at the top of the left side above Shapes,
//!   the Inspector open on the right, and the Home tab in its engineering arrangement (text,
//!   quick insert, layout and check groups; see `ribbon/home`).
//! - **Office**: rulers on, the left side's order and folding as a fresh install has them, and
//!   the Home tab as it always was.
//!
//! An unset preset (`UiState.preset` empty) draws the office look, so existing preferences and
//! every existing snapshot stay as they were. The first New page offers the choice (see
//! [`crate::gallery`]); nothing makes one of them the silent default.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::panes::{PaneLayout, Side};

/// The engineering workspace.
pub const ENGINEERING: &str = "engineering";

/// The office workspace (today's look).
pub const OFFICE: &str = "office";

/// The presets `ui.preset` takes.
pub const PRESETS: &[&str] = &[ENGINEERING, OFFICE];

/// The workspace a gallery audience suggests: engineering for the software, engineering and
/// industrial audiences, office for every other.
pub fn suggested(audience: &str) -> &'static str {
    if ["software", "engineering", "industrial"].iter().any(|a| a.eq_ignore_ascii_case(audience)) { ENGINEERING } else { OFFICE }
}

/// Whether the engineering workspace is on.
pub fn engineering(app: &CraftApp) -> bool {
    app.ui.preset == ENGINEERING
}

/// `ui.preset {name?}`: with no name (or a name that is not text) it reads the preset; with
/// `engineering` or `office` it stores it and applies its defaults once; any other name is an
/// error and changes nothing.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "ui.preset" {
        return None;
    }
    let Some(name) = p.get("name").and_then(Value::as_str) else {
        return Some(Ok(json!({"preset": app.ui.preset, "applied": []})));
    };
    if !PRESETS.contains(&name) {
        let shown: String = name.chars().take(64).collect();
        return Some(Err(format!("unknown workspace `{shown}`; ui.preset takes engineering or office")));
    }
    Some(Ok(apply(app, name)))
}

/// Store preset `name` (one of [`PRESETS`]) and apply its defaults. Returns the `ui.preset` result.
fn apply(app: &mut CraftApp, name: &str) -> Value {
    app.ui.preset = name.to_string();
    let engineering = name == ENGINEERING;
    let _ = app.run("view.rulers", json!({"value": !engineering}));
    let panes = &mut app.ui.panes;
    if engineering {
        // The Model Tree first on the left, unfolded; the Inspector on the right.
        panes.open("navigation", Some(Side::Left));
        panes.left.retain(|k| k != "navigation");
        panes.left.insert(0, "navigation".into());
        panes.collapsed.retain(|k| k != "navigation");
        panes.left_collapsed = false;
        panes.open("shapeData", Some(Side::Right));
        app.ui.tab = "Home".into();
    } else {
        // The default left side, in its order and folding; panes the person added there stay
        // below it.
        let fresh = PaneLayout::default();
        for k in &fresh.left {
            panes.open(k, Some(Side::Left));
        }
        let mut left: Vec<String> = fresh.left.clone();
        left.extend(panes.left.iter().filter(|k| !fresh.left.contains(k)).cloned());
        panes.left = left;
        for k in &fresh.left {
            panes.collapsed.retain(|c| c != k);
            if fresh.collapsed.contains(k) {
                panes.collapsed.push(k.clone());
            }
        }
        panes.left_collapsed = fresh.left_collapsed;
    }
    app.ui.panes.sanitize();
    let applied: &[&str] = if engineering { &["aids.rulers", "panes.navigation", "panes.shapeData", "tab"] } else { &["aids.rulers", "panes.left"] };
    json!({"preset": app.ui.preset, "applied": applied})
}
