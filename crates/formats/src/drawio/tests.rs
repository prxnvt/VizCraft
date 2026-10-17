use vizcraft_diagram::{Arrow, Color, Connector, Document, End, Page, Point, Route, Shape, ShapeKind};

use super::*;

const DOCS_EXAMPLE: &[u8] = include_bytes!("../../testdata/drawio-docs-example.drawio");
const MULTIPAGE: &[u8] = include_bytes!("../../testdata/multipage-compressed.drawio");

fn model(cells: &str) -> String {
    format!("<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>{cells}</root></mxGraphModel>")
}

/// Load, save and load again: the pages must come back equal.
fn round_trip(bytes: &[u8]) -> Document {
    let first = load(bytes).unwrap();
    let saved = save(&first).unwrap();
    let second = load(&saved).unwrap();
    assert_eq!(first.pages, second.pages, "saved as:\n{}", String::from_utf8_lossy(&saved));
    first
}

#[test]
fn docs_example_loads_and_round_trips() {
    let doc = round_trip(DOCS_EXAMPLE);
    assert_eq!(doc.pages.len(), 1);
    let page = doc.page().unwrap();
    assert_eq!((page.id.as_str(), page.name.as_str()), ("docs-example", "Page-1"));
    assert_eq!(page.shapes.len(), 2);
    let hello = page.shape("2").unwrap();
    assert_eq!((hello.text.as_str(), hello.x, hello.y, hello.w, hello.h), ("Hello,", 20.0, 20.0, 80.0, 30.0));
    assert_eq!(hello.kind, ShapeKind::Rectangle);
    assert_eq!(hello.style.fill, Color::WHITE);
    assert_eq!(page.shape("3").unwrap().text, "World!");
    let c = page.connector("4").unwrap();
    assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic("2"), End::dynamic("3")));
    assert_eq!((c.route, c.start_arrow, c.end_arrow), (Route::Straight, Arrow::None, Arrow::Triangle));
}

/// The multi-page fixture's document: every kind, point and dynamic glue, a free end, bends,
/// jumps, the three routes, styled text and kept draw.io data.
fn multipage_document() -> Document {
    let mut doc = Document::new();
    let kinds = [
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Diamond,
        ShapeKind::Triangle,
        ShapeKind::Parallelogram,
        ShapeKind::Hexagon,
        ShapeKind::Cylinder,
        ShapeKind::Document,
        ShapeKind::Cloud,
        ShapeKind::Terminator,
        ShapeKind::PredefinedProcess,
        ShapeKind::ManualInput,
        ShapeKind::Preparation,
        ShapeKind::Trapezoid,
        ShapeKind::OffPageReference,
    ];
    let page = doc.page_mut().unwrap();
    page.name = "Kinds".into();
    for (i, k) in kinds.iter().enumerate() {
        let mut s = Shape::new(format!("k{i}"), *k, 40.0 + 160.0 * (i % 4) as f64, 40.0 + 120.0 * (i / 4) as f64, 120.0, 70.0);
        s.stencil = read::master_for_kind(*k).into();
        s.text = format!("Kind {i}");
        page.shapes.push(s);
    }
    if let Some(s) = page.shapes.get_mut(1) {
        s.text = "Two\nlines".into();
        s.style.bold = true;
        s.style.fill = Color::rgb(0x12, 0x34, 0x56);
        s.angle_deg = 30.0;
        s.ext.insert("drawio.style".into(), "dashed=1;shadow=1".into());
    }
    if let Some(s) = page.shapes.get_mut(2) {
        s.text = "spaced  out <b>not bold</b> & more".into();
        s.style.italic = true;
        s.style.line = Color::TRANSPARENT;
    }
    page.connectors.push(Connector { id: "e1".into(), start: End::point("k0", 1), end: End::point("k1", 3), ..Connector::default() });
    page.connectors.push(Connector {
        id: "e2".into(),
        start: End::dynamic("k2"),
        end: End::free(Point::new(700.0, 600.0)),
        route: Route::Curved,
        start_arrow: Arrow::Diamond,
        end_arrow: Arrow::Open,
        text: "label".into(),
        waypoints: vec![Point::new(300.0, 500.0), Point::new(420.0, 520.0)],
        jumps: true,
        ..Connector::default()
    });
    page.connectors.push(Connector {
        id: "e3".into(),
        start: End::free(Point::new(10.0, 10.0)),
        end: End::dynamic("k3"),
        route: Route::Straight,
        start_arrow: Arrow::Circle,
        end_arrow: Arrow::None,
        ..Connector::default()
    });
    let second = doc.add_page("Second");
    if let Some(p) = doc.pages.get_mut(second) {
        p.width = 1200.0;
        p.height = 900.0;
        p.shapes.push(Shape::new("a", ShapeKind::Ellipse, 100.0, 100.0, 80.0, 80.0));
        p.shapes.push(Shape::new("b", ShapeKind::Rectangle, 400.0, 100.0, 80.0, 80.0));
        for s in &mut p.shapes {
            s.stencil = read::master_for_kind(s.kind).into();
        }
        p.connectors.push(Connector { id: "ab".into(), start: End::dynamic("a"), end: End::dynamic("b"), ..Connector::default() });
    }
    doc.sanitize();
    doc
}

