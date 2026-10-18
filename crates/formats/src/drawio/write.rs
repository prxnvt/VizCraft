//! VizCraft pages to draw.io cells.

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Route, Shape, ShapeKind, Style};
use vizcraft_drawio::{
    Attributes, Cell, Geometry, GraphModel, Style as DStyle, StyleItem, Wrapper, WrapperKind, format_f64, html_to_text, text_to_html,
};

use super::read::{COMPARTMENTS_ATTR, EXT_ATTR, EXT_FONT_STYLE, EXT_MODEL, EXT_STYLE, EXT_WRAPPER, master_for_kind};

/// The id of the root cell and of the one layer every page is written with.
const ROOT: &str = "0";
const LAYER: &str = "1";

/// A cell id for an item id: the two structural ids are taken, so items that came with those
/// ids (from a file whose layer had another id) get a prefix.
fn cell_id(id: &str) -> String {
    if id == ROOT || id == LAYER { format!("vizcraft-{id}") } else { id.to_string() }
}

fn hex(c: Color) -> String {
    if c.a == 0 { "none".into() } else { format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b) }
}

fn pair(s: &mut DStyle, k: &str, v: impl Into<String>) {
    s.items.push(StyleItem::Pair(k.into(), v.into()));
}

fn name(s: &mut DStyle, n: &str) {
    s.items.push(StyleItem::Name(n.into()));
}

/// The label value and whether it is written as HTML. HTML is used whenever reading it back
/// gives the same text; text that HTML would change (runs of spaces, tabs) is written plain,
/// where newlines survive as character references.
fn label(text: &str) -> (String, bool) {
    let html = text_to_html(text);
    if html_to_text(&html) == text { (html, true) } else { (text.to_string(), false) }
}

/// The keys shared by vertices and edges, then the leftover style from the source file.
fn finish(s: &mut DStyle, style: &Style, html: bool, ext: &std::collections::BTreeMap<String, String>) {
    pair(s, "strokeColor", hex(style.line));
    pair(s, "strokeWidth", format_f64(style.line_width));
    pair(s, "fontSize", format_f64(style.font_size));
    pair(s, "fontColor", hex(style.font_color));
    let extra = ext.get(EXT_FONT_STYLE).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0) & !3 & 255;
    let bits = u32::from(style.bold) | (u32::from(style.italic) << 1) | extra;
    if bits != 0 {
        pair(s, "fontStyle", bits.to_string());
    }
    if html {
        pair(s, "html", "1");
    }
    if let Some(rest) = ext.get(EXT_STYLE) {
        let rest = DStyle::parse(rest);
        // Our own keys are written above; a duplicate in the leftover would override them.
        for item in rest.items {
            let ours = match &item {
                StyleItem::Pair(k, _) => s.items.iter().any(|i| matches!(i, StyleItem::Pair(o, _) if o == k)),
                StyleItem::Name(n) => s.has_name(n),
            };
            if !ours {
                s.items.push(item);
            }
        }
    }
    s.trailing_semicolon = true;
}

/// The draw.io style of a shape: the keys draw.io needs to draw the kind, `vizcraft=<master>`,
/// the colours and font, then the style keys kept from the source file.
pub(crate) fn shape_style(shape: &Shape) -> DStyle {
    shape_style_with(shape, true)
}

