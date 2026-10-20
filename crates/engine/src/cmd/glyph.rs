//! Parametric glyph commands (`glyph.*`): list the builders, build a symbol without touching
//! the document, place a free symbol drawn by a builder, read and set a shape's builder
//! parameters, and redraw builder shapes.
//!
//! A builder shape redraws itself after every command whose inputs changed (see
//! `vizcraft_stencils::builders::sync`), so `glyph.setParams` and `element.setProperty` only set
//! values; `glyph.rebuild` forces a redraw, for example after a builder's code changed.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use vizcraft_diagram::glyph_state::GlyphState;
use vizcraft_diagram::vizcraft_model::PropValue;
use vizcraft_stencils::builders::{self, BuilderSpec, Built, ParamKind, apply, sync};

use super::model::{cat, check_len, page_param};
use super::{Kinds, num, opt_str, short, targets};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the glyph commands: `glyph.builders`, `glyph.build`, `glyph.insert`, `glyph.params`,
/// `glyph.setParams`, `glyph.rebuild`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("glyph.builders", "Glyph Builders", "Insert > Symbols", builders_cmd).params(
            r#"{"builder"?: name} -> {"builders": [{"name", "label", "description", "standards", "sources": [{"standard", "source"}], "params": [{"key", "label", "kind": "choice"|"int"|"number"|"bool"|"text", "choices"?, "min"?, "max"?, "maxLength"?, "default", "description"}]}]}"#,
        ),
        CommandSpec::new("glyph.build", "Build Glyph", "Insert > Symbols", build_cmd).params(
            r#"{"builder": name | "glyph": pack glyph id, "params"?: {key: value}, "module"?: page units} -> {"builder", "d", "lines", "svg", "w", "h", "module", "standard", "ports": [{"id", "label", "x", "y", "dir"}], "labels": [{"id", "text", "x", "y", "size", "anchor"}], "warnings"}; page units, origin top left; d is the filled outline, lines the stroked lines; a glyph's params are the defaults under the given ones; changes no document"#,
        ),
        CommandSpec::new("glyph.insert", "Insert Symbol", "Insert > Symbols", insert).params(
            r#"{"builder": name, "params"?: {key: value}, "x"?: n, "y"?: n, "page"?: page id, name or index} -> {"id", "builder", "shapes": [id], "primary", "items"}; places a free shape drawn by the builder (no element), its own parameter overrides from params, top left at x, y (default: the page frame's centre); on a page with a symbol grid module the first pin lands on the grid; selects it on the active page; one undo step. glyph.params and glyph.setParams work on it"#,
        ),
        CommandSpec::new("glyph.params", "Symbol Parameters", "Home > Symbol", params_cmd).params(
            r#"{"id": shape id on the active page} -> {"id", "builder", "glyph": pack glyph id or null, "params": [{"key", "value", "from": "default"|"glyph"|"shape"|"element", "spec"}]}; the value each parameter is drawn with and the layer it came from"#,
        ),
        CommandSpec::new("glyph.setParams", "Set Symbol Parameters", "Home > Symbol", set_params).params(
            r#"{"ids"?: [shape id], "params": {key: value or null}} -> {"ids"}; sets (or with null clears) the shapes' own parameter overrides, each checked against the builder's spec; the shapes redraw; one undo step. Element properties still win over these"#,
        ),
        CommandSpec::new("glyph.rebuild", "Rebuild Symbols", "Home > Symbol", rebuild).params(
            r#"{"ids"?: [shape id], "page"?: page id or name} -> {"page", "shapes": [id]}; redraws the builder shapes (all on the page without ids) from their current inputs; lists the ones that changed"#,
        ),
    ]
}

/// Most parameters one call may pass.
const MAX_PARAMS: usize = 64;

