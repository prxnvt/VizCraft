//! The graph model of one page: `<mxGraphModel>` with its attributes and its cells in document
//! order. The IR stays close to the XML so that reading, writing and reading again gives an equal
//! value; interpretation (what a style means, which cells are layers) is left to the caller.

use serde::{Deserialize, Serialize};

use crate::attrs::{Attributes, format_f64, parse_bool, parse_f64};
use crate::style::Style;
use crate::xml::{self, Element, write_attr, write_attrs};
use crate::{DrawioError, Result, limits};

/// One page's model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphModel {
    /// Every attribute of `<mxGraphModel>` in document order. The typed accessors below read the
    /// well-known ones; unknown attributes stay here untouched.
    pub attributes: Attributes,
    /// Cells in document order, including the two structural cells (`0`, the root, and `1`, the
    /// default layer) when the file has them.
    pub cells: Vec<Cell>,
}

/// The wrapper element a cell may be stored in to carry custom attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WrapperKind {
    /// `<UserObject>`.
    UserObject,
    /// `<object>`.
    Object,
}

impl WrapperKind {
    fn tag(self) -> &'static str {
        match self {
            WrapperKind::UserObject => "UserObject",
            WrapperKind::Object => "object",
        }
    }
}

/// The `<UserObject>`/`<object>` form of a cell. Its attributes hold the label (`label`) and any
/// custom data (`tooltip`, `link`, `placeholders`, user properties) in document order; the cell
/// id lives in [`Cell::id`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wrapper {
    /// Which element was used.
    pub kind: WrapperKind,
    /// The wrapper's attributes other than `id`.
    pub attributes: Attributes,
}

/// A point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
}

/// A rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// `<mxGeometry>`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// Left, or for a relative geometry (edge labels) the position along the edge.
    pub x: f64,
    /// Top, or the offset from the edge for a relative geometry.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// `relative="1"`: coordinates are relative to the parent.
    pub relative: bool,
    /// The `as` attribute, normally `geometry`.
    pub as_: Option<String>,
    /// `<mxPoint as="sourcePoint">`: a loose edge end.
    pub source_point: Option<Point>,
    /// `<mxPoint as="targetPoint">`.
    pub target_point: Option<Point>,
    /// `<Array as="points">`: edge waypoints in order.
    pub points: Vec<Point>,
    /// `<mxPoint as="offset">`: label offset.
    pub offset: Option<Point>,
    /// `<mxRectangle as="alternateBounds">`: the bounds of a collapsed container.
    pub alternate_bounds: Option<Rect>,
    /// Other attributes of `<mxGeometry>`, in order.
    pub extra: Attributes,
}

/// One cell: a vertex, an edge, a layer or the root.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// The id, from the `mxCell` or from the wrapper.
    pub id: String,
    /// The parent cell's id; `None` for the root.
    pub parent: Option<String>,
    /// The `value` attribute (the label in the plain form). `None` when absent; in the wrapper
    /// form the label is `wrapper.attributes["label"]` instead.
    pub value: Option<String>,
    /// The parsed style, `None` when the attribute is absent.
    pub style: Option<Style>,
    /// `vertex="1"`.
    pub vertex: bool,
    /// `edge="1"`.
    pub edge: bool,
    /// Edge source cell id.
    pub source: Option<String>,
    /// Edge target cell id.
    pub target: Option<String>,
    /// `connectable`, when given.
    pub connectable: Option<bool>,
    /// `visible`, when given (layers use `visible="0"`).
    pub visible: Option<bool>,
    /// `collapsed="1"`.
    pub collapsed: bool,
    /// The geometry, when the cell has one.
    pub geometry: Option<Geometry>,
    /// Other `mxCell` attributes, in order.
    pub extra: Attributes,
    /// The `<UserObject>`/`<object>` wrapper, when the cell uses that form.
    pub wrapper: Option<Wrapper>,
}

impl Cell {
    /// The label: the wrapper's `label` attribute in the wrapper form, otherwise `value`.
    pub fn label(&self) -> Option<&str> {
        match &self.wrapper {
            Some(w) => w.attributes.get("label").or(self.value.as_deref()),
            None => self.value.as_deref(),
        }
    }

    /// Whether the label is HTML (`html=1` in the style).
    pub fn is_html(&self) -> bool {
        self.style.as_ref().and_then(Style::html).unwrap_or(false)
    }

