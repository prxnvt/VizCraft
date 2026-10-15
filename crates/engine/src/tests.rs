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
    assert!(matches!(s.run("edit.copy", &json!({})), Err(EngineError::Disabled(_))), "copy with nothing selected");
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
            assert!(!s.doc.notes.is_empty(), "{}: emptied the document", spec.id);
            assert!(s.sel.note < s.doc.notes.len(), "{}: selection off the end", spec.id);
        }
    }
}

#[test]
fn a_panicking_command_restores_the_document() {
    let mut s = s();
    let mut specs = s.registry.all().to_vec();
    specs.push(crate::CommandSpec::new("edit.boom", "Boom", "Tests", |s, _| {
        s.doc.notes.clear();
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
    run(&mut s, "edit.setText", json!({"text": "hello", "note": 0}));
    assert!(s.dirty);
    assert!(s.rev() > rev0);
    assert_eq!(s.undo_label(), Some("Typing"));
    run(&mut s, "insert.note", json!({"title": "New"}));
    assert_eq!(s.doc.notes.len(), 4);
    assert_eq!(s.undo_label(), Some("Note"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.notes.len(), 3);
    assert_eq!(s.redo_label(), Some("Note"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, original);
    assert!(!s.can_undo());
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.notes.len(), 4);
    assert_eq!(s.doc.notes[0].body, "hello");
    assert!(!s.can_redo());
}

#[test]
fn typing_coalesces_but_other_commands_split_it() {
    let mut s = s();
    run(&mut s, "edit.setText", json!({"text": "a"}));
    run(&mut s, "edit.setText", json!({"text": "ab"}));
    run(&mut s, "edit.setText", json!({"text": "abc"}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.notes[0].body, sample::sample_document().notes[0].body, "three keystrokes are one step");
    run(&mut s, "edit.setText", json!({"text": "x"}));
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "edit.setText", json!({"text": "xy"}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.notes[0].body, "x", "a selection command closes the typing group");
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
fn clipboard_edits_the_selection() {
    let mut s = s();
    run(&mut s, "edit.setText", json!({"text": "hello world"}));
    run(&mut s, "edit.select", json!({"start": 0, "end": 5}));
    assert_eq!(run(&mut s, "edit.copy", json!({}))["text"], "hello");
    run(&mut s, "edit.cut", json!({}));
    assert_eq!(s.doc.notes[0].body, " world");
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(s.doc.notes[0].body, "hello world");
    run(&mut s, "edit.selectAll", json!({}));
    run(&mut s, "edit.paste", json!({"text": "bye"}));
    assert_eq!(s.doc.notes[0].body, "bye");
    assert_eq!(s.sel.end, 3);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.notes[0].body, "hello world");
}

#[test]
fn save_and_open_round_trip() {
    let dir = std::env::temp_dir().join(format!("protocraft-engine-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.craft.json");
    let mut s = s();
    run(&mut s, "edit.setText", json!({"text": "saved text"}));
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
    run(&mut s, "edit.setText", json!({"text": "x"}));
    s.path = Some("somewhere.craft.json".into());
    run(&mut s, "file.new", json!({}));
    assert!(!s.can_undo());
    assert!(!s.dirty);
    assert_eq!(s.path, None);
    assert_eq!(s.doc.notes.len(), 1);
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
