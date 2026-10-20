//! Electrical commands: the project's symbol and designation standards (`electrical.*`),
//! terminal strips (`terminal.*`), cables and their cores (`cable.*`), PLC channels
//! (`io.assign`), footprints on a panel layout and the netlist export (`net.export`).
//!
//! The sync pass (`vizcraft_notation::electrical_pass`) keeps designations, contact links, rungs,
//! terminal wires and coil mirrors in step afterwards, so these commands only set what a person
//! chose. Each command that changes the document is one undo step. Ladder commands are in
//! [`super::ladder`].

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vizcraft_diagram::net::NetIndex;
use vizcraft_diagram::{Document, Shape};
use vizcraft_model::{Element, PropValue, Relationship};
use vizcraft_notation::electrical_pass::{self, CONTACT_OF, TAG_FORMATS};

use super::model::{NewElement, cat, check_len, create_element, element_of, page_param, place_shape};
use super::{MAX_DELTA, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// The symbol families `electrical.setStandards` takes.
pub const SYMBOLS: &[&str] = &["iec", "ansi", "jic"];

/// Most terminals one strip command adds.
pub const MAX_TERMINALS: usize = 200;

/// Most cores one cable has.
pub const MAX_CORES: usize = 200;

/// Most I/O channels on one module.
pub const MAX_CHANNEL: u64 = 1024;

/// Return the electrical commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("electrical.setStandards", "Electrical Standards", "Electrical > Standard", set_standards).params(
            r#"{"symbols"?: "iec"|"ansi"|"jic", "tags"?: "iec81346"|"jic"|"y1444"} -> {"symbols", "tags", "pages": [id]}; sets the project's symbol family (Document.standards.symbols) and designation format (the tagFormat property), redraws every electrical page except panel footprint pages with that family's variant in place (centres, glue, each symbol's direction and the page's symbol grid kept) and recomposes every device tag keeping its number (K1 becomes CR1 under JIC), in one undo step"#,
        ),
        CommandSpec::new("electrical.link", "Link to Coil", "Electrical > Devices", link).params(
            r#"{"contacts"?: [id], "coil": id} -> {"coil", "tag", "contacts": [id]}; gives the contacts (default: the selected contacts) the coil's tag, which links them to it: each contact then shows where the coil is and the coil lists it in its mirror"#,
        ),
        CommandSpec::new("electrical.mirror", "Contact Mirror", "Electrical > Devices", mirror).params(
            r#"{"coil": id} -> {"coil", "tag", "contacts": [{"id", "tag", "form", "ports", "page", "sheet", "zone", "rung"}], "complementNo"?, "complementNc"?}; reads the contacts linked to a coil or overload and where each is drawn; changes nothing"#,
        ),
        CommandSpec::new("electrical.placeFootprints", "Place Footprints", "Electrical > Devices", place_footprints).params(
            r#"{"page"?: page, "ids"?: [element id], "x"?, "y"?, "columns"?: n} -> {"placed": [{"element", "shape"}]}; draws each device (default: every device not yet on the page) on a panel page as its footprint, in rows from x, y; the page should use the panel footprint variant"#,
        ),
        CommandSpec::new("terminal.strip", "Terminal Strip", "Electrical > Devices", strip).params(
            r#"{"page"?: page, "x": n, "y": n, "count": 1..200, "first"?: n (default 1), "orientation"?: "vertical"|"horizontal", "tag"?: string} -> {"strip", "shape", "tag", "terminals": [id], "shapes": [id]}; a terminal strip (X1) holding count terminals numbered from first, each its own element owned by the strip"#,
        ),
        CommandSpec::new("terminal.add", "Add Terminals", "Electrical > Devices", add_terminals).params(
            r#"{"strip": id, "count"?: 1..200} -> {"terminals": [id], "shapes": [id]}; adds terminals after the strip's last, numbered on from its highest number, and grows the strip"#,
        ),
        CommandSpec::new("terminal.renumber", "Renumber Terminals", "Electrical > Devices", renumber_terminals).params(
            r#"{"strip": id, "first"?: n (default 1)} -> {"terminals": [{"id", "number"}]}; numbers the strip's terminals from first in their drawn order (top to bottom, then left to right)"#,
        ),
        CommandSpec::new("terminal.bridge", "Bridge Terminals", "Electrical > Devices", bridge).params(
            r#"{"ids": [terminal id], "bridge": string|null} -> {"ids", "bridge"}; terminals sharing a bridge name are bridged; null removes the bridge"#,
        ),
        CommandSpec::new("cable.create", "Cable", "Electrical > Devices", cable_create).params(
            r#"{"from": element id, "to": element id, "cores": 1..200, "type"?: string, "page"?: page, "x"?: n, "y"?: n} -> {"cable", "tag", "shape"?, "cores": [id], "ends": [relationship id]}; a cable (W1) with its cores numbered 1.., joined to both ends by Cable End relationships; drawn between the ends (or at x, y) when both are on the page"#,
        ),
        CommandSpec::new("cable.assign", "Assign Cores", "Electrical > Devices", cable_assign).params(
            r#"{"cable": id, "nets"?: [net id] | "connectors"?: [wire id], "cores"?: [core id or number]} -> {"assigned": [{"core", "net"}]}; puts each net (or the net of each wire) on a core, in order: the core's net property and the net's cable and core properties"#,
        ),
        CommandSpec::new("io.assign", "Assign I/O", "Electrical > Devices", io_assign).params(
            r#"{"module": id, "channel": 0..1024, "tag"?: string, "signal"?: "DI"|"DO"|"AI"|"AO"} -> {"point", "address", "channel"}; the I/O point of a module's channel (made when missing): its address from the module's base address (I0.0 gives I0.3 for channel 3), the field device tag it serves and its signal; a row of the I/O list"#,
        ),
        CommandSpec::new("net.export", "Export Netlist", "Electrical > Lists", net_export).params(
            r#"{"format"?: "json"|"text", "page"?: page, "path"?: string} -> {"format", "nets", "content"?, "path"?, "bytes"?}; the nets with their pins (device tag and pin) in VizCraft's own netlist format (vizcraft_formats::netlist), written to path or returned as content"#,
        ),
    ]
}

