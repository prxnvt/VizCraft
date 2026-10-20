//! Process plant commands (`plant.*`, `line.assign`, `offpage.*`, and the body of
//! `loop.diagram`): nozzles on equipment, pipe lines, off-page pairs, loop numbering, the
//! process lists as live tables, isometric runs and the DEXPI class map.
//!
//! The prefix is `plant.*` because `process.*` is the Visio-style Process tab's
//! (`process.createSubprocess`). Commands that place several items build them here directly
//! (or run another command's body with [`call`]), so each stays one undo step; the sync pass
//! that follows numbers new pipes into lines and composes the tags.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, End, Point, Route};
use vizcraft_model::{Element, PropValue};
use vizcraft_notation::infer::Facts;
use vizcraft_notation::process_pass::{EXT_LINE, is_line_type, is_pipe, kind_nets, new_line};
use vizcraft_notation::process_rules::{COMPONENT, CONTINUES, INSTRUMENT, LOOP, PRIMARY};

use super::model::{NewElement, cat, check_len, create_element, element_of, page_param, place_shape};
use super::{MAX_DELTA, MAX_IDS, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// The nozzle element type.
pub const NOZZLE: &str = "process.Nozzle";
/// The pipe off-page connector type.
pub const PIPE_OFF_PAGE: &str = "process.PipeOffPageConnector";
/// The signal off-page connector type.
pub const SIGNAL_OFF_PAGE: &str = "process.SignalOffPageConnector";
/// The equipment supertype.
pub const EQUIPMENT: &str = "process.Equipment";
/// The lists `plant.list` makes.
pub const LISTS: &[&str] = &["line", "equipment", "instrument", "valve", "io", "stream", "nozzle", "fixtureUnits", "bom"];
/// The isometric directions `plant.isoRun` takes.
pub const ISO_DIRECTIONS: &[&str] = &["n", "s", "e", "w", "up", "down"];
/// Longest isometric leg, in page units.
const MAX_LEG: f64 = 100_000.0;
/// Most legs one run draws.
const MAX_LEGS: usize = 256;
/// The 2.5 mm module, the nozzle's size in page units comes from its glyph.
const NOZZLE_STEPS: [f64; 5] = [0.5, 0.25, 0.75, 0.125, 0.875];

/// Return the plant commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("line.assign", "Assign Line", "Notation > Lines", line_assign).params(
            r#"{"connectors"?: [connector id] (default: the selected pipes), "net"?: net id (its pipe segments), "line"?: line element id or "new"} -> {"line", "tag", "connectors"}; the pipe segments carry that line (its number); with no line, the line they already share (to read its id), else a new one; "new" always starts a new line; one undo step when anything changes"#,
        )
        .when(super::has_connectors),
        CommandSpec::new("offpage.pair", "Pair Off-page", "Notation > Lines", pair).params(
            r#"{"a"?: off-page connector (element or shape id), "b"?: another of the same type} (default: the two selected) -> {"tag", "relationship", "net"?, "line"?}; both take one tag (a's when it is free, else the next number), a Continues relationship joins them so each shows where the other is, and their pipe nets merge into one named net; b's pipes take a's line"#,
        )
        ,
        CommandSpec::new("offpage.place", "Place Off-page", "Notation > Lines", place).params(
            r#"{"connector"?: pipe connector id (default: the selected one), "end"?: "start"|"end" (default: its free end), "toPage"?: page id, name or index (default: the next sheet), "x"?: n, "y"?: n} -> {"a", "b", "shapes": [id, id], "tag"}; puts an off-page connector at the pipe's free end, its partner on the other page (at x, y, default the left margin at the same height) and pairs them"#,
        )
        .when(super::has_connectors),
        CommandSpec::new("plant.addNozzle", "Add Nozzle", "Notation > Equipment", add_nozzle).params(
            r#"{"shape"?: equipment shape or element id (default: the selected shape), "side": "left"|"right"|"top"|"bottom", "at"?: 0..1 along the side (default: the middle, else the next free place), "subTag"?: string (default N1, N2, ...), "nominalDiameter"?: string} -> {"id", "shape", "tag"}; a nozzle owned by the equipment, set on that side facing out and moving with it; its tag is the equipment's and its sub-tag (V-101-N1)"#,
        )
        .when(super::has_shapes),
        CommandSpec::new("plant.numbering", "Loop Numbering", "Notation > Tags", numbering).params(
            r#"{"scheme"?: "parallel"|"serial"} -> {"scheme"}; parallel numbers loops apart per measured variable (F-101 and L-101), serial in one sequence (ANSI/ISA-5.1, Annex A); without scheme it reads the current one; setting it is one undo step"#,
        ),
        CommandSpec::new("plant.list", "Plant List", "Notation > Lists", list).params(
            r#"{"list": "line"|"equipment"|"instrument"|"valve"|"io"|"stream"|"nozzle"|"fixtureUnits"|"bom", "page"?: page (only what it draws), "format"?: "csv"|"xlsx"|"markdown", "path"?: string} -> {"list", "columns", "keys", "ids", "rows", "editable", "count", "format"?, "content"?, "path"?, "bytes"?}; the process lists as live tables over the model (line list, equipment list, instrument index, valve list, I/O list, stream table, nozzle list, fixture units, bill of materials); an editable column's key is what table.set writes; with format it exports through the CSV, XLSX and Markdown writers (xlsx needs a path); read-only"#,
        ),
        CommandSpec::new("plant.isoRun", "Isometric Run", "Notation > Lines", iso_run).params(
            r#"{"from": {"x", "y"} or {"shape", "point"?}, "legs": [{"dir": "n"|"s"|"e"|"w"|"up"|"down", "length": page units}], "kind"?: connector type (default process.processLine), "line"?: line element id, "page"?: page} -> {"connectors", "elbows", "line", "end": {"x", "y"}}; straight pipe legs along the 30 degree isometric axes with an elbow at each turn, all on one line; one undo step"#,
        ),
        CommandSpec::new("plant.dexpiMap", "DEXPI Classes", "Notation > Lists", dexpi_map).params(
            r#"{"type"?: process element type} -> {"mappings": [{"type", "dexpiClass", "uri"?}], "unmapped": [type]}; the DEXPI 1.4 class each process type stands for, where the name was checked; read-only"#,
        ),
    ]
}

/// Run the body of registered command `id` with `params` inside the current command, so its
/// change joins this command's undo step (the sync pass runs once, after the caller).
pub(crate) fn call(s: &mut Session, id: &str, params: Value) -> CmdResult {
    let run = s.registry.get(id).map(|c| c.run).ok_or_else(|| EngineError::Failed(format!("`{id}` is not registered")))?;
    run(s, &params)
}

fn text(e: &Element, k: &str) -> String {
    e.prop(k).map(PropValue::display).unwrap_or_default().trim().to_string()
}

// ---------------------------------------------------------------- lines