/// Writes `testdata/multipage-compressed.drawio` with our own codec: the document above, its
/// second page stored compressed. Run with `--ignored` to regenerate the fixture.
#[test]
#[ignore]
fn write_multipage_fixture() {
    let bytes = save(&multipage_document()).unwrap();
    let mut file = vizcraft_drawio::read_file(&bytes).unwrap();
    file.pages.get_mut(1).unwrap().set_compressed(true);
    let out = vizcraft_drawio::write_file(&file);
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/multipage-compressed.drawio"), out).unwrap();
}

#[test]
fn multipage_compressed_fixture_round_trips() {
    let file = vizcraft_drawio::read_file(MULTIPAGE).unwrap();
    assert!(!file.pages.first().unwrap().compressed() && file.pages.get(1).unwrap().compressed());
    let doc = round_trip(MULTIPAGE);
    let want = multipage_document();
    assert_eq!(doc.pages.len(), 2);
    for (got, want) in doc.pages.iter().zip(&want.pages) {
        assert_eq!((&got.name, got.width, got.height), (&want.name, want.width, want.height));
        assert_eq!(got.shapes.len(), want.shapes.len());
        for (a, b) in got.shapes.iter().zip(&want.shapes) {
            assert_eq!(
                (&a.id, a.kind, &a.stencil, &a.text, a.x, a.y, a.w, a.h, a.angle_deg),
                (&b.id, b.kind, &b.stencil, &b.text, b.x, b.y, b.w, b.h, b.angle_deg)
            );
            assert_eq!(a.style, b.style, "{}", a.id);
            assert_eq!(a.ext, b.ext, "{}", a.id);
        }
        assert_eq!(got.connectors, want.connectors);
    }
}

