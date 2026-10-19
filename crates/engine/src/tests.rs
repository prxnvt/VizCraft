use serde_json::{Value, json};

use crate::{EngineError, Session, cmd, sample};

fn s() -> Session {
    Session::new(sample::sample_document())
}

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn registry_ids_are_unique_and_prefixed() {
    let reg = cmd::registry();
    let mut seen = std::collections::HashSet::new();
    for c in reg.all() {
        assert!(seen.insert(c.id), "duplicate id {}", c.id);
        let prefix = c.id.split('.').next().unwrap();
        assert!(
            [
                "file",
                "doc",
                "edit",
                "view",
                "shape",
                "arrange",
                "container",
                "lane",
                "callout",
                "compartment",
                "connector",
                "page",
                "layer",
                "data",
                "datagraphic",
                "datalink",
                "design",
                "layout",
                "model",
                "element",
                "inspector",
                "relationship",
                "validate",
                "notation",
                "insert",
                "suggest",
                "table",
                "pattern",
                "link",
                "comment",
                "process",
                "script",
                "addon",
                "macro",
                "help",
                "guide",
                "text",
                "shapeop",
                "master",
                "proof",
                "report",
                "picture",
                "ink",
                "variant",
                "generate"
            ]
            .contains(&prefix),
            "{}: unexpected prefix",
            c.id
        );
        assert!(!c.label.is_empty() && !c.location.is_empty(), "{}: label and location are required", c.id);
    }
    assert_eq!(reg.by_shortcut("mod+s").map(|c| c.id), Some("file.save"));
    assert_eq!(reg.by_shortcut("Mod+Y").map(|c| c.id), Some("edit.redo"));
}

#[test]
fn unknown_and_disabled_commands_are_errors() {
    let mut s = s();
    assert_eq!(s.run("nope.nothing", &json!({})), Err(EngineError::Unknown("nope.nothing".into())));
    assert!(matches!(s.run("edit.undo", &json!({})), Err(EngineError::Disabled(_))));
}

#[test]
fn hostile_params_never_panic() {
    let reg = cmd::registry();
    let junk = [
        json!(null),
        json!({}),
        json!([]),
        json!("text"),
        json!({"text": 5, "title": [], "path": {}, "note": 9999, "start": -1, "end": 1e300, "value": "x"}),
        json!({"value": -1e308, "note": 1e9, "start": 1e9, "end": 0}),
        json!({"text": "\u{0}\u{ffff}", "title": "", "value": f64::NAN, "name": ""}),
        json!({"ids": "x", "id": 5, "from": 5, "to": "s2", "stencil": 7, "angle": "NaN"}),
        json!({"ids": [1, null, {}], "dx": "inf", "dy": "-inf", "x": "NaN", "y": [], "w": {}, "h": true}),
        json!({"ids": ["s2", "c8", "nope"], "dx": 1e308, "dy": -1e308, "angle": 1e308, "lineWidth": 1e308, "fontSize": -1e308}),
        json!({"ids": ["s2"], "dx": f64::MAX, "dy": f64::MIN, "fill": "#zzz", "line": "", "fontColor": 3, "bold": "yes", "italic": 1}),
        json!({"id": "s2", "x": 1e308, "y": -1e308, "w": 1e308, "h": -1e308, "text": "x".repeat(70_000)}),
        json!({"id": "s3", "x": 0, "y": 0, "w": 0, "h": 0, "angle": -1e300, "text": ""}),
        json!({"stencil": "basic.rectangle", "x": 1e308, "y": -1e308, "w": 1e308, "h": 0, "text": "t"}),
        json!({"stencil": "nope.nope", "x": 0, "y": 0}),
        json!({"from": {"shape": "s2", "point": 99}, "to": {"x": "1", "y": 2}}),
        json!({"from": {"shape": "s2", "point": -1}, "to": {"shape": "s3", "point": 1.5}, "route": "zigzag", "startArrow": 5}),
        json!({"from": {"x": 1e308, "y": -1e308}, "to": {"shape": "s4"}, "route": "curved", "endArrow": "circle", "text": 9}),
        json!({"id": "c8", "end": "middle", "shape": "s9", "point": 1e20}),
        json!({"id": "c8", "end": "start", "x": 1e308, "y": -1e308}),
        json!({"id": "c9", "end": "end", "shape": "s2", "point": 18446744073709551615u64}),
        json!({"ids": ["c8", "c9"], "route": "", "start": "triangle", "end": "zz"}),
        json!({"ids": [], "add": "yes"}),
        json!({"ids": ["s2"], "add": true}),
        json!({"text": "{\"vizcraft\": \"clip\", \"shapes\": [1, {}, {\"id\": 3, \"x\": \"a\"}], \"connectors\": [{\"start\": {\"shape\": \"zz\"}}]}"}),
        json!({"text": "{\"vizcraft\": \"clip\", \"shapes\": {}, \"connectors\": 5}"}),
        json!({"text": "not json at all"}),
        json!({"commands": [{"command": "shape.move", "params": {"ids": ["s2"], "dx": 1e308}}, {"command": "nope.nope"}], "label": 5}),
        json!({"commands": [{"command": "shape.move", "params": {"ids": ["s2"], "dx": 5}}, {"id": "edit.undo"}], "label": "x".repeat(1000)}),
        json!({"commands": [{"command": "file.new"}, {"command": "edit.selectAll"}, {"command": "edit.delete"}, {"command": "fail.now"}]}),
        json!({"commands": [{"command": 5}, {"params": []}], "script": 7}),
        json!({"commands": "x", "script": "[{\"command\": \"edit.batch\", \"params\": {\"commands\": [{\"command\": \"script.run\", \"params\": {\"script\": []}}]}}]"}),
        json!({"script": [{"command": "edit.selectAll"}, {"command": "edit.copy"}, {"command": "edit.paste"}, {"id": "edit.delete"}], "label": ""}),
        json!({"script": "not json", "commands": []}),
        json!({"bytes": "!!not base64!!", "id": "s2", "crop": {"x": "a", "w": -1e308}, "points": [[1e308, "x"], {}], "mediaType": "image/gif", "maxDimension": -5, "quality": 1e300, "recolor": 7}),
        json!({"instance": "s2", "direction": "fromTable", "table": {"columns": [5, null], "rows": [[{}], "x", null]}, "path": "/nonexistent/\u{0}.csv", "source": "\u{0}", "merge": "yes", "type": 7}),
        nested_batches(40),
    ];
    for spec in reg.all() {
        // File commands touch the disk; given a path they would write junk files. `page.set` with a
        // path edits a file on disk the same way.
        if spec.id.starts_with("file.") || spec.id == "page.set" {
            continue;
        }
        for j in &junk {
            // Once with nothing selected and once with everything selected, so commands that
            // fall back to the selection run their bodies too.
            for select_all in [false, true] {
                let mut s = s();
                // A canvas size, as the UI reports one, so commands that need it run their bodies.
                s.view.canvas = Some([800.0, 600.0]);
                if select_all {
                    run(&mut s, "edit.selectAll", json!({}));
                }
                let _ = s.run(spec.id, j);
                assert!(s.view.zoom.is_finite() && s.view.pan.iter().all(|v| v.is_finite()), "{}: {j}", spec.id);
                let page = s.doc.page().unwrap();
                for sh in &page.shapes {
                    assert!(sh.x.is_finite() && sh.y.is_finite() && sh.w.is_finite() && sh.h.is_finite(), "{}: {j}", spec.id);
                }
                for id in &s.sel.items {
                    assert!(page.contains_id(id), "{}: stale selection {id}", spec.id);
                }
            }
        }
    }
}

