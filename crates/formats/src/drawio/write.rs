//! VizCraft pages to draw.io cells.

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Route, Shape, ShapeKind, Style};
use vizcraft_drawio::{
    Attributes, Cell, Geometry, GraphModel, Style as DStyle, StyleItem, Wrapper, WrapperKind, format_f64, html_to_text, text_to_html,
};

use super::read::{
    COMPARTMENTS_ATTR, END_LABEL_ATTR, EXT_ATTR, EXT_FONT_STYLE, EXT_MODEL, EXT_STYLE, EXT_WRAPPER, JUNCTION_KEY, MARKER_KEY, NET_KEY, NETS_ATTR,
    ORIENT_KEY, START_LABEL_ATTR, master_for_kind, orient_text,
};

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
        ShapeKind::Rectangle | ShapeKind::Freeform => pair(&mut s, "rounded", "0"),
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
        // A plain box carrying the `vizcraft` master key; the interaction itself is not written.
        ShapeKind::Lifeline | ShapeKind::ActorLifeline => {}
    }
    pair(&mut s, "whiteSpace", "wrap");
    // The stencil is written only when it agrees with the kind, so reading it back (where it
    // wins) cannot change the outline.
    let master = vizcraft_stencils::master(&shape.stencil).filter(|m| m.kind == shape.kind).map_or_else(|| master_for_kind(shape.kind), |m| m.id);
    pair(&mut s, "vizcraft", master);
    pair(&mut s, "fillColor", hex(shape.style.fill));
    if shape.orient.is_default() {
        if shape.angle_deg != 0.0 {
            pair(&mut s, "rotation", format_f64(shape.angle_deg));
        }
    } else {
        // draw.io mirrors in the frame (`flipH`) and then rotates, the order VizCraft applies the
        // orientation in, so the quarter turns ride in the rotation; `vizcraftOrient` says how
        // much of it they are.
        let rotation = (shape.angle_deg + 90.0 * f64::from(shape.orient.turns)).rem_euclid(360.0);
        if rotation != 0.0 {
            pair(&mut s, "rotation", format_f64(rotation));
        }
        if shape.orient.mirror {
            pair(&mut s, "flipH", "1");
        }
        pair(&mut s, ORIENT_KEY, orient_text(shape.orient));
    }
    if vizcraft_diagram::net::is_junction(shape) {
        pair(&mut s, JUNCTION_KEY, "1");
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
        Arrow::Slash => ("dash", false),
        Arrow::DoubleBar => ("ERmandOne", false),
        // draw.io has no small dot end; its filled oval is the nearest.
        Arrow::Dot => ("oval", false),
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
    if let Some(n) = &c.net {
        pair(&mut s, NET_KEY, n.clone());
    }
    if c.marker != vizcraft_diagram::LineMarker::None {
        pair(&mut s, MARKER_KEY, c.marker.name());
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
    let path = vizcraft_diagram::shapeop::path_json(shape);
    if shape.compartments.is_empty() && path.is_none() {
        return std::borrow::Cow::Borrowed(&shape.ext);
    }
    let mut ext = shape.ext.clone();
    if !shape.compartments.is_empty()
        && let Ok(json) = serde_json::to_string(&shape.compartments)
    {
        ext.entry(EXT_WRAPPER.to_string()).or_insert_with(|| "object".to_string());
        ext.insert(format!("{EXT_ATTR}{COMPARTMENTS_ATTR}"), json);
    }
    // A freeform outline travels the same way, in `vizcraftPath`; draw.io draws the frame.
    if let Some(json) = path {
        ext.entry(EXT_WRAPPER.to_string()).or_insert_with(|| "object".to_string());
        ext.insert(format!("{EXT_ATTR}{}", vizcraft_diagram::shapeop::PATH_ATTR), json);
    }
    std::borrow::Cow::Owned(ext)
}

/// The connector's `ext` with its end labels added as the wrapper attributes
/// [`START_LABEL_ATTR`] and [`END_LABEL_ATTR`], the way [`compartments_ext`] carries
/// compartments.
fn end_labels_ext(c: &Connector) -> std::borrow::Cow<'_, std::collections::BTreeMap<String, String>> {
    if c.start_label.is_empty() && c.end_label.is_empty() {
        return std::borrow::Cow::Borrowed(&c.ext);
    }
    let mut ext = c.ext.clone();
    ext.entry(EXT_WRAPPER.to_string()).or_insert_with(|| "object".to_string());
    for (attr, v) in [(START_LABEL_ATTR, &c.start_label), (END_LABEL_ATTR, &c.end_label)] {
        if !v.is_empty() {
            ext.insert(format!("{EXT_ATTR}{attr}"), v.clone());
        }
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
    model_from_page_with_nets(page, &[])
}

/// [`model_from_page`] with the nets its segments name written as JSON in the page model's
/// `vizcraftNets` attribute, so a reread restores them (other draw.io readers ignore it).
pub(crate) fn model_from_page_with_nets(page: &Page, nets: &[vizcraft_diagram::vizcraft_model::Net]) -> GraphModel {
    let mut model = GraphModel::with_default_layer();
    for (k, v) in page.ext.range(EXT_MODEL.to_string()..) {
        let Some(attr) = k.strip_prefix(EXT_MODEL) else { break };
        // A stale nets attribute kept from an import is replaced by the nets drawn now.
        if attr != NETS_ATTR && !attr.is_empty() && attr.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')) {
            model.attributes.push(attr, v.as_str());
        }
    }
    let used: std::collections::HashSet<&str> = page.connectors.iter().filter_map(|c| c.net.as_deref()).collect();
    let drawn: Vec<&vizcraft_diagram::vizcraft_model::Net> = nets.iter().filter(|n| used.contains(n.id.as_str())).collect();
    if !drawn.is_empty()
        && let Ok(json) = serde_json::to_string(&drawn)
    {
        model.attributes.set(NETS_ATTR, json);
    }
    let frame = page.frame();
    model.attributes.set("pageWidth", format_f64(frame.width()));
    model.attributes.set("pageHeight", format_f64(frame.height()));
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
        let mut cell = labelled(cell_id(&c.id), value, &end_labels_ext(c));
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
        // draw.io's points are the interior bends only. A route the router stored holds its whole
        // polyline, ends included; those two points sit on the ends, which the cell's source and target
        // already give, so they are left out and a reread draws the same line.
        let mut bends: &[vizcraft_diagram::Point] = &c.waypoints;
        // A curve is fitted through every stored point, ends included, so it keeps them.
        if c.route != vizcraft_diagram::Route::Curved && vizcraft_diagram::route::stored_route(page, c) && bends.len() >= 2 {
            bends = bends.get(1..bends.len() - 1).unwrap_or_default();
        }
        g.points = bends.iter().copied().map(dpoint).collect();
        cell.geometry = Some(g);
        model.cells.push(cell);
    }
    model
}

