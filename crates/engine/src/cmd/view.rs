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
    let mut v = vec![
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
    ];
    v.extend(aid_specs());
    v
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

// Visual aids and guides (Block 2). The aids are view settings, not document data: they live in
// `ViewState.aids`, the UI saves them in its preferences, and changing them is never an undo
// step. Guides are page data, so guide commands change the document and undo like any edit.

/// Default snap strength: how close, in screen points, an item must come to a target to snap.
pub const DEFAULT_SNAP_STRENGTH: f32 = 8.0;
/// Snap strength limits, in screen points.
pub const MIN_SNAP_STRENGTH: f32 = 1.0;
pub const MAX_SNAP_STRENGTH: f32 = 50.0;

/// What the canvas draws besides the drawing, and what dragged items snap to.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Aids {
    /// The grid under the shapes.
    pub grid: bool,
    /// Rulers along the top and left of the canvas.
    pub rulers: bool,
    /// Guide lines are drawn and attract.
    pub guides: bool,
    /// Lines where the page splits across sheets of paper.
    pub page_breaks: bool,
    /// Connection point markers on every shape, not only the one under the pointer.
    pub connection_points: bool,
    /// AutoConnect arrows on the shape under the pointer.
    pub auto_connect: bool,
    /// Snapping on or off as a whole.
    pub snap: bool,
    /// Snap to the grid.
    pub snap_grid: bool,
    /// Snap to guides.
    pub snap_guides: bool,
    /// Snap to other shapes' edges and centres.
    pub snap_shapes: bool,
    /// Snap to connection points.
    pub snap_points: bool,
    /// Snap reach in screen points.
    pub snap_strength: f32,
}

impl Default for Aids {
    fn default() -> Self {
        Aids {
            grid: true,
            rulers: true,
            guides: true,
            page_breaks: false,
            connection_points: false,
            auto_connect: true,
            snap: true,
            snap_grid: true,
            snap_guides: true,
            snap_shapes: true,
            snap_points: true,
            snap_strength: DEFAULT_SNAP_STRENGTH,
        }
    }
}

impl Aids {
    /// Snap reach in page units at `zoom`, or `None` when snapping is off.
    pub fn snap_tolerance(&self, zoom: f32) -> Option<f64> {
        let strength =
            if self.snap_strength.is_finite() { self.snap_strength.clamp(MIN_SNAP_STRENGTH, MAX_SNAP_STRENGTH) } else { DEFAULT_SNAP_STRENGTH };
        self.snap.then(|| f64::from(strength) / f64::from(zoom.max(0.01)))
    }
}

