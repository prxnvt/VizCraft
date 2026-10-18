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
    assert_eq!(v["version"], json!(VERSION));
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
    // Saved again it becomes a current-version file with no model section.
    let v = parse(&save(&doc).unwrap());
    assert_eq!(v["version"], json!(VERSION));
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

/// A document with every version 4 field set to something other than its default.
fn rich() -> Document {
    use vizcraft_diagram::{
        Behavior, Bullets, Color, Comment, CommentAnchor, CropRect, Dash, DocProperties, DrawingScale, Glow, Gradient, GradientKind, GradientStop,
        Guide, GuideOrientation, HAlign, HeaderFooter, Hyperlink, ImageAsset, ImageRef, Layer, LineCap, LinkKind, Locks, Margins, Orientation, Reply,
        Shadow, TextBlock, TextCase, TextDirection, Units, VAlign,
    };
    let mut doc = modelled();
    doc.properties = DocProperties {
        title: "Mixing line".into(),
        author: "Ada".into(),
        subject: "Batch plant".into(),
        keywords: "mix; heat".into(),
        description: "How a batch is mixed.\nSecond line.".into(),
    };
    doc.images.push(ImageAsset { id: "img1".into(), media_type: "image/png".into(), bytes: vec![0x89, b'P', b'N', b'G', 0, 1, 2, 255] });
    doc.comments.push(Comment {
        id: "k1".into(),
        anchor: CommentAnchor { page: "p1".into(), shape: Some("s2".into()) },
        author: "Bo".into(),
        time: "2026-10-01T09:00:00Z".into(),
        text: "Check the heat time".into(),
        replies: vec![Reply { author: "Ada".into(), time: "2026-10-02T10:00:00Z".into(), text: "Done".into() }],
    });
    doc.comments.push(Comment { id: "k2".into(), anchor: CommentAnchor { page: "p2".into(), shape: None }, ..Comment::default() });

    let page = &mut doc.pages[0];
    page.orientation = Orientation::Portrait;
    page.units = Units::Millimeters;
    page.drawing_scale = DrawingScale { page: 1.0, drawing: 50.0, units: Units::Meters };
    page.auto_size = true;
    page.background = Some("p3".into());
    page.header = HeaderFooter { left: "Left".into(), center: "Centre".into(), right: "Right".into(), margin: None };
    page.footer = HeaderFooter { left: String::new(), center: "Page 1".into(), right: String::new(), margin: None };
    page.guides = vec![
        Guide { orientation: GuideOrientation::Vertical, position: 250.0 },
        Guide { orientation: GuideOrientation::Horizontal, position: -10.5 },
    ];
    page.layers = vec![
        Layer { id: "L1".into(), name: "Equipment".into(), color: Some(Color::rgb(200, 0, 0)), ..Layer::default() },
        Layer { id: "L2".into(), name: "Notes".into(), visible: false, locked: true, print: false, snap: false, glue: false, color: None },
    ];

    let s = &mut page.shapes[1];
    s.layers = vec!["L1".into(), "L2".into()];
    s.parent = Some("s1".into());
    s.text_block = Some(TextBlock { dx: 5.0, dy: -4.0, w: 80.0, h: 30.0, angle_deg: 90.0 });
    s.links = vec![
        Hyperlink { kind: LinkKind::Url, target: "https://example.com/sop".into(), label: "SOP".into() },
        Hyperlink { kind: LinkKind::File, target: "docs/mix.pdf".into(), label: String::new() },
        Hyperlink { kind: LinkKind::Page, target: "p2".into(), label: "Detail".into() },
        Hyperlink { kind: LinkKind::Shape, target: "s4".into(), label: String::new() },
        Hyperlink { kind: LinkKind::Element, target: "e3".into(), label: String::new() },
    ];
    s.screen_tip = "Mixes the batch".into();
    s.locks = Locks { width: true, height: true, aspect: true, position: true, rotation: true, delete: true, select: true, text: true };
    s.behavior = Behavior { container: true, non_printing: true, hidden: false, no_glue: true, no_snap: true };
    s.image = Some(ImageRef { asset: "img1".into(), crop: CropRect { x: 0.1, y: 0.2, w: 0.5, h: 0.25 } });
    let st = &mut s.style;
    st.font_family = "JetBrains Mono".into();
    st.underline = true;
    st.strikethrough = true;
    st.text_case = TextCase::Title;
    st.highlight = Color::rgb(255, 255, 0);
    st.h_align = HAlign::Right;
    st.v_align = VAlign::Bottom;
    st.bullets = Bullets::Number;
    st.indent = 12.0;
    st.text_direction = TextDirection::RightToLeft;
    st.text_margins = Margins { left: 1.0, top: 2.0, right: 3.0, bottom: 4.5 };
    st.dash = Dash::DashDot;
    st.line_cap = LineCap::Round;
    st.corner_radius = 8.0;
    st.gradient = Some(Gradient {
        kind: GradientKind::Radial,
        angle_deg: 45.0,
        stops: vec![GradientStop { offset: 0.0, color: Color::WHITE }, GradientStop { offset: 1.0, color: Color { r: 0, g: 0, b: 128, a: 200 } }],
    });
    st.fill_transparency = 0.25;
    st.line_transparency = 0.5;
    st.shadow = Some(Shadow { dx: 4.0, dy: 6.0, blur: 3.0, color: Color { r: 0, g: 0, b: 0, a: 100 } });
    st.soft_edges = 5.0;
    st.glow = Some(Glow { radius: 7.0, color: Color::rgb(0, 200, 0) });

    let mut bg = Page::new("p3", "Background");
    bg.is_background = true;
    bg.shapes.push(Shape::new("s30", ShapeKind::Rectangle, 0.0, 0.0, 1100.0, 40.0));
    doc.pages.push(bg);
    doc
}

