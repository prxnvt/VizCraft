//! Tests of the process commands and the process sync pass: lines through valves and at
//! branches, live tags, loops, numbering, off-page pairs, nozzles, lists, isometric runs and
//! loop diagrams.

use serde_json::{Value, json};
use vizcraft_diagram::{Document, End};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id} {v}: {e}"))
}

/// A command that must be refused with a params error and leave the document alone.
fn refused(s: &mut Session, id: &str, v: Value) {
    let before = s.doc.clone();
    let e = s.run(id, &v).expect_err(&format!("{id} {v} should be refused"));
    assert!(matches!(e, crate::EngineError::Params(_)), "{id} {v}: {e}");
    assert_eq!(s.doc, before, "{id} {v} changed the document");
}

/// A command that must be exactly one undo step.
fn one_step(s: &mut Session, id: &str, v: Value) -> Value {
    let before = s.doc.clone();
    let r = run(s, id, v.clone());
    assert_ne!(s.doc, before, "{id} {v} changed nothing");
    let after = s.doc.clone();
    run(s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "{id} {v}: one undo takes it back");
    run(s, "edit.redo", json!({}));
    assert_eq!(s.doc, after, "{id} {v}: redo");
    r
}

/// An empty P&ID.
fn pid() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "process.pid"}));
    s
}

fn tag(s: &Session, id: &str) -> String {
    s.doc.model.element(id).map(|e| e.tag.clone()).unwrap_or_default()
}

fn line_of(s: &Session, connector: &str) -> Option<String> {
    s.doc.page().and_then(|p| p.connector(connector)).and_then(|c| c.ext.get(vizcraft_notation::process_pass::EXT_LINE).cloned())
}

/// A P&ID with a vessel, a nozzle on its right and a process line from the nozzle to the right.
fn vessel_and_pipe() -> (Session, String, String, String) {
    let mut s = pid();
    let unit = run(&mut s, "element.create", json!({"type": "process.Unit", "name": "Unit 1"}))["id"].as_str().unwrap().to_string();
    let v = run(&mut s, "element.create", json!({"type": "process.Vessel", "owner": unit, "x": 100, "y": 100}));
    let vessel = v["shape"].as_str().unwrap().to_string();
    let n = run(&mut s, "plant.addNozzle", json!({"shape": vessel, "side": "right"}));
    let nozzle = n["shape"].as_str().unwrap().to_string();
    let w = run(&mut s, "wire.draw", json!({"from": {"shape": nozzle, "point": 0}, "to": {"x": 600, "y": 160}, "type": "process.processLine"}));
    (s, vessel, nozzle, w["id"].as_str().unwrap().to_string())
}

#[test]
fn a_nozzle_takes_the_equipment_tag_and_its_sub_tag() {
    let (s, vessel, nozzle, _) = vessel_and_pipe();
    let page = s.doc.page().unwrap();
    let ve = page.shape(&vessel).unwrap().element.clone().unwrap();
    assert_eq!(tag(&s, &ve), "V-101", "class V, unit 1, first vessel");
    let ne = page.shape(&nozzle).unwrap().element.clone().unwrap();
    assert_eq!(tag(&s, &ne), "V-101-N1");
    assert_eq!(page.shape(&nozzle).unwrap().parent.as_deref(), Some(vessel.as_str()), "the nozzle moves with the vessel");
    let mut s = s;
    let second = one_step(&mut s, "plant.addNozzle", json!({"shape": vessel, "side": "top", "nominalDiameter": "2in"}));
    assert!(second["tag"].as_str().unwrap().ends_with("-N2") || tag(&s, second["id"].as_str().unwrap()) == "V-101-N2");
    refused(&mut s, "plant.addNozzle", json!({"shape": vessel, "side": "sideways"}));
    refused(&mut s, "plant.addNozzle", json!({"shape": nozzle, "side": "left"}));
    refused(&mut s, "plant.addNozzle", json!({"shape": vessel, "side": "left", "at": 2}));
    refused(&mut s, "plant.addNozzle", json!({"shape": vessel, "side": "left", "subTag": "N1"}));
}

