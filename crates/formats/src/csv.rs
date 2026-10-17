//! draw.io-style CSV import: rows become shapes, references between rows become connectors, and
//! the auto-layout places them as the CSV's `layout` directive asks.
//!
//! Row and edge styles go through the same mapping as `.drawio` files ([`crate::drawio`]), so a
//! `rhombus` row is a diamond here too. Containers (`parent` rows) are not modelled yet: their
//! members are kept as ordinary shapes, with a warning.

use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::{Connector, End, Page, Shape};
use vizcraft_drawio::{CsvDiagram, CsvLayout, CsvNode, NodeSize, Style as DStyle, html_to_text};

use crate::drawio::{Warnings, edge_look, short, vertex_look};
use crate::{FormatError, Imported};

/// Size of a shape whose row says nothing and whose label is short.
const DEFAULT_W: f64 = 120.0;
const DEFAULT_H: f64 = 60.0;
/// Rough width of one character and height of one line at the default font size, for sizing a
/// shape to its label.
const CHAR_W: f64 = 7.0;
const LINE_H: f64 = 16.0;

/// Read draw.io-style CSV text as one laid-out page.
pub fn import(text: &str) -> Result<Imported, FormatError> {
    let d = vizcraft_drawio::parse_csv(text).map_err(|e| FormatError::Import(format!("CSV: {e}")))?;
    if d.nodes.is_empty() {
        return Err(FormatError::Import("CSV: no data rows".into()));
    }
    let mut warnings = Warnings::default();
    for w in &d.warnings {
        warnings.push(w.clone());
    }
    let mut page = Page::new("", "Page-1");
    let containers = d.nodes.iter().filter(|n| n.is_container).count();
    if containers > 0 {
        warnings.push(format!("{containers} container rows are kept as ordinary shapes; their members are not grouped"));
    }
    let padding = d.directives.padding.filter(|p| p.is_finite() && *p >= 0.0).unwrap_or(10.0).min(1000.0);
    for n in &d.nodes {
        page.shapes.push(node_shape(n, padding, &mut warnings));
    }
    for e in &d.edges {
        let style = e.style.as_deref().map(DStyle::parse);
        let look = edge_look(style.as_ref());
        let html = style.as_ref().and_then(DStyle::html).unwrap_or(false);
        let text = e.label.as_deref().map(|l| if html { html_to_text(l) } else { l.to_string() }).unwrap_or_default();
        let mut c = Connector {
            // Left empty so the document hands out a fresh id when the page is adopted.
            id: String::new(),
            start: End::dynamic(e.source.clone()),
            end: End::dynamic(e.target.clone()),
            route: look.route,
            start_arrow: look.arrows.0,
            end_arrow: look.arrows.1,
            text,
            style: look.style,
            waypoints: Vec::new(),
            avoid: false,
            jumps: look.jumps,
            ext: look.ext,
        };
        c.sanitize();
        page.connectors.push(c);
    }
    let placed = d.nodes.iter().any(|n| n.left.is_some() || n.top.is_some());
    if !placed {
        let opts = options(&d, &mut warnings);
        match autolayout::layout_items(&mut page, None, &opts) {
            Ok(notes) => notes.into_iter().for_each(|n| warnings.push(n)),
            Err(e) => warnings.push(format!("the layout failed, rows are left in place: {e}")),
        }
    }
    Ok(Imported { pages: vec![page], warnings: warnings.into_vec() })
}

/// The shape for one row: style through the draw.io mapping, size from the row or the label.
fn node_shape(n: &CsvNode, padding: f64, warnings: &mut Warnings) -> Shape {
    let style = DStyle::parse(&n.style);
    let look = vertex_look(Some(&style), warnings);
    if let Some(why) = &look.skip {
        warnings.push(format!("row `{}`: {why}; drawn as a rectangle", short(&n.id)));
    }
    let text = if style.html().unwrap_or(false) { html_to_text(&n.label) } else { n.label.clone() };
    let lines = text.lines().count().max(1) as f64;
    let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let size = |s: Option<NodeSize>, estimate: f64| match s {
        Some(NodeSize::Fixed(v)) if v.is_finite() && v > 0.0 => v,
        _ => estimate,
    };
    let w = size(n.width, (longest * CHAR_W + 2.0 * padding).max(DEFAULT_W));
    let h = size(n.height, (lines * LINE_H + 2.0 * padding).max(DEFAULT_H));
    let mut shape = Shape::new(n.id.clone(), look.kind, n.left.unwrap_or(0.0), n.top.unwrap_or(0.0), w, h);
    shape.stencil = look.stencil;
    shape.style = look.style;
    shape.angle_deg = look.angle;
    shape.text = text;
    shape.ext = look.ext;
    shape.sanitize();
    shape
}

