//! Tests of the fluid power commands: each `fluid.*` command with undo, the examples, the
//! editions, the codes, the trace and the power unit.

use serde_json::{Value, json};
use vizcraft_diagram::Document;

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The fluid power issues of the document, as `rule: message`.
fn fluid_issues(s: &Session) -> Vec<String> {
    s.issues.iter().filter(|i| i.rule.starts_with("fluidpower.")).map(|i| format!("{}: {}", i.rule, i.message)).collect()
}

fn example(id: &str) -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "example.open", json!({"example": id}));
    s
}

#[test]
fn the_examples_open_clean_with_their_codes() {
    let s = example("fluidpower.hydraulicExample");
    assert!(fluid_issues(&s).is_empty(), "{:#?}", fluid_issues(&s));
    let mut tags: Vec<&str> = s.doc.model.elements.iter().map(|e| e.tag.as_str()).filter(|t| !t.is_empty()).collect();
    tags.sort_unstable();
    assert_eq!(tags, ["0M1", "0P1", "0S1", "0V1", "0Z1", "0Z2", "1A1", "1V1"]);
    let s = example("fluidpower.pneumaticExample");
    assert!(fluid_issues(&s).is_empty(), "{:#?}", fluid_issues(&s));
    let s = example("fluidpower.powerUnitExample");
    assert!(fluid_issues(&s).is_empty(), "{:#?}", fluid_issues(&s));
}

/// A hydraulic page with a pump, a 4/3 valve and a cylinder, nothing joined.
fn page() -> (Session, Value) {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "fluidpower.hydraulicCircuit"}));
    let pump = run(&mut s, "fluid.insert", json!({"type": "fluidpower.HydraulicPump", "x": 100, "y": 400}));
    let valve = run(&mut s, "fluid.insert", json!({"type": "fluidpower.HydraulicValve", "x": 300, "y": 250}));
    let cyl = run(&mut s, "fluid.insert", json!({"type": "fluidpower.HydraulicCylinder", "x": 320, "y": 100, "params": {"tag": "x"}}));
    (s, json!({"pump": pump, "valve": valve, "cyl": cyl}))
}

fn rules(s: &Session) -> Vec<String> {
    s.issues.iter().filter(|i| i.rule.starts_with("fluidpower.")).map(|i| i.rule.clone()).collect()
}

