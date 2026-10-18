//! Data graphics: small pictures around a shape that show its data, and the links that fill shape
//! data from an external table.
//!
//! A [`DataGraphic`] is a named list of [`DataItem`]s kept in `Document::data_graphics`; a shape
//! shows one through `Shape::data_graphic`. Each item reads one value (a shape data key, or an
//! element property) and shows it as one of four kinds:
//!
//! | Kind | Shows |
//! |---|---|
//! | `text` | a callout with the label and the value in a number format |
//! | `bar` | a bar filled from `min` to `max` in the item's colour |
//! | `icons` | the icon of the first rule that matches (names from the app's icon set) |
//! | `color` | the fill (and optionally line) of the first rule that matches |
//!
//! [`layout`] places every drawn item in a rectangle around the shape's bounds and [`render`]
//! turns those rectangles into [`Mark`]s, so the canvas and the SVG export draw the same thing
//! from one place. Colour by value is applied at draw time on a copy ([`recolored`]); it never
//! writes `shape.style`.
//!
//! A [`DataLink`] names a CSV or XLSX file, the column whose values identify rows, and the shape
//! data key that holds the same value on shapes; the engine's `datalink.refresh` reads the file
//! and writes the rows into shape data.

use std::borrow::Cow;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::data::{self, DataType};
use crate::geom::{Point, Rect};
use crate::model::{Color, Document, MAX_ID, Page, Shape, truncate};
use crate::serde_util::{lenient, lenient_vec, named_enum};

/// Most data graphics one document keeps.
pub const MAX_GRAPHICS: usize = 256;
/// Most items one data graphic holds.
pub const MAX_ITEMS: usize = 32;
/// Most rules one item holds.
pub const MAX_RULES: usize = 32;
/// Most data links one document keeps.
pub const MAX_LINKS: usize = 64;
/// Longest name, label, rule value or format, in bytes.
pub const MAX_NAME: usize = 128;
/// Longest file path, in bytes.
pub const MAX_PATH: usize = 4096;
/// Most columns a data link names.
pub const MAX_COLUMNS: usize = 256;
/// The `ext` key on a legend group naming the graphic it explains.
pub const LEGEND_EXT: &str = "vizcraft.legend";

/// Space between the shape and an outside item, and between stacked items.
const GAP: f64 = 4.0;
/// Text size of callouts and their height.
const TEXT_SIZE: f64 = 10.0;
const TEXT_H: f64 = 16.0;
/// Bar height, and the width a bar takes beside a shape.
const BAR_H: f64 = 10.0;
const BAR_SIDE_W: f64 = 60.0;
/// Icon size.
const ICON: f64 = 16.0;
/// Callout and bar chrome.
const CHROME_LINE: Color = Color::rgb(0x9a, 0xa5, 0xb1);
const CHROME_FILL: Color = Color::rgb(0xff, 0xff, 0xff);
const BAR_TRACK: Color = Color::rgb(0xe4, 0xe7, 0xeb);
const TEXT_COLOR: Color = Color::rgb(0x1f, 0x29, 0x33);

/// What an item shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// A text callout.
    #[default]
    Text,
    /// A data bar.
    Bar,
    /// An icon set.
    Icons,
    /// Colour by value.
    Color,
}

named_enum!(ItemKind, ItemKind::Text, {
    ItemKind::Text => "text",
    ItemKind::Bar => "bar",
    ItemKind::Icons => "icons",
    ItemKind::Color => "color",
});

impl ItemKind {
    /// Every kind, in menu order.
    pub const ALL: [ItemKind; 4] = [ItemKind::Text, ItemKind::Bar, ItemKind::Icons, ItemKind::Color];
}

/// Where an item's value comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Source {
    /// A shape data row, then the page's definition of the key (its default), then a property of
    /// the element the shape draws.
    #[default]
    Data,
    /// A property: `text`, `id`, `name`, `type`, `x`, `y`, `width`, `height`, `angle`,
    /// `stencil`, or a typed property of the element the shape draws.
    Property,
}

named_enum!(Source, Source::Data, {
    Source::Data => "data",
    Source::Property => "property",
});

/// Where an item sits around the shape. The four sides are outside the shape, the centre and the
/// corners inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Position {
    /// Above the shape, centred.
    Top,
    /// Below the shape, centred.
    #[default]
    Bottom,
    /// Left of the shape, centred.
    Left,
    /// Right of the shape, centred.
    Right,
    /// Centred inside the shape.
    Center,
    /// Inside the top left corner.
    TopLeft,
    /// Inside the top right corner.
    TopRight,
    /// Inside the bottom left corner.
    BottomLeft,
    /// Inside the bottom right corner.
    BottomRight,
}

