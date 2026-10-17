//! The document model: documents hold pages, pages hold shapes and connectors.
//!
//! Every type is plain data with serde. Fields are camelCase on disk, every field has a default,
//! and wrong-typed values fall back to defaults, so `{}` and files from newer versions load.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geom::{Path, Point, Rect, Transform, finite};
use crate::kind::ShapeKind;
use crate::serde_util::{lenient, lenient_vec, num};
use crate::{MAX_EXTENT, MAX_TEXT};

/// Document metadata: author, timestamps and revision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreProps {
    /// Who made the document.
    pub author: String,
    /// Who saved it last.
    pub last_modified_by: String,
    /// ISO-8601 timestamp of the first save, empty until then.
    pub created: String,
    /// ISO-8601 timestamp of the last save, empty until the first save.
    pub modified: String,
    /// Save count.
    pub revision: u32,
}

/// An sRGB colour with alpha. Serialized as `"#rrggbb"`, or `"#rrggbbaa"` when not opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
    /// Alpha, 0 (transparent) to 255 (opaque).
    pub a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl Color {
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Fully transparent (no fill).
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    /// An opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }
    /// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` is optional); `None` for anything else.
    pub fn parse(s: &str) -> Option<Color> {
        let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
        if !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex.len() {
            3 => {
                let nib = |i: usize| hex.get(i..=i).and_then(|h| u8::from_str_radix(h, 16).ok()).map(|v| v * 17);
                Some(Color::rgb(nib(0)?, nib(1)?, nib(2)?))
            }
            6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Color { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
            _ => None,
        }
    }
    /// The hex form used on disk.
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    /// Unparseable colours load as black rather than failing the document.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s: String = lenient(d)?;
        Ok(Color::parse(&s).unwrap_or_default())
    }
}

/// Fill, line and text style for a shape or connector.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Style {
    /// Fill colour (unused by connectors). Transparent means no fill.
    pub fill: Color,
    /// Line (stroke) colour.
    pub line: Color,
    /// Line width in world units.
    #[serde(deserialize_with = "num")]
    pub line_width: f64,
    /// Font size in points at 100% zoom.
    #[serde(deserialize_with = "num")]
    pub font_size: f64,
    /// Text colour.
    pub font_color: Color,
    /// Bold text.
    #[serde(deserialize_with = "lenient")]
    pub bold: bool,
    /// Italic text.
    #[serde(deserialize_with = "lenient")]
    pub italic: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style::DEFAULT
    }
}

impl Style {
    /// The style a new shape gets when nothing else says otherwise: white fill, slate line.
    pub const DEFAULT: Style = Style {
        fill: Color::WHITE,
        line: Color::rgb(0x3a, 0x4a, 0x5c),
        line_width: 1.5,
        font_size: 12.0,
        font_color: Color::rgb(0x1e, 0x26, 0x30),
        bold: false,
        italic: false,
    };

    /// Largest line width kept.
    pub const MAX_LINE_WIDTH: f64 = 100.0;
    /// Largest font size kept.
    pub const MAX_FONT_SIZE: f64 = 1000.0;

    /// Clamp the numbers to sane ranges (line width 0 to 100, font size 1 to 1000).
    pub fn sanitize(&mut self) {
        self.line_width = if self.line_width.is_finite() { self.line_width.clamp(0.0, Self::MAX_LINE_WIDTH) } else { 1.0 };
        self.font_size = if self.font_size.is_finite() && self.font_size > 0.0 { self.font_size.clamp(1.0, Self::MAX_FONT_SIZE) } else { 12.0 };
    }
}

