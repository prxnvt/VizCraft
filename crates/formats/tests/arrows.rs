//! Every connector line end, drawn at both ends of a connector at two line widths: the SVG is
//! compared with a golden file, and the same document must also export to PDF and PNG.
//!
//! To refresh the golden file after a deliberate change to the end geometry, run the test with
//! `VIZCRAFT_UPDATE_GOLDEN=1` and review the diff of `testdata/arrows.golden.svg`.

use vizcraft_diagram::{Arrow, Connector, Document, End, Point, Route};
use vizcraft_formats::pdf::export_pages;
use vizcraft_formats::svg::{SvgOptions, export_page};

const GOLDEN: &str = "testdata/arrows.golden.svg";
const WIDTHS: [f64; 2] = [1.0, 4.0];
const ROW: f64 = 50.0;

/// One straight connector per (end, width): the same end at the start and at the finish. The
/// thin ones run down the left column and the heavy ones down the right.
fn sheet() -> Document {
    let ends: Vec<Arrow> = Arrow::EVERY.into_iter().filter(|a| *a != Arrow::None).collect();
    let mut doc = Document::new();
    let page = doc.pages.first_mut().expect("a new document has a page");
    page.width = 680.0;
    page.height = ROW * ends.len() as f64 + 40.0;
    // A fixed sheet, so the golden frames the page and not the drawing bounds.
    page.board = vizcraft_diagram::Board::Fixed;
    for (i, arrow) in ends.iter().enumerate() {
        let y = 40.0 + ROW * i as f64;
        for (col, width) in WIDTHS.into_iter().enumerate() {
            let x0 = 40.0 + 330.0 * col as f64;
            let mut c = Connector::new(format!("c_{}_{}", arrow.name(), width), End::free(Point::new(x0, y)), End::free(Point::new(x0 + 280.0, y)));
            c.route = Route::Straight;
            c.start_arrow = *arrow;
            c.end_arrow = *arrow;
            c.style.line_width = width;
            page.connectors.push(c);
        }
    }
    doc
}

#[test]
fn every_end_at_both_ends_matches_the_golden_svg() {
    let doc = sheet();
    let svg = String::from_utf8(export_page(&doc, 0).expect("the sheet exports")).expect("SVG is UTF-8");
    let path = format!("{}/{GOLDEN}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &svg).expect("the golden file is writable");
    }
    let want = std::fs::read_to_string(&path).expect("testdata/arrows.golden.svg exists (run with VIZCRAFT_UPDATE_GOLDEN=1 to create it)");
    assert_eq!(svg, want, "the end geometry changed; refresh the golden file if that was intended");
}

#[test]
fn every_end_is_drawn_and_never_leaves_the_page() {
    let doc = sheet();
    let svg = String::from_utf8(export_page(&doc, 0).expect("the sheet exports")).expect("SVG is UTF-8");
    // Per connector: the line, then one path for each head (both ends carry a head).
    let heads: usize = Arrow::EVERY.into_iter().filter(|a| *a != Arrow::None).count() * WIDTHS.len() * 2;
    let connectors = (Arrow::EVERY.len() - 1) * WIDTHS.len();
    assert_eq!(svg.matches("<path ").count(), connectors + heads);
    assert!(svg.contains("width=\"680\""), "the sheet fits its page, so the canvas is the page");
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn the_sheet_exports_to_pdf() {
    let doc = sheet();
    let pdf = export_pages(&doc, &[0], &SvgOptions::default()).expect("the PDF renders");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn the_sheet_renders_to_png() {
    let doc = sheet();
    let (png, w, h) = vizcraft_formats::png::render_page(&doc, 0, 1.0).expect("the PNG renders");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']) && w > 0 && h > 0);
    if let Some(dir) = std::env::var_os("VIZCRAFT_ARROWS_PNG") {
        std::fs::write(dir, &png).expect("the PNG is writable");
    }
}