#[test]
fn unknown_style_keys_and_wrappers_are_kept() {
    let xml = model(
        r#"<UserObject label="Box" tooltip="hint" owner="ops" id="u1"><mxCell style="shape=mxgraph.custom.thing;dashed=1;glass=1;html=1;fillColor=#ff0000;" vertex="1" parent="1"><mxGeometry x="10" y="10" width="50" height="40" as="geometry"/></mxCell></UserObject>
           <mxCell id="e" style="dashed=1;endArrow=classic;fontStyle=5;" edge="1" parent="1" source="u1"><mxGeometry relative="1" as="geometry"><mxPoint x="300" y="300" as="targetPoint"/></mxGeometry></mxCell>"#,
    );
    let imp = import(xml.as_bytes()).unwrap();
    let page = imp.pages.first().unwrap();
    let s = page.shape("u1").unwrap();
    assert_eq!(s.text, "Box");
    assert_eq!(s.style.fill, Color::rgb(255, 0, 0));
    assert_eq!(s.ext.get("drawio.style").map(String::as_str), Some("shape=mxgraph.custom.thing;dashed=1;glass=1"));
    assert_eq!(s.ext.get("drawio.wrapper").map(String::as_str), Some("UserObject"));
    assert_eq!(s.ext.get("drawio.attr.owner").map(String::as_str), Some("ops"));
    assert!(imp.warnings.iter().any(|w| w.contains("mxgraph.custom.thing")));
    let c = page.connector("e").unwrap();
    assert!(c.style.bold && !c.style.italic);
    assert_eq!(c.ext.get("drawio.fontStyle").map(String::as_str), Some("4"));
    assert_eq!(c.end, End::free(Point::new(300.0, 300.0)));

    let text = page_xml(page).unwrap();
    for needle in ["shape=mxgraph.custom.thing", "dashed=1", "glass=1", "tooltip=\"hint\"", "owner=\"ops\"", "<UserObject", "fontStyle=5"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    let again = import(text.as_bytes()).unwrap();
    assert_eq!(again.pages.first().unwrap(), page);
}

#[test]
fn groups_layers_labels_and_images() {
    let xml = format!(
        r#"<mxfile><diagram id="d" name="One">{}</diagram></mxfile>"#,
        r#"<mxGraphModel pageWidth="500" pageHeight="400" grid="1"><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="L2" parent="0"/>
        <mxCell id="g" style="group" vertex="1" parent="1"><mxGeometry x="100" y="50" width="200" height="100" as="geometry"/></mxCell>
        <mxCell id="in" value="Inner" style="ellipse;" vertex="1" parent="g"><mxGeometry x="10" y="20" width="40" height="40" as="geometry"/></mxCell>
        <mxCell id="pic" style="shape=image;image=data:x" vertex="1" parent="L2"><mxGeometry x="400" y="300" width="20" height="20" as="geometry"/></mxCell>
        <mxCell id="d1" value="Choice" style="rhombus;exitX=1;exitY=0.5;" vertex="1" parent="L2"><mxGeometry x="0" y="0" width="60" height="60" as="geometry"/></mxCell>
        <mxCell id="e" style="edgeStyle=orthogonalEdgeStyle;exitX=1;exitY=0.5;entryX=0;entryY=0.5;jumpStyle=arc;" edge="1" parent="1" source="d1" target="in"><mxGeometry relative="1" as="geometry"><Array as="points"><mxPoint x="80" y="30"/></Array></mxGeometry></mxCell>
        <mxCell id="lab" value="yes" style="edgeLabel;html=1;" vertex="1" connectable="0" parent="e"><mxGeometry x="-0.5" relative="1" as="geometry"/></mxCell>
        <mxCell id="e2" edge="1" parent="1" source="in" target="pic"><mxGeometry relative="1" as="geometry"/></mxCell>
        <mxCell id="orphan" vertex="1" parent="nowhere"><mxGeometry width="5" height="5" as="geometry"/></mxCell>
        </root></mxGraphModel>"#
    );
    let imp = import(xml.as_bytes()).unwrap();
    let page = imp.pages.first().unwrap();
    assert_eq!((page.id.as_str(), page.name.as_str(), page.width, page.height), ("d", "One", 500.0, 400.0));
    assert_eq!(page.ext.get("drawio.model.grid").map(String::as_str), Some("1"));
    let ids: Vec<&str> = page.shapes.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["in", "d1"]);
    let inner = page.shape("in").unwrap();
    assert_eq!((inner.x, inner.y, inner.kind), (110.0, 70.0, ShapeKind::Ellipse));
    assert_eq!(page.shape("d1").unwrap().kind, ShapeKind::Diamond);
    assert_eq!(page.shape("d1").unwrap().stencil, "flowchart.decision");
    let e = page.connector("e").unwrap();
    assert_eq!(e.text, "yes");
    assert_eq!((e.route, e.jumps), (Route::Orthogonal, true));
    assert_eq!(e.waypoints, vec![Point::new(80.0, 30.0)]);
    // Diamond points: top, right, bottom, left; ellipse the same order.
    assert_eq!(e.start, End::point("d1", 1));
    assert_eq!(e.end, End::point("in", 3));
    // The end on the skipped image stays at the image's centre.
    assert_eq!(page.connector("e2").unwrap().end, End::free(Point::new(410.0, 310.0)));
    for needle in ["layers", "flattened", "images", "missing parent"] {
        assert!(imp.warnings.iter().any(|w| w.contains(needle)), "{needle}: {:?}", imp.warnings);
    }
}

#[test]
fn master_style_reads_back_as_the_master() {
    for set in vizcraft_stencils::sets() {
        for m in set.masters {
            let xml = model(&format!(
                r#"<mxCell id="s" value="x" style="{}" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>"#,
                master_style(m)
            ));
            let imp = import(xml.as_bytes()).unwrap();
            let s = imp.pages.first().unwrap().shape("s").unwrap();
            assert_eq!((s.kind, s.stencil.as_str()), (m.kind, m.id));
            assert_eq!(s.style, m.style, "{}", m.id);
            assert!(s.ext.is_empty(), "{}: {:?}", m.id, s.ext);
        }
    }
}