/// The JSON schema of one builder.
fn spec_json(spec: &BuilderSpec) -> Value {
    let params: Vec<Value> = spec
        .params
        .iter()
        .map(|p| {
            let mut o = json!({"key": p.key, "label": p.label, "kind": p.kind.name(), "default": prop_json(&p.default.value()), "description": p.description});
            match p.kind {
                ParamKind::Choice(names) => o["choices"] = json!(names),
                ParamKind::Int { min, max } => {
                    o["min"] = json!(min);
                    o["max"] = json!(max);
                }
                ParamKind::Number { min, max } => {
                    o["min"] = json!(min);
                    o["max"] = json!(max);
                }
                ParamKind::Text { max } => o["maxLength"] = json!(max),
                ParamKind::Bool => {}
            }
            o
        })
        .collect();
    let standards: Vec<&str> = spec.sources.iter().map(|(s, _)| *s).collect();
    let sources: Vec<Value> = spec.sources.iter().map(|(s, src)| json!({"standard": s, "source": src})).collect();
    json!({"name": spec.name, "label": spec.label, "description": spec.description, "standards": standards, "sources": sources, "params": params})
}

fn prop_json(v: &PropValue) -> Value {
    match v {
        PropValue::Bool(b) => json!(b),
        PropValue::Number(n) => json!(n),
        PropValue::Text(t) => json!(t),
    }
}

/// A JSON value as a property value; `None` for null, an error for anything else.
fn prop_of(key: &str, v: &Value) -> Result<Option<PropValue>, EngineError> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(PropValue::Bool(*b))),
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite())
            .map(|f| Some(PropValue::Number(f)))
            .ok_or_else(|| EngineError::Params(format!("`{}` must be a finite number", short(key)))),
        Value::String(t) if t.len() <= 1024 => Ok(Some(PropValue::Text(t.clone()))),
        Value::String(_) => Err(EngineError::Params(format!("`{}` is longer than 1024 bytes", short(key)))),
        _ => Err(EngineError::Params(format!("`{}` must be text, a number, true or false, or null", short(key)))),
    }
}

/// The `params` object, at most [`MAX_PARAMS`] entries; `None` when absent.
fn params_obj(v: &Value) -> Result<Option<&Map<String, Value>>, EngineError> {
    match v.get("params") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(m)) if m.len() <= MAX_PARAMS => {
            if let Some(k) = m.keys().find(|k| k.is_empty() || k.len() > 64) {
                return Err(EngineError::Params(format!("parameter key `{}` must be 1 to 64 bytes", short(k))));
            }
            Ok(Some(m))
        }
        Some(Value::Object(_)) => Err(EngineError::Params(format!("`params` has more than {MAX_PARAMS} entries"))),
        Some(_) => Err(EngineError::Params("`params` must be an object".into())),
    }
}

fn unknown(name: &str) -> EngineError {
    EngineError::Params(format!("no glyph builder is named `{}` (see glyph.builders)", short(name)))
}

fn builders_cmd(_s: &mut Session, v: &Value) -> CmdResult {
    let list: Vec<Value> = match opt_str(v, "builder")? {
        Some(name) => vec![spec_json(builders::spec(name).ok_or_else(|| unknown(name))?)],
        None => builders::specs().iter().map(spec_json).collect(),
    };
    Ok(json!({"builders": list}))
}

/// A built symbol as JSON, in page units.
pub fn built_json(name: &str, b: &Built) -> Value {
    let k = b.module;
    let ports: Vec<Value> = b.ports.iter().map(|p| json!({"id": p.id, "label": p.label, "x": p.x * k, "y": p.y * k, "dir": p.dir})).collect();
    let labels: Vec<Value> =
        b.labels.iter().map(|l| json!({"id": l.id, "text": l.text, "x": l.x * k, "y": l.y * k, "size": l.size * k, "anchor": l.anchor})).collect();
    json!({
        "builder": name,
        "d": builders::path_data(&b.fills, k),
        "lines": builders::path_data(&b.lines, k),
        "svg": builders::svg(b),
        "w": b.width(),
        "h": b.height(),
        "module": k,
        "standard": b.standard,
        "ports": ports,
        "labels": labels,
        "warnings": b.warnings,
    })
}