#[test]
fn a_pipe_starts_a_line_whose_number_follows_its_size() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    let line = line_of(&s, &pipe).expect("the pipe names a line");
    // wire.draw {type}: the connector is drawn as the kind, its net takes the kind's net kind.
    let c = s.doc.page().unwrap().connector(&pipe).unwrap().clone();
    assert_eq!(c.kind, "process.processLine");
    assert_eq!(s.doc.model.net(c.net.as_deref().unwrap()).unwrap().kind, vizcraft_model::NetKind::Pipe);
    refused(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 50, "y": 0}, "type": "nope.kind"}));
    assert_eq!(s.doc.model.element(&line).unwrap().type_id, "process.PipingNetworkSystem");
    assert_eq!(tag(&s, &line), "--001-");
    assert_eq!(s.doc.page().unwrap().connector(&pipe).unwrap().text, "", "no number shows until the line is complete");
    for (k, v) in [("nominalDiameter", "2in"), ("fluidCode", "P"), ("pipingClass", "A1")] {
        run(&mut s, "element.setProperty", json!({"id": line, "key": k, "value": v}));
    }
    assert_eq!(tag(&s, &line), "2in-P-001-A1");
    assert_eq!(s.doc.page().unwrap().connector(&pipe).unwrap().text, "2in-P-001-A1", "the number is written on the line");
    // size_edit_recomposes_line: the number stays, the size changes.
    run(&mut s, "element.setProperty", json!({"id": line, "key": "nominalDiameter", "value": "3in"}));
    assert_eq!(tag(&s, &line), "3in-P-001-A1");
    run(&mut s, "element.setProperty", json!({"id": line, "key": "label", "value": "off"}));
    assert_eq!(s.doc.page().unwrap().connector(&pipe).unwrap().text, "");
}

#[test]
fn splice_keeps_line_and_branch_gets_new_line() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    let line = line_of(&s, &pipe).unwrap();
    let valve = run(&mut s, "element.create", json!({"type": "process.GlobeValve", "x": 330, "y": 140}))["shape"].as_str().unwrap().to_string();
    let r = run(&mut s, "wire.splice", json!({"shape": valve, "connector": pipe}));
    let halves: Vec<String> = r["connectors"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
    assert_eq!(halves.len(), 2);
    for h in &halves {
        assert_eq!(line_of(&s, h).as_deref(), Some(line.as_str()), "both halves keep the line through the valve");
    }
    // A pipe drawn from the valve's other port inherits the line too.
    let tail = halves[1].clone();
    let end = s.doc.page().unwrap().connector(&tail).unwrap().end.clone();
    let at = match end {
        End::Free { x, y } => (x, y),
        End::Glued { .. } => panic!("the tail keeps the free end"),
    };
    // A branch from the middle of the tail gets a new line.
    let b = run(
        &mut s,
        "wire.draw",
        json!({"from": {"connector": tail, "x": at.0 - 40.0, "y": at.1}, "to": {"x": at.0 - 40.0, "y": at.1 + 200.0}, "type": "process.processLine"}),
    );
    let branch = b["id"].as_str().unwrap();
    let bl = line_of(&s, branch).unwrap();
    assert_ne!(bl, line, "a branch starts a new sequence");
    assert_eq!(tag(&s, &bl), "--002-");
    // Deleting the branch removes its line.
    run(&mut s, "edit.select", json!({"ids": [branch]}));
    run(&mut s, "edit.delete", json!({}));
    assert!(s.doc.model.element(&bl).is_none(), "the line went with its last segment");
}

