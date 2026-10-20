//! Tests of the electrical, ladder, terminal, cable and I/O commands and the electrical pass.

use serde_json::{Value, json};

use crate::Session;
use crate::cmd::model::fixture::{blank, refused, run};

/// Run `id` and check that one undo takes the document back to where it was.
fn one_step(s: &mut Session, id: &str, v: Value) -> Value {
    let before = s.doc.clone();
    let r = run(s, id, v.clone());
    run(s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "{id} {v} is one undo step");
    run(s, "edit.redo", json!({}));
    r
}

/// A blank document set up as an electrical control circuit.
fn circuit() -> Session {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "electrical.controlCircuit"}));
    s
}

fn element(s: &Session, id: &str) -> vizcraft_model::Element {
    s.doc.model.element(id).cloned().unwrap()
}

fn add(s: &mut Session, ty: &str, x: f64, y: f64) -> (String, String) {
    let r = run(s, "element.create", json!({"type": ty, "x": x, "y": y}));
    (r["id"].as_str().unwrap().to_string(), r["shape"].as_str().unwrap().to_string())
}

#[test]
fn the_pack_loads_with_its_types_and_document_types() {
    let cat = vizcraft_notation::catalog();
    let p = cat.pack("electrical").expect("the electrical pack ships");
    assert!(p.templates.len() >= 10 && p.examples.len() >= 3, "{} types, {} examples", p.templates.len(), p.examples.len());
    for v in ["electrical.iec", "electrical.ansi", "electrical.jic", "electrical.footprint"] {
        assert!(p.variant(v).is_some(), "{v}");
    }
    assert!(vizcraft_notation::template::variant_for(p, "jic").is_some());
}

#[test]
fn devices_take_class_codes_and_numbers_per_code() {
    let mut s = circuit();
    let (k1, _) = add(&mut s, "electrical.Coil", 100.0, 100.0);
    let (k2, _) = add(&mut s, "electrical.Coil", 200.0, 100.0);
    let (f1, _) = add(&mut s, "electrical.Fuse", 300.0, 100.0);
    assert_eq!((element(&s, &k1).tag, element(&s, &k2).tag, element(&s, &f1).tag), ("K1".into(), "K2".into(), "F1".into()));
    assert_eq!(element(&s, &k1).prop("code").map(|v| v.display()), Some("K".into()));
}

#[test]
fn a_contact_with_the_coils_tag_links_and_the_coil_shows_a_mirror() {
    let mut s = circuit();
    let (coil, coil_shape) = add(&mut s, "electrical.Coil", 100.0, 100.0);
    let (contact, _) = add(&mut s, "electrical.Contact", 300.0, 100.0);
    run(&mut s, "element.retag", json!({"id": contact, "tag": "K1"}));
    let links: Vec<_> = s.doc.model.relationships.iter().filter(|r| r.type_id == "electrical.ContactOf").collect();
    assert_eq!(links.len(), 1);
    assert_eq!((links[0].source.as_str(), links[0].target.as_str()), (contact.as_str(), coil.as_str()));
    let mirror = s.doc.pages[0].shapes.iter().find(|sh| sh.ext.get("vizcraft.mirror.for") == Some(&coil_shape)).expect("a mirror under the coil");
    assert!(mirror.text.starts_with("13-14 "), "{}", mirror.text);
    let m = run(&mut s, "electrical.mirror", json!({"coil": coil}));
    assert_eq!(m["contacts"][0]["id"], json!(contact));
    // Retagging the coil takes the contact along; retagging the contact away unlinks it.
    run(&mut s, "element.retag", json!({"id": coil, "tag": "K7"}));
    assert_eq!(element(&s, &contact).tag, "K7");
    run(&mut s, "element.retag", json!({"id": contact, "tag": "K9"}));
    assert!(!s.doc.model.relationships.iter().any(|r| r.type_id == "electrical.ContactOf"));
    assert!(!s.doc.pages[0].shapes.iter().any(|sh| sh.ext.contains_key("vizcraft.mirror.for")));
    // electrical.link gives contacts the coil's tag.
    let r = one_step(&mut s, "electrical.link", json!({"contacts": [contact], "coil": coil}));
    assert_eq!(r["tag"], "K7");
    assert_eq!(s.doc.model.relationships.iter().filter(|r| r.type_id == "electrical.ContactOf").count(), 1);
    // A second sync changes nothing.
    let before = s.doc.clone();
    vizcraft_notation::bind::sync(&mut s.doc, &before, vizcraft_notation::catalog());
    assert_eq!(s.doc, before, "the pass is idempotent");
}