/// A shape on a page: a frame (position, size, rotation), an outline kind, text and style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shape {
    /// Unique id within the document (`s1`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// The master this shape came from (`basic.rectangle`, `flowchart.decision`); informational.
    #[serde(deserialize_with = "lenient")]
    pub stencil: String,
    /// The outline geometry.
    #[serde(deserialize_with = "lenient")]
    pub kind: ShapeKind,
    /// Left of the unrotated frame.
    #[serde(deserialize_with = "num")]
    pub x: f64,
    /// Top of the unrotated frame.
    #[serde(deserialize_with = "num")]
    pub y: f64,
    /// Width of the frame.
    #[serde(deserialize_with = "num")]
    pub w: f64,
    /// Height of the frame.
    #[serde(deserialize_with = "num")]
    pub h: f64,
    /// Rotation about the frame centre in degrees, clockwise on screen.
    #[serde(deserialize_with = "num")]
    pub angle_deg: f64,
    /// The shape's text.
    #[serde(deserialize_with = "lenient")]
    pub text: String,
    /// Fill, line and text style.
    #[serde(deserialize_with = "lenient")]
    pub style: Style,
    /// Connection points in normalized `0..1` frame coordinates. Empty means the kind's defaults.
    #[serde(deserialize_with = "lenient_vec")]
    pub connection_points: Vec<Point>,
    /// Data from other formats that VizCraft does not model yet (for example `drawio.style`), kept
    /// so writing the file back loses nothing.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "BTreeMap::is_empty")]
    pub ext: BTreeMap<String, String>,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            id: String::new(),
            stencil: String::new(),
            kind: ShapeKind::Rectangle,
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 60.0,
            angle_deg: 0.0,
            text: String::new(),
            style: Style::DEFAULT,
            connection_points: Vec::new(),
            ext: BTreeMap::new(),
        }
    }
}

impl Shape {
    /// A shape of `kind` with the kind's default connection points and the default style.
    pub fn new(id: impl Into<String>, kind: ShapeKind, x: f64, y: f64, w: f64, h: f64) -> Self {
        let mut s = Shape { id: id.into(), kind, x, y, w, h, connection_points: kind.connection_points(), ..Shape::default() };
        s.sanitize();
        s
    }
    /// Clamp position, size, angle, text length and style into the model's safe ranges.
    pub fn sanitize(&mut self) {
        self.x = finite(self.x, 0.0);
        self.y = finite(self.y, 0.0);
        self.w = if self.w.is_finite() { self.w.clamp(0.0, MAX_EXTENT) } else { 0.0 };
        self.h = if self.h.is_finite() { self.h.clamp(0.0, MAX_EXTENT) } else { 0.0 };
        self.angle_deg = if self.angle_deg.is_finite() { self.angle_deg.rem_euclid(360.0) } else { 0.0 };
        truncate(&mut self.text, MAX_TEXT);
        self.style.sanitize();
        for p in &mut self.connection_points {
            *p = Point::new(finite(p.x, 0.5).clamp(0.0, 1.0), finite(p.y, 0.5).clamp(0.0, 1.0));
        }
        sanitize_ext(&mut self.ext);
    }
    /// The unrotated frame.
    pub fn frame(&self) -> Rect {
        Rect::from_xywh(self.x, self.y, self.w, self.h)
    }
    /// Centre of the frame (rotation does not move it).
    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    /// Local frame coordinates (`(0, 0)` to `(w, h)`) to world coordinates, rotation included.
    pub fn transform(&self) -> Transform {
        Transform::rotate_about(Point::new(self.w / 2.0, self.h / 2.0), self.angle_deg).then(&Transform::translate(self.x, self.y))
    }
    /// The outline in world coordinates.
    pub fn outline_world(&self) -> Path {
        self.kind.outline(self.w, self.h).transformed(&self.transform())
    }
    /// Interior detail lines in world coordinates, if the kind has any.
    pub fn detail_world(&self) -> Option<Path> {
        self.kind.detail(self.w, self.h).map(|p| p.transformed(&self.transform()))
    }
    /// Axis-aligned bounds of the rotated outline.
    pub fn bounds(&self) -> Rect {
        self.outline_world().bounds().unwrap_or_else(|| Rect::from_xywh(self.x, self.y, 0.0, 0.0))
    }
    /// The connection points in use, normalized: the stored ones, else the kind's defaults.
    pub fn connection_points(&self) -> Vec<Point> {
        if self.connection_points.is_empty() { self.kind.connection_points() } else { self.connection_points.clone() }
    }
    /// Connection point `i` in world coordinates.
    pub fn connection_point_world(&self, i: usize) -> Option<Point> {
        let n = self.connection_points().get(i).copied()?;
        Some(self.transform().apply(Point::new(n.x * self.w, n.y * self.h)))
    }
    /// Every connection point in world coordinates, in index order.
    pub fn connection_points_world(&self) -> Vec<Point> {
        let t = self.transform();
        self.connection_points().into_iter().map(|n| t.apply(Point::new(n.x * self.w, n.y * self.h))).collect()
    }
    /// The outward direction at connection point `i` in world space: the frame side the point is
    /// nearest to, rotated with the shape. `None` for a point at the centre or a bad index.
    pub fn connection_direction(&self, i: usize) -> Option<Point> {
        let n = self.connection_points().get(i).copied()?;
        let (dx, dy) = (n.x - 0.5, n.y - 0.5);
        let local = if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
            return None;
        } else if dx.abs() >= dy.abs() {
            Point::new(dx.signum(), 0.0)
        } else {
            Point::new(0.0, dy.signum())
        };
        self.transform().apply_vector(local).normalized()
    }
}

