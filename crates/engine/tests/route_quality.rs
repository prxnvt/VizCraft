//! The routing standard over every shipped template: built by `template.new` (seed, layout,
//! routes, title block and key), each page has no connector near a shape that is not its end, none
//! along its own end's edge, no two connectors on one run and no label on a shape or another label
//! (see `vizcraft_diagram::route_quality`). A failure lists every finding of every template.

use serde_json::{Value, json};
use vizcraft_diagram::route_quality::check;
use vizcraft_engine::{Document, Session};

fn templates(s: &mut Session) -> Vec<String> {
    let r = s.run("template.list", &json!({})).unwrap();
    r["templates"].as_array().unwrap().iter().filter_map(|t: &Value| t["id"].as_str().map(str::to_string)).collect()
}

#[test]
fn every_shipped_template_routes_cleanly() {
    let mut s = Session::new(Document::new());
    let ids = templates(&mut s);
    assert!(ids.len() > 10, "{ids:?}");
    let mut report = Vec::new();
    for id in &ids {
        let mut s = Session::new(Document::new());
        s.run("template.new", &json!({"template": id})).unwrap_or_else(|e| panic!("{id}: {e}"));
        for page in &s.doc.pages {
            for f in check(page) {
                report.push(format!("{id} ({}): {f}", page.name));
            }
        }
    }
    assert!(report.is_empty(), "{} findings:\n{}", report.len(), report.join("\n"));
}
