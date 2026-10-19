//! Tests of the sequence commands: one undo step each, the template's order, bound messages,
//! a clean template, the layout primitive, page copies, junk params, and the text and SVG
//! goldens of the template page.

use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_diagram::sequence::{Event, Part};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn template() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "uml.sequence"}));
    s
}

fn order(s: &mut Session) -> Vec<String> {
    let o = run(s, "sequence.order", json!({}));
    o["events"].as_array().unwrap().iter().map(|e| format!("{} {}", e["kind"].as_str().unwrap(), e["id"].as_str().unwrap())).collect()
}

#[test]
fn every_command_is_one_undo_step_and_the_seed_gives_the_template_order() {
    let mut s = Session::new(Document::new());
    let steps: Vec<(&str, Value)> = vec![
        ("sequence.new", json!({})),
        ("sequence.addLifeline", json!({"name": "Client", "id": "client", "actor": true})),
        ("sequence.addLifeline", json!({"name": "Orders API", "id": "api"})),
        ("sequence.addLifeline", json!({"name": "Database", "id": "db"})),
        ("sequence.addMessage", json!({"from": "client", "to": "api", "label": "GET /orders/42", "id": "m1"})),
        ("sequence.addMessage", json!({"from": "api", "to": "db", "label": "query", "id": "m2"})),
        ("sequence.reply", json!({"call": "m2", "label": "rows", "id": "m3"})),
        ("sequence.reply", json!({"call": "m1", "label": "200 OK", "id": "m4"})),
        ("sequence.addMessage", json!({"from": "api", "to": "client", "sort": "reply", "label": "404 Not Found", "id": "m5"})),
        ("sequence.wrap", json!({"id": "f1", "ids": ["m4", "m5"], "operator": "alt", "guard": "found"})),
        ("sequence.addOperand", json!({"id": "f1", "before": "m5", "guard": "else"})),
        ("sequence.addNote", json!({"text": "cached", "over": "db", "side": "right", "id": "n1", "after": "m3"})),
        ("sequence.move", json!({"id": "n1", "before": "m2"})),
        ("sequence.setFragment", json!({"id": "f1", "guards": ["found", "missing"]})),
        ("sequence.setSort", json!({"id": "m2", "sort": "async"})),
        ("sequence.setSort", json!({"id": "m2", "sort": "sync"})),
        ("sequence.layout", json!({"row": 40})),
    ];
    for (id, params) in &steps {
        let before = s.doc.clone();
        run(&mut s, id, params.clone());
        assert_ne!(s.doc, before, "{id} changed the document");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "{id} is one undo step");
        run(&mut s, "edit.redo", json!({}));
    }
    assert_eq!(
        order(&mut s),
        ["message m1", "note n1", "message m2", "message m3", "open f1", "message m4", "else f1", "message m5", "close f1"].map(String::from)
    );
    // The template seed gives the same order.
    let mut t = template();
    assert_eq!(
        order(&mut t),
        ["message m1", "message m2", "message m3", "open f1", "message m4", "else f1", "message m5", "close f1"].map(String::from)
    );
    let o = run(&mut t, "sequence.order", json!({}));
    let m4 = &o["events"][4];
    assert_eq!(
        (m4["from"].as_str(), m4["to"].as_str(), m4["sort"].as_str(), m4["fragment"].as_str(), m4["operand"].as_u64(), m4["depth"].as_u64()),
        (Some("api"), Some("client"), Some("reply"), Some("f1"), Some(0), Some(1))
    );
    assert_eq!(o["events"][6]["operand"], 1);
    assert_eq!(o["findings"], json!([]));
    // Unwrap, remove an operand, and refusals that name the bad id.
    run(&mut s, "sequence.removeOperand", json!({"id": "f1", "operand": 1}));
    assert!(!order(&mut s).contains(&"else f1".to_string()));
    run(&mut s, "sequence.unwrap", json!({"id": "f1"}));
    assert!(s.doc.page().unwrap().shape("f1").is_none());
    for (id, params, needle) in [
        ("sequence.addMessage", json!({"from": "ghost", "to": "api"}), "ghost"),
        ("sequence.move", json!({"id": "m1"}), "before"),
        ("sequence.move", json!({"id": "m1", "before": "m2", "after": "m3"}), "at most one"),
        ("sequence.wrap", json!({"ids": ["m1", "m3"], "operator": "opt"}), "n1"),
        ("sequence.wrap", json!({"ids": ["m1"], "operator": "maybe"}), "maybe"),
        ("sequence.reply", json!({"call": "nope"}), "nope"),
        ("sequence.addOperand", json!({"id": "m1", "before": "m2"}), "m1"),
    ] {
        let e = s.run(id, &params).unwrap_err().to_string();
        assert!(e.contains(needle), "{id}: {e}");
    }
}

