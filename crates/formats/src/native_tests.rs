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
    page.view = Some(View { viewpoint: "flowchart.Flowchart".into(), layout: "layered".into(), ..View::default() });
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
        ..DocProperties::default()
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
    s.image = Some(ImageRef { asset: "img1".into(), crop: CropRect { x: 0.1, y: 0.2, w: 0.5, h: 0.25 }, ..Default::default() });
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

/// A free shape with typed data of every kind, and a page that defines two of the keys.
fn with_data() -> Document {
    use vizcraft_diagram::data::{DataKey, DataRow, DataType, set_page_keys};
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
    let row = |key: &str, kind: DataType, value: &str| DataRow { key: key.into(), kind, value: value.into(), ..DataRow::default() };
    s.data.page_keys = true;
    s.data.rows = vec![
        DataRow { unit: "USD".into(), required: true, ..row("cost", DataType::Number, "12.5") },
        row("done", DataType::Bool, "false"),
        row("due", DataType::Date, "2024-02-29"),
        DataRow { values: vec!["Low".into(), "High".into()], default: Some("Low".into()), ..row("level", DataType::Enum, "High") },
        row("site", DataType::Url, "https://example.com/a?b=\"c\""),
        DataRow { label: "Note, \"quoted\"".into(), hidden: true, ..row("note", DataType::String, "multi\nline \u{e9}") },
    ];
    page.shapes.push(s);
    page.shapes.push(Shape::new("s2", ShapeKind::Rectangle, 200.0, 0.0, 100.0, 60.0));
    let keys = vec![
        DataKey { key: "cost".into(), kind: DataType::Number, unit: "USD".into(), required: true, ..DataKey::default() },
        DataKey {
            key: "level".into(),
            kind: DataType::Enum,
            values: vec!["Low".into(), "High".into()],
            default: Some("Low".into()),
            ..DataKey::default()
        },
    ];
    set_page_keys(page, &keys);
    doc.sanitize();
    doc
}

#[test]
fn shape_data_round_trips_and_is_left_out_when_empty() {
    let doc = with_data();
    let bytes = save(&doc).unwrap();
    let back = load(&bytes).unwrap();
    assert_eq!(back, doc);
    assert_eq!(save(&back).unwrap(), bytes, "saving again gives the same bytes");
    let v = parse(&bytes);
    let s = &v["document"]["pages"][0]["shapes"];
    assert_eq!(s[0]["data"]["rows"][0], json!({"key": "cost", "type": "number", "unit": "USD", "required": true, "value": "12.5"}));
    assert_eq!(s[0]["data"]["pageKeys"], true);
    assert!(s[1].get("data").is_none(), "a shape without data writes no data key");
    assert_eq!(vizcraft_diagram::data::page_keys(&back.pages[0]).len(), 2);
}

#[test]
fn hostile_shape_data_is_sanitised() {
    let long = "x".repeat(100_000);
    let mut rows: Vec<Value> = vec![
        json!({"key": "cost", "type": "number", "value": "lots"}),
        json!({"key": "cost", "type": "number", "value": 3}),
        json!({"key": "n", "type": "number", "value": 4}),
        json!({"key": "b", "type": "bool", "value": true}),
        json!({"key": "", "value": "no key"}),
        json!({"key": long, "value": "long key"}),
        json!({"key": "e", "type": "enum", "values": [1, "A", "A", ""], "value": "a"}),
        json!({"key": "u", "type": "url", "value": "javascript:alert(1)"}),
        json!({"key": "d", "type": "date", "value": "2023-02-29", "default": {"x": 1}}),
        json!({"key": "t", "type": 5, "value": [1, 2], "label": long, "unit": long, "hidden": "yes"}),
        json!("junk"),
        json!(7),
    ];
    rows.extend((0..500).map(|i| json!({"key": format!("k{i}"), "value": long})));
    let file = json!({
        "format": "vizcraft",
        "version": 4,
        "document": {"pages": [{"id": "p1", "ext": {"vizcraft.dataKeys": "[{\"key\": 5}, {\"key\": \"ok\", \"type\": \"enum\"}"},
                                "shapes": [{"id": "s1", "data": {"rows": rows, "pageKeys": "maybe"}}, {"id": "s2", "data": "junk"}]}]}
    });
    let doc = load(&serde_json::to_vec(&file).unwrap()).unwrap();
    let d = &doc.pages[0].shapes[0].data;
    use vizcraft_diagram::data::{DataType, MAX_LABEL, MAX_ROWS, MAX_UNIT, MAX_VALUE};
    assert_eq!(d.rows.len(), MAX_ROWS);
    assert!(!d.page_keys);
    let get = |k: &str| d.row(k).unwrap();
    assert_eq!((get("cost").kind, get("cost").value.as_str()), (DataType::String, "lots"), "a value that does not fit keeps its text");
    assert_eq!((get("n").kind, get("n").value.as_str()), (DataType::Number, "4"));
    assert_eq!(get("b").value, "true");
    assert_eq!((get("e").values.clone(), get("e").value.as_str()), (vec!["A".to_string()], "A"));
    assert_eq!(get("u").kind, DataType::String);
    assert_eq!((get("d").kind, get("d").default.clone()), (DataType::String, None));
    let t = get("t");
    assert_eq!((t.kind, t.value.as_str(), t.hidden), (DataType::String, "", false));
    assert!(t.label.len() <= MAX_LABEL && t.unit.len() <= MAX_UNIT);
    assert!(d.rows.iter().all(|r| r.value.len() <= MAX_VALUE && !r.key.is_empty()));
    assert!(doc.pages[0].shapes[1].data.rows.is_empty());
    assert!(vizcraft_diagram::data::page_keys(&doc.pages[0]).is_empty(), "unreadable page keys are none");
}

