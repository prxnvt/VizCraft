//! draw.io cells to VizCraft shapes and connectors.

use std::collections::{BTreeMap, HashMap};

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Point, Route, Shape, ShapeKind, Style};
use vizcraft_drawio::{Cell, GraphModel, Style as DStyle, StyleItem, WrapperKind, parse_color};

use super::Warnings;

/// `ext` key for the style items VizCraft does not map.
pub(crate) const EXT_STYLE: &str = "drawio.style";
/// `ext` key naming the wrapper element (`UserObject` or `object`).
pub(crate) const EXT_WRAPPER: &str = "drawio.wrapper";
/// Prefix of the `ext` keys holding the wrapper's attributes.
pub(crate) const EXT_ATTR: &str = "drawio.attr.";
/// `ext` key for `fontStyle` bits beyond bold and italic.
pub(crate) const EXT_FONT_STYLE: &str = "drawio.fontStyle";
/// Prefix of the page `ext` keys holding `mxGraphModel` attributes.
pub(crate) const EXT_MODEL: &str = "drawio.model.";
/// The wrapper attribute carrying a shape's compartments as JSON. draw.io keeps it untouched;
/// on import it lands in `ext` as `drawio.attr.vizcraftCompartments` and is read back into
/// `Shape::compartments` ([`take_compartments`]).
pub(crate) const COMPARTMENTS_ATTR: &str = "vizcraftCompartments";

/// What draw.io draws when a vertex style leaves a key out: white fill, black line.
pub(crate) const DRAWIO_SHAPE: Style = Style::basic(Color::WHITE, Color::BLACK, 1.0, 12.0, Color::BLACK);

/// What draw.io draws when an edge style leaves a key out.
pub(crate) const DRAWIO_EDGE: Style = Style::basic(Color::TRANSPARENT, Color::BLACK, 1.0, 12.0, Color::BLACK);

/// Deepest chain of nested vertices followed when adding up group offsets.
const MAX_NESTING: usize = 64;

/// The `shape=` values of the contract table and the kind each one maps to.
const SHAPES: &[(&str, ShapeKind)] = &[
    ("rect", ShapeKind::Rectangle),
    ("rectangle", ShapeKind::Rectangle),
    ("ellipse", ShapeKind::Ellipse),
    ("rhombus", ShapeKind::Diamond),
    ("triangle", ShapeKind::Triangle),
    ("parallelogram", ShapeKind::Parallelogram),
    ("hexagon", ShapeKind::Hexagon),
    ("cylinder3", ShapeKind::Cylinder),
    ("cylinder", ShapeKind::Cylinder),
    ("document", ShapeKind::Document),
    ("cloud", ShapeKind::Cloud),
    ("process", ShapeKind::PredefinedProcess),
    ("manualInput", ShapeKind::ManualInput),
    ("trapezoid", ShapeKind::Trapezoid),
    ("offPageConnector", ShapeKind::OffPageReference),
];

/// The stencil master a shape of `kind` is filed under when the file names none.
pub(crate) fn master_for_kind(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Rectangle | ShapeKind::Freeform => "basic.rectangle",
        ShapeKind::RoundedRectangle => "basic.roundedRectangle",
        ShapeKind::Ellipse => "basic.ellipse",
        ShapeKind::Diamond => "flowchart.decision",
        ShapeKind::Triangle => "basic.triangle",
        ShapeKind::Parallelogram => "basic.parallelogram",
        ShapeKind::Hexagon => "basic.hexagon",
        ShapeKind::Cylinder => "basic.cylinder",
        ShapeKind::Document => "basic.document",
        ShapeKind::Cloud => "basic.cloud",
        ShapeKind::Terminator => "flowchart.terminator",
        ShapeKind::PredefinedProcess => "flowchart.predefinedProcess",
        ShapeKind::ManualInput => "flowchart.manualInput",
        ShapeKind::Preparation => "flowchart.preparation",
        ShapeKind::Trapezoid => "flowchart.manualOperation",
        ShapeKind::OffPageReference => "flowchart.offPageReference",
    }
}