named_enum!(Position, Position::Bottom, {
    Position::Top => "top",
    Position::Bottom => "bottom",
    Position::Left => "left",
    Position::Right => "right",
    Position::Center => "center",
    Position::TopLeft => "topLeft",
    Position::TopRight => "topRight",
    Position::BottomLeft => "bottomLeft",
    Position::BottomRight => "bottomRight",
});

impl Position {
    /// Every position, in menu order.
    pub const ALL: [Position; 9] = [
        Position::Top,
        Position::Bottom,
        Position::Left,
        Position::Right,
        Position::Center,
        Position::TopLeft,
        Position::TopRight,
        Position::BottomLeft,
        Position::BottomRight,
    ];
}

/// How a rule compares the value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Op {
    /// Equal (numbers by value, text ignoring case).
    #[default]
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// From `value` to `to`, both included (a range).
    Between,
    /// The text contains `value`, ignoring case.
    Contains,
    /// The value is not set.
    Empty,
    /// The value is set.
    NotEmpty,
    /// Always matches: the last rule of a list, as an otherwise.
    Any,
}

named_enum!(Op, Op::Eq, {
    Op::Eq => "eq",
    Op::Ne => "ne",
    Op::Lt => "lt",
    Op::Le => "le",
    Op::Gt => "gt",
    Op::Ge => "ge",
    Op::Between => "between",
    Op::Contains => "contains",
    Op::Empty => "empty",
    Op::NotEmpty => "notEmpty",
    Op::Any => "any",
});

impl Op {
    /// Every operator, in menu order.
    pub const ALL: [Op; 11] = [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge, Op::Between, Op::Contains, Op::Empty, Op::NotEmpty, Op::Any];

    /// The operator for a name or a symbol (`=`, `!=`, `<`, `<=`, `>`, `>=`).
    pub fn parse(s: &str) -> Option<Op> {
        let s = s.trim();
        Op::from_name(s).or(match s {
            "=" | "==" => Some(Op::Eq),
            "!=" | "<>" => Some(Op::Ne),
            "<" => Some(Op::Lt),
            "<=" => Some(Op::Le),
            ">" => Some(Op::Gt),
            ">=" => Some(Op::Ge),
            _ => None,
        })
    }
    /// The symbol a legend shows.
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Between => "from",
            Op::Contains => "contains",
            Op::Empty => "is empty",
            Op::NotEmpty => "is set",
            Op::Any => "otherwise",
        }
    }
}

/// One rule of an icon set or a colour by value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rule {
    /// How the value is compared.
    #[serde(deserialize_with = "lenient")]
    pub op: Op,
    /// What it is compared with.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// The upper end of a `between` range.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub to: String,
    /// The icon an icon set shows (one of [`ICONS`]).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// The fill a colour by value applies.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// The line colour a colour by value applies.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub line: Option<Color>,
    /// What the legend says; empty says the rule itself.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// One item of a data graphic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataItem {
    /// What it shows.
    #[serde(deserialize_with = "lenient")]
    pub kind: ItemKind,
    /// Where the value comes from.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_data")]
    pub from: Source,
    /// The data key or property name.
    #[serde(deserialize_with = "lenient")]
    pub key: String,
    /// Where it sits.
    #[serde(deserialize_with = "lenient")]
    pub position: Position,
    /// Text before the value in a callout.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Number format of a callout: `0`, `0.00`, `#,##0`, `0%`, `$#,##0.00`; empty is general.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub format: String,
    /// The value an empty bar stands for.
    #[serde(deserialize_with = "crate::serde_util::num")]
    pub min: f64,
    /// The value a full bar stands for.
    #[serde(deserialize_with = "crate::serde_util::num")]
    pub max: f64,
    /// The bar colour.
    #[serde(deserialize_with = "lenient")]
    pub color: Color,
    /// Rules of an icon set or colour by value, first match wins.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

fn is_data(s: &Source) -> bool {
    *s == Source::Data
}

impl Default for DataItem {
    fn default() -> Self {
        DataItem {
            kind: ItemKind::Text,
            from: Source::Data,
            key: String::new(),
            position: Position::Bottom,
            label: String::new(),
            format: String::new(),
            min: 0.0,
            max: 100.0,
            color: Color::rgb(0x3b, 0x82, 0xf6),
            rules: Vec::new(),
        }
    }
}

/// A named set of items drawn around every shape that shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataGraphic {
    /// Unique id among the document's graphics.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Name shown in the gallery.
    #[serde(deserialize_with = "lenient")]
    pub name: String,
    /// The items, at most [`MAX_ITEMS`].
    #[serde(deserialize_with = "lenient_vec")]
    pub items: Vec<DataItem>,
}

/// The table file format of a data link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TableFormat {
    /// Comma separated values with a header row.
    #[default]
    Csv,
    /// An Office Open XML workbook; the first sheet unless one is named.
    Xlsx,
}