/// A page of two shapes joined by one connector per line end, each carrying a kind name.
fn kinds_doc() -> Document {
    use vizcraft_diagram::Arrow;
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.shapes.push(Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
    page.shapes.push(Shape::new("s2", ShapeKind::Rectangle, 400.0, 0.0, 100.0, 60.0));
    for (i, a) in Arrow::EVERY.into_iter().enumerate() {
        let mut c = Connector::new(format!("c{i}"), End::dynamic("s1"), End::dynamic("s2"));
        c.start_arrow = a;
        c.end_arrow = Arrow::EVERY[(i + 5) % Arrow::EVERY.len()];
        c.kind = if i % 2 == 0 { "basic.arrow".into() } else { "flowchart.flow".into() };
        c.text = format!("link {i}");
        page.connectors.push(c);
    }
    doc
}

#[test]
fn connector_kinds_and_every_line_end_round_trip_natively() {
    let doc = kinds_doc();
    let back = load(&save(&doc).unwrap()).unwrap();
    assert_eq!(back.pages[0].connectors, doc.pages[0].connectors);
    let bytes = String::from_utf8(save(&doc).unwrap()).unwrap();
    assert!(bytes.contains("\"kind\": \"flowchart.flow\"") || bytes.contains("\"kind\":\"flowchart.flow\""));
    assert!(bytes.contains("hollowTriangle") && bytes.contains("crowsFootCircle") && bytes.contains("barCircle"));
}

#[test]
fn a_connector_without_a_kind_writes_no_kind_and_junk_kinds_and_ends_load_tolerantly() {
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.connectors.push(Connector::new("c1", End::free(vizcraft_diagram::Point::ZERO), End::free(vizcraft_diagram::Point::new(9.0, 9.0))));
    let text = String::from_utf8(save(&doc).unwrap()).unwrap();
    assert!(!text.contains("\"kind\""), "{text}");
    let file = json!({
        "format": "vizcraft", "version": 4,
        "document": {"pages": [{"id": "p1", "connectors": [
            {"id": "c1", "kind": 7, "startArrow": "harpoon", "endArrow": "hollowDiamond"},
            {"id": "c2", "kind": "x".repeat(10_000), "endArrow": ["triangle"]},
            {"id": "c3", "kind": "basic.bus"}
        ]}]}
    });
    let doc = load(&serde_json::to_vec(&file).unwrap()).unwrap();
    let cs = &doc.pages[0].connectors;
    assert_eq!(
        (cs[0].kind.as_str(), cs[0].start_arrow, cs[0].end_arrow),
        ("", vizcraft_diagram::Arrow::None, vizcraft_diagram::Arrow::HollowDiamond)
    );
    assert!(cs[1].kind.len() <= 128, "a hostile kind is cut");
    assert_eq!(cs[2].kind, "basic.bus");
}

#[test]
fn connector_kinds_and_line_ends_round_trip_through_drawio() {
    let doc = kinds_doc();
    let bytes = crate::drawio::save(&doc).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("vizcraftKind=flowchart.flow"), "the kind is written as the vizcraftKind style key");
    let imported = crate::drawio::import(&bytes).unwrap();
    let back = &imported.pages[0].connectors;
    let want = &doc.pages[0].connectors;
    assert_eq!(back.len(), want.len());
    // draw.io has no dot end: it is written as the filled oval and reads back as the circle.
    let drawn = |a: vizcraft_diagram::Arrow| if a == vizcraft_diagram::Arrow::Dot { vizcraft_diagram::Arrow::Circle } else { a };
    for (b, w) in back.iter().zip(want) {
        assert_eq!((b.kind.as_str(), b.start_arrow, b.end_arrow), (w.kind.as_str(), drawn(w.start_arrow), drawn(w.end_arrow)), "{}", w.id);
    }
}

#[test]
fn drawio_hollow_and_er_ends_map_onto_the_notation_ends() {
    use vizcraft_diagram::Arrow;
    let xml = |style: &str| {
        format!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="a" value="" style="rounded=0;" vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="40" as="geometry"/></mxCell>
            <mxCell id="b" value="" style="rounded=0;" vertex="1" parent="1"><mxGeometry x="300" y="0" width="80" height="40" as="geometry"/></mxCell>
            <mxCell id="e" value="" style="{style}" edge="1" parent="1" source="a" target="b"><mxGeometry relative="1" as="geometry"/></mxCell>
            </root></mxGraphModel>"#
        )
    };
    let ends = |style: &str| {
        let page = crate::drawio::import(xml(style).as_bytes()).unwrap().pages.remove(0);
        let c = &page.connectors[0];
        (c.start_arrow, c.end_arrow)
    };
    assert_eq!(ends("endArrow=block;endFill=0;"), (Arrow::None, Arrow::HollowTriangle));
    assert_eq!(ends("startArrow=diamond;startFill=0;endArrow=none;"), (Arrow::HollowDiamond, Arrow::None));
    assert_eq!(ends("endArrow=oval;endFill=0;"), (Arrow::None, Arrow::OpenCircle));
    assert_eq!(ends("endArrow=block;endFill=1;"), (Arrow::None, Arrow::Triangle));
    assert_eq!(ends("startArrow=ERmandOne;endArrow=ERzeroToMany;"), (Arrow::DoubleBar, Arrow::CrowsFootCircle));
    assert_eq!(ends("startArrow=ERmany;endArrow=ERoneToMany;"), (Arrow::CrowsFoot, Arrow::CrowsFootBar));
    assert_eq!(ends("endArrow=ERzeroToOne;"), (Arrow::None, Arrow::BarCircle));
    assert_eq!(ends("endArrow=ERone;endFill=0;"), (Arrow::None, Arrow::Bar), "an ER end has no hollow form");
}

#[test]
fn a_new_document_is_infinite_and_a_file_without_the_board_key_loads_fixed() {
    use vizcraft_diagram::Board;
    let doc = Document::new();
    assert_eq!(doc.pages[0].board, Board::Infinite);
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["pages"][0]["board"], json!("infinite"), "the name on disk");
    assert_eq!(load(&bytes).unwrap().pages[0].board, Board::Infinite);

    // A version 4 file from before boards has no key: its sheet stays.
    let mut old = v.clone();
    old["document"]["pages"][0].as_object_mut().unwrap().remove("board");
    let loaded = load(&serde_json::to_vec_pretty(&old).unwrap()).unwrap();
    assert_eq!(loaded.pages[0].board, Board::Fixed);
    assert_eq!(loaded.pages[0].frame().width(), loaded.pages[0].width);

    // Fixed round trips, and a name from a newer version or a junk value loads as Fixed.
    let mut fixed = doc.clone();
    fixed.pages[0].board = Board::Fixed;
    let reloaded = load(&save(&fixed).unwrap()).unwrap();
    assert_eq!(reloaded.pages[0].board, Board::Fixed);
    for junk in [json!("endless"), json!(7), json!(null), json!({"a": 1})] {
        let mut v = v.clone();
        v["document"]["pages"][0]["board"] = junk.clone();
        assert_eq!(load(&serde_json::to_vec_pretty(&v).unwrap()).unwrap().pages[0].board, Board::Fixed, "{junk}");
    }
}