fn shape_style_with(shape: &Shape, html: bool) -> DStyle {
    let mut s = DStyle::default();
    let rest = shape.ext.get(EXT_STYLE).map(|r| DStyle::parse(r)).unwrap_or_default();
    match shape.kind {
        ShapeKind::Rectangle => pair(&mut s, "rounded", "0"),
        ShapeKind::RoundedRectangle => pair(&mut s, "rounded", "1"),
        ShapeKind::Ellipse => name(&mut s, "ellipse"),
        ShapeKind::Diamond => name(&mut s, "rhombus"),
        ShapeKind::Triangle => {
            name(&mut s, "triangle");
            // draw.io's triangle points right unless told otherwise; ours points up.
            if rest.get("direction").is_none() {
                pair(&mut s, "direction", "north");
            }
        }
        ShapeKind::Parallelogram => pair(&mut s, "shape", "parallelogram"),
        ShapeKind::Hexagon | ShapeKind::Preparation => pair(&mut s, "shape", "hexagon"),
        ShapeKind::Cylinder => pair(&mut s, "shape", "cylinder3"),
        ShapeKind::Document => pair(&mut s, "shape", "document"),
        ShapeKind::Cloud => {
            name(&mut s, "ellipse");
            pair(&mut s, "shape", "cloud");
        }
        ShapeKind::Terminator => {
            pair(&mut s, "rounded", "1");
            pair(&mut s, "arcSize", "50");
        }
        ShapeKind::PredefinedProcess => pair(&mut s, "shape", "process"),
        ShapeKind::ManualInput => pair(&mut s, "shape", "manualInput"),
        ShapeKind::Trapezoid => pair(&mut s, "shape", "trapezoid"),
        ShapeKind::OffPageReference => pair(&mut s, "shape", "offPageConnector"),
    }
    pair(&mut s, "whiteSpace", "wrap");
    // The stencil is written only when it agrees with the kind, so reading it back (where it
    // wins) cannot change the outline.
    let master = vizcraft_stencils::master(&shape.stencil).filter(|m| m.kind == shape.kind).map_or_else(|| master_for_kind(shape.kind), |m| m.id);
    pair(&mut s, "vizcraft", master);
    pair(&mut s, "fillColor", hex(shape.style.fill));
    if shape.angle_deg != 0.0 {
        pair(&mut s, "rotation", format_f64(shape.angle_deg));
    }
    finish(&mut s, &shape.style, html, &shape.ext);
    s
}

fn arrow(a: Arrow) -> &'static str {
    match a {
        Arrow::None => "none",
        Arrow::Triangle => "block",
        Arrow::Open => "open",
        Arrow::Diamond => "diamond",
        Arrow::Circle => "oval",
    }
}

/// The normalized position of connection point `point` of `shape` on `page`.
fn glue_point(page: &Page, end: &End) -> Option<vizcraft_diagram::Point> {
    match end {
        End::Glued { shape, .. } => {
            let s = page.shape(shape)?;
            s.connection_points().get(end.resolve_port(s)?).copied()
        }
        _ => None,
    }
}

fn edge_style(page: &Page, c: &Connector, html: bool) -> DStyle {
    let mut s = DStyle::default();
    match c.route {
        Route::Straight => {}
        Route::Orthogonal => pair(&mut s, "edgeStyle", "orthogonalEdgeStyle"),
        Route::Curved => {
            // draw.io curves only along a route with bends, which the orthogonal style supplies.
            pair(&mut s, "edgeStyle", "orthogonalEdgeStyle");
            pair(&mut s, "curved", "1");
        }
    }
    pair(&mut s, "startArrow", arrow(c.start_arrow));
    pair(&mut s, "endArrow", arrow(c.end_arrow));
    if c.jumps {
        pair(&mut s, "jumpStyle", "arc");
    }
    if let Some(p) = glue_point(page, &c.start) {
        pair(&mut s, "exitX", format_f64(p.x));
        pair(&mut s, "exitY", format_f64(p.y));
    }
    if let Some(p) = glue_point(page, &c.end) {
        pair(&mut s, "entryX", format_f64(p.x));
        pair(&mut s, "entryY", format_f64(p.y));
    }
    finish(&mut s, &c.style, html, &c.ext);
    s
}

/// The wrapper form a cell was read in, from `ext`, with the label and custom attributes.
fn wrapper(ext: &std::collections::BTreeMap<String, String>, value: &str) -> Option<Wrapper> {
    let kind = match ext.get(EXT_WRAPPER).map(String::as_str) {
        Some("UserObject") => WrapperKind::UserObject,
        Some("object") => WrapperKind::Object,
        _ => return None,
    };
    let mut attributes = Attributes::new();
    attributes.push("label", value);
    for (k, v) in ext.range(EXT_ATTR.to_string()..) {
        let Some(attr) = k.strip_prefix(EXT_ATTR) else { break };
        // `id` and `label` belong to the cell; a file could still have smuggled them in.
        if !attr.is_empty()
            && attr != "id"
            && attr != "label"
            && attr.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
        {
            attributes.push(attr, v.as_str());
        }
    }
    Some(Wrapper { kind, attributes })
}