fn line_assign(s: &mut Session, v: &Value) -> CmdResult {
    let kinds = kind_nets(cat());
    let page_ix = s.doc.active_index();
    let ids: Vec<String> = match (v.get("connectors").filter(|x| !x.is_null()), opt_str(v, "net")?) {
        (Some(_), Some(_)) => return Err(EngineError::Params("give `connectors` or `net`, not both".into())),
        (Some(raw), None) => {
            let arr = raw
                .as_array()
                .filter(|a| a.len() <= MAX_IDS)
                .ok_or_else(|| EngineError::Params("`connectors` must be a list of connector ids".into()))?;
            arr.iter()
                .map(|x| x.as_str().map(str::to_string).ok_or_else(|| EngineError::Params("`connectors` must hold ids".into())))
                .collect::<Result<_, _>>()?
        }
        (None, Some(n)) => {
            let n = check_len("net", n)?;
            s.doc.model.net(n).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(n))))?;
            s.doc.page().map(|pg| pg.connectors.iter().filter(|c| c.net.as_deref() == Some(n)).map(|c| c.id.clone()).collect()).unwrap_or_default()
        }
        (None, None) => s.sel.items.clone(),
    };
    let page = s.doc.pages.get(page_ix).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut pipes = Vec::new();
    for id in &ids {
        let c = page.connector(id).ok_or_else(|| EngineError::Params(format!("`{}` is not a connector on this page", short(id))))?;
        if !is_pipe(&s.doc, &kinds, c) {
            return Err(EngineError::Params(format!("`{}` is not a pipe", short(id))));
        }
        pipes.push(id.clone());
    }
    if pipes.is_empty() {
        return Err(EngineError::Params("no pipe to assign: select pipes or name them".into()));
    }
    // Without a line, the one the segments already share, else a new one.
    let shared = {
        let lines: HashSet<Option<&String>> = pipes.iter().map(|id| page.connector(id).and_then(|c| c.ext.get(EXT_LINE))).collect();
        match lines.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(l)] if s.doc.model.element(l).is_some_and(|e| is_line_type(&e.type_id)) => Some((*l).clone()),
            _ => None,
        }
    };
    let line = match (opt_str(v, "line")?, shared) {
        (Some(l), _) if l != "new" => {
            let l = check_len("line", l)?;
            s.doc
                .model
                .element(l)
                .filter(|e| is_line_type(&e.type_id))
                .ok_or_else(|| EngineError::Params(format!("`{}` is not a line", short(l))))?;
            l.to_string()
        }
        (None, Some(l)) => l,
        _ => {
            let air = pipes.iter().any(|id| {
                s.doc
                    .pages
                    .get(page_ix)
                    .and_then(|pg| pg.connector(id))
                    .is_some_and(|c| vizcraft_notation::process_pass::AIR_KINDS.contains(&c.kind.as_str()))
            });
            let ty = if air { vizcraft_notation::process_pass::DUCT } else { vizcraft_notation::process_pass::LINE };
            new_line(&mut s.doc, cat(), ty, &BTreeMap::new())
                .ok_or_else(|| EngineError::Failed("the model already holds the most elements it can".into()))?
        }
    };
    if let Some(pg) = s.doc.pages.get_mut(page_ix) {
        for id in &pipes {
            if let Some(c) = pg.connector_mut(id) {
                c.ext.insert(EXT_LINE.to_string(), line.clone());
            }
        }
    }
    let tag = s.doc.model.element(&line).map(|e| e.tag.clone()).unwrap_or_default();
    Ok(json!({"line": line, "tag": tag, "connectors": pipes}))
}

// ---------------------------------------------------------------- off-page pairs

/// The off-page connector element `k` names, and its type.
fn off_page(s: &Session, v: &Value, k: &str, nth: usize) -> Result<(String, String), EngineError> {
    // Without the param, the selection's `nth` shape.
    let picked = match opt_str(v, k)? {
        Some(id) => id.to_string(),
        None => {
            let page = s.doc.page();
            let shapes: Vec<&String> = s.sel.items.iter().filter(|i| page.is_some_and(|p| p.shape(i).is_some())).collect();
            shapes.get(nth).map(|x| (*x).clone()).ok_or_else(|| EngineError::Params(format!("select two off-page connectors, or name `{k}`")))?
        }
    };
    let id = check_len(k, &picked)?;
    let e =
        element_of(&s.doc, id).and_then(|e| s.doc.model.element(&e).cloned()).filter(|e| e.type_id == PIPE_OFF_PAGE || e.type_id == SIGNAL_OFF_PAGE);
    let e = e.ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an off-page connector", short(id))))?;
    Ok((e.id, e.type_id))
}

/// The number of the next pair in an off-page scope: one more than the highest numeric tag.
fn next_pair_tag(doc: &Document, type_id: &str) -> String {
    let highest = doc.model.elements.iter().filter(|e| e.type_id == type_id).filter_map(|e| e.tag.trim().parse::<u64>().ok()).max().unwrap_or(0);
    highest.saturating_add(1).to_string()
}

/// Connectors glued to any view of element `e`, as (page index, connector id).
fn glued_to(doc: &Document, e: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for sh in page.shapes.iter().filter(|sh| sh.element.as_deref() == Some(e)) {
            for c in page.connectors.iter().filter(|c| c.touches(&sh.id)) {
                out.push((pi, c.id.clone()));
            }
        }
    }
    out
}

/// Pair two off-page connectors: one tag, a Continues relationship, the pipe nets merged and
/// b's pipes on a's line. Returns the result object.
fn pair_elements(s: &mut Session, a: &str, b: &str, ty: &str) -> Result<Value, EngineError> {
    if a == b {
        return Err(EngineError::Params("an off-page connector cannot pair with itself".into()));
    }
    let tag_a = s.doc.model.element(a).map(|e| e.tag.trim().to_string()).unwrap_or_default();
    let tag_b = s.doc.model.element(b).map(|e| e.tag.trim().to_string()).unwrap_or_default();
    let used = |t: &str, doc: &Document| doc.model.elements.iter().any(|e| e.type_id == ty && e.id != a && e.id != b && e.tag.trim() == t);
    let tag = if !tag_a.is_empty() && !used(&tag_a, &s.doc) {
        tag_a
    } else if !tag_b.is_empty() && !used(&tag_b, &s.doc) {
        tag_b
    } else {
        next_pair_tag(&s.doc, ty)
    };
    for id in [a, b] {
        if let Some(e) = s.doc.model.element_mut(id) {
            e.tag = tag.clone();
        }
    }
    // One Continues relationship between them.
    let existing = s
        .doc
        .model
        .relationships
        .iter()
        .find(|r| r.type_id == CONTINUES && ((r.source == a && r.target == b) || (r.source == b && r.target == a)))
        .map(|r| r.id.clone());
    let rel = match existing {
        Some(r) => r,
        None => {
            let rid = s.doc.next_id("r");
            s.doc.model.add_relationship(vizcraft_model::Relationship::new(rid.clone(), CONTINUES, a, b)).map_err(super::model::model_err)?;
            rid
        }
    };
    let mut out = json!({"tag": tag, "relationship": rel});
    // The nets of the pipes at either connector become one named net, so the net pass never
    // splits them although nothing draws them together.
    let ends: Vec<(usize, String)> = glued_to(&s.doc, a).into_iter().chain(glued_to(&s.doc, b)).collect();
    let mut nets: Vec<String> = Vec::new();
    for (pi, cid) in &ends {
        if let Some(n) = s.doc.pages.get(*pi).and_then(|p| p.connector(cid)).and_then(|c| c.net.clone())
            && !nets.contains(&n)
        {
            nets.push(n);
        }
    }
    if let Some(into) = nets.first().cloned() {
        let gone: HashSet<String> = nets.iter().skip(1).cloned().collect();
        for page in &mut s.doc.pages {
            for c in &mut page.connectors {
                if c.net.as_ref().is_some_and(|n| gone.contains(n)) {
                    c.net = Some(into.clone());
                }
            }
        }
        s.doc.model.nets.retain(|n| !gone.contains(&n.id));
        let name = format!("Off-page {tag}");
        let taken = s.doc.model.nets.iter().any(|n| n.id != into && n.name == name);
        if let Some(n) = s.doc.model.net_mut(&into)
            && n.name.is_empty()
            && n.number.is_empty()
            && !taken
        {
            n.name = name;
            // The partner references already say where the line goes on; the name is for the
            // net pass only, so it is not written on the line.
            n.label = vizcraft_model::NetLabel::Off;
        }
        if let Some(o) = out.as_object_mut() {
            o.insert("net".into(), json!(into));
        }
    }
    // b's pipes take a's line.
    let line_of =
        |doc: &Document, e: &str| glued_to(doc, e).into_iter().find_map(|(pi, c)| doc.pages.get(pi)?.connector(&c)?.ext.get(EXT_LINE).cloned());
    if let (Some(la), Some(lb)) = (line_of(&s.doc, a), line_of(&s.doc, b))
        && la != lb
    {
        for page in &mut s.doc.pages {
            for c in &mut page.connectors {
                if c.ext.get(EXT_LINE) == Some(&lb) {
                    c.ext.insert(EXT_LINE.to_string(), la.clone());
                }
            }
        }
        if let Some(o) = out.as_object_mut() {
            o.insert("line".into(), json!(la));
        }
    }
    Ok(out)
}