/// How a vertex style maps onto a shape.
pub(crate) struct VertexLook {
    /// The outline.
    pub kind: ShapeKind,
    /// The stencil master id.
    pub stencil: String,
    /// Fill, line and text.
    pub style: Style,
    /// Rotation in degrees.
    pub angle: f64,
    /// Leftover style and extra font bits, for `ext`.
    pub ext: BTreeMap<String, String>,
    /// Why the cell cannot be shown as a shape (an image), when it cannot.
    pub skip: Option<String>,
}

/// How an edge style maps onto a connector.
pub(crate) struct EdgeLook {
    /// The route style.
    pub route: Route,
    /// Arrowheads at the start and end.
    pub arrows: (Arrow, Arrow),
    /// The connector kind (`vizcraftKind`), empty for none.
    pub kind: String,
    /// Line and text.
    pub style: Style,
    /// Line jumps.
    pub jumps: bool,
    /// `exitX`/`exitY`, when both are given.
    pub exit: Option<Point>,
    /// `entryX`/`entryY`, when both are given.
    pub entry: Option<Point>,
    /// Leftover style and extra font bits, for `ext`.
    pub ext: BTreeMap<String, String>,
}

/// Remove the bare name `name`, saying whether it was there.
fn take_name(s: &mut DStyle, name: &str) -> bool {
    let had = s.has_name(name);
    s.items.retain(|i| !matches!(i, StyleItem::Name(n) if n == name));
    had
}

/// A colour value: `none` is transparent, a hex colour is that colour, anything else (`default`,
/// named colours) keeps `fallback`.
fn color(v: Option<String>, fallback: Color) -> Color {
    match v.as_deref().map(str::trim) {
        Some("none") => Color::TRANSPARENT,
        Some(t) => parse_color(t).map(|c| Color::rgb(c.r, c.g, c.b)).unwrap_or(fallback),
        None => fallback,
    }
}

fn number(v: Option<String>) -> Option<f64> {
    v.and_then(|t| vizcraft_drawio::parse_f64(&t))
}

/// Read the keys shared by vertices and edges into `style`, removing them from `s`.
fn common(s: &mut DStyle, style: &mut Style, ext: &mut BTreeMap<String, String>) {
    style.fill = color(s.remove("fillColor"), style.fill);
    style.line = color(s.remove("strokeColor"), style.line);
    style.font_color = color(s.remove("fontColor"), style.font_color);
    if let Some(w) = number(s.remove("strokeWidth")) {
        style.line_width = w;
    }
    if let Some(f) = number(s.remove("fontSize")) {
        style.font_size = f;
    }
    if let Some(bits) = number(s.remove("fontStyle")) {
        // A bit mask: 1 bold, 2 italic, 4 underline, 8 strike-through. The last two are kept so
        // writing the file puts them back.
        let bits = bits.clamp(0.0, 255.0) as u32;
        style.bold = bits & 1 != 0;
        style.italic = bits & 2 != 0;
        if bits & !3 != 0 {
            ext.insert(EXT_FONT_STYLE.into(), (bits & !3).to_string());
        }
    }
    // Labels are always read as plain text and written back in one form of our choosing.
    s.remove("html");
    if s.get("whiteSpace") == Some("wrap") {
        s.remove("whiteSpace");
    }
    style.sanitize();
}

fn leftover(s: &DStyle, ext: &mut BTreeMap<String, String>) {
    let mut s = s.clone();
    s.trailing_semicolon = false;
    let text = s.to_style_string();
    if !text.is_empty() {
        ext.insert(EXT_STYLE.into(), text);
    }
}