#[test]
fn conformance_profile_round_trips_as_a_document_field() {
    use vizcraft_diagram::vizcraft_model::Severity;
    use vizcraft_diagram::{Conformance, RuleOverride};
    let mut doc = modelled();
    doc.conformance = Conformance {
        enabled: true,
        user_set: true,
        rulesets: vec!["flowchart.iso5807".into(), "drawing.tidy".into()],
        overrides: vec![RuleOverride { rule: "flowchart.singleExit".into(), severity: Some(Severity::Info), disabled: false }],
        ..Conformance::default()
    };
    doc.conformance.suppress("flowchart.illegalFlow|r1,e1,e2|c1,s1,s2|p1", "an archive copy");
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["conformance"]["rulesets"], json!(["flowchart.iso5807", "drawing.tidy"]));
    assert_eq!(v["document"]["conformance"]["suppressions"][0]["reason"], "an archive copy");
    assert_eq!(load(&bytes).unwrap().conformance, doc.conformance);
    // The default profile is left out, and a junk one reads as the default.
    let free = save(&Document::new()).unwrap();
    assert!(parse(&free)["document"].get("conformance").is_none());
    let mut junk = parse(&free);
    junk["document"]["conformance"] = json!(["not", "a", "profile"]);
    assert!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().conformance.is_default());
}