#[test]
fn insert_draws_a_built_symbol_in_one_undo_step_and_the_checks_see_it() {
    let (mut s, ids) = page();
    let sh = ids["valve"]["shape"].as_str().unwrap();
    let shape = s.doc.page().unwrap().shape(sh).unwrap();
    assert_eq!(shape.glyph.as_ref().unwrap().ports, ["P", "A", "B", "T", "el", "er"]);
    let cyl = s.doc.page().unwrap().shape(ids["cyl"]["shape"].as_str().unwrap()).unwrap();
    assert_eq!(cyl.glyph.as_ref().unwrap().params.get("tag").and_then(|v| v.as_str()), Some("x"));
    let r = rules(&s);
    for rule in ["fluidpower.openPort", "fluidpower.pressurePath", "fluidpower.reliefAfterPump", "fluidpower.supply"] {
        assert!(r.iter().any(|x| x == rule), "{rule} in {r:?}");
    }
    let before = s.doc.clone();
    run(&mut s, "fluid.insert", json!({"type": "fluidpower.ReliefValve"}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before);
    assert!(s.run("fluid.insert", &json!({"type": "fluidpower.Component"})).is_err(), "abstract");
    assert!(s.run("fluid.insert", &json!({"type": "flowchart.Process"})).is_err());
    assert!(s.run("fluid.insert", &json!({"type": "fluidpower.HydraulicValve", "params": {"positions": 9}})).is_err());
    assert!(s.run("fluid.insert", &json!({"type": "fluidpower.HydraulicValve", "params": {"ghost": 1}})).is_err());
}

#[test]
fn plug_closes_ports_and_reopens_them() {
    let (mut s, ids) = page();
    let e = ids["cyl"]["element"].as_str().unwrap();
    let r = run(&mut s, "fluid.plug", json!({"id": e, "ports": ["A", "B"]}));
    assert_eq!(r["plugged"], json!(["A", "B"]));
    assert!(!s.issues.iter().any(|i| i.rule == "fluidpower.openPort" && i.subjects.first().map(String::as_str) == Some(e)));
    let r = run(&mut s, "fluid.plug", json!({"id": e, "ports": ["B"], "plugged": false}));
    assert_eq!(r["plugged"], json!(["A"]));
    assert!(s.run("fluid.plug", &json!({"id": e, "ports": ["Q"]})).is_err());
    assert!(s.run("fluid.plug", &json!({"id": e, "ports": "A"})).is_err());
    // The fix the open port issue offers runs.
    let issue = s.issues.iter().find(|i| i.rule == "fluidpower.openPort").unwrap().clone();
    let fix = issue.fixes.first().unwrap();
    run(&mut s, &fix.command, fix.params.clone());
}

#[test]
fn wire_draw_takes_a_connector_type_and_its_net_kind() {
    let (mut s, ids) = page();
    let (pump, valve) = (ids["pump"]["shape"].as_str().unwrap(), ids["valve"]["shape"].as_str().unwrap());
    let r = run(&mut s, "wire.draw", json!({"from": {"shape": pump, "point": 0}, "to": {"shape": valve, "point": 0}, "type": "fluidpower.pilot"}));
    let c = s.doc.page().unwrap().connector(r["id"].as_str().unwrap()).unwrap().clone();
    assert_eq!((c.kind.as_str(), c.style.dash), ("fluidpower.pilot", vizcraft_diagram::Dash::Dashed));
    let net = s.doc.model.net(r["net"].as_str().unwrap()).unwrap();
    assert_eq!(net.kind, vizcraft_model::NetKind::Fluid);
    let rel = s.doc.model.relationship(c.relationship.as_deref().unwrap()).unwrap();
    assert_eq!(rel.type_id, "fluidpower.PilotLine");
    // A pilot line to the pump's pressure port and the valve's P is on the wrong ports.
    assert!(rules(&s).iter().any(|r| r == "fluidpower.lineEnds"));
    assert!(s.run("wire.draw", &json!({"from": {"x": 0, "y": 0}, "to": {"x": 50, "y": 0}, "type": "nope.nope"})).is_err());
}

#[test]
fn the_1995_edition_changes_only_line_dashes_and_jic_draws_ansi() {
    let mut s = example("fluidpower.pneumaticExample");
    // Positions to a thousandth of a unit: the redraw may round the last bit.
    let xy =
        |s: &Session| s.doc.page().unwrap().shapes.iter().map(|sh| (sh.id.clone(), (sh.x * 1e3).round(), (sh.y * 1e3).round())).collect::<Vec<_>>();
    let before = (xy(&s), s.doc.clone());
    let r = run(&mut s, "variant.set", json!({"variant": "fluidpower.iso1995"}));
    assert_eq!(xy(&s), before.0, "nothing moves");
    let pilots: Vec<_> = s.doc.page().unwrap().connectors.iter().filter(|c| c.kind == "fluidpower.pilotLong").collect();
    assert_eq!(pilots.len(), 2, "{r}");
    assert!(pilots.iter().all(|c| c.style.dash == vizcraft_diagram::Dash::LongDash));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before.1, "one undo step");
    let pack = vizcraft_notation::catalog().pack("fluidpower").unwrap();
    assert_eq!(vizcraft_notation::template::variant_for(pack, "jic").map(|v| v.id.as_str()), Some("fluidpower.ansi"));
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "fluidpower.hydraulicCircuit", "standards": {"symbols": "jic"}}));
    assert_eq!(s.doc.page().unwrap().view.as_ref().unwrap().variant, "fluidpower.ansi");
}

