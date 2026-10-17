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
        assert!(["file", "edit", "view", "insert", "help"].contains(&prefix), "{}: unexpected prefix", c.id);
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
    ];
    for spec in reg.all() {
        // File commands touch the disk; given a path they would write junk files.
        if spec.id.starts_with("file.") {
            continue;
        }
        for j in &junk {
            let mut s = s();
            let _ = s.run(spec.id, j);
        }
    }
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
fn clipboard_template_errors() {
    let mut s = s();
    assert!(matches!(s.run("edit.cut", &json!({})), Err(EngineError::Failed(_))));
    assert!(matches!(s.run("edit.copy", &json!({})), Err(EngineError::Failed(_))));
    assert!(matches!(s.run("edit.paste", &json!({})), Err(EngineError::Failed(_))));
    assert!(matches!(s.run("edit.selectAll", &json!({})), Err(EngineError::Failed(_))));
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

    let r = run(&mut s, "file.import", json!({"path": "/tmp/in.svg"}));
    assert_eq!(r["imported"], true);
    assert_eq!(r["path"], "/tmp/in.svg");
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
