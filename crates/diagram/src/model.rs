//! The document model: documents hold pages, pages hold shapes and connectors.
//!
//! Every type is plain data with serde. Fields are camelCase on disk, every field has a default,
//! and wrong-typed values fall back to defaults, so `{}` and files from newer versions load.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::data::ShapeData;
use crate::doc_fields::{Comment, DocProperties, ImageAsset, sanitize_comments, sanitize_images};
use crate::geom::{Path, Point, Rect, Transform, finite};
use crate::kind::ShapeKind;
use crate::page_fields::{DrawingScale, Guide, HeaderFooter, Layer, MAX_LAYERS, Orientation, Units, sanitize_guides, sanitize_layers};
use crate::serde_util::{is_default, lenient, lenient_vec, num};
use crate::shape_fields::{Behavior, Hyperlink, ImageRef, Locks, TextBlock};
use crate::style_fields::{Bullets, Dash, Glow, Gradient, HAlign, LineCap, Margins, Shadow, TextCase, TextDirection, VAlign};
use crate::theme::DocTheme;
use crate::{MAX_EXTENT, MAX_TEXT};
use vizcraft_model::{Model, View};

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
    /// Font family name; empty means the default (Inter). At most [`Style::MAX_FONT_FAMILY`] bytes.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub font_family: String,
    /// Underlined text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub underline: bool,
    /// Struck-through text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    /// Letter case applied when drawing.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub text_case: TextCase,
    /// Colour behind the text; transparent means none.
    #[serde(skip_serializing_if = "is_transparent")]
    pub highlight: Color,
    /// Horizontal text alignment.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub h_align: HAlign,
    /// Vertical text alignment.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub v_align: VAlign,
    /// Marker before every text line.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub bullets: Bullets,
    /// Extra space before every text line, in world units, 0 to [`Style::MAX_INDENT`].
    #[serde(deserialize_with = "num", skip_serializing_if = "is_default")]
    pub indent: f64,
    /// Which way text runs.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub text_direction: TextDirection,
    /// Space between the text block edge and the text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub text_margins: Margins,
    /// Dash pattern of the line.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub dash: Dash,
    /// How open line ends are drawn.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub line_cap: LineCap,
    /// Corner rounding radius in world units, 0 to [`Style::MAX_CORNER_RADIUS`]; rounds the
    /// corners of straight-sided outlines and connector bends.
    #[serde(deserialize_with = "num", skip_serializing_if = "is_default")]
    pub corner_radius: f64,
    /// Gradient fill drawn instead of `fill` when set and drawable.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    /// Fill transparency, 0 (as the colour says) to 1 (invisible); multiplies the fill alpha.
    #[serde(deserialize_with = "num", skip_serializing_if = "is_default")]
    pub fill_transparency: f64,
    /// Line transparency, 0 to 1; multiplies the line alpha.
    #[serde(deserialize_with = "num", skip_serializing_if = "is_default")]
    pub line_transparency: f64,
    /// Drop shadow; `None` for no shadow.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// Soft edges: how far in from the outline the shape fades out, in world units, 0 to
    /// [`Style::MAX_EFFECT`].
    #[serde(deserialize_with = "num", skip_serializing_if = "is_default")]
    pub soft_edges: f64,
    /// Glow around the outline; `None` for no glow.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub glow: Option<Glow>,
}

fn is_transparent(c: &Color) -> bool {
    c.a == 0
}

/// `c` with its alpha scaled by `1 - transparency` (transparency clamped to `0..=1`).
fn fade(c: Color, transparency: f64) -> Color {
    let keep = 1.0 - finite(transparency, 0.0).clamp(0.0, 1.0);
    Color { a: (f64::from(c.a) * keep).round().clamp(0.0, 255.0) as u8, ..c }
}

impl Default for Style {
    fn default() -> Self {
        Style::DEFAULT
    }
}

