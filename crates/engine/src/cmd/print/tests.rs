//! `file.export` with PDF and several pages, `file.print` and `page.setHeaderFooter`.

use serde_json::{Value, json};
use vizcraft_diagram::{Layer, Shape, ShapeKind};

use crate::Session;
use crate::doc::Document;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A fresh directory for one test's files.
fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vizcraft-print-tests-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn path_in(d: &std::path::Path, name: &str) -> String {
    d.join(name).to_string_lossy().to_string()
}

/// `/Count` of the PDF page tree.
fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.split("/Count").skip(1).filter_map(|r| r.trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).max().unwrap_or(0)
}

fn three_pages() -> Session {
    let mut doc = Document::new();
    doc.add_page("Second");
    let i = doc.add_page("Back");
    doc.pages[i].is_background = true;
    Session::new(doc)
}

#[test]
fn export_pdf_holds_every_foreground_page_by_default() {
    let d = dir("pdf");
    let mut s = three_pages();
    let out = path_in(&d, "doc.pdf");
    let r = run(&mut s, "file.export", json!({"path": out}));
    assert_eq!(r["format"], "pdf");
    assert_eq!(r["pages"].as_array().unwrap().len(), 2, "the background page is left out: {r}");
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(page_count(&bytes), 2);
    assert!(String::from_utf8_lossy(&bytes).contains("/FontFile") || s.doc.pages.iter().all(|p| p.shapes.is_empty()));
    let one = path_in(&d, "one.out");
    run(&mut s, "file.export", json!({"path": one, "format": "pdf", "pages": "active"}));
    assert_eq!(page_count(&std::fs::read(&one).unwrap()), 1);
    run(&mut s, "file.export", json!({"path": one, "format": "PDF", "pages": ["Back", 0, "Back"]}));
    assert_eq!(page_count(&std::fs::read(&one).unwrap()), 2, "repeats are dropped, background pages may be named");
}

