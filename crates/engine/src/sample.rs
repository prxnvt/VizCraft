//! The sample document: a small flowchart on page 1, built from the stencil catalog, so the app,
//! tests and screenshots start from something that exercises shapes, glue, arrows and labels.

use vizcraft_diagram::{Arrow, Connector, End, Route, Shape};

use crate::doc::Document;

/// Centre line of the main column.
const COLUMN_X: f64 = 450.0;

/// The sample flowchart: Start, Gather input, Valid?, Save diagram, End, with a "No" branch to
/// Fix input that loops back. Connectors use point glue, orthogonal routes and triangle arrows.
pub fn sample_document() -> Document {
    // The title stays empty for now: the title bar shows it, and the chrome snapshots and tests
    // (owned by the UI pieces) expect "Untitled". Set it to "Sample flowchart" when the canvas
    // piece re-records the snapshots.
    let mut doc = Document::new();

    // (master, text, top y); the main column is centred on COLUMN_X.
    let column = [
        ("flowchart.terminator", "Start", 40.0),
        ("flowchart.process", "Gather input", 150.0),
        ("flowchart.decision", "Valid?", 270.0),
        ("flowchart.process", "Save diagram", 420.0),
        ("flowchart.terminator", "End", 540.0),
    ];
    let mut shapes: Vec<Shape> = Vec::new();
    for (master, text, y) in column {
        let id = doc.next_id("s");
        if let Some(mut s) = vizcraft_stencils::new_shape(master, id, 0.0, y, None, None) {
            s.x = COLUMN_X - s.w / 2.0;
            s.text = text.into();
            shapes.push(s);
        }
    }
    // The fix-up step sits right of the decision, level with its centre.
    let fix_id = doc.next_id("s");
    if let Some(mut s) = vizcraft_stencils::new_shape("flowchart.process", fix_id, 650.0, 285.0, None, None) {
        s.text = "Fix input".into();
        shapes.push(s);
    }
    let id = |i: usize| shapes.get(i).map(|s| s.id.clone()).unwrap_or_default();
    // Connection points are top, right, bottom, left (0..3) for every flowchart master.
    let (top, right, bottom, left) = (0, 1, 2, 3);
    let links = [
        (id(0), bottom, id(1), top, ""),
        (id(1), bottom, id(2), top, ""),
        (id(2), bottom, id(3), top, "Yes"),
        (id(2), right, id(5), left, "No"),
        (id(5), top, id(1), right, ""),
        (id(3), bottom, id(4), top, ""),
    ];
    let mut connectors = Vec::new();
    for (from, fp, to, tp, text) in links {
        let mut c = Connector::new(doc.next_id("c"), End::point(from, fp), End::point(to, tp));
        c.route = Route::Orthogonal;
        c.end_arrow = Arrow::Triangle;
        c.text = text.into();
        connectors.push(c);
    }
    if let Some(page) = doc.page_mut() {
        page.shapes = shapes;
        page.connectors = connectors;
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_a_glued_flowchart_that_fits_the_page() {
        let doc = sample_document();
        let page = doc.page().unwrap();
        assert_eq!(page.shapes.len(), 6);
        assert_eq!(page.connectors.len(), 6);
        let texts: Vec<_> = page.shapes.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["Start", "Gather input", "Valid?", "Save diagram", "End", "Fix input"]);
        for s in &page.shapes {
            assert!(page.rect().contains_rect(&s.bounds()), "{} off the page", s.id);
        }
        for c in &page.connectors {
            assert!(c.start.shape().is_some() && c.end.shape().is_some(), "{} not glued", c.id);
            let pts = vizcraft_diagram::route_polyline(page, c);
            assert!(pts.windows(2).all(|w| w[0].x == w[1].x || w[0].y == w[1].y), "{} not orthogonal", c.id);
        }
        let labels: Vec<_> = page.connectors.iter().filter(|c| !c.text.is_empty()).map(|c| c.text.as_str()).collect();
        assert_eq!(labels, ["Yes", "No"]);
        // Reads top to bottom: each main-column step starts below the previous one.
        assert!(page.shapes.windows(2).take(4).all(|w| w[1].y > w[0].y + w[0].h));
    }
}
