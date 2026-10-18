//! PDF export of compartments. PDF pages are drawn from the SVG export (see the `pdf` module
//! doc), so compartments need no PDF code of their own: these tests prove the class and the
//! equipment tag reach the PDF through that one drawing path.

use vizcraft_diagram::compartment::{Compartment, CompartmentKind};
use vizcraft_diagram::{Document, Shape, ShapeKind};

use super::tests::page_count;
use super::*;
use crate::svg::export_page_with;

fn doc() -> Document {
    let mut doc = Document::new();
    let mut class = Shape::new("k", ShapeKind::Rectangle, 10.0, 20.0, 160.0, 100.0);
    class.text = "Order".into();
    let mut attrs = Compartment::new("attrs", CompartmentKind::List);
    attrs.rows = vec!["+ id: int".into(), "+ total: money".into()];
    let mut ops = Compartment::new("ops", CompartmentKind::List);
    ops.rows = vec!["+ pay()".into()];
    class.compartments = vec![attrs, ops];
    let mut tag = Shape::new("t", ShapeKind::Rectangle, 300.0, 20.0, 120.0, 90.0);
    tag.text = "P-101".into();
    let mut desc = Compartment::new("desc", CompartmentKind::Paragraph);
    desc.text = "Feed pump".into();
    let mut fields = Compartment::new("fields", CompartmentKind::List);
    fields.rows = vec!["Flow: 12 m3/h".into(), "Head: 30 m".into()];
    tag.compartments = vec![desc, fields];
    if let Some(p) = doc.page_mut() {
        p.shapes.push(class);
        p.shapes.push(tag);
    }
    doc.sanitize();
    doc
}

#[test]
fn compartments_reach_the_pdf_through_the_svg_it_is_drawn_from() {
    let doc = doc();
    let opts = SvgOptions::default();
    let svg = String::from_utf8(export_page_with(&doc, 0, &opts).unwrap()).unwrap();
    for text in ["+ id: int", "+ total: money", "+ pay()", "Feed pump", "Flow: 12 m3/h", "Head: 30 m"] {
        assert!(svg.contains(&format!(">{text}</tspan>")), "{text} is drawn: {svg}");
    }
    let pdf = export_pages(&doc, &[0], &opts).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(page_count(&pdf), 1);
    // The rows are real text: the embedded Inter subset is in the file.
    assert!(String::from_utf8_lossy(&pdf).contains("FontFile"), "fonts embedded");
    let printed = print_pages(&doc, &[0], &SvgOptions { print: true, ..opts }, &PrintLayout::default()).unwrap();
    assert_eq!(printed.sheets(), 1);
}
