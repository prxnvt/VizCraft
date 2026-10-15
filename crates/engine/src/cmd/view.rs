//! View menu and status bar: zoom, dark mode, panes.

use serde_json::{Value, json};

use crate::{CmdResult, CommandSpec, EngineError, Session, ViewState, p};

/// Zoom limits, as a percentage.
pub const MIN_ZOOM: f32 = 25.0;
pub const MAX_ZOOM: f32 = 400.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("view.zoomIn", "Zoom In", "View > Zoom", |s, _| {
            let z = (s.view.zoom * 10.0).round() / 10.0 + 0.1;
            zoom(s, &json!({"value": z * 100.0}))
        })
        .key("Mod+="),
        CommandSpec::new("view.zoomOut", "Zoom Out", "View > Zoom", |s, _| {
            let z = (s.view.zoom * 10.0).round() / 10.0 - 0.1;
            zoom(s, &json!({"value": z * 100.0}))
        })
        .key("Mod+-"),
        CommandSpec::new("view.zoom", "Zoom", "View > Zoom", zoom).params(r#"{"value"?: percent}"#),
        CommandSpec::new("view.zoom100", "100%", "View > Zoom", |s, _| zoom(s, &json!({"value": 100}))).key("Mod+0"),
        CommandSpec::new("view.dark", "Dark Mode", "View > Appearance", |s, v| toggle(s, v, |x| &mut x.dark_mode)).params(r#"{"value"?: bool}"#),
        CommandSpec::new("view.navigationPane", "Navigation Pane", "View > Show", |s, v| toggle(s, v, |x| &mut x.nav_pane))
            .params(r#"{"value"?: bool}"#),
        CommandSpec::new("view.propertiesPane", "Properties Pane", "View > Show", |s, v| toggle(s, v, |x| &mut x.props_pane))
            .params(r#"{"value"?: bool}"#),
    ]
}

/// Set a flag to `value`, or flip it when there is none.
fn toggle(s: &mut Session, v: &Value, f: fn(&mut ViewState) -> &mut bool) -> CmdResult {
    let flag = f(&mut s.view);
    *flag = p::bool(v, "value").unwrap_or(!*flag);
    Ok(serde_json::to_value(&s.view).unwrap_or_default())
}

fn zoom(s: &mut Session, v: &Value) -> CmdResult {
    match p::f32(v, "value") {
        Some(pct) => {
            // Out-of-range asks are clamped rather than refused: Zoom In at the limit is a no-op,
            // not an error in the status bar.
            s.view.zoom = pct.clamp(MIN_ZOOM, MAX_ZOOM) / 100.0;
        }
        None if v.get("value").is_some() => return Err(EngineError::Params("zoom `value` must be a percentage".into())),
        None => s.ui_requests.push(json!({"open": "zoom"})),
    }
    Ok(json!({"zoom": s.view.zoom}))
}