/// Run command `id`'s body directly, inside the caller's undo step (no sync between steps).
pub(crate) fn call(s: &mut Session, id: &str, v: Value) -> CmdResult {
    let run = s.registry.get(id).map(|c| c.run).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
    run(s, &v)
}

/// A clamped coordinate param.
pub(crate) fn coord(v: &Value, k: &str) -> Result<Option<f64>, EngineError> {
    Ok(super::num(v, k)?.map(|n| n.clamp(-MAX_DELTA, MAX_DELTA)))
}

/// Draw shape `shape` on page `page` now with its builder (ports in place), as the rebuild pass
/// would after the command.
pub(crate) fn draw_now(doc: &mut Document, page: usize, shape: &str) {
    let Some(pg) = doc.pages.get(page) else { return };
    let variant = vizcraft_stencils::builders::sync::page_variant(pg).to_string();
    let module = vizcraft_stencils::builders::sync::page_module(pg);
    let element = pg.shape(shape).and_then(|s| s.element.clone()).and_then(|e| doc.model.element(&e).cloned());
    if let Some(s) = doc.pages.get_mut(page).and_then(|pg| pg.shape_mut(shape)) {
        let _ = vizcraft_stencils::builders::sync::redraw(s, element.as_ref(), cat(), &variant, module);
    }
}

/// Make an element of `type_id` and, when `at` is given, draw it on page `page` at once.
pub(crate) fn place_device(
    doc: &mut Document,
    type_id: &str,
    owner: Option<String>,
    props: BTreeMap<String, PropValue>,
    tag: Option<String>,
    at: Option<(usize, f64, f64)>,
) -> Result<(String, String, Option<String>), EngineError> {
    let ne = NewElement { type_id: type_id.to_string(), owner, properties: props, tag, ..NewElement::default() };
    let (id, tag) = create_element(doc, ne)?;
    let shape = match at {
        Some((page, x, y)) => {
            let sid = place_shape(doc, &id, page, x, y, None, None)?;
            draw_now(doc, page, &sid);
            Some(sid)
        }
        None => None,
    };
    Ok((id, tag, shape))
}

/// The element `v[k]` names, which must be of one of `types` (or a subtype).
fn typed_element(s: &Session, v: &Value, k: &str, types: &[&str]) -> Result<String, EngineError> {
    let raw = check_len(k, p::req_str(v, k)?)?;
    let id = element_of(&s.doc, raw).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element", short(raw))))?;
    let e = s.doc.model.element(&id).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element", short(raw))))?;
    let facts = vizcraft_notation::infer::Facts::new(&s.doc.model, cat());
    if !types.iter().any(|t| facts.is_a(&e.type_id, t)) {
        return Err(EngineError::Params(format!("`{k}`: `{}` is a {}, not a {}", short(raw), e.type_id, types.join(" or "))));
    }
    Ok(id)
}

/// The strings of list param `k`: `None` when absent, at most `MAX_IDS`.
pub(crate) fn str_list(v: &Value, k: &str) -> Result<Option<Vec<String>>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= super::MAX_IDS => a
            .iter()
            .map(|x| x.as_str().map(|t| super::model::cut(t, 256)).ok_or_else(|| EngineError::Params(format!("`{k}` must be a list of ids"))))
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be a list of at most {} ids", super::MAX_IDS))),
    }
}