#[test]
fn structural_ids_are_not_reused_and_load_makes_ids_unique() {
    // Two pages that both use ids 2 and 3, and a layer with another id so a shape can be `1`.
    let xml = r#"<mxfile>
      <diagram id="a" name="A"><mxGraphModel><root><mxCell id="r"/><mxCell id="L" parent="r"/>
        <mxCell id="1" value="one" vertex="1" parent="L"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="2" vertex="1" parent="L"><mxGeometry x="50" width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="3" edge="1" parent="L" source="1" target="2"><mxGeometry relative="1" as="geometry"/></mxCell>
      </root></mxGraphModel></diagram>
      <diagram id="b" name="B"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="2" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell>
        <mxCell id="3" edge="1" parent="1" source="2"><mxGeometry relative="1" as="geometry"><mxPoint x="5" y="5" as="targetPoint"/></mxGeometry></mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let doc = load(xml.as_bytes()).unwrap();
    let mut ids = std::collections::HashSet::new();
    for p in &doc.pages {
        assert!(ids.insert(p.id.clone()));
        for s in &p.shapes {
            assert!(ids.insert(s.id.clone()), "{} twice", s.id);
        }
        for c in &p.connectors {
            assert!(ids.insert(c.id.clone()), "{} twice", c.id);
            for end in [&c.start, &c.end] {
                if let Some(s) = end.shape() {
                    assert!(p.shape(s).is_some(), "glue to {s} lost");
                }
            }
        }
    }
    let saved = String::from_utf8(save(&doc).unwrap()).unwrap();
    assert!(saved.contains("id=\"vizcraft-1\""), "{saved}");
    let again = load(saved.as_bytes()).unwrap();
    for (a, b) in again.pages.iter().zip(&doc.pages) {
        assert_eq!((a.shapes.len(), a.connectors.len()), (b.shapes.len(), b.connectors.len()));
    }
}

#[test]
fn truncated_and_garbage_input_are_errors() {
    for cut in [0, 1, 10, DOCS_EXAMPLE.len() / 3, DOCS_EXAMPLE.len() / 2, DOCS_EXAMPLE.len() - 12] {
        assert!(load(DOCS_EXAMPLE.get(..cut).unwrap()).is_err(), "cut at {cut}");
    }
    for cut in [MULTIPAGE.len() / 2, MULTIPAGE.len() - 20] {
        assert!(load(MULTIPAGE.get(..cut).unwrap()).is_err(), "cut at {cut}");
    }
    for junk in [
        &b"\xff\xfe\x00garbage"[..],
        b"not xml at all",
        b"<html><body/></html>",
        b"<mxfile></mxfile>",
        b"<mxfile><diagram>!!!notbase64</diagram></mxfile>",
    ] {
        assert!(load(junk).is_err(), "{}", String::from_utf8_lossy(junk));
        assert!(import(junk).is_err());
    }
}

#[test]
fn hostile_cells_never_panic() {
    let cells = [
        r#"<mxCell id="a" vertex="1" parent="a"><mxGeometry x="1e308" y="-1e308" width="NaN" height="-5" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" vertex="1" parent="c"/><mxCell id="c" vertex="1" parent="b"><mxGeometry as="geometry"/></mxCell>"#,
        r#"<mxCell id="e" edge="1" parent="1" source="e" target="zz" style="exitX=NaN;exitY=inf;fontStyle=-9;strokeWidth=1e308;rotation=inf;vizcraft=nope.nope"/>"#,
        r#"<mxCell id="v" vertex="1" parent="e" value="label on a missing edge"/>"#,
        r#"<mxCell id="1" vertex="1" parent="1"><mxGeometry width="1" height="1" as="geometry"/></mxCell>"#,
    ];
    for c in cells {
        let xml = model(c);
        if let Ok(doc) = load(xml.as_bytes()) {
            for p in &doc.pages {
                for s in &p.shapes {
                    assert!(s.x.is_finite() && s.y.is_finite() && s.w.is_finite() && s.h.is_finite());
                }
            }
            save(&doc).unwrap();
        }
    }
    let deep = format!("{}{}", "<mxCell id=\"x\" vertex=\"1\" parent=\"x\">".repeat(3), "</mxCell>".repeat(3));
    let _ = load(model(&deep).as_bytes());
}

#[test]
fn a_document_saved_as_drawio_keeps_its_drawing() {
    let mut doc = multipage_document();
    if let Some(p) = doc.page_mut() {
        p.connectors.push(Connector { id: "gone".into(), start: End::dynamic("missing"), end: End::dynamic("k0"), ..Connector::default() });
    }
    let back = load(&save(&doc).unwrap()).unwrap();
    let c = back.page().unwrap().connector("gone").unwrap();
    assert!(matches!(c.start, End::Free { .. }), "glue to a missing shape becomes a free end");
    let page: &Page = back.page().unwrap();
    assert_eq!(page.shapes.len(), 16);
}