/// Map a vertex style onto a shape's kind, master and style.
pub(crate) fn vertex_look(style: Option<&DStyle>, warnings: &mut Warnings) -> VertexLook {
    let mut s = style.cloned().unwrap_or_default();
    let mut ext = BTreeMap::new();
    if s.has_name("image") || s.get("shape") == Some("image") {
        return VertexLook {
            kind: ShapeKind::Rectangle,
            stencil: String::new(),
            style: DRAWIO_SHAPE,
            angle: 0.0,
            ext,
            skip: Some("images are not supported yet".into()),
        };
    }
    let master = s.remove("vizcraft").and_then(|id| {
        let m = vizcraft_stencils::master(id.trim());
        if m.is_none() {
            warnings.push(format!("unknown VizCraft master `{}`; the draw.io style is used instead", super::short(&id)));
        }
        m
    });
    let kind = match master {
        Some(m) => {
            // The writer adds draw.io's own keys for the kind so draw.io draws it; the master
            // already says what it is, so exactly those keys are dropped rather than kept twice.
            let shape_key = |s: &mut DStyle, v: &str| {
                if s.get("shape") == Some(v) {
                    s.remove("shape");
                }
            };
            match m.kind {
                ShapeKind::Rectangle | ShapeKind::RoundedRectangle | ShapeKind::Freeform => {
                    s.remove("rounded");
                }
                ShapeKind::Ellipse => {
                    take_name(&mut s, "ellipse");
                }
                ShapeKind::Diamond => {
                    take_name(&mut s, "rhombus");
                }
                ShapeKind::Triangle => {
                    take_name(&mut s, "triangle");
                    if s.get("direction") == Some("north") {
                        s.remove("direction");
                    }
                }
                ShapeKind::Cloud => {
                    take_name(&mut s, "ellipse");
                    shape_key(&mut s, "cloud");
                }
                ShapeKind::Terminator => {
                    s.remove("rounded");
                    s.remove("arcSize");
                }
                ShapeKind::Parallelogram => shape_key(&mut s, "parallelogram"),
                ShapeKind::Hexagon | ShapeKind::Preparation => shape_key(&mut s, "hexagon"),
                ShapeKind::Cylinder => shape_key(&mut s, "cylinder3"),
                ShapeKind::Document => shape_key(&mut s, "document"),
                ShapeKind::PredefinedProcess => shape_key(&mut s, "process"),
                ShapeKind::ManualInput => shape_key(&mut s, "manualInput"),
                ShapeKind::Trapezoid => shape_key(&mut s, "trapezoid"),
                ShapeKind::OffPageReference => shape_key(&mut s, "offPageConnector"),
            }
            m.kind
        }
        None => {
            let named = s.get("shape").map(str::to_string);
            let mut kind = match named {
                Some(n) => match SHAPES.iter().find(|(name, _)| *name == n) {
                    Some((_, k)) => {
                        s.remove("shape");
                        if *k == ShapeKind::Cloud {
                            take_name(&mut s, "ellipse");
                        }
                        *k
                    }
                    None => {
                        warnings.push(format!("draw.io shape `{}` is drawn as a rectangle", super::short(&n)));
                        ShapeKind::Rectangle
                    }
                },
                None if take_name(&mut s, "ellipse") => ShapeKind::Ellipse,
                None if take_name(&mut s, "rhombus") => ShapeKind::Diamond,
                None if take_name(&mut s, "triangle") => ShapeKind::Triangle,
                None => ShapeKind::Rectangle,
            };
            if kind == ShapeKind::Triangle && s.get("direction") == Some("north") {
                s.remove("direction");
            }
            if kind == ShapeKind::Rectangle {
                // The writer always says `rounded` for rectangles, so it is consumed here; an
                // unknown `shape=` drawn as a rectangle keeps its rounding too.
                if s.get_bool("rounded") == Some(true) {
                    kind = ShapeKind::RoundedRectangle;
                }
                s.remove("rounded");
            }
            kind
        }
    };
    let mut style = master.map(|m| m.style.clone()).unwrap_or(DRAWIO_SHAPE);
    if s.has_name("text") {
        // A text cell draws no box unless its style asks for one.
        style.fill = Color::TRANSPARENT;
        style.line = Color::TRANSPARENT;
    }
    common(&mut s, &mut style, &mut ext);
    let angle = number(s.remove("rotation")).unwrap_or(0.0);
    leftover(&s, &mut ext);
    VertexLook { kind, stencil: master.map_or_else(|| master_for_kind(kind).to_string(), |m| m.id.to_string()), style, angle, ext, skip: None }
}

/// The style key naming a connector's kind (`flowchart.flow`), the edge twin of the vertex `vizcraft`
/// master key.
pub(crate) const KIND_KEY: &str = "vizcraftKind";