/// Keep at most `max` bytes of `s`, cutting at a char boundary.
/// Most bends one connector keeps.
pub const MAX_WAYPOINTS: usize = 1000;

/// Most `ext` entries one item keeps.
pub const MAX_EXT_ENTRIES: usize = 256;

/// Longest `ext` key kept, in bytes; longer keys are dropped.
const MAX_EXT_KEY: usize = 256;

/// Longest id an import or a caller may choose.
pub const MAX_ID: usize = 64;

/// Whether `id` is a plain id a caller may choose: 1 to [`MAX_ID`] letters, digits, `_`, `-` or `.`.
pub fn is_plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Keep `ext` bounded: at most [`MAX_EXT_ENTRIES`] entries, short keys, values capped at
/// [`MAX_TEXT`].
fn sanitize_ext(ext: &mut BTreeMap<String, String>) {
    ext.retain(|k, _| !k.is_empty() && k.len() <= MAX_EXT_KEY);
    while ext.len() > MAX_EXT_ENTRIES {
        if ext.pop_last().is_none() {
            break;
        }
    }
    for v in ext.values_mut() {
        truncate(v, MAX_TEXT);
    }
}

fn truncate(s: &mut String, max: usize) {
    if s.len() > max {
        let mut cut = max;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
}

/// How a connector runs between its ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Route {
    /// One straight segment.
    Straight,
    /// Horizontal and vertical segments, leaving each glued shape perpendicular to its side.
    /// Unknown route names in a file load as this.
    #[default]
    Orthogonal,
    /// One cubic curve.
    Curved,
}

/// An arrowhead style at one end of a connector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Arrow {
    /// No arrowhead. Unknown arrow names in a file load as this.
    #[default]
    None,
    /// Filled triangle.
    Triangle,
    /// Two open strokes (a chevron).
    Open,
    /// Filled diamond.
    Diamond,
    /// Filled circle.
    Circle,
}

crate::serde_util::named_enum!(Route, Route::Orthogonal, {
    Route::Straight => "straight",
    Route::Orthogonal => "orthogonal",
    Route::Curved => "curved",
});

crate::serde_util::named_enum!(Arrow, Arrow::None, {
    Arrow::None => "none",
    Arrow::Triangle => "triangle",
    Arrow::Open => "open",
    Arrow::Diamond => "diamond",
    Arrow::Circle => "circle",
});

/// One end of a connector: glued to a shape or free on the page.
///
/// On disk an end is `{"shape": "s1", "point": 2}` (point glue), `{"shape": "s1"}` (dynamic
/// glue) or `{"x": 10, "y": 20}` (free), the same shape the `connector.*` commands take.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum End {
    /// Glued to a shape. `point: Some(i)` sticks to connection point `i`; `None` is dynamic glue,
    /// which picks the shape's connection point nearest the other end every time it routes.
    Glued {
        /// The shape's id.
        shape: String,
        /// The connection point index, or `None` for dynamic glue.
        #[serde(skip_serializing_if = "Option::is_none")]
        point: Option<usize>,
    },
    /// A free point on the page.
    Free {
        /// World x.
        x: f64,
        /// World y.
        y: f64,
    },
}

impl Default for End {
    fn default() -> Self {
        End::Free { x: 0.0, y: 0.0 }
    }
}

