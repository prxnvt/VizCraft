//! File menu: new, open, save, save as, export, close, user name.
//!
//! Without a `path`, `file.open`, `file.saveAs`, `file.export` and `file.import` ask the UI for
//! a picker through `ui_requests`; with one they work headless (CLI, control channel, tests).
//! Formats are chosen by `crate::io` from the file name.
//!
//! `file.import` and `page.set` share [`import_opts`], [`place`] and [`import_into_session`]:
//! the imported pages are adopted into the document (ids kept when plain and free), optionally
//! laid out and routed, and the result names the target page, the new ids and the warnings.
//! With `merge`, `file.import` and `file.reimport` follow the re-import contract instead
//! ([`merge`]): the imported items become model elements matched to the ones already there.

use std::borrow::Cow;

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::{End, Page, Point, Rect, Route};
use vizcraft_formats::Imported;

use super::{opt_bool, opt_str, prune_selection, sel_result, select, short};
use crate::doc::Document;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

mod merge;

/// Params doc shared by `file.import` and the MCP tools that call it.
const IMPORT_PARAMS: &str = r#"{"path"?: string, "text"?: string, "format"?: "drawio"|"mermaid"|"csv", "page"?: id|name|index, "replace"?: bool, "layout"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL", "routing"?: "orthogonal", "merge"?: bool, "source"?: string, "type"?: element type} -> {page, ids, warnings} (format from the extension or the text; a page name that matches nothing makes a new page); with merge: the shapes become model elements matched to the model by the last import of the same source (default: the path), id, tag, then name within the type -> {page, added, updated, removed, kept, ids: {source id: element id}, issues, warnings}, removed elements flagged with an import.removed issue, one undo step"#;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("file.new", "New", "File", new).key("Mod+N").params(r#"{"template"?: "blank|sample"}"#),
        CommandSpec::new("file.open", "Open", "File", open).key("Mod+O").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.save", "Save", "File", save).key("Mod+S").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.saveAs", "Save As", "File", save_as)
            .key("Mod+Shift+S")
            .params(r#"{"path"?: string} (a path ending in .pdf exports every foreground page as PDF instead of saving)"#),
        CommandSpec::new("file.close", "Close", "File", |s, _| {
            s.ui_requests.push(json!({"close": true}));
            sel_result(s)
        })
        .key("Mod+W"),
        CommandSpec::new("file.export", "Export...", "File", export).key("Mod+Shift+E").params(
            r#"{"path"?: string, "format"?: "svg"|"png"|"pdf"|"drawio", "page"?: id|name|index, "pages"?: "active"|"all"|[id|name|index], "scale"?: number, "print"?: bool} -> {exported, path, format, page, pages, paths} (svg: one page; png: the active page unless `page` or `pages` names others, several pages write name-1.png, name-2.png and so on, listed in `paths`; pdf: one vector page per document page, every foreground page by default, fonts embedded; png: `scale` pixels per unit, 0.25 to 4, default 1; drawio: every page; `print` leaves out non-printing shapes and layers, otherwise the export matches the screen; format from the extension when omitted)"#,
        ),
        CommandSpec::new("file.import", "Import...", "File", import).key("Mod+Shift+I").params(IMPORT_PARAMS),
        CommandSpec::new("file.reimport", "Re-import", "File", merge::reimport)
            .params(r#"{"source"?: string, "page"?: page, "layout"?, "direction"?, "routing"?, "type"?: element type} -> the merge result of file.import; reads the recorded source file again (default: the newest import with a file) and merges it"#)
            .when(|s| if s.doc.imports.iter().any(|r| !r.source.is_empty()) { None } else { Some("nothing has been imported from a file with merge") }),
        CommandSpec::new("file.setAuthor", "User Name", "File > Options", |s, v| {
            s.author = p::req_str(v, "name")?.trim().to_string();
            Ok(json!({"author": s.author}))
        })
        .params(r#"{"name": string}"#),
    ]
}

fn new(s: &mut Session, v: &Value) -> CmdResult {
    let doc = match p::str(v, "template").unwrap_or("blank") {
        "sample" => crate::sample::sample_document(),
        _ => Document::new(),
    };
    s.set_document(doc);
    s.path = None;
    sel_result(s)
}

fn open(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "openFile"}));
        return sel_result(s);
    };
    let bytes = std::fs::read(path).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    let doc = crate::io::load_bytes(path, &bytes)?;
    s.set_document(doc);
    s.path = Some(path.into());
    Ok(json!({"path": path}))
}