#[test]
fn renumber_writes_letter_and_numeric_codes_and_follows_the_step_chart() {
    let mut s = example("fluidpower.pneumaticExample");
    let tag = |s: &Session, t: &str| s.doc.model.elements.iter().find(|e| e.type_id == t).map(|e| e.tag.clone()).unwrap();
    assert_eq!(tag(&s, "fluidpower.PneumaticCylinder"), "1A1");
    let r = run(&mut s, "fluid.renumber", json!({"codes": "numeric"}));
    assert!(!r["changed"].as_array().unwrap().is_empty());
    let cyl = tag(&s, "fluidpower.PneumaticCylinder");
    assert!(cyl.starts_with("1."), "{cyl}");
    let chart = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.StepChart").unwrap();
    assert!(chart.prop("rows").and_then(|v| v.as_str()).unwrap().starts_with(&cyl), "rows follow the code");
    assert!(!rules(&s).iter().any(|r| r == "fluidpower.stepReachable"), "{:?}", fluid_issues(&s));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(tag(&s, "fluidpower.PneumaticCylinder"), "1A1");
    // A code of the wrong letter is reported, with renumber as the fix.
    let e = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.PneumaticCylinder").unwrap().id.clone();
    run(&mut s, "element.retag", json!({"id": e, "tag": "1V9"}));
    assert!(rules(&s).iter().any(|r| r == "fluidpower.codeFormat"));
    assert!(s.run("fluid.renumber", &json!({"codes": "roman"})).is_err());
}

#[test]
fn port_sets_and_rest_positions_are_checked_and_fixed() {
    let (mut s, ids) = page();
    let valve = ids["valve"]["element"].as_str().unwrap().to_string();
    run(&mut s, "fluid.setPortSet", json!({"set": "iso5599"}));
    let shape = s.doc.page().unwrap().shape(ids["valve"]["shape"].as_str().unwrap()).unwrap();
    assert_eq!(shape.glyph.as_ref().unwrap().ports[..4], ["1", "2", "4", "3"]);
    assert!(rules(&s).iter().any(|r| r == "fluidpower.portSet"));
    run(&mut s, "fluid.setPortSet", json!({"ids": [valve], "set": "iso9461"}));
    assert!(!rules(&s).iter().any(|r| r == "fluidpower.portSet"));
    run(&mut s, "element.setProperty", json!({"id": valve, "key": "rest", "value": 0}));
    let issue = s.issues.iter().find(|i| i.rule == "fluidpower.restPosition").cloned().expect("a spring-centred valve drawn in an end square");
    let fix = issue.fixes.first().unwrap();
    run(&mut s, &fix.command, fix.params.clone());
    assert!(!rules(&s).iter().any(|r| r == "fluidpower.restPosition"));
    assert!(s.run("fluid.setPortSet", &json!({"set": "letters"})).is_err());
    let cyl = ids["cyl"]["element"].as_str().unwrap();
    assert!(s.run("fluid.setPortSet", &json!({"ids": [cyl], "set": "iso5599"})).is_err());
}

#[test]
fn trace_stores_an_analysis_and_makes_no_undo_step() {
    let mut s = example("fluidpower.hydraulicExample");
    let pump = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.HydraulicPump").unwrap().id.clone();
    let cyl = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.HydraulicCylinder").unwrap().id.clone();
    let steps = s.transactions().len();
    let doc = s.doc.clone();
    let r = run(&mut s, "fluid.trace", json!({"from": pump}));
    assert_eq!(r["kind"], "fluidTrace");
    let stored = s.analysis.clone().unwrap();
    assert!(stored.elements.contains(&cyl), "any position reaches the cylinder");
    assert_eq!((s.transactions().len(), &s.doc), (steps, &doc));
    let rest = run(&mut s, "fluid.trace", json!({"from": pump, "positions": "rest"}));
    assert!(!rest["elements"].as_array().unwrap().iter().any(|x| x == &json!(cyl)), "a spring-centred valve blocks at rest");
    assert!(s.run("fluid.trace", &json!({"from": {"shape": "nope", "point": 0}})).is_err());
    assert!(s.run("fluid.trace", &json!({"from": pump, "positions": "some"})).is_err());
    let c = run(&mut s, "fluid.circuit", json!({}));
    assert_eq!(c["tanks"].as_array().unwrap().len(), 1);
    assert!(c["components"].as_array().unwrap().iter().all(|p| p["ports"].as_array().unwrap().iter().all(|q| q["open"] == false)));
}