#[cfg(test)]
mod route_tests {
    use vizcraft_diagram::{Connector, Document, End, Point, Shape, ShapeKind};

    use crate::drawio::{load, save};

    #[test]
    fn a_routed_connector_writes_only_interior_bends_and_rereads_the_same() {
        let mut doc = Document::new();
        let page = &mut doc.pages[0];
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 80.0, 40.0));
        page.shapes.push(Shape::new("wall", ShapeKind::Rectangle, 150.0, -100.0, 40.0, 260.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 100.0, 80.0, 40.0));
        page.connectors.push(Connector::new("c", End::dynamic("a"), End::dynamic("b")));
        vizcraft_diagram::autoroute::reroute(page, None);
        let c = page.connector("c").unwrap();
        assert!(vizcraft_diagram::route::stored_route(page, c), "the router stored the whole polyline");
        let whole = c.waypoints.clone();
        assert!(whole.len() >= 4, "{whole:?}");
        let drawn = vizcraft_diagram::route_polyline(page, c);
        let back = load(&save(&doc).unwrap()).unwrap();
        let bp = &back.pages[0];
        let rc = bp.connector("c").expect("the connector survives");
        let interior: Vec<Point> = whole[1..whole.len() - 1].to_vec();
        assert_eq!(rc.waypoints.len(), interior.len(), "only interior bends are written: {:?}", rc.waypoints);
        for (got, want) in rc.waypoints.iter().zip(&interior) {
            assert!(got.distance(*want) < 1.0, "{got:?} against {want:?}");
        }
        let again = vizcraft_diagram::route_polyline(bp, rc);
        assert_eq!(again.len(), drawn.len(), "{again:?} against {drawn:?}");
    }
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

#[cfg(test)]
mod net_tests {
    use vizcraft_diagram::net::{is_junction, split_at};
    use vizcraft_diagram::vizcraft_model::{Net, NetKind};
    use vizcraft_diagram::{Arrow, Connector, Document, End, LineMarker, Orient, Point, Route, Shape, ShapeKind};

    use crate::drawio::{load, save};