#[test]
fn compartments_and_port_glue_round_trip() {
    use vizcraft_diagram::compartment::{Compartment, CompartmentKind, Height};
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    // A UML-style class: name, attributes, operations.
    let mut class = Shape::new("k", ShapeKind::Rectangle, 10.0, 20.0, 160.0, 100.0);
    class.text = "Order".into();
    let mut attrs = Compartment::new("attrs", CompartmentKind::List);
    attrs.rows = vec!["+ id: int".into(), "+ total: \"money\"".into()];
    let mut ops = Compartment::new("ops", CompartmentKind::List);
    ops.title = Some("Operations".into());
    ops.rows = vec!["+ pay()".into()];
    ops.fill = Some(vizcraft_diagram::Color::rgb(0xee, 0xf2, 0xf7));
    ops.font_size = Some(10.0);
    ops.bold = Some(false);
    class.compartments = vec![attrs, ops];
    // An equipment tag: a paragraph and a list of fields, plus a slot holding a pin.
    let mut tag = Shape::new("t", ShapeKind::Rectangle, 300.0, 20.0, 120.0, 140.0);
    tag.text = "P-101".into();
    let mut desc = Compartment::new("desc", CompartmentKind::Paragraph);
    desc.text = "Feed pump\nduty".into();
    desc.align = vizcraft_diagram::HAlign::Center;
    let mut fields = Compartment::new("fields", CompartmentKind::List);
    fields.rows = vec!["Flow: 12 m3/h".into()];
    fields.height = Height::Fixed(30.0);
    let mut pins = Compartment::new("pins", CompartmentKind::Slot);
    pins.members = vec!["pin".into()];
    tag.compartments = vec![desc, fields, pins];
    let mut pin = Shape::new("pin", ShapeKind::Ellipse, 310.0, 120.0, 10.0, 10.0);
    pin.parent = Some("t".into());
    page.shapes.extend([class, tag, pin]);
    page.connectors.push(Connector::new("c", End::port("k", "attrs.right"), End::port("t", "fields.left")));
    doc.sanitize();
    let bytes = save(&doc).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains(r#""port": "attrs.right""#) && !text.contains(r#""point""#), "{text}");
    let back = load(&bytes).unwrap();
    assert_eq!(back.pages, doc.pages);
    // Files without compartments stay as they were: the field is left out.
    let plain = save(&Document::new()).unwrap();
    assert!(!String::from_utf8_lossy(&plain).contains("compartments"));
}

#[test]
fn page_routing_policy_round_trips_and_old_files_route_every_connector() {
    use vizcraft_diagram::{Avoid, RoutingPolicy, StraightFallback};
    let mut doc = Document::new();
    doc.pages[0].routing = RoutingPolicy { avoid: Avoid::Never, straight_fallback: StraightFallback::Keep };
    let bytes = save(&doc).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(r#""routing""#) && text.contains(r#""straightFallback": "keep""#), "{text}");
    assert_eq!(load(&bytes).unwrap().pages[0].routing, doc.pages[0].routing);
    // The default is left out, and a file without it (or with junk) loads with avoidance on.
    let plain = String::from_utf8(save(&Document::new()).unwrap()).unwrap();
    assert!(!plain.contains("routing"));
    let mut file: Value = serde_json::from_str(&text).unwrap();
    file["document"]["pages"][0]["routing"] = json!({"avoid": ["x"], "straightFallback": 9});
    let junk = load(&serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(junk.pages[0].routing, RoutingPolicy::default());
}

/// A freeform with a hole (a subtract result) and an open freeform line.
fn freeforms() -> Document {
    use vizcraft_diagram::geom::{Path, Point};
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 25.0, 25.0, 50.0, 50.0);
    let r = vizcraft_diagram::shapeop::boolean(vizcraft_diagram::shapeop::BoolOp::Subtract, &[&a, &b]);
    let ring = vizcraft_diagram::shapeop::freeform(&a, "ring".into(), &vizcraft_diagram::shapeop::regions_path(&r), true).unwrap();
    let line =
        vizcraft_diagram::shapeop::freeform(&a, "line".into(), &Path::polyline(&[Point::new(200.0, 0.0), Point::new(260.0, 40.0)]), false).unwrap();
    page.shapes.extend([ring, line]);
    doc
}

/// A document with one data graphic (a callout and an icon set), shown by `s1`, and one link.
fn with_graphics() -> Document {
    use vizcraft_diagram::data_graphic::{DataGraphic, DataItem, DataLink, ItemKind, Op, Rule, TableFormat};
    let mut doc = Document::new();
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
    s.data_graphic = Some("dg1".into());
    doc.page_mut().unwrap().shapes.push(s);
    let icons = DataItem {
        kind: ItemKind::Icons,
        key: "cost".into(),
        rules: vec![Rule { op: Op::Gt, value: "10".into(), icon: "check".into(), ..Rule::default() }],
        ..DataItem::default()
    };
    doc.data_graphics.push(DataGraphic {
        id: "dg1".into(),
        name: "Cost".into(),
        items: vec![DataItem { key: "cost".into(), ..DataItem::default() }, icons],
    });
    doc.data_links.push(DataLink {
        id: "dl1".into(),
        path: "/data/assets.xlsx".into(),
        format: TableFormat::Xlsx,
        sheet: "Data".into(),
        key_column: "Tag".into(),
        data_key: "Tag".into(),
        columns: vec!["Tag".into(), "Cost".into()],
        last_refresh: "2026-10-18T12:00:00Z".into(),
    });
    doc.sanitize();
    doc
}

#[test]
fn freeform_paths_round_trip_natively_and_through_drawio() {
    let doc = freeforms();
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["kind"], "freeform");
    assert_eq!(v["document"]["pages"][0]["shapes"][1]["path"]["closed"], false);
    assert_eq!(load(&bytes).unwrap().pages, doc.pages);
    let xml = crate::drawio::save(&doc).unwrap();
    assert!(String::from_utf8_lossy(&xml).contains("vizcraftPath"));
    let back = crate::drawio::load(&xml).unwrap();
    let page = back.page().unwrap();
    for (got, want) in page.shapes.iter().zip(&doc.page().unwrap().shapes) {
        assert_eq!((got.kind, &got.path, got.x, got.y, got.w, got.h), (want.kind, &want.path, want.x, want.y, want.w, want.h));
        assert!(!got.ext.keys().any(|k| k.contains("vizcraftPath")), "{:?}", got.ext);
    }
    // Junk paths load tolerantly; a path on another kind is dropped.
    let mut junk = v.clone();
    junk["document"]["pages"][0]["shapes"][0]["path"] =
        json!({"els": [{"lineTo": {"x": 1, "y": 2}}, 5, {"moveTo": {"x": 1e300, "y": 0}}], "closed": "x"});
    junk["document"]["pages"][0]["shapes"][1]["kind"] = json!("rectangle");
    let back = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    let shapes = &back.page().unwrap().shapes;
    assert!(shapes[0].path.as_ref().is_some_and(|f| f.els.is_empty()));
    assert!(shapes[1].path.is_none());
}

#[test]
fn shape_names_round_trip_and_are_capped() {
    let mut doc = Document::new();
    let mut s = Shape::new("s", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    s.name = "Pump \"P-101\"".into();
    doc.page_mut().unwrap().shapes.push(s);
    let bytes = save(&doc).unwrap();
    assert_eq!(parse(&bytes)["document"]["pages"][0]["shapes"][0]["name"], "Pump \"P-101\"");
    assert_eq!(load(&bytes).unwrap().pages, doc.pages);
    assert!(!String::from_utf8_lossy(&save(&Document::new()).unwrap()).contains("\"name\": \"\""));
    let mut v = parse(&bytes);
    v["document"]["pages"][0]["shapes"][0]["name"] = json!("n".repeat(10_000));
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.page().unwrap().shapes[0].name.len(), Shape::MAX_NAME);
}

#[test]
fn document_masters_round_trip_and_junk_is_dropped() {
    use vizcraft_diagram::CustomMaster;
    let mut doc = Document::new();
    let shapes = vec![Shape::new("a", ShapeKind::Ellipse, 40.0, 40.0, 30.0, 30.0)];
    doc.masters.push(CustomMaster::new("m1".into(), "Valve", shapes, Vec::new()).unwrap());
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["masters"][0]["name"], "Valve");
    assert_eq!(load(&bytes).unwrap().masters, doc.masters);
    assert!(parse(&save(&Document::new()).unwrap())["document"].get("masters").is_none());
    let mut junk = v.clone();
    junk["document"]["masters"] =
        json!([5, {"id": "m1"}, {"id": "bad id", "shapes": [{}]}, v["document"]["masters"][0], v["document"]["masters"][0]]);
    assert_eq!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().masters, doc.masters);
}

#[test]
fn document_protection_round_trips_and_defaults_off() {
    let mut doc = Document::new();
    doc.protection = vizcraft_diagram::Protection { shapes: true, backgrounds: false };
    let bytes = save(&doc).unwrap();
    assert_eq!(parse(&bytes)["document"]["protection"]["shapes"], true);
    assert_eq!(load(&bytes).unwrap().protection, doc.protection);
    let free = save(&Document::new()).unwrap();
    assert!(parse(&free)["document"].get("protection").is_none());
    let mut junk = parse(&free);
    junk["document"]["protection"] = json!({"shapes": "yes", "backgrounds": 3});
    assert_eq!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().protection, vizcraft_diagram::Protection::default());
}

#[test]
fn document_language_and_ignored_words_round_trip_and_are_sanitised() {
    let mut doc = Document::new();
    doc.language = "fr-FR".into();
    doc.ignored_words = vec!["vizcraft".into(), "teh".into()];
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["language"], "fr-FR");
    assert_eq!(v["document"]["ignoredWords"], json!(["vizcraft", "teh"]));
    let back = load(&bytes).unwrap();
    assert_eq!((back.language.as_str(), back.ignored_words.clone()), ("fr-FR", doc.ignored_words.clone()));
    // Defaults are left out of the file.
    let plain = parse(&save(&Document::new()).unwrap());
    assert!(plain["document"].get("language").is_none() && plain["document"].get("ignoredWords").is_none());
    // Junk loads as the default, a tag takes its usual case, words are lowered and deduplicated.
    let mut junk = parse(&save(&Document::new()).unwrap());
    junk["document"]["language"] = json!("not a tag!");
    junk["document"]["ignoredWords"] = json!(["Teh", "teh", 7, "x".repeat(500)]);
    let loaded = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    assert_eq!(loaded.language, "");
    assert_eq!(loaded.ignored_words.len(), 2);
    assert_eq!(loaded.ignored_words[0], "teh");
    junk["document"]["language"] = json!("en-gb");
    assert_eq!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().language, "en-GB");
}