/// `edit.batch` params with `depth` batches nested inside each other around one shape move.
fn nested_batches(depth: usize) -> Value {
    let mut v = json!({"commands": [{"command": "shape.move", "params": {"ids": ["s2"], "dx": 1}}]});
    for _ in 0..depth {
        v = json!({"commands": [{"command": "edit.batch", "params": v}, {"command": "script.run", "params": {"script": [{"command": "edit.selectNone"}]}}]});
    }
    v
}

#[test]
fn a_panicking_command_restores_the_document() {
    let mut s = s();
    let mut specs = s.registry.all().to_vec();
    specs.push(crate::CommandSpec::new("edit.boom", "Boom", "Tests", |s, _| {
        s.doc.title = "half done".into();
        let v: Vec<u8> = Vec::new();
        let _ = v[3];
        Ok(json!({}))
    }));
    s.registry = crate::Registry::new(specs);
    let before = s.doc.clone();
    let r = s.run("edit.boom", &json!({}));
    assert!(matches!(r, Err(EngineError::Failed(ref m)) if m.contains("panicked")), "{r:?}");
    assert_eq!(s.doc, before);
    assert!(!s.dirty);
    assert!(!s.can_undo());
    assert!(s.status.contains("panicked"));
}

#[test]
fn undo_round_trips() {
    let mut s = s();
    let original = s.doc.clone();
    let rev0 = s.rev();
    run(&mut s, "edit.setTitle", json!({"title": "First"}));
    assert!(s.dirty);
    assert!(s.rev() > rev0);
    assert_eq!(s.undo_label(), Some("Document Title"));
    run(&mut s, "edit.setTitle", json!({"title": "Second"}));
    assert_eq!(s.doc.title, "Second");
    assert_eq!(s.undo_label(), Some("Document Title"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.title, "First");
    assert_eq!(s.redo_label(), Some("Document Title"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, original);
    assert!(!s.can_undo());
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.title, "First");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.title, "Second");
    assert!(!s.can_redo());
}

#[test]
fn view_commands_are_not_undo_steps() {
    let mut s = s();
    run(&mut s, "view.zoomIn", json!({}));
    run(&mut s, "view.dark", json!({}));
    run(&mut s, "view.navigationPane", json!({"value": false}));
    assert!(!s.can_undo());
    assert!(!s.dirty);
    assert!((s.view.zoom - 1.1).abs() < 1e-5);
    assert!(s.view.dark_mode);
    assert!(!s.view.nav_pane);
    run(&mut s, "view.zoom", json!({"value": 10_000}));
    assert_eq!(s.view.zoom, cmd::view::MAX_ZOOM / 100.0);
    assert!(s.run("view.zoom", &json!({"value": "big"})).is_err());
    run(&mut s, "view.zoom", json!({}));
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "zoom"})));
}

#[test]
fn zoom_fit_shows_the_whole_page_or_drawing() {
    let mut s = s();
    // Without a canvas there is nothing to fit into: disabled, and the view is untouched.
    assert!(matches!(s.run("view.zoomFit", &json!({})), Err(EngineError::Disabled(_))));
    assert_eq!(s.view.zoom, 1.0);
    // A page as wide as a laid-out Mermaid import, in a canvas that cuts it off at 100%.
    s.doc.page_mut().unwrap().width = 1470.0;
    s.doc.page_mut().unwrap().board = vizcraft_diagram::Board::Fixed;
    s.view.canvas = Some([1000.0, 700.0]);
    let r = run(&mut s, "view.zoomFit", json!({}));
    let page = s.doc.page().unwrap().frame();
    let m = cmd::view::FIT_MARGIN;
    let (zoom, pan) = (s.view.zoom, s.view.pan);
    assert_eq!(r["zoom"], json!(zoom));
    assert!(page.width() as f32 * zoom <= 1000.0 - 2.0 * m && page.height() as f32 * zoom <= 700.0 - 2.0 * m, "{zoom}");
    assert!(page.width() as f32 * (zoom + 0.02) > 1000.0 - 2.0 * m, "fits snugly: {zoom}");
    assert_eq!(pan, [page.center().x as f32, page.center().y as f32]);
    assert!(!s.can_undo() && !s.dirty);
    // The drawing only: centred on the shapes, closer in than the whole page.
    run(&mut s, "view.zoomFit", json!({"page": false}));
    assert!(s.view.zoom > zoom);
    assert_ne!(s.view.pan, pan);
    // Limits: a huge page stops at the smallest zoom, a tiny drawing at the largest, and a
    // canvas squeezed to nothing, or not a number, still leaves a finite view.
    s.doc.page_mut().unwrap().width = 1e9;
    run(&mut s, "view.zoomFit", json!({}));
    assert_eq!(s.view.zoom, cmd::view::MIN_ZOOM / 100.0);
    s.doc.page_mut().unwrap().shapes.truncate(1);
    s.doc.page_mut().unwrap().connectors.clear();
    s.view.canvas = Some([1e6, 1e6]);
    run(&mut s, "view.zoomFit", json!({"page": false}));
    assert_eq!(s.view.zoom, cmd::view::MAX_ZOOM / 100.0);
    for canvas in [[0.0, 0.0], [f32::NAN, 10.0], [f32::INFINITY, f32::INFINITY], [-5.0, 1e9]] {
        s.view.canvas = Some(canvas);
        run(&mut s, "view.zoomFit", json!({}));
        assert!(s.view.zoom.is_finite() && s.view.pan.iter().all(|v| v.is_finite()), "{canvas:?}");
    }
}

#[test]
fn zoom_fit_on_an_infinite_board_fits_the_drawing_frame() {
    let mut s = s();
    assert_eq!(s.doc.page().unwrap().board, vizcraft_diagram::Board::Infinite, "the sample is on an infinite board");
    s.doc.page_mut().unwrap().width = 1e9;
    s.view.canvas = Some([1000.0, 700.0]);
    run(&mut s, "view.zoomFit", json!({}));
    let frame = s.doc.page().unwrap().frame();
    assert!(s.view.zoom > cmd::view::MIN_ZOOM / 100.0, "the stored sheet size plays no part: {}", s.view.zoom);
    assert_eq!(s.view.pan, [frame.center().x as f32, frame.center().y as f32]);
    let m = cmd::view::FIT_MARGIN;
    assert!(frame.width() as f32 * s.view.zoom <= 1000.0 - 2.0 * m && frame.height() as f32 * s.view.zoom <= 700.0 - 2.0 * m);
}

