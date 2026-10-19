//! Tests of the key commands: rows follow the page, refresh is idempotent, exclusions hold, the
//! key moves as one and the layouts leave it alone.

use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_diagram::key::{KEY_EXT, is_key, is_key_connector};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A user calling a compute node that reads a database, with steps 1 and 2.
fn topology() -> Session {
    let mut s = Session::new(Document::new());
    let u = run(&mut s, "element.create", json!({"type": "cloud.User", "name": "Visitor", "x": 0, "y": 0}));
    let c = run(&mut s, "element.create", json!({"type": "cloud.Compute", "name": "App", "x": 300, "y": 0}));
    let d = run(&mut s, "element.create", json!({"type": "cloud.Database", "name": "Orders", "x": 600, "y": 0}));
    let r1 = run(&mut s, "relationship.create", json!({"from": u["id"], "to": c["id"], "name": "browse"}));
    let r2 = run(&mut s, "relationship.create", json!({"from": c["id"], "to": d["id"], "name": "query"}));
    run(&mut s, "connector.setStep", json!({"id": r1["id"], "step": 1}));
    run(&mut s, "connector.setStep", json!({"id": r2["id"], "step": 2}));
    s
}

fn labels(r: &Value) -> Vec<(String, String)> {
    r["rows"].as_array().unwrap().iter().map(|x| (x["kind"].as_str().unwrap().to_string(), x["label"].as_str().unwrap().to_string())).collect()
}

#[test]
fn rows_follow_the_types_kinds_and_steps_on_the_page() {
    let mut s = topology();
    let r = run(&mut s, "key.insert", json!({}));
    let got = labels(&r);
    let want = [("type", "User"), ("type", "Compute"), ("type", "Database"), ("line", "Request"), ("step", "1  browse"), ("step", "2  query")];
    assert_eq!(got, want.map(|(a, b)| (a.to_string(), b.to_string())));
    assert_eq!(r["truncated"], false);
    let id = r["id"].as_str().unwrap().to_string();
    let page = s.doc.page().unwrap();
    let g = page.shape(&id).unwrap();
    assert!(is_key(g) && vizcraft_diagram::arrange::is_group(g));
    assert!(page.shapes.iter().filter(|sh| is_key(sh)).all(|sh| sh.element.is_none()));
    assert_eq!(page.connectors.iter().filter(|c| is_key_connector(c)).count(), 1, "one line sample");
    // Placed right of the drawing.
    let right = page.shapes.iter().filter(|sh| !is_key(sh)).map(|sh| sh.bounds().x1).fold(f64::MIN, f64::max);
    assert!(g.x >= right, "{} >= {right}", g.x);
    // The model rules see nothing new.
    let issues = run(&mut s, "validate.run", json!({}));
    assert!(
        issues["issues"].as_array().unwrap().iter().all(|i| !i["items"].as_array().unwrap().iter().any(|x| x.as_str().unwrap().starts_with(&id)))
    );
    let got = run(&mut s, "key.get", json!({}));
    assert_eq!((got["id"].as_str(), got["rows"].as_array().unwrap().len()), (Some(id.as_str()), 6));
}

#[test]
fn insert_then_refresh_is_idempotent_and_keeps_the_corner() {
    let mut s = topology();
    run(&mut s, "key.insert", json!({"x": 900, "y": 500, "title": "Legend"}));
    let once = s.doc.page().unwrap().clone();
    let r = run(&mut s, "key.refresh", json!({}));
    assert_eq!((r["pages"].as_u64(), r["rows"].as_u64()), (Some(1), Some(6)));
    let again = s.doc.page().unwrap();
    let count = |p: &vizcraft_diagram::Page| (p.shapes.len(), p.connectors.len());
    assert_eq!(count(again), count(&once));
    let g = again.shapes.iter().find(|sh| is_key(sh) && vizcraft_diagram::arrange::is_group(sh)).unwrap();
    assert_eq!((g.x, g.y), (900.0, 500.0));
    assert!(again.shapes.iter().any(|sh| is_key(sh) && sh.text == "Legend"), "the title is kept");
    assert_eq!(run(&mut s, "key.refresh", json!({"all": true}))["pages"], 1);
    let removed = run(&mut s, "key.remove", json!({}));
    assert!(removed["removed"].as_u64().unwrap() > 10);
    assert!(s.doc.page().unwrap().shapes.iter().all(|sh| !sh.ext.contains_key(KEY_EXT)));
    assert_eq!(run(&mut s, "key.get", json!({}))["id"], Value::Null);
}

