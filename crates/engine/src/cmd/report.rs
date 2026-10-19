//! Shape reports: tables built from the shapes and connectors of a page or the document.
//!
//! Three built-in reports: `inventory` (one row per shape), `data` (one row per shape that has
//! shape data, one column per data key in use) and `connections` (one row per connector). A run
//! returns the columns and rows, and by format also writes a CSV file, writes an HTML file with
//! one table, writes a Markdown pipe table or an XLSX workbook, or draws the table on the page as
//! a shape with one list compartment. The writers are `vizcraft_formats::tables`.

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, End, Page, Rect, Shape, compartment as cp};

use super::{opt_str, page_mut, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Most rows one report returns; past it the result says `truncated`.
pub const MAX_ROWS: usize = 50_000;

/// A built-in report: `(id, name, what it lists)`.
const REPORTS: &[(&str, &str, &str)] = &[
    ("inventory", "Shape Inventory", "One row per shape: id, name, type, text and page"),
    ("data", "Shape Data", "One row per shape with shape data, one column per data key in use"),
    ("connections", "Connections", "One row per connector: from, to, kind and relationship"),
];

/// Return the report commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("report.list", "Shape Reports", "Review > Reporting", list)
            .params(r#"{} -> {"reports": [{"id", "name", "description"}]}; the built-in reports"#),
        CommandSpec::new("report.run", "Run Shape Report", "Review > Reporting", run).params(
            r#"{"report": "inventory"|"data"|"connections", "scope"?: "page"|"document", "format"?: "csv"|"html"|"markdown"|"xlsx"|"table", "path"?: string, "page"?: id|name|index} -> {"report", "scope", "format", "columns": [string], "rows": [[string]], "truncated": bool, "path"?, "bytes"?, "content"?, "id"?}; scope defaults to the page; csv, html and markdown write the file at path (or return it as content when there is no path); xlsx needs a path (a workbook is binary); table draws a shape at the frame's right edge with one tab-separated row per record in a list compartment"#,
        ),
    ]
}

fn list(_: &mut Session, _: &Value) -> CmdResult {
    Ok(json!({"reports": REPORTS.iter().map(|(id, name, what)| json!({"id": id, "name": name, "description": what})).collect::<Vec<_>>()}))
}

/// A report's columns and rows.
struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    truncated: bool,
}

/// One line of text: line breaks and tabs become spaces so a record stays on its row.
fn flat(t: &str) -> String {
    t.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// What a shape is called in a report: its model element's name, else its text.
fn label(doc: &Document, sh: &Shape) -> String {
    let named = sh.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| e.name.clone()).filter(|n| !n.is_empty());
    flat(&named.unwrap_or_else(|| sh.text.clone()))
}

/// What a shape is: its element's type, else its stencil, else its outline.
fn type_of(doc: &Document, sh: &Shape) -> String {
    if let Some(e) = sh.element.as_deref().and_then(|e| doc.model.element(e)) {
        return e.type_id.clone();
    }
    if sh.stencil.is_empty() { sh.kind.name().to_string() } else { sh.stencil.clone() }
}

/// An end of a connector for the connections report: the glued shape's label (or id), or `free`.
fn end_name(doc: &Document, page: &Page, e: &End) -> String {
    match e.shape() {
        Some(id) => match page.shape(id) {
            Some(sh) => Some(label(doc, sh)).filter(|l| !l.is_empty()).unwrap_or_else(|| id.to_string()),
            None => id.to_string(),
        },
        None => "free".to_string(),
    }
}

fn relationship_of(doc: &Document, c: &Connector) -> String {
    c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).map(|r| r.type_id.clone()).unwrap_or_default()
}