#[test]
fn the_power_unit_is_one_undo_step_with_every_port_joined() {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "fluidpower.hydraulicCircuit"}));
    let before = s.doc.clone();
    let steps = s.transactions().len();
    let r = run(&mut s, "fluid.powerUnit", json!({"accumulator": true, "cooler": true, "pump": "variable"}));
    assert_eq!(s.transactions().len(), steps + 1);
    assert!(r["elements"]["accumulator"].is_string() && r["elements"]["cooler"].is_string());
    assert!(
        !rules(&s).iter().any(|r| r == "fluidpower.openPort" || r == "fluidpower.supply" || r == "fluidpower.reliefAfterPump"),
        "{:?}",
        fluid_issues(&s)
    );
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before);
    assert!(s.run("fluid.powerUnit", &json!({"pump": "big"})).is_err());
    assert!(s.run("fluid.powerUnit", &json!({"circuit": -1})).is_err());
}

#[test]
fn the_step_chart_is_made_read_and_edited_cell_by_cell() {
    let mut s = example("fluidpower.pneumaticExample");
    let chart = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.StepChart").unwrap().id.clone();
    let r = run(&mut s, "fluid.steps", json!({"chart": chart}));
    assert_eq!((r["steps"].as_u64(), r["findings"].as_array().unwrap().len()), (Some(2), 0), "{r}");
    run(&mut s, "fluid.insertStep", json!({"chart": chart}));
    let r = run(&mut s, "fluid.steps", json!({"chart": chart}));
    assert_eq!(r["steps"].as_u64(), Some(3));
    assert!(r["findings"].as_array().unwrap().iter().any(|f| f["idle"] == true), "the copied step moves nothing");
    assert!(rules(&s).iter().any(|r| r == "fluidpower.idleStep"));
    run(&mut s, "fluid.setStep", json!({"chart": chart, "row": 0, "step": 3, "state": 1}));
    run(&mut s, "fluid.deleteStep", json!({"chart": chart, "at": 1}));
    let r = run(&mut s, "fluid.steps", json!({"chart": chart}));
    assert_eq!(r["rows"][0]["states"], json!([0, 0, 1]));
    for bad in
        [json!({"chart": chart, "row": 5, "step": 0, "state": 1}), json!({"chart": chart, "row": 0, "step": 0, "state": 2}), json!({"chart": "nope"})]
    {
        assert!(s.run("fluid.setStep", &bad).is_err(), "{bad}");
    }
    // A row naming no actuator is reported.
    let shape = s.doc.page().unwrap().shapes.iter().find(|sh| sh.element.as_deref() == Some(chart.as_str())).unwrap().id.clone();
    run(&mut s, "element.setProperty", json!({"id": shape, "key": "rows", "value": "9A9 0 1 0"}));
    assert!(rules(&s).iter().any(|r| r == "fluidpower.stepReachable"));
    let made = run(&mut s, "fluid.stepChart", json!({"steps": 4}));
    assert!(made["rows"].as_str().unwrap().starts_with("1A1"));
    assert!(s.run("fluid.stepChart", &json!({"steps": 99})).is_err());
}