#[test]
fn shape_language_round_trips_and_is_sanitised() {
    let mut doc = Document::new();
    let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
    a.language = "de-DE".into();
    let b = Shape::new("b", ShapeKind::Rectangle, 200.0, 0.0, 100.0, 60.0);
    doc.page_mut().unwrap().shapes.extend([a, b]);
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["language"], "de-DE");
    assert!(v["document"]["pages"][0]["shapes"][1].get("language").is_none(), "the default is left out");
    assert_eq!(load(&bytes).unwrap().pages[0].shapes[0].language, "de-DE");
    let mut junk = v.clone();
    junk["document"]["pages"][0]["shapes"][0]["language"] = json!("en_us");
    junk["document"]["pages"][0]["shapes"][1]["language"] = json!(["x"]);
    let loaded = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    assert_eq!(loaded.pages[0].shapes[0].language, "en-US");
    assert_eq!(loaded.pages[0].shapes[1].language, "");
}

#[test]
fn data_graphics_round_trip_and_hostile_ones_are_sanitised() {
    let doc = with_graphics();
    let bytes = save(&doc).unwrap();
    assert_eq!(load(&bytes).unwrap(), doc);
    let mut v = parse(&bytes);
    assert_eq!(v["document"]["dataGraphics"][0]["items"][1]["rules"][0], json!({"op": "gt", "value": "10", "icon": "check"}));
    assert!(parse(&save(&Document::new()).unwrap())["document"].get("dataGraphics").is_none(), "none writes no key");
    v["document"]["dataGraphics"] = json!([
        {"id": "dg1", "name": "x".repeat(10_000), "items": [{"kind": "warp", "min": "a", "max": -1e308, "rules": [{"op": 5, "icon": "smiley"}]}]},
        {"id": "dg1"},
        {"id": "bad id"},
        7,
    ]);
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.data_graphics.len(), 1, "repeated, unusable and junk graphics are dropped");
    let g = &back.data_graphics[0];
    assert!(g.name.len() <= vizcraft_diagram::data_graphic::MAX_NAME);
    assert!(g.items[0].min < g.items[0].max && g.items[0].rules[0].icon.is_empty());
}

#[test]
fn data_links_round_trip_and_hostile_ones_are_sanitised() {
    let doc = with_graphics();
    let bytes = save(&doc).unwrap();
    let mut v = parse(&bytes);
    assert_eq!(v["document"]["dataLinks"][0]["keyColumn"], "Tag");
    assert_eq!(v["document"]["dataLinks"][0]["format"], "xlsx");
    v["document"]["dataLinks"] =
        json!([{"id": "dl1", "path": "p".repeat(100_000), "format": "pdf", "columns": ["c".repeat(500), 3]}, {"id": ""}, {"id": "dl1"}]);
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.data_links.len(), 1);
    let l = &back.data_links[0];
    assert_eq!(l.format, vizcraft_diagram::data_graphic::TableFormat::Csv, "an unknown format reads as CSV");
    assert!(l.path.len() <= vizcraft_diagram::data_graphic::MAX_PATH && l.columns.iter().all(|c| c.len() <= 128));
}

#[test]
fn shape_data_graphic_round_trips_and_a_dangling_one_is_cleared() {
    let doc = with_graphics();
    let mut v = parse(&save(&doc).unwrap());
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["dataGraphic"], "dg1");
    v["document"]["pages"][0]["shapes"][0]["dataGraphic"] = json!("nope");
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.pages[0].shapes[0].data_graphic, None, "a graphic the document lacks is cleared");
    v["document"]["pages"][0]["shapes"][0]["dataGraphic"] = json!({"x": 1});
    assert_eq!(load(&serde_json::to_vec(&v).unwrap()).unwrap().pages[0].shapes[0].data_graphic, None);
}

#[test]
fn picture_adjustments_round_trip_and_junk_is_clamped() {
    use vizcraft_diagram::shape_fields::{Adjust, Recolor};
    use vizcraft_diagram::{Color, ImageAsset, ImageRef};
    let mut doc = Document::new();
    doc.images.push(ImageAsset { id: "img1".into(), media_type: "image/png".into(), bytes: vec![1, 2, 3] });
    let mut s = Shape::new("s", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    let adjust = Adjust { brightness: -0.2, contrast: 0.4, transparency: 0.3, recolor: Recolor::Tint(Color::rgb(0, 128, 255)) };
    s.image = Some(ImageRef { asset: "img1".into(), adjust, ..Default::default() });
    let mut plain = s.clone();
    plain.id = "t".into();
    plain.image = Some(ImageRef { asset: "img1".into(), ..Default::default() });
    doc.page_mut().unwrap().shapes.extend([s, plain]);
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["image"]["adjust"]["recolor"], "#0080ff");
    assert!(v["document"]["pages"][0]["shapes"][1]["image"].get("adjust").is_none(), "neutral adjustments are not written");
    assert_eq!(load(&bytes).unwrap().pages, doc.pages);
    let mut junk = v.clone();
    junk["document"]["pages"][0]["shapes"][0]["image"]["adjust"] = json!({"brightness": 9, "contrast": "x", "transparency": -3, "recolor": "plaid"});
    let back = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    let a = back.page().unwrap().shapes[0].image.as_ref().unwrap().adjust;
    assert_eq!(a, Adjust { brightness: 1.0, contrast: 0.0, transparency: 0.0, recolor: Recolor::None });
}