    /// The label as plain text, converting HTML labels with [`crate::html_to_text`].
    pub fn plain_label(&self) -> String {
        let raw = self.label().unwrap_or("");
        if self.is_html() { crate::label::html_to_text(raw) } else { raw.to_string() }
    }

    /// A custom attribute of the wrapper form (`tooltip`, `link`, user properties).
    pub fn custom(&self, key: &str) -> Option<&str> {
        self.wrapper.as_ref().and_then(|w| w.attributes.get(key))
    }
}

macro_rules! num_attr {
    ($(#[$doc:meta] $fn:ident => $key:literal),* $(,)?) => {
        $(#[$doc] pub fn $fn(&self) -> Option<f64> { self.attributes.get_f64($key) })*
    };
}

macro_rules! bool_attr {
    ($(#[$doc:meta] $fn:ident => $key:literal),* $(,)?) => {
        $(#[$doc] pub fn $fn(&self) -> Option<bool> { self.attributes.get_bool($key) })*
    };
}

impl GraphModel {
    num_attr! {
        /// `dx`: horizontal scroll translation.
        dx => "dx",
        /// `dy`: vertical scroll translation.
        dy => "dy",
        /// `gridSize`.
        grid_size => "gridSize",
        /// `pageScale`.
        page_scale => "pageScale",
        /// `pageWidth`.
        page_width => "pageWidth",
        /// `pageHeight`.
        page_height => "pageHeight",
    }

    bool_attr! {
        /// `grid`: grid shown.
        grid => "grid",
        /// `guides`.
        guides => "guides",
        /// `tooltips`.
        tooltips => "tooltips",
        /// `connect`.
        connect => "connect",
        /// `arrows`.
        arrows => "arrows",
        /// `fold`.
        fold => "fold",
        /// `page`: page view on.
        page => "page",
        /// `math`.
        math => "math",
        /// `shadow`.
        shadow => "shadow",
    }

    /// `background` colour, raw.
    pub fn background(&self) -> Option<&str> {
        self.attributes.get("background")
    }

    /// `adaptiveColors`, raw.
    pub fn adaptive_colors(&self) -> Option<&str> {
        self.attributes.get("adaptiveColors")
    }

    /// The cell with this id.
    pub fn cell(&self, id: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.id == id)
    }

    /// A model with the two structural cells draw.io expects: the root `0` and the default layer
    /// `1`.
    pub fn with_default_layer() -> Self {
        let root = Cell { id: "0".into(), ..Cell::default() };
        let layer = Cell { id: "1".into(), parent: Some("0".into()), ..Cell::default() };
        GraphModel { attributes: Attributes::new(), cells: vec![root, layer] }
    }
}

/// Parses model XML whose root is `<mxGraphModel>`. Empty or whitespace-only input gives an
/// empty model, since draw.io writes empty pages that way.
pub fn read_model(src: &str) -> Result<GraphModel> {
    let src = src.trim_start_matches('\u{feff}');
    if src.trim().is_empty() {
        return Ok(GraphModel::default());
    }
    if src.len() > limits::MAX_PAGE_BYTES {
        return Err(DrawioError::Limit(format!("model larger than {} bytes", limits::MAX_PAGE_BYTES)));
    }
    let root = xml::parse(src)?;
    model_from_element(&root)
}

pub(crate) fn model_from_element(root: &Element) -> Result<GraphModel> {
    if root.name != "mxGraphModel" {
        return Err(DrawioError::NotDrawio(format!("expected <mxGraphModel>, found <{}>", root.name)));
    }
    let mut model = GraphModel { attributes: root.attrs.clone(), cells: Vec::new() };
    let Some(cells_root) = root.child("root") else {
        return Ok(model);
    };
    for child in &cells_root.children {
        let cell = match child.name.as_str() {
            "mxCell" => cell_from(child, None)?,
            "UserObject" | "object" => {
                let kind = if child.name == "UserObject" { WrapperKind::UserObject } else { WrapperKind::Object };
                let mut attributes = child.attrs.clone();
                let id = attributes.remove("id");
                let inner = child.child("mxCell");
                let mut cell = match inner {
                    Some(inner) => cell_from(inner, id.clone())?,
                    None => Cell { id: id.clone().unwrap_or_default(), ..Cell::default() },
                };
                if let Some(id) = id {
                    cell.id = id;
                }
                cell.wrapper = Some(Wrapper { kind, attributes });
                cell
            }
            // Unknown elements under <root> have no meaning we can keep.
            _ => continue,
        };
        if model.cells.len() >= limits::MAX_CELLS {
            return Err(DrawioError::Limit(format!("more than {} cells", limits::MAX_CELLS)));
        }
        model.cells.push(cell);
    }
    Ok(model)
}

fn cell_from(el: &Element, wrapper_id: Option<String>) -> Result<Cell> {
    let mut a = el.attrs.clone();
    let id = a.remove("id");
    let mut cell = Cell {
        id: wrapper_id.or(id).unwrap_or_default(),
        parent: a.remove("parent"),
        value: a.remove("value"),
        style: a.remove("style").map(|s| Style::parse(&s)),
        vertex: take_flag(&mut a, "vertex"),
        edge: take_flag(&mut a, "edge"),
        source: a.remove("source"),
        target: a.remove("target"),
        connectable: take_opt_flag(&mut a, "connectable"),
        visible: take_opt_flag(&mut a, "visible"),
        collapsed: take_flag(&mut a, "collapsed"),
        geometry: None,
        extra: Attributes::new(),
        wrapper: None,
    };
    cell.extra = a;
    cell.geometry = el.child("mxGeometry").map(geometry_from);
    Ok(cell)
}

/// Removes a flag that is either set or not. A junk value is kept in the extra attributes so it
/// is not silently lost.
fn take_flag(a: &mut Attributes, key: &str) -> bool {
    match a.get(key).map(parse_bool) {
        Some(Some(v)) => {
            a.remove(key);
            v
        }
        _ => false,
    }
}

fn take_opt_flag(a: &mut Attributes, key: &str) -> Option<bool> {
    let v = a.get(key).and_then(parse_bool)?;
    a.remove(key);
    Some(v)
}

/// Removes a numeric attribute; junk values stay in the extra attributes.
fn take_num(a: &mut Attributes, key: &str) -> f64 {
    match a.get(key).map(parse_f64) {
        Some(Some(v)) => {
            a.remove(key);
            v
        }
        _ => 0.0,
    }
}

fn point_from(el: &Element) -> Point {
    Point { x: el.attrs.get_f64("x").unwrap_or(0.0), y: el.attrs.get_f64("y").unwrap_or(0.0) }
}

fn geometry_from(el: &Element) -> Geometry {
    let mut a = el.attrs.clone();
    let mut g = Geometry {
        x: take_num(&mut a, "x"),
        y: take_num(&mut a, "y"),
        width: take_num(&mut a, "width"),
        height: take_num(&mut a, "height"),
        relative: take_flag(&mut a, "relative"),
        as_: a.remove("as"),
        ..Geometry::default()
    };
    g.extra = a;
    for child in &el.children {
        let role = child.attrs.get("as");
        match (child.name.as_str(), role) {
            ("mxPoint", Some("sourcePoint")) => g.source_point = Some(point_from(child)),
            ("mxPoint", Some("targetPoint")) => g.target_point = Some(point_from(child)),
            ("mxPoint", Some("offset")) => g.offset = Some(point_from(child)),
            ("Array", Some("points")) => {
                g.points = child.children.iter().filter(|p| p.name == "mxPoint").map(point_from).collect();
            }
            ("mxRectangle", Some("alternateBounds")) => {
                let r = &child.attrs;
                g.alternate_bounds = Some(Rect {
                    x: r.get_f64("x").unwrap_or(0.0),
                    y: r.get_f64("y").unwrap_or(0.0),
                    width: r.get_f64("width").unwrap_or(0.0),
                    height: r.get_f64("height").unwrap_or(0.0),
                });
            }
            _ => {}
        }
    }
    g
}

/// Serialises a model as `<mxGraphModel>` XML, one cell per line.
pub fn write_model(model: &GraphModel) -> String {
    let mut out = String::new();
    out.push_str("<mxGraphModel");
    write_attrs(&mut out, &model.attributes);
    out.push_str(">\n  <root>\n");
    for cell in &model.cells {
        write_cell(&mut out, cell);
    }
    out.push_str("  </root>\n</mxGraphModel>");
    out
}

fn write_cell(out: &mut String, cell: &Cell) {
    let indent = match &cell.wrapper {
        Some(w) => {
            out.push_str("    <");
            out.push_str(w.kind.tag());
            write_attrs(out, &w.attributes);
            write_attr(out, "id", &cell.id);
            out.push_str(">\n");
            "      "
        }
        None => "    ",
    };
    out.push_str(indent);
    out.push_str("<mxCell");
    if cell.wrapper.is_none() {
        write_attr(out, "id", &cell.id);
    }
    if let Some(v) = &cell.value {
        write_attr(out, "value", v);
    }
    if let Some(s) = &cell.style {
        write_attr(out, "style", &s.to_style_string());
    }
    if cell.vertex {
        write_attr(out, "vertex", "1");
    }
    if cell.edge {
        write_attr(out, "edge", "1");
    }
    if let Some(c) = cell.connectable {
        write_attr(out, "connectable", if c { "1" } else { "0" });
    }
    if let Some(v) = cell.visible {
        write_attr(out, "visible", if v { "1" } else { "0" });
    }
    if cell.collapsed {
        write_attr(out, "collapsed", "1");
    }
    if let Some(p) = &cell.parent {
        write_attr(out, "parent", p);
    }
    if let Some(s) = &cell.source {
        write_attr(out, "source", s);
    }
    if let Some(t) = &cell.target {
        write_attr(out, "target", t);
    }
    write_attrs(out, &cell.extra);
    match &cell.geometry {
        Some(g) => {
            out.push_str(">\n");
            write_geometry(out, g, indent);
            out.push_str(indent);
            out.push_str("</mxCell>\n");
        }
        None => out.push_str(" />\n"),
    }
    if let Some(w) = &cell.wrapper {
        out.push_str("    </");
        out.push_str(w.kind.tag());
        out.push_str(">\n");
    }
}

fn write_num(out: &mut String, key: &str, v: f64) {
    // draw.io omits zero coordinates; reading treats a missing one as zero, so this round-trips.
    if v != 0.0 && v.is_finite() {
        write_attr(out, key, &format_f64(v));
    }
}

fn write_point(out: &mut String, indent: &str, p: &Point, role: Option<&str>) {
    out.push_str(indent);
    out.push_str("<mxPoint");
    write_num(out, "x", p.x);
    write_num(out, "y", p.y);
    if let Some(r) = role {
        write_attr(out, "as", r);
    }
    out.push_str(" />\n");
}

fn write_geometry(out: &mut String, g: &Geometry, indent: &str) {
    out.push_str(indent);
    out.push_str("  <mxGeometry");
    write_num(out, "x", g.x);
    write_num(out, "y", g.y);
    write_num(out, "width", g.width);
    write_num(out, "height", g.height);
    if g.relative {
        write_attr(out, "relative", "1");
    }
    write_attrs(out, &g.extra);
    if let Some(a) = &g.as_ {
        write_attr(out, "as", a);
    }
    let has_children =
        g.source_point.is_some() || g.target_point.is_some() || !g.points.is_empty() || g.offset.is_some() || g.alternate_bounds.is_some();
    if !has_children {
        out.push_str(" />\n");
        return;
    }
    out.push_str(">\n");
    let inner = format!("{indent}    ");
    if let Some(p) = &g.source_point {
        write_point(out, &inner, p, Some("sourcePoint"));
    }
    if let Some(p) = &g.target_point {
        write_point(out, &inner, p, Some("targetPoint"));
    }
    if !g.points.is_empty() {
        out.push_str(&inner);
        out.push_str("<Array as=\"points\">\n");
        let pt_indent = format!("{inner}  ");
        for p in &g.points {
            write_point(out, &pt_indent, p, None);
        }
        out.push_str(&inner);
        out.push_str("</Array>\n");
    }
    if let Some(p) = &g.offset {
        write_point(out, &inner, p, Some("offset"));
    }
    if let Some(r) = &g.alternate_bounds {
        out.push_str(&inner);
        out.push_str("<mxRectangle");
        write_num(out, "x", r.x);
        write_num(out, "y", r.y);
        write_num(out, "width", r.width);
        write_num(out, "height", r.height);
        write_attr(out, "as", "alternateBounds");
        out.push_str(" />\n");
    }
    out.push_str(indent);
    out.push_str("  </mxGeometry>\n");
}
