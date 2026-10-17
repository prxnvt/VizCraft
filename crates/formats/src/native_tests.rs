//! Tests for the native file: version 3 with the model section, older versions and bare
//! documents, and tolerant or refused model sections.

use serde_json::{Value, json};
use vizcraft_diagram::vizcraft_model::{Element, PackRef, PropValue, Relationship, SCHEMA_VERSION, View};
use vizcraft_diagram::{Connector, Document, End, Page, Shape, ShapeKind};

use super::{VERSION, load, save};
use crate::FormatError;

/// A two-page flowchart model: three elements (one owned by another), two relationships, every
/// kind of property value, a page view, bound shapes and connectors, and a free shape.
fn modelled() -> Document {
    let mut doc = Document::new();
    doc.title = "Model test".into();
    doc.model.use_pack(PackRef { id: "flowchart".into(), version: "1.0.0".into() });
    let mut start = Element::new("e1", "flowchart.Terminator");
    start.name = "Start".into();
    start.tag = "T1".into();
    let mut step = Element::new("e2", "flowchart.Process");
    step.name = "Mix \"batch\" & heat".into();
    step.tag = "S1".into();
    step.properties.insert("duration".into(), PropValue::Number(4.5));
    step.properties.insert("manual".into(), PropValue::Bool(true));
    step.properties.insert("owner".into(), PropValue::Text("Line 2".into()));
    let mut sub = Element::new("e3", "flowchart.PredefinedProcess");
    sub.owner = Some("e2".into());
    sub.properties.insert("reference".into(), PropValue::Text("SOP-7".into()));
    for e in [start, step, sub] {
        doc.model.add_element(e).unwrap();
    }
    let mut flow = Relationship::new("r1", "flowchart.Flow", "e1", "e2");
    flow.name = "go".into();
    flow.properties.insert("condition".into(), PropValue::Text("ready".into()));
    flow.properties.insert("weight".into(), PropValue::Number(-2.0));
    flow.properties.insert("loop".into(), PropValue::Bool(false));
    doc.model.add_relationship(flow).unwrap();
    doc.model.add_relationship(Relationship::new("r2", "flowchart.Flow", "e2", "e3")).unwrap();

    let page = doc.page_mut().unwrap();
    page.view = Some(View { viewpoint: "flowchart.Flowchart".into(), layout: "layered".into() });
    let mut s1 = Shape::new("s1", ShapeKind::Terminator, 100.0, 40.0, 120.0, 60.0);
    s1.element = Some("e1".into());
    s1.text = "Start".into();
    let mut s2 = Shape::new("s2", ShapeKind::Rectangle, 100.0, 160.0, 120.0, 60.0);
    s2.element = Some("e2".into());
    let mut s3 = Shape::new("s3", ShapeKind::PredefinedProcess, 100.0, 280.0, 120.0, 60.0);
    s3.element = Some("e3".into());
    let free = Shape::new("s4", ShapeKind::Ellipse, 400.0, 40.0, 80.0, 80.0);
    page.shapes.extend([s1, s2, s3, free]);
    let mut c1 = Connector::new("c1", End::dynamic("s1"), End::dynamic("s2"));
    c1.relationship = Some("r1".into());
    c1.text = "go".into();
    let mut c2 = Connector::new("c2", End::dynamic("s2"), End::dynamic("s3"));
    c2.relationship = Some("r2".into());
    let c3 = Connector::new("c3", End::dynamic("s3"), End::dynamic("s4"));
    page.connectors.extend([c1, c2, c3]);

    // A second page shows e2 again, without a viewpoint.
    let mut p2 = Page::new("p2", "Detail");
    let mut s5 = Shape::new("s5", ShapeKind::Rectangle, 0.0, 0.0, 120.0, 60.0);
    s5.element = Some("e2".into());
    p2.shapes.push(s5);
    doc.pages.push(p2);
    doc.id_seq = 20;
    doc
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn version_3_round_trip_keeps_model_and_views() {
    let doc = modelled();
    let bytes = save(&doc).unwrap();
    let loaded = load(&bytes).unwrap();
    assert_eq!(loaded, doc);
    assert_eq!(loaded.model.elements.len(), 3);
    assert_eq!(loaded.model.relationships.len(), 2);
    assert_eq!(loaded.shape("s5").and_then(|s| s.element.as_deref()), Some("e2"));
    assert_eq!(loaded.connector("c1").and_then(|c| c.relationship.as_deref()), Some("r1"));
    // Saving the loaded document again gives the same bytes.
    assert_eq!(save(&loaded).unwrap(), bytes);
}

#[test]
fn version_3_layout_and_key_order() {
    let bytes = save(&modelled()).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let at = |key: &str| text.find(&format!("\n  \"{key}\":")).unwrap_or_else(|| panic!("no top-level {key} in {text}"));
    assert!(at("format") < at("version") && at("version") < at("model") && at("model") < at("document"), "{text}");
    let v = parse(&bytes);
    assert_eq!(v["version"], json!(3));
    assert_eq!(v["model"]["schemaVersion"], json!(SCHEMA_VERSION));
    assert_eq!(v["model"]["packs"], json!([{"id": "flowchart", "version": "1.0.0"}]));
    assert_eq!(v["model"]["elements"][1]["properties"], json!({"duration": 4.5, "manual": true, "owner": "Line 2"}));
    assert_eq!(v["model"]["elements"][2]["owner"], json!("e2"));
    assert_eq!(v["document"]["pages"][0]["view"], json!({"viewpoint": "flowchart.Flowchart", "layout": "layered"}));
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["element"], json!("e1"));
    assert_eq!(v["document"]["pages"][0]["connectors"][0]["relationship"], json!("r1"));
    // The document's own serde form never carries the model.
    assert!(v["document"].get("model").is_none());
}