fn pair(s: &mut Session, v: &Value) -> CmdResult {
    let (a, ta) = off_page(s, v, "a", 0)?;
    let (b, tb) = off_page(s, v, "b", 1)?;
    if ta != tb {
        return Err(EngineError::Params("a pipe off-page connector pairs only with another pipe one, a signal one with a signal one".into()));
    }
    pair_elements(s, &a, &b, &ta)
}

fn place(s: &mut Session, v: &Value) -> CmdResult {
    let here = s.doc.active_index();
    // Without params: the selected pipe, its free end, the next sheet.
    let cid = match opt_str(v, "connector")? {
        Some(c) => check_len("connector", c)?.to_string(),
        None => {
            let page = s.doc.page();
            s.sel
                .items
                .iter()
                .find(|i| page.is_some_and(|p| p.connector(i).is_some()))
                .cloned()
                .ok_or_else(|| EngineError::Params("select a pipe with a free end, or name the `connector`".into()))?
        }
    };
    let end = match opt_str(v, "end")? {
        Some(e) => e.to_string(),
        None => match s.doc.page().and_then(|p| p.connector(&cid)) {
            Some(c) if matches!(c.end, End::Free { .. }) => "end".to_string(),
            _ => "start".to_string(),
        },
    };
    let end = end.as_str();
    if !matches!(end, "start" | "end") {
        return Err(EngineError::Params("`end` must be start or end".into()));
    }
    let to = match opt_str(v, "toPage")? {
        Some(r) => {
            let r = check_len("toPage", r)?;
            s.doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r))))?
        }
        None => s
            .doc
            .pages
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != here && !p.is_background)
            .map(|(i, _)| i)
            .find(|i| *i > here)
            .or_else(|| s.doc.pages.iter().enumerate().find(|(i, p)| *i != here && !p.is_background).map(|(i, _)| i))
            .ok_or_else(|| EngineError::Params("add another sheet for the partner first, or name `toPage`".into()))?,
    };
    if to == here {
        return Err(EngineError::Params("`toPage` must be another page".into()));
    }
    let page = s.doc.pages.get(here).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let c = page.connector(&cid).ok_or_else(|| EngineError::Params(format!("`{}` is not a connector on this page", short(&cid))))?;
    let free = match if end == "start" { &c.start } else { &c.end } {
        End::Free { x, y } => Point::new(*x, *y),
        End::Glued { .. } => {
            return Err(EngineError::Params(format!("the {end} of `{}` is glued; an off-page connector goes on a free end", short(&cid))));
        }
    };
    let (x, y) = (super::num(v, "x")?, super::num(v, "y")?);
    let (ea, _) = create_element(&mut s.doc, NewElement { type_id: PIPE_OFF_PAGE.into(), ..NewElement::default() })?;
    let sa = place_shape(&mut s.doc, &ea, here, free.x, free.y, None, None)?;
    // The pipe runs into the connector's in-port at its end, out of its out-port at its start.
    let point = if end == "end" { 0 } else { 1 };
    let at = s.doc.pages.get(here).and_then(|pg| pg.shape(&sa)).and_then(|sh| sh.connection_point_world(point)).unwrap_or(free);
    if let Some(sh) = s.doc.pages.get_mut(here).and_then(|pg| pg.shape_mut(&sa)) {
        sh.x += free.x - at.x;
        sh.y += free.y - at.y;
        sh.sanitize();
    }
    if let Some(c) = s.doc.pages.get_mut(here).and_then(|pg| pg.connector_mut(&cid)) {
        let glue = End::point(sa.clone(), point);
        if end == "start" { c.start = glue } else { c.end = glue }
    }
    let (eb, _) = create_element(&mut s.doc, NewElement { type_id: PIPE_OFF_PAGE.into(), ..NewElement::default() })?;
    let bx = x.unwrap_or(40.0).clamp(-MAX_DELTA, MAX_DELTA);
    let by = y.unwrap_or(free.y).clamp(-MAX_DELTA, MAX_DELTA);
    let sb = place_shape(&mut s.doc, &eb, to, bx, by, None, None)?;
    let r = pair_elements(s, &ea, &eb, PIPE_OFF_PAGE)?;
    select(s, vec![sa.clone()]);
    with_sel(s, json!({"a": ea, "b": eb, "shapes": [sa, sb], "tag": r.get("tag").cloned().unwrap_or(Value::Null)}))
}

// ---------------------------------------------------------------- nozzles

fn add_nozzle(s: &mut Session, v: &Value) -> CmdResult {
    // Without a shape, the selected one.
    let target = match opt_str(v, "shape")? {
        Some(t) => check_len("shape", t)?.to_string(),
        None => s.sel.primary.clone().ok_or_else(|| EngineError::Params("select a piece of equipment, or name its `shape`".into()))?,
    };
    let target = target.as_str();
    let here = s.doc.active_index();
    let page = s.doc.pages.get(here).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    // A shape on this page, or an element drawn on it.
    let shape = page
        .shape(target)
        .or_else(|| page.shapes.iter().find(|sh| sh.element.as_deref() == Some(target)))
        .ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(target))))?
        .clone();
    let equipment = shape.element.clone().filter(|e| {
        let facts = Facts::new(&s.doc.model, cat());
        s.doc.model.element(e).is_some_and(|x| facts.is_a(&x.type_id, EQUIPMENT))
    });
    let equipment = equipment.ok_or_else(|| EngineError::Params(format!("`{}` does not draw equipment", short(target))))?;
    let side = p::req_str(v, "side")?;
    let turns: u8 = match side {
        "right" => 0,
        "bottom" => 1,
        "left" => 2,
        "top" => 3,
        _ => return Err(EngineError::Params("`side` must be left, right, top or bottom".into())),
    };
    let given_at = super::num(v, "at")?;
    if given_at.is_some_and(|a| !(0.0..=1.0).contains(&a)) {
        return Err(EngineError::Params("`at` must be from 0 to 1".into()));
    }
    let nozzles: Vec<&Element> =
        s.doc.model.elements.iter().filter(|e| e.type_id == NOZZLE && e.owner.as_deref() == Some(equipment.as_str())).collect();
    let sub = match opt_str(v, "subTag")? {
        Some(t) => {
            let t = t.trim();
            if t.is_empty() || t.len() > 32 {
                return Err(EngineError::Params("`subTag` must be 1 to 32 bytes".into()));
            }
            if nozzles.iter().any(|n| text(n, "subTag") == t) {
                return Err(EngineError::Params(format!("the equipment already has a nozzle `{}`", short(t))));
            }
            t.to_string()
        }
        None => (1..=nozzles.len().saturating_add(1))
            .map(|k| format!("N{k}"))
            .find(|t| !nozzles.iter().any(|n| text(n, "subTag") == *t))
            .unwrap_or_else(|| "N1".into()),
    };
    // Where along the side: the given place, else the first of the usual places no nozzle holds.
    let frame = (shape.x, shape.y, shape.w, shape.h);
    let taken: Vec<Point> = page
        .shapes
        .iter()
        .filter(|sh| {
            sh.parent.as_deref() == Some(shape.id.as_str())
                && sh.element.as_deref().and_then(|e| s.doc.model.element(e)).is_some_and(|e| e.type_id == NOZZLE)
        })
        .map(|sh| Point::new(sh.x + sh.w / 2.0, sh.y + sh.h / 2.0))
        .collect();
    let spot = |a: f64| -> Point {
        match turns {
            0 => Point::new(frame.0 + frame.2, frame.1 + frame.3 * a),
            1 => Point::new(frame.0 + frame.2 * a, frame.1 + frame.3),
            2 => Point::new(frame.0, frame.1 + frame.3 * a),
            _ => Point::new(frame.0 + frame.2 * a, frame.1),
        }
    };
    let at = given_at.unwrap_or_else(|| {
        NOZZLE_STEPS.iter().copied().find(|a| !taken.iter().any(|t| t.distance(spot(*a)) < 15.0 + frame.2.min(frame.3) * 0.05)).unwrap_or(0.5)
    });
    let mut props = BTreeMap::new();
    props.insert("subTag".to_string(), PropValue::Text(sub));
    if let Some(d) = opt_str(v, "nominalDiameter")? {
        props.insert("nominalDiameter".to_string(), PropValue::Text(super::model::cut(d, 64)));
    }
    let (id, _) = create_element(
        &mut s.doc,
        NewElement { type_id: NOZZLE.into(), owner: Some(equipment.clone()), properties: props, ..NewElement::default() },
    )?;
    let sid = place_shape(&mut s.doc, &id, here, frame.0, frame.1, None, None)?;
    let p0 = spot(at);
    if let Some(sh) = s.doc.pages.get_mut(here).and_then(|pg| pg.shape_mut(&sid)) {
        let (w, h) = (sh.w, sh.h);
        // The stub's inner end (the middle of its left side before turning) touches the side.
        let (x, y) = match turns {
            0 => (p0.x, p0.y - h / 2.0),
            1 => (p0.x - w / 2.0, p0.y),
            2 => (p0.x - w, p0.y - h / 2.0),
            _ => (p0.x - w / 2.0, p0.y - h),
        };
        sh.x = x;
        sh.y = y;
        sh.orient.turns = turns;
        sh.parent = Some(shape.id.clone());
        sh.sanitize();
    }
    select(s, vec![sid.clone()]);
    let tag = s.doc.model.element(&id).map(|e| e.tag.clone()).unwrap_or_default();
    with_sel(s, json!({"id": id, "shape": sid, "tag": tag}))
}