fn save(s: &mut Session, v: &Value) -> CmdResult {
    let path = match p::str(v, "path").map(std::path::PathBuf::from).or_else(|| s.path.clone()) {
        Some(p) => p,
        None => {
            s.ui_requests.push(json!({"open": "saveAs"}));
            return Ok(json!({"saved": false}));
        }
    };
    // Metadata is stamped before the bytes are made, so the file records this save.
    s.doc.core.modified = super::now_iso();
    if s.doc.core.created.is_empty() {
        s.doc.core.created = s.doc.core.modified.clone();
    }
    if s.doc.core.author.is_empty() {
        s.doc.core.author = s.author.clone();
    }
    s.doc.core.last_modified_by = s.author.clone();
    s.doc.core.revision = s.doc.core.revision.saturating_add(1);
    let name = path.to_string_lossy().to_string();
    let bytes = crate::io::save_bytes(&name, &s.doc)?;
    std::fs::write(&path, bytes).map_err(|e| EngineError::Failed(format!("{name}: {e}")))?;
    s.path = Some(path);
    s.mark_saved();
    s.status = format!("Saved {name}");
    Ok(json!({"saved": true, "path": name}))
}

fn save_as(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "saveAs"}));
        return Ok(json!({"saved": false}));
    };
    // Save As offers PDF: the document is exported, every foreground page, and stays where it was
    // saved (a PDF cannot be opened again as a document).
    if path.to_ascii_lowercase().ends_with(".pdf") {
        let r = export(s, &json!({"path": path, "format": "pdf"}))?;
        return Ok(json!({"saved": false, "exported": r.get("exported"), "path": path, "format": "pdf"}));
    }
    save(s, v)
}

fn export(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "export"}));
        return Ok(json!({"exported": false}));
    };
    let format = crate::io::export_format(path, p::str(v, "format"))?;
    // Read every param before writing anything, so a bad page or scale leaves no file behind.
    // A PDF holds every foreground page unless told otherwise; SVG and PNG the active page.
    let pages = super::print::resolve_pages(s, v, if format == "pdf" { "all" } else { "active" })?;
    let scale = super::num(v, "scale")?.unwrap_or(1.0);
    if scale <= 0.0 {
        return Err(EngineError::Params("`scale` must be greater than 0".into()));
    }
    let opts = vizcraft_formats::svg::SvgOptions { print: opt_bool(v, "print")?.unwrap_or(false), date: super::print::today() };
    let fail = |e: vizcraft_formats::FormatError| EngineError::Failed(format!("{path}: {e}"));
    let first = pages.first().copied().unwrap_or(0);
    // (file name, bytes) for every file to write, all made before any is written.
    let files: Vec<(String, Vec<u8>)> = match format {
        "pdf" => vec![(path.to_string(), vizcraft_formats::pdf::export_pages(&s.doc, &pages, &opts).map_err(fail)?)],
        "png" if pages.len() > 1 => {
            let (stem, ext) = numbered_stem(path);
            let mut out = Vec::with_capacity(pages.len());
            for (n, &i) in pages.iter().enumerate() {
                let (png, _, _) = vizcraft_formats::png::render_page_with(&s.doc, i, scale, &opts).map_err(fail)?;
                out.push((format!("{stem}-{}{ext}", n + 1), png));
            }
            out
        }
        "png" => vec![(path.to_string(), vizcraft_formats::png::render_page_with(&s.doc, first, scale, &opts).map_err(fail)?.0)],
        "svg" if pages.len() > 1 => return Err(EngineError::Params("an SVG file holds one page; export PNG or PDF for several".into())),
        "svg" => vec![(path.to_string(), vizcraft_formats::svg::export_page_with(&s.doc, first, &opts).map_err(fail)?)],
        _ => vec![(path.to_string(), crate::io::export_page_bytes(path, Some(format), &s.doc, first, scale)?)],
    };
    for (name, bytes) in &files {
        std::fs::write(name, bytes).map_err(|e| EngineError::Failed(format!("{name}: {e}")))?;
    }
    s.status = format!("Exported {path}");
    let ids: Vec<String> = pages.iter().filter_map(|&i| s.doc.pages.get(i).map(|p| p.id.clone())).collect();
    let paths: Vec<&String> = files.iter().map(|(n, _)| n).collect();
    Ok(json!({"exported": true, "path": path, "format": format, "page": ids.first(), "pages": ids, "paths": paths}))
}