#[test]
fn multi_page_png_writes_every_page() {
    let d = dir("png");
    let mut s = three_pages();
    let out = path_in(&d, "plan.png");
    let r = run(&mut s, "file.export", json!({"path": out, "pages": "all"}));
    let paths: Vec<String> = r["paths"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
    assert_eq!(paths, vec![path_in(&d, "plan-1.png"), path_in(&d, "plan-2.png")]);
    for p in &paths {
        assert!(std::fs::read(p).unwrap().starts_with(b"\x89PNG"), "{p}");
    }
    assert!(!std::path::Path::new(&out).exists(), "only the numbered files are written");
    // One page keeps the plain name.
    let r = run(&mut s, "file.export", json!({"path": out}));
    assert_eq!(r["paths"], json!([out]));
    // Several pages to one SVG is refused before anything is written.
    let svg = path_in(&d, "x.svg");
    assert!(s.run("file.export", &json!({"path": svg, "pages": "all"})).is_err());
    assert!(!std::path::Path::new(&svg).exists());
}

#[test]
fn export_matches_the_screen_unless_print_is_set() {
    let d = dir("printflag");
    let mut s = Session::new(Document::new());
    if let Some(p) = s.doc.pages.first_mut() {
        let mut n = Shape::new("ghost", ShapeKind::Rectangle, 10.0, 10.0, 50.0, 50.0);
        n.behavior.non_printing = true;
        p.shapes.push(n);
    }
    let out = path_in(&d, "a.svg");
    run(&mut s, "file.export", json!({"path": out}));
    assert!(std::fs::read_to_string(&out).unwrap().contains("<g id=\"ghost\">"));
    run(&mut s, "file.export", json!({"path": out, "print": true}));
    assert!(!std::fs::read_to_string(&out).unwrap().contains("<g id=\"ghost\">"));
}

#[test]
fn header_and_footer_are_set_and_expanded_on_export() {
    let d = dir("hf");
    let mut s = three_pages();
    s.doc.title = "Works".into();
    let r = run(
        &mut s,
        "page.setHeaderFooter",
        json!({"all": true, "header": {"left": "{title}", "right": "{date}"}, "footer": {"center": "Page {page} of {pages}"}, "margin": 40}),
    );
    assert_eq!(r["pages"].as_array().unwrap().len(), 3);
    assert_eq!(r["footer"]["margin"], 40.0);
    // Slots not given keep their text.
    run(&mut s, "page.setHeaderFooter", json!({"page": "Second", "header": {"center": "Mid"}}));
    assert_eq!(s.doc.pages[1].header.left, "{title}");
    assert_eq!(s.doc.pages[1].header.center, "Mid");
    let out = path_in(&d, "p2.svg");
    run(&mut s, "file.export", json!({"path": out, "page": "Second"}));
    let svg = std::fs::read_to_string(&out).unwrap();
    assert!(svg.contains(">Page 2 of 2</tspan>") && svg.contains(">Works</tspan>") && svg.contains(">Mid</tspan>"), "{svg}");
    assert!(svg.contains(&format!(">{}</tspan>", super::today())), "{svg}");
    // Undo puts the header back.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.pages[1].header.center, "");
    run(&mut s, "page.setHeaderFooter", json!({"all": true, "margin": null}));
    assert!(s.doc.pages.iter().all(|p| p.header.margin.is_none() && p.footer.margin.is_none()));
}

#[test]
fn print_skips_non_printing_shapes_tiles_and_asks_the_os() {
    let d = dir("print");
    let mut s = Session::new(Document::new());
    if let Some(p) = s.doc.pages.first_mut() {
        p.layers = vec![Layer { id: "notes".into(), print: false, ..Layer::default() }];
        let mut far = Shape::new("far", ShapeKind::Rectangle, 4000.0, 10.0, 50.0, 50.0);
        far.layers = vec!["notes".into()];
        p.shapes.push(far);
        p.shapes.push(Shape::new("wide", ShapeKind::Rectangle, 0.0, 100.0, 2000.0, 100.0));
    }
    let out = path_in(&d, "print.pdf");
    // Without `send`, an agent or script only writes the PDF: nothing goes to a printer.
    let r = run(&mut s, "file.print", json!({"path": out}));
    assert_eq!(r["sent"].as_bool(), Some(false), "{r}");
    assert!(s.ui_requests.is_empty(), "printing is opt-in");
    let r = run(&mut s, "file.print", json!({"path": out, "send": true}));
    // The far shape is on a non-printing layer, so only the wide one counts: two sheets across.
    assert_eq!(r["sheets"], 2, "{r}");
    assert_eq!(r["pages"][0]["cols"], 2);
    assert_eq!(page_count(&std::fs::read(&out).unwrap()), 2);
    assert_eq!(s.ui_requests.last(), Some(&json!({"print": out})));
    s.ui_requests.clear();
    let r = run(&mut s, "file.print", json!({"path": out, "fit": true, "send": false}));
    assert_eq!((r["sheets"].as_u64(), r["sent"].as_bool()), (Some(1), Some(false)));
    assert!(s.ui_requests.is_empty(), "send false only writes the file");
    let r = run(&mut s, "file.print", json!({"path": out, "tiles": [3, 2], "paper": "a4", "send": false}));
    assert_eq!(r["sheets"], 6);
    let r = run(&mut s, "file.print", json!({"path": out, "paper": {"width": 1100, "height": 850}, "send": false}));
    assert_eq!(r["sheets"], 2);
    // No path: a temporary file.
    let r = run(&mut s, "file.print", json!({"send": false}));
    let tmp = r["path"].as_str().unwrap().to_string();
    assert!(std::fs::read(&tmp).unwrap().starts_with(b"%PDF-"));
    let _ = std::fs::remove_file(tmp);
}

#[test]
fn save_as_pdf_exports_instead_of_saving() {
    let d = dir("saveas");
    let mut s = three_pages();
    let out = path_in(&d, "Doc.PDF");
    let r = run(&mut s, "file.saveAs", json!({"path": out}));
    assert_eq!((r["saved"].as_bool(), r["format"].as_str()), (Some(false), Some("pdf")));
    assert_eq!(page_count(&std::fs::read(&out).unwrap()), 2);
    assert!(s.path.is_none(), "the document is not now a PDF file");
}

#[test]
fn hostile_print_and_header_params_are_errors() {
    let mut s = three_pages();
    let before = s.doc.clone();
    for (id, v) in [
        ("file.print", json!({"pages": 7, "send": false})),
        ("file.print", json!({"pages": [], "send": false})),
        ("file.print", json!({"pages": "nope", "send": false})),
        ("file.print", json!({"page": 0, "pages": "all", "send": false})),
        ("file.print", json!({"tiles": [0, 1], "send": false})),
        ("file.print", json!({"tiles": [1], "send": false})),
        ("file.print", json!({"tiles": [1, 1000], "send": false})),
        ("file.print", json!({"paper": "napkin", "send": false})),
        ("file.print", json!({"paper": {"width": -1, "height": 5}, "send": false})),
        ("file.print", json!({"paper": 3, "send": false})),
        ("file.print", json!({"fit": "yes", "send": false})),
        ("file.print", json!({"path": "", "send": false})),
        ("file.print", json!({"path": "/no/such/dir/x.pdf", "send": false})),
        ("file.export", json!({"path": "/tmp/x.pdf", "pages": {"a": 1}})),
        ("file.export", json!({"path": "/tmp/x.pdf", "pages": [true]})),
        ("file.export", json!({"path": "/tmp/x.pdf", "print": 1})),
        ("page.setHeaderFooter", json!({"header": "left"})),
        ("page.setHeaderFooter", json!({"header": {"middle": "x"}})),
        ("page.setHeaderFooter", json!({"header": {"left": 5}})),
        ("page.setHeaderFooter", json!({"header": {"left": "x".repeat(5000)}})),
        ("page.setHeaderFooter", json!({"margin": -1})),
        ("page.setHeaderFooter", json!({"margin": "wide"})),
        ("page.setHeaderFooter", json!({"all": true, "page": 0})),
        ("page.setHeaderFooter", json!({"header": {"left": "ok"}, "footer": {"left": 1}})),
    ] {
        assert!(s.run(id, &v).is_err(), "{id} {v}");
    }
    assert_eq!(s.doc, before, "failed commands change nothing");
    assert!(s.ui_requests.is_empty());
}