#[test]
fn save_and_open_round_trip() {
    let dir = std::env::temp_dir().join(format!("vizcraft-engine-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("template.craft.json");
    let mut s = s();
    run(&mut s, "edit.setTitle", json!({"title": "saved title"}));
    run(&mut s, "file.setAuthor", json!({"name": "Ada"}));
    let r = run(&mut s, "file.save", json!({"path": path.to_string_lossy()}));
    assert_eq!(r["saved"], true);
    assert!(!s.dirty);
    assert_eq!(s.path.as_deref(), Some(path.as_path()));
    assert!(s.can_undo(), "saving keeps the undo history");
    assert_eq!(s.doc.core.last_modified_by, "Ada");
    assert_eq!(s.doc.core.revision, 1);

    let mut t = Session::new(crate::Document::new());
    run(&mut t, "file.open", json!({"path": path.to_string_lossy()}));
    assert_eq!(t.doc, s.doc);
    assert!(!t.dirty);

    // The bytes API alone, and a bare document without the wrapper.
    let bytes = crate::io::save_bytes("x.craft.json", &s.doc).unwrap();
    assert_eq!(crate::io::load_bytes("x.craft.json", &bytes).unwrap(), s.doc);
    let bare = serde_json::to_vec(&s.doc).unwrap();
    assert_eq!(crate::io::load_bytes("bare.json", &bare).unwrap(), s.doc);
    assert!(crate::io::load_bytes("x.txt", &bytes).is_err());
    assert!(crate::io::load_bytes("x.json", b"{not json").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_without_a_path_asks_the_ui() {
    let mut s = s();
    assert_eq!(run(&mut s, "file.save", json!({}))["saved"], false);
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "saveAs"})));
    run(&mut s, "file.open", json!({}));
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "openFile"})));
    run(&mut s, "help.about", json!({}));
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "about"})));
    run(&mut s, "file.close", json!({}));
    assert_eq!(s.ui_requests.pop(), Some(json!({"close": true})));
}

#[test]
fn new_document_clears_history_and_path() {
    let mut s = s();
    run(&mut s, "edit.setTitle", json!({"title": "x"}));
    s.path = Some("somewhere.craft.json".into());
    run(&mut s, "file.new", json!({}));
    assert!(!s.can_undo());
    assert!(!s.dirty);
    assert_eq!(s.path, None);
    assert_eq!(s.doc.title, "");
    run(&mut s, "file.new", json!({"template": "sample"}));
    assert_eq!(s.doc, sample::sample_document());
}

#[test]
fn commands_report_enablement() {
    let s = s();
    let infos = s.commands();
    let undo = infos.iter().find(|c| c.id == "edit.undo").unwrap();
    assert!(!undo.enabled);
    assert_eq!(undo.reason, "nothing to undo");
    let save = infos.iter().find(|c| c.id == "file.save").unwrap();
    assert!(save.enabled);
    assert_eq!(save.shortcut, "Mod+S");
}

#[test]
fn pan_commands_update_view_state() {
    let mut s = s();
    assert_eq!(s.view.pan, [0.0, 0.0]);
    run(&mut s, "view.pan", json!({"dx": 10.0, "dy": -20.0}));
    assert_eq!(s.view.pan, [10.0, -20.0]);
    run(&mut s, "view.pan", json!({"x": 50.0, "y": 60.0}));
    assert_eq!(s.view.pan, [50.0, 60.0]);
    run(&mut s, "view.resetPan", json!({}));
    assert_eq!(s.view.pan, [0.0, 0.0]);
}

#[test]
fn export_and_import_commands() {
    let mut s = s();
    let r = run(&mut s, "file.export", json!({}));
    assert_eq!(r["exported"], false);
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "export"})));

    let r = run(&mut s, "file.export", json!({"path": "/tmp/out.svg"}));
    assert_eq!(r["exported"], true);
    assert_eq!(r["path"], "/tmp/out.svg");

    let r = run(&mut s, "file.import", json!({}));
    assert_eq!(r["imported"], false);
    assert_eq!(s.ui_requests.pop(), Some(json!({"open": "import"})));

    // The real import reads the file; a missing one is an error (file.rs tests cover imports).
    assert!(s.run("file.import", &json!({"path": "/nonexistent/in.svg"})).is_err());
}

#[test]
fn selection_model_methods() {
    let mut sel = crate::Selection::empty();
    assert!(sel.is_empty());
    assert!(!sel.contains("node1"));

    sel.select_single("node1");
    assert!(!sel.is_empty());
    assert_eq!(sel.primary, Some("node1".into()));
    assert!(sel.contains("node1"));

    sel.toggle("node2");
    assert_eq!(sel.items.len(), 2);
    assert!(sel.contains("node2"));

    sel.toggle("node1");
    assert_eq!(sel.items.len(), 1);
    assert!(!sel.contains("node1"));
    assert_eq!(sel.primary, Some("node2".into()));

    sel.clear();
    assert!(sel.is_empty());

    let range_sel = crate::Selection::range(100, 500);
    assert_eq!(range_sel.range, Some((100, 500)));
    assert!(!range_sel.is_empty());
}

// Sample ids: shapes s2 Start, s3 Gather input, s4 Valid?, s5 Save diagram, s6 End, s7 Fix input;
// connectors c8 s2-s3, c9 s3-s4, c10 s4-s5 "Yes", c11 s4-s7 "No", c12 s7-s3, c13 s5-s6.

fn blank() -> Session {
    Session::new(crate::Document::new())
}

fn page(s: &Session) -> &vizcraft_diagram::Page {
    s.doc.page().unwrap()
}

fn shape<'a>(s: &'a Session, id: &str) -> &'a vizcraft_diagram::Shape {
    page(s).shape(id).unwrap()
}

fn conn<'a>(s: &'a Session, id: &str) -> &'a vizcraft_diagram::Connector {
    page(s).connector(id).unwrap()
}

fn params_err(s: &mut Session, id: &str, v: Value) -> String {
    match s.run(id, &v) {
        Err(EngineError::Params(m)) => m,
        other => panic!("{id} {v}: expected a params error, got {other:?}"),
    }
}

#[test]
fn shape_add_places_selects_and_undoes() {
    let mut s = blank();
    let r = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 10, "y": 20, "w": 80, "h": 40, "text": "Hi"}));
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(r["items"], json!([id]));
    let sh = shape(&s, &id);
    assert_eq!((sh.x, sh.y, sh.w, sh.h), (10.0, 20.0, 80.0, 40.0));
    assert_eq!(sh.text, "Hi");
    assert_eq!(sh.stencil, "basic.rectangle");
    assert_eq!(s.sel.primary.as_deref(), Some(id.as_str()));
    assert_eq!(s.undo_label(), Some("Add Shape"));

    // Default size from the master, and the minimum size clamp.
    let r = run(&mut s, "shape.add", json!({"stencil": "flowchart.decision", "x": 0, "y": 0}));
    let d = shape(&s, r["id"].as_str().unwrap());
    let m = vizcraft_stencils::master("flowchart.decision").unwrap();
    assert_eq!((d.w, d.h), (m.w, m.h));
    let r = run(&mut s, "shape.add", json!({"stencil": "basic.ellipse", "x": 0, "y": 0, "w": 0, "h": -5}));
    let e = shape(&s, r["id"].as_str().unwrap());
    assert_eq!((e.w, e.h), (cmd::MIN_SIZE, cmd::MIN_SIZE));

    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert!(page(&s).shapes.is_empty());
    assert!(s.sel.items.is_empty());
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(page(&s).shapes.len(), 1);
    assert_eq!(s.sel.items, std::slice::from_ref(&id));

    assert!(params_err(&mut s, "shape.add", json!({"stencil": "nope", "x": 0, "y": 0})).contains("unknown stencil"));
    params_err(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": "1", "y": 0}));
    params_err(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "y": 0}));
    params_err(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "text": 4}));
}

