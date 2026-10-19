//! Page commands (`page.*`): list, read and replace pages; add, delete, duplicate, rename and
//! reorder them; switch the active page; page setup, background pages and fitting the page to
//! the drawing.
//!
//! A page is named by id, name or zero-based index (`Document::find_page`). Where `page` is
//! optional it defaults to the active page (`Document::page`), the one the canvas shows and every
//! other page-scoped command acts on. With `path`, `page.list`, `page.get` and `page.set` work on
//! that file (`.drawio` or `.vizcraft`) and leave the session alone; without it they work on the
//! open document. `page.set` without a path is `file.import` with `format: "drawio"` and
//! `replace: true`, and shares its code.
//!
//! Every editing command is one undo step. Switching the active page is not: it changes what is
//! shown, not the drawing, so it neither makes an undo step nor marks the document edited.

use std::collections::HashMap;

use serde_json::{Value, json};
use vizcraft_diagram::{Board, DrawingScale, End, Orientation, Page, Rect, Units};

use super::file::{import_into_session, import_opts, place};
use super::{drop_undrawn_relationships, num, opt_bool, opt_str, short};
use crate::doc::Document;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Most pages a document may have; `page.add` and `page.duplicate` refuse past it.
pub const MAX_PAGES: usize = 1000;

/// Longest page name, in characters.
pub const MAX_PAGE_NAME: usize = 256;

/// Space `page.fitToDrawing` leaves around the drawing when no `margin` is given (a quarter inch).
pub const FIT_MARGIN: f64 = vizcraft_diagram::board::FIT_MARGIN;

/// Largest `margin` `page.fitToDrawing` accepts.
pub const MAX_FIT_MARGIN: f64 = 10_000.0;

/// Space an auto-size page keeps beyond its furthest shape, and the step its size grows in.
pub const AUTO_SIZE_STEP: f64 = 50.0;

/// Units a page can measure in, in the order the Page Setup dialog lists them.
pub const UNITS: [Units; 7] = [Units::Inches, Units::Feet, Units::Millimeters, Units::Centimeters, Units::Meters, Units::Points, Units::Pixels];

/// The page commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("page.list", "List Pages", "Page", list).params(
            r#"{"path"?: string} -> {pages: [{index, id, name, width, height, shapes, connectors, isBackground, background, active}], active}"#,
        ),
        CommandSpec::new("page.get", "Get Page", "Page", get)
            .params(r#"{"page"?: id|name|index (default the active page), "path"?: string, "format"?: "drawio"|"vizcraft"} -> {page, format, content}"#),
        CommandSpec::new("page.set", "Set Page", "Page", set).params(
            r#"{"page": id|name|index, "xml": string, "path"?: string, "layout"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL", "routing"?: "orthogonal"} -> {page, ids, warnings} (replaces the page from draw.io XML; a name that matches nothing adds a page; with path the file is edited and saved, the open document is untouched)"#,
        ),
        CommandSpec::new("page.add", "New Page", "Insert > Pages", add).params(
            r#"{"name"?: string, "background"?: bool, "index"?: number} -> {page, index, name} (takes the active page's size and setup; a foreground page goes after the active page and shares its background, a background page goes last; the new page becomes active)"#,
        ),
        CommandSpec::new("page.delete", "Delete Page", "Page", delete)
            .params(r#"{"page"?: id|name|index} -> {deleted, active} (refused for the last foreground page; references to a deleted background page are cleared)"#),
        CommandSpec::new("page.duplicate", "Duplicate Page", "Insert > Pages", duplicate)
            .params(r#"{"page"?: id|name|index} -> {page, index, name, ids} (a copy after it with fresh ids, made active)"#),
        CommandSpec::new("page.rename", "Rename Page", "Page", rename)
            .params(r#"{"page"?: id|name|index, "name": string} -> {page, name} (names are unique and at most 256 characters)"#),
        CommandSpec::new("page.move", "Reorder Page", "Page", move_page).params(r#"{"page": id|name|index, "index": number} -> {page, index}"#),
        CommandSpec::new("page.setActive", "Go to Page", "Page", set_active)
            .params(r#"{"page": id|name|index} -> {page, index} (not an undo step)"#),
        CommandSpec::new("page.setup", "Page Setup", "Design > Page Setup", setup).params(
            r#"{"page"?: id|name|index, "width"?: number, "height"?: number, "size"?: "A0".."A4"|"ANSI A".."ANSI E" (a paper size, instead of width and height), "orientation"?: "portrait"|"landscape", "units"?: "in"|"ft"|"mm"|"cm"|"m"|"pt"|"px", "drawingScale"?: {page, drawing, units?}, "autoSize"?: bool, "board"?: "infinite"|"fixed"} -> {page, board, width, height, orientation, units, drawingScale, autoSize} (board is infinite for new pages; width, height, orientation and autoSize apply to a fixed board only and are kept while it is infinite; orientation swaps width and height to match; with no params the UI opens the Page Setup dialog)"#,
        ),
        CommandSpec::new("page.setBackground", "Background Page", "Page", set_background).params(
            r#"{"page"?: id|name|index, "background": id|name|index|null} -> {page, background} (only a background page may be assigned, never one that would make a cycle)"#,
        ),
        CommandSpec::new("page.fitToDrawing", "Fit to Drawing", "Design > Page Setup", fit_to_drawing)
            .params(r#"{"page"?: id|name|index, "margin"?: number (default 25)} -> {page, width, height, dx, dy} (moves the drawing to the margin and sizes the page around it)"#),
    ]
}

/// The document a command reads: the file at `path` when given, else the open one.
fn source<'a>(s: &'a Session, path: Option<&str>) -> Result<std::borrow::Cow<'a, Document>, EngineError> {
    match path {
        Some(path) => Ok(std::borrow::Cow::Owned(load_file(path)?)),
        None => Ok(std::borrow::Cow::Borrowed(&s.doc)),
    }
}

fn load_file(path: &str) -> Result<Document, EngineError> {
    if !crate::io::is_document(path) {
        return Err(EngineError::Params(format!("{}: page commands read .drawio and .vizcraft files", short(path))));
    }
    let bytes = crate::io::read_limited(path)?;
    crate::io::load_bytes(path, &bytes)
}

/// A page param (`key`) as text: a string, or a number meaning an index.
fn ref_param(v: &Value, key: &str) -> Result<Option<String>, EngineError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(EngineError::Params(format!("`{key}` must be a page id, name or index"))),
    }
}

/// The `page` param as text.
fn page_ref(v: &Value) -> Result<Option<String>, EngineError> {
    ref_param(v, "page")
}

/// The index of the page `r` names in `doc`.
fn resolve(doc: &Document, r: &str) -> Result<usize, EngineError> {
    doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r))))
}