    /// A wire with a junction on it, a marked branch, and a turned, mirrored glyph.
    fn wired() -> Document {
        let mut doc = Document::new();
        doc.model.nets.push(Net { name: "L1".into(), ..Net::new("n1", NetKind::Potential) });
        let page = &mut doc.pages[0];
        let mut glyph = Shape::new("r1", ShapeKind::Rectangle, 300.0, 50.0, 40.0, 20.0);
        glyph.connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
        glyph.orient = Orient { turns: 1, mirror: true };
        glyph.angle_deg = 10.0;
        page.shapes.push(glyph);
        let w = Connector {
            route: Route::Orthogonal,
            end_arrow: Arrow::None,
            net: Some("n1".into()),
            ..Connector::new("w1", End::free(Point::new(0.0, 100.0)), End::free(Point::new(200.0, 100.0)))
        };
        page.connectors.push(w);
        split_at(page, "w1", Point::new(100.0, 100.0), "w2", "j1", true).unwrap();
        page.connectors.push(Connector {
            marker: LineMarker::DoubleSlash,
            ..Connector {
                end_arrow: Arrow::None,
                net: Some("n1".into()),
                ..Connector::new("w3", End::point("j1", 0), End::free(Point::new(100.0, 200.0)))
            }
        });
        doc
    }

    #[test]
    fn a_wire_a_junction_and_an_oriented_glyph_survive_a_round_trip() {
        let doc = wired();
        let bytes = save(&doc).unwrap();
        let xml = String::from_utf8(bytes.clone()).unwrap();
        assert!(xml.contains("vizcraftOrient=1m") && xml.contains("flipH=1") && xml.contains("rotation=100"), "{xml}");
        assert!(xml.contains("vizcraftNet=n1") && xml.contains("vizcraftMarker=doubleSlash") && xml.contains("vizcraftJunction=1"), "{xml}");
        assert!(xml.contains("vizcraftNets="), "{xml}");
        let back = load(&bytes).unwrap();
        let p = &back.pages[0];
        let g = p.shape("r1").unwrap();
        assert_eq!((g.orient, g.angle_deg), (Orient { turns: 1, mirror: true }, 10.0));
        let j = p.shape("j1").unwrap();
        assert!(is_junction(j) && j.connection_points.len() == 1);
        assert_eq!(back.model.nets, doc.model.nets, "the net with its name comes back");
        for id in ["w1", "w2", "w3"] {
            assert_eq!(p.connector(id).unwrap().net.as_deref(), Some("n1"), "{id}");
        }
        assert_eq!(p.connector("w3").unwrap().marker, LineMarker::DoubleSlash);
        assert_eq!(p.connector("w3").unwrap().start, End::point("j1", 0));
        assert!(p.ext.keys().all(|k| !k.contains("vizcraftNets")), "the carrier attribute is used up");
        // Writing the reread document again gives the same file.
        assert_eq!(save(&back).unwrap(), bytes);
    }

    #[test]
    fn foreign_flips_read_as_orientations() {
        let xml = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="a" vertex="1" parent="1" style="flipH=1;rotation=30"><mxGeometry x="0" y="0" width="40" height="20" as="geometry"/></mxCell>
            <mxCell id="b" vertex="1" parent="1" style="flipV=1"><mxGeometry x="0" y="0" width="40" height="20" as="geometry"/></mxCell>
            <mxCell id="c" vertex="1" parent="1" style="flipV=1;flipH=1"><mxGeometry x="0" y="0" width="40" height="20" as="geometry"/></mxCell>
            <mxCell id="d" vertex="1" parent="1" style="vizcraftOrient=junk;flipH=0"><mxGeometry x="0" y="0" width="40" height="20" as="geometry"/></mxCell>
            <mxCell id="e" edge="1" parent="1" style="vizcraftNet=ghost;vizcraftMarker=zigzag"><mxGeometry relative="1" as="geometry"/></mxCell>
            </root></mxGraphModel>"#;
        let doc = load(xml.as_bytes()).unwrap();
        let p = &doc.pages[0];
        let o = |id: &str| (p.shape(id).unwrap().orient, p.shape(id).unwrap().angle_deg);
        assert_eq!(o("a"), (Orient { turns: 0, mirror: true }, 30.0));
        assert_eq!(o("b"), (Orient { turns: 2, mirror: true }, 0.0), "a vertical flip is a mirror and two turns");
        assert_eq!(o("c"), (Orient { turns: 2, mirror: false }, 0.0), "both flips are a half turn");
        assert_eq!(o("d"), (Orient::NONE, 0.0));
        let e = p.connector("e").unwrap();
        assert_eq!((e.net.as_deref(), e.marker), (None, LineMarker::None), "a net that did not come back is cleared");
    }
}
