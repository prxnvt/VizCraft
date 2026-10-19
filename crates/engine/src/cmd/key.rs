//! Keys (`key.*`): a generated legend on a page that explains the element types, line kinds and
//! numbered steps drawn on it.
//!
//! `key.insert` builds the rows from the page (the bound element types in first-seen order, the
//! connector kinds, then the steps), lays them out with `vizcraft_diagram::key::block` and draws
//! them with `title_block::shapes`, plus a small sample of each type's master, a short line of
//! each kind glued between two invisible anchors (so it moves with the key) and a badge per step.
//! Everything is grouped and marked with `KEY_EXT`; nothing is bound, so the model rules never
//! see it, and the drawing checks and the layout commands leave it alone. The group keeps the
//! key's settings (title, exclusions, steps) so `key.refresh` rebuilds it the same way, in place.
//! Freshness is explicit: the key changes only when one of these commands runs.

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::key::{self as k, KEY_EXT, KeyRow, Sample};
use vizcraft_diagram::{Color, Connector, End, Page, Point, Shape, ShapeKind, Style, arrange, sheet, title_block};

use super::model::{cat, page_param};
use super::{num, opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The group `ext` key holding the key's heading.
pub const TITLE_EXT: &str = "vizcraft.key.title";
/// The group `ext` key holding the excluded type and kind ids, comma separated.
pub const EXCLUDE_EXT: &str = "vizcraft.key.exclude";
/// The group `ext` key that is `off` when the key leaves steps out.
pub const STEPS_EXT: &str = "vizcraft.key.steps";

/// Space between the drawing and a key placed beside it, in world units.
pub const GAP: f64 = 40.0;

/// Longest title, in bytes.
const MAX_TITLE: usize = 128;

/// Most ids `exclude` may list.
const MAX_EXCLUDE: usize = 256;

/// Return the key commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("key.insert", "Insert Key", "Notation > Key", insert).params(
            r#"{"page"?: page, "x"?: n, "y"?: n, "title"?: string (default "Key"), "exclude"?: [type or kind id], "steps"?: bool (default true)} -> {"page", "id": group id | null, "rows": [{"label", "kind": "type"|"line"|"step", "ref"}], "truncated"}; replaces the page's key, keeping its corner unless x and y are given (default: 40 right of the drawing); a page with nothing to explain gets no key"#,
        ),
        CommandSpec::new("key.refresh", "Refresh Key", "Notation > Key", refresh)
            .params(r#"{"page"?: page, "all"?: bool} -> {"pages", "rows"}; rebuilds the key of the page (or of every page with one) with its own settings, in place"#),
        CommandSpec::new("key.remove", "Remove Key", "Notation > Key", remove).params(r#"{"page"?: page} -> {"removed"}"#),
        CommandSpec::new("key.get", "Key", "Notation > Key", get)
            .params(r#"{"page"?: page} -> {"id": group id | null, "rows", "exclude", "x", "y"}; the rows are those the key shows when refreshed"#),
    ]
}

/// The key's settings.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    title: String,
    exclude: Vec<String>,
    steps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { title: "Key".into(), exclude: Vec::new(), steps: true }
    }
}

/// The key group on `page`, if it has one.
fn key_group(page: &Page) -> Option<&Shape> {
    page.shapes.iter().find(|s| k::is_key(s) && arrange::is_group(s))
}