/// The page a command acts on: the `page` param, else the active page.
fn target(s: &Session, v: &Value) -> Result<usize, EngineError> {
    match page_ref(v)? {
        Some(r) => resolve(&s.doc, &r),
        None => Ok(s.doc.active_index()),
    }
}

/// The page a command acts on, which the caller must name.
fn required_target(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let r = page_ref(v)?.ok_or_else(|| EngineError::Params("`page` (id, name or index) is required".into()))?;
    resolve(&s.doc, &r)
}

/// A non-negative whole-number param.
fn index_param(v: &Value, key: &str) -> Result<Option<usize>, EngineError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .map(Some)
            .ok_or_else(|| EngineError::Params(format!("`{key}` must be a whole number, 0 or more"))),
    }
}

/// Name the active page by id, so a command that inserts, removes or reorders pages keeps the
/// same page active (an empty `active_page` means "the first", which would move with the order).
fn pin_active(doc: &mut Document) {
    let i = doc.active_index();
    doc.set_active_index(i);
}

/// `name` trimmed and checked: not empty, not too long, and not the name of another page.
fn checked_name(doc: &Document, name: &str, except: Option<usize>) -> Result<String, EngineError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EngineError::Params("a page name cannot be empty".into()));
    }
    if name.chars().count() > MAX_PAGE_NAME {
        return Err(EngineError::Params(format!("a page name is at most {MAX_PAGE_NAME} characters")));
    }
    if doc.pages.iter().enumerate().any(|(i, p)| Some(i) != except && p.name == name) {
        return Err(EngineError::Params(format!("there is already a page called `{}`", short(name))));
    }
    Ok(name.to_string())
}

/// The first of `stem-1`, `stem-2`, ... that no page is called.
fn unique_name(doc: &Document, stem: &str) -> String {
    let mut n = doc.pages.len().saturating_add(1);
    loop {
        let name = format!("{stem}-{n}");
        if !doc.pages.iter().any(|p| p.name == name) || n == usize::MAX {
            return name;
        }
        n = n.saturating_add(1);
    }
}

/// A free name for a copy of the page called `name`: `name (2)`, `name (3)`, ...
fn copy_name(doc: &Document, name: &str) -> String {
    let base: String = name.chars().take(MAX_PAGE_NAME.saturating_sub(8)).collect();
    let mut n: usize = 2;
    loop {
        let candidate = format!("{base} ({n})");
        if !doc.pages.iter().any(|p| p.name == candidate) || n == usize::MAX {
            return candidate;
        }
        n = n.saturating_add(1);
    }
}