fn aid_specs() -> Vec<CommandSpec> {
    const FLAG: &str = r#"{"value"?: bool} (flips when value is omitted)"#;
    vec![
        CommandSpec::new("view.rulers", "Ruler", "View > Show", |s, v| aid(s, v, |a| &mut a.rulers)).params(FLAG),
        CommandSpec::new("view.grid", "Grid", "View > Show", |s, v| aid(s, v, |a| &mut a.grid)).params(FLAG),
        CommandSpec::new("view.guides", "Guides", "View > Show", |s, v| aid(s, v, |a| &mut a.guides)).params(FLAG),
        CommandSpec::new("view.pageBreaks", "Page Breaks", "View > Show", |s, v| aid(s, v, |a| &mut a.page_breaks)).params(FLAG),
        CommandSpec::new("view.connectionPoints", "Connection Points", "View > Show", |s, v| aid(s, v, |a| &mut a.connection_points))
            .params(FLAG),
        CommandSpec::new("view.autoConnect", "AutoConnect", "View > Visual Aids", |s, v| aid(s, v, |a| &mut a.auto_connect)).params(FLAG),
        CommandSpec::new("view.snap", "Snap", "View > Visual Aids", snap).params(
            r#"{"value"?: bool, "grid"?: bool, "guides"?: bool, "shapes"?: bool, "points"?: bool, "strength"?: points 1-50} (with no params, flips snapping)"#,
        ),
        CommandSpec::new("view.snapSettings", "Snap and Glue", "View > Visual Aids", |s, _| {
            s.ui_requests.push(json!({"open": "snapSettings"}));
            Ok(aids_json(s))
        }),
        CommandSpec::new("guide.add", "Add Guide", "View > Guides", guide_add)
            .params(r#"{"orientation": "horizontal"|"vertical", "position": n, "page"?: id|name|index} -> {page, index, guides}"#),
        CommandSpec::new("guide.move", "Move Guide", "View > Guides", guide_move)
            .params(r#"{"index": n, "position": n, "page"?: id|name|index} -> {page, index, guides}"#),
        CommandSpec::new("guide.delete", "Delete Guide", "View > Guides", guide_delete).params(r#"{"index": n, "page"?: id|name|index} -> {page, guides}"#),
    ]
}

fn aids_json(s: &Session) -> Value {
    serde_json::to_value(s.view.aids).unwrap_or_default()
}

/// Set an aid to `value`, or flip it when there is none.
fn aid(s: &mut Session, v: &Value, f: fn(&mut Aids) -> &mut bool) -> CmdResult {
    let value = super::opt_bool(v, "value")?;
    let flag = f(&mut s.view.aids);
    *flag = value.unwrap_or(!*flag);
    Ok(aids_json(s))
}

fn snap(s: &mut Session, v: &Value) -> CmdResult {
    let value = super::opt_bool(v, "value")?;
    let targets = [
        ("grid", super::opt_bool(v, "grid")?),
        ("guides", super::opt_bool(v, "guides")?),
        ("shapes", super::opt_bool(v, "shapes")?),
        ("points", super::opt_bool(v, "points")?),
    ];
    let strength = super::num(v, "strength")?;
    let a = &mut s.view.aids;
    for (k, b) in targets {
        let Some(b) = b else { continue };
        match k {
            "grid" => a.snap_grid = b,
            "guides" => a.snap_guides = b,
            "shapes" => a.snap_shapes = b,
            _ => a.snap_points = b,
        }
    }
    if let Some(st) = strength {
        a.snap_strength = (st as f32).clamp(MIN_SNAP_STRENGTH, MAX_SNAP_STRENGTH);
    }
    let other = targets.iter().any(|(_, b)| b.is_some()) || strength.is_some();
    match value {
        Some(b) => a.snap = b,
        // A bare `view.snap` is the toggle; one that sets targets or strength leaves it alone.
        None if !other => a.snap = !a.snap,
        None => {}
    }
    Ok(aids_json(s))
}

/// The page a guide command acts on: `page` (id, name or index), else the active page.
fn guide_page(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let r = match v.get("page") {
        None | Some(Value::Null) => {
            return if s.doc.pages.is_empty() { Err(EngineError::Failed("the document has no page".into())) } else { Ok(s.doc.active_index()) };
        }
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
    };
    s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", super::short(&r))))
}

fn guide_position(v: &Value) -> Result<f64, EngineError> {
    Ok(super::req_num(v, "position")?.clamp(-dg::MAX_EXTENT, dg::MAX_EXTENT))
}

fn guide_index(v: &Value, page: &dg::Page) -> Result<usize, EngineError> {
    let i = super::req_num(v, "index")?;
    if i < 0.0 || i.fract() != 0.0 || i >= page.guides.len() as f64 {
        return Err(EngineError::Params(format!("`index` must name one of the page's {} guides", page.guides.len())));
    }
    Ok(i as usize)
}

fn guides_json(page: &dg::Page) -> Value {
    serde_json::to_value(&page.guides).unwrap_or_default()
}

fn guide_add(s: &mut Session, v: &Value) -> CmdResult {
    let orientation = match super::opt_str(v, "orientation")?.map(|o| o.trim().to_ascii_lowercase()) {
        Some(o) if o == "horizontal" || o == "h" => dg::GuideOrientation::Horizontal,
        Some(o) if o == "vertical" || o == "v" => dg::GuideOrientation::Vertical,
        _ => return Err(EngineError::Params("`orientation` must be \"horizontal\" or \"vertical\"".into())),
    };
    let position = guide_position(v)?;
    let pi = guide_page(s, v)?;
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    if page.guides.len() >= dg::page_fields::MAX_GUIDES {
        return Err(EngineError::Failed(format!("a page keeps at most {} guides", dg::page_fields::MAX_GUIDES)));
    }
    page.guides.push(dg::Guide { orientation, position });
    Ok(json!({"page": page.id, "index": page.guides.len() - 1, "guides": guides_json(page)}))
}

fn guide_move(s: &mut Session, v: &Value) -> CmdResult {
    let position = guide_position(v)?;
    let pi = guide_page(s, v)?;
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    let i = guide_index(v, page)?;
    if let Some(g) = page.guides.get_mut(i) {
        g.position = position;
    }
    Ok(json!({"page": page.id, "index": i, "guides": guides_json(page)}))
}

fn guide_delete(s: &mut Session, v: &Value) -> CmdResult {
    let pi = guide_page(s, v)?;
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    let i = guide_index(v, page)?;
    page.guides.remove(i);
    Ok(json!({"page": page.id, "guides": guides_json(page)}))
}

#[cfg(test)]
mod aid_tests {
    use super::*;

    fn session() -> Session {
        Session::new(crate::sample::sample_document())
    }

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    #[test]
    fn toggles_flip_without_a_value_and_set_with_one() {
        let mut s = session();
        for (id, key) in [
            ("view.grid", "grid"),
            ("view.rulers", "rulers"),
            ("view.guides", "guides"),
            ("view.pageBreaks", "pageBreaks"),
            ("view.connectionPoints", "connectionPoints"),
            ("view.autoConnect", "autoConnect"),
            ("view.snap", "snap"),
        ] {
            let before = aids_json(&s)[key].as_bool().unwrap();
            let r = run(&mut s, id, json!({}));
            assert_eq!(r[key], json!(!before), "{id} flips");
            let r = run(&mut s, id, json!({"value": true}));
            assert_eq!(r[key], json!(true), "{id} sets");
            run(&mut s, id, json!({"value": true}));
            assert_eq!(aids_json(&s)[key], json!(true), "{id} with the same value stays");
            assert!(s.run(id, &json!({"value": "yes"})).is_err(), "{id} refuses a non-boolean");
        }
        // View settings are not document changes: nothing to undo.
        assert!(!s.can_undo());
    }

    #[test]
    fn snap_sets_targets_and_strength_without_flipping() {
        let mut s = session();
        let r = run(&mut s, "view.snap", json!({"grid": false, "points": false, "strength": 500}));
        assert_eq!(
            (r["snap"].clone(), r["snapGrid"].clone(), r["snapPoints"].clone(), r["snapShapes"].clone()),
            (json!(true), json!(false), json!(false), json!(true))
        );
        assert_eq!(r["snapStrength"], json!(MAX_SNAP_STRENGTH));
        assert!(s.run("view.snap", &json!({"strength": "big"})).is_err());
        assert_eq!(s.view.aids.snap_tolerance(2.0), Some(25.0));
        run(&mut s, "view.snap", json!({"value": false}));
        assert_eq!(s.view.aids.snap_tolerance(2.0), None);
    }

    #[test]
    fn snap_settings_asks_the_ui_for_its_dialog() {
        let mut s = session();
        run(&mut s, "view.snapSettings", json!({}));
        assert_eq!(s.ui_requests, vec![json!({"open": "snapSettings"})]);
    }

    #[test]
    fn guides_add_move_delete_and_undo() {
        let mut s = session();
        let r = run(&mut s, "guide.add", json!({"orientation": "vertical", "position": 250}));
        assert_eq!(r["index"], json!(0));
        run(&mut s, "guide.add", json!({"orientation": "horizontal", "position": 1e12, "page": 0}));
        let g = &s.doc.page().unwrap().guides;
        assert_eq!(g.len(), 2);
        assert_eq!(g[1], dg::Guide { orientation: dg::GuideOrientation::Horizontal, position: dg::MAX_EXTENT });
        run(&mut s, "guide.move", json!({"index": 0, "position": 300.5}));
        assert_eq!(s.doc.page().unwrap().guides[0].position, 300.5);
        run(&mut s, "guide.delete", json!({"index": 1}));
        assert_eq!(s.doc.page().unwrap().guides.len(), 1);
        // Each one is its own undo step.
        assert!(s.undo());
        assert_eq!(s.doc.page().unwrap().guides.len(), 2);
        assert!(s.undo());
        assert_eq!(s.doc.page().unwrap().guides[0].position, 250.0);
        assert!(s.undo());
        assert!(s.undo());
        assert!(s.doc.page().unwrap().guides.is_empty());
        assert!(s.redo());
        assert_eq!(s.doc.page().unwrap().guides.len(), 1);
    }

    #[test]
    fn guide_commands_refuse_bad_params() {
        let mut s = session();
        for (id, v) in [
            ("guide.add", json!({"orientation": "diagonal", "position": 1})),
            ("guide.add", json!({"orientation": "vertical"})),
            ("guide.add", json!({"orientation": "vertical", "position": 1, "page": "nope"})),
            ("guide.add", json!({"orientation": "vertical", "position": 1, "page": true})),
            ("guide.move", json!({"index": 0, "position": 1})),
            ("guide.delete", json!({"index": -1})),
            ("guide.delete", json!({"index": 0.5})),
        ] {
            assert!(s.run(id, &v).is_err(), "{id} {v}");
        }
        assert!(s.doc.page().unwrap().guides.is_empty());
        assert!(!s.can_undo());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    #[test]
    fn guide_commands_default_to_the_active_page() {
        let mut s = Session::new(Document::new());
        s.run("page.add", &json!({"name": "Second"})).unwrap();
        s.run("page.setActive", &json!({"page": "Second"})).unwrap();
        s.run("guide.add", &json!({"orientation": "vertical", "position": 40})).unwrap();
        assert!(s.doc.pages[0].guides.is_empty(), "the first page is untouched");
        assert_eq!(s.doc.pages[1].guides.len(), 1, "the active page gets the guide");
    }
}
