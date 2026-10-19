//! Drawing sheet commands: the page's sheet (standard, size, zones, number, title block), its
//! generated frame, zones at a point, revisions, balloons and recomposing context tags.
//!
//! The sheet is data on the page (`vizcraft_diagram::sheet`); its frame, zone labels, centring
//! marks and title block are generated shapes on the page's decoration page
//! (`vizcraft_diagram::decor`), rebuilt after every change. ISO 5457 A3 landscape is the default
//! sheet; ASME Y14.1 is always offered.

use serde_json::{Value, json};
use vizcraft_diagram::decor;
use vizcraft_diagram::sheet::{self, MAX_COLS, MAX_NUMBER, MAX_ROWS, PAPER_SIZES, RevisionScheme, Sheet, SheetSize, SheetStandard, TitleBlockKind};
use vizcraft_diagram::{Board, Connector, Document, End, Orientation, Point, Route};
use vizcraft_model::PropValue;
use vizcraft_model::identity::IdFormat;
use vizcraft_notation::sheet_pass::{self, BALLOON, CALLS_OUT, PART, REVISION};

use super::model::{NewElement, cat, create_element, model_err, place_shape, req_element};
use super::{num, opt_bool, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The connector type of a balloon's leader (the `sheet` pack's).
pub const LEADER: &str = "sheet.leader";

/// Return the sheet commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sheet.set", "Sheet", "Design > Sheet", set).params(
            r#"{"page"?: id|name|index, "sheet"?: "<standard>.<size>.<orientation>" (preset id, see sheet.presets), "standard"?: "iso"|"asme", "size"?: "a0".."a4"|"a".."e", "orientation"?: "landscape"|"portrait", "zones"?: {"cols"?: 1..99, "rows"?: 1..24}, "number"?: 1..9999|null, "titleBlock"?: "iso7200"|"asme"|"none", "revisionScheme"?: "iso7200"|"asmeY14_35", "off"?: true} -> {"page", "sheet", "presetId"}; sets the page to the trimmed size (fixed board, mm for ISO, in for ASME) and draws the frame; with nothing given a page without a sheet gets the document's standard sheet (ISO A3 landscape)"#,
        ),
        CommandSpec::new("sheet.get", "Get Sheet", "Design > Sheet", get).params(
            r#"{"page"?: id|name|index} -> {"page", "sheet"|null, "presetId", "geometry": {"trimmed", "frame", "colEdges", "rowEdges"}, "zones": [{"zone", "x", "y", "w", "h"}]}; read-only"#,
        ),
        CommandSpec::new("sheet.list", "Sheets", "Design > Sheet", list)
            .params(r#"{} -> {"sheets": [{"page", "name", "number", "count", "size", "standard", "presetId"}]}; read-only"#),
        CommandSpec::new("sheet.presets", "Sheet Sizes", "Design > Sheet", presets).params(
            r#"{} -> {"default", "presets": [preset id], "standards": [{"id", "name", "label", "sizes": [{"id", "name", "width", "height", "unit", "zones": {"cols", "rows"}}]}]}; ISO A3 landscape first; read-only"#,
        ),
        CommandSpec::new("sheet.titleBlock", "Title Block", "Design > Sheet", title_block)
            .params(r#"{"page"?: id|name|index, "kind"?: "iso7200"|"asme"|"none" (default: the sheet standard's)} -> {"page", "titleBlock"}; regenerates the frame"#),
        CommandSpec::new("sheet.frames", "Frame", "Design > Sheet", frames)
            .params(r#"{"page"?: id|name|index, "all"?: bool} -> {"pages": n, "shapes": n}; regenerates the frames of the page (or every sheet)"#),
        CommandSpec::new("sheet.zoneAt", "Zone at Point", "Design > Sheet", zone_at)
            .params(r#"{"page"?: id|name|index, "x": number, "y": number} -> {"zone": "B4"|null, "row", "col"}; read-only"#),
        CommandSpec::new("sheet.revise", "Revise", "Design > Sheet", revise).params(
            r#"{"description": string, "approved"?: string, "date"?: "YYYY-MM-DD" (default today), "zone"?: string} -> {"rev", "element"}; without a description the UI opens the Revise dialog and nothing changes; adds a sheet.Revision with the next letter of the sheet's revision scheme and sets the revisionIndex property"#,
        ),
        CommandSpec::new("sheet.balloon", "Balloon", "Design > Sheet", balloon).params(
            r#"{"part": element or shape id, "page"?: id|name|index, "x"?: number, "y"?: number} -> {"balloon", "shape", "relationship", "connector"}; places a balloon calling out the part with a glued leader"#,
        ),
        CommandSpec::new("sheet.renumber", "Renumber", "Design > Sheet", renumber).params(
            r#"{"scope"?: identifier scope id, "page"?: id|name|index} -> {"changed": [{"element", "from", "to"}]}; recomposes tags with {page}, {zone}, {rung} or {owner.tag} parts, keeping their numbers (an empty numbered tag is left to element.retag)"#,
        ),
    ]
}

/// The sheet preset a page gets when `sheet.set` names none: the project's chosen sheet
/// (`Document.standards`, set through `template.standards`), ISO A3 landscape by default.
pub fn default_preset(doc: &Document) -> String {
    doc.standards.sheet().to_string()
}

/// The page a command acts on: the `page` param (id, name or index), else the active page.
pub fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    match v.get("page") {
        None | Some(Value::Null) => Ok(s.doc.active_index()),
        Some(r) => {
            let r = match r {
                Value::String(t) => t.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
            };
            s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))
        }
    }
}