impl End {
    /// Point glue to connection point `point` of `shape`.
    pub fn point(shape: impl Into<String>, point: usize) -> Self {
        End::Glued { shape: shape.into(), point: Some(point) }
    }
    /// Dynamic glue to `shape`.
    pub fn dynamic(shape: impl Into<String>) -> Self {
        End::Glued { shape: shape.into(), point: None }
    }
    /// A free end at `p`.
    pub fn free(p: Point) -> Self {
        End::Free { x: p.x, y: p.y }
    }
    /// The glued shape's id, if glued.
    pub fn shape(&self) -> Option<&str> {
        match self {
            End::Glued { shape, .. } => Some(shape),
            End::Free { .. } => None,
        }
    }
}

impl<'de> Deserialize<'de> for End {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// The on-disk form with every field optional, so junk becomes a free end at the origin.
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Raw {
            #[serde(deserialize_with = "lenient")]
            shape: Option<String>,
            #[serde(deserialize_with = "lenient")]
            point: Option<usize>,
            #[serde(deserialize_with = "num")]
            x: f64,
            #[serde(deserialize_with = "num")]
            y: f64,
        }
        let raw: Raw = lenient(d)?;
        Ok(match raw.shape {
            Some(shape) if !shape.is_empty() => End::Glued { shape, point: raw.point },
            _ => End::Free { x: raw.x, y: raw.y },
        })
    }
}

/// A connector between two ends, routed from the shapes it is glued to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Connector {
    /// Unique id within the document (`c1`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Where it starts.
    pub start: End,
    /// Where it ends.
    pub end: End,
    /// How it runs between the ends.
    #[serde(deserialize_with = "lenient")]
    pub route: Route,
    /// Arrowhead at the start.
    #[serde(deserialize_with = "lenient")]
    pub start_arrow: Arrow,
    /// Arrowhead at the end.
    #[serde(deserialize_with = "lenient")]
    pub end_arrow: Arrow,
    /// Label text, drawn at the middle of the route.
    #[serde(deserialize_with = "lenient")]
    pub text: String,
    /// Line and text style (the fill is unused).
    #[serde(deserialize_with = "lenient")]
    pub style: Style,
    /// Bends between the ends, in world coordinates. Empty means the route is worked out from the
    /// glue alone; set by obstacle-avoiding routing or read from a file.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub waypoints: Vec<Point>,
    /// Route around shapes: the engine recomputes `waypoints` after every document change.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub avoid: bool,
    /// Draw a small hop wherever this connector crosses one drawn before it.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub jumps: bool,
    /// Data from other formats that VizCraft does not model yet, kept for round trips.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "BTreeMap::is_empty")]
    pub ext: BTreeMap<String, String>,
}

impl Default for Connector {
    fn default() -> Self {
        Connector {
            id: String::new(),
            start: End::default(),
            end: End::default(),
            route: Route::Orthogonal,
            start_arrow: Arrow::None,
            end_arrow: Arrow::Triangle,
            text: String::new(),
            style: Style { fill: Color::TRANSPARENT, ..Style::DEFAULT },
            waypoints: Vec::new(),
            avoid: false,
            jumps: false,
            ext: BTreeMap::new(),
        }
    }
}

impl Connector {
    /// An orthogonal connector from `start` to `end` with a triangle arrowhead at the end.
    pub fn new(id: impl Into<String>, start: End, end: End) -> Self {
        Connector { id: id.into(), start, end, ..Connector::default() }
    }
    /// Clamp free end coordinates, text length and style into the model's safe ranges.
    pub fn sanitize(&mut self) {
        for e in [&mut self.start, &mut self.end] {
            if let End::Free { x, y } = e {
                *x = finite(*x, 0.0);
                *y = finite(*y, 0.0);
            }
        }
        truncate(&mut self.text, MAX_TEXT);
        self.style.sanitize();
        self.waypoints.truncate(MAX_WAYPOINTS);
        for p in &mut self.waypoints {
            *p = Point::new(finite(p.x, 0.0), finite(p.y, 0.0));
        }
        sanitize_ext(&mut self.ext);
    }
    /// Whether either end is glued to `shape`.
    pub fn touches(&self, shape: &str) -> bool {
        self.start.shape() == Some(shape) || self.end.shape() == Some(shape)
    }
}