#[test]
fn loops_number_their_instruments_and_renumber_them_at_once() {
    let mut s = pid();
    let unit = run(&mut s, "element.create", json!({"type": "process.Unit"}))["id"].as_str().unwrap().to_string();
    let lt =
        run(&mut s, "element.create", json!({"type": "process.ProcessInstrumentationFunction", "properties": {"letters": "LT"}, "x": 100, "y": 300}));
    let lic = run(
        &mut s,
        "element.create",
        json!({"type": "process.ProcessInstrumentationFunction", "properties": {"letters": "LIC", "location": "panelFront"}, "x": 200, "y": 300}),
    );
    let (lt, lic) = (lt["id"].as_str().unwrap().to_string(), lic["id"].as_str().unwrap().to_string());
    assert_eq!(tag(&s, &lt), "LT-");
    let r = one_step(&mut s, "loop.create", json!({"members": [lt, lic], "unit": unit}));
    let lp = r["id"].as_str().unwrap().to_string();
    assert_eq!(tag(&s, &lp), "101");
    assert_eq!((tag(&s, &lt), tag(&s, &lic)), ("LT-101".to_string(), "LIC-101".to_string()));
    // The bubble draws the tag split: letters above, number below.
    let shape = s.doc.page().unwrap().shapes.iter().find(|sh| sh.element.as_deref() == Some(lic.as_str())).unwrap().clone();
    let texts: Vec<String> = shape.glyph.as_ref().unwrap().labels.iter().map(|l| l.text.clone()).collect();
    assert_eq!(texts, ["LIC", "101"]);
    // loop_renumber_retags_instruments
    run(&mut s, "element.retag", json!({"id": lp, "tag": "105"}));
    assert_eq!((tag(&s, &lt), tag(&s, &lic)), ("LT-105".to_string(), "LIC-105".to_string()));
    let shape = s.doc.page().unwrap().shapes.iter().find(|sh| sh.element.as_deref() == Some(lic.as_str())).unwrap().clone();
    assert_eq!(shape.glyph.as_ref().unwrap().labels.iter().map(|l| l.text.clone()).collect::<Vec<_>>(), ["LIC", "105"]);
    // A letters edit retags at once.
    run(&mut s, "element.setProperty", json!({"id": lic, "key": "letters", "value": "LC"}));
    assert_eq!(tag(&s, &lic), "LC-105");
    // Taking an instrument out of its loop leaves only its letters.
    one_step(&mut s, "loop.assign", json!({"loop": null, "elements": [lt]}));
    assert_eq!(tag(&s, &lt), "LT-");
    refused(&mut s, "loop.create", json!({"variable": "lower"}));
    refused(&mut s, "loop.create", json!({"variable": "L", "members": [unit]}));
    refused(&mut s, "loop.create", json!({"variable": "L", "loopType": "fancy"}));
    refused(&mut s, "loop.assign", json!({"loop": lt, "elements": [lic]}));
    refused(&mut s, "loop.assign", json!({"loop": lp, "elements": []}));
}

#[test]
fn parallel_numbering_allows_f101_and_l101_and_serial_flags_them() {
    let mut s = pid();
    let unit = run(&mut s, "element.create", json!({"type": "process.Unit"}))["id"].as_str().unwrap().to_string();
    let f = run(&mut s, "loop.create", json!({"variable": "F", "unit": unit}))["id"].as_str().unwrap().to_string();
    let l = run(&mut s, "loop.create", json!({"variable": "L", "unit": unit}))["id"].as_str().unwrap().to_string();
    assert_eq!((tag(&s, &f), tag(&s, &l)), ("101".to_string(), "101".to_string()), "parallel: each variable counts from the start");
    let dup = |s: &Session| s.issues.iter().any(|i| i.rule == "process.duplicateLoop");
    assert!(!dup(&s));
    assert_eq!(run(&mut s, "plant.numbering", json!({}))["scheme"], "parallel");
    one_step(&mut s, "plant.numbering", json!({"scheme": "serial"}));
    assert_eq!(run(&mut s, "plant.numbering", json!({}))["scheme"], "serial");
    assert!(dup(&s), "serial numbering reports the shared number");
    run(&mut s, "plant.numbering", json!({"scheme": "parallel"}));
    assert!(s.doc.model.id_formats.is_empty(), "back to the pack's own numbering");
    refused(&mut s, "plant.numbering", json!({"scheme": "mixed"}));
}

#[test]
fn identity_formats_read_and_set_the_project_format() {
    let mut s = pid();
    let all = run(&mut s, "identity.formats", json!({"pack": "process"}));
    let line = all["scopes"].as_array().unwrap().iter().find(|x| x["id"] == "process.line").unwrap().clone();
    assert_eq!(line["format"], "{nominalDiameter}-{fluidCode}-{n:3}-{pipingClass}");
    one_step(&mut s, "identity.setFormat", json!({"scope": "process.line", "format": "{fluidCode}-{n:4}"}));
    let all = run(&mut s, "identity.formats", json!({"pack": "process"}));
    let line = all["scopes"].as_array().unwrap().iter().find(|x| x["id"] == "process.line").unwrap().clone();
    assert_eq!((line["format"].as_str(), line["overridden"].as_bool()), (Some("{fluidCode}-{n:4}"), Some(true)));
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 200, "y": 0}, "type": "process.processLine"}));
    let l = line_of(&s, w["id"].as_str().unwrap()).unwrap();
    assert_eq!(tag(&s, &l), "-0001");
    run(&mut s, "identity.setFormat", json!({"scope": "process.line", "format": null}));
    assert!(s.doc.model.id_formats.is_empty());
    refused(&mut s, "identity.setFormat", json!({"scope": "process.line", "format": "{fluidCode}"}));
    refused(&mut s, "identity.setFormat", json!({"scope": "process.line", "format": "{oops"}));
    refused(&mut s, "identity.setFormat", json!({"scope": "nope.scope", "format": "{n}"}));
    refused(&mut s, "identity.setFormat", json!({"scope": "process.line"}));
    refused(&mut s, "identity.setFormat", json!({"scope": "process.loop", "numberBy": "not a key"}));
}