/// A foreground page index from the params.
fn sheet_page(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let i = page_index(s, v)?;
    match s.doc.pages.get(i) {
        Some(p) if p.is_background => Err(EngineError::Params("a background page has no sheet; choose a foreground page".into())),
        Some(_) => Ok(i),
        None => Err(EngineError::Failed("the document has no page".into())),
    }
}

fn bad(k: &str, what: &str) -> EngineError {
    EngineError::Params(format!("`{k}` must be {what}"))
}

/// An optional name param parsed by `parse`.
fn named<T>(v: &Value, k: &str, what: &str, parse: impl Fn(&str) -> Option<T>) -> Result<Option<T>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => parse(t).map(Some).ok_or_else(|| bad(k, what)),
    }
}

/// A whole number in `1..=max`.
fn count(x: &Value, k: &str, max: u32) -> Result<u32, EngineError> {
    x.as_f64()
        .filter(|f| f.is_finite() && f.fract() == 0.0 && *f >= 1.0 && *f <= f64::from(max))
        .map(|f| f as u32)
        .ok_or_else(|| bad(k, &format!("a whole number from 1 to {max}")))
}

fn sheet_json(doc: &Document, i: usize) -> Value {
    let p = doc.pages.get(i);
    let sh = p.and_then(|p| p.sheet.as_ref());
    json!({
        "page": p.map(|p| p.id.clone()),
        "sheet": sh,
        "presetId": sh.map(Sheet::preset_id),
        "number": sh.map(|_| sheet::sheet_number(doc, i)),
    })
}