// ---------------------------------------------------------------- numbering

fn numbering(s: &mut Session, v: &Value) -> CmdResult {
    let scope = "process.loop";
    let pack_by = cat().scopes().into_iter().find(|sc| sc.id == scope).map(|sc| sc.number_by).unwrap_or_default();
    let current = |doc: &Document| {
        let by = doc.model.id_formats.get(scope).and_then(|o| o.number_by.clone()).unwrap_or_else(|| pack_by.clone());
        if by.is_empty() { "serial" } else { "parallel" }
    };
    let Some(scheme) = opt_str(v, "scheme")? else { return Ok(json!({"scheme": current(&s.doc)})) };
    let by = match scheme {
        "parallel" => "variable",
        "serial" => "",
        _ => return Err(EngineError::Params("`scheme` must be parallel or serial".into())),
    };
    let mut o = s.doc.model.id_formats.get(scope).cloned().unwrap_or_default();
    o.number_by = (by != pack_by).then(|| by.to_string());
    if o.is_empty() {
        s.doc.model.id_formats.remove(scope);
    } else {
        s.doc.model.id_formats.insert(scope.to_string(), o);
    }
    Ok(json!({"scheme": current(&s.doc)}))
}

// ---------------------------------------------------------------- lists

/// A process list as a table: column titles, the key each column reads (and table.set writes,
/// when editable), the id each row shows and the cells.
pub struct PlantList {
    /// The list's name.
    pub name: &'static str,
    /// Column titles.
    pub columns: Vec<String>,
    /// The key each column reads.
    pub keys: Vec<String>,
    /// Whether table.set may write each column.
    pub editable: Vec<bool>,
    /// The element or relationship each row shows (empty for a total row).
    pub ids: Vec<String>,
    /// The cells.
    pub rows: Vec<Vec<String>>,
}

/// What one list reads for its rows.
struct Ctx<'a> {
    doc: &'a Document,
    facts: Facts<'a>,
    page: Option<usize>,
    /// Page names each element is drawn on.
    drawn: HashMap<&'a str, Vec<&'a str>>,
    /// Line ids each element's views touch through pipe segments.
    lines_at: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Ctx<'a> {
    fn new(doc: &'a Document, page: Option<usize>) -> Self {
        let mut drawn: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut lines_at: HashMap<&str, Vec<&str>> = HashMap::new();
        for pg in &doc.pages {
            let mut by_shape: HashMap<&str, &str> = HashMap::new();
            for sh in &pg.shapes {
                if let Some(e) = sh.element.as_deref() {
                    let list = drawn.entry(e).or_default();
                    if !list.contains(&pg.name.as_str()) {
                        list.push(pg.name.as_str());
                    }
                    by_shape.insert(sh.id.as_str(), e);
                }
            }
            for c in &pg.connectors {
                let Some(l) = c.ext.get(EXT_LINE) else { continue };
                let pages = drawn.entry(l.as_str()).or_default();
                if !pages.contains(&pg.name.as_str()) {
                    pages.push(pg.name.as_str());
                }
                for end in [&c.start, &c.end] {
                    if let Some(e) = end.shape().and_then(|s| by_shape.get(s)) {
                        let list = lines_at.entry(e).or_default();
                        if !list.contains(&l.as_str()) {
                            list.push(l.as_str());
                        }
                    }
                }
            }
        }
        Ctx { doc, facts: Facts::new(&doc.model, cat()), page, drawn, lines_at }
    }

    /// Whether `id` shows on the page the list is for (every element when none is given).
    fn on_page(&self, id: &str) -> bool {
        match self.page.and_then(|i| self.doc.pages.get(i)) {
            None => true,
            Some(pg) => self.drawn.get(id).is_some_and(|names| names.contains(&pg.name.as_str())),
        }
    }

    fn pages(&self, id: &str) -> String {
        self.drawn.get(id).map(|v| v.join(", ")).unwrap_or_default()
    }

    fn tag_or_name(&self, id: &str) -> String {
        self.doc.model.element(id).map(|e| if e.tag.trim().is_empty() { e.name.clone() } else { e.tag.clone() }).unwrap_or_default()
    }

    fn type_label(&self, e: &Element) -> String {
        self.facts
            .element_type(&e.type_id)
            .map(|t| t.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| vizcraft_model::local_of(&e.type_id).to_string())
    }

    fn line(&self, id: &str) -> Option<&'a Element> {
        self.lines_at.get(id).and_then(|v| v.first()).and_then(|l| self.doc.model.element(l))
    }

    fn loop_of(&self, e: &Element) -> Option<&'a Element> {
        self.facts.ancestors(&e.id).into_iter().filter_map(|a| self.facts.element(a)).find(|a| a.type_id == LOOP)
    }

    fn is(&self, e: &Element, ty: &str) -> bool {
        self.facts.is_a(&e.type_id, ty)
    }
}

/// A column: its title, the key it reads, and whether table.set may write it.
type Col = (&'static str, &'static str, bool);

/// The rows of `elements` with `cols`, each cell read by `cell` (a property key reads the
/// element's property).
fn table<'a>(name: &'static str, cols: &[Col], items: Vec<&'a Element>, mut cell: impl FnMut(&'a Element, &str) -> Option<String>) -> PlantList {
    let mut out = PlantList {
        name,
        columns: cols.iter().map(|c| c.0.to_string()).collect(),
        keys: cols.iter().map(|c| c.1.to_string()).collect(),
        editable: cols.iter().map(|c| c.2).collect(),
        ids: Vec::new(),
        rows: Vec::new(),
    };
    for e in items {
        out.ids.push(e.id.clone());
        out.rows.push(cols.iter().map(|(_, key, _)| cell(e, key).unwrap_or_else(|| text(e, key))).collect());
    }
    out
}

/// The tags of the equipment and nozzles a line's segments end on, in drawing order.
fn line_ends(doc: &Document, line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for pg in &doc.pages {
        for c in pg.connectors.iter().filter(|c| c.ext.get(EXT_LINE).is_some_and(|l| l == line)) {
            for end in [&c.start, &c.end] {
                let Some(e) = end.shape().and_then(|s| pg.shape(s)).and_then(|sh| sh.element.as_deref()).and_then(|e| doc.model.element(e)) else {
                    continue;
                };
                // Off-page connectors only pass the line on; its ends are nozzles and equipment.
                if e.type_id == NOZZLE || cat().element_type(&e.type_id).is_some_and(|(_, t)| t.extends.as_deref() == Some(EQUIPMENT)) {
                    let t = if e.tag.is_empty() { e.name.clone() } else { e.tag.clone() };
                    if !t.is_empty() && !out.contains(&t) {
                        out.push(t);
                    }
                }
            }
        }
    }
    out
}