impl Style {
    /// The style a new shape gets when nothing else says otherwise: white fill, slate line.
    pub const DEFAULT: Style = Style::basic(Color::WHITE, Color::rgb(0x3a, 0x4a, 0x5c), 1.5, 12.0, Color::rgb(0x1e, 0x26, 0x30));

    /// A style with these colours and sizes, upright regular text and every other field at its
    /// default. A `const fn` because `Style { .., ..Style::DEFAULT }` cannot be used in a constant
    /// (the style owns strings and lists).
    pub const fn basic(fill: Color, line: Color, line_width: f64, font_size: f64, font_color: Color) -> Style {
        Style {
            fill,
            line,
            line_width,
            font_size,
            font_color,
            bold: false,
            italic: false,
            font_family: String::new(),
            underline: false,
            strikethrough: false,
            text_case: TextCase::Normal,
            highlight: Color::TRANSPARENT,
            h_align: HAlign::Center,
            v_align: VAlign::Middle,
            bullets: Bullets::None,
            indent: 0.0,
            text_direction: TextDirection::LeftToRight,
            text_margins: Margins::DEFAULT,
            dash: Dash::Solid,
            line_cap: LineCap::Butt,
            corner_radius: 0.0,
            gradient: None,
            fill_transparency: 0.0,
            line_transparency: 0.0,
            shadow: None,
            soft_edges: 0.0,
            glow: None,
        }
    }

    /// Largest line width kept.
    pub const MAX_LINE_WIDTH: f64 = 100.0;
    /// Largest font size kept.
    pub const MAX_FONT_SIZE: f64 = 1000.0;

    /// Longest font family name kept, in bytes.
    pub const MAX_FONT_FAMILY: usize = 128;
    /// Largest indent kept.
    pub const MAX_INDENT: f64 = 1000.0;
    /// Largest corner radius kept.
    pub const MAX_CORNER_RADIUS: f64 = 10_000.0;
    /// Largest soft-edge distance kept.
    pub const MAX_EFFECT: f64 = 500.0;

