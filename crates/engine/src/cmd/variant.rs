//! Rendering variant commands (`variant.*`): list the variants of the page's notation pack and
//! redraw a page with one.
//!
//! A variant is another way of drawing the same types (Chen against crow's foot): it maps
//! element types to glyphs and relationship types to connector types (see
//! `vizcraft_notation::Variant`). The page records its choice in `View.variant`; shapes placed
//! afterwards take the variant's glyph (`place_shape`), and `variant.set` redraws what is on the
//! page already, in one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::{Page, Point};
use vizcraft_model::{Model, View, pack_of};
use vizcraft_notation::Pack;

use super::model::{cat, check_len, page_param};
use super::short;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the variant commands: `variant.list`, `variant.set`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("variant.list", "Notation Variants", "Notation > Variant", list).params(
            r#"{"page"?: page id, name or index} -> {"page", "pack": id or null, "current": variant id ("" for the default drawing), "variants": [{"id", "label", "description", "current"}]}; the pack is the page's viewpoint's, else its variant's, else the pack of the first element drawn on it"#,
        ),
        CommandSpec::new("variant.set", "Notation Variant", "Notation > Variant", set).params(
            r#"{"page"?: page id, name or index, "variant": variant id, or "" or null for the default drawing} -> {"page", "variant", "shapes": [id], "connectors": [id]}; redraws every element shape on the page with the variant's glyph (kind, path, size and style from the master; centre, text and glue kept) and every relationship connector with the variant's connector type, in one undo step"#,
        ),
    ]
}

/// The page's variant id, empty for the default drawing.
fn current(page: &Page) -> &str {
    page.view.as_ref().map(|v| v.variant.as_str()).unwrap_or_default()
}

/// The pack whose variants apply to `page`: the viewpoint's, else the current variant's, else
/// the pack of the first element shape's type.
fn page_pack<'a>(page: &Page, model: &Model) -> Option<&'a Pack> {
    let c = cat();
    let viewpoint = page.view.as_ref().map(|v| v.viewpoint.as_str()).filter(|v| !v.is_empty());
    let from_view = [viewpoint, Some(current(page)).filter(|v| !v.is_empty())].into_iter().flatten().find_map(|id| c.pack(pack_of(id)));
    from_view.or_else(|| {
        page.shapes
            .iter()
            .filter_map(|s| s.element.as_deref())
            .filter_map(|e| model.element(e))
            .find_map(|e| c.element_type(&e.type_id).map(|(p, _)| p))
    })
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let now = current(page);
    let pack = page_pack(page, &s.doc.model);
    let variants: Vec<Value> = pack
        .map(|p| p.variants.iter().map(|x| json!({"id": x.id, "label": x.label, "description": x.description, "current": x.id == now})).collect())
        .unwrap_or_default();
    Ok(json!({"page": page.id, "pack": pack.map(|p| p.id.clone()), "current": now, "variants": variants}))
}

/// The ids of the shapes and connectors [`redraw`] changed.
#[derive(Default)]
struct Redrawn {
    shapes: Vec<String>,
    connectors: Vec<String>,
}