#[test]
fn connector_end_labels_round_trip_natively_and_through_drawio() {
    let mut doc = kinds_doc();
    for (i, c) in doc.pages[0].connectors.iter_mut().enumerate() {
        c.start_label = if i % 2 == 0 { "1".into() } else { String::new() };
        c.end_label = format!("N{i}");
    }
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    assert_eq!(
        (v["document"]["pages"][0]["connectors"][0]["startLabel"].as_str(), v["document"]["pages"][0]["connectors"][1].get("startLabel")),
        (Some("1"), None)
    );
    assert_eq!(load(&bytes).unwrap().pages[0].connectors, doc.pages[0].connectors);
    // draw.io keeps them as wrapper attributes and the import reads them back, leaving `ext` clean.
    let xml = crate::drawio::save(&doc).unwrap();
    assert!(String::from_utf8_lossy(&xml).contains("vizcraftEndLabel=\"N0\""));
    let back = crate::drawio::import(&xml).unwrap();
    for (b, w) in back.pages[0].connectors.iter().zip(&doc.pages[0].connectors) {
        assert_eq!((&b.start_label, &b.end_label), (&w.start_label, &w.end_label), "{}", w.id);
        assert!(b.ext.is_empty(), "{:?}", b.ext);
    }
    // Junk loads tolerantly and is capped.
    let mut junk = v.clone();
    junk["document"]["pages"][0]["connectors"][0]["startLabel"] = json!(7);
    junk["document"]["pages"][0]["connectors"][0]["endLabel"] = json!("x".repeat(100_000));
    let back = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    let c = &back.pages[0].connectors[0];
    assert!(c.start_label.is_empty() && c.end_label.len() <= vizcraft_diagram::MAX_TEXT);
}

#[test]
fn page_rendering_variant_round_trips_and_is_capped() {
    let mut doc = modelled();
    if let Some(view) = doc.pages[0].view.as_mut() {
        view.variant = "flowchart.compact".into();
    }
    let bytes = save(&doc).unwrap();
    assert_eq!(parse(&bytes)["document"]["pages"][0]["view"]["variant"], "flowchart.compact");
    assert_eq!(load(&bytes).unwrap().pages[0].view, doc.pages[0].view);
    // No variant writes no key; junk loads as none; a huge id is cut.
    let plain = modelled();
    assert!(parse(&save(&plain).unwrap())["document"]["pages"][0]["view"].get("variant").is_none());
    let mut v = parse(&bytes);
    v["document"]["pages"][0]["view"]["variant"] = json!(["x"]);
    assert_eq!(load(&serde_json::to_vec(&v).unwrap()).unwrap().pages[0].view.as_ref().map(|x| x.variant.clone()), Some(String::new()));
    v["document"]["pages"][0]["view"]["variant"] = json!("é".repeat(10_000));
    let cut = load(&serde_json::to_vec(&v).unwrap()).unwrap().pages[0].view.clone().unwrap().variant;
    assert!(cut.len() <= View::MAX_VARIANT && cut.chars().all(|c| c == 'é'));
}

#[test]
fn pattern_instances_round_trip_and_hostile_ones_are_sanitised() {
    let mut doc = modelled();
    doc.model.patterns.push(vizcraft_diagram::vizcraft_model::PatternInstance {
        id: "pat1".into(),
        pattern: "workflow.parallelSplit".into(),
        args: json!({"branches": 2}),
        members: [("split".to_string(), "e1".to_string()), ("join".to_string(), "e2".to_string())].into_iter().collect(),
        generated: [("split".to_string(), "Split".to_string())].into_iter().collect(),
        page: "page1".into(),
        group: "g1".into(),
        frames: Default::default(),
    });
    let bytes = save(&doc).unwrap();
    assert_eq!(load(&bytes).unwrap().model.patterns, doc.model.patterns);
    let mut v = parse(&bytes);
    v["model"]["patterns"] = json!([
        {"id": "pat1", "pattern": "x".repeat(10_000), "args": "junk", "members": {"split": "e1", "ghost": "nope"}},
        {"id": "pat1"},
        {"id": ""},
        42
    ]);
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.model.patterns.len(), 1);
    let p = &back.model.patterns[0];
    assert!(p.pattern.len() <= vizcraft_diagram::vizcraft_model::MAX_KEY);
    assert_eq!((p.args.clone(), p.members.len()), (json!({}), 1));
}

#[test]
fn import_records_round_trip_and_hostile_ones_are_sanitised() {
    use vizcraft_diagram::import_record::{ImportRecord, MAX_SOURCE};
    let mut doc = modelled();
    doc.imports.push(ImportRecord {
        source: "/tmp/flow.mmd".into(),
        format: "mermaid".into(),
        time: 1_700_000_000,
        page: doc.pages[0].id.clone(),
        elements: [("A".to_string(), "e1".to_string()), ("B".to_string(), "e2".to_string())].into_iter().collect(),
        relationships: Vec::new(),
        removed: vec!["e2".into()],
    });
    let mut v = parse(&save(&doc).unwrap());
    assert_eq!(v["document"]["imports"][0]["elements"]["A"], "e1");
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(back.imports, doc.imports);
    v["document"]["imports"] = json!([
        {"source": "s".repeat(MAX_SOURCE * 2), "elements": {"A": "nope", "B": "e1", "": "e2"}, "removed": ["e1", 5, "missing"], "time": "x"},
        {"source": 7},
        "junk"
    ]);
    let back = load(&serde_json::to_vec(&v).unwrap()).unwrap();
    let r = &back.imports[0];
    assert!(r.source.len() <= MAX_SOURCE);
    assert_eq!(r.elements.len(), 1, "ids the model lacks are dropped");
    assert_eq!(r.removed, vec!["e1".to_string()]);
    assert_eq!(r.time, 0);
}

/// A file written before Block 6E (no nets, no orientation, no module) loads and saves back
/// byte for byte.
#[test]
fn a_pre_6e_file_is_byte_identical_after_load_and_save() {
    let bytes = include_bytes!("../testdata/pre-6e-sample.vizcraft");
    let doc = load(bytes).unwrap();
    assert!(doc.model.nets.is_empty() && doc.pages.iter().flat_map(|p| &p.shapes).all(|s| s.orient.is_default()));
    assert_eq!(String::from_utf8(save(&doc).unwrap()).unwrap(), String::from_utf8(bytes.to_vec()).unwrap());
}