#[test]
fn free_drawing_omits_the_model_section() {
    let mut doc = Document::new();
    doc.page_mut().unwrap().shapes.push(Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["version"], json!(VERSION));
    assert!(v.get("model").is_none(), "{v}");
    let loaded = load(&bytes).unwrap();
    assert!(loaded.model.is_empty());
    assert_eq!(loaded, doc);
}

/// A version 2 file as Cascade 1 wrote it, typed by hand: one page, two shapes, one connector.
fn cascade_1_document() -> Value {
    json!({
        "title": "Plant notes",
        "core": {"author": "Ada", "revision": 2},
        "pages": [{
            "id": "p1",
            "name": "Page-1",
            "width": 1100,
            "height": 850,
            "shapes": [
                {"id": "s2", "stencil": "basic.rectangle", "kind": "rectangle", "x": 40, "y": 60, "w": 120, "h": 60, "text": "Pump"},
                {"id": "s3", "stencil": "basic.ellipse", "kind": "ellipse", "x": 300, "y": 60, "w": 80, "h": 80, "angleDeg": 15, "text": "Tank"}
            ],
            "connectors": [
                {"id": "c4", "start": {"shape": "s2"}, "end": {"shape": "s3", "point": 1}, "text": "feed"}
            ]
        }],
        "idSeq": 4
    })
}

#[test]
fn version_2_file_loads_as_free_drawing_with_identical_shapes() {
    let file = json!({"format": "vizcraft", "version": 2, "document": cascade_1_document()});
    let doc = load(file.to_string().as_bytes()).unwrap();
    // The diagram crate's own loader is what read version 2 files before the model existed.
    let old: Document = serde_json::from_value(cascade_1_document()).unwrap();
    assert!(doc.model.is_empty());
    assert_eq!(doc, old);
    assert_eq!(doc.title, "Plant notes");
    let page = &doc.pages[0];
    assert_eq!(page.shapes.len(), 2);
    assert_eq!(page.shapes[1].text, "Tank");
    assert_eq!(page.shapes[1].kind, ShapeKind::Ellipse);
    assert_eq!(page.shapes[1].angle_deg, 15.0);
    assert!(page.shapes.iter().all(|s| s.element.is_none()));
    assert_eq!(page.connectors.len(), 1);
    assert!(page.view.is_none());
    // Saved again it becomes a version 3 file with no model section.
    let v = parse(&save(&doc).unwrap());
    assert_eq!(v["version"], json!(3));
    assert!(v.get("model").is_none());
}

#[test]
fn version_2_ignores_a_stray_model_section() {
    let model = json!({"schemaVersion": 99, "elements": [{"id": "e1", "type": "flowchart.Process"}]});
    let file = json!({"format": "vizcraft", "version": 2, "model": model, "document": cascade_1_document()});
    let doc = load(file.to_string().as_bytes()).unwrap();
    assert!(doc.model.is_empty());
}

#[test]
fn dangling_references_load_as_free_drawing() {
    let mut doc = modelled();
    {
        let page = doc.page_mut().unwrap();
        page.shapes[0].element = Some("e404".into());
        page.connectors[0].relationship = Some("r404".into());
        // Names a relationship, not an element: still not a valid shape reference.
        page.shapes[3].element = Some("r2".into());
    }
    // Serialized by hand so the save path cannot repair anything first.
    let file = json!({
        "format": "vizcraft",
        "version": 3,
        "model": serde_json::to_value(&doc.model).unwrap(),
        "document": serde_json::to_value(&doc).unwrap(),
    });
    let loaded = load(file.to_string().as_bytes()).unwrap();
    let page = &loaded.pages[0];
    assert_eq!(page.shapes[0].element, None);
    assert_eq!(page.shapes[3].element, None);
    assert_eq!(page.connectors[0].relationship, None);
    // Valid references survive, and the model is untouched.
    assert_eq!(page.shapes[1].element.as_deref(), Some("e2"));
    assert_eq!(page.connectors[1].relationship.as_deref(), Some("r2"));
    assert_eq!(loaded.model, doc.model);
}

