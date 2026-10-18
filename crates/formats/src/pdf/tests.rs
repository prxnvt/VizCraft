//! PDF export and print layout tests. PDF streams are compressed, so text is checked in the SVG
//! the PDF is built from and the PDF itself is checked for its structure (page count, fonts).

use vizcraft_diagram::{HeaderFooter, Layer, Page, Shape, ShapeKind};

use super::*;
use crate::svg::export_page_with;

/// The `/Count` of the page tree: the number of pages in the file.
pub(crate) fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.split("/Count").skip(1).filter_map(|rest| rest.trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).max().unwrap_or(0)
}

fn two_pages() -> Document {
    let mut doc = Document::new();
    doc.title = "Works".into();
    let mut second = Page::new("p2", "Second");
    second.shapes.push(Shape::new("b", ShapeKind::Ellipse, 100.0, 100.0, 200.0, 100.0));
    doc.pages.push(second);
    for p in &mut doc.pages {
        p.board = vizcraft_diagram::Board::Fixed;
        p.header = HeaderFooter { left: "{title}".into(), right: "{date}".into(), ..HeaderFooter::default() };
        p.footer = HeaderFooter { center: "Page {page} of {pages}: {pageName}".into(), margin: Some(40.0), ..HeaderFooter::default() };
    }
    if let Some(p) = doc.pages.first_mut() {
        let mut s = Shape::new("a", ShapeKind::Rectangle, 50.0, 50.0, 200.0, 100.0);
        s.text = "Hello".into();
        p.shapes.push(s);
    }
    doc
}

fn svg_text(doc: &Document, i: usize, opts: &SvgOptions) -> String {
    String::from_utf8(export_page_with(doc, i, opts).unwrap()).unwrap()
}