/// A cell with its label in the plain or wrapper form.
fn labelled(id: String, value: String, ext: &std::collections::BTreeMap<String, String>) -> Cell {
    match wrapper(ext, &value) {
        Some(w) => Cell { id, wrapper: Some(w), ..Cell::default() },
        None => Cell { id, value: Some(value), ..Cell::default() },
    }
}

/// The shape's `ext` with its compartments added as the JSON wrapper attribute
/// [`COMPARTMENTS_ATTR`] (an `object` wrapper when the cell had none), so draw.io keeps them and
/// the import reads them back.
fn compartments_ext(shape: &Shape) -> std::borrow::Cow<'_, std::collections::BTreeMap<String, String>> {
    if shape.compartments.is_empty() {
        return std::borrow::Cow::Borrowed(&shape.ext);
    }
    let mut ext = shape.ext.clone();
    if let Ok(json) = serde_json::to_string(&shape.compartments) {
        ext.entry(EXT_WRAPPER.to_string()).or_insert_with(|| "object".to_string());
        ext.insert(format!("{EXT_ATTR}{COMPARTMENTS_ATTR}"), json);
    }
    std::borrow::Cow::Owned(ext)
}

fn geometry() -> Geometry {
    Geometry { as_: Some("geometry".into()), ..Geometry::default() }
}

fn dpoint(p: vizcraft_diagram::Point) -> vizcraft_drawio::Point {
    vizcraft_drawio::Point { x: p.x, y: p.y }
}

/// One page as a draw.io model: the root cell `0`, the layer `1`, the shapes, then the
/// connectors.
pub(crate) fn model_from_page(page: &Page) -> GraphModel {
    let mut model = GraphModel::with_default_layer();
    for (k, v) in page.ext.range(EXT_MODEL.to_string()..) {
        let Some(attr) = k.strip_prefix(EXT_MODEL) else { break };
        if !attr.is_empty() && attr.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')) {
            model.attributes.push(attr, v.as_str());
        }
    }
    model.attributes.set("pageWidth", format_f64(page.width));
    model.attributes.set("pageHeight", format_f64(page.height));
    for shape in &page.shapes {
        let (value, html) = label(&shape.text);
        let mut cell = labelled(cell_id(&shape.id), value, &compartments_ext(shape));
        cell.parent = Some(LAYER.into());
        cell.vertex = true;
        cell.style = Some(shape_style_with(shape, html));
        cell.geometry = Some(Geometry { x: shape.x, y: shape.y, width: shape.w, height: shape.h, ..geometry() });
        model.cells.push(cell);
    }
    for c in &page.connectors {
        let (value, html) = label(&c.text);
        let mut cell = labelled(cell_id(&c.id), value, &c.ext);
        cell.parent = Some(LAYER.into());
        cell.edge = true;
        cell.style = Some(edge_style(page, c, html));
        let ends = vizcraft_diagram::resolve_ends(page, c);
        let mut g = Geometry { relative: true, ..geometry() };
        // A glued end names its cell; any other end (free, or glued to a shape that is gone) is
        // written as a loose point where it is drawn.
        match c.start.shape().filter(|id| page.shape(id).is_some()) {
            Some(id) => cell.source = Some(cell_id(id)),
            None => g.source_point = Some(dpoint(ends.start.pos)),
        }
        match c.end.shape().filter(|id| page.shape(id).is_some()) {
            Some(id) => cell.target = Some(cell_id(id)),
            None => g.target_point = Some(dpoint(ends.end.pos)),
        }
        g.points = c.waypoints.iter().copied().map(dpoint).collect();
        cell.geometry = Some(g);
        model.cells.push(cell);
    }
    model
}