/// The element ids in `v[k]` (shapes resolve to their elements).
fn element_list(s: &Session, v: &Value, k: &str) -> Result<Option<Vec<String>>, EngineError> {
    let Some(list) = str_list(v, k)? else { return Ok(None) };
    list.iter()
        .map(|i| element_of(&s.doc, i).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element", short(i)))))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn set_standards(s: &mut Session, v: &Value) -> CmdResult {
    let symbols = opt_str(v, "symbols")?;
    let tags = opt_str(v, "tags")?;
    if symbols.is_none() && tags.is_none() {
        return Err(EngineError::Params("give `symbols` (iec, ansi, jic) or `tags` (iec81346, jic, y1444)".into()));
    }
    if let Some(sy) = symbols
        && !SYMBOLS.contains(&sy)
    {
        return Err(EngineError::Params(format!("`symbols` must be one of {}, not `{}`", SYMBOLS.join(", "), short(sy))));
    }
    if let Some(t) = tags
        && !TAG_FORMATS.contains(&t)
    {
        return Err(EngineError::Params(format!("`tags` must be one of {}, not `{}`", TAG_FORMATS.join(", "), short(t))));
    }
    let mut pages = Vec::new();
    if let Some(sy) = symbols {
        s.doc.standards.symbols = sy.to_string();
        let variant = format!("{}.{sy}", electrical_pass::PACK);
        let ids: Vec<String> = s.doc.pages.iter().filter(|pg| is_electrical_page(&s.doc, pg)).map(|pg| pg.id.clone()).collect();
        for id in ids {
            keep_directions(&mut s.doc, &id);
            // The page keeps its symbol grid: symbols redraw on the grid their wires were drawn
            // on, so no wire bend falls off a pin (an ANSI symbol on a 2.5 mm grid is a touch
            // smaller, nothing moves).
            let module = s.doc.pages.iter().find(|p| p.id == id).and_then(|p| p.view.as_ref()).map(|v| v.module);
            let pins = first_pins(&s.doc, &id);
            call(s, "variant.set", json!({"page": id, "variant": variant}))?;
            if let (Some(m), Some(view)) = (module, s.doc.pages.iter_mut().find(|p| p.id == id).and_then(|p| p.view.as_mut()))
                && !m.is_none()
            {
                view.module = m;
            }
            restore_pins(&mut s.doc, &id, &pins);
            pages.push(id);
        }
    }
    if let Some(t) = tags {
        s.doc.properties.fields.insert(electrical_pass::TAG_FORMAT.to_string(), t.to_string());
    }
    let tags = electrical_pass::tag_format(&s.doc);
    Ok(json!({"symbols": s.doc.standards.symbols, "tags": tags, "pages": pages}))
}

/// The builders whose symbols take a `direction`.
const DIRECTED: &[&str] = &["contact", "coil", "passive"];

/// Pin each directed builder symbol on page `page` to the direction it is drawn in now (unless
/// the person chose one), so a symbol set drawn the other way (JIC contacts lie across a rung,
/// IEC ones stand upright) redraws in place along the same wires.
fn keep_directions(doc: &mut Document, page: &str) {
    let Some(pg) = doc.pages.iter_mut().find(|p| p.id == page) else { return };
    for sh in pg.shapes.iter_mut() {
        let pts = sh.connection_points_world();
        let Some(g) = sh.glyph.as_mut() else { continue };
        if !DIRECTED.contains(&g.builder.as_str()) || g.params.contains_key("direction") {
            continue;
        }
        let (Some(a), Some(b)) = (pts.first(), pts.get(1)) else { continue };
        let upright = (a.x - b.x).abs() < (a.y - b.y).abs();
        g.params.insert("direction".into(), PropValue::Text(if upright { "vertical" } else { "horizontal" }.into()));
    }
}

/// The first pin of every builder symbol on page `page`, in page units.
fn first_pins(doc: &Document, page: &str) -> Vec<(String, vizcraft_diagram::Point)> {
    let Some(pg) = doc.pages.iter().find(|p| p.id == page) else { return Vec::new() };
    pg.shapes
        .iter()
        .filter(|sh| sh.glyph.as_ref().is_some_and(|g| !g.builder.is_empty()))
        .filter_map(|sh| Some((sh.id.clone(), *sh.connection_points_world().first()?)))
        .collect()
}

/// Redraw each builder symbol of page `page` now and move it so its first pin is back where it
/// was: a symbol whose size differs in the new set stays on its wires.
fn restore_pins(doc: &mut Document, page: &str, pins: &[(String, vizcraft_diagram::Point)]) {
    let Some(pi) = doc.pages.iter().position(|p| p.id == page) else { return };
    for (sid, was) in pins {
        draw_now(doc, pi, sid);
        let Some(sh) = doc.pages.get_mut(pi).and_then(|p| p.shape_mut(sid)) else { continue };
        let Some(now) = sh.connection_points_world().first().copied() else { continue };
        let (dx, dy) = (was.x - now.x, was.y - now.y);
        if dx.is_finite() && dy.is_finite() && (dx.abs() > 1e-9 || dy.abs() > 1e-9) {
            sh.x += dx;
            sh.y += dy;
        }
    }
}

/// Whether `page` draws electrical symbols and is not a panel footprint page.
fn is_electrical_page(doc: &Document, page: &vizcraft_diagram::Page) -> bool {
    let view = page.view.as_ref();
    if view.is_some_and(|v| v.variant == "electrical.footprint") || page.is_background {
        return false;
    }
    view.is_some_and(|v| {
        vizcraft_model::pack_of(&v.viewpoint) == electrical_pass::PACK || vizcraft_model::pack_of(&v.variant) == electrical_pass::PACK
    }) || page.shapes.iter().filter_map(|sh| sh.element.as_deref()).filter_map(|e| doc.model.element(e)).any(|e| e.pack() == electrical_pass::PACK)
}

/// The coil and contact types (`ContactOf` targets and sources).
fn contact_types() -> (Vec<String>, Vec<String>) {
    let mut contacts = Vec::new();
    let mut coils = Vec::new();
    if let Some(pk) = cat().pack(electrical_pass::PACK) {
        for c in pk.connections.iter().filter(|c| c.relationship == CONTACT_OF) {
            contacts.extend(c.source.iter().cloned());
            coils.extend(c.target.iter().cloned());
        }
    }
    (contacts, coils)
}

fn link(s: &mut Session, v: &Value) -> CmdResult {
    let (contact_types, coil_types) = contact_types();
    let coil_refs: Vec<&str> = coil_types.iter().map(String::as_str).collect();
    let coil = typed_element(s, v, "coil", &coil_refs)?;
    let contacts = match element_list(s, v, "contacts")? {
        Some(c) => c,
        None => s.sel.items.iter().filter_map(|i| element_of(&s.doc, i)).collect(),
    };
    let contacts: Vec<String> = contacts.into_iter().filter(|c| s.doc.model.element(c).is_some_and(|e| contact_types.contains(&e.type_id))).collect();
    if contacts.is_empty() {
        return Err(EngineError::Params("no contacts to link: give `contacts` or select them".into()));
    }
    let tag = s.doc.model.element(&coil).map(|e| e.tag.clone()).unwrap_or_default();
    if tag.is_empty() {
        return Err(EngineError::Params("the coil has no tag to give its contacts".into()));
    }
    for c in &contacts {
        if let Some(e) = s.doc.model.element_mut(c) {
            e.tag = tag.clone();
        }
    }
    Ok(json!({"coil": coil, "tag": tag, "contacts": contacts}))
}

fn mirror(s: &mut Session, v: &Value) -> CmdResult {
    let (_, coil_types) = contact_types();
    let refs: Vec<&str> = coil_types.iter().map(String::as_str).collect();
    let coil = typed_element(s, v, "coil", &refs)?;
    let doc = &s.doc;
    let e = doc.model.element(&coil).ok_or_else(|| EngineError::Params("no such coil".into()))?;
    let mut rows = Vec::new();
    for r in doc.model.relationships.iter().filter(|r| r.type_id == CONTACT_OF && r.target == coil) {
        let Some(c) = doc.model.element(&r.source) else { continue };
        let at = vizcraft_notation::sheet_pass::location(doc, &c.id);
        let ports: Vec<String> =
            at.as_ref().and_then(|l| doc.pages.get(l.page_index)?.shape(&l.view)?.glyph.as_ref().map(|g| g.ports.clone())).unwrap_or_default();
        rows.push(json!({
            "id": c.id, "tag": c.tag,
            "form": c.prop("form").map(PropValue::display).unwrap_or_else(|| "no".into()),
            "ports": ports,
            "page": at.as_ref().map(|l| l.page.clone()),
            "sheet": at.as_ref().map(|l| l.sheet),
            "zone": at.as_ref().map(|l| l.zone.clone()),
            "rung": c.prop("rung").map(PropValue::display),
        }));
    }
    let mut out = json!({"coil": coil, "tag": e.tag, "contacts": rows});
    for k in ["complementNo", "complementNc"] {
        if let (Some(x), Some(o)) = (e.prop(k), out.as_object_mut()) {
            o.insert(k.into(), json!(x.display()));
        }
    }
    Ok(out)
}

/// The device types (the `electrical.device` identifier scope).
fn device_types() -> Vec<String> {
    cat()
        .pack(electrical_pass::PACK)
        .and_then(|p| p.identifiers.iter().find(|s| s.id == electrical_pass::DEVICE_SCOPE))
        .map(|s| s.types.clone())
        .unwrap_or_default()
}

fn place_footprints(s: &mut Session, v: &Value) -> CmdResult {
    let page = page_param(&s.doc, v)?;
    let devices = device_types();
    let wanted: Vec<String> = match element_list(s, v, "ids")? {
        Some(ids) => ids,
        None => {
            let pg = s.doc.pages.get(page).ok_or_else(|| EngineError::Params("no such page".into()))?;
            s.doc
                .model
                .elements
                .iter()
                .filter(|e| devices.contains(&e.type_id) && e.type_id != "electrical.Cable")
                .filter(|e| !pg.shapes.iter().any(|sh| sh.element.as_deref() == Some(e.id.as_str())))
                .map(|e| e.id.clone())
                .collect()
        }
    };
    let x0 = coord(v, "x")?.unwrap_or(40.0);
    let y0 = coord(v, "y")?.unwrap_or(40.0);
    let columns = super::num(v, "columns")?.unwrap_or(8.0).clamp(1.0, 100.0) as usize;
    let (mut x, mut y, mut row_h, mut col) = (x0, y0, 0.0f64, 0usize);
    let mut placed = Vec::new();
    for e in wanted.into_iter().take(super::MAX_IDS) {
        let sid = place_shape(&mut s.doc, &e, page, x, y, None, None)?;
        draw_now(&mut s.doc, page, &sid);
        let (w, h) = s.doc.pages.get(page).and_then(|pg| pg.shape(&sid)).map(|sh| (sh.w, sh.h)).unwrap_or((40.0, 40.0));
        placed.push(json!({"element": e, "shape": sid}));
        col += 1;
        row_h = row_h.max(h);
        if col >= columns {
            col = 0;
            x = x0;
            y += row_h + 20.0;
            row_h = 0.0;
        } else {
            x += w + 10.0;
        }
    }
    let shapes: Vec<String> = placed.iter().filter_map(|p| p["shape"].as_str().map(str::to_string)).collect();
    if s.doc.active_index() == page {
        select(s, shapes);
    }
    with_sel(s, json!({"placed": placed}))
}

/// The number a terminal shows: its `number` property as a whole number.
fn terminal_number(e: &Element) -> Option<i64> {
    e.prop("number").and_then(|v| v.display().trim().parse::<i64>().ok())
}

/// Lay out `count` terminals of strip `strip` from number `first` on page `page`, starting at
/// `(x, y)`, `step` apart along `vertical` or across. Returns the new element and shape ids.
#[allow(clippy::too_many_arguments)]
fn add_terminal_shapes(
    doc: &mut Document,
    strip: &str,
    page: usize,
    x: f64,
    y: f64,
    count: usize,
    first: i64,
    vertical: bool,
) -> Result<(Vec<String>, Vec<String>), EngineError> {
    let mut ids = Vec::new();
    let mut shapes = Vec::new();
    let mut at = (x, y);
    for k in 0..count {
        let n = first.saturating_add(k as i64);
        let props: BTreeMap<String, PropValue> = [("number".to_string(), PropValue::Text(n.to_string()))].into();
        let owner_tag = doc.model.element(strip).map(|e| e.tag.clone()).unwrap_or_default();
        let tag = Some(format!("{owner_tag}:{n}"));
        let (id, _, shape) = place_device(doc, "electrical.Terminal", Some(strip.to_string()), props, tag, Some((page, at.0, at.1)))?;
        let (w, h) = shape.as_deref().and_then(|sid| doc.pages.get(page)?.shape(sid)).map(|sh| (sh.w, sh.h)).unwrap_or((40.0, 20.0));
        if let Some(sid) = shape {
            if let Some(sh) = doc.pages.get_mut(page).and_then(|pg| pg.shape_mut(&sid)) {
                sh.parent = None;
            }
            shapes.push(sid);
        }
        ids.push(id);
        if vertical {
            at.1 += h;
        } else {
            at.0 += w + h;
        }
    }
    Ok((ids, shapes))
}

/// Fit strip shape `shape` around the shapes of its terminals with a margin.
fn fit_strip(doc: &mut Document, page: usize, strip: &str, shape: &str) {
    let Some(pg) = doc.pages.get(page) else { return };
    let members: Vec<&Shape> = pg
        .shapes
        .iter()
        .filter(|sh| sh.element.as_deref().and_then(|e| doc.model.element(e)).is_some_and(|e| e.owner.as_deref() == Some(strip)))
        .collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for m in &members {
        x0 = x0.min(m.x);
        y0 = y0.min(m.y);
        x1 = x1.max(m.x + m.w);
        y1 = y1.max(m.y + m.h);
    }
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return;
    }
    let pad = 6.0;
    let band = 22.0;
    if let Some(sh) = doc.pages.get_mut(page).and_then(|pg| pg.shape_mut(shape)) {
        sh.x = x0 - pad;
        sh.y = y0 - pad - band;
        sh.w = (x1 - x0) + 2.0 * pad;
        sh.h = (y1 - y0) + 2.0 * pad + band;
        sh.sanitize();
    }
}