/// `path` split for numbered files: `out/plan.png` gives (`out/plan`, `.png`); a name without an
/// extension keeps an empty one.
fn numbered_stem(path: &str) -> (&str, &str) {
    let file_start = path.rfind(['/', '\\']).map_or(0, |i| i + 1);
    match path.rfind('.').filter(|&i| i > file_start) {
        Some(i) => (path.get(..i).unwrap_or(path), path.get(i..).unwrap_or("")),
        None => (path, ""),
    }
}

fn import(s: &mut Session, v: &Value) -> CmdResult {
    let path = opt_str(v, "path")?;
    let text = opt_str(v, "text")?;
    if path.is_none() && text.is_none() {
        s.ui_requests.push(json!({"open": "import"}));
        return Ok(json!({"imported": false}));
    }
    let opts = import_opts(v)?;
    let format = opt_str(v, "format")?;
    // Text wins over a path: it is what the caller has in hand.
    let (bytes, name): (Cow<'_, [u8]>, Option<&str>) = match text {
        Some(t) => {
            if t.len() as u64 > crate::io::MAX_IMPORT_BYTES {
                return Err(EngineError::Params(format!("`text` is longer than {} MiB", crate::io::MAX_IMPORT_BYTES / (1024 * 1024))));
            }
            (Cow::Borrowed(t.as_bytes()), None)
        }
        None => {
            let path = path.unwrap_or_default();
            (Cow::Owned(crate::io::read_limited(path)?), Some(path))
        }
    };
    let format = crate::io::import_format(name, format, &bytes)?;
    let source = merge::source_param(v)?;
    if opt_bool(v, "merge")?.unwrap_or(false) {
        let fallback = merge::fallback_type(v)?;
        let imported = crate::io::import_bytes(format, &bytes)?;
        let source = source.or_else(|| name.map(str::to_string)).unwrap_or_default();
        return merge::merge(s, imported, &opts, &source, format, &fallback, v.get("direction").is_some());
    }
    let imported = crate::io::import_bytes(format, &bytes)?;
    import_into_session(s, imported, &opts, name)
}

/// How imported content is placed: the `page`, `replace`, `layout`, `direction` and `routing`
/// params of `file.import` and `page.set`.
pub(crate) struct ImportOpts {
    /// The target page: an id, name or index; `None` is the first page.
    pub page: Option<String>,
    /// Clear the target page first.
    pub replace: bool,
    /// Lay the imported shapes out with this algorithm.
    pub layout: Option<Algorithm>,
    /// Flow direction for the layout.
    pub direction: Direction,
    /// Route the imported connectors around shapes.
    pub avoid: bool,
}

/// Longest page reference accepted (it may become a new page's name).
const MAX_PAGE_REF: usize = 1024;

/// Gap between existing content and content imported beside it.
const IMPORT_GAP: f64 = 60.0;

/// Read the placement params; junk is a params error.
pub(crate) fn import_opts(v: &Value) -> Result<ImportOpts, EngineError> {
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.clone()),
        // An index may come as a number.
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(_) => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
    };
    if page.as_ref().is_some_and(|p| p.len() > MAX_PAGE_REF || p.is_empty()) {
        return Err(EngineError::Params(format!("`page` must be 1 to {MAX_PAGE_REF} bytes")));
    }
    let layout = match opt_str(v, "layout")?.map(|t| t.trim().to_ascii_lowercase()) {
        None => None,
        Some(t) => Some(match t.as_str() {
            "layered" | "hierarchical" => Algorithm::Layered,
            "tree" => Algorithm::Tree,
            "radial" => Algorithm::Radial,
            "force" | "organic" => Algorithm::Force,
            other => return Err(EngineError::Params(format!("`layout` must be layered, tree, radial or force, not `{}`", short(other)))),
        }),
    };
    let direction = match opt_str(v, "direction")? {
        None => Direction::TopBottom,
        Some(t) => parse_direction(t)?,
    };
    let avoid = match opt_str(v, "routing")?.map(|t| t.trim().to_ascii_lowercase()) {
        None => false,
        Some(t) if t == "orthogonal" => true,
        Some(t) if t == "none" || t.is_empty() => false,
        Some(t) => return Err(EngineError::Params(format!("`routing` must be \"orthogonal\", not `{}`", short(&t)))),
    };
    Ok(ImportOpts { page, replace: opt_bool(v, "replace")?.unwrap_or(false), layout, direction, avoid })
}