#[test]
fn switching_standards_redraws_in_place_recomposes_tags_and_undoes_in_one_step() {
    let mut s = circuit();
    let (coil, coil_shape) = add(&mut s, "electrical.Coil", 98.425, 98.425);
    let (contact, contact_shape) = add(&mut s, "electrical.Contact", 295.275, 98.425);
    run(&mut s, "element.retag", json!({"id": contact, "tag": "K1"}));
    let w = run(&mut s, "wire.draw", json!({"from": {"shape": contact_shape, "point": 1}, "to": {"shape": coil_shape, "point": 0}}));
    let centre = |s: &Session, id: &str| s.doc.pages[0].shape(id).unwrap().center();
    let (c1, c2) = (centre(&s, &coil_shape), centre(&s, &contact_shape));
    let before = s.doc.clone();
    let r = run(&mut s, "electrical.setStandards", json!({"symbols": "jic", "tags": "jic"}));
    assert_eq!((r["symbols"].as_str(), r["tags"].as_str()), (Some("jic"), Some("jic")));
    assert_eq!(s.doc.pages[0].view.as_ref().unwrap().variant, "electrical.jic");
    assert_eq!((element(&s, &coil).tag, element(&s, &contact).tag), ("CR1".into(), "CR1".into()));
    let close = |a: vizcraft_diagram::Point, b: vizcraft_diagram::Point| (a.x - b.x).abs() < 12.0 && (a.y - b.y).abs() < 12.0;
    assert!(close(centre(&s, &coil_shape), c1) && close(centre(&s, &contact_shape), c2), "centres stay");
    let wire = s.doc.pages[0].connector(w["id"].as_str().unwrap()).unwrap();
    assert_eq!(wire.start.shape(), Some(contact_shape.as_str()), "glue stays");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before, "one undo step");
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "electrical.setStandards", json!({"symbols": "iec", "tags": "iec81346"}));
    assert_eq!(element(&s, &coil).tag, "K1");
    assert!(refused(&mut s, "electrical.setStandards", json!({"symbols": "din"})).contains("symbols"));
    assert!(refused(&mut s, "electrical.setStandards", json!({})).contains("give"));
}

#[test]
fn a_wire_drawn_as_a_type_takes_its_look_and_net_kind() {
    let mut s = circuit();
    let r = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 200, "y": 0}, "type": "electrical.bus"}));
    let c = s.doc.pages[0].connector(r["id"].as_str().unwrap()).unwrap().clone();
    assert_eq!(c.kind, "electrical.bus");
    assert_eq!(c.style.line_width, 2.5);
    assert_eq!(s.doc.model.net(r["net"].as_str().unwrap()).unwrap().kind, vizcraft_model::NetKind::Potential);
    assert!(
        refused(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 0}, "to": {"x": 9, "y": 0}, "type": "nope.line"}))
            .contains("unknown connector type")
    );
}

#[test]
fn ladder_insert_draws_rails_and_numbers_wires_from_rungs() {
    let mut s = circuit();
    let r = one_step(&mut s, "ladder.insert", json!({"x": 100, "y": 100, "rungs": 5, "first": 1}));
    let (ladder, frame) = (r["ladder"].as_str().unwrap().to_string(), r["shape"].as_str().unwrap().to_string());
    assert_eq!(r["rails"].as_array().unwrap().len(), 2);
    let names: Vec<String> = r["nets"].as_array().unwrap().iter().map(|n| s.doc.model.net(n.as_str().unwrap()).unwrap().name.clone()).collect();
    assert_eq!(names, ["L1", "L2"]);
    // A coil dropped near rung 3 and placed on it gets rung 3.
    let (coil, coil_shape) = add(&mut s, "electrical.Coil", 300.0, 300.0);
    let f = s.doc.pages[0].shape(&frame).unwrap().clone();
    let step = f.h / 5.0;
    let y3 = f.y + step * 2.5;
    run(&mut s, "shape.move", json!({"id": coil_shape, "x": f.x + f.w / 2.0, "y": y3 - 5.0}));
    one_step(&mut s, "ladder.place", json!({"shapes": [coil_shape], "rung": 3}));
    assert_eq!(element(&s, &coil).prop("rung").and_then(|v| v.as_f64()), Some(3.0));
    // A wire along rung 3 numbers 103 on sheet 1, and the second wire on it 103A.
    let a = run(
        &mut s,
        "wire.draw",
        json!({"from": {"x": f.x + 60.0, "y": y3}, "to": {"x": f.x + 120.0, "y": y3}, "points": [{"x": f.x + 90.0, "y": y3}]}),
    );
    let b = run(
        &mut s,
        "wire.draw",
        json!({"from": {"x": f.x + 200.0, "y": y3}, "to": {"x": f.x + 260.0, "y": y3}, "points": [{"x": f.x + 230.0, "y": y3}]}),
    );
    let r = one_step(&mut s, "ladder.numberWires", json!({"ladder": ladder}));
    let number = |net: &Value| s.doc.model.net(net.as_str().unwrap()).unwrap().number.clone();
    assert_eq!((number(&a["net"]), number(&b["net"])), ("103".into(), "103A".into()), "{r}");
    // ladder.set stretches the rails to the new bottom.
    one_step(&mut s, "ladder.set", json!({"ladder": ladder, "rungs": 8}));
    let f2 = s.doc.pages[0].shape(&frame).unwrap().clone();
    assert!(f2.h > f.h);
}