/// The arrow a `startArrow` or `endArrow` value stands for. `hollow` is whether the matching
/// `startFill` or `endFill` is `0`, which turns the block, diamond and oval into their hollow
/// forms; the ER arrows are the notation ends.
fn arrow(v: Option<String>, absent: Arrow, hollow: bool) -> Arrow {
    match v.as_deref().map(str::trim) {
        None => absent,
        Some("none" | "") => Arrow::None,
        Some("open" | "openThin" | "openAsync") => Arrow::Open,
        Some("diamond" | "diamondThin") if hollow => Arrow::HollowDiamond,
        Some("diamond" | "diamondThin") => Arrow::Diamond,
        Some("oval" | "dot" | "circle" | "circlePlus") if hollow => Arrow::OpenCircle,
        Some("oval" | "dot" | "circle" | "circlePlus") => Arrow::Circle,
        Some("ERone" | "ERmandOne") => Arrow::Bar,
        Some("ERmany") => Arrow::CrowsFoot,
        Some("ERoneToMany") => Arrow::CrowsFootBar,
        Some("ERzeroToMany") => Arrow::CrowsFootCircle,
        Some("ERzeroToOne") => Arrow::BarCircle,
        Some("dash") => Arrow::Slash,
        // block, classic, their thin forms and anything unknown: a triangle is the closest.
        Some(_) if hollow => Arrow::HollowTriangle,
        Some(_) => Arrow::Triangle,
    }
}

/// Read `fill_key` (`startFill` or `endFill`) when it is `0` and the arrow value `arrow_key` is
/// one that has a hollow form; the fill key is then used up. Other values stay in the style.
fn take_hollow(s: &mut DStyle, arrow_key: &str, fill_key: &str) -> bool {
    let hollow_form =
        s.get(arrow_key).map(str::trim).is_some_and(|v| !matches!(v, "none" | "" | "open" | "openThin" | "openAsync") && !v.starts_with("ER"));
    let zero = s.get(fill_key).is_some_and(|v| v.trim() == "0");
    if hollow_form && zero {
        s.remove(fill_key);
        true
    } else {
        false
    }
}

/// Map an edge style onto a connector's route, arrows, style and glue hints.
pub(crate) fn edge_look(style: Option<&DStyle>) -> EdgeLook {
    let mut s = style.cloned().unwrap_or_default();
    let mut ext = BTreeMap::new();
    let orthogonal = matches!(s.get("edgeStyle"), Some("orthogonalEdgeStyle" | "elbowEdgeStyle"));
    let curved = s.get_bool("curved") == Some(true);
    s.remove("curved");
    // A curved connector is written with the orthogonal style (see the writer), so any edge
    // style it had is replaced.
    if orthogonal || curved {
        s.remove("edgeStyle");
    }
    let route = if curved {
        Route::Curved
    } else if orthogonal {
        Route::Orthogonal
    } else {
        Route::Straight
    };
    // draw.io draws a classic arrow at the end and nothing at the start when a style says nothing.
    let (start_hollow, end_hollow) = (take_hollow(&mut s, "startArrow", "startFill"), take_hollow(&mut s, "endArrow", "endFill"));
    let arrows = (arrow(s.remove("startArrow"), Arrow::None, start_hollow), arrow(s.remove("endArrow"), Arrow::Triangle, end_hollow));
    let kind = s.remove(KIND_KEY).unwrap_or_default();
    let jumps = s.remove("jumpStyle").is_some_and(|v| !matches!(v.trim(), "" | "none"));
    let pair = |s: &mut DStyle, kx: &str, ky: &str| {
        let (x, y) = (number(s.remove(kx)), number(s.remove(ky)));
        Some(Point::new(x?, y?))
    };
    let exit = pair(&mut s, "exitX", "exitY");
    let entry = pair(&mut s, "entryX", "entryY");
    let mut st = DRAWIO_EDGE;
    common(&mut s, &mut st, &mut ext);
    st.fill = Color::TRANSPARENT;
    leftover(&s, &mut ext);
    EdgeLook { route, arrows, kind, style: st, jumps, exit, entry, ext }
}