fn strip(s: &mut Session, v: &Value) -> CmdResult {
    let page = page_param(&s.doc, v)?;
    let x = coord(v, "x")?.ok_or_else(|| EngineError::Params("`x` is required".into()))?;
    let y = coord(v, "y")?.ok_or_else(|| EngineError::Params("`y` is required".into()))?;
    let count = p::usize(v, "count").ok_or_else(|| EngineError::Params("`count` (1 to 200) is required".into()))?;
    if !(1..=MAX_TERMINALS).contains(&count) {
        return Err(EngineError::Params(format!("`count` must be 1 to {MAX_TERMINALS}")));
    }
    let first = super::num(v, "first")?.unwrap_or(1.0).clamp(0.0, 99_999.0).round() as i64;
    let vertical = match opt_str(v, "orientation")?.unwrap_or("vertical") {
        "vertical" => true,
        "horizontal" => false,
        o => return Err(EngineError::Params(format!("`orientation` must be vertical or horizontal, not `{}`", short(o)))),
    };
    let tag = opt_str(v, "tag")?.map(|t| super::model::cut(t, vizcraft_model::MAX_TAG));
    let (strip_id, strip_tag, strip_shape) = place_device(&mut s.doc, "electrical.TerminalStrip", None, BTreeMap::new(), tag, Some((page, x, y)))?;
    let strip_shape = strip_shape.ok_or_else(|| EngineError::Failed("the strip was not drawn".into()))?;
    let (terminals, shapes) = add_terminal_shapes(&mut s.doc, &strip_id, page, x + 6.0, y + 28.0, count, first, vertical)?;
    fit_strip(&mut s.doc, page, &strip_id, &strip_shape);
    if s.doc.active_index() == page {
        select(s, vec![strip_shape.clone()]);
    }
    with_sel(s, json!({"strip": strip_id, "shape": strip_shape, "tag": strip_tag, "terminals": terminals, "shapes": shapes}))
}

