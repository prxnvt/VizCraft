//! The routing standard over every shipped example and document type: built by `example.open` or
//! `template.new` (seed, layout, routes, title block and key), each page has no connector near a shape that is not its end, none
//! along its own end's edge, no two connectors on one run and no label on a shape or another label
//! (see `vizcraft_diagram::route_quality`). A failure lists every finding of every template.

use serde_json::{Value, json};
use vizcraft_diagram::route_quality::check;
use vizcraft_engine::{Document, Session};

/// Every example, then every document type, each with the command that opens it.
fn templates(s: &mut Session) -> Vec<(&'static str, &'static str, String)> {
    let mut out = Vec::new();
    for (list, key, command, param) in
        [("example.list", "examples", "example.open", "example"), ("template.list", "templates", "template.new", "template")]
    {
        let r = s.run(list, &json!({})).unwrap();
        out.extend(r[key].as_array().unwrap().iter().filter_map(|t: &Value| t["id"].as_str().map(|id| (command, param, id.to_string()))));
    }
    out
}

#[test]
fn every_shipped_template_routes_cleanly() {
    let mut s = Session::new(Document::new());
    let ids = templates(&mut s);
    assert!(ids.iter().filter(|(c, _, _)| *c == "example.open").count() > 10, "{ids:?}");
    let mut report = Vec::new();
    for (command, param, id) in &ids {
        let mut s = Session::new(Document::new());
        s.run(command, &json!({ *param: id })).unwrap_or_else(|e| panic!("{id}: {e}"));
        for page in &s.doc.pages {
            for f in check(page) {
                report.push(format!("{id} ({}): {f}", page.name));
            }
        }
    }
    assert!(report.is_empty(), "{} findings:\n{}", report.len(), report.join("\n"));
}