/// One page of a document. Shapes and connectors are in z order, back to front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Page {
    /// Unique id within the document (`p1`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Name on the page tab.
    #[serde(deserialize_with = "lenient")]
    pub name: String,
    /// Page width in world units.
    #[serde(deserialize_with = "num")]
    pub width: f64,
    /// Page height in world units.
    #[serde(deserialize_with = "num")]
    pub height: f64,
    /// Shapes, back to front.
    #[serde(deserialize_with = "lenient_vec")]
    pub shapes: Vec<Shape>,
    /// Connectors, back to front (drawn above the shapes).
    #[serde(deserialize_with = "lenient_vec")]
    pub connectors: Vec<Connector>,
    /// Data from other formats that VizCraft does not model yet, kept for round trips.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "BTreeMap::is_empty")]
    pub ext: BTreeMap<String, String>,
}

impl Default for Page {
    fn default() -> Self {
        Page::new("p1", "Page-1")
    }
}

impl Page {
    /// Default page width: 11 inches at 100 units per inch, landscape.
    pub const DEFAULT_WIDTH: f64 = 1100.0;
    /// Default page height: 8.5 inches at 100 units per inch.
    pub const DEFAULT_HEIGHT: f64 = 850.0;

    /// A blank page of the default size.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Page {
            id: id.into(),
            name: name.into(),
            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
            shapes: Vec::new(),
            connectors: Vec::new(),
            ext: BTreeMap::new(),
        }
    }
    /// The page sheet as a rectangle at the origin.
    pub fn rect(&self) -> Rect {
        Rect::from_xywh(0.0, 0.0, self.width, self.height)
    }
    /// Clamp the page size and every shape and connector into safe ranges.
    pub fn sanitize(&mut self) {
        let dim = |v: f64, d: f64| if v.is_finite() && v >= 1.0 { v.min(MAX_EXTENT) } else { d };
        self.width = dim(self.width, Self::DEFAULT_WIDTH);
        self.height = dim(self.height, Self::DEFAULT_HEIGHT);
        self.shapes.iter_mut().for_each(Shape::sanitize);
        self.connectors.iter_mut().for_each(Connector::sanitize);
        sanitize_ext(&mut self.ext);
    }
    /// The shape with this id.
    pub fn shape(&self, id: &str) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }
    /// The shape with this id, mutably.
    pub fn shape_mut(&mut self, id: &str) -> Option<&mut Shape> {
        self.shapes.iter_mut().find(|s| s.id == id)
    }
    /// The connector with this id.
    pub fn connector(&self, id: &str) -> Option<&Connector> {
        self.connectors.iter().find(|c| c.id == id)
    }
    /// The connector with this id, mutably.
    pub fn connector_mut(&mut self, id: &str) -> Option<&mut Connector> {
        self.connectors.iter_mut().find(|c| c.id == id)
    }
    /// Whether any shape or connector has this id.
    pub fn contains_id(&self, id: &str) -> bool {
        self.shape(id).is_some() || self.connector(id).is_some()
    }
    /// Remove and return a shape. Connectors glued to it are left alone; call
    /// [`Page::detach_shape`] first to turn their ends into free points.
    pub fn remove_shape(&mut self, id: &str) -> Option<Shape> {
        let i = self.shapes.iter().position(|s| s.id == id)?;
        Some(self.shapes.remove(i))
    }
    /// Remove and return a connector.
    pub fn remove_connector(&mut self, id: &str) -> Option<Connector> {
        let i = self.connectors.iter().position(|c| c.id == id)?;
        Some(self.connectors.remove(i))
    }
    /// Ids of the connectors with at least one end glued to `shape`.
    pub fn connectors_glued_to(&self, shape: &str) -> Vec<String> {
        self.connectors.iter().filter(|c| c.touches(shape)).map(|c| c.id.clone()).collect()
    }
    /// Turn every connector end glued to `shape` into a free end at its current routed position,
    /// so deleting the shape leaves those connectors where they were drawn.
    pub fn detach_shape(&mut self, shape: &str) {
        let ends: Vec<(usize, crate::route::Ends)> =
            self.connectors.iter().enumerate().filter(|(_, c)| c.touches(shape)).map(|(i, c)| (i, crate::route::resolve_ends(self, c))).collect();
        for (i, ends) in ends {
            if let Some(c) = self.connectors.get_mut(i) {
                if c.start.shape() == Some(shape) {
                    c.start = End::free(ends.start.pos);
                }
                if c.end.shape() == Some(shape) {
                    c.end = End::free(ends.end.pos);
                }
            }
        }
    }
}