#[test]
fn terminal_strips_number_bridge_and_pass_nets_through() {
    let mut s = circuit();
    let r = one_step(&mut s, "terminal.strip", json!({"x": 100, "y": 100, "count": 4}));
    let strip = r["strip"].as_str().unwrap().to_string();
    let terms: Vec<String> = r["terminals"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect();
    assert_eq!(terms.len(), 4);
    assert_eq!(element(&s, &terms[0]).tag, "X1:1");
    assert_eq!(element(&s, &terms[3]).owner.as_deref(), Some(strip.as_str()));
    let r = one_step(&mut s, "terminal.add", json!({"strip": strip, "count": 2}));
    assert_eq!(r["terminals"].as_array().unwrap().len(), 2);
    let r = one_step(&mut s, "terminal.renumber", json!({"strip": strip, "first": 11}));
    assert_eq!(r["terminals"][0]["number"], "11");
    one_step(&mut s, "terminal.bridge", json!({"ids": [terms[0], terms[1]], "bridge": "B1"}));
    assert_eq!(element(&s, &terms[1]).prop("bridge").map(|v| v.display()), Some("B1".into()));
    // Wires on both sides of a terminal are one net, and the terminal records it.
    let shape = r["terminals"][0]["id"].as_str().unwrap();
    let ts = s.doc.pages[0].shapes.iter().find(|sh| sh.element.as_deref() == Some(shape)).unwrap().clone();
    let pins = ts.connection_points_world();
    let a = run(&mut s, "wire.draw", json!({"from": {"shape": ts.id, "point": 0}, "to": {"x": pins[0].x - 60.0, "y": pins[0].y}}));
    let b = run(&mut s, "wire.draw", json!({"from": {"shape": ts.id, "point": 1}, "to": {"x": pins[1].x + 60.0, "y": pins[1].y}}));
    let net = |s: &Session, c: &Value| s.doc.pages[0].connector(c["id"].as_str().unwrap()).unwrap().net.clone();
    assert_eq!(net(&s, &a), net(&s, &b), "a terminal passes its net through");
    let n = net(&s, &a).unwrap();
    run(&mut s, "net.set", json!({"net": n, "number": "17"}));
    assert_eq!(element(&s, shape).prop("netIn").map(|v| v.display()), Some("17".into()));
    assert!(refused(&mut s, "terminal.strip", json!({"x": 0, "y": 0, "count": 0})).contains("count"));
}

#[test]
fn cables_have_cores_ends_and_assignments() {
    let mut s = circuit();
    let (a, _) = add(&mut s, "electrical.Plug", 100.0, 100.0);
    let (b, _) = add(&mut s, "electrical.Plug", 500.0, 100.0);
    let r = one_step(&mut s, "cable.create", json!({"from": a, "to": b, "cores": 3, "type": "3G1.5"}));
    let cable = r["cable"].as_str().unwrap().to_string();
    assert_eq!(r["cores"].as_array().unwrap().len(), 3);
    assert_eq!(r["ends"].as_array().unwrap().len(), 2);
    assert!(r["shape"].is_string(), "drawn between its ends");
    let w = run(&mut s, "wire.draw", json!({"from": {"x": 0, "y": 400}, "to": {"x": 100, "y": 400}}));
    let r = one_step(&mut s, "cable.assign", json!({"cable": cable, "connectors": [w["id"]]}));
    let net = r["assigned"][0]["net"].as_str().unwrap();
    assert_eq!(s.doc.model.net(net).unwrap().properties.get("cable").map(|v| v.display()), Some("W1".into()));
    let issues = run(&mut s, "validate.run", json!({}));
    assert!(!issues["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "electrical.cableCores"), "{issues}");
}

#[test]
fn io_assign_makes_one_point_per_channel_with_its_address() {
    let mut s = circuit();
    let (m, _) = add(&mut s, "electrical.IoModule", 100.0, 100.0);
    run(&mut s, "element.setProperty", json!({"id": m, "key": "address", "value": "I0.0"}));
    let r = one_step(&mut s, "io.assign", json!({"module": m, "channel": 3, "tag": "S1"}));
    assert_eq!(r["address"], "I0.3");
    let again = run(&mut s, "io.assign", json!({"module": m, "channel": 3, "tag": "S2"}));
    assert_eq!(again["point"], r["point"], "the channel's point is reused");
    assert_eq!(super::channel_address("", "A1", 2), "A1.2");
    assert_eq!(super::channel_address("%IX4.0", "A1", 7), "%IX4.7");
}

#[test]
fn net_lists_tables_and_the_netlist_export() {
    let mut s = circuit();
    let (_, k) = add(&mut s, "electrical.Coil", 98.425, 98.425);
    let (_, f) = add(&mut s, "electrical.Fuse", 98.425, 295.275);
    run(&mut s, "wire.draw", json!({"from": {"shape": f, "point": 0}, "to": {"shape": k, "point": 1}}));
    run(&mut s, "net.renumber", json!({"start": 5}));
    let t = run(&mut s, "table.list", json!({"subject": "connections", "kind": "wire"}));
    assert_eq!(t["count"], 1);
    let row = &t["rows"][0];
    assert_eq!((row[0].as_str(), row[1].as_str(), row[2].as_str()), (Some("5"), Some("F1:1"), Some("K1:A2")));
    let nets = run(&mut s, "table.list", json!({"subject": "nets"}));
    let net = nets["ids"][0].as_str().unwrap().to_string();
    one_step(&mut s, "table.set", json!({"id": net, "key": "colour", "value": "BK"}));
    // A wire list cell edits the wire's net.
    let wire = t["ids"][0].as_str().unwrap().to_string();
    one_step(&mut s, "table.set", json!({"id": wire, "key": "wire", "value": "7"}));
    run(&mut s, "net.set", json!({"net": net, "number": "5"}));
    let csv = run(&mut s, "table.export", json!({"view": "list", "subject": "connections", "format": "csv"}));
    assert!(csv["content"].as_str().unwrap().contains("F1:1"));
    let dir = std::env::temp_dir().join(format!("vizcraft-elec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let xlsx = dir.join("wires.xlsx");
    run(&mut s, "table.export", json!({"view": "list", "subject": "connections", "format": "xlsx", "path": xlsx.to_str().unwrap()}));
    assert!(std::fs::metadata(&xlsx).unwrap().len() > 100);
    let text = run(&mut s, "net.export", json!({"format": "text"}));
    assert!(text["content"].as_str().unwrap().contains("net 5 (wire, colour=BK)\n  F1:1\n  K1:A2"), "{}", text["content"]);
    let j = run(&mut s, "net.export", json!({}));
    let parsed = vizcraft_formats::netlist::from_json(&serde_json::from_str(j["content"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(parsed.nets.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(refused(&mut s, "table.list", json!({"subject": "pipes"})).contains("subject"));
}

#[test]
fn rules_report_unconnected_pins_duplicate_numbers_and_bad_device_numbers() {
    let mut s = circuit();
    let (_, k) = add(&mut s, "electrical.Coil", 98.425, 98.425);
    let issues = |s: &mut Session| run(s, "validate.run", json!({}))["issues"].as_array().unwrap().clone();
    assert!(issues(&mut s).iter().any(|i| i["rule"] == "electrical.unconnectedPin" && i["message"].as_str().unwrap().contains("A1")));
    let (_, f) = add(&mut s, "electrical.Fuse", 98.425, 295.275);
    run(&mut s, "wire.draw", json!({"from": {"shape": f, "point": 0}, "to": {"shape": k, "point": 1}}));
    run(&mut s, "wire.draw", json!({"from": {"x": 600, "y": 600}, "to": {"x": 700, "y": 600}}));
    let nets: Vec<String> = s.doc.model.nets.iter().map(|n| n.id.clone()).collect();
    for n in &nets {
        run(&mut s, "net.set", json!({"net": n, "number": "9"}));
    }
    assert!(issues(&mut s).iter().any(|i| i["rule"] == "electrical.duplicateWireNumber"));
    // A relay with a number C37.2 does not have.
    run(&mut s, "validate.setProfile", json!({"rulesets": ["electrical.erc"]}));
    let r = run(&mut s, "element.create", json!({"type": "electrical.Relay", "x": 500, "y": 100, "properties": {"function": "999"}}));
    assert!(issues(&mut s).iter().any(|i| i["rule"] == "electrical.deviceNumber"));
    run(&mut s, "element.setProperty", json!({"id": r["id"], "key": "function", "value": "50/51"}));
    assert!(!issues(&mut s).iter().any(|i| i["rule"] == "electrical.deviceNumber"));
}

/// Compare page `page`'s SVG export with `testdata/electrical/<name>.golden.svg`
/// (`VIZCRAFT_BLESS=1` rewrites it; `VIZCRAFT_PNG_OUT=<dir>` also writes a PNG to look at).
fn golden(s: &Session, page: usize, name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/electrical");
    let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, page).unwrap()).unwrap();
    let path = dir.join(format!("{name}.golden.svg"));
    if std::env::var("VIZCRAFT_BLESS").is_ok() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, &svg).unwrap();
    }
    if let Ok(out) = std::env::var("VIZCRAFT_PNG_OUT") {
        let (png, _, _) = vizcraft_formats::png::render_page(&s.doc, page, 1.0).unwrap();
        std::fs::write(std::path::Path::new(&out).join(format!("{name}.png")), png).unwrap();
    }
    assert_eq!(svg, std::fs::read_to_string(&path).unwrap_or_default(), "{name}: SVG differs from {}", path.display());
}

#[test]
fn examples_export_as_their_goldens_in_each_symbol_standard() {
    let mut s = blank();
    run(&mut s, "example.open", json!({"example": "electrical.controlCircuitSample"}));
    golden(&s, 0, "control-iec");
    for (symbols, tags, name) in [("ansi", "y1444", "control-ansi"), ("jic", "jic", "control-jic")] {
        run(&mut s, "electrical.setStandards", json!({"symbols": symbols, "tags": tags}));
        golden(&s, 0, name);
    }
    for (example, name) in [("electrical.ladderSample", "ladder-jic"), ("electrical.oneLineSample", "one-line")] {
        let mut s = blank();
        run(&mut s, "example.open", json!({"example": example}));
        golden(&s, 0, name);
    }
}

#[test]
fn an_electrical_document_round_trips_through_the_native_format() {
    let mut s = blank();
    run(&mut s, "example.open", json!({"example": "electrical.motorControlSample"}));
    let bytes = crate::io::save_bytes("motor.vizcraft", &s.doc).unwrap();
    let back = crate::io::load_bytes("motor.vizcraft", &bytes).unwrap();
    assert_eq!(back.model, s.doc.model);
    assert_eq!(back.pages.len(), s.doc.pages.len());
    assert_eq!(back.pages, s.doc.pages);
}

#[test]
fn copy_and_paste_keeps_one_mirror_per_coil_and_links_the_pasted_contact() {
    let mut s = circuit();
    let (coil, coil_shape) = add(&mut s, "electrical.Coil", 100.0, 100.0);
    let (contact, contact_shape) = add(&mut s, "electrical.Contact", 300.0, 100.0);
    run(&mut s, "element.retag", json!({"id": contact, "tag": "K1"}));
    let mirror = s.doc.pages[0].shapes.iter().find(|sh| sh.ext.get("vizcraft.mirror.for") == Some(&coil_shape)).unwrap().id.clone();
    run(&mut s, "edit.select", json!({"ids": [contact_shape, mirror]}));
    run(&mut s, "edit.copy", json!({}));
    run(&mut s, "edit.paste", json!({}));
    let mirrors = s.doc.pages[0].shapes.iter().filter(|sh| sh.ext.get("vizcraft.mirror.for") == Some(&coil_shape)).count();
    assert_eq!(mirrors, 1, "one mirror per coil view");
    // A pasted contact is unassigned (paste numbers tags afresh and contacts are not numbered):
    // it links once it is given a coil's tag.
    let links = |s: &Session| s.doc.model.relationships.iter().filter(|r| r.type_id == "electrical.ContactOf" && r.target == coil).count();
    assert_eq!(links(&s), 1);
    let pasted = s.doc.model.elements.iter().find(|e| e.type_id == "electrical.Contact" && e.tag.is_empty()).unwrap().id.clone();
    run(&mut s, "electrical.link", json!({"contacts": [pasted], "coil": coil}));
    assert_eq!(links(&s), 2);
}