named_enum!(TableFormat, TableFormat::Csv, {
    TableFormat::Csv => "csv",
    TableFormat::Xlsx => "xlsx",
});

impl TableFormat {
    /// The format a file name's extension suggests: `.xlsx` and `.xlsm` are workbooks, anything
    /// else CSV.
    pub fn for_path(path: &str) -> TableFormat {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".xlsx") || lower.ends_with(".xlsm") { TableFormat::Xlsx } else { TableFormat::Csv }
    }
}

/// A link from shape data to an external table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataLink {
    /// Unique id among the links.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// The file.
    #[serde(deserialize_with = "lenient")]
    pub path: String,
    /// Its format.
    #[serde(deserialize_with = "lenient")]
    pub format: TableFormat,
    /// The sheet of a workbook; empty is the first.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub sheet: String,
    /// The column whose values identify rows.
    #[serde(deserialize_with = "lenient")]
    pub key_column: String,
    /// The shape data key holding the same value on shapes.
    #[serde(deserialize_with = "lenient")]
    pub data_key: String,
    /// The columns written to shape data; empty is every column.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// When the file was last read (ISO-8601).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub last_refresh: String,
}

impl Rule {
    /// Clamp text lengths and drop an unknown icon.
    pub fn sanitize(&mut self) {
        truncate(&mut self.value, MAX_NAME);
        truncate(&mut self.to, MAX_NAME);
        truncate(&mut self.label, MAX_NAME);
        if !ICONS.contains(&self.icon.as_str()) {
            self.icon.clear();
        }
    }
}

impl DataItem {
    /// Clamp text, keep `min` below `max` and cap the rules.
    pub fn sanitize(&mut self) {
        truncate(&mut self.key, data::MAX_KEY);
        truncate(&mut self.label, MAX_NAME);
        truncate(&mut self.format, MAX_NAME);
        self.min = if self.min.is_finite() { self.min.clamp(-1e15, 1e15) } else { 0.0 };
        self.max = if self.max.is_finite() { self.max.clamp(-1e15, 1e15) } else { 100.0 };
        if self.max <= self.min {
            self.max = self.min + 1.0;
        }
        self.rules.truncate(MAX_RULES);
        self.rules.iter_mut().for_each(Rule::sanitize);
    }
}

impl DataGraphic {
    /// Clamp the name and cap the items. Returns false when the id is unusable.
    pub fn sanitize(&mut self) -> bool {
        truncate(&mut self.name, MAX_NAME);
        self.items.truncate(MAX_ITEMS);
        self.items.iter_mut().for_each(DataItem::sanitize);
        crate::is_plain_id(&self.id)
    }
    /// The name, or the id when there is none.
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() { &self.id } else { &self.name }
    }
}

impl DataLink {
    /// Clamp text and cap the columns. Returns false when the id is unusable.
    pub fn sanitize(&mut self) -> bool {
        truncate(&mut self.path, MAX_PATH);
        truncate(&mut self.sheet, MAX_NAME);
        truncate(&mut self.key_column, MAX_NAME);
        truncate(&mut self.data_key, data::MAX_KEY);
        truncate(&mut self.last_refresh, 64);
        self.columns.truncate(MAX_COLUMNS);
        self.columns.iter_mut().for_each(|c| truncate(c, MAX_NAME));
        crate::is_plain_id(&self.id)
    }
}

/// Keep the document's graphics and links bounded with unique ids, and clear every shape's
/// `data_graphic` that names no graphic.
pub fn sanitize_doc(doc: &mut Document) {
    let mut seen = HashSet::new();
    doc.data_graphics.retain_mut(|g| g.sanitize() && seen.insert(g.id.clone()));
    doc.data_graphics.truncate(MAX_GRAPHICS);
    let mut seen = HashSet::new();
    doc.data_links.retain_mut(|l| l.sanitize() && seen.insert(l.id.clone()));
    doc.data_links.truncate(MAX_LINKS);
    let ids: HashSet<&str> = doc.data_graphics.iter().map(|g| g.id.as_str()).collect();
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            if let Some(g) = &mut s.data_graphic {
                truncate(g, MAX_ID);
                if !ids.contains(g.as_str()) {
                    s.data_graphic = None;
                }
            }
        }
    }
}

/// The names of the icons an icon set can show; each is also a name in the app's icon set.
pub const ICONS: &[&str] = &["check", "close", "info", "plus", "minus", "chevronUp", "chevronDown"];

/// A value read for an item: its text and, when it reads as one, its number.
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    /// The value as text; never empty.
    pub text: String,
    /// The number, when the text is one.
    pub number: Option<f64>,
}

impl Value {
    fn new(text: String) -> Option<Value> {
        if text.trim().is_empty() {
            return None;
        }
        let number = text.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        Some(Value { text, number })
    }
}