/// Layout options from the `layout`, `nodespacing` and `levelspacing` directives.
fn options(d: &CsvDiagram, warnings: &mut Warnings) -> Options {
    let (algorithm, direction) = match &d.layout {
        Some(CsvLayout::VerticalTree | CsvLayout::OrgChart) => (Algorithm::Tree, Direction::TopBottom),
        Some(CsvLayout::HorizontalTree) => (Algorithm::Tree, Direction::LeftRight),
        Some(CsvLayout::VerticalFlow) => (Algorithm::Layered, Direction::TopBottom),
        Some(CsvLayout::HorizontalFlow) => (Algorithm::Layered, Direction::LeftRight),
        Some(CsvLayout::Organic) => (Algorithm::Force, Direction::TopBottom),
        Some(CsvLayout::Circle) => (Algorithm::Radial, Direction::TopBottom),
        Some(CsvLayout::Auto | CsvLayout::None) | None => (Algorithm::Layered, Direction::TopBottom),
        Some(CsvLayout::Other(name)) => {
            warnings.push(format!("layout `{}` is not supported; a layered layout is used", short(name)));
            (Algorithm::Layered, Direction::TopBottom)
        }
    };
    let mut opts = Options { algorithm, direction, ..Options::default() };
    let spacing = |v: Option<f64>| v.filter(|v| v.is_finite() && *v >= 0.0).map(|v| v.min(10_000.0));
    if let Some(v) = spacing(d.spacing.node) {
        opts.node_spacing = v;
    }
    if let Some(v) = spacing(d.spacing.level) {
        opts.layer_spacing = v;
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::ShapeKind;

    const ORG: &str = "\
# label: %name%
# style: rounded=1;fillColor=#dae8fc;strokeColor=#6c8ebf;
# identity: id
# connect: {\"from\": \"manager\", \"to\": \"id\", \"invert\": true, \"style\": \"endArrow=open;\"}
# layout: verticaltree
# nodespacing: 30
# levelspacing: 50
id,name,manager
1,Ada,
2,Grace,1
3,Linus,1
4,Barbara,2
";

    #[test]
    fn rows_become_shapes_and_references_connectors() {
        let imp = import(ORG).unwrap();
        let page = imp.pages.first().unwrap();
        assert_eq!(page.shapes.len(), 4);
        assert_eq!(page.connectors.len(), 3);
        let ada = page.shapes.iter().find(|s| s.text == "Ada").unwrap();
        assert_eq!(ada.kind, ShapeKind::RoundedRectangle);
        assert_eq!(ada.style.fill.to_hex(), "#dae8fc");
        assert_eq!((ada.w, ada.h), (DEFAULT_W, DEFAULT_H));
        for c in &page.connectors {
            assert!(c.start.shape().is_some() && c.end.shape().is_some());
            assert_eq!(c.end_arrow, vizcraft_diagram::Arrow::Open);
        }
        // A top-bottom tree: the root is above its reports, and no two shapes overlap.
        let grace = page.shapes.iter().find(|s| s.text == "Grace").unwrap();
        assert!(ada.y + ada.h <= grace.y);
        for (i, a) in page.shapes.iter().enumerate() {
            for b in page.shapes.iter().skip(i + 1) {
                let (fa, fb) = (a.frame(), b.frame());
                assert!(fa.x1 <= fb.x0 || fb.x1 <= fa.x0 || fa.y1 <= fb.y0 || fb.y1 <= fa.y0, "{} overlaps {}", a.id, b.id);
            }
        }
    }

    #[test]
    fn left_and_top_columns_place_rows() {
        let text = "# label: %name%\n# left: x\n# top: y\nname,x,y\nA,10,20\nB,300,40\n";
        let imp = import(text).unwrap();
        let page = imp.pages.first().unwrap();
        let b = page.shapes.iter().find(|s| s.text == "B").unwrap();
        assert_eq!((b.x, b.y), (300.0, 40.0));
    }

    #[test]
    fn horizontal_flow_runs_left_to_right() {
        let text = ORG.replace("verticaltree", "horizontalflow");
        let imp = import(&text).unwrap();
        let page = imp.pages.first().unwrap();
        let ada = page.shapes.iter().find(|s| s.text == "Ada").unwrap();
        let grace = page.shapes.iter().find(|s| s.text == "Grace").unwrap();
        assert!(ada.x + ada.w <= grace.x);
    }

    #[test]
    fn long_labels_grow_the_shape_and_containers_warn() {
        let text = "# label: %name%\n# identity: id\n# parent: group\nid,name,group\ng,Team,\n1,A rather long label for one box,g\n";
        let imp = import(text).unwrap();
        let page = imp.pages.first().unwrap();
        let long = page.shapes.iter().find(|s| s.text.starts_with("A rather")).unwrap();
        assert!(long.w > DEFAULT_W);
        assert!(imp.warnings.iter().any(|w| w.contains("container")), "{:?}", imp.warnings);
    }

    #[test]
    fn hostile_text_is_an_error_or_a_page() {
        for text in ["", "#", "# label: %x%\n", "\u{0}\u{ffff}", "a,b\n\"unterminated", &"x,".repeat(5000), "# layout: [{\"layout\": \"x\"}]\na\n1\n"]
        {
            if let Ok(imp) = import(text) {
                assert_eq!(imp.pages.len(), 1);
            }
        }
        assert!(import("").is_err());
    }
}