/// Make page `i` show `sh`: its size, board and units follow the sheet, a sheet-pack view takes
/// the matching variant, the sheet pack is in use (its title block rules run) and the frame is
/// drawn.
pub fn apply(doc: &mut Document, i: usize, sh: Sheet) -> Result<(), EngineError> {
    let page = doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("no such page".into()))?;
    let (w, h) = sh.world_size();
    page.width = w;
    page.height = h;
    page.board = Board::Fixed;
    page.auto_size = false;
    page.units = sh.units();
    page.orientation = sh.orientation;
    if let Some(view) = page.view.as_mut().filter(|v| v.viewpoint.starts_with("sheet.")) {
        view.variant = match sh.standard {
            SheetStandard::Iso5457 => "sheet.iso5457".into(),
            SheetStandard::AsmeY14_1 => "sheet.asmeY14_1".into(),
        };
    }
    page.sheet = Some(sh);
    if let Some(p) = cat().pack_ref("sheet") {
        doc.model.use_pack(p);
    }
    let deco = decor::decoration(doc, i);
    decor::set(doc, i, deco).map_err(|e| EngineError::Params(e.to_string()))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let i = sheet_page(s, v)?;
    if opt_bool(v, "off")?.unwrap_or(false) {
        if let Some(p) = s.doc.pages.get_mut(i) {
            p.sheet = None;
        }
        let deco = decor::decoration(&s.doc, i);
        decor::set(&mut s.doc, i, deco).map_err(|e| EngineError::Params(e.to_string()))?;
        return Ok(sheet_json(&s.doc, i));
    }
    let preset = named(v, "sheet", "a preset id such as iso.a3.landscape (see sheet.presets)", sheet::parse_preset)?;
    let standard = named(v, "standard", "iso or asme", SheetStandard::parse)?;
    let size = named(v, "size", "a0 to a4 or a to e", SheetSize::parse)?;
    let orientation = named(v, "orientation", "landscape or portrait", |t| Orientation::from_name(&t.trim().to_ascii_lowercase()))?;
    let title = named(v, "titleBlock", "iso7200, asme or none", TitleBlockKind::from_name)?;
    let scheme = named(v, "revisionScheme", "iso7200 or asmeY14_35", RevisionScheme::from_name)?;
    let current = s.doc.pages.get(i).and_then(|p| p.sheet);
    let base = match (preset, current) {
        (Some((st, sz, o)), _) => Sheet::preset(st, sz, o),
        (None, Some(c)) => c,
        (None, None) => {
            let (st, sz, o) =
                sheet::parse_preset(&default_preset(&s.doc)).unwrap_or((SheetStandard::Iso5457, SheetSize::IsoA3, Orientation::Landscape));
            Sheet::preset(st, sz, o)
        }
    };
    let mut sh = base;
    if standard.is_some() || size.is_some() || orientation.is_some() {
        let st = standard.unwrap_or(base.standard);
        let sz = size.unwrap_or(if base.size.paper().standard == st {
            base.size
        } else if st == SheetStandard::Iso5457 {
            SheetSize::IsoA3
        } else {
            SheetSize::AnsiB
        });
        sh = Sheet::preset(st, sz, orientation.unwrap_or(base.orientation));
        sh.number = base.number;
    }
    match v.get("zones") {
        None | Some(Value::Null) => {}
        Some(Value::Object(z)) => {
            if let Some(c) = z.get("cols") {
                sh.zones.cols = count(c, "zones.cols", MAX_COLS)?;
            }
            if let Some(r) = z.get("rows") {
                sh.zones.rows = count(r, "zones.rows", MAX_ROWS)?;
            }
        }
        Some(_) => return Err(bad("zones", "an object {cols?, rows?}")),
    }
    match v.get("number") {
        None => {}
        Some(Value::Null) => sh.number = None,
        Some(n) => sh.number = Some(count(n, "number", MAX_NUMBER)?),
    }
    if let Some(t) = title {
        sh.title_block = t;
    }
    if let Some(r) = scheme {
        sh.revision_scheme = r;
    }
    sh.sanitize();
    apply(&mut s.doc, i, sh)?;
    Ok(sheet_json(&s.doc, i))
}