/// The settings stored on a key group.
fn stored(g: &Shape) -> Settings {
    let d = Settings::default();
    Settings {
        title: g.ext.get(TITLE_EXT).cloned().unwrap_or(d.title),
        exclude: g.ext.get(EXCLUDE_EXT).map(|e| e.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
        steps: g.ext.get(STEPS_EXT).is_none_or(|v| v != "off"),
    }
}

/// One row with what it refers to.
struct Row {
    row: KeyRow,
    kind: &'static str,
    reference: String,
}

fn rows_json(rows: &[Row]) -> Value {
    Value::Array(rows.iter().map(|r| json!({"label": r.row.label, "kind": r.kind, "ref": r.reference})).collect())
}

/// The rows `page` needs: element types, then connector kinds, then steps.
fn rows_of(doc: &crate::Document, page: &Page, set: &Settings) -> Vec<Row> {
    let c = cat();
    let variant = page.view.as_ref().map(|v| v.variant.as_str()).unwrap_or_default();
    let skip: HashSet<&str> = set.exclude.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for s in page.shapes.iter().filter(|s| !k::is_key(s)) {
        let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
        let t = e.type_id.as_str();
        if skip.contains(t) || !seen.insert(t.to_string()) {
            continue;
        }
        let Some(master) = c.master_for_type_in(t, variant) else { continue };
        let label =
            c.element_type(t).map(|(_, et)| et.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| vizcraft_model::local_of(t).to_string());
        out.push(Row { row: KeyRow { label, sample: Sample::Glyph(master.to_string()) }, kind: "type", reference: t.to_string() });
    }
    for con in page.connectors.iter().filter(|c| !k::is_key_connector(c)) {
        let rel_type = con.relationship.as_deref().and_then(|r| doc.model.relationship(r)).map(|r| r.type_id.clone());
        let kind = if con.kind.is_empty() {
            rel_type.as_deref().and_then(|t| c.connector_type_for_relationship_in(t, variant)).map(str::to_string)
        } else {
            Some(con.kind.clone())
        };
        let Some(kind) = kind.filter(|kd| vizcraft_stencils::connector_kind(kd).is_some()) else { continue };
        if skip.contains(kind.as_str()) || rel_type.as_deref().is_some_and(|t| skip.contains(t)) || !seen.insert(kind.clone()) {
            continue;
        }
        let from_kind = vizcraft_stencils::connector_kind(&kind).map(|kd| kd.label.to_string()).filter(|l| !l.is_empty());
        let from_rel = rel_type.as_deref().and_then(|t| c.relationship_type(t)).map(|(_, rt)| rt.label.clone()).filter(|l| !l.is_empty());
        let label = from_kind.or(from_rel).unwrap_or_else(|| vizcraft_model::local_of(&kind).to_string());
        out.push(Row { row: KeyRow { label, sample: Sample::Line(kind.clone()) }, kind: "line", reference: kind });
    }
    if set.steps {
        let mut steps: Vec<&Connector> = page.connectors.iter().filter(|c| c.step > 0 && !k::is_key_connector(c)).collect();
        steps.sort_by_key(|c| c.step);
        for con in steps {
            let name = con.relationship.as_deref().and_then(|r| doc.model.relationship(r)).map(|r| r.name.clone()).filter(|n| !n.trim().is_empty());
            let text = name.unwrap_or_else(|| con.text.clone());
            let label = format!("{}  {}", con.step, text.lines().next().unwrap_or_default()).trim_end().to_string();
            out.push(Row { row: KeyRow { label, sample: Sample::Step(con.step) }, kind: "step", reference: con.id.clone() });
        }
    }
    out
}

/// Remove every key item on `page`; returns how many went.
fn clear(page: &mut Page) -> usize {
    let before = page.shapes.len() + page.connectors.len();
    page.shapes.retain(|s| !k::is_key(s));
    page.connectors.retain(|c| !k::is_key_connector(c));
    before - page.shapes.len() - page.connectors.len()
}

/// Where a new key goes: right of the drawing (key items left out), level with its top.
fn beside(page: &Page) -> Point {
    let mut b: Option<(f64, f64, f64)> = None;
    for s in page.shapes.iter().filter(|s| !k::is_key(s)) {
        let r = s.bounds();
        if ![r.x0, r.y0, r.x1].iter().all(|v| v.is_finite()) {
            continue;
        }
        b = Some(match b {
            None => (r.x1, r.y0, r.x1),
            Some((_, y0, x1)) => (x1.max(r.x1), y0.min(r.y0), x1.max(r.x1)),
        });
    }
    match b {
        Some((_, y0, x1)) => Point::new(x1 + GAP, y0),
        None => Point::new(GAP, GAP),
    }
}

/// Draw the key for `set` on page `i` at `at` (or where the old key was, else beside the drawing).
/// Returns the group id (none when there is nothing to explain), the rows and whether some rows
/// did not fit.
fn build(s: &mut Session, i: usize, set: &Settings, at: Option<Point>) -> Result<(Option<String>, Vec<Row>, bool), EngineError> {
    let page = s.doc.pages.get(i).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let corner = at.or_else(|| key_group(page).map(|g| Point::new(g.x, g.y)));
    let rows = rows_of(&s.doc, page, set);
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Params("no such page".into()))?;
    clear(page);
    if rows.is_empty() {
        return Ok((None, rows, false));
    }
    let truncated = rows.len() > k::MAX_ROWS;
    let origin = corner.unwrap_or_else(|| beside(page));
    let gid = s.doc.next_id("key");
    let key_rows: Vec<KeyRow> = rows.iter().map(|r| r.row.clone()).collect();
    let (block, cells) = k::block(&set.title, &key_rows);
    let u = sheet::WORLD_PER_MM;
    let mut shapes = title_block::shapes(&block, origin, u, |role| Some(format!("{gid}_{role}")));
    let mut connectors = Vec::new();
    for cell in &cells {
        let (cx, cy, cw, ch) = (origin.x + cell.x * u, origin.y + cell.y * u, cell.w * u, cell.h * u);
        let id = |what: &str| format!("{gid}_s{}{what}", cell.row);
        match &cell.sample {
            Sample::Glyph(master) => {
                let Some(m) = vizcraft_stencils::master(master) else { continue };
                let (bw, bh) = (cw - 4.0 * u, ch - 2.0 * u);
                let scale = (bw / m.w.max(1.0)).min(bh / m.h.max(1.0)).min(1.0);
                let (w, h) = ((m.w * scale).max(2.0), (m.h * scale).max(2.0));
                let mut sh = m.instantiate(id(""), cx + (cw - w) / 2.0, cy + (ch - h) / 2.0, Some(w), Some(h));
                // A small sample shows a container's border, not a heading it has no room for,
                // and holds no members.
                sh.container = None;
                sh.behavior.container = false;
                sh.text_block = None;
                shapes.push(sh);
            }
            Sample::Line(kind) => {
                let my = cy + ch / 2.0;
                let anchor = |sid: String, x: f64| {
                    let mut a = Shape::new(sid, ShapeKind::Rectangle, x, my - 0.5, 1.0, 1.0);
                    a.style = Style { fill: Color::TRANSPARENT, line: Color::TRANSPARENT, line_width: 0.0, ..Style::DEFAULT };
                    a
                };
                let (a, b) = (anchor(id("a"), cx + 2.0 * u), anchor(id("b"), cx + cw - 2.0 * u - 1.0));
                let mut c = Connector::new(id("c"), End::point(a.id.clone(), 1), End::point(b.id.clone(), 3));
                super::connector_kind::apply_kind(&mut c, kind)?;
                c.route = vizcraft_diagram::Route::Straight;
                c.avoid = false;
                shapes.push(a);
                shapes.push(b);
                connectors.push(c);
            }
            Sample::Step(n) => {
                let d = (ch - 3.0 * u).max(4.0);
                let mut b = Shape::new(id(""), ShapeKind::Ellipse, cx + (cw - d) / 2.0, cy + (ch - d) / 2.0, d, d);
                let ink = Color::rgb(0x3a, 0x4a, 0x5c);
                b.style = Style {
                    fill: ink,
                    line: Color::WHITE,
                    font_color: Color::WHITE,
                    bold: true,
                    font_size: (d * 0.55).clamp(6.0, 24.0),
                    ..Style::DEFAULT
                };
                b.text = n.to_string();
                shapes.push(b);
            }
        }
    }
    for sh in &mut shapes {
        sh.ext.insert(KEY_EXT.into(), gid.clone());
        sh.sanitize();
    }
    for c in &mut connectors {
        c.ext.insert(KEY_EXT.into(), gid.clone());
        c.sanitize();
    }
    let ids: Vec<String> = shapes.iter().map(|sh| sh.id.clone()).collect();
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Params("no such page".into()))?;
    page.shapes.extend(shapes);
    page.connectors.extend(connectors);
    arrange::group(page, &ids, &gid).map_err(|e| EngineError::Failed(format!("the key could not be grouped: {e}")))?;
    if let Some(g) = page.shape_mut(&gid) {
        g.ext.insert(KEY_EXT.into(), gid.clone());
        g.ext.insert(TITLE_EXT.into(), set.title.clone());
        if !set.exclude.is_empty() {
            g.ext.insert(EXCLUDE_EXT.into(), set.exclude.join(","));
        }
        if !set.steps {
            g.ext.insert(STEPS_EXT.into(), "off".into());
        }
        g.text.clear();
    }
    Ok((Some(gid), rows, truncated))
}