#[test]
fn a_two_page_pdf_has_two_pages_and_embedded_fonts() {
    let doc = two_pages();
    let pdf = export_pages(&doc, &[0, 1], &SvgOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(page_count(&pdf), 2);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/FontFile"), "the Inter subset is embedded");
    assert!(text.contains("/MediaBox [0 0 792 612]") || text.contains("/MediaBox[0 0 792 612]"), "letter landscape at 0.72 pt per unit");
    let one = export_pages(&doc, &[1], &SvgOptions::default()).unwrap();
    assert_eq!(page_count(&one), 1);
}

#[test]
fn headers_and_footers_expand_their_fields() {
    let doc = two_pages();
    let opts = SvgOptions { date: "2026-10-17".into(), ..SvgOptions::default() };
    let first = svg_text(&doc, 0, &opts);
    assert!(first.contains(">Works</tspan>") && first.contains(">2026-10-17</tspan>"), "{first}");
    assert!(first.contains(">Page 1 of 2: Page-1</tspan>"), "{first}");
    // The footer sits its 40 unit margin above the bottom edge (850 - 40).
    assert!(first.contains("y=\"810\">Page 1 of 2"), "{first}");
    let second = svg_text(&doc, 1, &opts);
    assert!(second.contains(">Page 2 of 2: Second</tspan>"), "{second}");
    // Without a date the last save's date is used, and an unsaved document has none.
    let plain = svg_text(&doc, 0, &SvgOptions::default());
    assert!(!plain.contains("{date}") && !plain.contains("2026"), "{plain}");
}

#[test]
fn printing_leaves_out_non_printing_shapes_and_layers() {
    let mut doc = Document::new();
    if let Some(p) = doc.pages.first_mut() {
        p.layers = vec![Layer { id: "notes".into(), print: false, ..Layer::default() }];
        let mut on_layer = Shape::new("noted", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        on_layer.layers = vec!["notes".into()];
        let mut flagged = Shape::new("flagged", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        flagged.behavior.non_printing = true;
        // Far past the page: it would grow the canvas on screen but not on paper.
        let mut far = Shape::new("far", ShapeKind::Rectangle, 5000.0, 0.0, 10.0, 10.0);
        far.behavior.non_printing = true;
        p.shapes.extend([on_layer, flagged, far, Shape::new("kept", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0)]);
    }
    let screen = svg_text(&doc, 0, &SvgOptions::default());
    assert!(screen.contains("<g id=\"noted\">") && screen.contains("<g id=\"flagged\">"), "export matches the screen by default");
    let print = svg_text(&doc, 0, &SvgOptions { print: true, ..SvgOptions::default() });
    assert!(!print.contains("<g id=\"noted\">") && !print.contains("<g id=\"flagged\">") && !print.contains("<g id=\"far\">"), "{print}");
    assert!(print.contains("<g id=\"kept\">"));
    let printed = print_pages(&doc, &[0], &SvgOptions { print: true, ..SvgOptions::default() }, &PrintLayout::default()).unwrap();
    assert_eq!((printed.sheets(), page_count(&printed.pdf)), (1, 1), "the far non-printing shape does not add sheets");
}

#[test]
fn a_large_drawing_tiles_unless_fit() {
    let mut doc = Document::new();
    if let Some(p) = doc.pages.first_mut() {
        p.board = vizcraft_diagram::Board::Fixed;
        // Reaches to x 2500 on an 1100 wide page: three sheets across, one down.
        p.shapes.push(Shape::new("wide", ShapeKind::Rectangle, 0.0, 100.0, 2500.0, 100.0));
    }
    let opts = SvgOptions { print: true, ..SvgOptions::default() };
    let tiled = print_pages(&doc, &[0], &opts, &PrintLayout::default()).unwrap();
    let p = tiled.plans[0].1;
    assert_eq!((p.cols, p.rows, p.scale), (3, 1, 1.0));
    assert_eq!(page_count(&tiled.pdf), 3);
    let fit = print_pages(&doc, &[0], &opts, &PrintLayout { fit: true, ..PrintLayout::default() }).unwrap();
    assert_eq!((fit.sheets(), page_count(&fit.pdf)), (1, 1));
    assert!(fit.plans[0].1.scale < 0.5);
    let grid = print_pages(&doc, &[0], &opts, &PrintLayout { tiles: Some((2, 2)), ..PrintLayout::default() }).unwrap();
    assert_eq!(page_count(&grid.pdf), 4);
    // A page that fits prints on one sheet; A4 paper is used when asked for.
    let small = plan(&Document::new(), 0, &PrintLayout::default(), true).unwrap();
    assert_eq!((small.cols, small.rows, small.scale), (1, 1, 1.0));
    let a4 = plan(&doc, 0, &PrintLayout { paper: Some((827.0, 1169.0)), ..PrintLayout::default() }, true).unwrap();
    assert_eq!((a4.cols, a4.rows), (4, 1));
}

#[test]
fn hostile_layouts_and_pages_are_errors_or_capped() {
    let doc = two_pages();
    let opts = SvgOptions::default();
    assert!(export_pages(&doc, &[], &opts).is_err());
    assert!(matches!(export_pages(&doc, &[7], &opts), Err(FormatError::NoPage(7))));
    assert!(export_pages(&doc, &vec![0; MAX_SHEETS + 1], &opts).is_err());
    assert!(plan(&doc, usize::MAX, &PrintLayout::default(), false).is_err());
    for layout in [
        PrintLayout { paper: Some((0.0, -1.0)), ..PrintLayout::default() },
        PrintLayout { paper: Some((f64::NAN, f64::INFINITY)), ..PrintLayout::default() },
        PrintLayout { tiles: Some((0, u32::MAX)), ..PrintLayout::default() },
        PrintLayout { paper: Some((1.0, 1.0)), ..PrintLayout::default() },
    ] {
        let p = plan(&doc, 0, &layout, false).unwrap();
        assert!(p.cols >= 1 && p.rows >= 1 && p.cols <= MAX_TILES && p.rows <= MAX_TILES, "{layout:?}: {p:?}");
        assert!(p.scale.is_finite() && p.scale > 0.0, "{layout:?}: {p:?}");
    }
    // A tiny paper would need thousands of sheets; it is capped and scaled down instead.
    let tiny = print_pages(&doc, &[0], &opts, &PrintLayout { paper: Some((1.0, 1.0)), ..PrintLayout::default() });
    assert!(tiny.map(|t| t.sheets() <= MAX_SHEETS).unwrap_or(true));
    // A huge page is scaled to the PDF size limit.
    let mut huge = Document::new();
    if let Some(p) = huge.pages.first_mut() {
        p.width = vizcraft_diagram::MAX_EXTENT;
        p.height = 10.0;
    }
    let pdf = export_pages(&huge, &[0], &opts).unwrap();
    assert_eq!(page_count(&pdf), 1);
}