    /// Clamp the numbers to sane ranges (line width 0 to 100, font size 1 to 1000, transparency 0
    /// to 1, distances to their maximums) and cap the font family name.
    pub fn sanitize(&mut self) {
        self.line_width = if self.line_width.is_finite() { self.line_width.clamp(0.0, Self::MAX_LINE_WIDTH) } else { 1.0 };
        self.font_size = if self.font_size.is_finite() && self.font_size > 0.0 { self.font_size.clamp(1.0, Self::MAX_FONT_SIZE) } else { 12.0 };
        truncate(&mut self.font_family, Self::MAX_FONT_FAMILY);
        let unit = |v: f64| finite(v, 0.0).clamp(0.0, 1.0);
        self.fill_transparency = unit(self.fill_transparency);
        self.line_transparency = unit(self.line_transparency);
        self.indent = finite(self.indent, 0.0).clamp(0.0, Self::MAX_INDENT);
        self.corner_radius = finite(self.corner_radius, 0.0).clamp(0.0, Self::MAX_CORNER_RADIUS);
        self.soft_edges = finite(self.soft_edges, 0.0).clamp(0.0, Self::MAX_EFFECT);
        self.text_margins.sanitize();
        if let Some(g) = &mut self.gradient {
            g.sanitize();
        }
        if let Some(s) = &mut self.shadow {
            s.sanitize();
        }
        if let Some(g) = &mut self.glow {
            g.sanitize();
        }
    }
    /// The fill colour with `fill_transparency` applied to its alpha.
    pub fn effective_fill(&self) -> Color {
        fade(self.fill, self.fill_transparency)
    }
    /// The line colour with `line_transparency` applied to its alpha.
    pub fn effective_line(&self) -> Color {
        fade(self.line, self.line_transparency)
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
    /// The model element this shape is a view of; `None` for free drawing. Its text shows the
    /// element's name.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    /// Ids of the page layers the shape is on (see [`Page::layers`]); empty is no layer.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    /// The group or container shape this shape belongs to, on the same page.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Where the text sits; `None` is the shape's own frame.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub text_block: Option<TextBlock>,
    /// Hyperlinks, at most [`Hyperlink::MAX_PER_SHAPE`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Hyperlink>,
    /// Tooltip text, at most [`Shape::MAX_SCREEN_TIP`] bytes.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub screen_tip: String,
    /// Protection against editing.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub locks: Locks,
    /// Behaviour flags.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub behavior: Behavior,
    /// A picture from the document's image store drawn in the frame.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageRef>,
    /// Typed shape data rows (see [`crate::data`]).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub data: ShapeData,
    /// Container, swimlane or callout settings (see [`crate::container`]); `None` for an
    /// ordinary shape.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub container: Option<crate::container::Container>,
    /// Sections stacked below the main text (see [`crate::compartment`]).
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub compartments: Vec<crate::compartment::Compartment>,
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
            element: None,
            layers: Vec::new(),
            parent: None,
            text_block: None,
            links: Vec::new(),
            screen_tip: String::new(),
            locks: Locks::default(),
            behavior: Behavior::default(),
            image: None,
            data: ShapeData::default(),
            container: None,
            compartments: Vec::new(),
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
        self.layers.iter_mut().for_each(|l| truncate(l, MAX_ID));
        self.layers.truncate(MAX_LAYERS);
        if let Some(p) = &mut self.parent {
            truncate(p, MAX_ID);
        }
        if let Some(t) = &mut self.text_block {
            t.sanitize();
        }
        self.links.truncate(Hyperlink::MAX_PER_SHAPE);
        self.links.iter_mut().for_each(Hyperlink::sanitize);
        truncate(&mut self.screen_tip, Self::MAX_SCREEN_TIP);
        if let Some(i) = &mut self.image {
            i.sanitize();
        }
        self.data.sanitize();
        if let Some(c) = &mut self.container {
            c.sanitize();
        }
        crate::compartment::sanitize_all(&mut self.compartments);
    }
    /// Longest screen tip kept, in bytes.
    pub const MAX_SCREEN_TIP: usize = 4096;
    /// The text block in the shape's unrotated local frame (`(0, 0)` to `(w, h)`) with its own
    /// rotation relative to the shape: the stored [`TextBlock`], the main band above the
    /// compartments, or the whole frame.
    pub fn text_rect(&self) -> (Rect, f64) {
        match &self.text_block {
            None if !self.compartments.is_empty() => (crate::compartment::main_rect(self), 0.0),
            Some(t) => {
                let c = Point::new(self.w / 2.0 + t.dx, self.h / 2.0 + t.dy);
                (Rect::from_xywh(c.x - t.w / 2.0, c.y - t.h / 2.0, t.w, t.h), t.angle_deg)
            }
            None => (Rect::from_xywh(0.0, 0.0, self.w, self.h), 0.0),
        }
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
    /// The connection points in use, normalized: the stored ones, else the kind's defaults, then
    /// two per compartment (see [`crate::compartment::points`]).
    pub fn connection_points(&self) -> Vec<Point> {
        let mut pts = if self.connection_points.is_empty() { self.kind.connection_points() } else { self.connection_points.clone() };
        pts.extend(crate::compartment::points(self));
        pts
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

/// Most bends one connector keeps.
pub const MAX_WAYPOINTS: usize = 1000;

/// Most background pages drawn under one page.
pub const MAX_BACKGROUND_DEPTH: usize = 8;

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

/// Keep at most `max` bytes of `s`, cutting at a char boundary.
pub(crate) fn truncate(s: &mut String, max: usize) {
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
/// On disk an end is `{"shape": "s1", "point": 2}` (point glue), `{"shape": "s1", "port":
/// "attrs.left"}` (port glue to a compartment's point), `{"shape": "s1"}` (dynamic glue) or
/// `{"x": 10, "y": 20}` (free), the same shape the `connector.*` commands take.
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
        /// A compartment port (`"attrs.left"`); when it names a point on the shape it wins over
        /// `point` (see [`End::resolve_port`]).
        #[serde(skip_serializing_if = "Option::is_none")]
        port: Option<String>,
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
        End::Glued { shape: shape.into(), point: Some(point), port: None }
    }
    /// Dynamic glue to `shape`.
    pub fn dynamic(shape: impl Into<String>) -> Self {
        End::Glued { shape: shape.into(), point: None, port: None }
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
            #[serde(deserialize_with = "lenient")]
            port: Option<String>,
            #[serde(deserialize_with = "num")]
            x: f64,
            #[serde(deserialize_with = "num")]
            y: f64,
        }
        let raw: Raw = lenient(d)?;
        Ok(match raw.shape {
            Some(shape) if !shape.is_empty() => {
                let port = raw.port.filter(|p| !p.is_empty() && p.len() <= crate::compartment::MAX_PORT);
                End::Glued { shape, point: raw.point, port }
            }
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
    /// The model relationship this connector is a view of; `None` for free drawing. Its label
    /// shows the relationship's name.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
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
            relationship: None,
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
    /// The model view this page shows (viewpoint and layout); `None` for a free-drawing page.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub view: Option<View>,
    /// Portrait or landscape; informational next to `width` and `height`, which are the size.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub orientation: Orientation,
    /// Units for rulers and measurements.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub units: Units,
    /// Drawing scale.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub drawing_scale: DrawingScale,
    /// The page grows to fit its shapes.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub auto_size: bool,
    /// The background page drawn under this one: the id of another page with `is_background`.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// This page is a background page for others.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub is_background: bool,
    /// Header text, drawn near the top edge in exports.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub header: HeaderFooter,
    /// Footer text, drawn near the bottom edge in exports.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub footer: HeaderFooter,
    /// Guide lines, at most [`crate::page_fields::MAX_GUIDES`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
    /// Layers, at most [`MAX_LAYERS`], with unique ids.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
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
            view: None,
            orientation: Orientation::Landscape,
            units: Units::Inches,
            drawing_scale: DrawingScale::ONE_TO_ONE,
            auto_size: false,
            background: None,
            is_background: false,
            header: HeaderFooter::default(),
            footer: HeaderFooter::default(),
            guides: Vec::new(),
            layers: Vec::new(),
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
        self.drawing_scale.sanitize();
        if let Some(b) = &mut self.background {
            truncate(b, MAX_ID);
        }
        self.header.sanitize();
        self.footer.sanitize();
        sanitize_guides(&mut self.guides);
        sanitize_layers(&mut self.layers);
        self.sanitize_shape_refs();
    }
    /// Drop shape layer ids that name no layer on this page (and repeats), and parents that are
    /// not another shape on this page or that would make a cycle.
    fn sanitize_shape_refs(&mut self) {
        let layer_ids: HashSet<&str> = self.layers.iter().map(|l| l.id.as_str()).collect();
        for s in &mut self.shapes {
            let mut seen = HashSet::new();
            s.layers.retain(|l| layer_ids.contains(l.as_str()) && seen.insert(l.clone()));
        }
        let index: HashMap<String, usize> = self.shapes.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        let mut parent: Vec<Option<usize>> =
            self.shapes.iter().enumerate().map(|(i, s)| s.parent.as_ref().and_then(|p| index.get(p).copied()).filter(|&p| p != i)).collect();
        // Walk each parent chain once (0 unvisited, 1 on the current chain, 2 done); a step back
        // onto the current chain is a cycle, broken at the shape that closes it.
        let mut state = vec![0u8; parent.len()];
        for start in 0..parent.len() {
            let mut chain = Vec::new();
            let mut at = Some(start);
            while let Some(i) = at {
                match state.get(i).copied() {
                    Some(0) => {
                        if let Some(st) = state.get_mut(i) {
                            *st = 1;
                        }
                        chain.push(i);
                        at = parent.get(i).copied().flatten();
                    }
                    Some(1) => {
                        if let Some(&last) = chain.last()
                            && let Some(p) = parent.get_mut(last)
                        {
                            *p = None;
                        }
                        break;
                    }
                    _ => break,
                }
            }
            for i in chain {
                if let Some(st) = state.get_mut(i) {
                    *st = 2;
                }
            }
        }
        let ids: Vec<String> = self.shapes.iter().map(|s| s.id.clone()).collect();
        for (s, p) in self.shapes.iter_mut().zip(parent) {
            s.parent = p.and_then(|p| ids.get(p).cloned());
        }
    }
    /// The layer with this id.
    pub fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }
    /// Whether `shape` is drawn: not hidden, and on no layer or on at least one visible layer.
    pub fn shape_visible(&self, shape: &Shape) -> bool {
        !shape.behavior.hidden && (shape.layers.is_empty() || shape.layers.iter().any(|l| self.layer(l).is_none_or(|l| l.visible)))
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
    /// The semantic model the pages are views of; empty for free drawing. Not part of the
    /// document's own serde form: the native file writes it as its own versioned section.
    #[serde(skip)]
    pub model: Model,
    /// File properties (title, author, subject, keywords, description).
    #[serde(skip_serializing_if = "is_default")]
    pub properties: DocProperties,
    /// Review comments, at most [`Comment::MAX_COMMENTS`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    /// The image store shapes draw pictures from (see [`ImageAsset`] for the caps).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAsset>,
    /// Id of the page the canvas shows and commands act on ([`Document::page`]). Empty, or an id
    /// that names no page, means the first page; loading clears an unknown id.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub active_page: String,
    /// The document theme and variant (see [`crate::theme`]); the default is no theme.
    #[serde(skip_serializing_if = "is_default")]
    pub theme: DocTheme,
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
    #[serde(deserialize_with = "lenient")]
    properties: DocProperties,
    #[serde(deserialize_with = "lenient_vec")]
    comments: Vec<Comment>,
    #[serde(deserialize_with = "lenient_vec")]
    images: Vec<ImageAsset>,
    #[serde(deserialize_with = "lenient")]
    active_page: String,
    theme: DocTheme,
}

impl From<DocumentRepr> for Document {
    fn from(r: DocumentRepr) -> Self {
        let mut doc = Document {
            title: r.title,
            core: r.core,
            pages: r.pages,
            id_seq: r.id_seq,
            model: Model::new(),
            properties: r.properties,
            comments: r.comments,
            images: r.images,
            active_page: r.active_page,
            theme: r.theme,
        };
        // Pages only: the model is a separate section the caller sets afterwards, so checking view
        // references here would clear every one of them. The caller runs `sanitize` once the
        // model is in place.
        doc.sanitize_pages();
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
        Document {
            title: String::new(),
            core: CoreProps::default(),
            pages: vec![Page::new("p1", "Page-1")],
            id_seq: 1,
            model: Model::new(),
            properties: DocProperties::default(),
            comments: Vec::new(),
            images: Vec::new(),
            active_page: String::new(),
            theme: DocTheme::default(),
        }
    }
    /// Make the document safe to work on: at least one page, every number in range, a sane model,
    /// and view references that point at model items (see [`crate::views::sanitize_refs`]).
    pub fn sanitize(&mut self) {
        self.sanitize_pages();
        self.model.sanitize();
        crate::views::sanitize_refs(self);
    }
    /// At least one page, every page, shape and connector in range, the properties, comments and
    /// image store bounded, and references between them repaired; the model and view references
    /// are left alone.
    fn sanitize_pages(&mut self) {
        if self.pages.is_empty() {
            let id = self.next_id("p");
            self.pages.push(Page::new(id, "Page-1"));
        }
        self.pages.iter_mut().for_each(Page::sanitize);
        self.properties.sanitize();
        sanitize_comments(&mut self.comments);
        sanitize_images(&mut self.images);
        if !self.pages.iter().any(|p| p.id == self.active_page) {
            self.active_page.clear();
        }
        self.theme.sanitize();
        self.sanitize_doc_refs();
    }
    /// Clear references that name nothing: a background that is not another background page, a
    /// shape picture whose asset is not in the store, and comment anchors (an unknown page moves
    /// the comment to the first page; an unknown shape anchors it to its page).
    fn sanitize_doc_refs(&mut self) {
        let backgrounds: HashSet<String> = self.pages.iter().filter(|p| p.is_background).map(|p| p.id.clone()).collect();
        let assets: HashSet<&str> = self.images.iter().map(|a| a.id.as_str()).collect();
        for page in &mut self.pages {
            if page.background.as_ref().is_some_and(|b| *b == page.id || !backgrounds.contains(b)) {
                page.background = None;
            }
            for s in &mut page.shapes {
                if s.image.as_ref().is_some_and(|i| !assets.contains(i.asset.as_str())) {
                    s.image = None;
                }
            }
        }
        let first = self.pages.first().map(|p| p.id.clone()).unwrap_or_default();
        for c in &mut self.comments {
            let page = self.pages.iter().find(|p| p.id == c.anchor.page);
            let shape_ok = |p: &Page| c.anchor.shape.as_deref().is_none_or(|s| p.shape(s).is_some());
            match page {
                Some(p) if shape_ok(p) => {}
                Some(_) => c.anchor.shape = None,
                None => {
                    c.anchor.page = first.clone();
                    c.anchor.shape = None;
                }
            }
        }
    }
    /// The background pages drawn under page `index`, deepest first (a background may have its
    /// own background). Stops at a repeat or after [`MAX_BACKGROUND_DEPTH`] pages.
    pub fn background_chain(&self, index: usize) -> Vec<&Page> {
        let mut chain: Vec<&Page> = Vec::new();
        let mut at = self.pages.get(index).and_then(|p| p.background.as_deref());
        let mut seen: HashSet<&str> = self.pages.get(index).map(|p| p.id.as_str()).into_iter().collect();
        while let Some(id) = at {
            if chain.len() >= MAX_BACKGROUND_DEPTH || !seen.insert(id) {
                break;
            }
            let Some(p) = self.pages.iter().find(|p| p.id == id) else { break };
            chain.push(p);
            at = p.background.as_deref();
        }
        chain.reverse();
        chain
    }
    /// The image asset with this id.
    pub fn image(&self, id: &str) -> Option<&ImageAsset> {
        self.images.iter().find(|a| a.id == id)
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
    /// Whether any page, shape, connector, element or relationship has this id: they share one id
    /// space, so an id names exactly one thing.
    pub fn contains_id(&self, id: &str) -> bool {
        self.pages.iter().any(|p| p.id == id || p.contains_id(id)) || self.model.contains_id(id)
    }
    /// The index of the active page: the page [`Document::active_page`] names, else the first.
    pub fn active_index(&self) -> usize {
        self.pages.iter().position(|p| p.id == self.active_page).unwrap_or(0)
    }
    /// Make page `index` the active page; false (and no change) when there is no such page.
    pub fn set_active_index(&mut self, index: usize) -> bool {
        let Some(p) = self.pages.get(index) else { return false };
        self.active_page = p.id.clone();
        true
    }
    /// The active page (see [`Document::active_page`]); every page-scoped command acts on it.
    pub fn page(&self) -> Option<&Page> {
        self.pages.get(self.active_index())
    }
    /// The active page, mutably.
    pub fn page_mut(&mut self) -> Option<&mut Page> {
        let i = self.active_index();
        self.pages.get_mut(i)
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
            .chain(self.model.elements.iter().map(|e| e.id.clone()))
            .chain(self.model.relationships.iter().map(|r| r.id.clone()))
            .collect();
        for mut shape in src.shapes {
            let id = self.adopted_id(&shape.id, "s", &mut used);
            renamed.entry(std::mem::take(&mut shape.id)).or_insert_with(|| id.clone());
            shape.id = id.clone();
            // A reference to an element this document does not have would be dangling.
            if shape.element.as_deref().is_some_and(|e| self.model.element(e).is_none()) {
                shape.element = None;
            }
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
            if c.relationship.as_deref().is_some_and(|r| self.model.relationship(r).is_none()) {
                c.relationship = None;
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
