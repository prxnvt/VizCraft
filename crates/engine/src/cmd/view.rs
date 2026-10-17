//! View menu and status bar: zoom, dark mode, panes.

use serde_json::{Value, json};

use vizcraft_diagram::{self as dg, jumps};

use crate::{CmdResult, CommandSpec, EngineError, Session, ViewState, p};

/// Zoom limits, as a percentage.
pub const MIN_ZOOM: f32 = 25.0;
pub const MAX_ZOOM: f32 = 400.0;

/// Room left around a fitted page or drawing, in screen points, so its edge does not touch the
/// canvas edge. The same margin the canvas uses when it first centres a page.
pub const FIT_MARGIN: f32 = 24.0;

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
        CommandSpec::new("view.zoomFit", "Fit to Window", "View > Zoom", zoom_fit)
            .key("Mod+Shift+H")
            .params(r#"{"page"?: bool}"#)
            .when(|s| if s.view.canvas.is_some() { None } else { Some("No canvas is shown") }),
        CommandSpec::new("view.dark", "Dark Mode", "View > Appearance", |s, v| toggle(s, v, |x| &mut x.dark_mode)).params(r#"{"value"?: bool}"#),
        CommandSpec::new("view.navigationPane", "Navigation Pane", "View > Show", |s, v| toggle(s, v, |x| &mut x.nav_pane))
            .params(r#"{"value"?: bool}"#),
        CommandSpec::new("view.propertiesPane", "Properties Pane", "View > Show", |s, v| toggle(s, v, |x| &mut x.props_pane))
            .params(r#"{"value"?: bool}"#),
        CommandSpec::new("view.pan", "Pan Canvas", "View", pan).params(r#"{"x"?: n, "y"?: n, "dx"?: n, "dy"?: n}"#),
        CommandSpec::new("view.resetPan", "Reset Pan", "View", |s, _| {
            s.view.pan = [0.0, 0.0];
            Ok(json!({"pan": s.view.pan}))
        }),
    ]
}

fn pan(s: &mut Session, v: &Value) -> CmdResult {
    if let (Some(x), Some(y)) = (p::f32(v, "x"), p::f32(v, "y")) {
        s.view.pan = [x, y];
    } else {
        let dx = p::f32(v, "dx").unwrap_or(0.0);
        let dy = p::f32(v, "dy").unwrap_or(0.0);
        s.view.pan[0] += dx;
        s.view.pan[1] += dy;
    }
    Ok(json!({"pan": s.view.pan}))
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

/// Fit the page (`page`, the default) or the drawing on it into the canvas: the largest whole
/// percentage that shows it with [`FIT_MARGIN`] to spare, clamped to the zoom limits, centred.
fn zoom_fit(s: &mut Session, v: &Value) -> CmdResult {
    let Some([w, h]) = s.view.canvas else { return Err(EngineError::Disabled("No canvas is shown".into())) };
    let Some(page) = s.doc.page() else { return Err(EngineError::Failed("the document has no page".into())) };
    let target = if p::bool(v, "page").unwrap_or(true) { page.rect() } else { content_bounds(page).unwrap_or_else(|| page.rect()) };
    let (tw, th) = (target.width() as f32, target.height() as f32);
    let (aw, ah) = (w - 2.0 * FIT_MARGIN, h - 2.0 * FIT_MARGIN);
    // A canvas squeezed below the margins or a degenerate target has no meaningful fit; the
    // smallest zoom shows the most, which is what the person asked for.
    let pct = if aw > 0.0 && ah > 0.0 && tw > 0.0 && th > 0.0 { (aw / tw).min(ah / th) * 100.0 } else { MIN_ZOOM };
    // Rounded down to a whole percentage so the status bar reads cleanly and the edge still fits.
    let pct = if pct.is_finite() { pct.floor() } else { MIN_ZOOM };
    s.view.zoom = pct.clamp(MIN_ZOOM, MAX_ZOOM) / 100.0;
    let c = target.center();
    s.view.pan = [c.x as f32, c.y as f32];
    Ok(json!({"zoom": s.view.zoom, "pan": s.view.pan}))
}

/// The box around every shape and connector route on `page`, `None` on an empty page.
fn content_bounds(page: &dg::Page) -> Option<dg::Rect> {
    let shapes = page.shapes.iter().map(|s| s.bounds());
    let routes = jumps::page_geometry(page).into_iter().filter_map(|g| dg::Rect::bounding(g.polyline.iter().copied()));
    shapes.chain(routes).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
}