#[test]
fn shape_move_both_forms_and_undo() {
    let mut s = s();
    let (x0, y0) = (shape(&s, "s2").x, shape(&s, "s2").y);
    run(&mut s, "shape.move", json!({"ids": ["s2", "s3"], "dx": 10, "dy": -5}));
    assert_eq!((shape(&s, "s2").x, shape(&s, "s2").y), (x0 + 10.0, y0 - 5.0));
    assert_eq!(s.undo_label(), Some("Move"));
    run(&mut s, "shape.move", json!({"id": "s2", "x": 1, "y": 2}));
    assert_eq!((shape(&s, "s2").x, shape(&s, "s2").y), (1.0, 2.0));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!((shape(&s, "s2").x, shape(&s, "s2").y), (x0, y0));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(shape(&s, "s2").x, x0 + 10.0);

    // Default targets are the selection; nothing selected is a polite error.
    assert!(params_err(&mut s, "shape.move", json!({"dx": 1, "dy": 1})).contains("select"));
    run(&mut s, "edit.select", json!({"ids": ["s4"]}));
    let x4 = shape(&s, "s4").x;
    run(&mut s, "shape.move", json!({"dx": 3}));
    assert_eq!(shape(&s, "s4").x, x4 + 3.0);
    // Huge deltas clamp instead of overflowing to zero.
    run(&mut s, "shape.move", json!({"ids": ["s4"], "dx": f64::MAX}));
    assert_eq!(shape(&s, "s4").x, vizcraft_diagram::geom::MAX_COORD);
    params_err(&mut s, "shape.move", json!({"ids": ["c99"], "dx": 1}));
    params_err(&mut s, "shape.move", json!({"id": "s2", "x": 1}));
}

#[test]
fn glued_connector_follows_a_moved_shape() {
    let mut s = s();
    let c = conn(&s, "c13").clone();
    let before = vizcraft_diagram::connector_geometry(page(&s), &c).ends.end.pos;
    run(&mut s, "shape.move", json!({"ids": ["s6"], "dx": 40, "dy": 30}));
    let after = vizcraft_diagram::connector_geometry(page(&s), conn(&s, "c13")).ends.end.pos;
    assert_eq!((after.x - before.x, after.y - before.y), (40.0, 30.0));
    // Resize and rotate move the glue point too, with no connector bookkeeping.
    run(&mut s, "shape.resize", json!({"id": "s6", "w": 200, "h": 100}));
    let resized = vizcraft_diagram::connector_geometry(page(&s), conn(&s, "c13")).ends.end.pos;
    assert_ne!(resized, after);
    run(&mut s, "shape.rotate", json!({"ids": ["s6"], "angle": 90}));
    let rotated = vizcraft_diagram::connector_geometry(page(&s), conn(&s, "c13")).ends.end.pos;
    assert_ne!(rotated, resized);
}

#[test]
fn shape_resize_rotate_text_style() {
    let mut s = s();
    let r = run(&mut s, "shape.resize", json!({"id": "s3", "x": 5, "y": 6, "w": 0.1, "h": 1e300}));
    assert_eq!(r["w"], cmd::MIN_SIZE);
    let sh = shape(&s, "s3");
    assert_eq!((sh.x, sh.y, sh.w, sh.h), (5.0, 6.0, cmd::MIN_SIZE, vizcraft_diagram::MAX_EXTENT));
    params_err(&mut s, "shape.resize", json!({"id": "s3", "w": 10}));
    params_err(&mut s, "shape.resize", json!({"id": "c8", "w": 10, "h": 10}));

    run(&mut s, "shape.rotate", json!({"ids": ["s2", "s3"], "angle": -90}));
    assert_eq!(shape(&s, "s2").angle_deg, 270.0);
    assert_eq!(shape(&s, "s3").angle_deg, 270.0);
    params_err(&mut s, "shape.rotate", json!({"ids": ["s2"]}));

    run(&mut s, "shape.setText", json!({"id": "s2", "text": "Begin"}));
    assert_eq!(shape(&s, "s2").text, "Begin");
    assert_eq!(s.undo_label(), Some("Edit Text"));
    params_err(&mut s, "shape.setText", json!({"id": "s2"}));

    run(&mut s, "shape.setStyle", json!({"ids": ["s2", "c8"], "fill": "#ff0000", "line": "#00ff00", "lineWidth": 3, "fontSize": 20, "bold": true}));
    let st = &shape(&s, "s2").style;
    assert_eq!(st.fill, vizcraft_diagram::Color::rgb(255, 0, 0));
    assert_eq!((st.line_width, st.font_size, st.bold, st.italic), (3.0, 20.0, true, false));
    let cs = &conn(&s, "c8").style;
    assert_eq!(cs.line, vizcraft_diagram::Color::rgb(0, 255, 0));
    assert_eq!(cs.fill, vizcraft_diagram::Color::TRANSPARENT, "a connector has no fill");
    run(&mut s, "shape.setStyle", json!({"ids": ["s2"], "lineWidth": 1e9, "fontSize": 0}));
    assert_eq!(shape(&s, "s2").style.line_width, vizcraft_diagram::Style::MAX_LINE_WIDTH);
    let before = s.doc.clone();
    assert!(params_err(&mut s, "shape.setStyle", json!({"ids": ["s2"], "fill": "red", "bold": false})).contains("colour"));
    params_err(&mut s, "shape.setStyle", json!({"ids": ["s2"]}));
    assert_eq!(s.doc, before, "a bad style param changes nothing");
}

#[test]
fn shape_delete_frees_or_removes_glued_connectors() {
    let mut s = s();
    let drawn = vizcraft_diagram::connector_geometry(page(&s), conn(&s, "c13")).ends.end.pos;
    // s6 (End) has one connector, c13 from s5: its end becomes free where it was drawn.
    let r = run(&mut s, "shape.delete", json!({"ids": ["s6"]}));
    assert_eq!(r["deleted"], json!(["s6"]));
    assert_eq!(r["detached"], json!(["c13"]));
    assert_eq!(conn(&s, "c13").end, vizcraft_diagram::End::free(drawn));
    assert!(conn(&s, "c13").start.shape().is_some());
    run(&mut s, "edit.undo", json!({}));
    assert!(page(&s).shape("s6").is_some());
    assert_eq!(conn(&s, "c13").end.shape(), Some("s6"));

    // Deleting both ends of a connector removes it; the selection drops the deleted ids.
    run(&mut s, "edit.select", json!({"ids": ["s5", "s6", "s2"]}));
    let r = run(&mut s, "edit.delete", json!({}));
    let deleted: Vec<&str> = r["deleted"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(deleted.contains(&"c13") && deleted.contains(&"s5") && deleted.contains(&"s6") && deleted.contains(&"s2"));
    assert!(page(&s).connector("c13").is_none());
    assert!(page(&s).connector("c10").unwrap().end.shape().is_none(), "c10 ran from s4 to s5");
    assert!(s.sel.items.is_empty());
    assert_eq!(s.undo_label(), Some("Delete"));
    assert!(matches!(s.run("edit.delete", &json!({})), Err(EngineError::Disabled(_))));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, sample::sample_document());
    assert_eq!(s.sel.items, ["s5", "s6", "s2"]);
    run(&mut s, "edit.redo", json!({}));
    assert!(page(&s).shape("s5").is_none());

    params_err(&mut s, "shape.delete", json!({"ids": ["c8"]}));
    params_err(&mut s, "shape.delete", json!({}));
}