/// The strip's terminals with their shapes on the strip's page, in drawn order.
fn terminals_of(doc: &Document, strip: &str) -> Vec<(String, Option<(usize, String, f64, f64)>)> {
    let mut out: Vec<(String, Option<(usize, String, f64, f64)>)> = doc
        .model
        .elements
        .iter()
        .filter(|e| e.type_id == "electrical.Terminal" && e.owner.as_deref() == Some(strip))
        .map(|e| {
            let view = doc.pages.iter().enumerate().find_map(|(pi, pg)| {
                pg.shapes.iter().find(|sh| sh.element.as_deref() == Some(e.id.as_str())).map(|sh| (pi, sh.id.clone(), sh.x, sh.y))
            });
            (e.id.clone(), view)
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |v: &Option<(usize, String, f64, f64)>| v.as_ref().map(|(p, _, x, y)| (*p, *y, *x)).unwrap_or((usize::MAX, 0.0, 0.0));
        let (ka, kb) = (key(&a.1), key(&b.1));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(ka.2.total_cmp(&kb.2))
    });
    out
}

fn add_terminals(s: &mut Session, v: &Value) -> CmdResult {
    let strip = typed_element(s, v, "strip", &["electrical.TerminalStrip"])?;
    let count = p::usize(v, "count").unwrap_or(1);
    if !(1..=MAX_TERMINALS).contains(&count) {
        return Err(EngineError::Params(format!("`count` must be 1 to {MAX_TERMINALS}")));
    }
    let list = terminals_of(&s.doc, &strip);
    let highest = list.iter().filter_map(|(id, _)| s.doc.model.element(id).and_then(terminal_number)).max().unwrap_or(0);
    let strip_view = s
        .doc
        .pages
        .iter()
        .enumerate()
        .find_map(|(pi, pg)| pg.shapes.iter().find(|sh| sh.element.as_deref() == Some(strip.as_str())).map(|sh| (pi, sh.id.clone(), sh.x, sh.y)));
    let Some((page, strip_shape, sx, sy)) = strip_view else {
        return Err(EngineError::Params("the strip is not drawn; draw it first".into()));
    };
    // Below (or right of) the last terminal, in the direction the strip runs.
    let last: Vec<&Shape> = list
        .iter()
        .filter_map(|(_, view)| view.as_ref())
        .filter(|(p, ..)| *p == page)
        .filter_map(|(_, sid, ..)| s.doc.pages.get(page)?.shape(sid))
        .collect();
    let vertical = last.len() < 2 || last.windows(2).all(|w| w.first().map(|a| a.x) == w.get(1).map(|b| b.x));
    let (x, y) = match last.last() {
        Some(l) if vertical => (l.x, l.y + l.h),
        Some(l) => (l.x + l.w + l.h, l.y),
        None => (sx + 6.0, sy + 28.0),
    };
    let (terminals, shapes) = add_terminal_shapes(&mut s.doc, &strip, page, x, y, count, highest.saturating_add(1), vertical)?;
    fit_strip(&mut s.doc, page, &strip, &strip_shape);
    with_sel(s, json!({"terminals": terminals, "shapes": shapes}))
}