/// Keep a wrapper's kind and attributes (other than the label) in `ext`.
fn wrapper_ext(cell: &Cell, ext: &mut BTreeMap<String, String>) {
    let Some(w) = &cell.wrapper else { return };
    let kind = match w.kind {
        WrapperKind::UserObject => "UserObject",
        WrapperKind::Object => "object",
    };
    ext.insert(EXT_WRAPPER.into(), kind.into());
    for (k, v) in w.attributes.iter() {
        if k != "label" {
            ext.insert(format!("{EXT_ATTR}{k}"), v.to_string());
        }
    }
}

/// Move the compartments a VizCraft export carried in the wrapper attribute
/// [`COMPARTMENTS_ATTR`] out of `ext` into `Shape::compartments` (entries that do not parse are
/// dropped). A plain `object` wrapper that held nothing else was only their carrier and goes too,
/// so a round trip leaves `ext` as it was.
fn take_compartments(shape: &mut Shape) {
    let Some(json) = shape.ext.remove(&format!("{EXT_ATTR}{COMPARTMENTS_ATTR}")) else { return };
    let items: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap_or_default();
    shape.compartments =
        items.into_iter().take(vizcraft_diagram::compartment::MAX_COMPARTMENTS).filter_map(|v| serde_json::from_value(v).ok()).collect();
    if shape.ext.get(EXT_WRAPPER).is_some_and(|w| w == "object") && !shape.ext.keys().any(|k| k.starts_with(EXT_ATTR)) {
        shape.ext.remove(EXT_WRAPPER);
    }
}

/// Move the freeform outline a VizCraft export carried in the wrapper attribute
/// `vizcraftPath` out of `ext` into `Shape::path`, the same way as [`take_compartments`].
fn take_path(shape: &mut Shape) {
    let Some(json) = shape.ext.remove(&format!("{EXT_ATTR}{}", vizcraft_diagram::shapeop::PATH_ATTR)) else { return };
    vizcraft_diagram::shapeop::set_path_json(shape, &json);
    if shape.ext.get(EXT_WRAPPER).is_some_and(|w| w == "object") && !shape.ext.keys().any(|k| k.starts_with(EXT_ATTR)) {
        shape.ext.remove(EXT_WRAPPER);
    }
}

/// The wrapper attributes a VizCraft export carries a connector's end labels in.
pub(crate) const START_LABEL_ATTR: &str = "vizcraftStartLabel";
/// See [`START_LABEL_ATTR`].
pub(crate) const END_LABEL_ATTR: &str = "vizcraftEndLabel";

/// Move a connector's end labels out of `ext` into `start_label` and `end_label`, the same way
/// as [`take_compartments`].
fn take_end_labels(c: &mut Connector) {
    let start = c.ext.remove(&format!("{EXT_ATTR}{START_LABEL_ATTR}"));
    let end = c.ext.remove(&format!("{EXT_ATTR}{END_LABEL_ATTR}"));
    if start.is_none() && end.is_none() {
        return;
    }
    c.start_label = start.unwrap_or_default();
    c.end_label = end.unwrap_or_default();
    if c.ext.get(EXT_WRAPPER).is_some_and(|w| w == "object") && !c.ext.keys().any(|k| k.starts_with(EXT_ATTR)) {
        c.ext.remove(EXT_WRAPPER);
    }
}

/// draw.io flattens nesting on export, so a slot's members come back without their parent: give
/// it back to the members a slot lists that have none.
fn restore_slot_parents(page: &mut Page) {
    let owners: Vec<(String, Vec<String>)> = page
        .shapes
        .iter()
        .filter(|s| !s.compartments.is_empty())
        .map(|s| (s.id.clone(), s.compartments.iter().flat_map(|c| c.members.iter().cloned()).collect()))
        .collect();
    for (owner, members) in owners {
        for m in page.shapes.iter_mut().filter(|m| m.parent.is_none() && m.id != owner && members.contains(&m.id)) {
            m.parent = Some(owner.clone());
        }
    }
}