#[test]
fn connector_commands() {
    let mut s = s();
    let r = run(&mut s, "connector.add", json!({"from": {"shape": "s2", "point": 1}, "to": {"x": 900, "y": 40}, "text": "out"}));
    let id = r["id"].as_str().unwrap().to_string();
    let c = conn(&s, &id);
    assert_eq!(c.start, vizcraft_diagram::End::point("s2", 1));
    assert_eq!(c.end, vizcraft_diagram::End::Free { x: 900.0, y: 40.0 });
    assert_eq!(c.route, vizcraft_diagram::Route::Orthogonal);
    assert_eq!(c.end_arrow, vizcraft_diagram::Arrow::Triangle);
    assert_eq!(c.start_arrow, vizcraft_diagram::Arrow::None);
    assert_eq!(c.text, "out");
    assert_eq!(s.sel.items, std::slice::from_ref(&id));
    assert_eq!(s.undo_label(), Some("Add Connector"));

    let r = run(
        &mut s,
        "connector.add",
        json!({"from": {"shape": "s3"}, "to": {"shape": "s7"}, "route": "curved", "startArrow": "circle", "endArrow": "none"}),
    );
    let c2 = conn(&s, r["id"].as_str().unwrap());
    assert_eq!(c2.start, vizcraft_diagram::End::dynamic("s3"));
    assert_eq!(
        (c2.route, c2.start_arrow, c2.end_arrow),
        (vizcraft_diagram::Route::Curved, vizcraft_diagram::Arrow::Circle, vizcraft_diagram::Arrow::None)
    );

    // Glue the free end to a shape, then free the start.
    run(&mut s, "connector.glue", json!({"id": id, "end": "end", "shape": "s7", "point": 0}));
    assert_eq!(conn(&s, &id).end, vizcraft_diagram::End::point("s7", 0));
    run(&mut s, "connector.glue", json!({"id": id, "end": "start", "x": 5, "y": 6}));
    assert_eq!(conn(&s, &id).start, vizcraft_diagram::End::Free { x: 5.0, y: 6.0 });
    assert_eq!(s.undo_label(), Some("Glue Connector"));

    run(&mut s, "connector.setRoute", json!({"ids": ["c8", "c9"], "route": "straight"}));
    assert_eq!(conn(&s, "c9").route, vizcraft_diagram::Route::Straight);
    run(&mut s, "connector.setArrows", json!({"ids": ["c8"], "start": "diamond"}));
    assert_eq!((conn(&s, "c8").start_arrow, conn(&s, "c8").end_arrow), (vizcraft_diagram::Arrow::Diamond, vizcraft_diagram::Arrow::Triangle));
    run(&mut s, "connector.setText", json!({"id": "c8", "text": "go"}));
    assert_eq!(conn(&s, "c8").text, "go");

    // Errors leave the document alone.
    let before = s.doc.clone();
    params_err(&mut s, "connector.add", json!({"from": {"shape": "s2", "point": 4}, "to": {"x": 0, "y": 0}}));
    params_err(&mut s, "connector.add", json!({"from": {"shape": "zz"}, "to": {"x": 0, "y": 0}}));
    params_err(&mut s, "connector.add", json!({"from": 5, "to": {"x": 0, "y": 0}}));
    params_err(&mut s, "connector.add", json!({"from": {"x": 0}, "to": {"x": 0, "y": 0}}));
    params_err(&mut s, "connector.add", json!({"from": {"x": 0, "y": 0}, "to": {"x": 0, "y": 0}, "route": "zigzag"}));
    params_err(&mut s, "connector.glue", json!({"id": "c8", "end": "middle", "x": 0, "y": 0}));
    params_err(&mut s, "connector.glue", json!({"id": "s2", "end": "start", "x": 0, "y": 0}));
    params_err(&mut s, "connector.setRoute", json!({"ids": ["s2"], "route": "straight"}));
    params_err(&mut s, "connector.setArrows", json!({"ids": ["c8"]}));
    params_err(&mut s, "connector.setArrows", json!({"ids": ["c8"], "end": "star"}));
    assert_eq!(s.doc, before);

    // Moving a selected connector moves its free ends only.
    run(&mut s, "shape.move", json!({"ids": [id], "dx": 10, "dy": 10}));
    assert_eq!(conn(&s, &id).start, vizcraft_diagram::End::Free { x: 15.0, y: 16.0 });
    assert_eq!(conn(&s, &id).end, vizcraft_diagram::End::point("s7", 0));
}

#[test]
fn connector_commands_are_disabled_without_connectors() {
    let s = blank();
    let infos = s.commands();
    let route = infos.iter().find(|c| c.id == "connector.setRoute").unwrap();
    assert!(!route.enabled);
    assert_eq!(route.reason, "add a connector first");
    assert!(infos.iter().find(|c| c.id == "connector.add").unwrap().enabled);
    assert!(!infos.iter().find(|c| c.id == "shape.move").unwrap().enabled);
    // Cut and Copy are greyed with nothing selected, like Delete.
    for id in ["edit.copy", "edit.cut"] {
        assert!(matches!(blank().run(id, &json!({})), Err(EngineError::Disabled(m)) if m.contains("select")), "{id}");
    }
    assert_eq!(infos.iter().find(|c| c.id == "edit.delete").unwrap().reason, "select a shape or connector first");
}

#[test]
fn select_commands_are_not_undo_steps() {
    let mut s = s();
    let r = run(&mut s, "edit.select", json!({"ids": ["s2", "c8"]}));
    assert_eq!(r["items"], json!(["s2", "c8"]));
    assert_eq!(r["primary"], "s2");
    run(&mut s, "edit.select", json!({"ids": ["s3", "s2"], "add": true}));
    assert_eq!(s.sel.items, ["s2", "c8", "s3"]);
    assert_eq!(s.sel.primary.as_deref(), Some("s2"));
    // An unknown id is an error and leaves the selection as it was.
    assert!(params_err(&mut s, "edit.select", json!({"ids": ["s2", "zz"]})).contains("zz"));
    assert_eq!(s.sel.items, ["s2", "c8", "s3"]);
    run(&mut s, "edit.selectAll", json!({}));
    assert_eq!(s.sel.items.len(), 12);
    assert_eq!(s.sel.items.first().map(String::as_str), Some("s2"));
    run(&mut s, "edit.selectNone", json!({}));
    assert!(s.sel.is_empty());
    run(&mut s, "edit.select", json!({"ids": []}));
    assert!(!s.can_undo());
    assert!(!s.dirty);
    params_err(&mut s, "edit.select", json!({}));
    params_err(&mut s, "edit.select", json!({"ids": "s2"}));
    let many: Vec<String> = (0..cmd::MAX_IDS + 1).map(|i| format!("s{i}")).collect();
    assert!(params_err(&mut s, "edit.select", json!({"ids": many})).contains("more than"));
    assert!(matches!(blank().run("edit.selectAll", &json!({})), Err(EngineError::Disabled(_))));
}