fn renumber_terminals(s: &mut Session, v: &Value) -> CmdResult {
    let strip = typed_element(s, v, "strip", &["electrical.TerminalStrip"])?;
    let first = super::num(v, "first")?.unwrap_or(1.0).clamp(0.0, 99_999.0).round() as i64;
    let tag = s.doc.model.element(&strip).map(|e| e.tag.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for (k, (id, _)) in terminals_of(&s.doc, &strip).into_iter().enumerate() {
        let n = first.saturating_add(k as i64);
        if let Some(e) = s.doc.model.element_mut(&id) {
            e.properties.insert("number".into(), PropValue::Text(n.to_string()));
            e.tag = format!("{tag}:{n}");
        }
        out.push(json!({"id": id, "number": n.to_string()}));
    }
    Ok(json!({"terminals": out}))
}

fn bridge(s: &mut Session, v: &Value) -> CmdResult {
    let ids = element_list(s, v, "ids")?.ok_or_else(|| EngineError::Params("`ids` (terminal ids) is required".into()))?;
    let bridge = match v.get("bridge") {
        None => return Err(EngineError::Params("`bridge` (a name, or null to remove) is required".into())),
        Some(Value::Null) => None,
        Some(Value::String(b)) if b.trim().is_empty() => None,
        Some(Value::String(b)) => Some(super::model::cut(b.trim(), 64)),
        Some(_) => return Err(EngineError::Params("`bridge` must be text or null".into())),
    };
    for id in &ids {
        let e = s.doc.model.element(id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(id))))?;
        if e.type_id != "electrical.Terminal" {
            return Err(EngineError::Params(format!("`{}` is not a terminal", short(id))));
        }
    }
    for id in &ids {
        if let Some(e) = s.doc.model.element_mut(id) {
            match &bridge {
                Some(b) => {
                    e.properties.insert("bridge".into(), PropValue::Text(b.clone()));
                }
                None => {
                    e.properties.remove("bridge");
                }
            }
        }
    }
    Ok(json!({"ids": ids, "bridge": bridge}))
}

/// The first view of element `e`: page index and shape.
fn first_view<'a>(doc: &'a Document, e: &str) -> Option<(usize, &'a Shape)> {
    doc.pages.iter().enumerate().find_map(|(pi, pg)| pg.shapes.iter().find(|sh| sh.element.as_deref() == Some(e)).map(|sh| (pi, sh)))
}