#[test]
fn newer_model_schema_is_refused() {
    let newer = u64::from(SCHEMA_VERSION) + 1;
    let file = json!({"format": "vizcraft", "version": 3, "model": {"schemaVersion": newer}, "document": {}});
    let err = load(file.to_string().as_bytes()).unwrap_err();
    assert_eq!(err, FormatError::TooNew { found: newer, supported: SCHEMA_VERSION });
    assert!(err.to_string().contains("newer VizCraft"), "{err}");
    // A fractional or huge version is newer too.
    for v in [json!(1.5), json!(1e300)] {
        let file = json!({"format": "vizcraft", "version": 3, "model": {"schemaVersion": v}, "document": {}});
        assert!(matches!(load(file.to_string().as_bytes()), Err(FormatError::TooNew { .. })), "{v}");
    }
}

#[test]
fn junk_model_sections_are_tolerated() {
    let doc_value = serde_json::to_value(modelled()).unwrap();
    for junk in [json!(null), json!([1, 2]), json!("model"), json!(7), json!(true)] {
        let file = json!({"format": "vizcraft", "version": 3, "model": junk, "document": doc_value});
        let doc = load(file.to_string().as_bytes()).unwrap_or_else(|e| panic!("{junk}: {e}"));
        assert!(doc.model.is_empty(), "{junk}");
        // With no model, every view reference is cleared.
        assert!(doc.pages.iter().flat_map(|p| &p.shapes).all(|s| s.element.is_none()), "{junk}");
        assert!(doc.pages.iter().flat_map(|p| &p.connectors).all(|c| c.relationship.is_none()), "{junk}");
    }
    // An object with wrong-typed fields keeps what it can.
    let odd = json!({
        "schemaVersion": "one",
        "packs": 3,
        "elements": [{"id": "e1", "type": "flowchart.Process", "properties": {"ok": 1, "bad": {"x": 1}}}, 42, "junk"],
        "relationships": {"not": "a list"},
    });
    let file = json!({"format": "vizcraft", "version": 3, "model": odd, "document": doc_value});
    let doc = load(file.to_string().as_bytes()).unwrap();
    assert_eq!(doc.model.schema_version, SCHEMA_VERSION);
    assert!(doc.model.packs.is_empty() && doc.model.relationships.is_empty());
    assert_eq!(doc.model.elements.len(), 1);
    assert_eq!(doc.model.elements[0].properties.get("ok"), Some(&PropValue::Number(1.0)));
    assert!(!doc.model.elements[0].properties.contains_key("bad"));
    assert_eq!(doc.shape("s1").and_then(|s| s.element.as_deref()), Some("e1"));
    // A model section with no schema version reads as the current one.
    let file = json!({"format": "vizcraft", "version": 3, "model": {}, "document": {}});
    assert!(load(file.to_string().as_bytes()).unwrap().model.is_empty());
}

#[test]
fn bare_document_still_loads() {
    let bare = cascade_1_document();
    let doc = load(bare.to_string().as_bytes()).unwrap();
    assert_eq!(doc, serde_json::from_value::<Document>(bare).unwrap());
    assert!(doc.model.is_empty());
    // A bare document from a modelled one loads its geometry as free drawing, since only the
    // wrapper carries the model.
    let bare = serde_json::to_vec(&modelled()).unwrap();
    let doc = load(&bare).unwrap();
    assert!(doc.model.is_empty());
    assert_eq!(doc.pages.len(), 2);
    assert!(doc.pages.iter().flat_map(|p| &p.shapes).all(|s| s.element.is_none()));
    assert_eq!(doc.pages[0].view, modelled().pages[0].view, "page views are document data and survive");
}

#[test]
fn wrapper_errors_are_unchanged() {
    let future = json!({"format": "vizcraft", "version": u64::from(VERSION) + 1, "document": {}});
    assert_eq!(load(future.to_string().as_bytes()), Err(FormatError::TooNew { found: u64::from(VERSION) + 1, supported: VERSION }));
    let no_doc = json!({"format": "vizcraft", "version": 3, "model": {}});
    assert!(matches!(load(no_doc.to_string().as_bytes()), Err(FormatError::Invalid(_))));
    let good = save(&modelled()).unwrap();
    for cut in (0..good.len()).step_by(97) {
        assert!(load(&good[..cut]).is_err(), "truncated at {cut}");
    }
}