fn build(doc: &Document, report: &str, pages: &[usize]) -> Table {
    let mut t = Table { columns: Vec::new(), rows: Vec::new(), truncated: false };
    let full = |t: &mut Table| {
        if t.rows.len() >= MAX_ROWS {
            t.truncated = true;
        }
        t.truncated
    };
    let pages: Vec<&Page> = pages.iter().filter_map(|i| doc.pages.get(*i)).collect();
    match report {
        "inventory" => {
            t.columns = ["id", "name", "type", "text", "page"].map(String::from).to_vec();
            for page in pages {
                for sh in &page.shapes {
                    if full(&mut t) {
                        return t;
                    }
                    let name = sh.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| flat(&e.name)).unwrap_or_default();
                    t.rows.push(vec![sh.id.clone(), name, type_of(doc, sh), flat(&sh.text), page.name.clone()]);
                }
            }
        }
        "data" => {
            let mut keys: Vec<String> = Vec::new();
            for page in &pages {
                for row in page.shapes.iter().flat_map(|sh| &sh.data.rows) {
                    if !keys.contains(&row.key) {
                        keys.push(row.key.clone());
                    }
                }
            }
            t.columns = ["id".to_string(), "page".to_string()].into_iter().chain(keys.iter().cloned()).collect();
            for page in pages {
                for sh in page.shapes.iter().filter(|sh| !sh.data.rows.is_empty()) {
                    if full(&mut t) {
                        return t;
                    }
                    let mut row = vec![sh.id.clone(), page.name.clone()];
                    row.extend(keys.iter().map(|k| sh.data.rows.iter().find(|r| &r.key == k).map(|r| flat(&r.value)).unwrap_or_default()));
                    t.rows.push(row);
                }
            }
        }
        _ => {
            t.columns = ["from", "to", "kind", "relationship"].map(String::from).to_vec();
            for page in pages {
                for c in &page.connectors {
                    if full(&mut t) {
                        return t;
                    }
                    t.rows.push(vec![end_name(doc, page, &c.start), end_name(doc, page, &c.end), c.kind.clone(), relationship_of(doc, c)]);
                }
            }
        }
    }
    t
}

/// The report as the shared table type the writers take, its columns typed from the cells.
fn as_table(t: &Table) -> vizcraft_formats::table::Table {
    vizcraft_formats::tables::from_text(&t.columns, t.rows.clone())
}