fn rect_json(r: vizcraft_diagram::Rect) -> Value {
    json!({"x": r.x0, "y": r.y0, "w": r.width(), "h": r.height()})
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_index(s, v)?;
    let mut out = sheet_json(&s.doc, i);
    let Some(sh) = s.doc.pages.get(i).and_then(|p| p.sheet) else { return Ok(out) };
    let g = sheet::sheet_geometry(&sh);
    let mut zones = Vec::new();
    for (j, rw) in g.row_edges.windows(2).enumerate() {
        for (k, cw) in g.col_edges.windows(2).enumerate() {
            let (Some(y0), Some(y1), Some(x0), Some(x1)) = (rw.first(), rw.get(1), cw.first(), cw.get(1)) else { continue };
            let zone = format!("{}{}", sheet::row_label(&sh, j as u32), sheet::col_label(&sh, k as u32));
            zones.push(json!({"zone": zone, "x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}));
        }
    }
    if let Some(o) = out.as_object_mut() {
        o.insert(
            "geometry".into(),
            json!({"trimmed": rect_json(g.trimmed), "frame": rect_json(g.frame), "colEdges": g.col_edges, "rowEdges": g.row_edges}),
        );
        o.insert("zones".into(), Value::Array(zones));
    }
    Ok(out)
}

fn list(s: &mut Session, _: &Value) -> CmdResult {
    let count = sheet::sheet_count(&s.doc);
    let sheets: Vec<Value> = s
        .doc
        .pages
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_background)
        .filter_map(|(i, p)| {
            p.sheet.map(|sh| {
                json!({
                    "page": p.id, "name": p.name, "number": sheet::sheet_number(&s.doc, i), "count": count,
                    "size": sh.size.name(), "standard": sh.standard.short(), "presetId": sh.preset_id(),
                })
            })
        })
        .collect();
    Ok(json!({"sheets": sheets}))
}

fn presets(s: &mut Session, _: &Value) -> CmdResult {
    let standards: Vec<Value> = [SheetStandard::Iso5457, SheetStandard::AsmeY14_1]
        .iter()
        .map(|st| {
            let sizes: Vec<Value> = PAPER_SIZES
                .iter()
                .filter(|p| p.standard == *st)
                .map(|p| {
                    json!({
                        "id": p.size.name(), "name": p.name, "width": p.long, "height": p.short,
                        "unit": if p.inches { "in" } else { "mm" }, "zones": {"cols": p.zones.0, "rows": p.zones.1},
                    })
                })
                .collect();
            json!({"id": st.short(), "name": st.name(), "label": st.label(), "sizes": sizes})
        })
        .collect();
    Ok(json!({"default": default_preset(&s.doc), "presets": sheet::preset_ids(), "standards": standards}))
}

/// The sheet of page `i`, or an error naming `sheet.set`.
fn sheet_of(s: &Session, i: usize) -> Result<Sheet, EngineError> {
    s.doc.pages.get(i).and_then(|p| p.sheet).ok_or_else(|| EngineError::Params("the page has no sheet; run sheet.set first".into()))
}

fn title_block(s: &mut Session, v: &Value) -> CmdResult {
    let i = sheet_page(s, v)?;
    let mut sh = sheet_of(s, i)?;
    let kind = named(v, "kind", "iso7200, asme or none", TitleBlockKind::from_name)?;
    sh.title_block = kind.unwrap_or(match sh.standard {
        SheetStandard::Iso5457 => TitleBlockKind::Iso7200,
        SheetStandard::AsmeY14_1 => TitleBlockKind::Asme,
    });
    apply(&mut s.doc, i, sh)?;
    let page = s.doc.pages.get(i).map(|p| p.id.clone());
    Ok(json!({"page": page, "titleBlock": sh.title_block.name()}))
}

fn frames(s: &mut Session, v: &Value) -> CmdResult {
    let pages: Vec<usize> = if opt_bool(v, "all")?.unwrap_or(false) {
        (0..s.doc.pages.len()).filter(|i| s.doc.pages.get(*i).is_some_and(|p| !p.is_background && p.sheet.is_some())).collect()
    } else {
        let i = sheet_page(s, v)?;
        sheet_of(s, i)?;
        vec![i]
    };
    let mut shapes = 0usize;
    for &i in &pages {
        let deco = decor::decoration(&s.doc, i);
        decor::set(&mut s.doc, i, deco).map_err(|e| EngineError::Params(e.to_string()))?;
        let id = s.doc.pages.get(i).map(|p| p.id.clone()).unwrap_or_default();
        shapes = shapes.saturating_add(decor::generated_page(&s.doc, &id).and_then(|g| s.doc.pages.get(g)).map_or(0, |g| g.shapes.len()));
    }
    Ok(json!({"pages": pages.len(), "shapes": shapes}))
}

fn zone_at(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_index(s, v)?;
    let x = super::req_num(v, "x")?;
    let y = super::req_num(v, "y")?;
    let z = s.doc.pages.get(i).and_then(|p| sheet::zone_of(p, Point::new(x, y)));
    Ok(match z {
        Some(z) => json!({"zone": z.to_string(), "row": z.row, "col": z.col}),
        None => json!({"zone": null, "row": null, "col": null}),
    })
}

/// The sheet whose revision scheme revisions follow: the active page's, else the first sheet.
fn revision_scheme(doc: &Document) -> RevisionScheme {
    doc.page().and_then(|p| p.sheet).or_else(|| doc.pages.iter().find_map(|p| p.sheet)).map(|s| s.revision_scheme).unwrap_or_default()
}

/// Today as `YYYY-MM-DD`, or empty without a clock.
fn today() -> String {
    vizcraft_diagram::text_fields::system_now_secs()
        .map(|t| vizcraft_diagram::text_fields::iso_from_secs(t).chars().take(10).collect())
        .unwrap_or_default()
}

fn text_param(v: &Value, k: &str) -> Result<Option<String>, EngineError> {
    Ok(opt_str(v, k)?.map(|t| super::model::cut(t, 1024)))
}

fn revise(s: &mut Session, v: &Value) -> CmdResult {
    let Some(description) = text_param(v, "description")?.filter(|d| !d.trim().is_empty()) else {
        // No description: the UI asks for the revision's fields (the ribbon's Revise button);
        // programmatic callers pass them. Nothing changes.
        s.ui_requests.push(json!({"open": "revise"}));
        return Ok(json!({"rev": null, "element": null}));
    };
    let scheme = revision_scheme(&s.doc);
    let prev = s
        .doc
        .model
        .elements
        .iter()
        .filter(|e| e.type_id == REVISION)
        .filter_map(|e| {
            let r = e.prop("rev").map(|p| p.display()).unwrap_or_default();
            sheet::revision_rank(&r, scheme).map(|k| (k, r))
        })
        .max_by_key(|(k, _)| *k)
        .map(|(_, r)| r)
        .unwrap_or_else(|| "-".into());
    let rev = sheet::next_revision(&prev, scheme).ok_or_else(|| EngineError::Failed(format!("no revision follows {prev} in this scheme")))?;
    let mut props = std::collections::BTreeMap::new();
    props.insert("rev".to_string(), PropValue::Text(rev.clone()));
    props.insert("description".to_string(), PropValue::Text(description));
    props.insert("date".to_string(), PropValue::Text(text_param(v, "date")?.unwrap_or_else(today)));
    for k in ["approved", "zone"] {
        if let Some(t) = text_param(v, k)? {
            props.insert(k.to_string(), PropValue::Text(t));
        }
    }
    let (id, _) = create_element(
        &mut s.doc,
        NewElement { type_id: REVISION.into(), name: format!("Revision {rev}"), tag: Some(rev.clone()), properties: props, ..NewElement::default() },
    )?;
    s.doc.properties.fields.insert("revisionIndex".into(), rev.clone());
    s.doc.properties.sanitize();
    Ok(json!({"rev": rev, "element": id}))
}

fn balloon(s: &mut Session, v: &Value) -> CmdResult {
    let part = req_element(s, v, "part")?;
    if s.doc.model.element(&part).is_none_or(|e| e.type_id != PART) {
        return Err(EngineError::Params(format!("`part`: `{}` is not a sheet.Part", short(&part))));
    }
    let i = sheet_page(s, v)?;
    let view = s
        .doc
        .pages
        .get(i)
        .and_then(|p| p.shapes.iter().find(|sh| sh.element.as_deref() == Some(part.as_str())))
        .map(|sh| (sh.id.clone(), sh.x + sh.w + 40.0, sh.y - 50.0));
    let Some((part_view, dx, dy)) = view else {
        return Err(EngineError::Params("the part is not drawn on this page; place it first".into()));
    };
    let x = num(v, "x")?.unwrap_or(dx).clamp(-vizcraft_diagram::MAX_EXTENT, vizcraft_diagram::MAX_EXTENT);
    let y = num(v, "y")?.unwrap_or(dy).clamp(-vizcraft_diagram::MAX_EXTENT, vizcraft_diagram::MAX_EXTENT);
    let (bid, _) = create_element(&mut s.doc, NewElement { type_id: BALLOON.into(), ..NewElement::default() })?;
    let shape = place_shape(&mut s.doc, &bid, i, x, y, None, None)?;
    let rid = s.doc.next_id("r");
    let rel = vizcraft_model::Relationship::new(rid.clone(), CALLS_OUT, bid.clone(), part.clone());
    s.doc.model.add_relationship(rel).map_err(model_err)?;
    let cid = s.doc.next_id("c");
    let mut c = Connector::new(cid.clone(), End::dynamic(shape.clone()), End::dynamic(part_view));
    // The leader is the pack's connector type, so it lists, restyles and exports like any kind;
    // a catalog without it still gets a straight leader with a dot at the part.
    if super::connector_kind::apply_kind(&mut c, LEADER).is_err() {
        c.route = Route::Straight;
        c.end_arrow = vizcraft_diagram::Arrow::Circle;
    }
    c.relationship = Some(rid.clone());
    c.sanitize();
    if let Some(p) = s.doc.pages.get_mut(i) {
        p.connectors.push(c);
    }
    if i == s.doc.active_index() {
        select(s, vec![shape.clone()]);
    }
    with_sel(s, json!({"balloon": bid, "shape": shape, "relationship": rid, "connector": cid}))
}

fn renumber(s: &mut Session, v: &Value) -> CmdResult {
    let scope_filter = opt_str(v, "scope")?.map(str::to_string);
    let page_filter = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_index(s, v)?),
    };
    let scopes = cat().scopes();
    if let Some(f) = &scope_filter
        && !scopes.iter().any(|sc| &sc.id == f)
    {
        return Err(EngineError::Params(format!("no identifier scope `{}`", short(f))));
    }
    // An owner's tag feeds its members' tags, so recompose until nothing changes (at most the
    // nesting depth a chain of owners can reach in a few passes).
    let mut all: Vec<(String, String, String)> = Vec::new();
    for _ in 0..8 {
        let changes = renumber_pass(s, &scopes, scope_filter.as_deref(), page_filter)?;
        if changes.is_empty() {
            break;
        }
        for (id, from, to) in changes {
            if let Some(e) = s.doc.model.element_mut(&id) {
                e.tag = super::model::cut(&to, 256);
            }
            match all.iter_mut().find(|(x, _, _)| *x == id) {
                Some(c) => c.2 = to,
                None => all.push((id, from, to)),
            }
        }
    }
    let changes = all;
    let changed: Vec<Value> = changes.into_iter().map(|(element, from, to)| json!({"element": element, "from": from, "to": to})).collect();
    Ok(json!({"changed": changed}))
}