fn build_cmd(_s: &mut Session, v: &Value) -> CmdResult {
    let mut raw: BTreeMap<String, PropValue> = BTreeMap::new();
    let name = match (opt_str(v, "builder")?, opt_str(v, "glyph")?) {
        (Some(b), None) => builders::spec(b).ok_or_else(|| unknown(b))?.name,
        (None, Some(g)) => {
            let g = check_len("glyph", g)?;
            let (_, glyph) = cat().glyph(g).ok_or_else(|| EngineError::Params(format!("no glyph `{}`", short(g))))?;
            let spec =
                builders::spec(&glyph.builder).ok_or_else(|| EngineError::Params(format!("glyph `{}` is not drawn by a builder", short(g))))?;
            raw.extend(glyph.params.clone());
            spec.name
        }
        _ => return Err(EngineError::Params("give `builder` or `glyph`, not both".into())),
    };
    if let Some(m) = params_obj(v)? {
        for (k, val) in m {
            match prop_of(k, val)? {
                Some(p) => raw.insert(k.clone(), p),
                None => raw.remove(k),
            };
        }
    }
    let module = num(v, "module")?;
    if module.is_some_and(|m| m <= 0.0) {
        return Err(EngineError::Params("`module` must be a positive number of page units".into()));
    }
    let b = builders::build(name, &raw, module).map_err(|e| EngineError::Params(e.to_string()))?;
    Ok(built_json(name, &b))
}

/// Place a free builder shape (see the `glyph.insert` params doc).
fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let name = crate::p::req_str(v, "builder")?;
    let spec = builders::spec(name).ok_or_else(|| unknown(name))?;
    // The shape's own overrides, each checked against the builder's spec before anything changes.
    let mut own: BTreeMap<String, PropValue> = BTreeMap::new();
    if let Some(m) = params_obj(v)? {
        for (k, val) in m {
            let p = spec.param(k).ok_or_else(|| EngineError::Params(format!("`{}` is not a parameter of `{}`", short(k), spec.name)))?;
            if let Some(value) = prop_of(k, val)? {
                if let (_, Some(why)) = p.check(&value) {
                    return Err(EngineError::Params(why));
                }
                own.insert(k.clone(), value);
            }
        }
    }
    let index = page_param(&s.doc, v)?;
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let module = sync::page_module(page);
    let built = builders::build(spec.name, &own, module).map_err(|e| EngineError::Params(e.to_string()))?;
    let (w, h) = (built.width(), built.height());
    let centre = page.frame().center();
    let x = super::shape::coord(num(v, "x")?.unwrap_or(centre.x - w / 2.0));
    let y = super::shape::coord(num(v, "y")?.unwrap_or(centre.y - h / 2.0));
    let id = s.doc.next_id("s");
    let mut shape = vizcraft_diagram::Shape::new(id.clone(), vizcraft_diagram::ShapeKind::Freeform, x, y, w.max(1.0), h.max(1.0));
    shape.glyph = Some(GlyphState { builder: spec.name.to_string(), params: own, ..GlyphState::default() });
    apply::apply(&mut shape, spec.name, &built);
    if let Some(m) = module {
        apply::snap_first_port(&mut shape, m);
    }
    shape.sanitize();
    let page = s.doc.pages.get_mut(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    page.shapes.push(shape);
    if index == s.doc.active_index() {
        super::select(s, vec![id.clone()]);
    }
    super::with_sel(s, json!({"id": id, "builder": spec.name, "shapes": [id]}))
}

fn params_cmd(s: &mut Session, v: &Value) -> CmdResult {
    let id = check_len("id", crate::p::req_str(v, "id")?)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let shape = page.shape(id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(id))))?;
    let element = shape.element.as_deref().and_then(|e| s.doc.model.element(e));
    let r = apply::resolve(shape, element, cat(), sync::page_variant(page))
        .ok_or_else(|| EngineError::Params(format!("`{}` is not drawn by a glyph builder", short(id))))?;
    let all = spec_json(r.spec);
    let specs = all["params"].as_array().cloned().unwrap_or_default();
    let params: Vec<Value> = r
        .layers
        .iter()
        .map(|(k, val, from)| {
            let spec = specs.iter().find(|p| p["key"] == *k).cloned().unwrap_or(Value::Null);
            json!({"key": k, "value": prop_json(val), "from": from.name(), "spec": spec})
        })
        .collect();
    Ok(json!({"id": id, "builder": r.spec.name, "glyph": r.glyph.map(|g| g.id.clone()), "params": params}))
}