/// Nets, segment membership, line markers, orientation and the page module round trip.
#[test]
fn nets_markers_orientation_and_module_round_trip() {
    use vizcraft_diagram::vizcraft_model::{Module, Net, NetKind, NetLabel};
    use vizcraft_diagram::{LineMarker, Orient, Point};
    let mut doc = Document::new();
    let mut net = Net { name: "24V".into(), number: "2003".into(), label: NetLabel::Each, ..Net::new("n1", NetKind::Potential) };
    net.properties.insert("colour".into(), PropValue::Text("BU".into()));
    doc.model.nets.push(net);
    let page = doc.page_mut().unwrap();
    page.view = Some(View { module: Module::Iso2_5mm, ..View::default() });
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 40.0, 20.0);
    s.orient = Orient { turns: 3, mirror: true };
    page.shapes.push(s);
    let mut c = Connector::new("c1", End::point("s1", 0), End::free(Point::new(100.0, 0.0)));
    c.net = Some("n1".into());
    c.marker = LineMarker::Wave;
    page.connectors.push(c);
    let bytes = save(&doc).unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["model"]["nets"][0]["kind"], "potential", "a document holding only wiring keeps its model section");
    assert_eq!(v["document"]["pages"][0]["view"]["module"], "iso2_5mm");
    assert_eq!(v["document"]["pages"][0]["shapes"][0]["orient"], json!({"turns": 3, "mirror": true}));
    assert_eq!(v["document"]["pages"][0]["connectors"][0]["net"], "n1");
    assert_eq!(v["document"]["pages"][0]["connectors"][0]["marker"], "wave");
    let back = load(&bytes).unwrap();
    assert_eq!(back, doc);
    assert_eq!(save(&back).unwrap(), bytes);
    // A custom module is a number on disk.
    let mut doc = back;
    doc.page_mut().unwrap().view = Some(View { module: Module::Custom(12.5), ..View::default() });
    let v: Value = serde_json::from_slice(&save(&doc).unwrap()).unwrap();
    assert_eq!(v["document"]["pages"][0]["view"]["module"], 12.5);
}

/// Junk nets, orientations, markers and modules load as defaults, and dangling net references
/// are cleared.
#[test]
fn junk_nets_and_orientation_sanitise() {
    let file = json!({
        "format": "vizcraft",
        "version": 4,
        "model": {"schemaVersion": 1, "nets": [{"id": "n1", "kind": "plasma", "label": 7}, {"id": "n1"}, {"id": ""}, 5, {"id": "n2", "name": "x".repeat(1000)}]},
        "document": {"pages": [{"id": "p1", "name": "P", "view": {"module": "parsec"},
            "shapes": [{"id": "s1", "kind": "rectangle", "orient": {"turns": 9, "mirror": "maybe"}}, {"id": "s2", "orient": "sideways"}],
            "connectors": [
                {"id": "c1", "start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 1}, "net": "n1", "marker": "zigzag"},
                {"id": "c2", "start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 1}, "net": "ghost"},
                {"id": "c3", "start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 1}, "net": 5}
            ]}]}
    });
    let doc = load(file.to_string().as_bytes()).unwrap();
    let ids: Vec<&str> = doc.model.nets.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["n1", "n2"]);
    assert_eq!(doc.model.nets[0].kind, vizcraft_diagram::vizcraft_model::NetKind::Wire);
    assert!(doc.model.nets[1].name.len() <= vizcraft_diagram::vizcraft_model::MAX_TAG);
    let p = &doc.pages[0];
    assert!(p.view.as_ref().is_some_and(|v| v.module.is_none()));
    assert_eq!(p.shape("s1").unwrap().orient, vizcraft_diagram::Orient { turns: 1, mirror: false });
    assert!(p.shape("s2").unwrap().orient.is_default());
    let net = |id: &str| p.connector(id).unwrap().net.clone();
    assert_eq!((net("c1"), net("c2"), net("c3")), (Some("n1".into()), None, None));
    assert_eq!(p.connector("c1").unwrap().marker, vizcraft_diagram::LineMarker::None);
}

#[test]
fn sheets_document_fields_table_columns_and_per_owner_round_trip() {
    use vizcraft_diagram::compartment::{Column, Compartment, CompartmentKind};
    use vizcraft_diagram::sheet::{Sheet, SheetSize, SheetStandard};
    let mut doc = Document::new();
    let mut sh = Sheet::preset(SheetStandard::AsmeY14_1, SheetSize::AnsiD, vizcraft_diagram::Orientation::Landscape);
    sh.number = Some(3);
    doc.pages[0].sheet = Some(sh);
    doc.properties.fields.insert("legalOwner".into(), "Works Ltd".into());
    let mut s = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 200.0, 80.0);
    let mut c = Compartment::new("rows", CompartmentKind::List);
    c.columns = vec![Column { title: "ITEM".into(), width: 40.0 }, Column { title: "QTY".into(), width: 30.0 }];
    c.rows = vec!["1\t2".into()];
    s.compartments.push(c);
    doc.pages[0].shapes.push(s);
    doc.sanitize();
    let bytes = save(&doc).unwrap();
    let loaded = load(&bytes).unwrap();
    assert_eq!(loaded, doc);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"sheet\"") && text.contains("\"asmeY14_1\"") && text.contains("\"columns\"") && text.contains("\"fields\""), "{text}");
    // perOwner is pack data; it survives a serde round trip and is left out when false.
    let scope = vizcraft_diagram::vizcraft_model::IdScope { per_owner: true, ..Default::default() };
    let v = serde_json::to_value(&scope).unwrap();
    assert_eq!(v["perOwner"], true);
    assert_eq!(serde_json::from_value::<vizcraft_diagram::vizcraft_model::IdScope>(v).unwrap(), scope);
    assert!(serde_json::to_value(vizcraft_diagram::vizcraft_model::IdScope::default()).unwrap().get("perOwner").is_none());
    // A document from before sheets opens unchanged: no sheet, no fields.
    let old = load(&save(&Document::new()).unwrap()).unwrap();
    assert!(old.pages[0].sheet.is_none() && old.properties.fields.is_empty());
}