/// One renumbering pass: `(element, from, to)` for every tag that recomposes differently.
fn renumber_pass(
    s: &Session,
    scopes: &[vizcraft_model::IdScope],
    scope_filter: Option<&str>,
    page_filter: Option<usize>,
) -> Result<Vec<(String, String, String)>, EngineError> {
    let scopes = scopes.to_vec();
    let scope_filter = scope_filter.map(str::to_string);
    let ctxs = sheet::compose_ctxs(&s.doc);
    let views = sheet::first_views(&s.doc);
    let mut changes: Vec<(String, String, String)> = Vec::new();
    let model = &s.doc.model;
    let reg = vizcraft_model::Registry::new(model, &scopes);
    for e in &model.elements {
        let Some(scope) = reg.scope_for(&e.type_id) else { continue };
        if scope_filter.as_ref().is_some_and(|f| f != &scope.id) {
            continue;
        }
        if let Some(pi) = page_filter
            && views.get(e.id.as_str()).is_none_or(|(p, _)| *p != pi)
        {
            continue;
        }
        let Ok(format) = IdFormat::parse(&scope.format) else { continue };
        let ctx = ctxs.get(&e.id).cloned().unwrap_or_default();
        // A numbered tag that is still empty has no number to keep: `element.retag` gives one.
        if e.tag.is_empty() && format.has_number() {
            continue;
        }
        let Some(want) = sheet_pass::recomposed(model, &format, e, &ctx) else { continue };
        if want != e.tag {
            changes.push((e.id.clone(), e.tag.clone(), want));
        }
    }
    Ok(changes)
}

#[cfg(test)]
#[path = "sheet/tests.rs"]
mod tests;