/// Draw every element shape and relationship connector of pack `pack` on `page` as `variant`
/// draws them (empty: the default drawing). Items already drawn that way are left alone, so hand
/// styling survives on them.
fn redraw(page: &mut Page, model: &Model, pack: &str, variant: &str) -> Redrawn {
    let c = cat();
    let mut out = Redrawn::default();
    for sh in &mut page.shapes {
        let Some(t) = sh.element.as_deref().and_then(|e| model.element(e)).map(|e| e.type_id.clone()) else { continue };
        if pack_of(&t) != pack {
            continue;
        }
        let Some(m) = c.master_for_type_in(&t, variant).and_then(vizcraft_stencils::master) else { continue };
        if sh.stencil == m.id {
            continue;
        }
        let centre = Point::new(sh.x + sh.w / 2.0, sh.y + sh.h / 2.0);
        sh.w = m.w;
        sh.h = m.h;
        sh.x = centre.x - m.w / 2.0;
        sh.y = centre.y - m.h / 2.0;
        m.redraw(sh);
        if let Some(points) = vizcraft_stencils::port_points(&t) {
            sh.connection_points = points;
        }
        sh.sanitize();
        out.shapes.push(sh.id.clone());
    }
    for con in &mut page.connectors {
        let Some(t) = con.relationship.as_deref().and_then(|r| model.relationship(r)).map(|r| r.type_id.clone()) else { continue };
        if pack_of(&t) != pack {
            continue;
        }
        let Some(k) = vizcraft_stencils::connector_kind_for_relationship(&t, variant) else { continue };
        if con.kind == k.id {
            continue;
        }
        k.apply(con);
        out.connectors.push(con.id.clone());
    }
    out
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let variant = match v.get("variant") {
        None => return Err(EngineError::Params("`variant` (variant id, or \"\" or null for the default) is required".into())),
        Some(Value::Null) => String::new(),
        Some(Value::String(t)) => check_len("variant", t)?.to_string(),
        Some(_) => return Err(EngineError::Params("`variant` must be a variant id, \"\" or null".into())),
    };
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let pack = if variant.is_empty() {
        match page_pack(page, &s.doc.model) {
            Some(p) => p.id.clone(),
            // Nothing drawn from any pack and no variant to clear: nothing to do.
            None if current(page).is_empty() => return Ok(json!({"page": page.id, "variant": "", "shapes": [], "connectors": []})),
            None => pack_of(current(page)).to_string(),
        }
    } else {
        let (p, _) = cat().variant(&variant).ok_or_else(|| EngineError::Params(format!("unknown variant `{}`", short(&variant))))?;
        p.id.clone()
    };
    let Session { doc, .. } = s;
    let page = doc.pages.get_mut(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let done = redraw(page, &doc.model, &pack, &variant);
    match &mut page.view {
        Some(view) => view.variant = variant.clone(),
        None if variant.is_empty() => {}
        None => page.view = Some(View { variant: variant.clone(), ..View::default() }),
    }
    // A variant drawn on its own symbol grid module (an ANSI variant of an IEC pack) gives the
    // page that module; one that names none leaves the page's module as it was.
    if let Some(module) = cat().variant(&variant).map(|(_, x)| x.module).filter(|m| !m.is_none()) {
        page.view.get_or_insert_with(View::default).module = module;
    }
    let id = page.id.clone();
    Ok(json!({"page": id, "variant": variant, "shapes": done.shapes, "connectors": done.connectors}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::{Document, ShapeKind};

    use crate::Session;

    /// A pack built in code with two rendering variants, a path glyph and a type with ports,
    /// loaded into the catalog once (the `engtest` convention of `cmd/notation.rs`).
    fn pack() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let g = |id: &str, outline: &str, w: f64, h: f64| json!({"id": id, "label": id, "category": "Test", "outline": outline, "w": w, "h": h, "fill": "#ffffff", "line": "#203040"});
            let mut ring = g("vartest.ring", "path", 90.0, 70.0);
            ring["path"] = json!("M0 0 L1 0 L1 1 L0 1 Z M0.3 0.3 L0.7 0.3 L0.7 0.7 L0.3 0.7 Z");
            ring["detail"] = json!("M0 0.15 L1 0.15");
            let pack = json!({
                "id": "vartest", "version": "1.0.0", "name": "Variant test",
                "glyphs": [
                    {"id": "vartest.box", "label": "Box", "category": "Test", "outline": "rectangle", "w": 120, "h": 60, "fill": "#ffffff", "line": "#000000", "elementType": "vartest.Node"},
                    {"id": "vartest.store", "label": "Store", "category": "Test", "outline": "cylinder", "w": 80, "h": 90, "fill": "#ffffff", "line": "#000000", "elementType": "vartest.Store"},
                    g("vartest.round", "ellipse", 100.0, 100.0),
                    ring
                ],
                "elementTypes": [
                    {"id": "vartest.Node", "glyph": "vartest.box"},
                    {"id": "vartest.Store", "glyph": "vartest.store", "ports": [
                        {"id": "west", "label": "West", "direction": "in", "x": 0, "y": 0.5},
                        {"id": "east", "label": "East", "direction": "out", "x": 1, "y": 0.5},
                        {"id": "top", "direction": "inOut", "x": 0.5, "y": 0}
                    ]}
                ],
                "relationshipTypes": [{"id": "vartest.Link"}],
                "connections": [{"source": ["*"], "relationship": "vartest.Link", "target": ["*"]}],
                "connectorTypes": [
                    {"id": "vartest.line", "label": "Line", "relationshipType": "vartest.Link"},
                    {"id": "vartest.dashed", "label": "Dashed", "dash": "dashed", "endArrow": "slash"},
                    {"id": "vartest.thick", "label": "Thick", "lineWidth": 4}
                ],
                "variants": [
                    {"id": "vartest.round", "label": "Round", "description": "Circles and rings.", "glyphs": {"vartest.Node": "vartest.round", "vartest.Store": "vartest.ring"}, "connectors": {"vartest.Link": "vartest.dashed"}},
                    {"id": "vartest.heavy", "label": "Heavy", "connectors": {"vartest.Link": "vartest.thick"}}
                ]
            });
            let p = vizcraft_notation::loader::load_json(&pack.to_string()).unwrap();
            vizcraft_notation::catalog::add_pack(p).unwrap();
        });
    }

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// Two Nodes and a Store on the first page, linked Node to Store by a connector glued at the
    /// Store's connection point 1.
    fn drawn() -> (Session, [String; 3], String) {
        pack();
        let mut s = Session::new(Document::new());
        let mut shapes = Vec::new();
        for (t, x) in [("vartest.Node", 0.0), ("vartest.Node", 300.0), ("vartest.Store", 600.0)] {
            let r = run(&mut s, "element.create", json!({"type": t, "name": t, "x": x, "y": 100}));
            shapes.push(r["shape"].as_str().unwrap().to_string());
        }
        let c = run(&mut s, "connector.add", json!({"from": {"shape": shapes[0]}, "to": {"shape": shapes[2], "point": 1}, "kind": "vartest.line"}));
        let shapes: [String; 3] = shapes.try_into().unwrap();
        (s, shapes, c["id"].as_str().unwrap().to_string())
    }

    #[test]
    fn a_pack_loaded_at_run_time_draws_its_connector_kinds() {
        pack();
        let k = vizcraft_stencils::connector_kind("vartest.dashed").expect("the kinds are rebuilt for the new catalog");
        assert_eq!((k.dash, k.end_arrow), (vizcraft_diagram::Dash::Dashed, vizcraft_diagram::Arrow::Slash));
        assert_eq!(vizcraft_stencils::connector_kind_for_relationship("vartest.Link", "vartest.heavy").map(|k| k.id), Some("vartest.thick"));
    }

    #[test]
    fn a_typed_shape_takes_its_ports_and_glue_at_point_1_binds_to_port_1() {
        let (s, shapes, c) = drawn();
        let page = &s.doc.pages[0];
        let store = page.shape(&shapes[2]).unwrap();
        let pts: Vec<(f64, f64)> = store.connection_points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(pts, [(0.0, 0.5), (1.0, 0.5), (0.5, 0.0)]);
        let rel = page.connector(&c).unwrap().relationship.clone().expect("the connector draws a relationship");
        let r = s.doc.model.relationship(&rel).unwrap();
        assert_eq!(r.target_port.as_deref(), Some("east"));
        let marks = vizcraft_stencils::port_marks(store, "vartest.Store");
        assert_eq!(marks.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["West", "East", "top"]);
    }

    #[test]
    fn variant_set_redraws_every_element_shape_and_relationship_connector_in_one_step() {
        let (mut s, shapes, c) = drawn();
        let before = s.doc.clone();
        let list = run(&mut s, "variant.list", json!({}));
        assert_eq!((list["pack"].as_str(), list["current"].as_str()), (Some("vartest"), Some("")));
        assert_eq!(list["variants"].as_array().unwrap().len(), 2);
        let centre = s.doc.pages[0].shape(&shapes[0]).unwrap().frame().center();
        let r = run(&mut s, "variant.set", json!({"variant": "vartest.round"}));
        assert_eq!(r["shapes"].as_array().unwrap().len(), 3);
        assert_eq!(r["connectors"], json!([c]));
        let page = &s.doc.pages[0];
        for id in &shapes[..2] {
            let sh = page.shape(id).unwrap();
            assert_eq!((sh.stencil.as_str(), sh.kind, sh.w, sh.h), ("vartest.round", ShapeKind::Ellipse, 100.0, 100.0));
            assert!(!sh.text.is_empty(), "the text is kept");
        }
        assert_eq!(page.shape(&shapes[0]).unwrap().frame().center(), centre, "the centre is kept");
        let ring = page.shape(&shapes[2]).unwrap();
        assert_eq!((ring.kind, ring.w, ring.h), (ShapeKind::Freeform, 90.0, 70.0), "the path glyph places as a freeform of its size");
        assert_eq!(ring.connection_points.len(), 3, "the type's ports survive the redraw");
        let con = page.connector(&c).unwrap();
        assert_eq!((con.kind.as_str(), con.end_arrow), ("vartest.dashed", vizcraft_diagram::Arrow::Slash));
        assert_eq!(con.end.shape(), Some(shapes[2].as_str()), "the glue is kept");
        assert_eq!(page.view.as_ref().map(|v| v.variant.as_str()), Some("vartest.round"));
        assert_eq!(run(&mut s, "variant.list", json!({}))["current"], "vartest.round");
        // The freeform exports with an even-odd fill and its detail line unfilled.
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        assert!(svg.contains("fill-rule=\"evenodd\""), "even-odd fill");
        // The Store's three ports export as marks with their labels.
        let ports = svg.split("<g class=\"ports\">").nth(1).expect("a ports group");
        assert_eq!(ports.split("</g>").next().unwrap_or_default().matches("<rect").count(), 4, "three squares and the half fill of inOut");
        assert!(ports.contains(">West</tspan>") && ports.contains(">East</tspan>"));
        // A shape placed now takes the variant's glyph.
        let n = run(&mut s, "element.create", json!({"type": "vartest.Node", "name": "Later", "x": 0, "y": 400}));
        assert_eq!(s.doc.pages[0].shape(n["shape"].as_str().unwrap()).unwrap().stencil, "vartest.round");
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "one undo step takes the whole redraw back");
        // Back to the default drawing.
        run(&mut s, "variant.set", json!({"variant": "vartest.heavy"}));
        assert_eq!(s.doc.pages[0].connector(&c).unwrap().kind, "vartest.thick");
        assert_eq!(s.doc.pages[0].shape(&shapes[0]).unwrap().stencil, "vartest.box", "a variant that leaves a type alone draws it by default");
        run(&mut s, "variant.set", json!({"variant": null}));
        assert_eq!(s.doc.pages[0].connector(&c).unwrap().kind, "vartest.line");
    }

    #[test]
    fn variant_commands_refuse_junk() {
        let (mut s, _, _) = drawn();
        for bad in [
            json!({}),
            json!({"variant": 3}),
            json!({"variant": "vartest.nope"}),
            json!({"variant": "x".repeat(10_000)}),
            json!({"variant": "vartest.round", "page": "nope"}),
        ] {
            assert!(s.run("variant.set", &bad).is_err(), "{bad}");
        }
        assert!(s.run("variant.list", &json!({"page": 7})).is_err());
        // A page with nothing from any pack lists no variants and resets to nothing.
        let mut empty = Session::new(Document::new());
        assert_eq!(run(&mut empty, "variant.list", json!({}))["variants"], json!([]));
        assert_eq!(run(&mut empty, "variant.set", json!({"variant": ""}))["shapes"], json!([]));
    }
}