fn set_params(s: &mut Session, v: &Value) -> CmdResult {
    let m = params_obj(v)?.ok_or_else(|| EngineError::Params("`params` (object of key: value or null) is required".into()))?;
    let ids = targets(s, v, Kinds::Shapes)?;
    let page_index = s.doc.active_index();
    let variant = s.doc.page().map(|p| sync::page_variant(p).to_string()).unwrap_or_default();
    // Check everything before changing anything.
    let mut plan: Vec<(String, &'static str, BTreeMap<String, Option<PropValue>>)> = Vec::new();
    {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        for id in &ids {
            let shape = page.shape(id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(id))))?;
            let element = shape.element.as_deref().and_then(|e| s.doc.model.element(e));
            let r = apply::resolve(shape, element, cat(), &variant)
                .ok_or_else(|| EngineError::Params(format!("`{}` is not drawn by a glyph builder", short(id))))?;
            let mut changes = BTreeMap::new();
            for (k, val) in m {
                let spec = r.spec.param(k).ok_or_else(|| EngineError::Params(format!("`{}` is not a parameter of `{}`", short(k), r.spec.name)))?;
                let value = prop_of(k, val)?;
                if let Some(p) = &value {
                    let (_, why) = spec.check(p);
                    if let Some(why) = why {
                        return Err(EngineError::Params(why));
                    }
                }
                changes.insert(k.clone(), value);
            }
            plan.push((id.clone(), r.spec.name, changes));
        }
    }
    let page = s.doc.pages.get_mut(page_index).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    for (id, builder, changes) in plan {
        let Some(shape) = page.shapes.iter_mut().find(|x| x.id == id) else { continue };
        let g = shape.glyph.get_or_insert_with(|| GlyphState { builder: builder.to_string(), ..GlyphState::default() });
        for (k, value) in changes {
            match value {
                Some(p) => g.params.insert(k, p),
                None => g.params.remove(&k),
            };
        }
        shape.sanitize();
    }
    Ok(json!({"ids": ids}))
}