/// A diagram document: metadata and pages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "DocumentRepr")]
pub struct Document {
    /// Shown in the title bar when the document has no file yet.
    pub title: String,
    /// Author, timestamps, revision.
    pub core: CoreProps,
    /// Pages in tab order. A loaded document always has at least one.
    pub pages: Vec<Page>,
    /// Counter behind [`Document::next_id`]; the last number handed out.
    pub id_seq: u64,
}

/// The on-disk form; turned into a [`Document`] by fixing up anything a file got wrong.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct DocumentRepr {
    #[serde(deserialize_with = "lenient")]
    title: String,
    #[serde(deserialize_with = "lenient")]
    core: CoreProps,
    #[serde(deserialize_with = "lenient_vec")]
    pages: Vec<Page>,
    #[serde(deserialize_with = "lenient")]
    id_seq: u64,
}

impl From<DocumentRepr> for Document {
    fn from(r: DocumentRepr) -> Self {
        let mut doc = Document { title: r.title, core: r.core, pages: r.pages, id_seq: r.id_seq };
        doc.sanitize();
        doc
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

impl Document {
    /// A document with one blank page.
    pub fn new() -> Self {
        Document { title: String::new(), core: CoreProps::default(), pages: vec![Page::new("p1", "Page-1")], id_seq: 1 }
    }
    /// Make the document safe to work on: at least one page, every number in range.
    pub fn sanitize(&mut self) {
        if self.pages.is_empty() {
            let id = self.next_id("p");
            self.pages.push(Page::new(id, "Page-1"));
        }
        self.pages.iter_mut().for_each(Page::sanitize);
    }
    /// A fresh id such as `s7` or `c8`: the counter goes up by one (shared by every prefix, so an
    /// id is never reused after a delete) and skips any number already taken by a loaded file.
    pub fn next_id(&mut self, prefix: &str) -> String {
        loop {
            self.id_seq = self.id_seq.saturating_add(1);
            let id = format!("{prefix}{}", self.id_seq);
            // At u64::MAX the counter stops moving; a taken id there would loop forever, so give
            // up on uniqueness by counter and fall back to a suffix that cannot collide.
            if self.id_seq == u64::MAX {
                return format!("{prefix}{}x", self.id_seq);
            }
            if !self.contains_id(&id) {
                return id;
            }
        }
    }
    /// Whether any page, shape or connector has this id.
    pub fn contains_id(&self, id: &str) -> bool {
        self.pages.iter().any(|p| p.id == id || p.contains_id(id))
    }
    /// The active page. Cascade 1 has one page per document, so this is the first.
    pub fn page(&self) -> Option<&Page> {
        self.pages.first()
    }
    /// The active page, mutably.
    pub fn page_mut(&mut self) -> Option<&mut Page> {
        self.pages.first_mut()
    }
    /// The shape with this id on any page.
    pub fn shape(&self, id: &str) -> Option<&Shape> {
        self.pages.iter().find_map(|p| p.shape(id))
    }
    /// The shape with this id on any page, mutably.
    pub fn shape_mut(&mut self, id: &str) -> Option<&mut Shape> {
        self.pages.iter_mut().find_map(|p| p.shape_mut(id))
    }
    /// The connector with this id on any page.
    pub fn connector(&self, id: &str) -> Option<&Connector> {
        self.pages.iter().find_map(|p| p.connector(id))
    }
    /// The connector with this id on any page, mutably.
    pub fn connector_mut(&mut self, id: &str) -> Option<&mut Connector> {
        self.pages.iter_mut().find_map(|p| p.connector_mut(id))
    }
    /// Remove a shape from whichever page has it (see [`Page::remove_shape`]).
    pub fn remove_shape(&mut self, id: &str) -> Option<Shape> {
        self.pages.iter_mut().find_map(|p| p.remove_shape(id))
    }
    /// Remove a connector from whichever page has it.
    pub fn remove_connector(&mut self, id: &str) -> Option<Connector> {
        self.pages.iter_mut().find_map(|p| p.remove_connector(id))
    }
    /// The index of the page `r` names: an exact page id first, then an exact page name, then a
    /// zero-based index. The same order the draw.io page tools use, so agents can pass any of them.
    pub fn find_page(&self, r: &str) -> Option<usize> {
        self.pages
            .iter()
            .position(|p| p.id == r)
            .or_else(|| self.pages.iter().position(|p| p.name == r))
            .or_else(|| r.trim().parse::<usize>().ok().filter(|&i| i < self.pages.len()))
    }
    /// Append a blank page called `name` (or `Page-N` when empty) with a fresh id; returns its index.
    pub fn add_page(&mut self, name: &str) -> usize {
        let id = self.next_id("p");
        let name = if name.is_empty() { format!("Page-{}", self.pages.len().saturating_add(1)) } else { name.to_string() };
        let mut page = Page::new(id, name);
        truncate(&mut page.name, MAX_TEXT);
        self.pages.push(page);
        self.pages.len().saturating_sub(1)
    }
    /// Put the shapes and connectors of `src` on page `index`, after clearing it when `replace`
    /// is set, and return their ids (shapes first, then connectors, in `src` order).
    ///
    /// Imported ids are kept when they are plain ([`is_plain_id`]) and free in this document, so a
    /// Mermaid node `A` stays addressable as `A`; any other id gets a fresh one and connector glue
    /// is rewritten to match. Glue to a shape that is neither in `src` nor on the target page
    /// becomes a free end at the origin. An index past the last page changes nothing. With
    /// `replace`, the page also takes `src`'s `ext` data; its id, name and size stay.
    pub fn adopt(&mut self, index: usize, src: Page, replace: bool) -> Vec<String> {
        if index >= self.pages.len() {
            return Vec::new();
        }
        if replace && let Some(page) = self.pages.get_mut(index) {
            page.shapes.clear();
            page.connectors.clear();
            page.ext = src.ext.clone();
        }
        let mut renamed: HashMap<String, String> = HashMap::new();
        let mut ids = Vec::with_capacity(src.shapes.len().saturating_add(src.connectors.len()));
        // Every id in use, so large imports check each wanted id in constant time.
        let mut used: HashSet<String> = self
            .pages
            .iter()
            .flat_map(|p| p.shapes.iter().map(|s| s.id.clone()).chain(p.connectors.iter().map(|c| c.id.clone())).chain([p.id.clone()]))
            .collect();
        for mut shape in src.shapes {
            let id = self.adopted_id(&shape.id, "s", &mut used);
            renamed.entry(std::mem::take(&mut shape.id)).or_insert_with(|| id.clone());
            shape.id = id.clone();
            shape.sanitize();
            ids.push(id);
            if let Some(page) = self.pages.get_mut(index) {
                page.shapes.push(shape);
            }
        }
        for mut c in src.connectors {
            let id = self.adopted_id(&c.id, "c", &mut used);
            c.id = id.clone();
            for end in [&mut c.start, &mut c.end] {
                if let End::Glued { shape, .. } = end {
                    if let Some(new) = renamed.get(shape.as_str()) {
                        *shape = new.clone();
                    } else if !self.pages.get(index).is_some_and(|p| p.shape(shape).is_some()) {
                        *end = End::free(Point::ZERO);
                    }
                }
            }
            c.sanitize();
            ids.push(id);
            if let Some(page) = self.pages.get_mut(index) {
                page.connectors.push(c);
            }
        }
        ids
    }
    /// `wanted` when it is plain and not in `used`, else a fresh id; either way it joins `used`.
    fn adopted_id(&mut self, wanted: &str, prefix: &str, used: &mut HashSet<String>) -> String {
        let id = if is_plain_id(wanted) && !used.contains(wanted) { wanted.to_string() } else { self.next_id(prefix) };
        used.insert(id.clone());
        id
    }
}