fn html_escape(t: &str) -> String {
    t.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// The table as an HTML page: the document's title as a heading and one table.
fn to_html(title: &str, report: &str, t: &Table) -> String {
    let mut out = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>{0}</title></head>\n<body>\n<h1>{0}</h1>\n<h2>{1}</h2>\n<table border=\"1\">\n<thead><tr>",
        html_escape(title),
        html_escape(report)
    );
    out.extend(t.columns.iter().map(|c| format!("<th>{}</th>", html_escape(c))));
    out.push_str("</tr></thead>\n<tbody>\n");
    for row in &t.rows {
        out.push_str("<tr>");
        out.extend(row.iter().map(|f| format!("<td>{}</td>", html_escape(f))));
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n</body>\n</html>\n");
    out
}

/// The title a report carries: the document's title, else its title property, else `Untitled`.
fn doc_title(doc: &Document) -> String {
    [&doc.title, &doc.properties.title].into_iter().find(|t| !t.trim().is_empty()).cloned().unwrap_or_else(|| "Untitled".to_string())
}

/// Draw the table on the active page, right of everything on it, and return the shape's id.
fn draw_table(s: &mut Session, v: &Value, name: &str, t: &Table) -> Result<String, EngineError> {
    let id = super::new_id(s, v, "s")?;
    let frame = s.doc.page().map(Page::frame).unwrap_or(Rect::from_xywh(0.0, 0.0, Page::DEFAULT_WIDTH, Page::DEFAULT_HEIGHT));
    let widest = t.rows.iter().chain(std::iter::once(&t.columns)).map(|r| r.join("    ").chars().count()).max().unwrap_or(0);
    let w = (widest as f64 * 7.0 + 24.0).clamp(160.0, 900.0);
    let mut shape = vizcraft_stencils::new_shape("basic.rectangle", id.clone(), frame.x1 + 40.0, frame.y0, Some(w), Some(40.0))
        .ok_or_else(|| EngineError::Failed("the rectangle master is missing".into()))?;
    shape.text = name.to_string();
    let mut list = cp::Compartment::new("table", cp::CompartmentKind::List);
    list.title = Some(t.columns.join("\t"));
    list.rows = t.rows.iter().take(cp::MAX_ROWS).map(|r| r.join("\t")).collect();
    shape.compartments.push(list);
    shape.sanitize();
    let page = page_mut(s)?;
    page.shapes.push(shape);
    super::compartment::fit_shape(page, &id);
    select(s, vec![id.clone()]);
    Ok(id)
}

fn run(s: &mut Session, v: &Value) -> CmdResult {
    let report = p::req_str(v, "report")?;
    let Some((_, name, _)) = REPORTS.iter().find(|(id, ..)| *id == report) else {
        return Err(EngineError::Params(format!("`report` must be inventory, data or connections, not `{}`", short(report))));
    };
    let document = match opt_str(v, "scope")? {
        None | Some("page") => false,
        Some("document") => true,
        Some(o) => return Err(EngineError::Params(format!("`scope` must be page or document, not `{}`", short(o)))),
    };
    let format = opt_str(v, "format")?.unwrap_or("csv");
    if !matches!(format, "csv" | "html" | "markdown" | "xlsx" | "table") {
        return Err(EngineError::Params(format!("`format` must be csv, html, markdown, xlsx or table, not `{}`", short(format))));
    }
    let path = opt_str(v, "path")?;
    if path.is_some() && format == "table" {
        return Err(EngineError::Params("`path` is for files; a table is drawn on the page".into()));
    }
    if format == "xlsx" && path.is_none() {
        return Err(EngineError::Params("`path` is required for xlsx: a workbook is a binary file".into()));
    }
    let pages: Vec<usize> = if document {
        (0..s.doc.pages.len()).collect()
    } else {
        match super::proof::page_ref(v)? {
            Some(r) => vec![s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))?],
            None => vec![s.doc.active_index()],
        }
    };
    let t = build(&s.doc, report, &pages);
    let mut out = json!({
        "report": report, "scope": if document { "document" } else { "page" }, "format": format,
        "columns": t.columns, "rows": t.rows, "truncated": t.truncated,
    });
    let text = match format {
        "csv" => Some(vizcraft_formats::tables::write_csv(&as_table(&t))),
        "html" => Some(to_html(&doc_title(&s.doc), name, &t)),
        "markdown" => Some(vizcraft_formats::tables::write_markdown(&as_table(&t))),
        _ => None,
    };
    let bytes = match (format, path) {
        ("xlsx", Some(_)) => Some(vizcraft_formats::tables::write_xlsx(&as_table(&t), name).map_err(|e| EngineError::Failed(e.to_string()))?),
        _ => text.as_ref().map(|t| t.as_bytes().to_vec()),
    };
    if let Some(o) = out.as_object_mut() {
        match (path, bytes) {
            (Some(path), Some(bytes)) => {
                std::fs::write(path, &bytes).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
                o.insert("path".into(), json!(path));
                o.insert("bytes".into(), json!(bytes.len()));
            }
            _ => {
                if let Some(text) = text {
                    o.insert("content".into(), json!(text));
                }
            }
        }
    }
    if format == "table" {
        let id = draw_table(s, v, name, &t)?;
        if let Some(o) = out.as_object_mut() {
            o.insert("id".into(), json!(id));
        }
        return with_sel(s, out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::ShapeKind;

    fn run_ok(s: &mut Session, params: Value) -> Value {
        s.run("report.run", &params).unwrap_or_else(|e| panic!("report.run: {e}"))
    }

    /// The sample document, which has shapes, connectors, elements and relationships.
    fn sample() -> Session {
        Session::new(crate::sample::sample_document())
    }

    /// A small hand-made page so goldens do not move when the sample is edited.
    fn small() -> Session {
        let mut doc = Document::new();
        doc.title = "Plant, \"A\"".into();
        let page = doc.page_mut().unwrap();
        let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        a.text = "Pump, main".into();
        a.stencil = "basic.rectangle".into();
        a.data.rows.push(vizcraft_diagram::data::DataRow { key: "cost".into(), value: "12".into(), ..Default::default() });
        let mut b = Shape::new("b", ShapeKind::Ellipse, 300.0, 0.0, 100.0, 60.0);
        b.text = "Tank\nline two".into();
        page.shapes.extend([a, b]);
        page.connectors.push(Connector::new("c", End::dynamic("a"), End::dynamic("b")));
        Session::new(doc)
    }

    #[test]
    fn list_names_the_three_reports() {
        let mut s = small();
        let r = s.run("report.list", &json!({})).unwrap();
        let ids: Vec<&str> = r["reports"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["inventory", "data", "connections"]);
    }

    #[test]
    fn inventory_csv_matches_its_golden() {
        let mut s = small();
        let r = run_ok(&mut s, json!({"report": "inventory", "format": "csv"}));
        let golden = "id,name,type,text,page\na,,basic.rectangle,\"Pump, main\",Page-1\nb,,ellipse,Tank line two,Page-1\n";
        assert_eq!(r["content"], golden);
        assert_eq!(r["rows"].as_array().unwrap().len(), 2);
        let dir = std::env::temp_dir().join(format!("vizcraft-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("inventory.csv");
        let r = run_ok(&mut s, json!({"report": "inventory", "format": "csv", "path": path.to_string_lossy()}));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), golden);
        assert_eq!(r["bytes"], golden.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sample_inventory_has_a_row_per_shape() {
        let mut s = sample();
        let n: usize = s.doc.pages.iter().map(|p| p.shapes.len()).sum();
        let r = run_ok(&mut s, json!({"report": "inventory", "scope": "document", "format": "csv"}));
        assert_eq!(r["rows"].as_array().unwrap().len(), n);
        assert_eq!(r["content"].as_str().unwrap().lines().count(), n + 1);
        assert!(r["content"].as_str().unwrap().starts_with("id,name,type,text,page\n"));
    }

    #[test]
    fn html_has_one_table_the_title_and_a_row_per_record() {
        let mut s = small();
        let r = run_ok(&mut s, json!({"report": "connections", "format": "html"}));
        let html = r["content"].as_str().unwrap();
        assert_eq!(html.matches("<table").count(), 1);
        assert_eq!(html.matches("<tr>").count(), 2, "a header row and one connector");
        assert!(html.contains("<title>Plant, &quot;A&quot;</title>"));
        assert!(html.contains("<td>Pump, main</td><td>Tank line two</td>"));
        let r = run_ok(&mut s, json!({"report": "inventory", "format": "html"}));
        assert_eq!(r["content"].as_str().unwrap().matches("<tr>").count(), 3);
    }

    #[test]
    fn data_report_has_a_column_per_key() {
        let mut s = small();
        let r = run_ok(&mut s, json!({"report": "data", "format": "csv"}));
        assert_eq!(r["columns"], json!(["id", "page", "cost"]));
        assert_eq!(r["rows"], json!([["a", "Page-1", "12"]]));
    }

    #[test]
    fn table_format_draws_a_shape_right_of_the_frame_with_one_row_per_record() {
        let mut s = small();
        let frame_right = s.doc.page().unwrap().frame().x1;
        let r = run_ok(&mut s, json!({"report": "inventory", "format": "table"}));
        let id = r["id"].as_str().unwrap();
        let sh = s.doc.page().unwrap().shape(id).unwrap();
        assert_eq!(sh.text, "Shape Inventory");
        assert_eq!(sh.compartments.len(), 1);
        assert_eq!(sh.compartments[0].rows.len(), 2);
        assert_eq!(sh.compartments[0].rows[0], "a\t\tbasic.rectangle\tPump, main\tPage-1");
        assert_eq!(sh.compartments[0].title.as_deref(), Some("id\tname\ttype\ttext\tpage"));
        assert!(sh.x >= frame_right, "placed at the frame's right edge");
        assert_eq!(s.sel.primary.as_deref(), Some(id));
        assert!(s.can_undo());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), 2);
    }

    #[test]
    fn markdown_and_xlsx_are_written() {
        let mut s = small();
        let r = run_ok(&mut s, json!({"report": "data", "format": "markdown"}));
        assert_eq!(r["content"], "| id | page | cost |\n| --- | --- | --- |\n| a | Page-1 | 12 |\n");
        let dir = std::env::temp_dir().join(format!("vizcraft-report-xlsx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("inventory.xlsx");
        let r = run_ok(&mut s, json!({"report": "inventory", "format": "xlsx", "path": path.to_string_lossy()}));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(r["bytes"], bytes.len());
        assert!(r.get("content").is_none());
        #[cfg(not(target_arch = "wasm32"))]
        {
            let t = vizcraft_formats::table::read(&bytes, vizcraft_diagram::data_graphic::TableFormat::Xlsx, "").unwrap();
            let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, ["id", "name", "type", "text", "page"]);
            assert_eq!(t.rows.len(), 2);
            assert_eq!(t.rows[0][3], "Pump, main");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_params_are_refused() {
        let mut s = small();
        for bad in [
            json!({}),
            json!({"report": "nope"}),
            json!({"report": "inventory", "scope": "all"}),
            json!({"report": "inventory", "format": "xlsx"}),
            json!({"report": "inventory", "format": "pdf"}),
            json!({"report": "inventory", "format": "table", "path": "x"}),
            json!({"report": "inventory", "page": "missing"}),
        ] {
            assert!(s.run("report.run", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn running_a_csv_report_changes_nothing() {
        let mut s = small();
        let before = s.doc.clone();
        run_ok(&mut s, json!({"report": "connections", "format": "csv"}));
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }
}