/// The I/O channel wired to `e`, through any terminals.
fn channel_of(doc: &Document, e: &str) -> Option<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut frontier: Vec<&str> = vec![e];
    while let Some(n) = frontier.pop() {
        if !seen.insert(n) || seen.len() > 10_000 {
            continue;
        }
        if let Some(x) = doc.model.element(n)
            && x.type_id == "process.IOChannel"
        {
            return Some(x.tag.clone());
        }
        if n != e && doc.model.element(n).is_some_and(|x| x.type_id != "process.Terminal") {
            continue;
        }
        for r in doc.model.relationships.iter().filter(|r| r.type_id == "process.Wire") {
            if r.source == n {
                frontier.push(r.target.as_str());
            } else if r.target == n {
                frontier.push(r.source.as_str());
            }
        }
    }
    None
}

/// Build the list `name` over `doc` (only what `page` draws, when given).
pub fn build_list(doc: &Document, name: &str, page: Option<usize>) -> Result<PlantList, EngineError> {
    let cx = Ctx::new(doc, page);
    let els = || doc.model.elements.iter().filter(|e| e.pack() == "process" && cx.on_page(&e.id));
    let scopes = cat().scopes();
    let in_scope = |e: &Element, scope: &str| scopes.iter().any(|s| s.id == scope && s.types.contains(&e.type_id));
    Ok(match name {
        "line" => {
            let cols: &[Col] = &[
                ("Line number", "tag", false),
                ("Size", "nominalDiameter", true),
                ("Fluid", "fluidCode", true),
                ("Class", "pipingClass", true),
                ("From", "from", true),
                ("To", "to", true),
                ("P&ID", "pages", false),
                ("Design P", "designPressure", true),
                ("Design T", "designTemperature", true),
                ("Operating P", "operatingPressure", true),
                ("Operating T", "operatingTemperature", true),
                ("Insulation", "insulation", true),
                ("Tracing", "tracing", true),
                ("Test P", "testPressure", true),
            ];
            let mut items: Vec<&Element> = els().filter(|e| is_line_type(&e.type_id)).collect();
            items.sort_by(|a, b| a.tag.cmp(&b.tag));
            table("Line list", cols, items, |e, k| match k {
                "tag" => Some(e.tag.clone()),
                "pages" => Some(cx.pages(&e.id)),
                "from" | "to" => {
                    let own = text(e, k);
                    if !own.is_empty() {
                        return Some(own);
                    }
                    let ends = line_ends(doc, &e.id);
                    let end = if k == "from" { ends.first() } else { ends.last().filter(|_| ends.len() > 1) };
                    Some(end.cloned().unwrap_or_default())
                }
                _ => None,
            })
        }
        "equipment" => {
            let cols: &[Col] = &[
                ("Tag", "tag", true),
                ("Description", "name", true),
                ("Class", "classCode", true),
                ("Service", "service", true),
                ("P&ID", "pages", false),
                ("Size", "size", true),
                ("Design P", "designPressure", true),
                ("Design T", "designTemperature", true),
                ("Material", "material", true),
                ("Duty", "duty", true),
                ("Driver", "driver", true),
                ("Vendor", "vendor", true),
                ("Datasheet", "datasheet", true),
            ];
            let items: Vec<&Element> = els().filter(|e| in_scope(e, "process.equipment")).collect();
            table("Equipment list", cols, items, |e, k| match k {
                "tag" => Some(e.tag.clone()),
                "name" => Some(e.name.clone()),
                "pages" => Some(cx.pages(&e.id)),
                _ => None,
            })
        }
        "instrument" => {
            let cols: &[Col] = &[
                ("Tag", "tag", false),
                ("Loop", "loop", false),
                ("Service", "service", false),
                ("P&ID", "pages", false),
                ("Line or equipment", "measures", false),
                ("Type", "type", false),
                ("Location", "location", true),
                ("I/O type", "ioType", true),
                ("System", "system", true),
                ("Range", "range", true),
                ("Setpoint", "setpoint", true),
                ("Datasheet", "datasheet", true),
                ("Manufacturer", "manufacturer", true),
                ("Model", "model", true),
            ];
            let items: Vec<&Element> =
                els().filter(|e| cx.is(e, INSTRUMENT) || cx.is(e, PRIMARY) || (cx.is(e, COMPONENT) && !text(e, "letters").is_empty())).collect();
            table("Instrument index", cols, items, |e, k| match k {
                "tag" => Some(e.tag.clone()),
                "loop" => Some(cx.loop_of(e).map(|l| l.tag.clone()).unwrap_or_default()),
                "service" => Some(cx.loop_of(e).map(|l| text(l, "service")).unwrap_or_default()),
                "pages" => Some(cx.pages(&e.id)),
                "measures" => {
                    let target = doc
                        .model
                        .relationships
                        .iter()
                        .find(|r| r.type_id == "process.Measures" && r.source == e.id)
                        .map(|r| cx.tag_or_name(&r.target));
                    Some(target.or_else(|| cx.line(&e.id).map(|l| l.tag.clone())).unwrap_or_default())
                }
                "type" => Some(cx.type_label(e)),
                _ => None,
            })
        }
        "valve" => {
            let cols: &[Col] = &[
                ("Tag", "tag", false),
                ("P&ID", "pages", false),
                ("Line", "line", false),
                ("Size", "nominalDiameter", true),
                ("Class", "class", false),
                ("Type", "type", false),
                ("Ends", "ends", true),
                ("Actuator", "actuator", true),
                ("Fail", "fail", true),
                ("Normal position", "normallyClosed", true),
            ];
            let items: Vec<&Element> = els().filter(|e| in_scope(e, "process.instrument") && cx.is(e, COMPONENT)).collect();
            table("Valve list", cols, items, |e, k| match k {
                "tag" => Some(if e.tag.is_empty() { e.name.clone() } else { e.tag.clone() }),
                "pages" => Some(cx.pages(&e.id)),
                "line" => Some(cx.line(&e.id).map(|l| l.tag.clone()).unwrap_or_default()),
                "class" => Some(cx.line(&e.id).map(|l| text(l, "pipingClass")).unwrap_or_default()),
                "type" => Some(cx.type_label(e)),
                "nominalDiameter" => Some({
                    let own = text(e, k);
                    if own.is_empty() { cx.line(&e.id).map(|l| text(l, k)).unwrap_or_default() } else { own }
                }),
                "normallyClosed" => Some(if e.prop(k).and_then(PropValue::as_bool).unwrap_or(false) { "NC".into() } else { "NO".into() }),
                _ => None,
            })
        }
        "io" => {
            let cols: &[Col] = &[
                ("Tag", "tag", false),
                ("Loop", "loop", false),
                ("Service", "service", false),
                ("P&ID", "pages", false),
                ("I/O type", "ioType", true),
                ("Signal", "signal", true),
                ("System", "system", true),
                ("Channel", "channel", false),
                ("Range", "range", true),
                ("High alarm", "alarmHigh", true),
                ("Low alarm", "alarmLow", true),
            ];
            let items: Vec<&Element> = els().filter(|e| cx.is(e, INSTRUMENT) && !text(e, "ioType").is_empty()).collect();
            table("I/O list", cols, items, |e, k| match k {
                "tag" => Some(e.tag.clone()),
                "loop" => Some(cx.loop_of(e).map(|l| l.tag.clone()).unwrap_or_default()),
                "service" => Some(cx.loop_of(e).map(|l| text(l, "service")).unwrap_or_default()),
                "pages" => Some(cx.pages(&e.id)),
                "channel" => Some(channel_of(doc, &e.id).unwrap_or_default()),
                _ => None,
            })
        }
        "stream" => {
            let cols: &[&str] = &["Stream", "From", "To", "Phase", "Flow", "Temperature", "Pressure", "Composition"];
            let keys = ["name", "from", "to", "phase", "flow", "temperature", "pressure", "composition"];
            let on: HashSet<&str> = doc
                .pages
                .iter()
                .enumerate()
                .filter(|(i, _)| page.is_none_or(|p| p == *i))
                .flat_map(|(_, pg)| pg.connectors.iter().filter_map(|c| c.relationship.as_deref()))
                .collect();
            let mut out = PlantList {
                name: "Stream table",
                columns: cols.iter().map(|c| c.to_string()).collect(),
                keys: keys.iter().map(|k| k.to_string()).collect(),
                editable: vec![true, false, false, true, true, true, true, true],
                ids: Vec::new(),
                rows: Vec::new(),
            };
            let mut rels: Vec<&vizcraft_model::Relationship> =
                doc.model.relationships.iter().filter(|r| r.type_id == "process.Stream" && (page.is_none() || on.contains(r.id.as_str()))).collect();
            rels.sort_by_key(|r| (r.name.trim().parse::<u64>().unwrap_or(u64::MAX), r.name.clone()));
            for r in rels {
                out.ids.push(r.id.clone());
                out.rows.push(
                    keys.iter()
                        .map(|k| match *k {
                            "name" => r.name.clone(),
                            "from" => cx.tag_or_name(&r.source),
                            "to" => cx.tag_or_name(&r.target),
                            k => r.prop(k).map(PropValue::display).unwrap_or_default(),
                        })
                        .collect(),
                );
            }
            out
        }
        "nozzle" => {
            let cols: &[Col] = &[
                ("Tag", "tag", false),
                ("Equipment", "equipment", false),
                ("Sub-tag", "subTag", true),
                ("Size", "nominalDiameter", true),
                ("Rating", "nominalPressure", true),
                ("Service", "service", true),
            ];
            let items: Vec<&Element> = els().filter(|e| e.type_id == NOZZLE).collect();
            table("Nozzle list", cols, items, |e, k| match k {
                "tag" => Some(e.tag.clone()),
                "equipment" => Some(e.owner.as_deref().map(|o| cx.tag_or_name(o)).unwrap_or_default()),
                _ => None,
            })
        }
        "fixtureUnits" => {
            let cols: &[Col] = &[
                ("Fixture", "fixture", false),
                ("Kind", "kind", true),
                ("System", "system", true),
                ("Fixture units", "fixtureUnits", true),
                ("Sheet", "pages", false),
            ];
            let items: Vec<&Element> = els().filter(|e| e.type_id == "process.Fixture").collect();
            let (mut cw, mut hw) = (0.0f64, 0.0f64);
            for e in &items {
                let fu = e.prop("fixtureUnits").and_then(PropValue::as_f64).filter(|f| f.is_finite()).unwrap_or(0.0);
                let sys = text(e, "system");
                if sys.contains("CW") {
                    cw += fu;
                }
                if sys.contains("HW") {
                    hw += fu;
                }
            }
            let mut t = table("Fixture units", cols, items, |e, k| match k {
                "fixture" => Some(if e.name.is_empty() { e.id.clone() } else { e.name.clone() }),
                "pages" => Some(cx.pages(&e.id)),
                _ => None,
            });
            for (label, sum) in [("Total CW", cw), ("Total HW", hw)] {
                t.ids.push(String::new());
                t.rows.push(vec![label.into(), String::new(), String::new(), vizcraft_stencils::builders::params::fmt_num(sum), String::new()]);
            }
            t
        }
        "bom" => {
            let cols: &[Col] = &[
                ("Item", "tag", false),
                ("Description", "name", true),
                ("Type", "type", false),
                ("Size", "nominalDiameter", true),
                ("Ends", "ends", true),
            ];
            let mut items: Vec<&Element> = els().filter(|e| cx.is(e, COMPONENT)).collect();
            items.sort_by(|a, b| a.type_id.cmp(&b.type_id).then_with(|| a.tag.cmp(&b.tag)));
            table("Bill of materials", cols, items, |e, k| match k {
                "tag" => Some(if e.tag.is_empty() { e.id.clone() } else { e.tag.clone() }),
                "name" => Some(e.name.clone()),
                "type" => Some(cx.type_label(e)),
                _ => None,
            })
        }
        other => return Err(EngineError::Params(format!("`list` must be one of {}, not `{}`", LISTS.join(", "), short(other)))),
    })
}