/// The value item `item` reads from `shape` on `page`: see [`Source`]. `None` when it is not set.
pub fn value_of(doc: &Document, page: &Page, shape: &Shape, item: &DataItem) -> Option<Value> {
    let element = shape.element.as_deref().and_then(|e| doc.model.element(e));
    let prop = |key: &str| element.and_then(|el| el.properties.get(key)).map(prop_text);
    let text = match item.from {
        Source::Data => match shape.data.row(&item.key) {
            Some(r) if !r.value.is_empty() => Some(r.value.clone()),
            _ => data::page_keys(page)
                .into_iter()
                .find(|k| k.key == item.key && shape.data.page_keys)
                .and_then(|k| k.default)
                .or_else(|| prop(&item.key)),
        },
        Source::Property => match item.key.as_str() {
            "text" => Some(shape.text.clone()),
            "id" => Some(shape.id.clone()),
            "name" => Some(element.map(|e| e.name.clone()).unwrap_or_else(|| shape.text.clone())),
            "type" => element.map(|e| e.type_id.clone()),
            "stencil" => Some(shape.stencil.clone()),
            "x" => Some(data::fmt_number(shape.x)),
            "y" => Some(data::fmt_number(shape.y)),
            "width" => Some(data::fmt_number(shape.w)),
            "height" => Some(data::fmt_number(shape.h)),
            "angle" => Some(data::fmt_number(shape.angle_deg)),
            other => prop(other),
        },
    };
    Value::new(text?)
}

fn prop_text(v: &vizcraft_model::PropValue) -> String {
    match v {
        vizcraft_model::PropValue::Text(t) => t.clone(),
        vizcraft_model::PropValue::Number(n) => data::fmt_number(*n),
        vizcraft_model::PropValue::Bool(b) => b.to_string(),
    }
}

/// Whether `rule` matches `value`. Numbers compare by value when both sides are numbers; text
/// compares ignoring case (dates in `YYYY-MM-DD` order correctly as text).
pub fn matches(rule: &Rule, value: Option<&Value>) -> bool {
    use std::cmp::Ordering;
    let Some(v) = value else { return matches!(rule.op, Op::Empty | Op::Any) };
    let cmp = |other: &str| -> Ordering {
        match (v.number, other.trim().parse::<f64>().ok().filter(|n| n.is_finite())) {
            (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
            _ => v.text.trim().to_lowercase().cmp(&other.trim().to_lowercase()),
        }
    };
    match rule.op {
        Op::Any | Op::NotEmpty => true,
        Op::Empty => false,
        Op::Eq => cmp(&rule.value) == Ordering::Equal,
        Op::Ne => cmp(&rule.value) != Ordering::Equal,
        Op::Lt => cmp(&rule.value) == Ordering::Less,
        Op::Le => cmp(&rule.value) != Ordering::Greater,
        Op::Gt => cmp(&rule.value) == Ordering::Greater,
        Op::Ge => cmp(&rule.value) != Ordering::Less,
        Op::Between => cmp(&rule.value) != Ordering::Less && cmp(&rule.to) != Ordering::Greater,
        Op::Contains => v.text.to_lowercase().contains(&rule.value.to_lowercase()),
    }
}

/// `n` in `format`: the run of `#`, `0`, `,`, `.` and `%` is the pattern (decimals are the zeros
/// after the point, a comma groups thousands, `%` multiplies by 100), anything before or after it
/// is kept as is (`$#,##0.00`, `0.0 kg`). An empty format is the shortest form.
pub fn format_number(n: f64, format: &str) -> String {
    let is_pat = |c: char| matches!(c, '#' | '0' | ',' | '.' | '%');
    let Some(start) = format.find(is_pat) else {
        return if format.is_empty() { data::fmt_number(n) } else { format!("{format}{}", data::fmt_number(n)) };
    };
    let end = format.get(start..).and_then(|rest| rest.find(|c: char| !is_pat(c))).map_or(format.len(), |e| start + e);
    let (prefix, pattern, suffix) = (format.get(..start).unwrap_or(""), format.get(start..end).unwrap_or(""), format.get(end..).unwrap_or(""));
    let percent = pattern.contains('%');
    let group = pattern.contains(',');
    let decimals = pattern.split_once('.').map_or(0, |(_, d)| d.chars().filter(|c| *c == '0' || *c == '#').count()).min(10);
    let v = if percent { n * 100.0 } else { n };
    let mut body = format!("{:.*}", decimals, v.clamp(-1e300, 1e300));
    if group {
        body = group_thousands(&body);
    }
    format!("{prefix}{body}{}{suffix}", if percent { "%" } else { "" })
}

/// `1234567.5` as `1,234,567.5`.
fn group_thousands(s: &str) -> String {
    let (sign, rest) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
    let (int, frac) = rest.split_once('.').map_or((rest, None), |(i, f)| (i, Some(f)));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{out}.{f}"),
        None => format!("{sign}{out}"),
    }
}