#[test]
fn version_4_round_trip_keeps_every_new_field() {
    let doc = rich();
    let bytes = save(&doc).unwrap();
    let loaded = load(&bytes).unwrap();
    assert_eq!(loaded, doc);
    assert_eq!(save(&loaded).unwrap(), bytes);

    // The names on disk, so other tools can rely on them.
    let v = parse(&bytes);
    assert_eq!(v["version"], json!(4));
    let d = &v["document"];
    assert_eq!(d["properties"]["keywords"], json!("mix; heat"));
    assert_eq!(d["images"][0], json!({"id": "img1", "mediaType": "image/png", "bytes": "iVBORwABAv8="}));
    assert_eq!(d["comments"][0]["anchor"], json!({"page": "p1", "shape": "s2"}));
    assert_eq!(d["comments"][0]["replies"][0]["text"], json!("Done"));
    let p = &d["pages"][0];
    assert_eq!(p["orientation"], json!("portrait"));
    assert_eq!(p["units"], json!("mm"));
    assert_eq!(p["drawingScale"], json!({"page": 1.0, "drawing": 50.0, "units": "m"}));
    assert_eq!(p["autoSize"], json!(true));
    assert_eq!(p["background"], json!("p3"));
    assert_eq!(p["header"], json!({"left": "Left", "center": "Centre", "right": "Right"}));
    assert_eq!(p["footer"], json!({"center": "Page 1"}));
    assert_eq!(p["guides"][0], json!({"orientation": "vertical", "position": 250.0}));
    assert_eq!(p["layers"][0]["color"], json!("#c80000"));
    assert_eq!(p["layers"][1], json!({"id": "L2", "name": "Notes", "visible": false, "locked": true, "print": false, "snap": false, "glue": false}));
    assert_eq!(d["pages"][2]["isBackground"], json!(true));
    let s = &p["shapes"][1];
    assert_eq!(s["layers"], json!(["L1", "L2"]));
    assert_eq!(s["parent"], json!("s1"));
    assert_eq!(s["textBlock"], json!({"dx": 5.0, "dy": -4.0, "w": 80.0, "h": 30.0, "angleDeg": 90.0}));
    assert_eq!(s["links"][1], json!({"kind": "file", "target": "docs/mix.pdf"}));
    assert_eq!(s["links"][4]["kind"], json!("element"));
    assert_eq!(s["screenTip"], json!("Mixes the batch"));
    assert_eq!(s["locks"]["aspect"], json!(true));
    assert_eq!(s["behavior"], json!({"container": true, "nonPrinting": true, "hidden": false, "noGlue": true, "noSnap": true}));
    assert_eq!(s["image"], json!({"asset": "img1", "crop": {"x": 0.1, "y": 0.2, "w": 0.5, "h": 0.25}}));
    let st = &s["style"];
    for (key, want) in [
        ("fontFamily", json!("JetBrains Mono")),
        ("underline", json!(true)),
        ("strikethrough", json!(true)),
        ("textCase", json!("title")),
        ("highlight", json!("#ffff00")),
        ("hAlign", json!("right")),
        ("vAlign", json!("bottom")),
        ("bullets", json!("number")),
        ("indent", json!(12.0)),
        ("textDirection", json!("rtl")),
        ("textMargins", json!({"left": 1.0, "top": 2.0, "right": 3.0, "bottom": 4.5})),
        ("dash", json!("dashDot")),
        ("lineCap", json!("round")),
        ("cornerRadius", json!(8.0)),
        ("fillTransparency", json!(0.25)),
        ("lineTransparency", json!(0.5)),
        ("shadow", json!({"dx": 4.0, "dy": 6.0, "blur": 3.0, "color": "#00000064"})),
        ("softEdges", json!(5.0)),
        ("glow", json!({"radius": 7.0, "color": "#00c800"})),
    ] {
        assert_eq!(st[key], want, "{key}");
    }
    assert_eq!(st["gradient"]["kind"], json!("radial"));
    assert_eq!(st["gradient"]["stops"][1], json!({"offset": 1.0, "color": "#000080c8"}));
}