#[test]
fn the_pass_is_idempotent_and_an_old_document_stays_byte_identical() {
    let (s, _, _, _) = vessel_and_pipe();
    let mut doc = s.doc.clone();
    let before = doc.clone();
    vizcraft_notation::bind::sync(&mut doc, &before, vizcraft_notation::catalog());
    assert_eq!(doc, before, "a second run changes nothing");
    let old = crate::sample::sample_document();
    let bytes = crate::io::save_bytes("a.vizcraft", &old).unwrap();
    let mut loaded = crate::io::load_bytes("a.vizcraft", &bytes).unwrap();
    let was = loaded.clone();
    vizcraft_notation::bind::sync(&mut loaded, &was, vizcraft_notation::catalog());
    assert_eq!(crate::io::save_bytes("a.vizcraft", &loaded).unwrap(), bytes);
}

#[test]
fn switching_the_variant_keeps_every_frame_in_one_step() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    run(&mut s, "element.create", json!({"type": "process.ProcessInstrumentationFunction", "properties": {"letters": "LIC"}, "x": 300, "y": 400}));
    let valve =
        run(&mut s, "element.create", json!({"type": "process.GlobeValve", "properties": {"actuator": "diaphragm"}, "x": 330, "y": 140}))["shape"]
            .as_str()
            .unwrap()
            .to_string();
    run(&mut s, "wire.splice", json!({"shape": valve, "connector": pipe}));
    let frames = |s: &Session| -> Vec<(String, i64, i64, i64, i64)> {
        s.doc
            .page()
            .unwrap()
            .shapes
            .iter()
            .map(|sh| {
                (
                    sh.id.clone(),
                    (sh.x * 100.0).round() as i64,
                    (sh.y * 100.0).round() as i64,
                    (sh.w * 100.0).round() as i64,
                    (sh.h * 100.0).round() as i64,
                )
            })
            .collect()
    };
    let was = frames(&s);
    for variant in ["process.isa", "process.iso14617", "process.iso"] {
        let r = one_step(&mut s, "variant.set", json!({"variant": variant}));
        assert_eq!(frames(&s), was, "{variant}: {r}");
    }
    run(&mut s, "variant.set", json!({"variant": "process.isa"}));
    let bubble = s.doc.page().unwrap().shapes.iter().find(|sh| sh.glyph.as_ref().is_some_and(|g| g.builder == "bubble")).unwrap();
    assert_eq!(bubble.stencil, "process.instrumentIsa");
}

#[test]
fn off_page_connectors_pair_across_sheets() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    let first = s.doc.page().unwrap().id.clone();
    run(&mut s, "page.add", json!({"name": "Sheet 2"}));
    let second = s.doc.page().unwrap().id.clone();
    run(&mut s, "page.setActive", json!({"page": first}));
    let r = one_step(&mut s, "offpage.place", json!({"connector": pipe, "end": "end", "toPage": second}));
    let (a, b) = (r["a"].as_str().unwrap().to_string(), r["b"].as_str().unwrap().to_string());
    assert_eq!(tag(&s, &a), "1");
    assert_eq!(tag(&s, &b), "1");
    assert!(s.doc.model.relationships.iter().any(|x| x.type_id == "process.Continues" && x.source == a && x.target == b));
    assert!(!s.issues.iter().any(|i| i.rule == "process.unpairedOffPage"));
    // Deleting one leaves the other unpaired.
    let sb = r["shapes"][1].as_str().unwrap().to_string();
    run(&mut s, "page.setActive", json!({"page": second}));
    run(&mut s, "shape.delete", json!({"ids": [sb]}));
    run(&mut s, "model.delete", json!({"ids": [b]}));
    assert!(s.issues.iter().any(|i| i.rule == "process.unpairedOffPage"));
    refused(&mut s, "offpage.pair", json!({"a": a, "b": a}));
    refused(&mut s, "offpage.place", json!({"connector": "nope", "end": "end", "toPage": first}));
}