/// Something a renderer draws, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A rectangle.
    Rect {
        /// Where.
        rect: Rect,
        /// Fill, if any.
        fill: Option<Color>,
        /// Outline colour, if any.
        line: Option<Color>,
        /// Outline width.
        width: f64,
    },
    /// A circle.
    Circle {
        /// Centre.
        center: Point,
        /// Radius.
        radius: f64,
        /// Fill.
        fill: Color,
    },
    /// A filled convex polygon.
    Polygon {
        /// Corners in order.
        points: Vec<Point>,
        /// Fill.
        fill: Color,
    },
    /// An open line with round caps.
    Polyline {
        /// Points in order.
        points: Vec<Point>,
        /// Colour.
        line: Color,
        /// Width.
        width: f64,
    },
    /// One line of text centred in a rectangle.
    Text {
        /// Where.
        rect: Rect,
        /// The text.
        text: String,
        /// Font size.
        size: f64,
        /// Colour.
        color: Color,
    },
}

/// One drawn item: its index in the graphic, its rectangle and its marks.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// Index in [`DataGraphic::items`].
    pub index: usize,
    /// What it is.
    pub kind: ItemKind,
    /// Its rectangle; for colour by value, the shape's bounds.
    pub rect: Rect,
    /// What to draw (none for colour by value).
    pub marks: Vec<Mark>,
}

/// A shape's data graphic, worked out: the colours a colour by value applies and the items.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    /// Fill from the first colour item whose rule matched.
    pub fill: Option<Color>,
    /// Line colour from that rule.
    pub line: Option<Color>,
    /// The drawn items, in graphic order.
    pub items: Vec<Placed>,
}

/// The graphic `shape` shows, if it names one the document has.
pub fn graphic_for<'a>(doc: &'a Document, shape: &Shape) -> Option<&'a DataGraphic> {
    let id = shape.data_graphic.as_deref()?;
    doc.data_graphics.iter().find(|g| g.id == id)
}