#[test]
fn exclude_and_steps_off_leave_rows_out_and_an_empty_page_gets_no_key() {
    let mut s = topology();
    let r = run(&mut s, "key.insert", json!({"exclude": ["cloud.User", "cloud.request"], "steps": false}));
    assert_eq!(labels(&r).iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Compute", "Database"]);
    // A refresh keeps the settings.
    run(&mut s, "key.refresh", json!({}));
    assert_eq!(run(&mut s, "key.get", json!({}))["rows"].as_array().unwrap().len(), 2);
    assert!(s.run("key.insert", &json!({"exclude": "cloud.User"})).is_err());
    assert!(s.run("key.insert", &json!({"x": 5})).is_err());
    let mut blank = Session::new(Document::new());
    let r = run(&mut blank, "key.insert", json!({}));
    assert_eq!((r["id"].clone(), r["rows"].as_array().unwrap().len()), (Value::Null, 0));
}

#[test]
fn moving_the_key_carries_the_line_samples_and_layouts_leave_it_alone() {
    let mut s = topology();
    let id = run(&mut s, "key.insert", json!({}))["id"].as_str().unwrap().to_string();
    let line_x = |s: &Session| {
        let page = s.doc.page().unwrap();
        let c = page.connectors.iter().find(|c| is_key_connector(c)).unwrap();
        let g = vizcraft_diagram::connector_geometry(page, c);
        g.polyline[0].x
    };
    let before = line_x(&s);
    run(&mut s, "shape.move", json!({"ids": [id], "dx": 100, "dy": 0}));
    assert!((line_x(&s) - before - 100.0).abs() < 1e-6, "the line sample moved with the key");
    let frame = |s: &Session| {
        let g = s.doc.page().unwrap().shape(&id).unwrap();
        (g.x, g.y, g.w, g.h)
    };
    let at = frame(&s);
    run(&mut s, "layout.full", json!({"algorithm": "layered", "direction": "TB"}));
    assert_eq!(frame(&s), at, "layout.full leaves the key alone");
    run(&mut s, "layout.primitive", json!({"primitive": "nested", "roles": {"containers": []}}));
    assert_eq!(frame(&s), at, "layout.primitive leaves the key alone");
}

const TOPOLOGY_TEMPLATES: [&str; 6] = ["cloud.topology", "cloud.cluster", "network.logical", "network.physical", "dfd.context", "dfd.threat"];

#[test]
fn the_topology_templates_build_in_one_step_with_no_error() {
    for id in TOPOLOGY_TEMPLATES {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "template.new", json!({"template": id}));
        assert_eq!(r["skipped"], json!([]), "{id}");
        assert_eq!(s.undo_depth(), 1, "{id}");
        let issues = run(&mut s, "validate.run", json!({}));
        let errors: Vec<&Value> = issues["issues"].as_array().unwrap().iter().filter(|i| i["severity"] == "error").collect();
        assert!(errors.is_empty(), "{id}: {errors:?}");
    }
}

#[test]
fn the_cloud_topology_has_steps_a_key_nested_nodes_and_a_balancer_spof() {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "cloud.topology"}));
    let page = s.doc.page().unwrap();
    let mut steps: Vec<u16> = page.connectors.iter().map(|c| c.step).filter(|n| *n > 0).collect();
    steps.sort();
    assert_eq!(steps, [1, 2, 3, 4, 5]);
    assert!(page.shapes.iter().any(|sh| is_key(sh) && vizcraft_diagram::arrange::is_group(sh)), "a key");
    // Every element but the visitor sits in its container, in the drawing and in the model.
    for e in &s.doc.model.elements {
        let sh = page.shapes.iter().find(|sh| sh.element.as_deref() == Some(e.id.as_str())).unwrap();
        if e.type_id == "cloud.User" || e.type_id == "cloud.Region" {
            assert!(sh.parent.is_none(), "{} at the top", e.name);
            continue;
        }
        let parent = sh.parent.as_deref().and_then(|p| page.shape(p)).unwrap_or_else(|| panic!("{} has no container", e.name));
        assert_eq!(parent.element.as_deref(), e.owner.as_deref(), "{} is owned by the container it is drawn in", e.name);
    }
    let lb = s.doc.model.elements.iter().find(|e| e.type_id == "cloud.LoadBalancer").unwrap().id.clone();
    let spof = run(&mut s, "analyze.spof", json!({}));
    assert!(spof["elements"].as_array().unwrap().iter().any(|x| x == &json!(lb)), "{spof}");
    let text = run(&mut s, "topology.toMermaid", json!({}));
    assert!(text["text"].as_str().unwrap().contains("[Balancer] in "), "{text}");
}

/// The preview SVG of `template` (in `variant`) against `goldens/<name>`; `VIZCRAFT_BLESS=1`
/// rewrites it.
fn golden(name: &str, template: &str, variant: &str) {
    let mut s = Session::new(Document::new());
    let r = run(&mut s, "template.preview", json!({"template": template, "variant": variant}));
    let svg = r["svg"].as_str().unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cmd/key/goldens").join(name);
    if std::env::var("VIZCRAFT_BLESS").is_ok() {
        std::fs::write(&path, svg).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(svg == want, "{name} differs from its golden; run with VIZCRAFT_BLESS=1 after checking it");
    assert!(svg.contains(">Key</tspan>") || svg.contains(">Key<"), "{name} shows the key");
}

#[test]
fn template_previews_match_their_goldens() {
    golden("cloud-topology.svg", "cloud.topology", "");
    golden("dfd-gane-sarson.svg", "dfd.threat", "dfd.gane_sarson");
}