/// Where a cell sits in the page's tree.
enum Place {
    /// On the page, with its coordinates offset by the containing vertices' positions.
    Page(Point),
    /// A label of the edge with this id.
    EdgeLabel(String),
    /// Under a parent that is missing or unusable.
    Lost,
}

struct Tree<'a> {
    cells: &'a [Cell],
    by_id: HashMap<&'a str, usize>,
}

impl<'a> Tree<'a> {
    fn get(&self, id: &str) -> Option<&'a Cell> {
        self.by_id.get(id).and_then(|&i| self.cells.get(i))
    }
    /// A layer: a child of the root that is neither a vertex nor an edge.
    fn is_layer(&self, c: &Cell) -> bool {
        !c.vertex && !c.edge && c.parent.as_deref().and_then(|p| self.get(p)).is_some_and(|p| p.parent.is_none())
    }
    /// Walk up from `c`'s parent, adding the position of every containing vertex.
    fn place(&self, c: &Cell, nested: &mut bool) -> Place {
        let mut offset = Point::ZERO;
        let mut parent = c.parent.as_deref();
        for _ in 0..MAX_NESTING {
            let Some(pid) = parent else { return Place::Page(offset) };
            let Some(p) = self.get(pid) else { return Place::Lost };
            if p.edge {
                return Place::EdgeLabel(p.id.clone());
            }
            if !p.vertex {
                // A layer or the root: the coordinates are page coordinates from here on.
                return Place::Page(offset);
            }
            *nested = true;
            if let Some(g) = &p.geometry {
                offset = Point::new(offset.x + g.x, offset.y + g.y);
            }
            parent = p.parent.as_deref();
        }
        Place::Lost
    }
}