fn cable_create(s: &mut Session, v: &Value) -> CmdResult {
    let from_raw = check_len("from", p::req_str(v, "from")?)?;
    let to_raw = check_len("to", p::req_str(v, "to")?)?;
    let from = element_of(&s.doc, from_raw).ok_or_else(|| EngineError::Params(format!("`from`: `{}` is not an element", short(from_raw))))?;
    let to = element_of(&s.doc, to_raw).ok_or_else(|| EngineError::Params(format!("`to`: `{}` is not an element", short(to_raw))))?;
    if from == to {
        return Err(EngineError::Params("a cable joins two different ends".into()));
    }
    let cores = p::usize(v, "cores").ok_or_else(|| EngineError::Params("`cores` (1 to 200) is required".into()))?;
    if !(1..=MAX_CORES).contains(&cores) {
        return Err(EngineError::Params(format!("`cores` must be 1 to {MAX_CORES}")));
    }
    let mut props: BTreeMap<String, PropValue> = [("cores".to_string(), PropValue::Number(cores as f64))].into();
    if let Some(t) = opt_str(v, "type")? {
        props.insert("type".into(), PropValue::Text(super::model::cut(t, 128)));
    }
    // Drawn between its ends when both are on one page, or at x, y.
    let page = page_param(&s.doc, v)?;
    let ends = (first_view(&s.doc, &from), first_view(&s.doc, &to));
    let at = match (coord(v, "x")?, coord(v, "y")?, ends) {
        (Some(x), Some(y), _) => Some((page, x, y)),
        (_, _, (Some((pa, a)), Some((pb, b)))) if pa == pb => {
            let (ca, cb) = (a.center(), b.center());
            Some((pa, (ca.x + cb.x) / 2.0 - 39.0, (ca.y + cb.y) / 2.0 - 10.0))
        }
        _ => None,
    };
    let (cable, tag, shape) = place_device(&mut s.doc, "electrical.Cable", None, props, None, at)?;
    let mut core_ids = Vec::new();
    for n in 1..=cores {
        let props: BTreeMap<String, PropValue> = [("number".to_string(), PropValue::Text(n.to_string()))].into();
        let (id, _, _) = place_device(&mut s.doc, "electrical.Core", Some(cable.clone()), props, Some(format!("{tag}.{n}")), None)?;
        core_ids.push(id);
    }
    let mut rels = Vec::new();
    for end in [&from, &to] {
        if shape.is_some() {
            let r = call(s, "relationship.create", json!({"from": cable, "to": end, "type": "electrical.CableEnd"}))?;
            if let Some(id) = r["id"].as_str() {
                rels.push(id.to_string());
            }
        } else {
            let id = s.doc.next_id("r");
            s.doc
                .model
                .add_relationship(Relationship::new(id.clone(), "electrical.CableEnd", cable.clone(), end.clone()))
                .map_err(super::model::model_err)?;
            rels.push(id);
        }
    }
    if let Some(sid) = &shape
        && at.is_some_and(|(p, ..)| p == s.doc.active_index())
    {
        select(s, vec![sid.clone()]);
    }
    let mut out = json!({"cable": cable, "tag": tag, "cores": core_ids, "ends": rels});
    if let (Some(sid), Some(o)) = (shape, out.as_object_mut()) {
        o.insert("shape".into(), json!(sid));
    }
    with_sel(s, out)
}

fn cable_assign(s: &mut Session, v: &Value) -> CmdResult {
    let cable = typed_element(s, v, "cable", &["electrical.Cable"])?;
    let nets: Vec<String> = match (v.get("nets"), v.get("connectors")) {
        (Some(Value::Array(a)), _) => a
            .iter()
            .take(MAX_CORES)
            .map(|x| {
                let n = x.as_str().ok_or_else(|| EngineError::Params("`nets` must be net ids".into()))?;
                s.doc.model.net(n).map(|n| n.id.clone()).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(n))))
            })
            .collect::<Result<_, _>>()?,
        (_, Some(Value::Array(a))) => {
            let ix = NetIndex::build(&s.doc);
            a.iter()
                .take(MAX_CORES)
                .map(|x| {
                    let c = x.as_str().ok_or_else(|| EngineError::Params("`connectors` must be wire ids".into()))?;
                    ix.net_of_connector(c).map(str::to_string).ok_or_else(|| EngineError::Params(format!("`{}` is not a wire of a net", short(c))))
                })
                .collect::<Result<_, _>>()?
        }
        _ => return Err(EngineError::Params("give `nets` (net ids) or `connectors` (wire ids)".into())),
    };
    let mut cores: Vec<&Element> =
        s.doc.model.elements.iter().filter(|e| e.owner.as_deref() == Some(cable.as_str()) && e.type_id == "electrical.Core").collect();
    cores.sort_by_key(|c| c.prop("number").and_then(|n| n.display().parse::<u64>().ok()).unwrap_or(u64::MAX));
    let chosen: Vec<String> = match v.get("cores") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                let want = match x {
                    Value::Number(n) => n.to_string(),
                    Value::String(t) => t.clone(),
                    _ => String::new(),
                };
                cores
                    .iter()
                    .find(|c| c.id == want || c.prop("number").map(PropValue::display).as_deref() == Some(want.as_str()))
                    .map(|c| c.id.clone())
                    .ok_or_else(|| EngineError::Params(format!("the cable has no core `{}`", short(&want))))
            })
            .collect::<Result<_, _>>()?,
        _ => cores.iter().map(|c| c.id.clone()).collect(),
    };
    if nets.len() > chosen.len() {
        return Err(EngineError::Params(format!("{} nets for {} cores", nets.len(), chosen.len())));
    }
    let cable_tag = s.doc.model.element(&cable).map(|e| e.tag.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for (net, core) in nets.iter().zip(&chosen) {
        let number = s.doc.model.element(core).and_then(|c| c.prop("number")).map(PropValue::display).unwrap_or_default();
        let label = s
            .doc
            .model
            .net(net)
            .map(|n| if n.number.is_empty() { if n.name.is_empty() { n.id.clone() } else { n.name.clone() } } else { n.number.clone() });
        if let Some(c) = s.doc.model.element_mut(core) {
            c.properties.insert("net".into(), PropValue::Text(label.unwrap_or_default()));
        }
        if let Some(n) = s.doc.model.net_mut(net) {
            n.properties.insert("cable".into(), PropValue::Text(cable_tag.clone()));
            n.properties.insert("core".into(), PropValue::Text(number));
        }
        out.push(json!({"core": core, "net": net}));
    }
    Ok(json!({"assigned": out}))
}

