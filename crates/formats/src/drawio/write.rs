//! VizCraft pages to draw.io cells.

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Route, Shape, ShapeKind, Style};
use vizcraft_drawio::{
    Attributes, Cell, Geometry, GraphModel, Style as DStyle, StyleItem, Wrapper, WrapperKind, format_f64, html_to_text, text_to_html,
};

use super::read::{EXT_ATTR, EXT_FONT_STYLE, EXT_MODEL, EXT_STYLE, EXT_WRAPPER, master_for_kind};

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

/// The draw.io arrow name for `a`, and whether it is drawn hollow (written as `startFill` or
/// `endFill` set to `0`). The ER names are draw.io's entity-relation line ends.
fn arrow(a: Arrow) -> (&'static str, bool) {
    match a {
        Arrow::None => ("none", false),
        Arrow::Triangle => ("block", false),
        Arrow::Open => ("open", false),
        Arrow::Diamond => ("diamond", false),
        Arrow::Circle => ("oval", false),
        Arrow::HollowTriangle => ("block", true),
        Arrow::HollowDiamond => ("diamond", true),
        Arrow::OpenCircle => ("oval", true),
        Arrow::Bar => ("ERone", false),
        Arrow::CrowsFoot => ("ERmany", false),
        Arrow::CrowsFootBar => ("ERoneToMany", false),
        Arrow::CrowsFootCircle => ("ERzeroToMany", false),
        Arrow::BarCircle => ("ERzeroToOne", false),
    }
}

/// The normalized position of connection point `point` of `shape` on `page`.
fn glue_point(page: &Page, end: &End) -> Option<vizcraft_diagram::Point> {
    match end {
        End::Glued { shape, point: Some(i) } => page.shape(shape)?.connection_points().get(*i).copied(),
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
    for (key, fill_key, a) in [("startArrow", "startFill", c.start_arrow), ("endArrow", "endFill", c.end_arrow)] {
        let (name, hollow) = arrow(a);
        pair(&mut s, key, name);
        if hollow {
            pair(&mut s, fill_key, "0");
        }
    }
    if !c.kind.is_empty() {
        pair(&mut s, super::read::KIND_KEY, c.kind.clone());
    }
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
        let mut cell = labelled(cell_id(&shape.id), value, &shape.ext);
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