#[test]
fn default_fields_are_left_out_of_the_file() {
    let v = parse(&save(&modelled()).unwrap());
    let d = &v["document"];
    for key in ["properties", "comments", "images"] {
        assert!(d.get(key).is_none(), "{key}");
    }
    let p = &d["pages"][0];
    for key in ["orientation", "units", "drawingScale", "autoSize", "background", "isBackground", "header", "footer", "guides", "layers"] {
        assert!(p.get(key).is_none(), "{key}");
    }
    let s = &p["shapes"][0];
    for key in ["layers", "parent", "textBlock", "links", "screenTip", "locks", "behavior", "image"] {
        assert!(s.get(key).is_none(), "{key}");
    }
    let keys: Vec<&String> = s["style"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["bold", "fill", "fontColor", "fontSize", "italic", "line", "lineWidth"], "only the version 3 style keys");
}

#[test]
fn version_3_file_loads_unchanged() {
    let doc = modelled();
    let v4 = save(&doc).unwrap();
    let mut v3 = parse(&v4);
    v3["version"] = json!(3);
    let v3_bytes = serde_json::to_vec_pretty(&v3).unwrap();
    let loaded = load(&v3_bytes).unwrap();
    assert_eq!(loaded, doc);
    // Saved again, only the version number differs from what version 3 held.
    let resaved = save(&loaded).unwrap();
    assert_eq!(resaved, v4);
    let mut back = parse(&resaved);
    back["version"] = json!(3);
    assert_eq!(back, v3);
}