#[test]
fn copy_and_paste_remap_glue() {
    let mut s = s();
    // Two shapes and the connector between them, plus one whose other end is not copied.
    run(&mut s, "edit.select", json!({"ids": ["s2", "s3", "c8", "c9"]}));
    let r = run(&mut s, "edit.copy", json!({}));
    assert_eq!((r["shapes"].as_u64(), r["connectors"].as_u64()), (Some(2), Some(2)));
    let text = r["text"].as_str().unwrap().to_string();
    assert_eq!(s.clipboard, text);
    let clip: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(clip["vizcraft"], "clip");
    assert!(!s.can_undo(), "copy is not an undo step");

    let r = run(&mut s, "edit.paste", json!({}));
    let pasted: Vec<String> = r["pasted"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(pasted.len(), 4);
    assert_eq!(s.sel.items, pasted);
    assert_eq!(s.undo_label(), Some("Paste"));
    let (a, b, ca, cb) = (&pasted[0], &pasted[1], &pasted[2], &pasted[3]);
    for id in &pasted {
        assert!(!sample::sample_document().contains_id(id), "{id} is fresh");
    }
    assert_eq!(shape(&s, a).x, shape(&s, "s2").x + crate::cmd::edit::PASTE_OFFSET);
    assert_eq!(shape(&s, a).text, "Start");
    // c8 ran s2 -> s3: the copy runs between the two new shapes.
    assert_eq!(conn(&s, ca).start, vizcraft_diagram::End::point(a.clone(), 2));
    assert_eq!(conn(&s, ca).end, vizcraft_diagram::End::point(b.clone(), 0));
    // c9 ran s3 -> s4, and s4 was not copied: the start is glued to the new s3, the end is free,
    // offset from where it was drawn.
    assert_eq!(conn(&s, cb).start.shape(), Some(b.as_str()));
    let drawn = vizcraft_diagram::resolve_ends(page(&s), conn(&s, "c9")).end.pos;
    assert_eq!(
        conn(&s, cb).end,
        vizcraft_diagram::End::Free { x: drawn.x + crate::cmd::edit::PASTE_OFFSET, y: drawn.y + crate::cmd::edit::PASTE_OFFSET }
    );
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, sample::sample_document());

    // Paste from host text into another document.
    let mut t = blank();
    run(&mut t, "edit.paste", json!({"text": text}));
    assert_eq!(page(&t).shapes.len(), 2);
    assert_eq!(page(&t).connectors.len(), 2);

    // Polite errors.
    assert!(params_err(&mut blank(), "edit.paste", json!({})).contains("empty"));
    assert!(params_err(&mut t, "edit.paste", json!({"text": "hello"})).contains("VizCraft"));
    assert!(params_err(&mut t, "edit.paste", json!({"text": "{\"vizcraft\": \"clip\"}"})).contains("nothing"));
    assert!(params_err(&mut t, "edit.paste", json!({"text": "{\"shapes\": []}"})).contains("VizCraft"));
}

#[test]
fn cut_copies_then_deletes() {
    let mut s = s();
    run(&mut s, "edit.select", json!({"ids": ["s7"]}));
    let r = run(&mut s, "edit.cut", json!({}));
    assert!(r["text"].as_str().unwrap().contains("Fix input"));
    assert_eq!(r["deleted"], json!(["s7"]));
    assert!(page(&s).shape("s7").is_none());
    assert_eq!(s.undo_label(), Some("Cut"));
    assert!(s.sel.items.is_empty());
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(page(&s).shapes.len(), 6);
    assert_eq!(shape(&s, &s.sel.items[0]).text, "Fix input");
}

#[test]
fn chosen_ids_let_one_batch_add_and_connect() {
    let mut s = blank();
    run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": 0, "y": 0, "id": "start"}));
    run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": 0, "y": 200, "id": "next"}));
    let r = run(&mut s, "connector.add", json!({"from": {"shape": "start"}, "to": {"shape": "next"}, "id": "link"}));
    assert_eq!(r["id"], "link");
    assert_eq!(conn(&s, "link").id, "link");
    // Taken, empty, too long and odd ids are refused; the counter still skips chosen ids.
    for bad in [json!("start"), json!(""), json!("x".repeat(65)), json!("a b"), json!("p1")] {
        params_err(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "id": bad}));
    }
    params_err(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "id": 7}));
    let r = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    assert!(!["start", "next", "link"].contains(&r["id"].as_str().unwrap()));
}

/// The Cascade 4A done-when list, end to end through commands: a flowchart from the Flowchart pack
/// is a model, an illegal connection and a duplicate tag raise issues live, model and views
/// survive save and reload, and a Cascade 1 document still opens as free drawing.
#[test]
fn flowchart_from_the_pack_is_a_checked_model_that_survives_save() {
    let mut s = Session::new(crate::Document::new());
    // From the Shapes pane path: masters become elements, connectors become flows.
    let start = run(&mut s, "shape.add", json!({"stencil": "flowchart.terminator", "x": 100, "y": 40, "text": "Start", "id": "start"}));
    let check = run(&mut s, "shape.add", json!({"stencil": "flowchart.decision", "x": 100, "y": 160, "text": "Valid?", "id": "check"}));
    let save = run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": 100, "y": 320, "text": "Save", "id": "save"}));
    assert!(start["element"].is_string() && check["element"].is_string() && save["element"].is_string());
    run(&mut s, "connector.add", json!({"from": {"shape": "start"}, "to": {"shape": "check"}, "id": "c1"}));
    run(&mut s, "connector.add", json!({"from": {"shape": "check"}, "to": {"shape": "save"}, "id": "c2", "text": "yes"}));
    // From the agent path: elements and relationships by type.
    let db = run(&mut s, "element.create", json!({"type": "flowchart.Database", "name": "Orders", "x": 400, "y": 320}));
    let db2 = run(&mut s, "element.create", json!({"type": "flowchart.Database", "name": "Archive", "x": 400, "y": 480}));
    run(&mut s, "relationship.create", json!({"from": save["element"], "to": db["id"]}));

    let model = run(&mut s, "model.get", json!({}));
    assert_eq!(model["elements"].as_array().map(Vec::len), Some(5));
    assert_eq!(model["relationships"].as_array().map(Vec::len), Some(3), "{model}");
    assert!(model["relationships"].as_array().unwrap().iter().all(|r| r["type"] == "flowchart.Flow"));
    assert!(s.issues.iter().all(|i| i.severity != vizcraft_model::Severity::Error), "{:?}", s.issues);

    // Illegal connection, live: data does not flow from one store straight to another.
    let r = run(&mut s, "relationship.create", json!({"from": db["id"], "to": db2["id"]}));
    assert_eq!(r["legal"], json!(false));
    let illegal: Vec<_> = s.issues.iter().filter(|i| i.rule == "flowchart.illegalFlow").collect();
    assert_eq!(illegal.len(), 1, "{:?}", s.issues);
    assert!(illegal[0].subjects.iter().any(|x| Some(x.as_str()) == db["id"].as_str()));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.issues.iter().all(|i| i.rule != "flowchart.illegalFlow"));

    // Duplicate tag, live: steps are numbered, and a retag onto a used tag is an error.
    let save_el = save["element"].as_str().unwrap().to_string();
    let tag = s.doc.model.element(&save_el).map(|e| e.tag.clone()).unwrap();
    assert!(tag.starts_with('S'), "{tag}");
    let check_el = check["element"].as_str().unwrap().to_string();
    run(&mut s, "element.retag", json!({"id": check_el, "tag": tag}));
    assert!(s.issues.iter().any(|i| i.rule == "flowchart.duplicateStep" && i.severity == vizcraft_model::Severity::Error), "{:?}", s.issues);

    // Batch rules: the decision has one outgoing flow.
    let batch = run(&mut s, "validate.run", json!({}));
    assert!(batch["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "flowchart.danglingDecision"), "{batch}");

    // Save and reload: the model and its views come back equal.
    let bytes = crate::io::save_bytes("chart.vizcraft", &s.doc).unwrap();
    let back = crate::io::load_bytes("chart.vizcraft", &bytes).unwrap();
    assert_eq!(back.model, s.doc.model);
    assert_eq!(back.pages, s.doc.pages);
    assert_eq!(back.shape("save").and_then(|x| x.element.clone()), Some(save_el));
    let reopened = Session::new(back);
    assert_eq!(reopened.issues, s.issues);
}

/// A version 2 file from Cascade 1 opens as free drawing: empty model, references untouched, no
/// issues, and editing it does not bind its flowchart shapes.
#[test]
fn cascade_1_document_opens_as_free_drawing() {
    let v2 = r##"{"format": "vizcraft", "version": 2, "document": {"title": "Old", "pages": [{"id": "p1", "name": "Page-1",
        "shapes": [{"id": "s1", "stencil": "flowchart.database", "kind": "cylinder", "x": 0, "y": 0, "w": 90, "h": 100, "text": "A"},
                   {"id": "s2", "stencil": "flowchart.database", "kind": "cylinder", "x": 200, "y": 0, "w": 90, "h": 100, "text": "B"}],
        "connectors": [{"id": "c1", "start": {"shape": "s1"}, "end": {"shape": "s2"}}]}], "idSeq": 3}}"##;
    let doc = crate::io::load_bytes("old.vizcraft", v2.as_bytes()).unwrap();
    assert!(doc.model.is_empty());
    let mut s = Session::new(doc);
    assert!(s.issues.is_empty());
    run(&mut s, "shape.move", json!({"ids": ["s1"], "dx": 10, "dy": 0}));
    assert!(s.doc.model.is_empty(), "free shapes are never rebound");
    assert!(s.doc.connector("c1").is_some_and(|c| c.relationship.is_none()));
    assert!(s.issues.is_empty());
}