/// The list a `table.export {view: "pack", list: "process.<name>"}` names, as a table view:
/// (sheet name, columns, rows).
pub fn list_view(s: &Session, v: &Value) -> Result<(String, Vec<String>, Vec<Vec<String>>), EngineError> {
    let list = p::req_str(v, "list")?;
    let name = list
        .strip_prefix("process.")
        .ok_or_else(|| EngineError::Params(format!("`list` must be process.<name> ({}), not `{}`", LISTS.join(", "), short(list))))?;
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let t = build_list(&s.doc, name, page)?;
    Ok((t.name.to_string(), t.columns, t.rows))
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "list")?;
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let format = opt_str(v, "format")?;
    if format.is_some_and(|f| !matches!(f, "csv" | "xlsx" | "markdown")) {
        return Err(EngineError::Params("`format` must be csv, xlsx or markdown".into()));
    }
    let path = opt_str(v, "path")?;
    if format == Some("xlsx") && path.is_none() {
        return Err(EngineError::Params("`path` is required for xlsx: a workbook is a binary file".into()));
    }
    if path.is_some() && format.is_none() {
        return Err(EngineError::Params("`path` needs a `format`".into()));
    }
    let t = build_list(&s.doc, name, page)?;
    let mut out = json!({
        "list": name, "columns": t.columns, "keys": t.keys, "ids": t.ids, "rows": t.rows, "editable": t.editable, "count": t.rows.len(),
    });
    if let Some(f) = format {
        let table = vizcraft_formats::tables::from_text(&t.columns, t.rows.clone());
        let (content, bytes) = match f {
            "csv" => {
                let c = vizcraft_formats::tables::write_csv(&table);
                (Some(c.clone()), c.into_bytes())
            }
            "markdown" => {
                let c = vizcraft_formats::tables::write_markdown(&table);
                (Some(c.clone()), c.into_bytes())
            }
            _ => (None, vizcraft_formats::tables::write_xlsx(&table, t.name).map_err(|e| EngineError::Failed(e.to_string()))?),
        };
        if let Some(o) = out.as_object_mut() {
            o.insert("format".into(), json!(f));
            match (path, content) {
                (Some(path), _) => {
                    std::fs::write(path, &bytes).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
                    o.insert("path".into(), json!(path));
                    o.insert("bytes".into(), json!(bytes.len()));
                }
                (None, Some(c)) => {
                    o.insert("content".into(), json!(c));
                }
                (None, None) => {}
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- isometric runs

/// The page offset of one unit along an isometric direction (30 degree axes, y down).
pub fn iso_step(dir: &str) -> Option<(f64, f64)> {
    let (c, h) = (30f64.to_radians().cos(), 0.5);
    Some(match dir {
        "n" => (c, -h),
        "s" => (-c, h),
        "e" => (c, h),
        "w" => (-c, -h),
        "up" => (0.0, -1.0),
        "down" => (0.0, 1.0),
        _ => return None,
    })
}

fn iso_run(s: &mut Session, v: &Value) -> CmdResult {
    let here = page_param(&s.doc, v)?;
    let raw_legs = v.get("legs").and_then(Value::as_array).filter(|a| !a.is_empty() && a.len() <= MAX_LEGS);
    let raw_legs = raw_legs.ok_or_else(|| EngineError::Params(format!("`legs` must be a list of 1 to {MAX_LEGS} legs")))?;
    let mut legs: Vec<(f64, f64)> = Vec::new();
    for l in raw_legs {
        let dir = l.get("dir").and_then(Value::as_str).ok_or_else(|| EngineError::Params("each leg needs a `dir`".into()))?;
        let step = iso_step(dir).ok_or_else(|| EngineError::Params(format!("`dir` must be one of {}", ISO_DIRECTIONS.join(", "))))?;
        let len = l.get("length").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0 && *n <= MAX_LEG);
        let len = len.ok_or_else(|| EngineError::Params(format!("each leg needs a `length` from 0 to {MAX_LEG}")))?;
        legs.push((step.0 * len, step.1 * len));
    }
    let kind = opt_str(v, "kind")?.unwrap_or("process.processLine").to_string();
    let k = vizcraft_stencils::connector_kind(&kind).ok_or_else(|| EngineError::Params(format!("unknown connector kind `{}`", short(&kind))))?;
    if !k.net.is_some_and(|n| matches!(n, vizcraft_model::NetKind::Pipe | vizcraft_model::NetKind::Fluid)) {
        return Err(EngineError::Params(format!("`{}` is not a pipe kind", short(&kind))));
    }
    let from = v.get("from").filter(|f| f.is_object()).ok_or_else(|| EngineError::Params("`from` ({x, y} or {shape, point}) is required".into()))?;
    let page = s.doc.pages.get(here).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let (start_end, start) = match from.get("shape").and_then(Value::as_str) {
        Some(sh) => {
            let shape =
                page.shape(check_len("shape", sh)?).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(sh))))?;
            let point = from.get("point").and_then(Value::as_u64).map(|p| p as usize).unwrap_or(0);
            let at =
                shape.connection_point_world(point).ok_or_else(|| EngineError::Params(format!("`{}` has no connection point {point}", short(sh))))?;
            (End::point(shape.id.clone(), point), at)
        }
        None => {
            let x = super::req_num(from, "x")?.clamp(-MAX_DELTA, MAX_DELTA);
            let y = super::req_num(from, "y")?.clamp(-MAX_DELTA, MAX_DELTA);
            (End::free(Point::new(x, y)), Point::new(x, y))
        }
    };
    let line = match opt_str(v, "line")? {
        Some(l) => {
            let l = check_len("line", l)?;
            s.doc
                .model
                .element(l)
                .filter(|e| is_line_type(&e.type_id))
                .ok_or_else(|| EngineError::Params(format!("`{}` is not a line", short(l))))?;
            l.to_string()
        }
        None => new_line(&mut s.doc, cat(), vizcraft_notation::process_pass::LINE, &BTreeMap::new())
            .ok_or_else(|| EngineError::Failed("the model is full".into()))?,
    };
    let mut at = start;
    let mut prev = start_end;
    let mut connectors = Vec::new();
    let mut elbows = Vec::new();
    let n = legs.len();
    for (i, (dx, dy)) in legs.into_iter().enumerate() {
        let next = Point::new((at.x + dx).clamp(-MAX_DELTA, MAX_DELTA), (at.y + dy).clamp(-MAX_DELTA, MAX_DELTA));
        let end = if i + 1 < n {
            // An elbow at the turn; both legs glue to its one point.
            let (eid, _) = create_element(&mut s.doc, NewElement { type_id: "process.Elbow".into(), ..NewElement::default() })?;
            let sid = place_shape(&mut s.doc, &eid, here, next.x, next.y, None, None)?;
            if let Some(sh) = s.doc.pages.get_mut(here).and_then(|pg| pg.shape_mut(&sid)) {
                sh.x = next.x - sh.w / 2.0;
                sh.y = next.y - sh.h / 2.0;
                sh.sanitize();
            }
            elbows.push(sid.clone());
            End::point(sid, 0)
        } else {
            End::free(next)
        };
        let cid = s.doc.next_id("c");
        let mut c = Connector::new(cid.clone(), prev.clone(), end.clone());
        super::connector_kind::apply_kind(&mut c, &kind)?;
        c.route = Route::Straight;
        c.avoid = false;
        c.ext.insert(EXT_LINE.to_string(), line.clone());
        s.doc.pages.get_mut(here).ok_or_else(|| EngineError::Params("no such page".into()))?.connectors.push(c);
        connectors.push(cid);
        prev = end;
        at = next;
    }
    if here == s.doc.active_index() {
        select(s, connectors.clone());
    }
    with_sel(s, json!({"connectors": connectors, "elbows": elbows, "line": line, "end": {"x": at.x, "y": at.y}}))
}