#[test]
fn an_impossible_step_is_reported() {
    let mut s = example("fluidpower.pneumaticExample");
    let valve = s.doc.model.elements.iter().find(|e| e.type_id == "fluidpower.PneumaticValve").unwrap().id.clone();
    // Both squares now block everything: no position can drive the cylinder.
    run(&mut s, "element.setProperty", json!({"id": valve, "key": "paths", "value": "X; X"}));
    assert!(rules(&s).iter().any(|r| r == "fluidpower.stepReachable"), "{:?}", fluid_issues(&s));
}

/// Writes the fluid power pack's test cases (`crates/notation/packs/tests/fluidpower/`) from the
/// examples with one fault each. Run with `--ignored` after a change that alters the documents;
/// the notation crate's case runner checks them on every test run.
#[test]
#[ignore]
fn write_pack_cases() {
    let dir = format!("{}/../notation/packs/tests/fluidpower", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let profile = json!({"enabled": true, "userSet": true, "rulesets": ["fluidpower.circuit", "fluidpower.hydraulic", "fluidpower.pneumatic", "fluidpower.sequence"]});
    let find = |s: &Session, t: &str| s.doc.model.elements.iter().find(|e| e.type_id == t).unwrap().id.clone();
    let write = |name: &str, mut s: Session| {
        s.doc.conformance = serde_json::from_value(profile.clone()).unwrap();
        s.refresh_issues();
        let expect: Vec<Value> = s.issues.iter().map(|i| json!({"rule": i.rule, "items": i.items})).collect();
        println!("CASE {name}: {expect:?}");
        let file = json!({"format": "vizcraft", "version": 4, "document": s.doc, "model": s.doc.model});
        let case = json!({"document": file, "profile": profile, "expect": expect});
        std::fs::write(format!("{dir}/{name}.json"), serde_json::to_string(&case).unwrap() + "\n").unwrap();
    };
    write("clean_circuit", example("fluidpower.hydraulicExample"));
    let mut s = example("fluidpower.hydraulicExample");
    let filter =
        s.doc.page().unwrap().shapes.iter().find(|sh| sh.element.as_deref() == Some(find(&s, "fluidpower.Filter").as_str())).unwrap().id.clone();
    let wire = s.doc.page().unwrap().connectors.iter().find(|c| c.start.shape() == Some(filter.as_str())).unwrap().id.clone();
    run(&mut s, "edit.select", json!({"ids": [wire]}));
    run(&mut s, "edit.delete", json!({}));
    write("open_port", s);
    let mut s = example("fluidpower.hydraulicExample");
    let relief = find(&s, "fluidpower.ReliefValve");
    run(&mut s, "element.setProperty", json!({"id": relief, "key": "function", "value": "sequence"}));
    write("fixed_pump_without_relief", s);
    let mut s = example("fluidpower.hydraulicExample");
    let cyl = s
        .doc
        .page()
        .unwrap()
        .shapes
        .iter()
        .find(|sh| sh.element.as_deref() == Some(find(&s, "fluidpower.HydraulicCylinder").as_str()))
        .unwrap()
        .id
        .clone();
    let wire = s.doc.page().unwrap().connectors.iter().find(|c| c.end.shape() == Some(cyl.as_str())).unwrap().id.clone();
    run(&mut s, "connector.setKind", json!({"ids": [wire], "kind": "fluidpower.pilot"}));
    write("pilot_on_working_port", s);
    let mut s = example("fluidpower.hydraulicExample");
    let valve = find(&s, "fluidpower.HydraulicValve");
    run(&mut s, "element.setProperty", json!({"id": valve, "key": "rest", "value": 2}));
    write("valve_not_at_rest", s);
    let mut s = example("fluidpower.pneumaticExample");
    let valve = find(&s, "fluidpower.PneumaticValve");
    run(&mut s, "element.setProperty", json!({"id": valve, "key": "paths", "value": "P-A B-S; X"}));
    write("unreachable_step", s);
}