/// The `exclude` param: type or kind ids.
fn exclude_param(v: &Value) -> Result<Option<Vec<String>>, EngineError> {
    match v.get("exclude") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= MAX_EXCLUDE => a
            .iter()
            .map(|x| match x.as_str() {
                Some(t) if !t.is_empty() && t.len() <= 128 && !t.contains(',') => Ok(t.to_string()),
                _ => Err(EngineError::Params("`exclude` holds type or kind ids".into())),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(EngineError::Params(format!("`exclude` must be a list of at most {MAX_EXCLUDE} ids"))),
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let at = match (num(v, "x")?, num(v, "y")?) {
        (Some(x), Some(y)) => Some(Point::new(x.clamp(-1e6, 1e6), y.clamp(-1e6, 1e6))),
        (None, None) => None,
        _ => return Err(EngineError::Params("give both `x` and `y`, or neither".into())),
    };
    let mut set = s.doc.pages.get(i).and_then(key_group).map(stored).unwrap_or_default();
    if let Some(t) = opt_str(v, "title")? {
        if t.len() > MAX_TITLE {
            return Err(EngineError::Params(format!("`title` is longer than {MAX_TITLE} bytes")));
        }
        set.title = t.to_string();
    }
    if let Some(e) = exclude_param(v)? {
        set.exclude = e;
    }
    if let Some(b) = opt_bool(v, "steps")? {
        set.steps = b;
    }
    let (id, rows, truncated) = build(s, i, &set, at)?;
    let page = s.doc.pages.get(i).map(|p| p.id.clone());
    Ok(json!({"page": page, "id": id, "rows": rows_json(&rows), "truncated": truncated}))
}

fn refresh(s: &mut Session, v: &Value) -> CmdResult {
    let pages: Vec<usize> = if opt_bool(v, "all")?.unwrap_or(false) {
        (0..s.doc.pages.len()).filter(|i| s.doc.pages.get(*i).and_then(key_group).is_some()).collect()
    } else {
        let i = page_param(&s.doc, v)?;
        if s.doc.pages.get(i).and_then(key_group).is_some() { vec![i] } else { Vec::new() }
    };
    let mut rows = 0usize;
    for &i in &pages {
        let set = s.doc.pages.get(i).and_then(key_group).map(stored).unwrap_or_default();
        rows = rows.saturating_add(build(s, i, &set, None)?.1.len());
    }
    Ok(json!({"pages": pages.len(), "rows": rows}))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let removed = clear(page);
    super::prune_selection(s);
    Ok(json!({"removed": removed}))
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let page = s.doc.pages.get(i).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&i.to_string()))))?;
    let Some(g) = key_group(page) else {
        return Ok(json!({"id": null, "rows": [], "exclude": [], "x": null, "y": null}));
    };
    let set = stored(g);
    let rows = rows_of(&s.doc, page, &set);
    Ok(json!({"id": g.id, "title": set.title, "rows": rows_json(&rows), "exclude": set.exclude, "steps": set.steps, "x": g.x, "y": g.y}))
}

#[cfg(test)]
mod tests;