/// `TB` (or `TD`), `BT`, `LR` or `RL`, in any case.
pub(crate) fn parse_direction(t: &str) -> Result<Direction, EngineError> {
    match t.trim().to_ascii_uppercase().as_str() {
        "TB" | "TD" => Ok(Direction::TopBottom),
        "BT" => Ok(Direction::BottomTop),
        "LR" => Ok(Direction::LeftRight),
        "RL" => Ok(Direction::RightLeft),
        _ => Err(EngineError::Params(format!("`direction` must be TB, BT, LR or RL, not `{}`", short(t)))),
    }
}

/// The bounding box of a page's shapes, `None` when it has none.
fn shapes_bounds(page: &Page) -> Option<Rect> {
    page.shapes.iter().map(|s| s.bounds()).reduce(|a, b| a.union(&b))
}

/// Move everything on `page` by `(dx, dy)`: shapes, bends and free connector ends.
fn shift(page: &mut Page, dx: f64, dy: f64) {
    for s in &mut page.shapes {
        s.x += dx;
        s.y += dy;
    }
    for c in &mut page.connectors {
        for p in &mut c.waypoints {
            *p = Point::new(p.x + dx, p.y + dy);
        }
        for end in [&mut c.start, &mut c.end] {
            if let End::Free { x, y } = end {
                *x += dx;
                *y += dy;
            }
        }
    }
}

/// What [`place`] did: the target page index, every new id, and the warnings.
pub(crate) struct Placed {
    /// Index of the page the first imported page went to.
    pub index: usize,
    /// Ids of the new shapes and connectors, on every page, in import order.
    pub ids: Vec<String>,
    /// The importer's warnings and the layout's notes.
    pub warnings: Vec<String>,
}

/// Put imported pages into `doc`: the first on the target page (cleared with `replace`), any
/// others on new pages; then lay out and route as `opts` asks. Nothing is changed when there is
/// nothing to import.
///
/// Adding to a page that already has shapes never moves those shapes. With a `layout`, the new
/// shapes are placed around them by `layout_incremental`. Without one, the content keeps the
/// arrangement it came with (the source's, or the Mermaid and CSV importers' own layout) and is
/// set beside what is there, which is where incremental placement puts unconnected content.
pub(crate) fn place(doc: &mut Document, imported: Imported, opts: &ImportOpts) -> Result<Placed, EngineError> {
    let Imported { pages, mut warnings } = imported;
    let mut pages = pages.into_iter();
    let Some(mut first) = pages.next() else { return Err(EngineError::Failed("the source holds no page to import".into())) };
    let index = match &opts.page {
        None => doc.active_index(),
        Some(r) => match doc.find_page(r) {
            Some(i) => i,
            None => doc.add_page(r),
        },
    };
    let Some(target) = doc.pages.get_mut(index) else { return Err(EngineError::Failed("the document has no page".into())) };
    let incremental = !opts.replace && opts.layout.is_some() && !target.shapes.is_empty();
    if opts.replace {
        // A draw.io page brings its own size; other sources leave the page as it was.
        if (first.width, first.height) != (Page::DEFAULT_WIDTH, Page::DEFAULT_HEIGHT) {
            target.width = first.width;
            target.height = first.height;
            target.board = first.board;
        }
    } else if !incremental && let (Some(have), Some(new)) = (shapes_bounds(target), shapes_bounds(&first)) {
        shift(&mut first, have.x1 + IMPORT_GAP - new.x0, have.y0 - new.y0);
    }
    let mut placed: Vec<(usize, Vec<String>)> = vec![(index, doc.adopt(index, first, opts.replace))];
    for page in pages {
        let i = doc.add_page(&page.name);
        if let Some(p) = doc.pages.get_mut(i) {
            p.width = page.width;
            p.height = page.height;
            p.board = page.board;
        }
        placed.push((i, doc.adopt(i, page, true)));
    }
    let layout = opts.layout.map(|algorithm| Options { algorithm, direction: opts.direction, ..Options::default() });
    for (n, (i, ids)) in placed.iter().enumerate() {
        let Some(page) = doc.pages.get_mut(*i) else { continue };
        if let Some(layout) = &layout {
            let shapes: Vec<String> = ids.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
            if !shapes.is_empty() {
                // Only the first imported page can land beside existing shapes.
                let laid = if n == 0 && incremental {
                    autolayout::layout_incremental(page, &shapes, layout)
                } else {
                    autolayout::layout_items(page, Some(&shapes), layout)
                };
                match laid {
                    Ok(notes) => warnings.extend(notes),
                    Err(e) => warnings.push(format!("the layout was not applied: {e}")),
                }
            }
        }
        if opts.avoid {
            let connectors: Vec<String> = ids.iter().filter(|id| page.connector(id).is_some()).cloned().collect();
            for id in &connectors {
                if let Some(c) = page.connector_mut(id) {
                    c.route = Route::Orthogonal;
                    c.avoid = true;
                }
            }
            if !connectors.is_empty() {
                vizcraft_diagram::autoroute::reroute(page, Some(&connectors));
            }
        }
    }
    let ids = placed.into_iter().flat_map(|(_, ids)| ids).collect();
    Ok(Placed { index, ids, warnings })
}