/// The address of channel `channel` from a module's base address: the digits after the last
/// `.` are replaced (`I0.0` gives `I0.3`), else the channel is appended to the base or tag.
pub fn channel_address(base: &str, tag: &str, channel: u64) -> String {
    match base.rsplit_once('.') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => format!("{head}.{channel}"),
        _ if !base.is_empty() => format!("{base}{channel}"),
        _ => format!("{tag}.{channel}"),
    }
}

fn io_assign(s: &mut Session, v: &Value) -> CmdResult {
    let module = typed_element(s, v, "module", &["electrical.IoModule"])?;
    let channel = p::u64(v, "channel").ok_or_else(|| EngineError::Params("`channel` (0 to 1024) is required".into()))?;
    if channel > MAX_CHANNEL {
        return Err(EngineError::Params(format!("`channel` must be 0 to {MAX_CHANNEL}")));
    }
    let signal = match opt_str(v, "signal")? {
        Some(sg) if ["DI", "DO", "AI", "AO"].contains(&sg) => Some(sg.to_string()),
        Some(sg) => return Err(EngineError::Params(format!("`signal` must be DI, DO, AI or AO, not `{}`", short(sg)))),
        None => None,
    };
    let device = opt_str(v, "tag")?.map(|t| super::model::cut(t, vizcraft_model::MAX_TAG));
    let m = s.doc.model.element(&module).ok_or_else(|| EngineError::Params("no such module".into()))?;
    let address = channel_address(&m.prop("address").map(PropValue::display).unwrap_or_default(), &m.tag, channel);
    let signal = signal.or_else(|| m.prop("signal").map(PropValue::display)).unwrap_or_else(|| "DI".into());
    let module_tag = m.tag.clone();
    let existing = s.doc.model.elements.iter().find(|e| {
        e.type_id == "electrical.IoPoint"
            && e.owner.as_deref() == Some(module.as_str())
            && e.prop("channel").and_then(PropValue::as_f64).is_some_and(|c| c == channel as f64)
    });
    let point = match existing.map(|e| e.id.clone()) {
        Some(id) => id,
        None => {
            let (id, _, _) = place_device(&mut s.doc, "electrical.IoPoint", Some(module.clone()), BTreeMap::new(), Some(address.clone()), None)?;
            id
        }
    };
    if let Some(e) = s.doc.model.element_mut(&point) {
        e.tag = address.clone();
        e.properties.insert("module".into(), PropValue::Text(module_tag));
        e.properties.insert("channel".into(), PropValue::Number(channel as f64));
        e.properties.insert("address".into(), PropValue::Text(address.clone()));
        e.properties.insert("signal".into(), PropValue::Text(signal));
        if let Some(d) = device {
            e.properties.insert("device".into(), PropValue::Text(d.clone()));
            e.name = d;
        }
    }
    Ok(json!({"point": point, "address": address, "channel": channel}))
}

fn net_export(s: &mut Session, v: &Value) -> CmdResult {
    let format = opt_str(v, "format")?.unwrap_or("json");
    if !matches!(format, "json" | "text") {
        return Err(EngineError::Params(format!("`format` must be json or text, not `{}`", short(format))));
    }
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let list = netlist(&s.doc, page);
    let content = if format == "json" {
        serde_json::to_string_pretty(&vizcraft_formats::netlist::to_json(&list)).map_err(|e| EngineError::Failed(e.to_string()))?
    } else {
        vizcraft_formats::netlist::to_text(&list)
    };
    let mut out = json!({"format": format, "nets": list.nets.len()});
    if let Some(o) = out.as_object_mut() {
        match opt_str(v, "path")? {
            Some(path) => {
                std::fs::write(path, content.as_bytes()).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
                o.insert("path".into(), json!(path));
                o.insert("bytes".into(), json!(content.len()));
            }
            None => {
                o.insert("content".into(), json!(content));
            }
        }
    }
    Ok(out)
}

/// The document's nets as a netlist: each net with its pins (device tag and port), on every page
/// or on page `page`.
pub fn netlist(doc: &Document, page: Option<usize>) -> vizcraft_formats::netlist::Netlist {
    use vizcraft_formats::netlist::{Netlist, NetlistNet, NetlistPin};
    let ix = NetIndex::build(doc);
    let page_id = page.and_then(|i| doc.pages.get(i)).map(|p| p.id.clone());
    let mut nets = Vec::new();
    for n in &doc.model.nets {
        let Some(entry) = ix.nets.get(&n.id) else { continue };
        if page_id.as_ref().is_some_and(|pid| !entry.pages.contains(pid)) || entry.segments.is_empty() {
            continue;
        }
        let mut pins = Vec::new();
        for (shape, point) in &entry.pins {
            let Some(sh) = doc.pages.iter().find_map(|p| p.shape(shape)) else { continue };
            let Some(e) = sh.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            let port = vizcraft_notation::electrical_rules::port_id(cat(), sh, e, *point).unwrap_or_else(|| point.to_string());
            let device = if e.tag.is_empty() { e.id.clone() } else { e.tag.clone() };
            pins.push(NetlistPin { device, pin: port, element: e.id.clone() });
        }
        nets.push(NetlistNet {
            id: n.id.clone(),
            number: n.number.clone(),
            name: n.name.clone(),
            kind: n.kind.name().to_string(),
            properties: n.properties.iter().map(|(k, v)| (k.clone(), v.display())).collect(),
            pins,
        });
    }
    Netlist { title: doc.properties.fields.get("title").cloned().unwrap_or_else(|| doc.title.clone()), nets }
}

#[cfg(test)]
#[path = "electrical/tests.rs"]
mod tests;