fn refuse_when_full(doc: &Document) -> Result<(), EngineError> {
    if doc.pages.len() >= MAX_PAGES {
        return Err(EngineError::Params(format!("a document has at most {MAX_PAGES} pages")));
    }
    Ok(())
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let doc = source(s, opt_str(v, "path")?)?;
    let active = doc.active_index();
    let pages: Vec<Value> = doc
        .pages
        .iter()
        .enumerate()
        .map(|(index, p)| {
            json!({
                "index": index,
                "id": p.id,
                "name": p.name,
                "width": p.width,
                "height": p.height,
                "board": p.board,
                "frame": frame_json(p),
                "shapes": p.shapes.len(),
                "connectors": p.connectors.len(),
                "isBackground": p.is_background,
                "background": p.background,
                "active": index == active,
            })
        })
        .collect();
    let active_id = doc.pages.get(active).map(|p| p.id.clone());
    Ok(json!({"pages": pages, "active": active_id}))
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let format = match opt_str(v, "format")?.map(|f| f.trim().to_ascii_lowercase()) {
        None => "drawio",
        Some(f) if f == "drawio" || f == "xml" => "drawio",
        Some(f) if f == "vizcraft" || f == "json" => "vizcraft",
        Some(f) => return Err(EngineError::Params(format!("`format` must be drawio or vizcraft, not `{}`", short(&f)))),
    };
    let r = page_ref(v)?;
    let doc = source(s, opt_str(v, "path")?)?;
    let index = match &r {
        None => doc.active_index(),
        Some(r) => resolve(&doc, r)?,
    };
    let page = doc.pages.get(index).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let content = match format {
        "drawio" => vizcraft_formats::drawio::page_xml(page).map_err(|e| EngineError::Failed(e.to_string()))?,
        _ => serde_json::to_string_pretty(page).map_err(|e| EngineError::Failed(e.to_string()))?,
    };
    Ok(json!({"page": page.id, "format": format, "content": content}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let Some(r) = page_ref(v)? else { return Err(EngineError::Params("`page` (id, name or index) is required".into())) };
    let xml = opt_str(v, "xml")?.ok_or_else(|| EngineError::Params("`xml` (string) is required".into()))?;
    if xml.len() as u64 > crate::io::MAX_IMPORT_BYTES {
        return Err(EngineError::Params(format!("`xml` is longer than {} MiB", crate::io::MAX_IMPORT_BYTES / (1024 * 1024))));
    }
    let mut opts = import_opts(v)?;
    opts.page = Some(r);
    opts.replace = true;
    let imported = crate::io::import_bytes("drawio", xml.as_bytes())?;
    let Some(path) = opt_str(v, "path")? else {
        return import_into_session(s, imported, &opts, None);
    };
    let mut doc = load_file(path)?;
    let placed = place(&mut doc, imported, &opts)?;
    let bytes = crate::io::save_bytes(path, &doc)?;
    std::fs::write(path, bytes).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    let page = doc.pages.get(placed.index).map(|p| p.id.clone()).unwrap_or_default();
    Ok(json!({"page": page, "ids": placed.ids, "warnings": placed.warnings, "path": path}))
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let name = opt_str(v, "name")?;
    let background = opt_bool(v, "background")?.unwrap_or(false);
    let index = index_param(v, "index")?;
    refuse_when_full(&s.doc)?;
    let name = match name {
        Some(n) => checked_name(&s.doc, n, None)?,
        None => unique_name(&s.doc, if background { "Background" } else { "Page" }),
    };
    pin_active(&mut s.doc);
    let active = s.doc.active_index();
    let id = s.doc.next_id("p");
    let mut page = Page::new(id.clone(), name.clone());
    if let Some(from) = s.doc.page() {
        page.width = from.width;
        page.height = from.height;
        page.orientation = from.orientation;
        page.units = from.units;
        page.drawing_scale = from.drawing_scale;
        page.auto_size = from.auto_size;
        if !background && !from.is_background {
            page.background = from.background.clone();
        }
    }
    page.is_background = background;
    let len = s.doc.pages.len();
    let at = index.unwrap_or(if background { len } else { active.saturating_add(1) }).min(len);
    s.doc.pages.insert(at, page);
    s.doc.active_page = id.clone();
    s.sel.clear();
    Ok(json!({"page": id, "index": at, "name": name}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let i = target(s, v)?;
    let Some(page) = s.doc.pages.get(i) else { return Err(EngineError::Failed("the document has no page".into())) };
    if s.doc.pages.len() <= 1 {
        return Err(EngineError::Params("a document needs at least one page".into()));
    }
    if !page.is_background && s.doc.pages.iter().filter(|p| !p.is_background).count() <= 1 {
        return Err(EngineError::Params("the last foreground page cannot be deleted".into()));
    }
    pin_active(&mut s.doc);
    let was_active = s.doc.active_index() == i;
    let removed = s.doc.pages.remove(i);
    for p in &mut s.doc.pages {
        if p.background.as_deref() == Some(removed.id.as_str()) {
            p.background = None;
        }
    }
    s.doc.comments.retain(|c| c.anchor.page != removed.id);
    let relationships: Vec<String> = removed.connectors.iter().filter_map(|c| c.relationship.clone()).collect();
    drop_undrawn_relationships(&mut s.doc, &relationships);
    if was_active {
        // The page that took its place, else the one before it; a foreground page when there is
        // one, so deleting a page never leaves a background page showing on its own.
        let near = i.min(s.doc.pages.len().saturating_sub(1));
        let next = if s.doc.pages.get(near).is_some_and(|p| !p.is_background) {
            near
        } else {
            s.doc.pages.iter().position(|p| !p.is_background).unwrap_or(near)
        };
        s.doc.set_active_index(next);
    }
    s.sel.clear();
    let active = s.doc.page().map(|p| p.id.clone());
    Ok(json!({"deleted": removed.id, "active": active}))
}

fn duplicate(s: &mut Session, v: &Value) -> CmdResult {
    let i = target(s, v)?;
    refuse_when_full(&s.doc)?;
    let Some(src) = s.doc.pages.get(i).cloned() else { return Err(EngineError::Failed("the document has no page".into())) };
    let mut page = src.clone();
    page.id = s.doc.next_id("p");
    page.name = copy_name(&s.doc, &src.name);
    let mut map: HashMap<String, String> = HashMap::new();
    let mut ids = Vec::new();
    for sh in &mut page.shapes {
        let id = s.doc.next_id("s");
        map.insert(std::mem::replace(&mut sh.id, id.clone()), id.clone());
        ids.push(id);
    }
    for sh in &mut page.shapes {
        sh.parent = sh.parent.as_deref().and_then(|p| map.get(p)).cloned();
    }
    let mut cmap: HashMap<String, String> = HashMap::new();
    for c in &mut page.connectors {
        let cid = s.doc.next_id("c");
        cmap.insert(std::mem::replace(&mut c.id, cid), c.id.clone());
        ids.push(c.id.clone());
        for end in [&mut c.start, &mut c.end] {
            if let End::Glued { shape, .. } = end
                && let Some(new) = map.get(shape.as_str())
            {
                *shape = new.clone();
            }
        }
    }
    // A sequence page's event order and parts name the items by id.
    vizcraft_diagram::sequence::rename_ids(&mut page, &map, &cmap);
    let (id, name) = (page.id.clone(), page.name.clone());
    pin_active(&mut s.doc);
    let at = i.saturating_add(1).min(s.doc.pages.len());
    s.doc.pages.insert(at, page);
    s.doc.active_page = id.clone();
    s.sel.clear();
    Ok(json!({"page": id, "index": at, "name": name, "ids": ids}))
}

fn rename(s: &mut Session, v: &Value) -> CmdResult {
    let i = target(s, v)?;
    let name = opt_str(v, "name")?.ok_or_else(|| EngineError::Params("`name` (string) is required".into()))?;
    let name = checked_name(&s.doc, name, Some(i))?;
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    page.name = name.clone();
    Ok(json!({"page": page.id, "name": name}))
}

fn move_page(s: &mut Session, v: &Value) -> CmdResult {
    let i = required_target(s, v)?;
    let to = index_param(v, "index")?.ok_or_else(|| EngineError::Params("`index` (number) is required".into()))?;
    let to = to.min(s.doc.pages.len().saturating_sub(1));
    pin_active(&mut s.doc);
    if i != to {
        let page = s.doc.pages.remove(i);
        s.doc.pages.insert(to, page);
    }
    let id = s.doc.pages.get(to).map(|p| p.id.clone());
    Ok(json!({"page": id, "index": to}))
}

fn set_active(s: &mut Session, v: &Value) -> CmdResult {
    let i = required_target(s, v)?;
    if s.doc.active_index() != i || s.doc.active_page.is_empty() {
        s.doc.set_active_index(i);
        s.sel.clear();
        // Showing another page is not an edit: no undo step, and the edited flag stays as it was.
        let dirty = s.dirty;
        s.mark_saved();
        s.dirty = dirty;
    }
    let id = s.doc.pages.get(i).map(|p| p.id.clone());
    Ok(json!({"page": id, "index": i}))
}

/// A `units` name: the short names the file uses, or the long ones.
pub fn parse_units(t: &str) -> Option<Units> {
    let t = t.trim().to_ascii_lowercase();
    let long = match t.as_str() {
        "inches" | "inch" => Some(Units::Inches),
        "feet" | "foot" => Some(Units::Feet),
        "millimeters" | "millimetres" => Some(Units::Millimeters),
        "centimeters" | "centimetres" => Some(Units::Centimeters),
        "meters" | "metres" => Some(Units::Meters),
        "points" => Some(Units::Points),
        "pixels" => Some(Units::Pixels),
        _ => None,
    };
    long.or_else(|| UNITS.into_iter().find(|u| units_name(*u) == t))
}

/// The short name of `u` as files write it (`in`, `mm`).
pub fn units_name(u: Units) -> String {
    serde_json::to_value(u).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn units_param(v: &Value, key: &str) -> Result<Option<Units>, EngineError> {
    match opt_str(v, key)? {
        None => Ok(None),
        Some(t) => parse_units(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{key}` must be in, ft, mm, cm, m, pt or px"))),
    }
}

fn scale_param(v: &Value) -> Result<Option<DrawingScale>, EngineError> {
    let Some(x) = v.get("drawingScale").filter(|x| !x.is_null()) else { return Ok(None) };
    if !x.is_object() {
        return Err(EngineError::Params("`drawingScale` must be {page, drawing, units?}".into()));
    }
    let side = |k: &str| -> Result<f64, EngineError> {
        let n = num(x, k)?.ok_or_else(|| EngineError::Params(format!("`drawingScale.{k}` (number) is required")))?;
        if n <= 0.0 {
            return Err(EngineError::Params(format!("`drawingScale.{k}` must be greater than 0")));
        }
        Ok(n.clamp(DrawingScale::MIN, DrawingScale::MAX))
    };
    let mut scale = DrawingScale { page: side("page")?, drawing: side("drawing")?, units: Units::Inches };
    if let Some(u) = units_param(x, "units")? {
        scale.units = u;
    }
    Ok(Some(scale))
}

fn size_param(v: &Value, key: &str) -> Result<Option<f64>, EngineError> {
    match num(v, key)? {
        None => Ok(None),
        Some(n) if (1.0..=vizcraft_diagram::MAX_EXTENT).contains(&n) => Ok(Some(n)),
        Some(_) => Err(EngineError::Params(format!("`{key}` must be from 1 to {}", vizcraft_diagram::MAX_EXTENT))),
    }
}

/// A page's frame (see `Page::frame`) as `{x, y, w, h}`.
pub fn frame_json(p: &Page) -> Value {
    let f = p.frame();
    json!({"x": f.x0, "y": f.y0, "w": f.width(), "h": f.height()})
}

/// A page's setup as `page.setup` reports it.
pub fn setup_json(p: &Page) -> Value {
    json!({
        "page": p.id,
        "board": p.board,
        "width": p.width,
        "height": p.height,
        "orientation": p.orientation,
        "units": p.units,
        "drawingScale": p.drawing_scale,
        "autoSize": p.auto_size,
    })
}

fn setup(s: &mut Session, v: &Value) -> CmdResult {
    let i = target(s, v)?;
    let mut width = size_param(v, "width")?;
    let mut height = size_param(v, "height")?;
    if let Some(name) = opt_str(v, "size")? {
        // A paper size: its sides in world units, landscape unless an orientation or the page
        // says otherwise (the orientation step below swaps them).
        let size = vizcraft_diagram::sheet::SheetSize::parse(name)
            .ok_or_else(|| EngineError::Params(format!("`size`: `{}` is not a paper size (A0 to A4, ANSI A to E)", super::short(name))))?;
        if width.is_some() || height.is_some() {
            return Err(EngineError::Params("give `size` or `width` and `height`, not both".into()));
        }
        let (long, short) = size.paper().world();
        let portrait = s.doc.pages.get(i).is_some_and(|p| p.height > p.width);
        let (w, h) = if portrait { (short, long) } else { (long, short) };
        width = Some(w);
        height = Some(h);
    }
    let orientation = match opt_str(v, "orientation")?.map(|t| t.trim().to_ascii_lowercase()) {
        None => None,
        Some(t) if t == "portrait" => Some(Orientation::Portrait),
        Some(t) if t == "landscape" => Some(Orientation::Landscape),
        Some(_) => return Err(EngineError::Params("`orientation` must be portrait or landscape".into())),
    };
    let units = units_param(v, "units")?;
    let scale = scale_param(v)?;
    let auto_size = opt_bool(v, "autoSize")?;
    let board = match opt_str(v, "board")?.map(|t| t.trim().to_ascii_lowercase()) {
        None => None,
        Some(t) => Some(Board::from_name(&t).ok_or_else(|| EngineError::Params("`board` must be infinite or fixed".into()))?),
    };
    let nothing =
        board.is_none() && width.is_none() && height.is_none() && orientation.is_none() && units.is_none() && scale.is_none() && auto_size.is_none();
    if nothing && page_ref(v)?.is_none() {
        // The ribbon's launcher: the UI opens the Page Setup dialog, which runs this command
        // again with the settings. Other callers just get the setup back.
        s.ui_requests.push(json!({"open": "pageSetup"}));
    }
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(w) = width {
        page.width = w;
    }
    if let Some(h) = height {
        page.height = h;
    }
    if width.is_some() || height.is_some() {
        // A size typed in sets the orientation it implies; a square page keeps the one it had.
        if page.width > page.height {
            page.orientation = Orientation::Landscape;
        } else if page.height > page.width {
            page.orientation = Orientation::Portrait;
        }
    }
    if let Some(o) = orientation {
        let wide = page.width > page.height;
        let tall = page.height > page.width;
        if (o == Orientation::Portrait && wide) || (o == Orientation::Landscape && tall) {
            std::mem::swap(&mut page.width, &mut page.height);
        }
        page.orientation = o;
    }
    if let Some(u) = units {
        page.units = u;
    }
    if let Some(sc) = scale {
        page.drawing_scale = sc;
    }
    if let Some(b) = board {
        page.board = b;
    }
    if let Some(a) = auto_size {
        page.auto_size = a;
    }
    if page.auto_size && page.board == Board::Fixed && (auto_size == Some(true) || board.is_some()) {
        grow_to_fit(page);
    }
    Ok(setup_json(page))
}

fn set_background(s: &mut Session, v: &Value) -> CmdResult {
    super::protect::backgrounds(s)?;
    let i = target(s, v)?;
    if v.get("background").is_none() {
        return Err(EngineError::Params("`background` (a background page, or null) is required".into()));
    }
    let bg = match ref_param(v, "background")? {
        None => None,
        Some(r) => Some(resolve(&s.doc, &r)?),
    };
    let page_id = s.doc.pages.get(i).map(|p| p.id.clone()).unwrap_or_default();
    let bg_id = match bg {
        None => None,
        Some(b) => {
            let Some(bp) = s.doc.pages.get(b) else { return Err(EngineError::Failed("the document has no page".into())) };
            if !bp.is_background {
                return Err(EngineError::Params(format!("`{}` is not a background page", short(&bp.name))));
            }
            if b == i {
                return Err(EngineError::Params("a page cannot be its own background".into()));
            }
            // Walk the candidate's own chain: reaching this page would make a loop.
            let mut at = bp.background.clone();
            let mut steps = 0;
            while let Some(id) = at {
                if id == page_id {
                    return Err(EngineError::Params("that background would make a loop of background pages".into()));
                }
                steps += 1;
                if steps > MAX_PAGES {
                    break;
                }
                at = s.doc.pages.iter().find(|p| p.id == id).and_then(|p| p.background.clone());
            }
            Some(bp.id.clone())
        }
    };
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    page.background = bg_id.clone();
    Ok(json!({"page": page_id, "background": bg_id}))
}

/// Bounds of every shape and connector on `page`, or `None` when it has none.
pub fn drawing_bounds(page: &Page) -> Option<Rect> {
    vizcraft_diagram::board::drawing_bounds(page)
}

/// Grow an auto-size page to the right and down so its drawing fits with [`AUTO_SIZE_STEP`] to
/// spare, in whole steps. Never shrinks it. Returns whether the size changed.
pub fn grow_to_fit(page: &mut Page) -> bool {
    let Some(b) = drawing_bounds(page) else { return false };
    let grow = |extent: f64, edge: f64| {
        if edge <= extent {
            return extent;
        }
        let want = ((edge + AUTO_SIZE_STEP) / AUTO_SIZE_STEP).ceil() * AUTO_SIZE_STEP;
        want.min(vizcraft_diagram::MAX_EXTENT).max(extent)
    };
    let (w, h) = (grow(page.width, b.x1), grow(page.height, b.y1));
    let changed = w != page.width || h != page.height;
    page.width = w;
    page.height = h;
    changed
}

/// Grow every auto-size page whose drawing reaches past its edge (see [`grow_to_fit`]). The
/// session runs it after every change, in the same undo step, so a shape dropped or moved past
/// the edge takes the page with it.
pub fn auto_size(doc: &mut Document) {
    // An infinite board has no edge to grow, so the flag waits until the page is Fixed again.
    for page in doc.pages.iter_mut().filter(|p| p.auto_size && p.board == Board::Fixed) {
        grow_to_fit(page);
    }
}

fn fit_to_drawing(s: &mut Session, v: &Value) -> CmdResult {
    let i = target(s, v)?;
    let margin = match num(v, "margin")? {
        None => FIT_MARGIN,
        Some(m) if (0.0..=MAX_FIT_MARGIN).contains(&m) => m,
        Some(_) => return Err(EngineError::Params(format!("`margin` must be from 0 to {MAX_FIT_MARGIN}"))),
    };
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let b = drawing_bounds(page).ok_or_else(|| EngineError::Params("the page has nothing drawn on it".into()))?;
    let (dx, dy) = (margin - b.x0, margin - b.y0);
    for sh in &mut page.shapes {
        sh.x += dx;
        sh.y += dy;
    }
    for c in &mut page.connectors {
        for end in [&mut c.start, &mut c.end] {
            if let End::Free { x, y } = end {
                *x += dx;
                *y += dy;
            }
        }
        for p in &mut c.waypoints {
            p.x += dx;
            p.y += dy;
        }
    }
    for g in &mut page.guides {
        g.position += match g.orientation {
            vizcraft_diagram::GuideOrientation::Horizontal => dy,
            vizcraft_diagram::GuideOrientation::Vertical => dx,
        };
    }
    page.width = (b.width() + 2.0 * margin).clamp(1.0, vizcraft_diagram::MAX_EXTENT);
    page.height = (b.height() + 2.0 * margin).clamp(1.0, vizcraft_diagram::MAX_EXTENT);
    page.sanitize();
    Ok(json!({"page": page.id, "width": page.width, "height": page.height, "dx": dx, "dy": dy}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    const DOCS_EXAMPLE: &str = include_str!("../../../formats/testdata/drawio-docs-example.drawio");

    fn temp_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-page-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_and_get_the_open_document() {
        let mut s = Session::new(crate::sample::sample_document());
        let r = s.run("page.list", &json!({})).unwrap();
        let first = &r["pages"][0];
        assert_eq!(first["index"], 0);
        assert_eq!(first["shapes"], s.doc.page().unwrap().shapes.len());
        let xml = s.run("page.get", &json!({})).unwrap();
        assert_eq!(xml["format"], "drawio");
        assert!(xml["content"].as_str().unwrap().starts_with("<mxGraphModel"));
        let page_id = s.doc.page().unwrap().id.clone();
        let js = s.run("page.get", &json!({"page": page_id, "format": "vizcraft"})).unwrap();
        let back: vizcraft_diagram::Page = serde_json::from_str(js["content"].as_str().unwrap()).unwrap();
        assert_eq!(&back, s.doc.page().unwrap());
        assert!(s.run("page.get", &json!({"page": "nope"})).is_err());
        assert!(s.run("page.get", &json!({"page": 3})).is_err());
        assert!(s.run("page.get", &json!({"format": "png"})).is_err());
        assert!(!s.can_undo(), "reading changes nothing");
    }

    #[test]
    fn set_without_a_path_replaces_a_page_in_one_undo_step() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let r = s.run("page.set", &json!({"page": 0, "xml": DOCS_EXAMPLE, "routing": "orthogonal"})).unwrap();
        assert_eq!(r["ids"].as_array().unwrap().len(), 3);
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.len(), 2);
        assert!(page.connectors.iter().all(|c| c.avoid));
        assert!(s.run("page.set", &json!({"xml": DOCS_EXAMPLE})).is_err());
        assert!(s.run("page.set", &json!({"page": 0})).is_err());
        assert!(s.run("page.set", &json!({"page": 0, "xml": "<mxGraphModel><root>"})).is_err());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
    }

    #[test]
    fn path_commands_edit_the_file_and_leave_the_session_alone() {
        let dir = temp_dir("path");
        for ext in ["drawio", "vizcraft"] {
            let path = dir.join(format!("doc.{ext}"));
            let p = path.to_string_lossy().to_string();
            let mut s = Session::new(crate::sample::sample_document());
            s.run("file.save", &json!({"path": p})).unwrap();
            let mut t = Session::new(Document::new());
            let before = t.doc.clone();
            let r = t.run("page.set", &json!({"path": p, "page": "Added", "xml": DOCS_EXAMPLE, "layout": "layered"})).unwrap();
            assert_eq!(r["ids"].as_array().unwrap().len(), 3, "{ext}");
            assert_eq!(t.doc, before, "the open document is untouched");
            assert!(!t.can_undo());
            let list = t.run("page.list", &json!({"path": p})).unwrap();
            let pages = list["pages"].as_array().unwrap();
            assert_eq!(pages.len(), 2, "{ext}");
            assert_eq!(pages[1]["name"], "Added");
            assert_eq!(pages[1]["shapes"], 2);
            let got = t.run("page.get", &json!({"path": p, "page": "Added"})).unwrap();
            assert!(got["content"].as_str().unwrap().contains("Hello,"));
            // Replacing the page again keeps two pages.
            t.run("page.set", &json!({"path": p, "page": "Added", "xml": DOCS_EXAMPLE})).unwrap();
            assert_eq!(t.run("page.list", &json!({"path": p})).unwrap()["pages"].as_array().unwrap().len(), 2);
        }
        let bad = dir.join("notes.txt");
        std::fs::write(&bad, "x").unwrap();
        let mut t = Session::new(Document::new());
        assert!(t.run("page.list", &json!({"path": bad.to_string_lossy()})).is_err());
        assert!(t.run("page.set", &json!({"path": bad.to_string_lossy(), "page": 0, "xml": DOCS_EXAMPLE})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ids(s: &Session) -> Vec<String> {
        s.doc.pages.iter().map(|p| p.id.clone()).collect()
    }

    fn add_shape(s: &mut Session, x: f64, y: f64) -> String {
        let r = s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": x, "y": y})).unwrap();
        r["id"].as_str().or(r["primary"].as_str()).unwrap().to_string()
    }

    #[test]
    fn add_inserts_after_the_active_page_takes_its_setup_and_becomes_active() {
        let mut s = Session::new(Document::new());
        s.run("page.setup", &json!({"width": 500, "height": 400, "units": "mm"})).unwrap();
        let first = s.doc.pages[0].id.clone();
        let r = s.run("page.add", &json!({})).unwrap();
        assert_eq!(r["index"], 1);
        assert_eq!(r["name"], "Page-2");
        let p = s.doc.page().unwrap();
        assert_eq!((p.id.as_str(), p.width, p.height, p.units), (r["page"].as_str().unwrap(), 500.0, 400.0, vizcraft_diagram::Units::Millimeters));
        // Back on the first page, a named page goes right after it.
        s.run("page.setActive", &json!({"page": first})).unwrap();
        let r = s.run("page.add", &json!({"name": "  Middle  "})).unwrap();
        assert_eq!((r["index"].as_u64(), r["name"].as_str()), (Some(1), Some("Middle")));
        // A background page goes last; an index puts a page where asked.
        let r = s.run("page.add", &json!({"background": true})).unwrap();
        assert_eq!(r["index"], 3);
        assert!(s.doc.pages[3].is_background);
        assert_eq!(r["name"], "Background-4");
        let r = s.run("page.add", &json!({"index": 0, "name": "Front"})).unwrap();
        assert_eq!(r["index"], 0);
        for bad in
            [json!({"name": "Middle"}), json!({"name": "   "}), json!({"name": "x".repeat(300)}), json!({"index": -1}), json!({"background": "yes"})]
        {
            assert!(s.run("page.add", &bad).is_err(), "{bad}");
        }
        // Each add is one undo step, and undo puts the active page back too.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.pages.len(), 4);
        assert!(s.doc.page().unwrap().is_background);
    }

    #[test]
    fn delete_refuses_the_last_foreground_page_and_clears_references_to_a_background() {
        let mut s = Session::new(Document::new());
        assert!(s.run("page.delete", &json!({})).is_err(), "the only page stays");
        let bg = s.run("page.add", &json!({"background": true, "name": "Back"})).unwrap()["page"].as_str().unwrap().to_string();
        let first = s.doc.pages[0].id.clone();
        assert!(s.run("page.delete", &json!({"page": first})).is_err(), "the last foreground page stays");
        s.run("page.setBackground", &json!({"page": first, "background": "Back"})).unwrap();
        let fg2 = s.run("page.add", &json!({})).unwrap()["page"].as_str().unwrap().to_string();
        // A new foreground page added from the background page has no background of its own.
        assert_eq!(s.doc.pages.iter().find(|p| p.id == fg2).unwrap().background, None);
        s.run("page.setActive", &json!({"page": first})).unwrap();
        let before = s.doc.clone();
        let r = s.run("page.delete", &json!({"page": bg})).unwrap();
        assert_eq!(r["deleted"], bg);
        assert!(s.doc.pages.iter().all(|p| p.background.is_none()));
        assert_eq!(s.doc.page().unwrap().id, first, "deleting another page keeps the active one");
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
        // Deleting the active page shows a foreground page next to it.
        let r = s.run("page.delete", &json!({})).unwrap();
        assert_eq!(r["deleted"], first);
        assert_eq!(r["active"], fg2);
        assert!(s.run("page.delete", &json!({"page": "nope"})).is_err());
    }

    #[test]
    fn duplicate_copies_the_drawing_with_fresh_ids_and_glue() {
        let mut s = Session::new(crate::sample::sample_document());
        let src = s.doc.page().unwrap().clone();
        let r = s.run("page.duplicate", &json!({})).unwrap();
        assert_eq!(r["index"], 1);
        assert_eq!(r["name"], format!("{} (2)", src.name));
        let copy = s.doc.page().unwrap();
        assert_eq!(copy.id, r["page"].as_str().unwrap());
        assert_eq!((copy.shapes.len(), copy.connectors.len()), (src.shapes.len(), src.connectors.len()));
        assert!(copy.shapes.iter().all(|x| src.shape(&x.id).is_none()), "fresh shape ids");
        for c in &copy.connectors {
            assert!(src.connector(&c.id).is_none());
            for end in [&c.start, &c.end] {
                if let Some(id) = end.shape() {
                    assert!(copy.shape(id).is_some(), "glue points at the copy");
                }
            }
        }
        // Duplicating again names the next copy.
        s.run("page.duplicate", &json!({"page": 0})).unwrap();
        assert_eq!(s.doc.pages[1].name, format!("{} (3)", src.name));
        s.run("edit.undo", &json!({})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.pages.len(), 1);
    }

    #[test]
    fn rename_and_move_check_their_params() {
        let mut s = Session::new(Document::new());
        s.run("page.add", &json!({"name": "Two"})).unwrap();
        s.run("page.add", &json!({"name": "Three"})).unwrap();
        s.run("page.rename", &json!({"page": "Two", "name": "Second"})).unwrap();
        assert_eq!(s.doc.pages[1].name, "Second");
        s.run("page.rename", &json!({"page": "Second", "name": "Second"})).unwrap();
        for bad in [json!({"page": 0, "name": "Three"}), json!({"page": 0, "name": ""}), json!({"page": 0}), json!({"page": 9, "name": "x"})] {
            assert!(s.run("page.rename", &bad).is_err(), "{bad}");
        }
        let active = s.doc.page().unwrap().id.clone();
        let r = s.run("page.move", &json!({"page": "Three", "index": 0})).unwrap();
        assert_eq!(r["index"], 0);
        assert_eq!(s.doc.pages[0].name, "Three");
        assert_eq!(s.doc.page().unwrap().id, active, "the active page follows its page");
        // Past the end means last.
        s.run("page.move", &json!({"page": "Three", "index": 99})).unwrap();
        assert_eq!(s.doc.pages[2].name, "Three");
        for bad in [json!({"page": 0}), json!({"index": 1}), json!({"page": 0, "index": -2}), json!({"page": 0, "index": 1.5})] {
            assert!(s.run("page.move", &bad).is_err(), "{bad}");
        }
        let before = s.doc.clone();
        s.run("page.move", &json!({"page": 0, "index": 1})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
    }

    #[test]
    fn set_active_switches_what_commands_act_on_without_an_undo_step() {
        let mut s = Session::new(crate::sample::sample_document());
        let first = s.doc.pages[0].id.clone();
        let shapes = s.doc.pages[0].shapes.len();
        let second = s.run("page.add", &json!({})).unwrap()["page"].as_str().unwrap().to_string();
        let id = add_shape(&mut s, 50.0, 50.0);
        assert!(s.doc.pages[1].shape(&id).is_some(), "shape.add draws on the active page");
        assert_eq!(s.doc.pages[0].shapes.len(), shapes);
        s.mark_saved();
        let steps = s.undo_label().map(str::to_string);
        let r = s.run("page.setActive", &json!({"page": first})).unwrap();
        assert_eq!(r["index"], 0);
        assert_eq!(s.doc.page().unwrap().id, first);
        assert!(s.sel.is_empty(), "the selection was on the other page");
        assert!(!s.dirty, "showing a page is not an edit");
        assert_eq!(s.undo_label().map(str::to_string), steps, "and not an undo step");
        assert_eq!(s.run("page.list", &json!({})).unwrap()["active"], first);
        s.run("page.setActive", &json!({"page": 1})).unwrap();
        assert_eq!(s.doc.page().unwrap().id, second);
        for bad in [json!({}), json!({"page": "nope"}), json!({"page": 7}), json!({"page": true})] {
            assert!(s.run("page.setActive", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn active_page_round_trips_through_save_and_load() {
        let mut s = Session::new(crate::sample::sample_document());
        s.run("page.add", &json!({"name": "Second"})).unwrap();
        let active = s.doc.page().unwrap().id.clone();
        let bytes = crate::io::save_bytes("doc.vizcraft", &s.doc).unwrap();
        let back = crate::io::load_bytes("doc.vizcraft", &bytes).unwrap();
        assert_eq!(back.page().unwrap().id, active);
        assert_eq!(back.active_index(), 1);
        // An unknown id from a file falls back to the first page.
        let mut doc = s.doc.clone();
        doc.active_page = "missing".into();
        let bytes = crate::io::save_bytes("x.vizcraft", &doc).unwrap();
        let back = crate::io::load_bytes("x.vizcraft", &bytes).unwrap();
        assert_eq!(back.active_index(), 0);
        assert!(back.active_page.is_empty());
    }

    #[test]
    fn setup_sets_size_orientation_units_scale_and_auto_size() {
        let mut s = Session::new(Document::new());
        let r = s.run("page.setup", &json!({"orientation": "portrait"})).unwrap();
        assert_eq!((r["width"].as_f64(), r["height"].as_f64(), r["orientation"].as_str()), (Some(850.0), Some(1100.0), Some("portrait")));
        let r = s.run("page.setup", &json!({"width": 2000, "height": 1000})).unwrap();
        assert_eq!(r["orientation"], "landscape", "a size implies its orientation");
        s.run("page.setup", &json!({"units": "cm", "drawingScale": {"page": 1, "drawing": 100, "units": "m"}, "autoSize": true})).unwrap();
        let p = s.doc.page().unwrap();
        assert_eq!(p.units, vizcraft_diagram::Units::Centimeters);
        assert_eq!((p.drawing_scale.page, p.drawing_scale.drawing, p.drawing_scale.units), (1.0, 100.0, vizcraft_diagram::Units::Meters));
        assert!(p.auto_size);
        assert!(s.ui_requests.is_empty(), "settings never open the dialog");
        for bad in [
            json!({"width": 0}),
            json!({"height": -5}),
            json!({"width": 1e12}),
            json!({"orientation": "sideways"}),
            json!({"units": "furlongs"}),
            json!({"drawingScale": {"page": 0, "drawing": 1}}),
            json!({"drawingScale": 5}),
            json!({"autoSize": "on"}),
            json!({"page": "nope", "width": 100}),
        ] {
            assert!(s.run("page.setup", &bad).is_err(), "{bad}");
        }
        // With no settings the UI is asked to open the dialog; nothing changes.
        let before = s.doc.clone();
        let r = s.run("page.setup", &json!({})).unwrap();
        assert_eq!(r["width"], 2000.0);
        assert_eq!(s.ui_requests, vec![json!({"open": "pageSetup"})]);
        assert_eq!(s.doc, before);
        s.run("edit.undo", &json!({})).unwrap();
        assert!(!s.doc.page().unwrap().auto_size);
    }

    #[test]
    fn set_background_takes_only_background_pages_and_no_loops() {
        let mut s = Session::new(Document::new());
        let fg = s.doc.pages[0].id.clone();
        s.run("page.add", &json!({"name": "Other"})).unwrap();
        let a = s.run("page.add", &json!({"background": true, "name": "A"})).unwrap()["page"].as_str().unwrap().to_string();
        let b = s.run("page.add", &json!({"background": true, "name": "B"})).unwrap()["page"].as_str().unwrap().to_string();
        let r = s.run("page.setBackground", &json!({"page": fg, "background": "A"})).unwrap();
        assert_eq!(r["background"], a);
        s.run("page.setBackground", &json!({"page": "A", "background": "B"})).unwrap();
        assert!(s.run("page.setBackground", &json!({"page": "B", "background": "A"})).is_err(), "a loop");
        assert!(s.run("page.setBackground", &json!({"page": "B", "background": "B"})).is_err(), "itself");
        assert!(s.run("page.setBackground", &json!({"page": fg, "background": "Other"})).is_err(), "not a background page");
        assert!(s.run("page.setBackground", &json!({"page": fg})).is_err(), "background is required");
        assert!(s.run("page.setBackground", &json!({"page": fg, "background": "nope"})).is_err());
        let chain: Vec<String> = s.doc.background_chain(0).iter().map(|p| p.id.clone()).collect();
        assert_eq!(chain, vec![b.clone(), a.clone()]);
        s.run("page.setBackground", &json!({"page": fg, "background": null})).unwrap();
        assert_eq!(s.doc.pages[0].background, None);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.pages[0].background.as_deref(), Some(a.as_str()));
    }

    #[test]
    fn export_draws_the_background_page_under_the_foreground() {
        let mut s = Session::new(Document::new());
        let fg = s.doc.pages[0].id.clone();
        let front = add_shape(&mut s, 100.0, 100.0);
        s.run("page.add", &json!({"background": true, "name": "Back"})).unwrap();
        let back = add_shape(&mut s, 300.0, 300.0);
        s.run("page.setBackground", &json!({"page": fg, "background": "Back"})).unwrap();
        s.run("page.setActive", &json!({"page": fg})).unwrap();
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, s.doc.active_index()).unwrap()).unwrap();
        let bg_at = svg.find("class=\"background\"").unwrap();
        let back_at = svg.find(&format!("id=\"{back}\"")).unwrap();
        let front_at = svg.find(&format!("id=\"{front}\"")).unwrap();
        assert!(bg_at < back_at && back_at < front_at, "background first, then the page's own shapes");
    }

    #[test]
    fn fit_to_drawing_moves_the_drawing_to_the_margin_and_sizes_the_page() {
        let mut s = Session::new(Document::new());
        assert!(s.run("page.fitToDrawing", &json!({})).is_err(), "nothing drawn");
        let a = add_shape(&mut s, 300.0, 200.0);
        let b = add_shape(&mut s, 600.0, 500.0);
        s.run("connector.add", &json!({"from": {"shape": a}, "to": {"shape": b}})).unwrap();
        let before = super::drawing_bounds(s.doc.page().unwrap()).unwrap();
        let r = s.run("page.fitToDrawing", &json!({"margin": 10})).unwrap();
        let p = s.doc.page().unwrap();
        let after = super::drawing_bounds(p).unwrap();
        assert!((after.x0 - 10.0).abs() < 1e-6 && (after.y0 - 10.0).abs() < 1e-6, "{after:?}");
        assert!((p.width - (before.width() + 20.0)).abs() < 1e-6);
        assert!((p.height - (before.height() + 20.0)).abs() < 1e-6);
        assert_eq!(r["dx"].as_f64(), Some(10.0 - before.x0));
        assert!(s.run("page.fitToDrawing", &json!({"margin": -1})).is_err());
        assert!(s.run("page.fitToDrawing", &json!({"margin": 1e9})).is_err());
    }

    #[test]
    fn auto_size_grows_the_page_when_a_shape_goes_past_the_edge() {
        let mut s = Session::new(Document::new());
        s.run("page.setup", &json!({"board": "fixed"})).unwrap();
        add_shape(&mut s, 1200.0, 100.0);
        assert_eq!(s.doc.page().unwrap().width, 1100.0, "without auto size the page stays");
        s.run("page.setup", &json!({"autoSize": true})).unwrap();
        let p = s.doc.page().unwrap();
        let edge = super::drawing_bounds(p).unwrap().x1;
        assert!(p.width >= edge + super::AUTO_SIZE_STEP && p.width % super::AUTO_SIZE_STEP == 0.0, "turning it on fits the drawing: {}", p.width);
        let id = add_shape(&mut s, 200.0, 1000.0);
        let p = s.doc.page().unwrap();
        assert!(p.height > 1000.0, "a shape added past the bottom grows the page: {}", p.height);
        let h = p.height;
        s.run("shape.move", &json!({"ids": [id], "dx": 0, "dy": 500})).unwrap();
        assert!(s.doc.page().unwrap().height > h, "moving it further grows it again");
        // Growth is part of the step that caused it.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().height, h);
        // It never shrinks.
        s.run("edit.delete", &json!({"ids": [id]})).unwrap();
        assert_eq!(s.doc.page().unwrap().height, h);
        // An infinite board ignores the flag: nothing grows.
        s.run("page.setup", &json!({"board": "infinite"})).unwrap();
        add_shape(&mut s, 5000.0, 5000.0);
        assert_eq!(s.doc.page().unwrap().height, h);
    }

    #[test]
    fn board_round_trips_through_setup_and_list_reports_the_frame() {
        let mut s = Session::new(Document::new());
        let r = s.run("page.setup", &json!({})).unwrap();
        assert_eq!(r["board"], "infinite", "a new document is infinite");
        s.ui_requests.clear();
        let r = s.run("page.list", &json!({})).unwrap();
        assert_eq!(r["pages"][0]["board"], "infinite");
        assert_eq!(r["pages"][0]["frame"], json!({"x": 0.0, "y": 0.0, "w": 1100.0, "h": 850.0}), "an empty infinite page frames a Letter sheet");
        add_shape(&mut s, 400.0, 300.0);
        let f = s.run("page.list", &json!({})).unwrap()["pages"][0]["frame"].clone();
        assert!(f["w"].as_f64().unwrap() < 1100.0 && f["x"].as_f64().unwrap() < 400.0, "{f}");
        let r = s.run("page.setup", &json!({"board": "fixed", "width": 500, "height": 400})).unwrap();
        assert_eq!((r["board"].as_str(), s.doc.page().unwrap().board), (Some("fixed"), vizcraft_diagram::Board::Fixed));
        let f = s.run("page.list", &json!({})).unwrap()["pages"][0]["frame"].clone();
        assert_eq!(f, json!({"x": 0.0, "y": 0.0, "w": 500.0, "h": 400.0}));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().board, vizcraft_diagram::Board::Infinite, "undo restores the board");
        let r = s.run("page.setup", &json!({"board": "infinite"})).unwrap();
        assert_eq!(r["board"], "infinite");
        assert!(s.run("page.setup", &json!({"board": "endless"})).is_err());
        assert!(s.run("page.setup", &json!({"board": 3})).is_err());
        // A page added later is infinite, whatever the active page is.
        s.run("page.setup", &json!({"board": "fixed"})).unwrap();
        s.run("page.add", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().board, vizcraft_diagram::Board::Infinite);
    }

    #[test]
    fn page_commands_keep_page_ids_unique() {
        let mut s = Session::new(crate::sample::sample_document());
        s.run("page.add", &json!({})).unwrap();
        s.run("page.duplicate", &json!({"page": 0})).unwrap();
        s.run("page.add", &json!({"background": true})).unwrap();
        let mut all = ids(&s);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 4);
    }
}