/// What one item shows for `value`, before placement: its size, and for text its words.
enum Content {
    Text(String),
    Bar(f64),
    Icon(&'static str),
    Color(Option<Color>, Option<Color>),
}

fn content(item: &DataItem, value: Option<&Value>) -> Option<Content> {
    match item.kind {
        ItemKind::Text => {
            let v = value?;
            let shown = match v.number {
                Some(n) if !item.format.is_empty() => format_number(n, &item.format),
                _ => short(&v.text, 40),
            };
            Some(Content::Text(if item.label.is_empty() { shown } else { format!("{}: {shown}", item.label) }))
        }
        ItemKind::Bar => {
            let n = value?.number?;
            let span = item.max - item.min;
            let f = if span > 0.0 { ((n - item.min) / span).clamp(0.0, 1.0) } else { 0.0 };
            Some(Content::Bar(f))
        }
        ItemKind::Icons => {
            let rule = item.rules.iter().find(|r| matches(r, value))?;
            ICONS.iter().find(|i| **i == rule.icon).map(|i| Content::Icon(i))
        }
        ItemKind::Color => {
            let rule = item.rules.iter().find(|r| matches(r, value))?;
            Some(Content::Color(rule.fill, rule.line))
        }
    }
}

/// At most `n` chars of `s`.
fn short(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().nth(n).is_some() {
        out.push_str("...");
    }
    out
}

/// Width a line of callout text takes: an estimate shared by canvas and SVG, so the box is the
/// same in both.
pub fn text_width(text: &str) -> f64 {
    (text.chars().count() as f64 * TEXT_SIZE * 0.58 + 8.0).max(24.0)
}

/// The size an item's content takes beside a shape of bounds `b`.
fn size(c: &Content, position: Position, b: &Rect) -> (f64, f64) {
    match c {
        Content::Text(t) => (text_width(t), TEXT_H),
        Content::Bar(_) => match position {
            Position::Left | Position::Right => (BAR_SIDE_W, BAR_H),
            _ => (b.width().clamp(40.0, 160.0), BAR_H),
        },
        Content::Icon(_) => (ICON, ICON),
        Content::Color(..) => (b.width(), b.height()),
    }
}

/// Each drawn item's index, kind and rectangle around `shape`: outside items stack away from
/// the shape, centre items stack down from the middle and corner items stack inwards. Items with
/// nothing to show (no value, no matching rule) take no room. Colour by value takes the
/// shape's bounds.
pub fn layout(doc: &Document, page: &Page, shape: &Shape, graphic: &DataGraphic) -> Vec<(usize, ItemKind, Rect)> {
    placed(doc, page, shape, graphic).into_iter().map(|(i, k, r, _)| (i, k, r)).collect()
}

fn placed(doc: &Document, page: &Page, shape: &Shape, graphic: &DataGraphic) -> Vec<(usize, ItemKind, Rect, Content)> {
    let b = shape.bounds();
    let mut used = [0.0f64; 9];
    let mut out = Vec::new();
    for (i, item) in graphic.items.iter().enumerate() {
        let value = value_of(doc, page, shape, item);
        let Some(c) = content(item, value.as_ref()) else { continue };
        if let Content::Color(..) = c {
            out.push((i, item.kind, b, c));
            continue;
        }
        let (w, h) = size(&c, item.position, &b);
        let slot = Position::ALL.iter().position(|p| *p == item.position).unwrap_or(1);
        let at = used.get(slot).copied().unwrap_or(0.0);
        let (cx, cy) = (b.center().x, b.center().y);
        let rect = match item.position {
            Position::Top => Rect::from_xywh(cx - w / 2.0, b.y0 - GAP - at - h, w, h),
            Position::Bottom => Rect::from_xywh(cx - w / 2.0, b.y1 + GAP + at, w, h),
            Position::Left => Rect::from_xywh(b.x0 - GAP - at - w, cy - h / 2.0, w, h),
            Position::Right => Rect::from_xywh(b.x1 + GAP + at, cy - h / 2.0, w, h),
            Position::Center => Rect::from_xywh(cx - w / 2.0, cy - h / 2.0 + at, w, h),
            Position::TopLeft => Rect::from_xywh(b.x0 + 2.0 + at, b.y0 + 2.0, w, h),
            Position::TopRight => Rect::from_xywh(b.x1 - 2.0 - at - w, b.y0 + 2.0, w, h),
            Position::BottomLeft => Rect::from_xywh(b.x0 + 2.0 + at, b.y1 - 2.0 - h, w, h),
            Position::BottomRight => Rect::from_xywh(b.x1 - 2.0 - at - w, b.y1 - 2.0 - h, w, h),
        };
        let step = match item.position {
            Position::Left | Position::Right | Position::TopLeft | Position::TopRight | Position::BottomLeft | Position::BottomRight => w,
            _ => h,
        };
        if let Some(u) = used.get_mut(slot) {
            *u += step + GAP;
        }
        out.push((i, item.kind, rect, c));
    }
    out
}

/// `shape`'s data graphic, worked out: `None` when it shows none.
pub fn render(doc: &Document, page: &Page, shape: &Shape) -> Option<Rendered> {
    let g = graphic_for(doc, shape)?;
    let mut r = Rendered::default();
    for (index, kind, rect, c) in placed(doc, page, shape, g) {
        let marks = match c {
            Content::Text(text) => vec![
                Mark::Rect { rect, fill: Some(CHROME_FILL), line: Some(CHROME_LINE), width: 0.75 },
                Mark::Text { rect, text, size: TEXT_SIZE, color: TEXT_COLOR },
            ],
            Content::Bar(f) => {
                let color = g.items.get(index).map_or(Color::BLACK, |i| i.color);
                let mut m = vec![Mark::Rect { rect, fill: Some(BAR_TRACK), line: None, width: 0.0 }];
                if f > 0.0 {
                    m.push(Mark::Rect {
                        rect: Rect::from_xywh(rect.x0, rect.y0, rect.width() * f, rect.height()),
                        fill: Some(color),
                        line: None,
                        width: 0.0,
                    });
                }
                m.push(Mark::Rect { rect, fill: None, line: Some(CHROME_LINE), width: 0.75 });
                m
            }
            Content::Icon(name) => icon_marks(name, rect),
            Content::Color(fill, line) => {
                if r.fill.is_none() && r.line.is_none() {
                    r.fill = fill;
                    r.line = line;
                }
                Vec::new()
            }
        };
        r.items.push(Placed { index, kind, rect, marks });
    }
    Some(r)
}

/// `shape` with its colour by value applied, for drawing only; the shape itself is unchanged.
pub fn recolored<'a>(shape: &'a Shape, r: &Rendered) -> Cow<'a, Shape> {
    if r.fill.is_none() && r.line.is_none() {
        return Cow::Borrowed(shape);
    }
    let mut s = shape.clone();
    if let Some(f) = r.fill {
        s.style.fill = f;
        s.style.gradient = None;
    }
    if let Some(l) = r.line {
        s.style.line = l;
    }
    Cow::Owned(s)
}

/// The main colour of an icon, for legend swatches.
pub fn icon_color(name: &str) -> Color {
    match name {
        "check" | "plus" | "chevronUp" => Color::rgb(0x16, 0xa3, 0x4a),
        "close" | "chevronDown" => Color::rgb(0xdc, 0x26, 0x26),
        "minus" => Color::rgb(0xf5, 0x9e, 0x0b),
        _ => Color::rgb(0x25, 0x63, 0xeb),
    }
}