/// Import into the open document as one undo step: place, select what came in (when it is on
/// the page the canvas shows), report.
pub(crate) fn import_into_session(s: &mut Session, imported: Imported, opts: &ImportOpts, source: Option<&str>) -> CmdResult {
    let placed = place(&mut s.doc, imported, opts)?;
    let page = s.doc.pages.get(placed.index);
    let page_id = page.map(|p| p.id.clone()).unwrap_or_default();
    let (shapes, connectors) = page.map_or((0, 0), |p| {
        let on = |id: &String| p.contains_id(id);
        (
            placed.ids.iter().filter(|id| on(id) && p.shape(id).is_some()).count(),
            placed.ids.iter().filter(|id| on(id) && p.connector(id).is_some()).count(),
        )
    });
    if placed.index == 0 {
        let here: Vec<String> = placed.ids.iter().filter(|id| s.doc.page().is_some_and(|p| p.contains_id(id))).cloned().collect();
        select(s, here);
    } else {
        prune_selection(s);
    }
    let from = source.map(|p| format!(" from {p}")).unwrap_or_default();
    s.status = format!("Imported {shapes} shapes and {connectors} connectors{from}");
    Ok(json!({"page": page_id, "ids": placed.ids, "warnings": placed.warnings}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    /// A fresh directory under the system temp dir, unique to this process and test.
    fn temp_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-file-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_open_restores_the_sample() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("sample.vizcraft");
        let mut s = Session::new(crate::sample::sample_document());
        let r = s.run("file.save", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["saved"], true);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 4"));

        let mut t = Session::new(Document::new());
        t.run("file.open", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(t.doc, s.doc);
        assert!(!t.doc.page().unwrap().connectors.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_writes_svg() {
        let dir = temp_dir("export");
        let mut s = Session::new(crate::sample::sample_document());
        let path = dir.join("page.svg");
        let r = s.run("file.export", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["exported"], true);
        assert_eq!(r["format"], "svg");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg"));
        assert!(s.status.starts_with("Exported"));
        assert!(!s.dirty, "exporting does not change the document");

        // An explicit format wins over the extension; an unknown one writes nothing.
        let other = dir.join("page.out");
        s.run("file.export", &json!({"path": other.to_string_lossy(), "format": "svg"})).unwrap();
        assert!(std::fs::read(&other).unwrap().starts_with(b"<?xml"));
        let bad = dir.join("page.jpg");
        assert!(s.run("file.export", &json!({"path": bad.to_string_lossy()})).is_err());
        assert!(!bad.exists());

        // Without a path the UI is asked for a picker.
        assert_eq!(s.run("file.export", &json!({})).unwrap()["exported"], false);
        assert_eq!(s.ui_requests.pop(), Some(json!({"open": "export"})));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_writes_png_and_picks_pages() {
        let dir = temp_dir("export-png");
        let mut s = Session::new(crate::sample::sample_document());
        let second = s.doc.add_page("Second");
        let second_id = s.doc.pages[second].id.clone();
        let path = dir.join("page.png");
        let shown = path.to_string_lossy().to_string();
        let r = s.run("file.export", &json!({"path": shown})).unwrap();
        assert_eq!((r["format"].as_str(), r["page"].as_str()), (Some("png"), Some(s.doc.pages[0].id.as_str())));
        let one = std::fs::read(&path).unwrap();
        assert!(one.starts_with(b"\x89PNG\r\n\x1a\n"));
        let width = |png: &[u8]| u32::from_be_bytes(png[16..20].try_into().unwrap());
        s.run("file.export", &json!({"path": shown, "scale": 2})).unwrap();
        let two = std::fs::read(&path).unwrap();
        assert!(width(&two).abs_diff(width(&one) * 2) <= 2, "{} against {}", width(&two), width(&one));

        // The page by id, name or index, for png and svg alike.
        for page in [json!(second_id), json!("Second"), json!(second), json!(second.to_string())] {
            let r = s.run("file.export", &json!({"path": shown, "page": page})).unwrap();
            assert_eq!(r["page"], json!(second_id), "{page}");
        }
        let svg = dir.join("second.svg");
        let r = s.run("file.export", &json!({"path": svg.to_string_lossy(), "page": "Second"})).unwrap();
        assert_eq!(r["page"].as_str(), Some(second_id.as_str()));
        // With no `page`, export follows the active page.
        s.run("page.setActive", &json!({"page": "Second"})).unwrap();
        let r = s.run("file.export", &json!({"path": svg.to_string_lossy()})).unwrap();
        assert_eq!(r["page"].as_str(), Some(second_id.as_str()), "export defaults to the active page");
        s.run("page.setActive", &json!({"page": 0})).unwrap();
        let r = s.run("file.export", &json!({"path": svg.to_string_lossy(), "page": "Second"})).unwrap();
        assert_eq!((r["format"].as_str(), r["page"].as_str()), (Some("svg"), Some(second_id.as_str())));

        // Bad pages and scales are params errors and write nothing.
        let none = dir.join("none.png");
        let none_shown = none.to_string_lossy().to_string();
        for bad in [
            json!({"page": "nope"}),
            json!({"page": 99}),
            json!({"page": -1}),
            json!({"page": 1.5}),
            json!({"page": true}),
            json!({"scale": 0}),
            json!({"scale": -2}),
            json!({"scale": "big"}),
        ] {
            let mut params = bad.clone();
            params["path"] = json!(none_shown);
            assert!(matches!(s.run("file.export", &params), Err(crate::EngineError::Params(_))), "{bad}");
            assert!(!none.exists(), "{bad}");
        }
        // Huge scales are clamped by the renderer rather than refused.
        assert!(s.run("file.export", &json!({"path": none_shown, "scale": 1e9})).is_ok());
        assert!(!s.dirty, "exporting does not change the document");
        let _ = std::fs::remove_dir_all(&dir);
    }

    const DOCS_EXAMPLE: &str = include_str!("../../../formats/testdata/drawio-docs-example.drawio");
    const MULTIPAGE: &[u8] = include_bytes!("../../../formats/testdata/multipage-compressed.drawio");

    fn page_shapes(s: &Session) -> Vec<String> {
        s.doc.page().unwrap().shapes.iter().map(|x| x.id.clone()).collect()
    }

    #[test]
    fn drawio_files_open_save_and_reopen() {
        let dir = temp_dir("drawio");
        let src = dir.join("multi.drawio");
        std::fs::write(&src, MULTIPAGE).unwrap();
        let mut s = Session::new(Document::new());
        s.run("file.open", &json!({"path": src.to_string_lossy()})).unwrap();
        assert_eq!(s.doc.pages.len(), 2);
        let list = s.run("page.list", &json!({})).unwrap();
        let get = s.run("page.get", &json!({"page": "Second"})).unwrap();
        let out = dir.join("out.drawio");
        assert_eq!(s.run("file.save", &json!({"path": out.to_string_lossy()})).unwrap()["saved"], true);
        assert!(std::fs::read_to_string(&out).unwrap().starts_with("<mxfile"));

        let mut t = Session::new(Document::new());
        t.run("file.open", &json!({"path": out.to_string_lossy()})).unwrap();
        assert_eq!(t.run("page.list", &json!({})).unwrap(), list);
        assert_eq!(t.run("page.get", &json!({"page": "Second"})).unwrap(), get);
        // A router-stored polyline is written with interior bends only, so a reread keeps fewer
        // waypoints; the line it draws is the same.
        assert_eq!(t.doc.pages.len(), s.doc.pages.len());
        for (x, y) in t.doc.pages.iter().zip(&s.doc.pages) {
            assert_eq!(x.shapes, y.shapes);
            assert_eq!(x.connectors.len(), y.connectors.len());
            for (cx, cy) in x.connectors.iter().zip(&y.connectors) {
                assert_eq!(vizcraft_diagram::route_polyline(x, cx), vizcraft_diagram::route_polyline(y, cy), "{} is drawn the same", cx.id);
            }
        }

        // Export as draw.io writes every page; the document stays as it was.
        let exported = dir.join("export.out");
        let r = s.run("file.export", &json!({"path": exported.to_string_lossy(), "format": "drawio"})).unwrap();
        assert_eq!(r["format"], "drawio");
        assert_eq!(std::fs::read_to_string(&exported).unwrap().matches("<diagram").count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_places_content_beside_what_is_there_in_one_undo_step() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let right = s.doc.page().unwrap().shapes.iter().map(|x| x.bounds().x1).fold(f64::MIN, f64::max);
        let r = s.run("file.import", &json!({"text": DOCS_EXAMPLE})).unwrap();
        let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(r["page"], s.doc.page().unwrap().id);
        assert_eq!(s.sel.items, ids, "the imported items are selected");
        let page = s.doc.page().unwrap();
        for id in &ids {
            if let Some(sh) = page.shape(id) {
                assert!(sh.x >= right + 59.0, "{id} at {} not right of {right}", sh.x);
            }
        }
        // The draw.io ids 2, 3 and 4 are taken by nothing in the sample, so they are kept.
        assert!(ids.contains(&"2".to_string()));
        assert!(s.status.starts_with("Imported 2 shapes and 1 connectors"));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
    }

    #[test]
    fn import_with_a_layout_beside_existing_shapes_pins_them() {
        let mut s = Session::new(crate::sample::sample_document());
        let before: Vec<(String, f64, f64)> = s.doc.page().unwrap().shapes.iter().map(|x| (x.id.clone(), x.x, x.y)).collect();
        let r = s.run("file.import", &json!({"text": DOCS_EXAMPLE, "layout": "layered"})).unwrap();
        let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).unwrap();
        let page = s.doc.page().unwrap();
        for (id, x, y) in &before {
            let sh = page.shape(id).unwrap();
            assert_eq!((sh.x, sh.y), (*x, *y), "{id} moved");
        }
        for id in ids.iter().filter(|id| page.shape(id).is_some()) {
            let a = page.shape(id).unwrap().bounds();
            for other in page.shapes.iter().filter(|o| &o.id != id) {
                let b = other.bounds();
                let overlap = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
                assert!(!overlap, "{id} overlaps {}", other.id);
            }
        }
    }

    #[test]
    fn import_replace_new_pages_and_multipage_sources() {
        let mut s = Session::new(crate::sample::sample_document());
        s.run("file.import", &json!({"text": DOCS_EXAMPLE, "replace": true})).unwrap();
        assert_eq!(page_shapes(&s), ["2", "3"]);
        // A page name that matches nothing makes a page; the canvas page keeps its selection.
        let r = s.run("file.import", &json!({"text": DOCS_EXAMPLE, "page": "Imported"})).unwrap();
        assert_eq!(s.doc.pages.len(), 2);
        assert_eq!(r["page"], s.doc.pages[1].id);
        assert_eq!(s.doc.pages[1].name, "Imported");
        assert_eq!(s.doc.pages[1].shapes.len(), 2);
        assert!(s.sel.items.iter().all(|id| s.doc.page().unwrap().contains_id(id)));
        // A two-page source fills the target and adds one page.
        let text = String::from_utf8(MULTIPAGE.to_vec()).unwrap();
        s.run("file.import", &json!({"text": text, "page": 0, "replace": true})).unwrap();
        assert_eq!(s.doc.pages.len(), 3);
        assert_eq!(s.doc.page().unwrap().shapes.len(), 16);
        assert_eq!(s.doc.pages[2].name, "Second");
        // Ids stay unique across the document.
        let mut seen = std::collections::HashSet::new();
        for p in &s.doc.pages {
            for id in p.shapes.iter().map(|x| &x.id).chain(p.connectors.iter().map(|c| &c.id)) {
                assert!(seen.insert(id.clone()), "{id} twice");
            }
        }
    }

    #[test]
    fn import_layout_and_routing_options() {
        let mut s = Session::new(Document::new());
        let r = s.run("file.import", &json!({"text": DOCS_EXAMPLE, "layout": "tree", "direction": "LR", "routing": "orthogonal"})).unwrap();
        let page = s.doc.page().unwrap();
        let (a, b) = (page.shape("2").unwrap(), page.shape("3").unwrap());
        assert!(a.x + a.w <= b.x, "left to right");
        let c = page.connector("4").unwrap();
        assert!(c.avoid);
        assert_eq!(c.route, vizcraft_diagram::Route::Orthogonal);
        assert!(r["warnings"].is_array());
        for bad in
            [json!({"layout": "spiral"}), json!({"direction": "up"}), json!({"routing": "fast"}), json!({"page": []}), json!({"format": "pdf"})]
        {
            let mut v = bad.clone();
            v["text"] = json!(DOCS_EXAMPLE);
            assert!(matches!(s.run("file.import", &v), Err(crate::EngineError::Params(_))), "{bad}");
        }
    }

    #[test]
    fn import_csv_text_and_files_by_extension() {
        let dir = temp_dir("csv");
        let csv = "# label: %name%\n# identity: id\n# connect: {\"from\": \"boss\", \"to\": \"id\"}\nid,name,boss\n1,A,\n2,B,1\n";
        let path = dir.join("people.csv");
        std::fs::write(&path, csv).unwrap();
        let mut s = Session::new(Document::new());
        let r = s.run("file.import", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["ids"].as_array().unwrap().len(), 3);
        let r = s.run("file.import", &json!({"text": csv, "replace": true})).unwrap();
        assert_eq!(r["ids"].as_array().unwrap().len(), 3);
        assert_eq!(s.doc.page().unwrap().shapes.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_without_a_source_asks_for_a_picker() {
        let mut s = Session::new(Document::new());
        assert_eq!(s.run("file.import", &json!({})).unwrap()["imported"], false);
        assert_eq!(s.ui_requests.pop(), Some(json!({"open": "import"})));
        assert!(s.run("file.import", &json!({"path": "/nonexistent/x.drawio"})).is_err());
    }

    #[test]
    fn import_hostile_text_never_panics() {
        let long = "<".repeat(100_000);
        let nested = format!("{}{}", "<a>".repeat(10_000), "</a>".repeat(10_000));
        let texts = [
            "",
            "\u{0}\u{ffff}",
            "<",
            "<mxfile>",
            "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"2\" vertex=\"1\" parent=\"0\"><mxGeometry x=\"1e308\" width=\"-1\" as=\"geometry\"/></mxCell></root></mxGraphModel>",
            "<mxfile><diagram>%%%%</diagram></mxfile>",
            &long,
            &nested,
            "# label: %x%\n",
            "#\n\"",
            "flowchart LR\nA-->B",
            "graph TD; A-->",
        ];
        for text in texts {
            for format in [None, Some("drawio"), Some("mermaid"), Some("csv")] {
                for extra in [
                    json!({}),
                    json!({"replace": true, "layout": "force", "routing": "orthogonal"}),
                    json!({"page": "x"}),
                    json!({"merge": true, "layout": "tree", "routing": "orthogonal", "source": "s"}),
                ] {
                    let mut s = Session::new(crate::sample::sample_document());
                    let mut v = extra.clone();
                    v["text"] = json!(text);
                    if let Some(f) = format {
                        v["format"] = json!(f);
                    }
                    let _ = s.run("file.import", &v);
                    for p in &s.doc.pages {
                        for sh in &p.shapes {
                            assert!(sh.x.is_finite() && sh.y.is_finite() && sh.w.is_finite() && sh.h.is_finite());
                        }
                    }
                    for id in &s.sel.items {
                        assert!(s.doc.page().unwrap().contains_id(id), "stale selection {id}");
                    }
                }
            }
        }
        let mut s = Session::new(Document::new());
        let huge = "x".repeat((crate::io::MAX_IMPORT_BYTES + 1) as usize);
        assert!(s.run("file.import", &json!({"text": huge})).is_err());
    }
}