#[test]
fn lists_read_the_model_and_export() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    let line = line_of(&s, &pipe).unwrap();
    for (k, v) in [("nominalDiameter", "2in"), ("fluidCode", "P"), ("pipingClass", "A1"), ("designPressure", "10")] {
        run(&mut s, "element.setProperty", json!({"id": line, "key": k, "value": v}));
    }
    let before = s.doc.clone();
    let r = run(&mut s, "plant.list", json!({"list": "line"}));
    assert_eq!(s.doc, before, "a list changes nothing");
    assert_eq!(r["count"], 1);
    assert_eq!(r["rows"][0][0], "2in-P-001-A1");
    assert_eq!(r["rows"][0][4], "V-101-N1", "From: the nozzle the line starts at");
    let csv = run(&mut s, "plant.list", json!({"list": "line", "format": "csv"}))["content"].as_str().unwrap().to_string();
    let golden = format!("{}/testdata/process/line-list.csv", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("VIZCRAFT_BLESS").is_ok() {
        std::fs::create_dir_all(format!("{}/testdata/process", env!("CARGO_MANIFEST_DIR"))).unwrap();
        std::fs::write(&golden, &csv).unwrap();
    }
    assert_eq!(csv, std::fs::read_to_string(&golden).unwrap_or_default(), "the line list CSV changed");
    let eq = run(&mut s, "plant.list", json!({"list": "equipment"}));
    assert_eq!(eq["rows"][0][0], "V-101");
    assert_eq!(run(&mut s, "plant.list", json!({"list": "nozzle"}))["rows"][0][0], "V-101-N1");
    for l in super::LISTS {
        run(&mut s, "plant.list", json!({"list": l}));
    }
    // The same list through table.export.
    let t = run(&mut s, "table.export", json!({"view": "pack", "list": "process.line", "format": "csv"}));
    assert_eq!(t["content"].as_str().unwrap(), csv);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = std::env::temp_dir().join(format!("vizcraft-plant-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lines.xlsx");
        let r = run(&mut s, "plant.list", json!({"list": "line", "format": "xlsx", "path": path.to_string_lossy()}));
        assert!(r["bytes"].as_u64().unwrap() > 0);
        let back = vizcraft_formats::table::read(&std::fs::read(&path).unwrap(), vizcraft_diagram::data_graphic::TableFormat::Xlsx, "").unwrap();
        assert_eq!(back.rows.first().map(|r| r.first().cloned()), Some(Some("2in-P-001-A1".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }
    refused(&mut s, "plant.list", json!({"list": "nope"}));
    refused(&mut s, "plant.list", json!({"list": "line", "format": "xlsx"}));
}

#[test]
fn trace_and_loop_diagram() {
    let mut s = pid();
    let unit = run(&mut s, "element.create", json!({"type": "process.Unit"}))["id"].as_str().unwrap().to_string();
    let mk = |s: &mut Session, ty: &str, letters: &str, x: f64| -> (String, String) {
        let r = run(s, "element.create", json!({"type": ty, "properties": {"letters": letters}, "x": x, "y": 300}));
        (r["id"].as_str().unwrap().to_string(), r["shape"].as_str().unwrap().to_string())
    };
    let (lt, slt) = mk(&mut s, "process.ProcessInstrumentationFunction", "LT", 100.0);
    let (lic, slic) = mk(&mut s, "process.ProcessInstrumentationFunction", "LIC", 300.0);
    let (lv, slv) = mk(&mut s, "process.GlobeValve", "LV", 500.0);
    run(&mut s, "element.setProperty", json!({"id": lic, "key": "location", "value": "panelFront"}));
    run(&mut s, "loop.create", json!({"members": [lt, lic, lv], "unit": unit}));
    let t = run(&mut s, "loop.trace", json!({"element": lic}));
    assert_eq!(t["complete"], false, "no signal lines yet");
    run(&mut s, "connector.add", json!({"from": {"shape": slt}, "to": {"shape": slic}, "kind": "process.electricSignal"}));
    run(&mut s, "connector.add", json!({"from": {"shape": slic}, "to": {"shape": slv}, "kind": "process.electricSignal"}));
    let before = s.doc.clone();
    let t = run(&mut s, "loop.trace", json!({"element": lic}));
    assert_eq!(s.doc, before, "a trace changes nothing");
    assert_eq!(t["complete"], true, "{t}");
    let roles: Vec<&str> = t["chain"].as_array().unwrap().iter().map(|c| c["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["transmitter", "controller", "final"]);
    assert_eq!(t["shapes"].as_array().unwrap().len(), 3);
    let loops = run(&mut s, "loop.list", json!({}));
    assert_eq!(loops["loops"][0]["tag"], "101");
    let pages = s.doc.pages.len();
    let d = one_step(&mut s, "loop.diagram", json!({"loop": lic}));
    assert_eq!(s.doc.pages.len(), pages + 1);
    let page = s.doc.page().unwrap();
    assert_eq!(page.name, "Loop 101");
    assert_eq!(page.view.as_ref().unwrap().viewpoint, "process.Loop");
    assert_eq!(d["channels"].as_array().unwrap().len(), 2, "the transmitter and the valve each take a channel");
    let sheet_issues: Vec<&vizcraft_model::Issue> = s.issues.iter().filter(|i| i.rule == "process.oneLoopPerSheet").collect();
    assert!(sheet_issues.is_empty(), "{sheet_issues:?}");
    refused(&mut s, "loop.trace", json!({"element": unit}));
    refused(&mut s, "loop.diagram", json!({"loop": unit}));
}

#[test]
fn an_isometric_run_draws_legs_with_elbows_on_one_line() {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "process.isometric"}));
    let r = one_step(
        &mut s,
        "plant.isoRun",
        json!({"from": {"x": 200, "y": 400}, "legs": [{"dir": "n", "length": 200}, {"dir": "up", "length": 100}, {"dir": "e", "length": 150}]}),
    );
    assert_eq!(r["connectors"].as_array().unwrap().len(), 3);
    assert_eq!(r["elbows"].as_array().unwrap().len(), 2);
    let line = r["line"].as_str().unwrap();
    for c in r["connectors"].as_array().unwrap() {
        assert_eq!(line_of(&s, c.as_str().unwrap()).as_deref(), Some(line));
    }
    let end = (r["end"]["x"].as_f64().unwrap(), r["end"]["y"].as_f64().unwrap());
    let c30 = 30f64.to_radians().cos();
    assert!((end.0 - (200.0 + 350.0 * c30)).abs() < 1e-6 && (end.1 - (400.0 - 100.0 - 100.0 + 75.0)).abs() < 1e-6, "{end:?}");
    refused(&mut s, "plant.isoRun", json!({"from": {"x": 0, "y": 0}, "legs": [{"dir": "nw", "length": 10}]}));
    refused(&mut s, "plant.isoRun", json!({"from": {"x": 0, "y": 0}, "legs": []}));
    refused(&mut s, "plant.isoRun", json!({"from": {"x": 0, "y": 0}, "legs": [{"dir": "n", "length": 10}], "kind": "process.electricSignal"}));
}

#[test]
fn line_assign_and_the_dexpi_map() {
    let (mut s, _, _, pipe) = vessel_and_pipe();
    let old = line_of(&s, &pipe).unwrap();
    let r = one_step(&mut s, "line.assign", json!({"connectors": [pipe]}));
    let new = r["line"].as_str().unwrap().to_string();
    assert_ne!(new, old);
    assert!(s.doc.model.element(&old).is_none(), "the old line lost its last segment");
    refused(&mut s, "line.assign", json!({"connectors": ["nope"]}));
    let m = run(&mut s, "plant.dexpiMap", json!({"type": "process.Pump"}));
    assert_eq!(m["mappings"][0]["dexpiClass"], "Pump");
    let all = run(&mut s, "plant.dexpiMap", json!({}));
    assert!(all["unmapped"].as_array().unwrap().iter().any(|t| t == "process.Weld"));
    refused(&mut s, "plant.dexpiMap", json!({"type": "flowchart.Process"}));
}