#[test]
fn batch_undoes_and_redoes_in_one_step() {
    let mut s = s();
    let before = s.doc.clone();
    let r = run(
        &mut s,
        "edit.batch",
        json!({"label": "Build a pair", "commands": [
            {"command": "shape.add", "params": {"stencil": "basic.rectangle", "x": 0, "y": 0, "id": "pa"}},
            {"command": "shape.add", "params": {"stencil": "basic.rectangle", "x": 0, "y": 200, "id": "pb"}},
            {"command": "connector.add", "params": {"from": {"shape": "pa"}, "to": {"shape": "pb"}, "id": "pc"}},
            {"command": "edit.select", "params": {"ids": ["s2"]}},
            {"command": "shape.move", "params": {"dx": 10, "dy": 0}},
        ]}),
    );
    assert_eq!(r["completed"], 5);
    assert_eq!(r["results"][2]["result"]["id"], "pc");
    let after = s.doc.clone();
    assert_eq!(s.undo_label(), Some("Build a pair"));
    let log = s.transactions();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].label, "Build a pair");
    assert_eq!(log[0].origin, crate::Origin::Ui);
    assert_eq!(log[0].commands, ["shape.add", "shape.add", "connector.add", "edit.select", "shape.move"]);

    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before);
    assert!(!s.can_undo(), "the whole batch was one step");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc, after);
    assert!(!s.can_redo());
    assert_eq!(s.transactions().len(), 1, "undo and redo add no log entries");

    // A batch of selection commands changes nothing and makes no step.
    run(&mut s, "edit.batch", json!({"commands": [{"command": "edit.selectAll"}, {"command": "edit.selectNone"}]}));
    assert_eq!(s.undo_label(), Some("Build a pair"));
    // Batches nest, and the outer one is still one step with the default label.
    run(
        &mut s,
        "edit.batch",
        json!({"commands": [{"command": "edit.batch", "params": {"commands": [{"command": "shape.delete", "params": {"ids": ["pa"]}}]}}]}),
    );
    assert_eq!(s.undo_label(), Some("Batch"));
    assert_eq!(s.transactions().back().unwrap().commands, ["edit.batch", "shape.delete"]);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, after);
}

#[test]
fn a_failing_batch_leaves_the_document_unchanged() {
    let mut s = s();
    run(&mut s, "shape.setText", json!({"id": "s2", "text": "First"}));
    run(&mut s, "edit.select", json!({"ids": ["s3"]}));
    let (doc, sel, rev) = (s.doc.clone(), s.sel.clone(), s.rev());
    for (commands, at) in [
        (
            json!([{"command": "shape.move", "params": {"ids": ["s2"], "dx": 50, "dy": 0}}, {"command": "edit.selectAll"}, {"command": "nope.nope"}]),
            2,
        ),
        (json!([{"command": "shape.delete", "params": {"ids": ["s2"]}}, {"command": "edit.undo"}]), 1),
        (json!([{"command": "file.new"}, {"command": "shape.setText", "params": {"id": "s2", "text": "gone"}}]), 1),
        (
            json!([{"command": "shape.add", "params": {"stencil": "basic.rectangle", "x": 0, "y": 0}}, {"command": "edit.batch", "params": {"commands": [{"command": "edit.selectNone"}, {"command": "shape.move", "params": {"ids": ["zz"], "dx": 1, "dy": 1}}]}}]),
            1,
        ),
    ] {
        let e = s.run("edit.batch", &json!({"commands": commands})).unwrap_err().to_string();
        assert!(e.contains(&format!("commands[{at}]")), "{e}");
        assert_eq!(s.doc, doc, "{e}");
        assert_eq!(s.sel, sel, "{e}");
        assert_eq!(s.undo_label(), Some("Edit Text"), "history kept: {e}");
        assert!(s.rev() != rev, "the UI repaints after a rollback");
    }
    assert!(s.path.is_none());
    assert_eq!(s.transactions().len(), 1);
    // The engine API reports the same way.
    let steps = vec![("shape.move".to_string(), json!({"ids": ["s2"], "dx": 1, "dy": 1})), ("nope.nope".to_string(), json!({}))];
    assert!(s.transaction("Move", &steps).is_err());
    assert_eq!(s.doc, doc);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, sample::sample_document());
    // Bad params name the entry; too deep a nest is refused.
    assert!(params_err(&mut s, "edit.batch", json!({"commands": []})).contains("empty"));
    assert!(params_err(&mut s, "edit.batch", json!({"commands": [{"params": {}}]})).contains("commands[0]"));
    assert!(params_err(&mut s, "edit.batch", json!({"commands": [{"command": "edit.selectAll", "params": 3}]})).contains("params"));
    assert!(s.run("edit.batch", &nested_batches(9)).unwrap_err().to_string().contains("deep"));
    assert_eq!(s.doc, sample::sample_document());
}

#[test]
fn the_transaction_log_names_the_origin_and_is_bounded() {
    let mut s = s();
    s.run_as(crate::Origin::Mcp, "edit.batch", &json!({"label": "Agent", "commands": [{"command": "edit.setTitle", "params": {"title": "a"}}]}))
        .unwrap();
    s.run_as(crate::Origin::Control, "edit.setTitle", &json!({"title": "b"})).unwrap();
    run(&mut s, "script.run", json!({"script": [{"command": "edit.setTitle", "params": {"title": "c"}}]}));
    let log: Vec<_> = s.transactions().iter().map(|t| (t.label.as_str(), t.origin)).collect();
    use crate::Origin::*;
    assert_eq!(log, [("Agent", Mcp), ("Document Title", Control), ("Run Script", Script)]);
    assert_eq!(s.origin(), Ui, "run_as and script.run put the origin back");
    for i in 0..crate::session::MAX_LOG + 5 {
        run(&mut s, "edit.setTitle", json!({"title": format!("t{i}")}));
    }
    assert_eq!(s.transactions().len(), crate::session::MAX_LOG);
    assert_eq!(s.transactions().back().unwrap().origin, Ui);
}