fn rebuild(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let wanted = super::ids_param(v)?;
    let Session { doc, .. } = s;
    let page = doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    if let Some(bad) = wanted.iter().flatten().find(|id| page.shape(id).is_none()) {
        return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(bad))));
    }
    let variant = sync::page_variant(page).to_string();
    let module = sync::page_module(page);
    let page_id = page.id.clone();
    let elements = &doc.model;
    let page = doc.pages.get_mut(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let mut changed = Vec::new();
    for shape in &mut page.shapes {
        if wanted.as_ref().is_some_and(|ids| !ids.contains(&shape.id)) {
            continue;
        }
        let element = shape.element.as_deref().and_then(|e| elements.element(e));
        if apply::resolve(shape, element, cat(), &variant).is_none() {
            continue;
        }
        if matches!(sync::redraw(shape, element, cat(), &variant, module), Ok(true)) {
            changed.push(shape.id.clone());
        }
    }
    Ok(json!({"page": page_id, "shapes": changed}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::{Document, ShapeKind};
    use vizcraft_stencils::builders::{MODULE_INCH, MODULE_METRIC};

    use crate::Session;

    /// A pack built in code with builder glyphs: an IEC contact, its JIC variant and an
    /// instrument bubble, loaded into the catalog once (the `engtest` convention of
    /// `cmd/notation.rs`).
    fn pack() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let g = |id: &str, builder: &str, params: Value, t: &str| {
                json!({"id": id, "label": id, "category": "Test", "outline": "builder", "w": 40, "h": 40, "fill": "#ffffff", "line": "#203040",
                       "builder": builder, "params": params, "elementType": t, "source": "own design"})
            };
            let pack = json!({
                "id": "glyphtest", "version": "1.0.0", "name": "Glyph test",
                "glyphs": [
                    g("glyphtest.contact", "contact", json!({"standard": "iec60617"}), "glyphtest.Contact"),
                    g("glyphtest.contactJic", "contact", json!({"standard": "jic"}), ""),
                    g("glyphtest.bubble", "bubble", json!({}), "glyphtest.Instrument")
                ],
                "elementTypes": [
                    {"id": "glyphtest.Contact", "glyph": "glyphtest.contact", "properties": [{"key": "poles", "kind": "integer"}, {"key": "form", "kind": "enum", "values": ["no", "nc"]}]},
                    {"id": "glyphtest.Instrument", "glyph": "glyphtest.bubble"}
                ],
                "relationshipTypes": [{"id": "glyphtest.Wire"}],
                "connections": [{"source": ["*"], "relationship": "glyphtest.Wire", "target": ["*"]}],
                "variants": [{"id": "glyphtest.jic", "label": "JIC", "glyphs": {"glyphtest.Contact": "glyphtest.contactJic"}}]
            });
            let p = vizcraft_notation::loader::load_json(&pack.to_string()).unwrap();
            vizcraft_notation::catalog::add_pack(p).unwrap();
        });
    }

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// A session with one contact element drawn; returns the session, element and shape ids.
    fn drawn() -> (Session, String, String) {
        pack();
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "element.create", json!({"type": "glyphtest.Contact", "name": "K1", "x": 100, "y": 100}));
        (s, r["id"].as_str().unwrap().to_string(), r["shape"].as_str().unwrap().to_string())
    }

    #[test]
    fn a_placed_contact_is_built_and_poles_redraw_it_in_one_undo_step() {
        let (mut s, e, sh) = drawn();
        let shape = s.doc.pages[0].shape(&sh).unwrap().clone();
        assert_eq!(shape.kind, ShapeKind::Freeform);
        assert_eq!(shape.glyph.as_ref().unwrap().ports, ["13", "14"]);
        assert_eq!((shape.w, shape.h), (2.0 * MODULE_METRIC, 4.0 * MODULE_METRIC));
        let before = s.doc.clone();
        run(&mut s, "element.setProperty", json!({"id": e, "key": "poles", "value": 3}));
        let shape = s.doc.pages[0].shape(&sh).unwrap();
        assert_eq!(shape.glyph.as_ref().unwrap().ports, ["13", "14", "23", "24", "33", "34"]);
        assert_eq!(shape.connection_points.len(), 6);
        assert_eq!((shape.x, shape.y), (100.0, 100.0), "the top left is kept");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "one undo step");
    }

    #[test]
    fn a_page_module_snaps_the_first_port_to_the_grid_even_after_a_turn() {
        let (mut s, _e, sh) = drawn();
        let pin = |s: &Session| {
            let shape = s.doc.pages[0].shape(&sh).unwrap();
            let p = shape.connection_points[0];
            shape.transform().apply(vizcraft_diagram::geom::Point::new(p.x * shape.w, p.y * shape.h))
        };
        let on_grid = |v: f64| ((v / 10.0).round() * 10.0 - v).abs() < 1e-6;
        run(&mut s, "shape.move", json!({"ids": [sh], "dx": 3.3, "dy": 7.1}));
        run(&mut s, "shape.rotate", json!({"ids": [sh], "turns": 1}));
        run(&mut s, "page.setModule", json!({"module": "inch0_1"}));
        let at = pin(&s);
        assert!(on_grid(at.x) && on_grid(at.y), "first port at {at:?} is on the 0.1 inch grid");
    }

    #[test]
    fn a_variant_redraws_the_contact_as_jic_at_its_module_and_keeps_the_poles() {
        let (mut s, e, sh) = drawn();
        run(&mut s, "element.setProperty", json!({"id": e, "key": "poles", "value": 3}));
        run(&mut s, "variant.set", json!({"variant": "glyphtest.jic"}));
        let shape = s.doc.pages[0].shape(&sh).unwrap();
        assert_eq!(shape.stencil, "glyphtest.contactJic");
        assert_eq!((shape.w, shape.h), (4.0 * MODULE_INCH, 6.0 * MODULE_INCH), "0.1 inch module, three rungs");
        assert_eq!(shape.glyph.as_ref().unwrap().ports.len(), 6);
        assert!(shape.connection_points.iter().all(|p| p.x == 0.0 || p.x == 1.0), "pins at the rung ends");
    }

    #[test]
    fn glue_at_a_built_point_binds_to_the_builders_port_and_a_second_sync_changes_nothing() {
        let (mut s, e, sh) = drawn();
        run(&mut s, "element.setProperty", json!({"id": e, "key": "poles", "value": 3}));
        let b = run(&mut s, "element.create", json!({"type": "glyphtest.Instrument", "name": "TI", "x": 400, "y": 100}));
        let bubble = b["shape"].as_str().unwrap().to_string();
        // Point 2 is the third pin: the second pole's upper terminal.
        let c = run(&mut s, "connector.add", json!({"from": {"shape": bubble, "point": 3}, "to": {"shape": sh, "point": 2}}));
        let page = &s.doc.pages[0];
        let rel = page.connector(c["id"].as_str().unwrap()).unwrap().relationship.clone().unwrap();
        let r = s.doc.model.relationship(&rel).unwrap();
        assert_eq!((r.target_port.as_deref(), r.source_port.as_deref()), (Some("23"), Some("w")));
        // Rebuilding against itself, or as if everything were new, changes nothing.
        let mut doc = s.doc.clone();
        let same = doc.clone();
        assert!(vizcraft_stencils::builders::sync::rebuild(&mut doc, &same, vizcraft_notation::catalog()).is_empty());
        assert!(vizcraft_stencils::builders::sync::rebuild(&mut doc, &Document::new(), vizcraft_notation::catalog()).is_empty());
        assert_eq!(doc, same);
        // A command that changes nothing about the inputs leaves the shape alone.
        let shape = s.doc.pages[0].shape(&sh).unwrap().clone();
        run(&mut s, "shape.move", json!({"ids": [bubble], "dx": 10, "dy": 0}));
        assert_eq!(s.doc.pages[0].shape(&sh).unwrap(), &shape);
    }

    #[test]
    fn the_palette_master_matches_glyph_build_and_bubble_slots_reach_the_page_svg() {
        let (mut s, _, _) = drawn();
        let m = vizcraft_stencils::master("glyphtest.contact").expect("the builder glyph is a master");
        let r = run(&mut s, "glyph.build", json!({"glyph": "glyphtest.contact"}));
        assert_eq!((m.w, m.h), (r["w"].as_f64().unwrap(), r["h"].as_f64().unwrap()));
        let local = m.path.as_ref().unwrap().transformed(&vizcraft_diagram::Transform::scale(m.w, m.h));
        let want = format!("{} {}", r["d"].as_str().unwrap(), r["lines"].as_str().unwrap());
        assert_eq!(vizcraft_stencils::builders::path_data(&local, 1.0), want.trim());
        assert_eq!(m.glyph.as_ref().unwrap().ports, ["13", "14"]);
        run(&mut s, "element.create", json!({"type": "glyphtest.Instrument", "name": "TI", "x": 400, "y": 100}));
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        assert!(svg.contains(">TI</tspan>") && svg.contains(">100</tspan>"), "the bubble's slots");
        assert!(svg.contains(">13</tspan>"), "the contact's terminal numbers");
    }

    #[test]
    fn insert_places_a_free_builder_shape_that_params_and_set_params_edit() {
        let mut s = Session::new(Document::new());
        let before = s.doc.clone();
        let r = run(&mut s, "glyph.insert", json!({"builder": "contact", "x": 100, "y": 120}));
        let id = r["id"].as_str().unwrap().to_string();
        assert_eq!((r["primary"].as_str(), r["shapes"].clone()), (Some(id.as_str()), json!([id])));
        assert_eq!(s.sel.items, std::slice::from_ref(&id), "the new shape is selected");
        let shape = s.doc.pages[0].shape(&id).unwrap().clone();
        assert_eq!(shape.kind, ShapeKind::Freeform);
        assert!(shape.element.is_none(), "a free symbol, no element");
        assert_eq!((shape.x, shape.y), (100.0, 120.0));
        assert_eq!(shape.glyph.as_ref().unwrap().builder, "contact");
        assert_eq!(shape.glyph.as_ref().unwrap().ports, ["13", "14"]);
        assert_eq!(shape.connection_points.len(), 2);
        // glyph.params reads it; glyph.setParams redraws it.
        let p = run(&mut s, "glyph.params", json!({"id": id}));
        assert_eq!(p["builder"], "contact");
        assert!(p["params"].as_array().unwrap().iter().any(|x| x["key"] == "poles"), "{p}");
        run(&mut s, "glyph.setParams", json!({"ids": [id], "params": {"poles": 2}}));
        let shape = s.doc.pages[0].shape(&id).unwrap();
        assert_eq!(shape.glyph.as_ref().unwrap().ports, ["13", "14", "23", "24"]);
        assert_eq!(shape.connection_points.len(), 4);
        // One undo step each.
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before);
    }

    #[test]
    fn insert_takes_params_snaps_to_the_module_and_refuses_junk() {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "glyph.insert", json!({"builder": "contact", "params": {"poles": 3}}));
        let id = r["id"].as_str().unwrap().to_string();
        assert_eq!(s.doc.pages[0].shape(&id).unwrap().glyph.as_ref().unwrap().ports.len(), 6);
        run(&mut s, "page.setModule", json!({"module": "inch0_1"}));
        let r = run(&mut s, "glyph.insert", json!({"builder": "coil", "x": 33.3, "y": 71.7}));
        let shape = s.doc.pages[0].shape(r["id"].as_str().unwrap()).unwrap();
        let p = shape.connection_point_world(0).unwrap();
        let on_grid = |v: f64| ((v / 10.0).round() * 10.0 - v).abs() < 1e-6;
        assert!(on_grid(p.x) && on_grid(p.y), "the first pin is on the 0.1 inch grid: {p:?}");
        for b in vizcraft_stencils::builders::specs() {
            run(&mut s, "glyph.insert", json!({"builder": b.name}));
        }
        for bad in [
            json!({}),
            json!({"builder": "nope"}),
            json!({"builder": "contact", "params": {"poles": "many"}}),
            json!({"builder": "contact", "params": {"zzz": 1}}),
            json!({"builder": "contact", "page": "nope"}),
        ] {
            assert!(s.run("glyph.insert", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn params_set_params_and_rebuild() {
        let (mut s, e, sh) = drawn();
        let r = run(&mut s, "glyph.params", json!({"id": sh}));
        assert_eq!((r["builder"].as_str(), r["glyph"].as_str()), (Some("contact"), Some("glyphtest.contact")));
        let from = |r: &Value, k: &str| r["params"].as_array().unwrap().iter().find(|p| p["key"] == k).unwrap()["from"].as_str().unwrap().to_string();
        assert_eq!((from(&r, "standard"), from(&r, "form")), ("glyph".into(), "default".into()));
        let before = s.doc.clone();
        run(&mut s, "glyph.setParams", json!({"ids": [sh], "params": {"form": "nc", "timing": "onDelay"}}));
        assert_eq!(s.doc.pages[0].shape(&sh).unwrap().glyph.as_ref().unwrap().ports, ["11", "12"]);
        assert_eq!(from(&run(&mut s, "glyph.params", json!({"id": sh})), "form"), "shape");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before);
        // The element wins over the shape.
        run(&mut s, "glyph.setParams", json!({"ids": [sh], "params": {"form": "nc"}}));
        run(&mut s, "element.setProperty", json!({"id": e, "key": "form", "value": "no"}));
        assert_eq!(from(&run(&mut s, "glyph.params", json!({"id": sh})), "form"), "element");
        assert_eq!(s.doc.pages[0].shape(&sh).unwrap().glyph.as_ref().unwrap().ports, ["13", "14"]);
        // Null clears an override; values are checked against the spec.
        run(&mut s, "glyph.setParams", json!({"ids": [sh], "params": {"form": null}}));
        assert!(s.doc.pages[0].shape(&sh).unwrap().glyph.as_ref().unwrap().params.is_empty());
        for bad in [
            json!({"ids": [sh], "params": {"poles": 2.5}}),
            json!({"ids": [sh], "params": {"form": "sideways"}}),
            json!({"ids": [sh], "params": {"ghost": 1}}),
            json!({"ids": [sh], "params": {"form": []}}),
            json!({"ids": [sh]}),
            json!({"ids": ["nope"], "params": {}}),
        ] {
            assert!(s.run("glyph.setParams", &bad).is_err(), "{bad}");
        }
        // A hand resize stretches until a rebuild draws the true size again.
        let w = s.doc.pages[0].shape(&sh).unwrap().w;
        run(&mut s, "shape.resize", json!({"id": sh, "w": 200, "h": 200}));
        let r = run(&mut s, "glyph.rebuild", json!({}));
        assert_eq!(r["shapes"], json!([sh]));
        assert_eq!(s.doc.pages[0].shape(&sh).unwrap().w, w);
        assert_eq!(run(&mut s, "glyph.rebuild", json!({"ids": [sh]}))["shapes"], json!([]));
        assert!(s.run("glyph.rebuild", &json!({"ids": ["nope"]})).is_err());
    }

    #[test]
    fn builders_and_build_describe_and_refuse_junk() {
        pack();
        let mut s = Session::new(Document::new());
        let all = run(&mut s, "glyph.builders", json!({}));
        // The wave 1 builders, plus those the engineering packs add.
        assert!(all["builders"].as_array().unwrap().len() >= 10);
        let one = run(&mut s, "glyph.builders", json!({"builder": "valve"}));
        let v = &one["builders"][0];
        assert_eq!(v["sources"][0]["standard"], "ISO 1219");
        assert!(v["params"].as_array().unwrap().iter().any(|p| p["key"] == "paths" && p["kind"] == "text"));
        let b = run(&mut s, "glyph.build", json!({"builder": "valve", "params": {"positions": 3, "ports": 4}, "module": 10}));
        assert_eq!((b["w"].as_f64(), b["h"].as_f64(), b["module"].as_f64()), (Some(160.0), Some(60.0), Some(10.0)));
        assert!(b["svg"].as_str().unwrap().starts_with("<svg"));
        // Four fluid ports, then the left solenoid's electrical port.
        assert_eq!(b["ports"].as_array().unwrap().len(), 5);
        let warned = run(&mut s, "glyph.build", json!({"builder": "contact", "params": {"poles": 40}}));
        assert_eq!(warned["warnings"].as_array().unwrap().len(), 1);
        assert!(!s.can_undo(), "building changes no document");
        for bad in [
            json!({}),
            json!({"builder": "nope"}),
            json!({"builder": "valve", "glyph": "glyphtest.contact"}),
            json!({"glyph": "basic.rectangle"}),
            json!({"builder": "valve", "params": 5}),
            json!({"builder": "valve", "module": -1}),
            json!({"builder": "valve", "params": {"x": {"y": 1}}}),
        ] {
            assert!(s.run("glyph.build", &bad).is_err(), "{bad}");
        }
        assert!(s.run("glyph.builders", &json!({"builder": "nope"})).is_err());
        assert!(s.run("glyph.params", &json!({"id": "nope"})).is_err());
    }

    #[test]
    fn a_pack_naming_an_unknown_builder_is_refused() {
        let dir = std::env::temp_dir().join(format!("vizcraft-glyph-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pack = json!({
            "id": "glyphbad", "version": "1.0.0", "name": "Bad",
            "glyphs": [{"id": "glyphbad.x", "label": "X", "category": "T", "outline": "builder", "w": 10, "h": 10, "fill": "#ffffff", "line": "#000000", "builder": "teleporter"}]
        });
        std::fs::write(dir.join("glyphbad.json"), pack.to_string()).unwrap();
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "notation.load", json!({"path": dir.join("glyphbad.json")}));
        assert!(r["pack"].is_null() && r["errors"][0].as_str().unwrap().contains("teleporter"), "{r}");
        let mut ok = pack.clone();
        ok["id"] = json!("glyphwarn");
        ok["glyphs"][0]["id"] = json!("glyphwarn.x");
        ok["glyphs"][0]["builder"] = json!("coil");
        std::fs::write(dir.join("glyphwarn.json"), ok.to_string()).unwrap();
        let r = run(&mut s, "notation.load", json!({"path": dir.join("glyphwarn.json")}));
        assert_eq!(r["pack"]["id"], "glyphwarn");
        assert!(r["warnings"][0].as_str().unwrap().contains("source"), "a missing source only warns: {r}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