/// One draw.io page model as a VizCraft page. Cell ids are kept as they are.
pub(crate) fn page_from_model(model: &GraphModel, id: String, name: String, warnings: &mut Warnings) -> Page {
    let mut page = Page::new(id, name);
    // A draw.io page is a sheet of the size the file gives; it keeps its look.
    page.board = vizcraft_diagram::Board::Fixed;
    if let Some(w) = model.page_width().filter(|w| *w > 0.0) {
        page.width = w;
    }
    if let Some(h) = model.page_height().filter(|h| *h > 0.0) {
        page.height = h;
    }
    for (k, v) in model.attributes.iter() {
        if k != "pageWidth" && k != "pageHeight" {
            page.ext.insert(format!("{EXT_MODEL}{k}"), v.to_string());
        }
    }
    let mut by_id = HashMap::with_capacity(model.cells.len());
    for (i, c) in model.cells.iter().enumerate() {
        if by_id.contains_key(c.id.as_str()) {
            warnings.push(format!("cell id `{}` is used twice; the later cell is skipped", super::short(&c.id)));
            continue;
        }
        by_id.insert(c.id.as_str(), i);
    }
    let tree = Tree { cells: &model.cells, by_id };
    let layers = model.cells.iter().filter(|c| tree.is_layer(c)).count();
    if layers > 1 {
        warnings.push(format!("{layers} layers were merged onto one page"));
    }

    // Absolute frames of every vertex, shown or not, so edges to a skipped cell still end there.
    let mut frames: HashMap<&str, (Point, Point)> = HashMap::new();
    let mut labels: HashMap<String, Vec<String>> = HashMap::new();
    let mut nested_any = false;
    for (i, c) in model.cells.iter().enumerate() {
        if tree.by_id.get(c.id.as_str()) != Some(&i) || !c.vertex {
            continue;
        }
        let mut nested = false;
        let offset = match tree.place(c, &mut nested) {
            Place::Page(o) => o,
            Place::EdgeLabel(edge) => {
                let text = c.plain_label();
                if !text.trim().is_empty() {
                    labels.entry(edge).or_default().push(text);
                }
                continue;
            }
            Place::Lost => {
                warnings.push(format!("cell `{}` has a missing parent and was skipped", super::short(&c.id)));
                continue;
            }
        };
        nested_any |= nested;
        let Some(g) = &c.geometry else {
            warnings.push(format!("cell `{}` has no geometry and was skipped", super::short(&c.id)));
            continue;
        };
        let top_left = Point::new(offset.x + g.x, offset.y + g.y);
        frames.insert(c.id.as_str(), (top_left, Point::new(g.width, g.height)));
        if c.style.as_ref().is_some_and(|s| s.has_name("group")) {
            // A group cell draws nothing itself; its members are placed with its offset.
            continue;
        }
        let look = vertex_look(c.style.as_ref(), warnings);
        if let Some(why) = look.skip {
            warnings.push(format!("cell `{}` was skipped: {why}", super::short(&c.id)));
            continue;
        }
        let mut shape = Shape::new(c.id.clone(), look.kind, top_left.x, top_left.y, g.width, g.height);
        shape.stencil = look.stencil;
        shape.style = look.style;
        shape.angle_deg = look.angle;
        shape.text = c.plain_label();
        shape.ext = look.ext;
        wrapper_ext(c, &mut shape.ext);
        take_compartments(&mut shape);
        take_path(&mut shape);
        shape.sanitize();
        page.shapes.push(shape);
    }
    restore_slot_parents(&mut page);
    if nested_any {
        warnings.push("groups and containers were flattened: their members are placed on the page directly");
    }

    let shape_index: HashMap<String, usize> = page.shapes.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
    for (i, c) in model.cells.iter().enumerate() {
        if tree.by_id.get(c.id.as_str()) != Some(&i) || !c.edge || c.vertex {
            continue;
        }
        let mut nested = false;
        let offset = match tree.place(c, &mut nested) {
            Place::Page(o) => o,
            Place::EdgeLabel(_) | Place::Lost => {
                warnings.push(format!("edge `{}` has an unusable parent and was skipped", super::short(&c.id)));
                continue;
            }
        };
        let look = edge_look(c.style.as_ref());
        let g = c.geometry.clone().unwrap_or_default();
        let shift = |p: &vizcraft_drawio::Point| Point::new(offset.x + p.x, offset.y + p.y);
        let waypoints: Vec<Point> = g.points.iter().map(shift).collect();
        let end = |cell: Option<&String>, hint: Option<Point>, loose: Option<&vizcraft_drawio::Point>, fallback: Option<Point>| -> End {
            if let Some(shape) = cell.and_then(|id| shape_index.get(id).and_then(|&i| page.shapes.get(i))) {
                return match hint {
                    Some(h) => {
                        let pts = shape.connection_points();
                        match vizcraft_diagram::route::nearest_point(&pts, h) {
                            Some(i) => match vizcraft_diagram::compartment::port_for_index(shape, i) {
                                Some(port) => End::port(shape.id.clone(), port),
                                None => End::point(shape.id.clone(), i),
                            },
                            None => End::dynamic(shape.id.clone()),
                        }
                    }
                    None => End::dynamic(shape.id.clone()),
                };
            }
            if let Some(p) = loose {
                return End::free(shift(p));
            }
            // An end on a skipped cell (an image) stays where that cell was.
            if let Some((tl, size)) = cell.and_then(|id| frames.get(id.as_str())) {
                return End::free(Point::new(tl.x + size.x / 2.0, tl.y + size.y / 2.0));
            }
            End::free(fallback.unwrap_or(Point::ZERO))
        };
        let start = end(c.source.as_ref(), look.exit, g.source_point.as_ref(), waypoints.first().copied());
        let finish = end(c.target.as_ref(), look.entry, g.target_point.as_ref(), waypoints.last().copied());
        let mut text = c.plain_label();
        if let Some(extra) = labels.remove(&c.id) {
            for t in extra {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&t);
            }
        }
        let mut conn = Connector {
            id: c.id.clone(),
            start,
            end: finish,
            route: look.route,
            start_arrow: look.arrows.0,
            end_arrow: look.arrows.1,
            text,
            kind: look.kind,
            style: look.style,
            waypoints,
            avoid: false,
            jumps: look.jumps,
            ext: look.ext,
            relationship: None,
            ..Connector::default()
        };
        wrapper_ext(c, &mut conn.ext);
        take_end_labels(&mut conn);
        conn.sanitize();
        page.connectors.push(conn);
    }
    page
}