#[test]
fn a_fragment_round_trip_keeps_connections_and_gives_fresh_ids() {
    let s = s();
    let ids = ["s2", "s3", "c8"];
    let frag = crate::Fragment::extract(&s.doc, |id| ids.contains(&id));
    assert_eq!((frag.shapes.len(), frag.connectors.len()), (2, 1));
    let back = crate::Fragment::from_json(&serde_json::from_str(&frag.to_json().to_string()).unwrap()).unwrap();
    assert_eq!(back, frag);

    for mut doc in [s.doc.clone(), crate::Document::new()] {
        let old = doc.clone();
        let new = back.insert(&mut doc, 100.0, -50.0).unwrap();
        assert_eq!(new.len(), 3);
        for id in &new {
            assert!(!old.contains_id(id), "{id} is fresh");
        }
        let page = doc.page().unwrap();
        let c = page.connector(&new[2]).unwrap();
        assert_eq!(c.start.shape(), Some(new[0].as_str()), "glue follows the copies");
        assert_eq!(c.end.shape(), Some(new[1].as_str()));
        let (orig, copy) = (sample::sample_document().page().unwrap().shape("s2").unwrap().clone(), page.shape(&new[0]).unwrap());
        assert_eq!((copy.x, copy.y), (orig.x + 100.0, orig.y - 50.0));
    }
    // Parents follow the copies when copied along and are cleared when left behind.
    let mut doc = sample::sample_document();
    if let Some(page) = doc.pages.first_mut() {
        for sh in page.shapes.iter_mut() {
            sh.parent = match sh.id.as_str() {
                "s3" => Some("s2".into()),
                "s2" => Some("gone".into()),
                _ => None,
            };
        }
    }
    let frag = crate::Fragment::extract(&doc, |id| ["s2", "s3"].contains(&id));
    let new = frag.insert(&mut doc, 10.0, 10.0).unwrap();
    let page = doc.page().unwrap();
    assert_eq!(page.shape(&new[0]).unwrap().parent, None);
    assert_eq!(page.shape(&new[1]).unwrap().parent.as_deref(), Some(new[0].as_str()));
    // Hostile offsets and hand-made fragments do not break the page.
    let mut doc = crate::Document::new();
    let ids = back.insert(&mut doc, f64::NAN, f64::INFINITY).unwrap();
    assert!(doc.page().unwrap().shape(&ids[0]).unwrap().x.is_finite());
    assert!(crate::Fragment::from_json(&json!({"shapes": []})).is_err());
    assert!(crate::Fragment::from_json(&json!({"vizcraft": "clip", "shapes": 5})).unwrap().is_empty());
}

#[test]
fn a_recorded_script_holds_no_macro_steps_and_the_undo_depths_follow_the_stacks() {
    let mut s = blank();
    run(&mut s, "script.recordStart", json!({}));
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    // Whether the macro folder exists or not, listing and running macros is not a step.
    let _ = s.run("macro.list", &json!({}));
    let _ = s.run("macro.run", &json!({"name": "nothing"}));
    let script = run(&mut s, "script.recordStop", json!({}))["script"].clone();
    let ids: Vec<&str> = script.as_array().unwrap().iter().filter_map(|st| st["command"].as_str()).collect();
    assert_eq!(ids, ["shape.add"]);
    assert!(!crate::session::recordable("macro.run"));
    assert_eq!((s.undo_depth(), s.redo_depth()), (1, 0));
    assert_eq!(s.can_undo(), s.undo_depth() > 0);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!((s.undo_depth(), s.redo_depth()), (0, 1));
    assert_eq!(s.can_redo(), s.redo_depth() > 0);
}

#[test]
fn page_defaults_follow_the_active_page_and_the_inspector_label() {
    let mut s = blank();
    run(&mut s, "page.add", json!({"name": "Two"}));
    run(&mut s, "page.add", json!({"name": "Three"}));
    run(&mut s, "page.setActive", json!({"page": 2}));
    assert_eq!(s.doc.active_index(), 2);
    let made = run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Here", "x": 10, "y": 10}));
    let shape = made["shape"].as_str().unwrap().to_string();
    assert!(s.doc.pages[2].shape(&shape).is_some(), "an element made with no `page` lands on the active page");
    assert!(s.doc.pages[0].shapes.is_empty());
    assert_eq!(s.sel.items, vec![shape], "and its shape is selected, as it is on the active page");
    run(&mut s, "model.setViewpoint", json!({"viewpoint": null}));
    let label = s.commands().into_iter().find(|c| c.id == "view.shapeDataPane").map(|c| c.label);
    assert_eq!(label.as_deref(), Some("Inspector"));
}

#[test]
fn a_recorded_script_replays_to_the_same_document() {
    let mut s = blank();
    let first = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}))["id"].as_str().unwrap().to_string();
    run(&mut s, "script.recordStart", json!({}));
    assert!(s.is_recording());
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 200}));
    run(&mut s, "view.zoomIn", json!({}));
    let a = run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": 300, "y": 0}))["id"].as_str().unwrap().to_string();
    run(&mut s, "connector.add", json!({"from": {"shape": first}, "to": {"shape": a}}));
    // An undone step and the selection made after it leave the script with it; redo brings
    // the undone step back.
    run(&mut s, "shape.add", json!({"stencil": "basic.ellipse", "x": 600, "y": 0, "id": "gone"}));
    run(&mut s, "edit.select", json!({"ids": ["gone"]}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "shape.setText", json!({"id": first, "text": "kept"}));
    run(&mut s, "shape.setText", json!({"id": first, "text": "kept, then undone"}));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.batch", json!({"commands": [{"command": "edit.selectAll"}, {"command": "edit.copy"}, {"command": "edit.paste"}]}));
    let _ = s.run("shape.move", &json!({"ids": ["zz"], "dx": 1, "dy": 1}));
    let script = run(&mut s, "script.recordStop", json!({}))["script"].clone();
    assert!(!s.is_recording());
    let ids: Vec<&str> = script.as_array().unwrap().iter().map(|x| x["command"].as_str().unwrap()).collect();
    assert_eq!(ids, ["shape.add", "shape.add", "connector.add", "shape.setText", "shape.setText", "edit.batch"]);
    assert!(matches!(s.run("script.recordStop", &json!({})), Err(EngineError::Disabled(_))));

    // Replay on the document as it was when recording started.
    let mut t = blank();
    run(&mut t, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let r = run(&mut t, "script.run", json!({"script": script.to_string()}));
    assert_eq!(r["completed"], 6);
    assert_eq!(t.doc, s.doc);
    assert_eq!(t.undo_label(), Some("Run Script"));
    run(&mut t, "edit.undo", json!({}));
    assert_eq!(page(&t).shapes.len(), 1, "the replay is one undo step");
    // A failing script changes nothing.
    let before = t.doc.clone();
    let bad = json!([{"command": "shape.add", "params": {"stencil": "basic.rectangle", "x": 0, "y": 0}}, {"command": "nope.nope"}]);
    assert!(t.run("script.run", &json!({"script": bad})).unwrap_err().to_string().contains("commands[1]"));
    assert_eq!(t.doc, before);
    assert!(params_err(&mut t, "script.run", json!({})).contains("script"));
    assert!(params_err(&mut t, "script.run", json!({"script": "[]"})).contains("empty"));
}