#[cfg(test)]
mod compartment_tests {
    use vizcraft_diagram::compartment::{Compartment, CompartmentKind};
    use vizcraft_diagram::{Connector, Document, End, Shape, ShapeKind};

    use crate::drawio::{load, save};

    fn doc() -> Document {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        let mut class = Shape::new("k", ShapeKind::Rectangle, 10.0, 20.0, 160.0, 100.0);
        class.text = "Order".into();
        let mut attrs = Compartment::new("attrs", CompartmentKind::List);
        attrs.rows = vec!["+ id: int".into(), "+ note: \"a <b> & c\"".into()];
        let mut ops = Compartment::new("ops", CompartmentKind::List);
        ops.rows = vec!["+ pay()".into()];
        class.compartments = vec![attrs, ops];
        let mut tag = Shape::new("t", ShapeKind::Rectangle, 300.0, 20.0, 120.0, 140.0);
        tag.text = "P-101".into();
        tag.ext.insert("drawio.wrapper".into(), "UserObject".into());
        tag.ext.insert("drawio.attr.owner".into(), "plant".into());
        let mut desc = Compartment::new("desc", CompartmentKind::Paragraph);
        desc.text = "Feed pump\nduty".into();
        let mut pins = Compartment::new("pins", CompartmentKind::Slot);
        pins.members = vec!["pin".into()];
        tag.compartments = vec![desc, pins];
        let mut pin = Shape::new("pin", ShapeKind::Ellipse, 310.0, 120.0, 10.0, 10.0);
        pin.parent = Some("t".into());
        page.shapes.extend([class, tag, pin]);
        page.connectors.push(Connector::new("c", End::port("k", "attrs.right"), End::dynamic("t")));
        doc.sanitize();
        doc
    }

    #[test]
    fn compartments_round_trip_through_drawio_ext() {
        let doc = doc();
        let bytes = save(&doc).unwrap();
        let xml = String::from_utf8_lossy(&bytes);
        assert!(xml.contains("vizcraftCompartments="), "{xml}");
        let back = load(&bytes).unwrap();
        let (a, b) = (doc.page().unwrap(), back.page().unwrap());
        for id in ["k", "t", "pin"] {
            let (x, y) = (a.shape(id).unwrap(), b.shape(id).unwrap());
            assert_eq!(x.compartments, y.compartments, "{id}");
            assert_eq!(x.parent, y.parent, "{id}: the slot member keeps its parent");
        }
        // The carrier wrapper of the class goes; the tag keeps its own wrapper and attribute.
        assert!(b.shape("k").unwrap().ext.is_empty(), "{:?}", b.shape("k").unwrap().ext);
        assert_eq!(b.shape("t").unwrap().ext, a.shape("t").unwrap().ext);
        // The port glue comes back as the port it was written from.
        assert_eq!(b.connector("c").unwrap().start, End::port("k", "attrs.right"));
        // And the second trip changes nothing.
        let again = load(&save(&back).unwrap()).unwrap();
        assert_eq!(again.pages, back.pages);
    }

    #[test]
    fn junk_compartments_in_a_drawio_file_load_as_far_as_they_parse() {
        let xml = r#"<mxfile><diagram id="p" name="P"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><object label="x" vizcraftCompartments="[{&quot;id&quot;: &quot;a&quot;, &quot;kind&quot;: &quot;list&quot;, &quot;rows&quot;: [&quot;r&quot;, 5]}, 7, &quot;junk&quot;]" id="s"><mxCell vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="60" as="geometry"/></mxCell></object><object label="y" vizcraftCompartments="not json" id="u"><mxCell vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="60" as="geometry"/></mxCell></object></root></mxGraphModel></diagram></mxfile>"#;
        let doc = load(xml.as_bytes()).unwrap();
        let p = doc.page().unwrap();
        let s = p.shape("s").unwrap();
        assert_eq!(s.compartments.len(), 1);
        assert_eq!(s.compartments[0].rows, vec!["r".to_string()]);
        assert!(p.shape("u").unwrap().compartments.is_empty());
    }
}