#[test]
fn hostile_new_fields_are_sanitised() {
    let long = "x".repeat(100_000);
    let stops: Vec<Value> = (0..100).map(|i| json!({"offset": 5 - i, "color": "#ff0000"})).collect();
    let links: Vec<Value> = (0..100).map(|_| json!({"kind": "nope", "target": long, "label": long})).collect();
    let layers: Vec<Value> = (0..300).map(|i| json!({"id": format!("L{}", i % 280), "name": long, "visible": "yes"})).collect();
    let guides: Vec<Value> = (0..2000).map(|_| json!({"orientation": 3, "position": 1e300})).collect();
    let style = json!({"fontFamily": long, "indent": -5, "cornerRadius": 1e300, "fillTransparency": 9, "lineTransparency": -3,
                                   "softEdges": 1e9, "textMargins": {"left": -1, "top": 1e300, "right": "x", "bottom": 2},
                                   "gradient": {"kind": "spiral", "angleDeg": 720.5, "stops": stops},
                                   "shadow": {"dx": 1e300, "dy": -1e300, "blur": -2, "color": 5},
                                   "glow": {"radius": 1e300}, "dash": "zigzag", "lineCap": false, "hAlign": 4, "textCase": "shout"});
    let shapes = json!([
        {"id": "s1", "parent": "s2", "layers": ["L1", "L1", "missing", long], "screenTip": long,
         "textBlock": {"dx": 1e300, "dy": "a", "w": -5, "h": 1e300, "angleDeg": -90},
         "links": links, "image": {"asset": "ok", "crop": {"x": 2, "y": -1, "w": 5, "h": 0.5}},
         "locks": "all", "behavior": {"hidden": 1},
         "style": style},
        {"id": "s2", "parent": "s3", "image": {"asset": "html"}},
        {"id": "s3", "parent": "s1"},
        {"id": "s4", "parent": "s4"}
    ]);
    let file = json!({
        "format": "vizcraft",
        "version": 4,
        "document": {
            "properties": {"title": long, "author": 7, "description": long.repeat(2)},
            "comments": [
                {"id": long, "anchor": {"page": "nowhere", "shape": "s1"}, "text": long.repeat(2), "replies": [{"text": long.repeat(2)}, 5]},
                {"id": "k2", "anchor": {"page": "p1", "shape": "ghost"}},
                "junk"
            ],
            "images": [
                {"id": "ok", "mediaType": "image/png", "bytes": "AAEC"},
                {"id": "ok", "mediaType": "image/png", "bytes": "AAEC"},
                {"id": "bad64", "mediaType": "image/png", "bytes": "!!!!"},
                {"id": "html", "mediaType": "text/html", "bytes": "AAEC"},
                {"id": "a b", "mediaType": "image/png", "bytes": "AAEC"},
                {"id": "weird", "mediaType": "image/<svg>", "bytes": "AAEC"}
            ],
            "pages": [
                {
                    "id": "p1",
                    "background": "p1",
                    "drawingScale": {"page": -1, "drawing": 1e300, "units": "parsecs"},
                    "orientation": ["portrait"],
                    "header": {"left": long},
                    "guides": guides,
                    "layers": layers,
                    "shapes": shapes
                },
                {"id": "p2", "background": "p1"}
            ]
        }
    });
    let doc = load(file.to_string().as_bytes()).unwrap();

    assert!(doc.properties.title.len() <= 1024 && doc.properties.author.is_empty());
    assert!(doc.properties.description.len() <= vizcraft_diagram::MAX_TEXT);
    assert_eq!(doc.comments.len(), 2);
    assert!(doc.comments[0].id.len() <= 64 && doc.comments[0].text.len() <= vizcraft_diagram::MAX_TEXT);
    assert_eq!(doc.comments[0].anchor.page, "p1", "an unknown page moves the comment to the first page");
    assert_eq!(doc.comments[0].anchor.shape, None);
    assert_eq!(doc.comments[0].replies.len(), 1);
    assert_eq!(doc.comments[1].anchor.shape, None, "an unknown shape anchors the comment to its page");
    let ids: Vec<&str> = doc.images.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["ok"]);
    assert_eq!(doc.images[0].bytes, [0, 1, 2]);

    let page = &doc.pages[0];
    assert_eq!(page.background, None, "a page cannot be its own background");
    assert_eq!(doc.pages[1].background, None, "p1 is not a background page");
    assert_eq!(page.drawing_scale.page, 1.0);
    assert_eq!(page.drawing_scale.drawing, 1.0e6);
    assert_eq!(page.drawing_scale.units, vizcraft_diagram::Units::Inches);
    assert!(page.header.left.len() <= 1024);
    assert_eq!(page.guides.len(), 1000);
    assert!(page.guides.iter().all(|g| g.position == vizcraft_diagram::MAX_EXTENT));
    assert_eq!(page.layers.len(), 256);
    assert!(page.layers.iter().all(|l| l.name.len() <= 256 && l.visible));

    let s1 = &page.shapes[0];
    assert_eq!(s1.layers, ["L1"]);
    assert!(s1.screen_tip.len() <= 4096);
    let tb = s1.text_block.unwrap();
    assert_eq!((tb.dx, tb.dy, tb.w, tb.h, tb.angle_deg), (vizcraft_diagram::MAX_EXTENT, 0.0, 0.0, vizcraft_diagram::MAX_EXTENT, 270.0));
    assert_eq!(s1.links.len(), 32);
    assert!(s1.links.iter().all(|l| l.target.len() <= 4096 && l.label.len() <= 1024));
    let crop = s1.image.as_ref().unwrap().crop;
    assert_eq!((crop.x, crop.y, crop.w, crop.h), (1.0, 0.0, 0.0, 0.5));
    assert!(!s1.behavior.hidden && !s1.locks.delete);
    let st = &s1.style;
    assert!(st.font_family.len() <= 128);
    assert_eq!((st.indent, st.corner_radius, st.fill_transparency, st.line_transparency), (0.0, 10_000.0, 1.0, 0.0));
    assert_eq!(st.soft_edges, 500.0);
    assert_eq!((st.text_margins.left, st.text_margins.top, st.text_margins.right, st.text_margins.bottom), (0.0, 1000.0, 0.0, 2.0));
    let g = st.gradient.as_ref().unwrap();
    assert_eq!(g.kind, vizcraft_diagram::GradientKind::Linear);
    assert_eq!(g.angle_deg, 0.5);
    assert_eq!(g.stops.len(), 16);
    assert!(g.stops.iter().all(|s| (0.0..=1.0).contains(&s.offset)));
    assert!(g.stops.windows(2).all(|w| w[0].offset <= w[1].offset));
    let sh = st.shadow.unwrap();
    assert_eq!((sh.dx, sh.dy, sh.blur), (1000.0, -1000.0, 0.0));
    assert_eq!(st.glow.unwrap().radius, 500.0);
    assert_eq!(st.dash, vizcraft_diagram::Dash::Solid);

    // The parent cycle s1 -> s2 -> s3 -> s1 is broken at one link; self-parents are cleared.
    let parents: Vec<Option<&str>> = page.shapes.iter().map(|s| s.parent.as_deref()).collect();
    assert_eq!(parents.iter().filter(|p| p.is_none()).count(), 2, "{parents:?}");
    assert_eq!(parents[3], None);
    assert_eq!(page.shapes[1].image, None, "the asset was dropped, so the reference is cleared");
    // Sanitising is stable: saving and loading again changes nothing.
    assert_eq!(load(&save(&doc).unwrap()).unwrap(), doc);
}
