use serde_json::json;

use crate::{Document, Session};

fn open(example: &str) -> Session {
    let mut s = Session::new(Document::new());
    s.run("example.open", &json!({"example": example})).unwrap_or_else(|e| panic!("{example}: {e}"));
    s
}

fn names(s: &Session, ids: &serde_json::Value) -> Vec<String> {
    ids.as_array().unwrap().iter().map(|id| s.doc.model.element(id.as_str().unwrap()).unwrap().name.clone()).collect()
}

#[test]
fn pump_fault_tree_cut_sets_follow_the_transfer() {
    let mut s = open("reliability.pumpFaultTree");
    let (undo, dirty) = (s.can_undo(), s.dirty);
    let r = s.run("rel.cutsets", &json!({})).unwrap();
    let sets: Vec<Vec<String>> = r["sets"].as_array().unwrap().iter().map(|x| names(&s, &x["events"])).collect();
    assert_eq!(sets.len(), 4, "{sets:?}");
    assert!(sets.contains(&vec!["Valve shut".to_string()]) && sets.contains(&vec!["Grid down".to_string()]));
    assert!(sets.contains(&vec!["Pump A fails".to_string(), "Pump B fails".to_string()]));
    let rare = r["rareEvent"].as_f64().unwrap();
    assert!((rare - (0.001 + 0.0005 + 0.002 + 0.0004)).abs() < 1e-12, "{rare}");
    assert_eq!((s.can_undo(), s.dirty), (undo, dirty), "analyses make no undo step");
}

#[test]
fn cooling_rbd_availability() {
    let mut s = open("reliability.coolingRbd");
    let before = s.can_undo();
    let r = s.run("rel.availability", &json!({})).unwrap();
    let pump = 2000.0 / 2020.0;
    let pumps = 1.0 - (1.0 - pump) * (1.0 - pump);
    let fans = 3.0 * 0.98f64.powi(2) * 0.02 + 0.98f64.powi(3);
    let want = 0.999 * pumps * fans;
    assert!((r["availability"].as_f64().unwrap() - want).abs() < 1e-12, "{r}");
    assert_eq!(r["blocks"].as_array().unwrap().len(), 6);
    assert_eq!(s.can_undo(), before);
}

#[test]
fn event_tree_outcomes_sum_to_the_initiator() {
    let mut s = open("reliability.coolingEventTree");
    let r = s.run("rel.outcomes", &json!({})).unwrap();
    let total: f64 = r["outcomes"].as_array().unwrap().iter().map(|o| o["frequency"].as_f64().unwrap()).sum();
    assert!((total - 0.1).abs() < 1e-12, "{r}");
}

#[test]
fn a_stale_rpn_is_flagged_and_its_fix_recomputes_it_in_one_step() {
    let mut s = open("reliability.pumpFmea");
    assert!(!s.issues.iter().any(|i| i.rule == "reliability.rpnCurrent"));
    let id = s.doc.model.elements.iter().find(|e| e.name == "Seal leak").unwrap().id.clone();
    s.run("element.setProperty", &json!({"id": id, "key": "severity", "value": 7})).unwrap();
    let issue = s.issues.iter().find(|i| i.rule == "reliability.rpnCurrent").cloned().expect("the stale RPN is flagged");
    let fix = issue.fixes.first().unwrap();
    s.run(&fix.command, &fix.params).unwrap();
    assert_eq!(s.doc.model.element(&id).unwrap().prop("rpn").and_then(|v| v.as_f64()), Some(84.0));
    assert!(!s.issues.iter().any(|i| i.rule == "reliability.rpnCurrent"));
    s.run("edit.undo", &json!({})).unwrap();
    assert!(s.issues.iter().any(|i| i.rule == "reliability.rpnCurrent"), "undo brings the stale RPN back in one step");
    let ws = s.doc.model.elements.iter().find(|e| e.name == "Cooling pump FMEA").unwrap().id.clone();
    let r = s.run("rel.rpn", &json!({"ids": [ws]})).unwrap();
    assert_eq!(r["updated"].as_array().unwrap().len(), 1);
    assert!(s.run("rel.rpn", &json!({"ids": ["nope"]})).is_err());
}

#[test]
fn rectangular_gates_redraw_in_place() {
    let mut s = open("reliability.pumpFaultTree");
    let centres = |s: &Session| {
        s.doc.page().unwrap().shapes.iter().map(|sh| (sh.id.clone(), sh.x + sh.w / 2.0, sh.y + sh.h / 2.0, sh.stencil.clone())).collect::<Vec<_>>()
    };
    let before = centres(&s);
    s.run("variant.set", &json!({"variant": "reliability.rectangular"})).unwrap();
    let after = centres(&s);
    assert!(after.iter().any(|(_, _, _, st)| st == "reliability.orRect"), "{after:?}");
    for (a, b) in before.iter().zip(&after) {
        assert!((a.1 - b.1).abs() < 1e-6 && (a.2 - b.2).abs() < 1e-6, "{} moved", a.0);
    }
}