/// The marks of icon `name` in `r` (a square): a coloured disc with a white glyph, or a
/// coloured arrow head for the chevrons.
pub fn icon_marks(name: &str, r: Rect) -> Vec<Mark> {
    let c = icon_color(name);
    let s = r.width().min(r.height());
    let o = Point::new(r.center().x - s / 2.0, r.center().y - s / 2.0);
    // Points on a 16 by 16 grid.
    let p = |x: f64, y: f64| Point::new(o.x + x * s / 16.0, o.y + y * s / 16.0);
    let w = 2.0 * s / 16.0;
    let disc = Mark::Circle { center: r.center(), radius: s / 2.0, fill: c };
    let white = Color::WHITE;
    let line = |pts: &[(f64, f64)]| Mark::Polyline { points: pts.iter().map(|&(x, y)| p(x, y)).collect(), line: white, width: w };
    match name {
        "check" => vec![disc, line(&[(4.5, 8.5), (7.0, 11.0), (11.5, 5.5)])],
        "close" => vec![disc, line(&[(5.0, 5.0), (11.0, 11.0)]), line(&[(11.0, 5.0), (5.0, 11.0)])],
        "plus" => vec![disc, line(&[(8.0, 4.5), (8.0, 11.5)]), line(&[(4.5, 8.0), (11.5, 8.0)])],
        "minus" => vec![disc, line(&[(4.5, 8.0), (11.5, 8.0)])],
        "chevronUp" => vec![Mark::Polygon { points: vec![p(8.0, 2.0), p(15.0, 13.0), p(1.0, 13.0)], fill: c }],
        "chevronDown" => vec![Mark::Polygon { points: vec![p(1.0, 3.0), p(15.0, 3.0), p(8.0, 14.0)], fill: c }],
        _ => vec![disc, line(&[(8.0, 7.0), (8.0, 12.0)]), Mark::Circle { center: p(8.0, 4.5), radius: 1.25 * s / 16.0, fill: white }],
    }
}

/// One row of a legend: a swatch colour, the icon when the rule shows one, and the words.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendRow {
    /// Swatch fill.
    pub swatch: Color,
    /// The icon an icon-set rule shows.
    pub icon: Option<String>,
    /// What the row says.
    pub label: String,
}

/// The legend of `g`: one row per rule of its icon sets and colours by value.
pub fn legend_rows(g: &DataGraphic) -> Vec<LegendRow> {
    let mut out = Vec::new();
    for item in g.items.iter().filter(|i| matches!(i.kind, ItemKind::Icons | ItemKind::Color)) {
        for rule in &item.rules {
            let label = if rule.label.is_empty() { rule_text(&item.key, rule) } else { rule.label.clone() };
            let (swatch, icon) = match item.kind {
                ItemKind::Icons => (icon_color(&rule.icon), Some(rule.icon.clone()).filter(|i| !i.is_empty())),
                _ => (rule.fill.or(rule.line).unwrap_or(Color::WHITE), None),
            };
            out.push(LegendRow { swatch, icon, label });
        }
    }
    out
}

/// A rule in words: `cost > 10`, `cost from 1 to 5`, `status otherwise`.
fn rule_text(key: &str, r: &Rule) -> String {
    match r.op {
        Op::Between => format!("{key} from {} to {}", r.value, r.to),
        Op::Empty | Op::NotEmpty | Op::Any => format!("{key} {}", r.op.symbol()),
        op => format!("{key} {} {}", op.symbol(), r.value),
    }
}

