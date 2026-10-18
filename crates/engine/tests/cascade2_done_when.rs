//! The Cascade 2 done-when list, run through commands only (the control channel and MCP send the
//! same ids): a multi-page cross-functional flowchart with swimlanes, layers, a theme, typed shape
//! data and aligned shapes, exported to PNG and PDF.

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    match s.run(id, &v) {
        Ok(r) => r,
        Err(e) => panic!("{id} {v}: {e}"),
    }
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap_or_default().to_string()
}

fn pdf_pages(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.split("/Count").skip(1).filter_map(|r| r.trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).max().unwrap_or(0)
}

#[test]
fn cascade_2_done_when() {
    let mut s = Session::new(Document::new());

    // Two pages, the first renamed, editing the first.
    let first = s.doc.pages[0].id.clone();
    run(&mut s, "page.rename", json!({"page": first, "name": "Intake"}));
    run(&mut s, "page.add", json!({"name": "Approval"}));
    run(&mut s, "page.setActive", json!({"page": "Intake"}));

    // A cross-functional flowchart: three horizontal lanes and a step in each.
    let set = run(&mut s, "lane.addSet", json!({"x": 40, "y": 40, "lanes": 3, "orientation": "horizontal"}));
    let lanes: Vec<String> = set["lanes"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect();
    assert_eq!(lanes.len(), 3);
    let mut steps = Vec::new();
    for (i, lane) in lanes.iter().enumerate() {
        let x = 200.0 + 60.0 * i as f64;
        let step = id(&run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": x, "y": 0, "text": format!("Step {i}")})));
        run(&mut s, "container.addMember", json!({"container": lane, "ids": [step]}));
        steps.push(step);
    }
    for pair in steps.windows(2) {
        run(&mut s, "connector.add", json!({"from": {"shape": pair[0]}, "to": {"shape": pair[1]}, "route": "orthogonal"}));
    }

    // Layers, a theme, typed shape data and aligned shapes.
    run(&mut s, "layer.add", json!({"name": "Review"}));
    run(&mut s, "layer.assign", json!({"ids": [steps[1]], "layer": "Review"}));
    let themes = run(&mut s, "design.themes", json!({}));
    let theme = themes["themes"][0]["id"].as_str().unwrap().to_string();
    run(&mut s, "design.setTheme", json!({"theme": theme}));
    run(&mut s, "data.set", json!({"ids": [steps[0]], "key": "costCentre", "value": "Operations"}));
    run(&mut s, "data.set", json!({"ids": [steps[0]], "key": "minutes", "value": 15, "type": "number"}));
    run(&mut s, "arrange.align", json!({"ids": steps, "edge": "left"}));

    let page = s.doc.page().unwrap();
    assert_eq!(page.name, "Intake");
    let xs: Vec<f64> = steps.iter().map(|i| page.shape(i).unwrap().x).collect();
    assert!(xs.iter().all(|x| (x - xs[0]).abs() < 1e-6), "aligned left: {xs:?}");
    for (step, lane) in steps.iter().zip(&lanes) {
        assert_eq!(page.shape(step).unwrap().parent.as_deref(), Some(lane.as_str()), "each step stays in its lane");
    }
    assert!(page.layers.iter().any(|l| l.name == "Review"));
    assert_eq!(page.shape(&steps[1]).unwrap().layers.len(), 1);
    assert_eq!(s.doc.theme.id, theme);
    let rows = run(&mut s, "data.get", json!({"id": steps[0]})).to_string();
    assert!(rows.contains("Operations") && rows.contains("minutes"), "{rows}");

    // Export: PNG of every page and one PDF with both pages.
    let dir = std::env::temp_dir().join(format!("vizcraft-c2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("flow.png");
    let r = run(&mut s, "file.export", json!({"path": png.to_string_lossy(), "pages": "all"}));
    let paths: Vec<String> = r["paths"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
    assert_eq!(paths.len(), 2, "{r}");
    for p in &paths {
        assert!(std::fs::read(p).unwrap().starts_with(b"\x89PNG"), "{p}");
    }
    let pdf = dir.join("flow.pdf");
    run(&mut s, "file.export", json!({"path": pdf.to_string_lossy()}));
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(pdf_pages(&bytes), 2);

    // The whole document survives a save and load.
    let saved = dir.join("flow.vizcraft");
    run(&mut s, "file.saveAs", json!({"path": saved.to_string_lossy()}));
    let mut t = Session::new(Document::new());
    run(&mut t, "file.open", json!({"path": saved.to_string_lossy()}));
    assert_eq!(t.doc.pages.len(), 2);
    assert_eq!(t.doc.theme.id, theme);
    let _ = std::fs::remove_dir_all(dir);
}