fn dexpi_map(_s: &mut Session, v: &Value) -> CmdResult {
    let only = opt_str(v, "type")?;
    let pack = cat().pack("process").ok_or_else(|| EngineError::Failed("the process pack is not loaded".into()))?;
    if let Some(t) = only
        && pack.element_type(t).is_none()
    {
        return Err(EngineError::Params(format!("`{}` is not a process element type", short(t))));
    }
    let mut mappings = Vec::new();
    let mut unmapped = Vec::new();
    for t in pack.element_types.iter().filter(|t| only.is_none_or(|o| o == t.id)) {
        match vizcraft_formats::dexpi_map::class_of(&t.id) {
            Some(m) => {
                let mut o = json!({"type": t.id, "dexpiClass": m.class});
                if let (Some(uri), Some(map)) = (m.uri, o.as_object_mut()) {
                    map.insert("uri".into(), json!(uri));
                }
                mappings.push(o);
            }
            None => unmapped.push(t.id.clone()),
        }
    }
    Ok(json!({"mappings": mappings, "unmapped": unmapped}))
}

// ---------------------------------------------------------------- loop diagrams

/// Space between rows of a loop diagram, in page units.
const ROW: f64 = 110.0;
/// Width of one lane of a loop diagram.
const LANE: f64 = 220.0;