#[test]
fn messages_are_bound_to_uml_message_relationships_with_their_sort() {
    let s = template();
    let page = s.doc.page().unwrap();
    for (id, sort) in [("m1", "synchCall"), ("m3", "reply"), ("m5", "reply")] {
        let c = page.connector(id).unwrap();
        let r = s.doc.model.relationship(c.relationship.as_deref().unwrap()).unwrap();
        assert_eq!(r.type_id, "uml.Message");
        assert_eq!(r.properties.get("sort").and_then(|v| v.as_str()), Some(sort), "{id}");
        assert_eq!(c.kind, if sort == "reply" { "uml.replyMessage" } else { "uml.syncMessage" });
    }
    let client = page.shape("client").unwrap();
    let e = s.doc.model.element(client.element.as_deref().unwrap()).unwrap();
    assert_eq!((e.type_id.as_str(), e.name.as_str()), ("uml.ActorLifeline", "Client"));
    assert_eq!(page.view.as_ref().map(|v| v.viewpoint.as_str()), Some("uml.Sequence"));
}

#[test]
fn the_template_is_clean() {
    let mut s = template();
    let issues = run(&mut s, "validate.run", json!({}));
    let issues = issues["issues"].as_array().unwrap();
    assert!(issues.iter().all(|i| i["severity"] != "error"), "{issues:?}");
    assert!(issues.iter().all(|i| i["rule"] != "uml.messageOrder" && i["rule"] != "uml.repliesMatched"), "{issues:?}");
    // Moving a reply above its call is reported live.
    run(&mut s, "sequence.move", json!({"id": "m3", "before": "m2"}));
    let issues = run(&mut s, "validate.run", json!({}));
    assert!(issues["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "uml.repliesMatched"), "{issues}");
}

#[test]
fn the_layout_primitive_equals_sequence_layout() {
    let mut a = template();
    let mut b = template();
    for s in [&mut a, &mut b] {
        let page = s.doc.page_mut().unwrap();
        page.shape_mut("db").unwrap().x += 700.0;
    }
    run(&mut a, "layout.primitive", json!({"primitive": "sequence"}));
    run(&mut b, "sequence.layout", json!({}));
    assert_eq!(a.doc.pages, b.doc.pages);
    assert!(a.doc.page().unwrap().shape("db").unwrap().x < 700.0, "packed again");
}

#[test]
fn a_duplicated_page_keeps_its_order_and_fragments() {
    let mut s = template();
    let src = s.doc.page().unwrap().clone();
    run(&mut s, "page.duplicate", json!({}));
    let copy = s.doc.page().unwrap();
    let ev = &copy.sequence.as_ref().unwrap().events;
    assert_eq!(ev.len(), src.sequence.as_ref().unwrap().events.len());
    for e in ev {
        match e {
            Event::Message(m) => assert!(copy.connector(m).is_some() && src.connector(m).is_none(), "{m}"),
            other => assert!(copy.shape(other.id()).is_some() && src.shape(other.id()).is_none(), "{other:?}"),
        }
    }
    assert!(copy.shapes.iter().any(|x| matches!(&x.sequence, Some(Part::Fragment { operands, .. }) if operands.len() == 2)));
    let bars = |p: &vizcraft_diagram::Page| p.shapes.iter().filter(|x| matches!(x.sequence, Some(Part::Activation { .. }))).count();
    assert_eq!(bars(copy), bars(&src));
}

#[test]
fn junk_params_never_panic_on_a_sequence_page() {
    let reg = crate::cmd::registry();
    let junk = [
        json!(null),
        json!({}),
        json!({"id": "x".repeat(1000), "before": 5, "after": [], "operator": "zz", "ids": [null]}),
        json!({"from": "s1", "to": "s1", "sort": 9, "in": {"fragment": 3}}),
        json!({"from": "api", "to": "api", "sort": "lost", "in": {"fragment": "f1", "operand": 99}}),
        json!({"id": "f1", "ids": ["m1", "m1", "f1"], "operator": "loop", "covers": ["db", 5], "operand": 1e300}),
        json!({"id": "f1", "before": "f1", "after": "m4", "guards": vec!["a"; 40]}),
        json!({"call": "m3", "id": "m1", "name": "", "at": -1, "over": [], "row": f64::MAX, "columnGap": -1}),
        json!({"id": "m4", "sort": "found", "grammar": "zz", "page": "\u{0}"}),
        json!({"id": "m5", "in": {"fragment": "f1", "operand": 0}, "text": "x".repeat(70_000), "over": ["client", "db", "api"]}),
    ];
    for spec in reg.all().iter().filter(|s| s.id.starts_with("sequence.")) {
        for j in &junk {
            let mut s = template();
            let _ = s.run(spec.id, j);
            let page = s.doc.page().unwrap();
            assert!(page.shapes.iter().all(|x| x.x.is_finite() && x.y.is_finite() && x.w.is_finite() && x.h.is_finite()), "{}: {j}", spec.id);
        }
    }
}

#[test]
fn golden_mermaid_and_plantuml_text_of_the_template() {
    let mut s = template();
    let m = run(&mut s, "sequence.write", json!({"grammar": "mermaid"}));
    let want = "sequenceDiagram
  actor client as Client
  participant api as Orders API
  participant db as Database
  client->>+api: GET /orders/42
  api->>+db: query
  db-->>-api: rows
  alt found
    api-->>client: 200 OK
  else else
    api-->>-client: 404 Not Found
  end
";
    assert_eq!(m["text"], want);
    assert_eq!(m["warnings"], json!([]));
    let p = run(&mut s, "sequence.write", json!({"grammar": "plantuml"}));
    let want = "@startuml
actor \"Client\" as client
participant \"Orders API\" as api
participant \"Database\" as db
client -> api : GET /orders/42
activate api
api -> db : query
activate db
db --> api : rows
deactivate db
alt found
  api --> client : 200 OK
else else
  api --> client : 404 Not Found
  deactivate api
end
@enduml
";
    assert_eq!(p["text"], want);
    // The Mermaid text reads back to the same order and bars.
    let page = vizcraft_formats::mermaid::import(m["text"].as_str().unwrap()).unwrap().pages.remove(0);
    let kinds: Vec<&str> =
        page.sequence.as_ref().unwrap().events.iter().map(Event::kind).filter(|k| *k != "activate" && *k != "deactivate").collect();
    assert_eq!(kinds, ["message", "message", "message", "open", "message", "else", "message", "close"]);
    let bars = page.shapes.iter().filter(|x| matches!(x.sequence, Some(Part::Activation { .. }))).count();
    assert_eq!(bars, 2);
}

#[test]
fn golden_svg_of_a_call_and_its_reply() {
    let mut s = Session::new(Document::new());
    run(&mut s, "sequence.new", json!({}));
    run(&mut s, "sequence.addLifeline", json!({"name": "A", "id": "a"}));
    run(&mut s, "sequence.addLifeline", json!({"name": "B", "id": "b"}));
    run(&mut s, "sequence.addMessage", json!({"from": "a", "to": "b", "label": "call", "id": "m1"}));
    run(&mut s, "sequence.reply", json!({"call": "m1", "label": "done", "id": "m2"}));
    let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
    let body: String = svg.lines().filter(|l| l.contains("<path") || l.contains("<g id")).map(str::trim).collect::<Vec<_>>().join("\n");
    // Heads, dash segments, the activation bar on B, the call's filled head and the reply's
    // dashed line and open head.
    let want = r##"<g id="a">
<path d="M50 40 L130 40 L130 80 L50 80 Z M90 188" fill="#f2f6fb" stroke="#2b3a4a" stroke-width="1.5"/>
<path d="M90 80 L90 85.891 M90 89.818 L90 95.709 M90 99.636 L90 105.527 M90 109.455 L90 115.345 M90 119.273 L90 125.164 M90 129.091 L90 134.982 M90 138.909 L90 144.8 M90 148.727 L90 154.618 M90 158.545 L90 164.436 M90 168.364 L90 174.255 M90 178.182 L90 184.073" fill="none" stroke="#2b3a4a" stroke-width="1.5"/>
<g id="b">
<path d="M200 40 L280 40 L280 80 L200 80 Z M240 188" fill="#f2f6fb" stroke="#2b3a4a" stroke-width="1.5"/>
<path d="M240 80 L240 85.891 M240 89.818 L240 95.709 M240 99.636 L240 105.527 M240 109.455 L240 115.345 M240 119.273 L240 125.164 M240 129.091 L240 134.982 M240 138.909 L240 144.8 M240 148.727 L240 154.618 M240 158.545 L240 164.436 M240 168.364 L240 174.255 M240 178.182 L240 184.073" fill="none" stroke="#2b3a4a" stroke-width="1.5"/>
<g id="s7">
<path d="M235 134 L245 134 L245 170 L235 170 Z" fill="#ffffff" stroke="#2b3a4a" stroke-width="1"/>
<g id="m1">
<path d="M90 134 L223.875 134" fill="none" stroke="#34495e" stroke-width="1.25" stroke-linejoin="round"/>
<path d="M235 134 L223.875 138.45 L223.875 129.55 Z" fill="#34495e" stroke="#34495e" stroke-width="1.25" stroke-linejoin="round"/>
<g id="m2">
<path d="M235 170 L90 170" fill="none" stroke="#34495e" stroke-width="1.25" stroke-dasharray="5 3.75" stroke-linejoin="round"/>
<path d="M101.125 165.55 L90 170 L101.125 174.45" fill="none" stroke="#34495e" stroke-width="1.25" stroke-linejoin="round"/>"##;
    assert_eq!(body, want);
}

#[test]
fn native_round_trip() {
    let s = template();
    let bytes = vizcraft_formats::native::save(&s.doc).unwrap();
    let back = vizcraft_formats::native::load(&bytes).unwrap();
    assert_eq!(back.pages, s.doc.pages);
    assert_eq!(back.model, s.doc.model);
}

#[test]
fn deleting_a_lifeline_leaves_free_ends_the_dangling_rule_reports() {
    let mut s = template();
    run(&mut s, "shape.delete", json!({"ids": ["db"]}));
    let page = s.doc.page().unwrap();
    assert!(page.connector("m2").unwrap().end.shape().is_none());
    let events = &page.sequence.as_ref().unwrap().events;
    assert!(!events.contains(&Event::Message("m2".into())) && events.contains(&Event::Message("m1".into())));
    let issues = run(&mut s, "validate.run", json!({}));
    assert!(issues["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "uml.dangling"), "{issues}");
}

#[test]
fn the_move_and_activation_commands_are_one_undo_step_each() {
    let mut s = template();
    let steps: Vec<(&str, Value)> = vec![
        ("sequence.moveDown", json!({"id": "m1"})),
        ("sequence.moveUp", json!({"id": "m1"})),
        ("sequence.moveRight", json!({"id": "client"})),
        ("sequence.moveLeft", json!({"id": "client"})),
        ("sequence.setActivations", json!({"mode": "explicit"})),
        ("sequence.setActivations", json!({"mode": "auto"})),
        ("sequence.setActivation", json!({"id": "m1", "activate": true, "deactivate": true})),
        ("sequence.setActivation", json!({"id": "m1", "activate": false})),
    ];
    for (id, params) in &steps {
        let before = s.doc.clone();
        let out = run(&mut s, id, params.clone());
        assert_ne!(s.doc, before, "{id} {params} changed the document: {out}");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "{id} is one undo step");
        run(&mut s, "edit.redo", json!({}));
    }
    // The order came back; the activation run after m1 keeps only the deactivate.
    assert_eq!(order(&mut s)[..3], ["message m1", "deactivate client", "message m2"].map(String::from));
    let x = |s: &Session, id: &str| s.doc.page().unwrap().shape(id).map(|v| v.x + v.w / 2.0).unwrap();
    // Alt+Right swaps the columns: Client and Orders API trade places, heads stay on their row.
    let (c, a, y) = (x(&s, "client"), x(&s, "api"), s.doc.page().unwrap().shape("api").unwrap().y);
    run(&mut s, "sequence.moveRight", json!({"id": "client"}));
    assert!(x(&s, "client") > x(&s, "api") && x(&s, "api") <= c + 1e-9 && x(&s, "client") >= a - 1e-9);
    assert_eq!(s.doc.page().unwrap().shape("api").unwrap().y, y);
    // The selection is the default id; refusals name what is wrong.
    s.sel = crate::Selection::single("m2");
    run(&mut s, "sequence.moveDown", json!({}));
    assert_eq!(order(&mut s)[2..4], ["message m3", "message m2"].map(String::from));
    for (id, params, needle) in [
        ("sequence.moveLeft", json!({"id": "api"}), "leftmost"),
        ("sequence.moveRight", json!({"id": "db"}), "rightmost"),
        ("sequence.moveUp", json!({"id": "m1"}), "already first"),
        ("sequence.moveDown", json!({"id": "f1"}), "already last"),
        ("sequence.moveLeft", json!({"id": "m1"}), "not a lifeline"),
        ("sequence.setActivations", json!({"mode": "sometimes"}), "sometimes"),
        ("sequence.setActivation", json!({"id": "m1"}), "activate"),
        ("sequence.setActivation", json!({"id": "db", "activate": true}), "not a message"),
    ] {
        let e = s.run(id, &params).unwrap_err().to_string();
        assert!(e.contains(needle), "{id}: {e}");
    }
}

#[test]
fn move_up_at_an_operand_start_leaves_the_fragment() {
    let mut s = template();
    // m5 starts the else operand: up goes to the end of the found operand.
    run(&mut s, "sequence.moveUp", json!({"id": "m5"}));
    assert_eq!(order(&mut s)[3..], ["open f1", "message m4", "message m5", "else f1", "close f1"].map(String::from));
    // m4 starts the first operand: up leaves the fragment.
    run(&mut s, "sequence.moveUp", json!({"id": "m4"}));
    assert_eq!(order(&mut s)[3..5], ["message m4", "open f1"].map(String::from));
    // Down at the end of the last operand leaves it too.
    let mut t = template();
    run(&mut t, "sequence.moveDown", json!({"id": "m5"}));
    assert_eq!(order(&mut t).last().map(String::as_str), Some("message m5"));
}

#[test]
fn alt_arrow_shortcuts_are_unique() {
    let reg = crate::cmd::registry();
    for (key, id) in
        [("Alt+Up", "sequence.moveUp"), ("Alt+Down", "sequence.moveDown"), ("Alt+Left", "sequence.moveLeft"), ("Alt+Right", "sequence.moveRight")]
    {
        let owners: Vec<&str> = reg.all().iter().filter(|c| c.shortcut.split(" / ").any(|k| k == key)).map(|c| c.id).collect();
        assert_eq!(owners, vec![id], "{key}");
    }
}

#[test]
fn shape_add_of_a_lifeline_master_joins_the_interaction() {
    let mut s = template();
    let out = run(&mut s, "shape.add", json!({"stencil": "uml.lifeline", "x": 900, "y": 40, "text": "Cache"}));
    let id = out["id"].as_str().unwrap().to_string();
    let page = s.doc.page().unwrap();
    assert!(matches!(page.shape(&id).unwrap().sequence, Some(Part::Lifeline { destroyed: false })));
    // A message-type connector drawn between two lifelines joins the order as that sort.
    let out = run(&mut s, "connector.add", json!({"from": {"shape": "api"}, "to": {"shape": id}, "kind": "uml.asyncMessage", "text": "warm"}));
    let c = out["id"].as_str().unwrap().to_string();
    let o = run(&mut s, "sequence.order", json!({}));
    let row = o["events"].as_array().unwrap().iter().find(|e| e["id"] == c.as_str()).cloned().unwrap();
    assert_eq!((row["kind"].as_str(), row["sort"].as_str()), (Some("message"), Some("async")));
}

#[test]
fn golden_message_labels_sit_above_their_lines_in_svg() {
    let mut s = Session::new(Document::new());
    run(&mut s, "sequence.new", json!({}));
    run(&mut s, "sequence.addLifeline", json!({"name": "A", "id": "a"}));
    run(&mut s, "sequence.addLifeline", json!({"name": "B", "id": "b"}));
    run(&mut s, "sequence.addMessage", json!({"from": "a", "to": "b", "label": "call", "id": "m1"}));
    let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
    let group: Vec<&str> = svg.lines().map(str::trim).skip_while(|l| *l != r#"<g id="m1">"#).take_while(|l| *l != "</g>").collect();
    let attr = |l: &str, k: &str| -> f64 {
        let at = l.find(&format!(" {k}=\"")).unwrap() + k.len() + 3;
        l[at..].split('"').next().unwrap().parse().unwrap()
    };
    let rect = group.iter().find(|l| l.starts_with("<rect")).unwrap();
    let line = group.iter().find(|l| l.starts_with("<path d=\"M")).unwrap();
    let line_y: f64 = line.split_whitespace().nth(2).unwrap().trim_end_matches('"').parse().unwrap();
    assert!(attr(rect, "y") + attr(rect, "height") < line_y, "{rect} above {line}");
}

/// The page's stems, bars, fragment tabs and guards, and message label boxes.
fn template_boxes(s: &Session) -> (Vec<(String, f64, f64, f64)>, Vec<(String, vizcraft_diagram::Rect)>) {
    use vizcraft_diagram::container::Decor;
    let page = s.doc.page().unwrap();
    let mut lines = Vec::new();
    let mut boxes = Vec::new();
    for x in &page.shapes {
        match &x.sequence {
            Some(Part::Lifeline { .. }) => {
                let head = if x.kind == vizcraft_diagram::ShapeKind::ActorLifeline {
                    vizcraft_diagram::sequence::ACTOR_HEAD
                } else {
                    vizcraft_diagram::sequence::HEAD
                };
                lines.push((format!("stem {}", x.id), x.x + x.w / 2.0, x.y + head, x.y + x.h));
            }
            Some(Part::Activation { .. }) => {
                for bx in [x.x, x.x + x.w] {
                    lines.push((format!("bar {}", x.id), bx, x.y, x.y + x.h));
                }
            }
            Some(Part::Fragment { .. }) => {
                for d in vizcraft_diagram::sequence::fragment_decor(x) {
                    match d {
                        Decor::Line { points, dashed: false, .. } => boxes.push(("tab".into(), vizcraft_diagram::Rect::bounding(points).unwrap())),
                        Decor::Text { center, text, size, .. } => {
                            let w = text.chars().count() as f64 * size * 0.6;
                            boxes.push((
                                text,
                                vizcraft_diagram::Rect::new(center.x - w / 2.0, center.y - size / 2.0, center.x + w / 2.0, center.y + size / 2.0),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for c in &page.connectors {
        let g = vizcraft_diagram::connector_geometry(page, c);
        if let Some(r) = vizcraft_diagram::sequence::label_rect(c, g.label_pos) {
            boxes.push((format!("label {}", c.id), r));
        }
    }
    (lines, boxes)
}

#[test]
fn on_the_template_no_stem_or_bar_crosses_a_tab_guard_or_label_and_operands_are_even() {
    let mut s = template();
    for step in 0..3 {
        let (lines, boxes) = template_boxes(&s);
        assert!(boxes.iter().filter(|b| b.0 == "tab").count() == 1 && boxes.iter().any(|b| b.0 == "[else]"), "{boxes:?}");
        for (what, x, y0, y1) in &lines {
            for (name, r) in &boxes {
                let hit = *x > r.x0 && *x < r.x1 && *y1 > r.y0 && *y0 < r.y1;
                assert!(!hit, "step {step}: {what} crosses {name} {r:?}");
            }
        }
        // Evenly spaced operands: each first row one row below its guard line, the separator and
        // the bottom half a row below the last row.
        let page = s.doc.page().unwrap();
        let row = page.sequence.as_ref().unwrap().row;
        let f = page.shape("f1").unwrap();
        let Some(Part::Fragment { operands, .. }) = &f.sequence else { panic!() };
        let y = |id: &str| page.connector(id).and_then(|c| vizcraft_diagram::connector_geometry(page, c).polyline.first().map(|p| p.y)).unwrap();
        let sep = f.y + operands[1].at;
        let gaps = [y("m4") - f.y, sep - y("m4"), y("m5") - sep, f.y + f.h - y("m5")];
        for (got, want) in gaps.iter().zip([row, row / 2.0, row, row / 2.0]) {
            assert!((got - want).abs() < 1e-6, "step {step}: {gaps:?}");
        }
        // Again with the guards longer and the rows taller.
        run(&mut s, "sequence.setFragment", json!({"id": "f1", "guards": ["order found in the cache", "else"]}));
        run(&mut s, "sequence.layout", json!({"row": 48 + 8 * step}));
    }
}