/// The data type a value of `text` most likely has, for defining page keys from a table column.
pub fn infer_text_type(text: &str) -> DataType {
    let t = text.trim();
    if t.parse::<f64>().is_ok_and(f64::is_finite) {
        DataType::Number
    } else if data::normalize_text(DataType::Bool, &[], "", t).is_ok() && !t.is_empty() {
        DataType::Bool
    } else if data::normalize_text(DataType::Date, &[], "", t).is_ok() && !t.is_empty() {
        DataType::Date
    } else {
        DataType::String
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShapeKind;
    use crate::data::DataRow;

    fn doc_with(items: Vec<DataItem>, value: &str) -> Document {
        let mut doc = Document::new();
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 100.0, 100.0, 120.0, 60.0);
        s.data.rows.push(DataRow { key: "cost".into(), kind: DataType::Number, value: value.into(), ..DataRow::default() });
        s.data_graphic = Some("g1".into());
        if let Some(p) = doc.page_mut() {
            p.shapes.push(s);
        }
        doc.data_graphics.push(DataGraphic { id: "g1".into(), name: "Cost".into(), items });
        doc
    }

    fn rule(op: Op, value: &str) -> Rule {
        Rule { op, value: value.into(), ..Rule::default() }
    }

    #[test]
    fn rules_compare_numbers_by_value_and_text_ignoring_case() {
        let v = |t: &str| Value::new(t.into());
        assert!(matches(&rule(Op::Gt, "9"), v("10").as_ref()), "10 > 9 as numbers, not as text");
        assert!(matches(&rule(Op::Eq, "done"), v("Done").as_ref()));
        assert!(matches(&Rule { to: "20".into(), ..rule(Op::Between, "10") }, v("15").as_ref()));
        assert!(!matches(&Rule { to: "20".into(), ..rule(Op::Between, "10") }, v("25").as_ref()));
        assert!(matches(&rule(Op::Empty, ""), None));
        assert!(!matches(&rule(Op::Eq, ""), None));
        assert!(matches(&rule(Op::Any, ""), v("x").as_ref()));
        assert_eq!(Op::parse(">="), Some(Op::Ge));
    }

    #[test]
    fn number_formats() {
        assert_eq!(format_number(1234.5, ""), "1234.5");
        assert_eq!(format_number(1234.5, "0"), "1234");
        assert_eq!(format_number(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(format_number(0.125, "0.0%"), "12.5%");
        assert_eq!(format_number(-1234567.0, "$#,##0"), "$-1,234,567");
        assert_eq!(format_number(3.0, "0.0 kg"), "3.0 kg");
    }

    #[test]
    fn layout_stacks_items_and_skips_ones_with_nothing_to_show() {
        let items = vec![
            DataItem { kind: ItemKind::Text, key: "cost".into(), position: Position::Bottom, ..DataItem::default() },
            DataItem { kind: ItemKind::Bar, key: "cost".into(), position: Position::Bottom, ..DataItem::default() },
            DataItem { kind: ItemKind::Text, key: "missing".into(), position: Position::Top, ..DataItem::default() },
        ];
        let doc = doc_with(items, "50");
        let page = doc.page().unwrap();
        let s = page.shape("s1").unwrap();
        let l = layout(&doc, page, s, &doc.data_graphics[0]);
        assert_eq!(l.len(), 2, "the missing key draws nothing");
        assert_eq!(l[0].2.y0, 160.0 + GAP);
        assert_eq!(l[1].2.y0, l[0].2.y1 + GAP, "the bar stacks under the callout");
        let r = render(&doc, page, s).unwrap();
        let Mark::Rect { rect, .. } = &r.items[1].marks[1] else { panic!("bar fill") };
        assert!((rect.width() - l[1].2.width() * 0.5).abs() < 1e-9, "half full at 50 of 0 to 100");
    }

    #[test]
    fn colour_by_value_recolours_a_copy() {
        let items = vec![DataItem {
            kind: ItemKind::Color,
            key: "cost".into(),
            rules: vec![
                Rule { fill: Some(Color::rgb(255, 0, 0)), ..rule(Op::Gt, "10") },
                Rule { fill: Some(Color::rgb(0, 255, 0)), ..rule(Op::Any, "") },
            ],
            ..DataItem::default()
        }];
        let doc = doc_with(items, "50");
        let page = doc.page().unwrap();
        let s = page.shape("s1").unwrap();
        let r = render(&doc, page, s).unwrap();
        assert_eq!(r.fill, Some(Color::rgb(255, 0, 0)));
        assert_eq!(recolored(s, &r).style.fill, Color::rgb(255, 0, 0));
        assert_eq!(s.style.fill, Shape::default().style.fill);
        assert_eq!(legend_rows(&doc.data_graphics[0]).len(), 2);
    }

    #[test]
    fn sanitising_drops_bad_ids_and_dangling_references() {
        let mut doc = doc_with(vec![DataItem { min: f64::NAN, max: -5.0, ..DataItem::default() }], "1");
        doc.data_graphics.push(DataGraphic { id: "bad id".into(), ..DataGraphic::default() });
        doc.data_graphics.push(DataGraphic { id: "g1".into(), ..DataGraphic::default() });
        doc.data_links.push(DataLink { id: "".into(), ..DataLink::default() });
        if let Some(p) = doc.page_mut() {
            let mut s = Shape::new("s2", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
            s.data_graphic = Some("nope".into());
            p.shapes.push(s);
        }
        sanitize_doc(&mut doc);
        assert_eq!(doc.data_graphics.len(), 1);
        assert!(doc.data_links.is_empty());
        let item = &doc.data_graphics[0].items[0];
        assert!(item.min < item.max);
        assert_eq!(doc.page().unwrap().shape("s2").unwrap().data_graphic, None);
        assert_eq!(doc.page().unwrap().shape("s1").unwrap().data_graphic.as_deref(), Some("g1"));
    }
}