/// The run of `loop.diagram` (its spec is in `cmd/loops.rs`).
pub(crate) fn loop_diagram(s: &mut Session, v: &Value) -> CmdResult {
    let from = element_of(&s.doc, check_len("loop", p::req_str(v, "loop")?)?)
        .ok_or_else(|| EngineError::Params("`loop` must be a loop or one of its members".into()))?;
    let lp = if s.doc.model.element(&from).is_some_and(|e| e.type_id == LOOP) {
        from
    } else {
        s.doc
            .model
            .ancestors(&from)
            .into_iter()
            .find(|a| s.doc.model.element(a).is_some_and(|e| e.type_id == LOOP))
            .ok_or_else(|| EngineError::Params("`loop` is in no loop".into()))?
    };
    let tag = s.doc.model.element(&lp).map(|e| e.tag.clone()).unwrap_or_default();
    let sheet = opt_str(v, "sheet")?.map(str::to_string);
    let base = match opt_str(v, "name")? {
        Some(n) => check_len("name", n)?.to_string(),
        None => format!("Loop {}", if tag.is_empty() { lp.clone() } else { tag.clone() }),
    };
    let mut name = base.clone();
    for k in 2..1000 {
        if !s.doc.pages.iter().any(|p| p.name == name) {
            break;
        }
        name = format!("{base} ({k})");
    }
    let variant = s.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.variant.clone()).unwrap_or_default();
    let facts_members: Vec<(String, &'static str, String)> = {
        let facts = Facts::new(&s.doc.model, cat());
        s.doc
            .model
            .descendants(&lp)
            .iter()
            .filter_map(|m| s.doc.model.element(m))
            .filter(|e| e.type_id != LOOP)
            .map(|e| (e.id.clone(), super::loops::role(&facts, e), text(e, "location")))
            .collect()
    };
    call(s, "page.add", json!({"name": name}))?;
    let pi = s.doc.active_index();
    call(s, "model.setViewpoint", json!({"viewpoint": "process.Loop"}))?;
    if !variant.is_empty()
        && let Some(view) = s.doc.pages.get_mut(pi).and_then(|p| p.view.as_mut())
    {
        view.variant = variant;
    }
    if let Some(sh) = sheet {
        call(s, "sheet.set", json!({"sheet": sh}))?;
    }
    // Five lanes left to right, as ISA-5.4 sections a loop diagram.
    // Inside the sheet frame, as long as the most devices one lane holds.
    let (x0, y0) = (130.0, 120.0);
    let rows = facts_members.len().max(1) as f64;
    let lane_len = 200.0 + ROW * rows;
    let set = s.doc.next_id("lanes");
    let mut set_shape = vizcraft_diagram::container::new_lane_set(set.clone(), x0, y0, true, &format!("Loop {tag}"));
    set_shape.sanitize();
    let titles = ["Field", "Junction box", "Marshalling", "Control room", "Panel front"];
    {
        let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the new page is missing".into()))?;
        page.shapes.push(set_shape);
    }
    let mut lanes = Vec::new();
    for t in titles {
        let lid = s.doc.next_id("lane");
        let mut lane = vizcraft_diagram::container::new_lane(lid.clone(), t, true);
        lane.w = LANE;
        lane.h = lane_len;
        let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the new page is missing".into()))?;
        vizcraft_diagram::container::add_lane(page, &set, lane, None).map_err(|e| EngineError::Failed(e.to_string()))?;
        lanes.push(lid);
    }
    let xs: Vec<f64> =
        (0..titles.len()).map(|i| lanes.get(i).and_then(|l| s.doc.pages.get(pi)?.shape(l)).map_or(x0 + LANE * i as f64, |sh| sh.x)).collect();
    let lane_x = |i: usize| -> f64 { xs.get(i).copied().unwrap_or(x0 + LANE * i as f64) };
    let page_id = s.doc.pages.get(pi).map(|p| p.id.clone()).unwrap_or_default();
    let top = y0 + 100.0;
    // Field devices that carry a signal, then the others; controllers to the control room or the panel.
    let wired: Vec<&(String, &'static str, String)> = facts_members.iter().filter(|(_, r, _)| matches!(*r, "transmitter" | "final")).collect();
    let mut shapes: Vec<String> = Vec::new();
    let mut field_row = 0usize;
    let mut room_row = 0usize;
    let mut panel_row = 0usize;
    let mut device_shapes: Vec<(String, String, &'static str)> = Vec::new();
    for (id, role, location) in &facts_members {
        let (lane, row) = match (*role, location.as_str()) {
            ("controller" | "other", "panelFront" | "localFront") => {
                panel_row += 1;
                (4, panel_row - 1)
            }
            ("controller" | "other", l) if !l.is_empty() && l != "field" => {
                room_row += 1;
                (3, room_row - 1)
            }
            _ => {
                field_row += 1;
                (0, field_row - 1)
            }
        };
        let x = lane_x(lane) + 70.0;
        let y = top + ROW * row as f64;
        let r = call(s, "element.show", json!({"id": id, "page": page_id, "x": x, "y": y}));
        let sid = match r {
            Ok(r) => r.get("shape").and_then(Value::as_str).unwrap_or_default().to_string(),
            Err(_) => continue,
        };
        shapes.push(sid.clone());
        device_shapes.push((id.clone(), sid, role));
    }
    // Junction box and marshalling cabinet with a terminal per signal, cables and channels.
    let n = wired.len().max(1);
    let box_h = 60.0 + ROW * n as f64;
    let make = |s: &mut Session,
                ty: &str,
                owner: Option<String>,
                props: BTreeMap<String, PropValue>,
                x: f64,
                y: f64,
                w: Option<f64>,
                h: Option<f64>|
     -> Result<(String, String), EngineError> {
        let (eid, _) = create_element(&mut s.doc, NewElement { type_id: ty.into(), owner, properties: props, ..NewElement::default() })?;
        let sid = place_shape(&mut s.doc, &eid, pi, x, y, w, h)?;
        Ok((eid, sid))
    };
    let (jb, jb_shape) = make(s, "process.JunctionBox", None, BTreeMap::new(), lane_x(1) + 20.0, top - 40.0, Some(LANE - 40.0), Some(box_h))?;
    let (jb_strip, jb_strip_shape) =
        make(s, "process.TerminalStrip", Some(jb.clone()), BTreeMap::new(), lane_x(1) + 60.0, top - 10.0, Some(100.0), Some(box_h - 40.0))?;
    let (mc, mc_shape) = make(s, "process.MarshallingCabinet", None, BTreeMap::new(), lane_x(2) + 20.0, top - 40.0, Some(LANE - 40.0), Some(box_h))?;
    let (mc_strip, mc_strip_shape) =
        make(s, "process.TerminalStrip", Some(mc.clone()), BTreeMap::new(), lane_x(2) + 60.0, top - 10.0, Some(100.0), Some(box_h - 40.0))?;
    shapes.extend([jb_shape.clone(), jb_strip_shape.clone(), mc_shape.clone(), mc_strip_shape.clone()]);
    for (e, name) in [(&jb, "Junction box"), (&jb_strip, "Terminals"), (&mc, "Marshalling cabinet"), (&mc_strip, "Terminals")] {
        super::model::rename_element(&mut s.doc, e, name)?;
    }
    let set_parent = |s: &mut Session, child: &str, parent: &str| {
        if let Some(sh) = s.doc.pages.get_mut(pi).and_then(|p| p.shape_mut(child)) {
            sh.parent = Some(parent.to_string());
        }
    };
    set_parent(s, &jb_strip_shape, &jb_shape);
    set_parent(s, &mc_strip_shape, &mc_shape);
    let mut terminals = Vec::new();
    let mut channels = Vec::new();
    let mut cables = Vec::new();
    let pairs = wired.len().max(1) as f64;
    for (cable_lane, owner) in [(1usize, lp.clone()), (2usize, lp.clone())] {
        let mut props = BTreeMap::new();
        props.insert("pairs".to_string(), PropValue::Number(pairs));
        let (cid, cs) = make(s, "process.Cable", Some(owner), props, lane_x(cable_lane) - 20.0, top + ROW * pairs - 50.0, None, None)?;
        cables.push(cid);
        shapes.push(cs);
    }
    for (k, (id, _, _)) in wired.iter().enumerate() {
        let y = top + ROW * k as f64;
        let Some((_, dev_shape, role)) = device_shapes.iter().find(|(e, _, _)| e == id).cloned() else { continue };
        let (_, tj) = make(s, "process.Terminal", Some(jb_strip.clone()), BTreeMap::new(), lane_x(1) + 100.0, y + 10.0, None, None)?;
        set_parent(s, &tj, &jb_strip_shape);
        let (_, tm) = make(s, "process.Terminal", Some(mc_strip.clone()), BTreeMap::new(), lane_x(2) + 100.0, y + 10.0, None, None)?;
        set_parent(s, &tm, &mc_strip_shape);
        let mut props = BTreeMap::new();
        props.insert("rack".to_string(), PropValue::Number(1.0));
        props.insert("slot".to_string(), PropValue::Number(1.0));
        props.insert("channel".to_string(), PropValue::Number((k + 1) as f64));
        props.insert("ioType".to_string(), PropValue::Text(if role == "final" { "AO".into() } else { "AI".into() }));
        let (_, io) = make(s, "process.IOChannel", None, props, lane_x(3) + 120.0, y + 10.0, None, None)?;
        terminals.extend([tj.clone(), tm.clone()]);
        channels.push(io.clone());
        shapes.extend([tj.clone(), tm.clone(), io.clone()]);
        // Bubbles land at their east pin; a valve, which has no signal pin, at its top.
        let dev = s.doc.pages.get(pi).and_then(|p| p.shape(&dev_shape)).cloned();
        let from = match dev {
            Some(d) if d.glyph.as_ref().is_some_and(|g| g.builder == "bubble") => json!({"shape": dev_shape, "point": 1}),
            Some(d) => json!({"x": d.x + d.w / 2.0, "y": d.y}),
            None => continue,
        };
        // Each wire takes one bend halfway across, so it runs straight between the lanes.
        let pin_at = |s: &Session, end: &Value| -> Option<Point> {
            match end.get("shape").and_then(Value::as_str) {
                Some(sh) => s.doc.pages.get(pi)?.shape(sh)?.connection_point_world(end.get("point").and_then(Value::as_u64).unwrap_or(0) as usize),
                None => Some(Point::new(end.get("x")?.as_f64()?, end.get("y")?.as_f64()?)),
            }
        };
        let legs = [
            (from, json!({"shape": tj, "point": 0})),
            (json!({"shape": tj, "point": 1}), json!({"shape": tm, "point": 0})),
            (json!({"shape": tm, "point": 1}), json!({"shape": io, "point": 0})),
        ];
        for (a, b) in legs {
            let (Some(pa), Some(pb)) = (pin_at(s, &a), pin_at(s, &b)) else { continue };
            let mx = (pa.x + pb.x) / 2.0;
            call(s, "wire.draw", json!({"from": a, "to": b, "type": "process.loopWire", "points": [{"x": mx, "y": pa.y}, {"x": mx, "y": pb.y}]}))?;
        }
    }
    s.sel.clear();
    Ok(json!({"page": s.doc.pages.get(pi).map(|p| p.id.clone()), "shapes": shapes, "cables": cables, "terminals": terminals, "channels": channels}))
}

#[cfg(test)]
#[path = "plant_tests.rs"]
mod tests;
