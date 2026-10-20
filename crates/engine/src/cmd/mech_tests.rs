use serde_json::{Value, json};

use crate::{Document, Session};

fn open(example: &str) -> Session {
    let mut s = Session::new(Document::new());
    s.run("example.open", &json!({"example": example})).unwrap_or_else(|e| panic!("{example}: {e}"));
    s
}

fn shape_of(s: &Session, name: &str) -> String {
    let e = s.doc.model.elements.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no element {name}"));
    s.doc.pages.iter().flat_map(|p| &p.shapes).find(|sh| sh.element.as_deref() == Some(e.id.as_str())).unwrap().id.clone()
}

/// Every example of the four packs opens and holds to its own rules with no issue, live or batch.
#[test]
fn every_wave4_example_passes_its_own_rules() {
    let mut s = Session::new(Document::new());
    let list = s.run("example.list", &json!({})).unwrap();
    let ids: Vec<String> = list["examples"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str())
        .filter(|id| ["mechanical.", "annotation.", "reliability.", "idef0."].iter().any(|p| id.starts_with(p)))
        .map(str::to_string)
        .collect();
    assert_eq!(ids.len(), 11, "{ids:?}");
    let mut report = Vec::new();
    for id in &ids {
        let mut s = open(id);
        let r = s.run("validate.run", &json!({"scope": "all"})).unwrap();
        for i in r["issues"].as_array().unwrap() {
            report.push(format!("{id}: {} {}", i["rule"], i["message"]));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}

#[test]
fn every_wave4_document_type_starts_empty_with_its_setup() {
    let want = [
        ("mechanical.kinematic", "mechanical.Kinematic", "mechanical.iso3952"),
        ("mechanical.transmission", "mechanical.Transmission", "mechanical.iso3952"),
        ("mechanical.bondGraph", "mechanical.BondGraph", ""),
        ("mechanical.controlBlock", "mechanical.ControlBlock", ""),
        ("annotation.detail", "annotation.Detail", "annotation.iso"),
        ("reliability.faultTree", "reliability.FaultTree", "reliability.iec61025"),
        ("reliability.rbd", "reliability.RBD", ""),
        ("reliability.eventTree", "reliability.EventTree", ""),
        ("reliability.fmea", "reliability.FMEA", ""),
        ("idef0.model", "idef0.Context", "idef0.fips183"),
    ];
    for (t, vp, variant) in want {
        let mut s = Session::new(Document::new());
        let r = s.run("template.new", &json!({"template": t})).unwrap_or_else(|e| panic!("{t}: {e}"));
        assert_eq!(r["viewpoint"], json!(vp), "{t}");
        assert_eq!(r["variant"], json!(variant), "{t}");
        assert_eq!(r["sheet"], json!("iso.a3.landscape"), "{t}");
        assert!(s.doc.model.elements.is_empty(), "{t} draws nothing");
        let rulesets = r["rulesets"].as_array().unwrap();
        assert!(!rulesets.is_empty(), "{t}");
        let page = s.doc.page().unwrap();
        assert!(page.sheet.is_some(), "{t} has a sheet");
    }
}

#[test]
fn four_bar_mobility_is_one_and_makes_no_undo_step() {
    let mut s = open("mechanical.fourBar");
    let dirty = s.dirty;
    let undo = s.can_undo();
    let r = s.run("mech.mobility", &json!({})).unwrap();
    assert_eq!((r["m"].clone(), r["n"].clone(), r["j1"].clone()), (json!(1), json!(4), json!(4)), "{r}");
    assert_eq!((s.dirty, s.can_undo()), (dirty, undo));
    assert!(s.analysis.is_some());
    assert_eq!(open("mechanical.sliderCrank").run("mech.mobility", &json!({})).unwrap()["m"], json!(1));
    // A link from B to D makes the linkage a structure: mobility 0, and the rule says so.
    let (b, d) = (shape_of(&s, "B"), shape_of(&s, "D"));
    s.run("relationship.create", &json!({"from": b, "to": d, "type": "mechanical.Link"})).unwrap();
    assert_eq!(s.run("mech.mobility", &json!({})).unwrap()["m"], json!(0));
    assert!(s.issues.iter().any(|i| i.rule == "mechanical.mobility"), "{:?}", s.issues);
}

#[test]
fn gearbox_ratio_from_the_selection() {
    let mut s = open("mechanical.gearbox");
    let (m, l) = (shape_of(&s, "M1"), shape_of(&s, "Conveyor"));
    s.run("edit.select", &json!({"ids": [m, l]})).unwrap();
    let r = s.run("mech.ratio", &json!({})).unwrap();
    assert_eq!((r["ratio"].clone(), r["sign"].clone()), (json!("1:6"), json!("same")), "{r}");
    s.run("edit.selectNone", &json!({})).unwrap();
    assert!(s.run("mech.ratio", &json!({})).is_err());
}

#[test]
fn causality_is_one_undo_step_and_the_pi_loop_reads_by_mason() {
    let mut s = open("mechanical.massSpringDamper");
    let strokes =
        |s: &Session| s.doc.model.relationships.iter().map(|r| r.prop("causal").map(|v| v.display()).unwrap_or_default()).collect::<Vec<_>>();
    assert_eq!(strokes(&s), ["end", "end", "start", "start"]);
    let ids: Vec<String> = s.doc.model.relationships.iter().map(|r| r.id.clone()).collect();
    for id in &ids {
        s.run("relationship.setProperty", &json!({"id": id, "key": "causal", "value": null})).unwrap();
    }
    assert!(s.issues.iter().any(|i| i.rule == "mechanical.causalStroke"));
    let r = s.run("mech.causality", &json!({})).unwrap();
    assert_eq!(r["assigned"], json!(4));
    assert_eq!(strokes(&s), ["end", "end", "start", "start"]);
    assert!(!s.issues.iter().any(|i| i.rule == "mechanical.causalStroke"));
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(strokes(&s), ["", "", "", ""]);
    let mut p = open("mechanical.piLoop");
    let m: Value = p.run("mech.mason", &json!({})).unwrap();
    assert_eq!(m["formula"], json!("(Kp + Ki/s)·G(s) / (1 + (Kp + Ki/s)·G(s))"), "{m}");
}