#[test]
fn glyph_builder_state_round_trips_and_old_shapes_write_as_before() {
    use vizcraft_diagram::glyph_state::{GlyphLabel, GlyphState};
    let mut doc = Document::new();
    let mut s = Shape::new("s1", ShapeKind::Freeform, 10.0, 20.0, 19.685, 39.37);
    let path = vizcraft_diagram::geom::Path::polyline(&[vizcraft_diagram::Point::new(9.8, 0.0), vizcraft_diagram::Point::new(9.8, 14.0)]);
    s.path = Some(vizcraft_diagram::Freeform::in_frame(&path, false, 19.685, 39.37));
    s.glyph = Some(GlyphState {
        builder: "contact".into(),
        params: [("poles".to_string(), PropValue::Number(3.0)), ("form".to_string(), PropValue::Text("nc".into()))].into_iter().collect(),
        ports: vec!["11".into(), "12".into()],
        labels: vec![GlyphLabel { id: "tag".into(), text: "K1".into(), x: 1.2, y: 0.5, size: 11.8, anchor: "start".into() }],
    });
    s.sanitize();
    let plain = Shape::new("s2", ShapeKind::Rectangle, 0.0, 0.0, 50.0, 30.0);
    if let Some(p) = doc.page_mut() {
        p.shapes.extend([s, plain]);
    }
    let bytes = save(&doc).unwrap();
    let v = parse(&bytes);
    let shapes = &v["document"]["pages"][0]["shapes"];
    assert_eq!(shapes[0]["glyph"]["builder"], "contact");
    assert_eq!(shapes[0]["glyph"]["params"]["poles"], 3.0);
    assert_eq!(shapes[0]["path"]["frame"], json!([19.685, 39.37]));
    assert!(shapes[1].get("glyph").is_none(), "a plain shape writes no glyph field");
    assert_eq!(load(&bytes).unwrap().pages, doc.pages);
    // Junk glyph state loads tolerantly.
    let mut junk = v.clone();
    junk["document"]["pages"][0]["shapes"][0]["glyph"] = json!({"builder": [1], "params": {"poles": {}, "form": "no"}, "ports": "x", "labels": [5]});
    let back = load(&serde_json::to_vec(&junk).unwrap()).unwrap();
    let g = back.page().unwrap().shapes[0].glyph.clone().unwrap();
    assert_eq!((g.builder.as_str(), g.params.len(), g.ports.len(), g.labels.len()), ("", 1, 0, 0));
    junk["document"]["pages"][0]["shapes"][0]["glyph"] = json!("not a glyph");
    assert!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().page().unwrap().shapes[0].glyph.is_none());
}

#[test]
fn project_standards_round_trip_and_an_old_file_loads_with_none() {
    use vizcraft_diagram::standards::Standards;
    let mut doc = modelled();
    doc.standards = Standards { symbols: "jic".into(), sheet: "ansi.b.landscape".into() };
    let bytes = save(&doc).unwrap();
    assert_eq!(parse(&bytes)["document"]["standards"], json!({"symbols": "jic", "sheet": "ansi.b.landscape"}));
    assert_eq!(load(&bytes).unwrap().standards, doc.standards);
    // A file written before the field existed, and a junk value, read as the defaults.
    let free = save(&Document::new()).unwrap();
    assert!(parse(&free)["document"].get("standards").is_none());
    let old = load(&free).unwrap();
    assert_eq!((old.standards.symbols(), old.standards.sheet()), ("iec", "iso.a3.landscape"));
    let mut junk = parse(&free);
    junk["document"]["standards"] = json!(7);
    assert_eq!(load(&serde_json::to_vec(&junk).unwrap()).unwrap().standards, Standards::default());
}

#[test]
fn connector_steps_round_trip_and_old_files_load_with_none() {
    let mut doc = Document::new();
    let mut c = Connector::new("c1", End::free(vizcraft_diagram::Point::new(0.0, 0.0)), End::free(vizcraft_diagram::Point::new(50.0, 0.0)));
    c.step = 4;
    doc.pages[0].connectors.push(c);
    doc.pages[0].connectors.push(Connector::new(
        "c2",
        End::free(vizcraft_diagram::Point::new(0.0, 9.0)),
        End::free(vizcraft_diagram::Point::new(9.0, 9.0)),
    ));
    let bytes = save(&doc).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert_eq!(text.matches("\"step\"").count(), 1, "only a set step is written");
    let back = load(&bytes).unwrap();
    assert_eq!(back.pages[0].connectors[0].step, 4);
    assert_eq!(back.pages[0].connectors[1].step, 0);
    let old = text.replace("\"step\": 4", "\"text\": \"\"");
    assert_eq!(load(old.as_bytes()).unwrap().pages[0].connectors[0].step, 0);
}

#[test]
fn double_outlines_connector_halves_and_label_frames_round_trip_and_old_files_load_plain() {
    use vizcraft_diagram::connector_look::{Half, Halves, LabelFrame};
    use vizcraft_diagram::{Compound, Dash};
    let mut doc = kinds_doc();
    doc.pages[0].shapes[0].style.compound = Compound::Double;
    let c = &mut doc.pages[0].connectors[0];
    c.halves = Halves { start: Half { dash: Some(Dash::Dashed), double: true }, end: Half::default() };
    c.label_frame = LabelFrame::DoubleDiamond;
    let bytes = save(&doc).unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert!(text.contains("\"compound\"") && text.contains("\"labelFrame\"") && text.contains("\"halves\""));
    assert_eq!(load(&bytes).unwrap(), doc);
    // A file written before these fields existed loads with one outline, plain halves and the box.
    let old = load(&save(&kinds_doc()).unwrap()).unwrap();
    assert!(!String::from_utf8_lossy(&save(&kinds_doc()).unwrap()).contains("\"compound\""));
    assert_eq!(old.pages[0].shapes[0].style.compound, Compound::Single);
    assert!(old.pages[0].connectors.iter().all(|c| c.halves.is_plain() && c.label_frame == LabelFrame::Box));
}
