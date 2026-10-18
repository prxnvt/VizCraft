//! Containers, swimlanes and callouts: shapes that carry a [`Container`] in `Shape::container`.
//!
//! - A **container** draws itself (its own fill and outline, plus a heading band) and holds
//!   members, which name it as their `parent` like group members do. Unlike a group, a click on a
//!   member selects the member ([`crate::arrange::group_chain`] stops at a container), the frame is
//!   refitted only when asked ([`fit`]) or when `auto_fit` is on, and membership changes as shapes
//!   are dropped in or dragged fully out ([`drop_changes`]). A locked container refuses
//!   membership changes. Moving a container moves its members (the arrange code carries
//!   descendants); resizing it does not scale them.
//! - A **lane set** is a container of **lanes**, each a container stacked along the set
//!   ([`layout_lanes`]): horizontal lanes are bands stacked top to bottom with their heading on
//!   the left, vertical lanes are columns with their heading on top. Lanes keep their members when
//!   they are added, removed, reordered or resized, since a lane moves with its members. The set
//!   may carry phase separators across the lanes ([`Phase`]).
//! - A **callout** is a text shape aimed at a target shape; its leader line ([`leader`]) is worked
//!   out from both shapes each time it is drawn, so it follows either when it moves.
//!
//! The heading is the shape's own text, placed in the heading band through `Shape::text_block`
//! ([`refresh`]), so the text editor, the theme and every exporter treat it like any text. The
//! extra drawing (the band, separator lines, phases and leaders) is described once here as
//! [`Decor`] items for the canvas and SVG export to draw the same way.
//!
//! Nothing here panics: ids that name nothing are skipped, parent chains are walked at most
//! [`arrange::MAX_GROUP_DEPTH`] steps and coordinates are clamped.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::arrange;
use crate::geom::{MAX_COORD, Point, Rect, finite};
use crate::model::{Color, MAX_ID, Page, Shape, truncate};
use crate::serde_util::{lenient, lenient_true, lenient_vec, named_enum, num};
use crate::shape_fields::TextBlock;
use crate::style_fields::Dash;
use crate::{MAX_EXTENT, ShapeKind};

/// The `stencil` of a container shape (informational; the [`Container`] settings decide).
pub const CONTAINER_STENCIL: &str = "vizcraft.container";
/// The `stencil` of a lane set.
pub const LANE_SET_STENCIL: &str = "vizcraft.laneSet";
/// The `stencil` of a lane.
pub const LANE_STENCIL: &str = "vizcraft.lane";
/// The `stencil` of a callout.
pub const CALLOUT_STENCIL: &str = "vizcraft.callout";

/// Thickness of a lane set's title band, in world units.
pub const SET_TITLE: f64 = 32.0;
/// Thickness of a lane's heading band.
pub const LANE_HEADING: f64 = 36.0;
/// Thickness of the band that holds phase titles, when a lane set has phases.
pub const PHASE_BAND: f64 = 24.0;
/// Thickness of a new lane.
pub const LANE_THICKNESS: f64 = 140.0;
/// Thinnest a lane gets.
pub const MIN_LANE: f64 = 40.0;
/// Length of a new lane set (along its lanes).
pub const LANE_LENGTH: f64 = 760.0;
/// Most lanes one set holds.
pub const MAX_LANES: usize = 64;
/// Most phases one set holds.
pub const MAX_PHASES: usize = 64;
/// Longest phase title kept, in bytes.
pub const MAX_PHASE_TITLE: usize = 256;
/// Size of a new container placed without members.
pub const DEFAULT_SIZE: (f64, f64) = (320.0, 220.0);

/// What a shape with container settings is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    /// A container that holds members. Unknown names load as this.
    #[default]
    Container,
    /// A set of lanes.
    LaneSet,
    /// One lane of a lane set.
    Lane,
    /// A callout aimed at a target shape.
    Callout,
}

named_enum!(Role, Role::Container, {
    Role::Container => "container",
    Role::LaneSet => "laneSet",
    Role::Lane => "lane",
    Role::Callout => "callout",
});

/// Which edge of a container its heading band runs along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeadingPosition {
    /// Along the top edge. Unknown names load as this.
    #[default]
    Top,
    /// Along the bottom edge.
    Bottom,
    /// Down the left edge, text turned to read upwards.
    Left,
    /// Down the right edge, text turned to read downwards.
    Right,
}

named_enum!(HeadingPosition, HeadingPosition::Top, {
    HeadingPosition::Top => "top",
    HeadingPosition::Bottom => "bottom",
    HeadingPosition::Left => "left",
    HeadingPosition::Right => "right",
});

/// How a container's heading is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeadingStyle {
    /// A filled band. Unknown names load as this.
    #[default]
    Band,
    /// The text over a separator line.
    Line,
    /// The text alone.
    Plain,
}

named_enum!(HeadingStyle, HeadingStyle::Band, {
    HeadingStyle::Band => "band",
    HeadingStyle::Line => "line",
    HeadingStyle::Plain => "plain",
});

/// Which side of its target a callout's leader meets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// The side facing the callout. Unknown names load as this.
    #[default]
    Auto,
    /// The middle of the top edge.
    Top,
    /// The middle of the bottom edge.
    Bottom,
    /// The middle of the left edge.
    Left,
    /// The middle of the right edge.
    Right,
}

named_enum!(Side, Side::Auto, {
    Side::Auto => "auto",
    Side::Top => "top",
    Side::Bottom => "bottom",
    Side::Left => "left",
    Side::Right => "right",
});

/// A phase separator across a lane set's lanes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Phase {
    /// Distance of the separator from the set's leading edge (left for horizontal lanes, top for
    /// vertical ones), in world units.
    #[serde(deserialize_with = "num")]
    pub at: f64,
    /// The title of the phase that starts at the separator.
    #[serde(deserialize_with = "lenient")]
    pub title: String,
}

/// Container, lane or callout settings, stored on the shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Container {
    /// What the shape is.
    #[serde(deserialize_with = "lenient")]
    pub role: Role,
    /// Space [`fit`] leaves between the members and the frame (and the heading), world units.
    #[serde(deserialize_with = "num")]
    pub margin: f64,
    /// Which edge the heading runs along.
    #[serde(deserialize_with = "lenient")]
    pub heading_position: HeadingPosition,
    /// How the heading is drawn.
    #[serde(deserialize_with = "lenient")]
    pub heading_style: HeadingStyle,
    /// Thickness of the heading band; 0 leaves no heading.
    #[serde(deserialize_with = "num")]
    pub heading_size: f64,
    /// Fill of the heading band (and of a lane set's phase band, lightened).
    #[serde(deserialize_with = "lenient")]
    pub band: Color,
    /// The style preset last applied (`classic`, `note`, ...); informational.
    #[serde(deserialize_with = "lenient")]
    pub style: String,
    /// The frame follows the members: refitted after every change.
    #[serde(deserialize_with = "lenient")]
    pub auto_fit: bool,
    /// Membership is fixed: nothing is added or removed by drops or commands.
    #[serde(deserialize_with = "lenient")]
    pub locked: bool,
    /// A lane set or lane runs top to bottom (lanes are columns) instead of left to right.
    #[serde(deserialize_with = "lenient")]
    pub vertical: bool,
    /// A lane set's phase separators, at most [`MAX_PHASES`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<Phase>,
    /// A callout's target shape, on the same page.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// A callout draws its leader line.
    #[serde(deserialize_with = "lenient_true")]
    pub leader: bool,
    /// Which side of the target a callout's leader meets.
    #[serde(deserialize_with = "lenient")]
    pub side: Side,
}

impl Default for Container {
    fn default() -> Self {
        Container {
            role: Role::Container,
            margin: 20.0,
            heading_position: HeadingPosition::Top,
            heading_style: HeadingStyle::Band,
            heading_size: 28.0,
            band: Color::rgb(0xdf, 0xe6, 0xee),
            style: String::new(),
            auto_fit: false,
            locked: false,
            vertical: false,
            phases: Vec::new(),
            target: None,
            leader: true,
            side: Side::Auto,
        }
    }
}

impl Container {
    /// Largest margin or heading size kept.
    pub const MAX_BAND: f64 = 1000.0;

    /// Clamp the numbers, cap the lists and strings.
    pub fn sanitize(&mut self) {
        self.margin = finite(self.margin, 20.0).clamp(0.0, Self::MAX_BAND);
        self.heading_size = finite(self.heading_size, 28.0).clamp(0.0, Self::MAX_BAND);
        truncate(&mut self.style, 32);
        self.phases.truncate(MAX_PHASES);
        for p in &mut self.phases {
            p.at = finite(p.at, 0.0).clamp(0.0, MAX_EXTENT);
            truncate(&mut p.title, MAX_PHASE_TITLE);
        }
        self.phases.sort_by(|a, b| a.at.total_cmp(&b.at));
        if let Some(t) = &mut self.target {
            truncate(t, MAX_ID);
        }
    }

    /// Whether shapes can be members of this one (containers and lanes; lane sets hold lanes).
    pub fn holds_members(&self) -> bool {
        matches!(self.role, Role::Container | Role::Lane)
    }

    /// Whether this is a box that draws a frame and heading (anything but a callout).
    pub fn is_box(&self) -> bool {
        self.role != Role::Callout
    }
}

/// The settings of `s`, if it is a container, lane set, lane or callout.
pub fn settings(s: &Shape) -> Option<&Container> {
    s.container.as_ref()
}

/// The role of `s`, if it has container settings.
pub fn role(s: &Shape) -> Option<Role> {
    s.container.as_ref().map(|c| c.role)
}

/// Whether `s` is a container, lane set or lane (a box that may have members).
pub fn is_container(s: &Shape) -> bool {
    s.container.as_ref().is_some_and(Container::is_box)
}

/// Whether `s` is a callout.
pub fn is_callout(s: &Shape) -> bool {
    role(s) == Some(Role::Callout)
}

/// Whether `s` is a locked container.
pub fn is_locked(s: &Shape) -> bool {
    s.container.as_ref().is_some_and(|c| c.is_box() && c.locked)
}

// ---------------------------------------------------------------------------------------------
// Style presets

/// A named look for a container or callout: colours, outline and heading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    /// The name used in command params.
    pub name: &'static str,
    /// The name shown in menus.
    pub label: &'static str,
    /// Body fill.
    pub fill: Color,
    /// Outline colour.
    pub line: Color,
    /// Heading band fill.
    pub band: Color,
    /// Heading or text colour.
    pub font: Color,
    /// Outline dash pattern.
    pub dash: Dash,
    /// Corner rounding.
    pub corner: f64,
    /// Heading drawing.
    pub heading: HeadingStyle,
    /// Bold heading text.
    pub bold: bool,
    /// The outline (callouts: `Ellipse` for a bubble).
    pub kind: ShapeKind,
}

const fn preset(name: &'static str, label: &'static str, fill: Color, line: Color, band: Color, font: Color) -> Preset {
    Preset {
        name,
        label,
        fill,
        line,
        band,
        font,
        dash: Dash::Solid,
        corner: 0.0,
        heading: HeadingStyle::Band,
        bold: false,
        kind: ShapeKind::Rectangle,
    }
}

/// The container styles, in gallery order. The first is the default.
pub const CONTAINER_STYLES: &[Preset] = &[
    preset(
        "classic",
        "Classic",
        Color::rgb(0xf7, 0xf9, 0xfc),
        Color::rgb(0x5b, 0x6b, 0x7d),
        Color::rgb(0xdf, 0xe6, 0xee),
        Color::rgb(0x1e, 0x26, 0x30),
    ),
    Preset {
        heading: HeadingStyle::Line,
        ..preset("outline", "Outline", Color::TRANSPARENT, Color::rgb(0x6d, 0x7a, 0x88), Color::TRANSPARENT, Color::rgb(0x2b, 0x34, 0x40))
    },
    Preset {
        bold: true,
        ..preset("slate", "Slate", Color::rgb(0xee, 0xf1, 0xf5), Color::rgb(0x33, 0x47, 0x5b), Color::rgb(0x33, 0x47, 0x5b), Color::WHITE)
    },
    Preset {
        corner: 10.0,
        ..preset(
            "sage",
            "Sage",
            Color::rgb(0xf1, 0xf6, 0xef),
            Color::rgb(0x6f, 0x8f, 0x62),
            Color::rgb(0xd5, 0xe5, 0xcd),
            Color::rgb(0x2c, 0x3d, 0x26),
        )
    },
    Preset {
        dash: Dash::Dashed,
        heading: HeadingStyle::Plain,
        ..preset("dashed", "Dashed", Color::TRANSPARENT, Color::rgb(0x7d, 0x87, 0x94), Color::TRANSPARENT, Color::rgb(0x3a, 0x44, 0x50))
    },
    Preset {
        bold: true,
        ..preset(
            "amber",
            "Amber",
            Color::rgb(0xff, 0xfa, 0xf0),
            Color::rgb(0xb7, 0x86, 0x2f),
            Color::rgb(0xf3, 0xdf, 0xb5),
            Color::rgb(0x4a, 0x35, 0x10),
        )
    },
];

/// The callout styles, in menu order. The first is the default.
pub const CALLOUT_STYLES: &[Preset] = &[
    preset("box", "Box", Color::WHITE, Color::rgb(0x5b, 0x6b, 0x7d), Color::TRANSPARENT, Color::rgb(0x1e, 0x26, 0x30)),
    preset("plain", "Text Only", Color::TRANSPARENT, Color::TRANSPARENT, Color::TRANSPARENT, Color::rgb(0x1e, 0x26, 0x30)),
    Preset {
        corner: 10.0,
        ..preset("rounded", "Rounded", Color::rgb(0xf4, 0xf7, 0xfb), Color::rgb(0x6d, 0x7a, 0x88), Color::TRANSPARENT, Color::rgb(0x1e, 0x26, 0x30))
    },
    preset("note", "Note", Color::rgb(0xff, 0xf6, 0xc8), Color::rgb(0xc9, 0xa9, 0x3a), Color::TRANSPARENT, Color::rgb(0x3d, 0x32, 0x0c)),
    Preset {
        kind: ShapeKind::Ellipse,
        ..preset("bubble", "Bubble", Color::WHITE, Color::rgb(0x4f, 0x6f, 0x96), Color::TRANSPARENT, Color::rgb(0x1e, 0x26, 0x30))
    },
];

/// The container style named `name`.
pub fn container_style(name: &str) -> Option<&'static Preset> {
    CONTAINER_STYLES.iter().find(|p| p.name == name)
}

/// The callout style named `name`.
pub fn callout_style(name: &str) -> Option<&'static Preset> {
    CALLOUT_STYLES.iter().find(|p| p.name == name)
}

/// Give `s` the look of preset `p`: fill, outline, text colour, heading band and style.
pub fn apply_preset(s: &mut Shape, p: &Preset) {
    s.style.fill = p.fill;
    s.style.line = p.line;
    s.style.font_color = p.font;
    s.style.dash = p.dash;
    s.style.corner_radius = p.corner;
    s.style.bold = p.bold;
    if let Some(c) = &mut s.container {
        c.band = p.band;
        c.style = p.name.to_string();
        if c.is_box() {
            c.heading_style = p.heading;
        } else {
            s.kind = p.kind;
            s.connection_points = p.kind.connection_points();
        }
    }
    s.sanitize();
}

// ---------------------------------------------------------------------------------------------
// New shapes

fn new_box(id: impl Into<String>, stencil: &str, c: Container, x: f64, y: f64, w: f64, h: f64) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, w, h);
    s.stencil = stencil.to_string();
    s.behavior.container = true;
    s.container = Some(c);
    s
}

/// A new container at `x, y`, `w` by `h`, headed `heading`, in the style `preset` (the default
/// style when it names none).
pub fn new_container(id: impl Into<String>, x: f64, y: f64, w: f64, h: f64, heading: &str) -> Shape {
    let mut s = new_box(id, CONTAINER_STENCIL, Container::default(), x, y, w, h);
    s.text = heading.to_string();
    if let Some(p) = CONTAINER_STYLES.first() {
        apply_preset(&mut s, p);
    }
    refresh(&mut s);
    s
}

/// A new, empty lane set at `x, y` with lanes running left to right (or top to bottom when
/// `vertical`). Its frame is set by [`layout_lanes`] once lanes are added.
pub fn new_lane_set(id: impl Into<String>, x: f64, y: f64, vertical: bool, title: &str) -> Shape {
    let c = Container { role: Role::LaneSet, heading_size: SET_TITLE, vertical, margin: 0.0, ..Container::default() };
    let (w, h) = if vertical { (MIN_LANE, LANE_LENGTH) } else { (LANE_LENGTH, SET_TITLE + MIN_LANE) };
    let mut s = new_box(id, LANE_SET_STENCIL, c, x, y, w, h);
    s.text = title.to_string();
    if let Some(p) = container_style("slate") {
        apply_preset(&mut s, p);
    }
    refresh(&mut s);
    s
}

/// A new lane titled `title`, for a set that runs as `vertical` says. [`layout_lanes`] places it.
pub fn new_lane(id: impl Into<String>, title: &str, vertical: bool) -> Shape {
    let heading_position = if vertical { HeadingPosition::Top } else { HeadingPosition::Left };
    let c = Container { role: Role::Lane, heading_size: LANE_HEADING, heading_position, vertical, margin: 10.0, ..Container::default() };
    let (w, h) = if vertical { (LANE_THICKNESS, LANE_LENGTH) } else { (LANE_LENGTH, LANE_THICKNESS) };
    let mut s = new_box(id, LANE_STENCIL, c, 0.0, 0.0, w, h);
    s.text = title.to_string();
    if let Some(p) = CONTAINER_STYLES.first() {
        apply_preset(&mut s, p);
    }
    // Lanes leave the set's fill (and its phase separators) showing through.
    s.style.fill = Color::TRANSPARENT;
    refresh(&mut s);
    s
}

/// A new callout aimed at `target`, with its top-left at `x, y`.
pub fn new_callout(id: impl Into<String>, target: &str, text: &str, x: f64, y: f64) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, 140.0, 48.0);
    s.stencil = CALLOUT_STENCIL.to_string();
    s.text = text.to_string();
    s.container = Some(Container { role: Role::Callout, target: Some(target.to_string()), heading_size: 0.0, margin: 0.0, ..Container::default() });
    if let Some(p) = CALLOUT_STYLES.first() {
        apply_preset(&mut s, p);
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Heading geometry

/// The heading band of `s` in its unrotated local frame (`(0, 0)` to `(w, h)`), if it has one.
pub fn heading_rect(s: &Shape) -> Option<Rect> {
    let c = s.container.as_ref().filter(|c| c.is_box())?;
    let (w, h) = (s.w, s.h);
    let horizontal = matches!(c.heading_position, HeadingPosition::Top | HeadingPosition::Bottom);
    let t = c.heading_size.min(if horizontal { h } else { w });
    if t <= 0.0 {
        return None;
    }
    Some(match c.heading_position {
        HeadingPosition::Top => Rect::new(0.0, 0.0, w, t),
        HeadingPosition::Bottom => Rect::new(0.0, h - t, w, h),
        HeadingPosition::Left => Rect::new(0.0, 0.0, t, h),
        HeadingPosition::Right => Rect::new(w - t, 0.0, w, h),
    })
}

/// Put the text block of `s` over its heading band (turned for a side heading), so the heading
/// is the shape's own text. Shapes without a heading keep their text in the frame.
pub fn refresh(s: &mut Shape) {
    let Some(c) = s.container.as_ref() else { return };
    if !c.is_box() {
        return;
    }
    let pos = c.heading_position;
    s.text_block = heading_rect(s).map(|r| {
        let (dx, dy) = (r.center().x - s.w / 2.0, r.center().y - s.h / 2.0);
        match pos {
            HeadingPosition::Top | HeadingPosition::Bottom => TextBlock { dx, dy, w: r.width(), h: r.height(), angle_deg: 0.0 },
            HeadingPosition::Left => TextBlock { dx, dy, w: r.height(), h: r.width(), angle_deg: 270.0 },
            HeadingPosition::Right => TextBlock { dx, dy, w: r.height(), h: r.width(), angle_deg: 90.0 },
        }
    });
}

/// Where members sit inside `s`, in world space: the frame less the heading band.
pub fn content_rect(s: &Shape) -> Rect {
    let f = s.frame();
    let Some(c) = s.container.as_ref() else { return f };
    let t = heading_rect(s).map_or(0.0, |r| r.width().min(r.height()));
    match c.heading_position {
        HeadingPosition::Top => Rect::new(f.x0, f.y0 + t, f.x1, f.y1),
        HeadingPosition::Bottom => Rect::new(f.x0, f.y0, f.x1, f.y1 - t),
        HeadingPosition::Left => Rect::new(f.x0 + t, f.y0, f.x1, f.y1),
        HeadingPosition::Right => Rect::new(f.x0, f.y0, f.x1 - t, f.y1),
    }
}

// ---------------------------------------------------------------------------------------------
// Decorations

/// One piece of a container's or callout's own drawing, in world space, beyond its outline and
/// text.
#[derive(Clone, Debug, PartialEq)]
pub enum Decor {
    /// A filled polygon.
    Fill {
        /// The corners.
        points: Vec<Point>,
        /// The fill.
        color: Color,
    },
    /// An open polyline.
    Line {
        /// The points.
        points: Vec<Point>,
        /// The stroke colour.
        color: Color,
        /// The stroke width.
        width: f64,
        /// Drawn dashed.
        dashed: bool,
    },
    /// One line of text centred on `center`, turned by `angle` degrees clockwise.
    Text {
        /// Centre of the text.
        center: Point,
        /// The room it has along its baseline (longer text is cut).
        room: f64,
        /// Rotation, degrees clockwise.
        angle: f64,
        /// The text.
        text: String,
        /// Font size.
        size: f64,
        /// Colour.
        color: Color,
        /// Semibold.
        bold: bool,
    },
}

/// `c` mixed halfway towards white (alpha kept).
fn lighten(c: Color) -> Color {
    let mix = |v: u8| ((u16::from(v) + 255) / 2) as u8;
    Color { r: mix(c.r), g: mix(c.g), b: mix(c.b), a: c.a }
}

/// The outline colour a decoration uses: the shape's line, or a slate grey when it has none.
fn line_of(s: &Shape) -> Color {
    let l = s.style.effective_line();
    if l.a == 0 { Color::rgb(0x6d, 0x7a, 0x88) } else { l }
}

/// What `s` draws between its outline and its text: the heading band or separator, and a lane
/// set's phase band, separators and phase titles. Empty for other shapes.
pub fn decor(s: &Shape) -> Vec<Decor> {
    let mut out = Vec::new();
    let Some(c) = s.container.as_ref().filter(|c| c.is_box()) else { return out };
    let t = s.transform();
    let world = |pts: &[(f64, f64)]| pts.iter().map(|(x, y)| t.apply(Point::new(*x, *y))).collect::<Vec<_>>();
    let lw = s.style.line_width.clamp(0.0, 100.0);
    let line = line_of(s);
    if let Some(r) = heading_rect(s) {
        // The inner edge of the band: the separator between heading and content.
        let edge = match c.heading_position {
            HeadingPosition::Top => [(r.x0, r.y1), (r.x1, r.y1)],
            HeadingPosition::Bottom => [(r.x0, r.y0), (r.x1, r.y0)],
            HeadingPosition::Left => [(r.x1, r.y0), (r.x1, r.y1)],
            HeadingPosition::Right => [(r.x0, r.y0), (r.x0, r.y1)],
        };
        match c.heading_style {
            HeadingStyle::Band => {
                // Inset by half the outline so the band never covers the outline's inner half.
                let i = lw / 2.0;
                if c.band.a > 0 {
                    out.push(Decor::Fill {
                        points: world(&[(r.x0 + i, r.y0 + i), (r.x1 - i, r.y0 + i), (r.x1 - i, r.y1 - i), (r.x0 + i, r.y1 - i)]),
                        color: c.band,
                    });
                }
                if s.style.effective_line().a > 0 {
                    out.push(Decor::Line { points: world(&edge), color: line, width: lw.max(0.5), dashed: false });
                }
            }
            HeadingStyle::Line => out.push(Decor::Line { points: world(&edge), color: line, width: lw.max(0.5), dashed: false }),
            HeadingStyle::Plain => {}
        }
    }
    if c.role == Role::LaneSet && !c.phases.is_empty() {
        phase_decor(s, c, &world, line, &mut out);
    }
    out
}

/// A lane set's phase band, separators and titles.
fn phase_decor(s: &Shape, c: &Container, world: &dyn Fn(&[(f64, f64)]) -> Vec<Point>, line: Color, out: &mut Vec<Decor>) {
    let title = heading_rect(s).map_or(0.0, |r| r.height());
    let (w, h) = (s.w, s.h);
    let band = if c.band.a > 0 { lighten(c.band) } else { Color::rgb(0xf0, 0xf3, 0xf7) };
    let ats = phase_positions(s, c);
    let size = (s.style.font_size * 0.9).clamp(6.0, 48.0);
    let font = Color::rgb(0x2b, 0x34, 0x40);
    if c.vertical {
        let (x0, x1) = (0.0, PHASE_BAND.min(w));
        out.push(Decor::Fill { points: world(&[(x0, title), (x1, title), (x1, h), (x0, h)]), color: band });
        out.push(Decor::Line { points: world(&[(x1, title), (x1, h)]), color: line, width: 1.0, dashed: false });
        for (i, (at, name)) in ats.iter().enumerate() {
            out.push(Decor::Line { points: world(&[(0.0, *at), (w, *at)]), color: line, width: 1.0, dashed: true });
            let end = ats.get(i + 1).map_or(h, |n| n.0);
            if !name.is_empty() && end > *at {
                let center = world(&[((x0 + x1) / 2.0, (at + end) / 2.0)]).into_iter().next().unwrap_or(Point::ZERO);
                out.push(Decor::Text { center, room: end - at, angle: 270.0 + s.angle_deg, text: name.clone(), size, color: font, bold: false });
            }
        }
    } else {
        let (y0, y1) = (title, (title + PHASE_BAND).min(h));
        out.push(Decor::Fill { points: world(&[(0.0, y0), (w, y0), (w, y1), (0.0, y1)]), color: band });
        out.push(Decor::Line { points: world(&[(0.0, y1), (w, y1)]), color: line, width: 1.0, dashed: false });
        for (i, (at, name)) in ats.iter().enumerate() {
            out.push(Decor::Line { points: world(&[(*at, y0), (*at, h)]), color: line, width: 1.0, dashed: true });
            let end = ats.get(i + 1).map_or(w, |n| n.0);
            if !name.is_empty() && end > *at {
                let center = world(&[((at + end) / 2.0, (y0 + y1) / 2.0)]).into_iter().next().unwrap_or(Point::ZERO);
                out.push(Decor::Text { center, room: end - at, angle: s.angle_deg, text: name.clone(), size, color: font, bold: false });
            }
        }
    }
}

/// The phase separators of lane set `s`, clamped inside its lanes, in local units along it.
fn phase_positions(s: &Shape, c: &Container) -> Vec<(f64, String)> {
    let title = heading_rect(s).map_or(0.0, |r| r.height());
    let (lo, hi) = if c.vertical { (title + LANE_HEADING, s.h) } else { (LANE_HEADING, s.w) };
    c.phases.iter().filter(|p| p.at >= lo && p.at <= hi).map(|p| (p.at, p.title.clone())).collect()
}

/// The two ends of callout `s`'s leader line, from the edge of the callout to its target, or
/// `None` without a target on the page, with the leader off, or when the two overlap.
pub fn leader(page: &Page, s: &Shape) -> Option<(Point, Point)> {
    let c = s.container.as_ref().filter(|c| c.role == Role::Callout && c.leader)?;
    let target = page.shape(c.target.as_deref()?).filter(|t| t.id != s.id && page.shape_visible(t))?;
    let tb = target.bounds();
    let cb = s.bounds();
    let cc = cb.center();
    let mid = |side: Side| -> Point {
        let m = tb.center();
        match side {
            Side::Top => Point::new(m.x, tb.y0),
            Side::Bottom => Point::new(m.x, tb.y1),
            Side::Left => Point::new(tb.x0, m.y),
            Side::Right => Point::new(tb.x1, m.y),
            Side::Auto => m,
        }
    };
    let side = match c.side {
        Side::Auto => {
            // The side of the target that faces the callout.
            let d = cc - tb.center();
            let (hw, hh) = (tb.width().max(1e-9) / 2.0, tb.height().max(1e-9) / 2.0);
            if (d.x / hw).abs() >= (d.y / hh).abs() {
                if d.x < 0.0 { Side::Left } else { Side::Right }
            } else if d.y < 0.0 {
                Side::Top
            } else {
                Side::Bottom
            }
        }
        other => other,
    };
    let to = mid(side);
    if cb.contains(to) || !to.is_finite() {
        return None;
    }
    let from = clip_to_rect(cc, to, &cb);
    Some((from, to))
}

/// Where the segment from `inside` (in `r`) to `outside` leaves `r`.
fn clip_to_rect(inside: Point, outside: Point, r: &Rect) -> Point {
    let d = outside - inside;
    let mut t: f64 = 1.0;
    for (dv, lo, hi, v) in [(d.x, r.x0, r.x1, inside.x), (d.y, r.y0, r.y1, inside.y)] {
        if dv > 1e-12 {
            t = t.min((hi - v) / dv);
        } else if dv < -1e-12 {
            t = t.min((lo - v) / dv);
        }
    }
    let t = t.clamp(0.0, 1.0);
    Point::new(inside.x + d.x * t, inside.y + d.y * t)
}

/// The leader of callout `s` as decorations: the line and a dot where it meets the target.
pub fn leader_decor(page: &Page, s: &Shape) -> Vec<Decor> {
    let Some((a, b)) = leader(page, s) else { return Vec::new() };
    let color = line_of(s);
    let r = 2.5;
    let dot = (0..12).map(|i| {
        let t = f64::from(i) * std::f64::consts::TAU / 12.0;
        Point::new(b.x + r * t.cos(), b.y + r * t.sin())
    });
    vec![Decor::Line { points: vec![a, b], color, width: 1.25, dashed: false }, Decor::Fill { points: dot.collect(), color }]
}

/// Every callout leader on `page`, for drawing under the shapes.
pub fn page_leaders(page: &Page) -> Vec<Decor> {
    page.shapes.iter().filter(|s| is_callout(s) && page.shape_visible(s)).flat_map(|s| leader_decor(page, s)).collect()
}

// ---------------------------------------------------------------------------------------------
// Membership

/// Why a container change was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContainerError {
    /// The id names no container on the page.
    NotAContainer,
    /// The container is locked.
    Locked,
    /// None of the ids could join or leave.
    NoShapes,
    /// The shapes are in different groups or containers.
    MixedParents,
    /// The id names no lane set.
    NotALaneSet,
    /// The id names no lane.
    NotALane,
    /// The lane set is full.
    TooManyLanes,
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ContainerError::NotAContainer => "not a container on this page",
            ContainerError::Locked => "the container is locked; unlock it to change its members",
            ContainerError::NoShapes => "none of the shapes can join or leave this container",
            ContainerError::MixedParents => "the shapes are in different groups or containers",
            ContainerError::NotALaneSet => "not a lane set on this page",
            ContainerError::NotALane => "not a lane on this page",
            ContainerError::TooManyLanes => "the lane set already has the most lanes it can hold",
        })
    }
}

/// A coordinate clamped to the model's range.
fn coord(v: f64) -> f64 {
    if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 }
}

/// Move `ids` and everything inside them by `dx, dy` without refitting anything (the refit code
/// itself moves lanes this way).
fn shift(page: &mut Page, ids: &[String], dx: f64, dy: f64) {
    if (dx == 0.0 && dy == 0.0) || !dx.is_finite() || !dy.is_finite() {
        return;
    }
    let all: HashSet<String> = arrange::with_descendants(page, ids).into_iter().collect();
    for s in page.shapes.iter_mut().filter(|s| all.contains(&s.id)) {
        s.x = coord(s.x + dx);
        s.y = coord(s.y + dy);
        s.sanitize();
    }
}

/// Restack `members` (each with everything inside it) just above the last shape of `container`'s
/// own block, so members always draw over their container.
fn stack_above(page: &mut Page, container: &str, members: &[String]) {
    let moving: HashSet<String> = arrange::with_descendants(page, members).into_iter().collect();
    if moving.contains(container) {
        return;
    }
    let (block, rest): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut page.shapes).into_iter().partition(|s| moving.contains(&s.id));
    page.shapes = rest;
    let mine: HashSet<String> = arrange::with_descendants(page, &[container.to_string()]).into_iter().collect();
    let at = page.shapes.iter().rposition(|s| mine.contains(&s.id)).map_or(page.shapes.len(), |i| i + 1);
    let tail = page.shapes.split_off(at.min(page.shapes.len()));
    page.shapes.extend(block);
    page.shapes.extend(tail);
}

/// Whether `id` may become a member of `container`: not the container itself or one of its
/// ancestors, not a lane, not inside a group, not in a locked container.
fn may_join(page: &Page, container: &str, id: &str) -> bool {
    let Some(s) = page.shape(id) else { return false };
    if id == container || role(s) == Some(Role::Lane) || arrange::ancestors(page, container).iter().any(|a| a == id) {
        return false;
    }
    match s.parent.as_deref().and_then(|p| page.shape(p)) {
        Some(p) => !arrange::is_group(p) && !is_locked(p),
        None => true,
    }
}

/// The container `id` names, checked to hold members and be unlocked.
fn open_container<'a>(page: &'a Page, id: &str) -> Result<&'a Shape, ContainerError> {
    let s = page.shape(id).filter(|s| s.container.as_ref().is_some_and(Container::holds_members)).ok_or(ContainerError::NotAContainer)?;
    if is_locked(s) { Err(ContainerError::Locked) } else { Ok(s) }
}

/// Make `ids` members of `container` (they draw above it from then on). Shapes that cannot join
/// are skipped; an auto-fit container refits. Returns the shapes that joined.
pub fn add_members(page: &mut Page, container: &str, ids: &[String]) -> Result<Vec<String>, ContainerError> {
    open_container(page, container)?;
    let joining: Vec<String> = arrange::top_level(page, ids)
        .into_iter()
        .filter(|id| may_join(page, container, id) && page.shape(id).is_some_and(|s| s.parent.as_deref() != Some(container)))
        .collect();
    if joining.is_empty() {
        return Err(ContainerError::NoShapes);
    }
    for s in page.shapes.iter_mut().filter(|s| joining.contains(&s.id)) {
        s.parent = Some(container.to_string());
    }
    stack_above(page, container, &joining);
    arrange::refit_groups(page);
    Ok(joining)
}

/// Take `ids` out of `container`: they move up to the container's own parent. Returns the shapes
/// that left.
pub fn remove_members(page: &mut Page, container: &str, ids: &[String]) -> Result<Vec<String>, ContainerError> {
    let up = open_container(page, container)?.parent.clone();
    let leaving: Vec<String> = ids.iter().filter(|id| page.shape(id).is_some_and(|s| s.parent.as_deref() == Some(container))).cloned().collect();
    if leaving.is_empty() {
        return Err(ContainerError::NoShapes);
    }
    for s in page.shapes.iter_mut().filter(|s| leaving.contains(&s.id)) {
        s.parent = up.clone();
    }
    arrange::refit_groups(page);
    Ok(leaving)
}

/// The frame `(x, y, w, h)` container `id` needs to hold its members with its margins and heading,
/// or `None` without members.
pub fn fit_frame(page: &Page, id: &str) -> Option<(f64, f64, f64, f64)> {
    let s = page.shape(id)?;
    let c = s.container.as_ref()?;
    let b = arrange::bounds_of(page, &arrange::children(page, id))?;
    let r = b.inflate(c.margin);
    let t = if c.heading_size > 0.0 { c.heading_size } else { 0.0 };
    let r = match c.heading_position {
        HeadingPosition::Top => Rect::new(r.x0, r.y0 - t, r.x1, r.y1),
        HeadingPosition::Bottom => Rect::new(r.x0, r.y0, r.x1, r.y1 + t),
        HeadingPosition::Left => Rect::new(r.x0 - t, r.y0, r.x1, r.y1),
        HeadingPosition::Right => Rect::new(r.x0, r.y0, r.x1 + t, r.y1),
    };
    let (w, h) = (r.width().min(MAX_EXTENT), r.height().min(MAX_EXTENT));
    [r.x0, r.y0, w, h].iter().all(|v| v.is_finite()).then_some((coord(r.x0), coord(r.y0), w, h))
}

/// Set container `id`'s frame to hold its members (see [`fit_frame`]); the members stay where
/// they are. Returns whether the frame changed.
pub fn fit(page: &mut Page, id: &str) -> bool {
    let Some((x, y, w, h)) = fit_frame(page, id) else { return false };
    let Some(s) = page.shape_mut(id) else { return false };
    let changed = (s.x, s.y, s.w, s.h) != (x, y, w, h);
    (s.x, s.y, s.w, s.h) = (x, y, w, h);
    s.sanitize();
    refresh(s);
    changed
}

/// Remove container `id`, keeping its members: they move up to its parent. Connectors glued to it
/// keep a free end where they were. Returns the former members.
pub fn disband(page: &mut Page, id: &str) -> Result<Vec<String>, ContainerError> {
    let s = page.shape(id).filter(|s| is_container(s)).ok_or(ContainerError::NotAContainer)?;
    if s.container.as_ref().is_some_and(|c| c.locked) {
        return Err(ContainerError::Locked);
    }
    let up = s.parent.clone();
    let mut freed = Vec::new();
    for m in page.shapes.iter_mut().filter(|m| m.parent.as_deref() == Some(id)) {
        m.parent = up.clone();
        freed.push(m.id.clone());
    }
    page.detach_shape(id);
    page.remove_shape(id);
    arrange::refit_groups(page);
    Ok(freed)
}

/// Wrap `ids` in container `c` (already built, not yet on the page): it takes the shapes' shared
/// parent and the place of the lowest of them in z order, the shapes become its members, and its
/// frame fits them. Returns the members.
pub fn wrap(page: &mut Page, mut c: Shape, ids: &[String]) -> Result<Vec<String>, ContainerError> {
    let members = arrange::top_level(page, ids);
    let first = members.first().ok_or(ContainerError::NoShapes)?;
    let shared = page.shape(first).and_then(|s| s.parent.clone());
    if members.iter().any(|m| page.shape(m).and_then(|s| s.parent.clone()) != shared) {
        return Err(ContainerError::MixedParents);
    }
    if shared.as_deref().and_then(|p| page.shape(p)).is_some_and(is_locked) {
        return Err(ContainerError::Locked);
    }
    let at = page.shapes.iter().position(|s| members.contains(&s.id)).unwrap_or(page.shapes.len());
    c.parent = shared;
    let cid = c.id.clone();
    page.shapes.insert(at.min(page.shapes.len()), c);
    for s in page.shapes.iter_mut().filter(|s| members.contains(&s.id)) {
        s.parent = Some(cid.clone());
    }
    stack_above(page, &cid, &members);
    fit(page, &cid);
    arrange::refit_groups(page);
    Ok(members)
}

/// The membership changes a drop of `ids`, moved by `dx, dy`, makes: `(shape, new parent)` for
/// each shape whose container changes. Container frames are taken from before the move (an
/// auto-fit container would otherwise grow to follow a member being dragged out).
///
/// - A shape joins the innermost container whose frame holds all of it (topmost on a tie).
/// - A member dragged fully out of its container leaves it (for an outer container that holds
///   it, or the page); one only partly out stays.
/// - Locked containers neither take nor give up members; lanes stay in their set; members of
///   groups move with their group.
pub fn drop_changes(page: &Page, ids: &[String], dx: f64, dy: f64) -> Vec<(String, Option<String>)> {
    let (dx, dy) = (finite(dx, 0.0), finite(dy, 0.0));
    let subjects: Vec<String> = arrange::top_level(page, ids)
        .into_iter()
        .filter(|id| {
            page.shape(id).is_some_and(|s| {
                role(s) != Some(Role::Lane) && !s.parent.as_deref().and_then(|p| page.shape(p)).is_some_and(|p| arrange::is_group(p) || is_locked(p))
            })
        })
        .collect();
    let moving: HashSet<String> = arrange::with_descendants(page, &subjects).into_iter().collect();
    // Candidate containers: not moving, by depth then z order.
    let candidates: Vec<(usize, usize, &Shape)> = page
        .shapes
        .iter()
        .enumerate()
        .filter(|(_, s)| !moving.contains(&s.id) && s.container.as_ref().is_some_and(Container::holds_members) && page.shape_visible(s))
        .map(|(i, s)| (arrange::ancestors(page, &s.id).len(), i, s))
        .collect();
    let mut out = Vec::new();
    for id in subjects {
        let Some(s) = page.shape(&id) else { continue };
        let b0 = s.bounds();
        let b = Rect::new(b0.x0 + dx, b0.y0 + dy, b0.x1 + dx, b0.y1 + dy);
        let cur = s.parent.clone().filter(|p| page.shape(p).is_some_and(is_container));
        // A shape inside a group or under a non-container parent is not ours to move.
        if s.parent.is_some() && cur.is_none() {
            continue;
        }
        let target = candidates.iter().filter(|(_, _, c)| c.bounds().contains_rect(&b)).max_by_key(|(d, i, _)| (*d, *i)).map(|(_, _, c)| *c);
        let still_touching = cur.as_deref().and_then(|p| page.shape(p)).is_some_and(|p| p.bounds().intersects(&b));
        let new = match target {
            // Dropping into a locked container changes nothing.
            Some(t) if t.container.as_ref().is_some_and(|c| c.locked) => cur.clone(),
            // Partly out of a container but still inside one of its ancestors: stays.
            Some(t) if still_touching && cur.as_deref().is_some_and(|p| arrange::ancestors(page, p).contains(&t.id)) => cur.clone(),
            Some(t) => Some(t.id.clone()),
            None if still_touching => cur.clone(),
            None => None,
        };
        if new != cur {
            out.push((id, new));
        }
    }
    out
}

/// Apply changes from [`drop_changes`]: set each parent and stack joiners above their container.
pub fn apply_changes(page: &mut Page, changes: &[(String, Option<String>)]) {
    for (id, parent) in changes {
        if let Some(s) = page.shape_mut(id) {
            s.parent = parent.clone();
        }
        if let Some(p) = parent {
            stack_above(page, p, std::slice::from_ref(id));
        }
    }
    arrange::refit_groups(page);
}

// ---------------------------------------------------------------------------------------------
// Lanes

/// The lanes of lane set `set`, in order.
pub fn lanes(page: &Page, set: &str) -> Vec<String> {
    page.shapes.iter().filter(|s| s.parent.as_deref() == Some(set) && role(s) == Some(Role::Lane)).map(|s| s.id.clone()).collect()
}

/// The lane set `id` names.
fn lane_set<'a>(page: &'a Page, id: &str) -> Result<&'a Shape, ContainerError> {
    page.shape(id).filter(|s| role(s) == Some(Role::LaneSet)).ok_or(ContainerError::NotALaneSet)
}

/// The set lane `id` belongs to.
pub fn set_of(page: &Page, lane: &str) -> Option<String> {
    let s = page.shape(lane).filter(|s| role(s) == Some(Role::Lane))?;
    s.parent.clone().filter(|p| page.shape(p).is_some_and(|p| role(p) == Some(Role::LaneSet)))
}

/// Where the lanes of `set` start: below its title (and phase band), or right of its phase band.
fn lanes_origin(set: &Shape, c: &Container) -> (f64, f64) {
    let title = heading_rect(set).map_or(0.0, |r| r.height());
    let band = if c.phases.is_empty() { 0.0 } else { PHASE_BAND };
    if c.vertical { (set.x + band, set.y + title) } else { (set.x, set.y + title + band) }
}

/// Stack the lanes of `set` along it, in order, each keeping its thickness and taking the set's
/// length; a lane that moves carries its members. The set's thickness becomes the sum of its
/// lanes'. Nothing is refitted.
pub fn layout_lanes(page: &mut Page, set: &str) {
    let Some(s) = page.shape(set) else { return };
    let Some(c) = s.container.clone().filter(|c| c.role == Role::LaneSet) else { return };
    let ids = lanes(page, set);
    if ids.is_empty() {
        return;
    }
    let (ox, oy) = lanes_origin(s, &c);
    let (sx, sy, sw, sh) = (s.x, s.y, s.w, s.h);
    let mut at = if c.vertical { ox } else { oy };
    for id in &ids {
        let Some(l) = page.shape(id) else { continue };
        let (lx, ly) = (l.x, l.y);
        let (nx, ny, nw, nh) = if c.vertical {
            (at, oy, l.w.max(MIN_LANE), (sy + sh - oy).max(MIN_LANE))
        } else {
            (ox, at, (sx + sw - ox).max(MIN_LANE), l.h.max(MIN_LANE))
        };
        shift(page, std::slice::from_ref(id), nx - lx, ny - ly);
        if let Some(l) = page.shape_mut(id) {
            (l.x, l.y, l.w, l.h) = (coord(nx), coord(ny), nw.min(MAX_EXTENT), nh.min(MAX_EXTENT));
            if let Some(lc) = &mut l.container {
                lc.vertical = c.vertical;
                lc.heading_position = if c.vertical { HeadingPosition::Top } else { HeadingPosition::Left };
            }
            l.sanitize();
            refresh(l);
        }
        at += if c.vertical { nw } else { nh };
    }
    if let Some(s) = page.shape_mut(set) {
        if c.vertical {
            s.w = (at - sx).clamp(0.0, MAX_EXTENT);
        } else {
            s.h = (at - sy).clamp(0.0, MAX_EXTENT);
        }
        s.sanitize();
        refresh(s);
    }
}

/// Add lane `lane` (built with [`new_lane`]) to `set` at `index` (the end when `None` or past
/// it). Returns the lane's index.
pub fn add_lane(page: &mut Page, set: &str, mut lane: Shape, index: Option<usize>) -> Result<usize, ContainerError> {
    let s = lane_set(page, set)?;
    if s.container.as_ref().is_some_and(|c| c.locked) {
        return Err(ContainerError::Locked);
    }
    let vertical = s.container.as_ref().is_some_and(|c| c.vertical);
    let existing = lanes(page, set);
    if existing.len() >= MAX_LANES {
        return Err(ContainerError::TooManyLanes);
    }
    lane.parent = Some(set.to_string());
    if let Some(c) = &mut lane.container {
        c.vertical = vertical;
    }
    let index = index.unwrap_or(existing.len()).min(existing.len());
    // Before the block of the lane now at `index`, else after the set's whole block.
    let at = match existing.get(index) {
        Some(next) => page.shapes.iter().position(|x| x.id == *next),
        None => {
            let mine: HashSet<String> = arrange::with_descendants(page, &[set.to_string()]).into_iter().collect();
            page.shapes.iter().rposition(|x| mine.contains(&x.id)).map(|i| i + 1)
        }
    }
    .unwrap_or(page.shapes.len());
    page.shapes.insert(at.min(page.shapes.len()), lane);
    arrange::refit_groups(page);
    Ok(index)
}

/// Move lane `lane` to position `index` in its set (clamped); its members go with it.
pub fn move_lane(page: &mut Page, lane: &str, index: usize) -> Result<usize, ContainerError> {
    let set = set_of(page, lane).ok_or(ContainerError::NotALane)?;
    let mut order = lanes(page, &set);
    let from = order.iter().position(|l| l == lane).ok_or(ContainerError::NotALane)?;
    let index = index.min(order.len().saturating_sub(1));
    let id = order.remove(from);
    order.insert(index, id);
    // Pull every lane block out and put them back in the new order where the first one was.
    let blocks: Vec<Vec<String>> = order.iter().map(|l| arrange::with_descendants(page, std::slice::from_ref(l))).collect();
    let all: HashSet<&String> = blocks.iter().flatten().collect();
    let first = page.shapes.iter().position(|s| all.contains(&s.id)).unwrap_or(page.shapes.len());
    let (taken, rest): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut page.shapes).into_iter().partition(|s| all.contains(&s.id));
    let mut taken: Vec<Option<Shape>> = taken.into_iter().map(Some).collect();
    let mut ordered = Vec::with_capacity(taken.len());
    for block in &blocks {
        for id in block {
            if let Some(slot) = taken.iter_mut().find(|t| t.as_ref().is_some_and(|s| s.id == *id)) {
                ordered.extend(slot.take());
            }
        }
    }
    ordered.extend(taken.into_iter().flatten());
    page.shapes = rest;
    let tail = page.shapes.split_off(first.min(page.shapes.len()));
    page.shapes.extend(ordered);
    page.shapes.extend(tail);
    arrange::refit_groups(page);
    Ok(index)
}

/// Add a phase separator to `set` at `at` (from its leading edge) titled `title`.
pub fn add_phase(page: &mut Page, set: &str, at: f64, title: &str) -> Result<(), ContainerError> {
    lane_set(page, set)?;
    let s = page.shape_mut(set).ok_or(ContainerError::NotALaneSet)?;
    let Some(c) = s.container.as_mut() else { return Err(ContainerError::NotALaneSet) };
    if c.phases.len() >= MAX_PHASES {
        return Err(ContainerError::TooManyLanes);
    }
    c.phases.push(Phase { at, title: title.to_string() });
    s.sanitize();
    arrange::refit_groups(page);
    Ok(())
}

/// Turn lane set `set` the other way: horizontal lanes become vertical ones and back. Each lane
/// keeps its thickness and its members keep their place in it, mirrored across the diagonal.
pub fn flip(page: &mut Page, set: &str) -> Result<bool, ContainerError> {
    let s = lane_set(page, set)?;
    let Some(mut c) = s.container.clone() else { return Err(ContainerError::NotALaneSet) };
    let ids = lanes(page, set);
    // Each member's offset from its lane's top-left, swapped, from where the lane will be.
    let mut offsets: Vec<(String, Vec<(String, Point)>)> = Vec::new();
    for l in &ids {
        let Some(lane) = page.shape(l) else { continue };
        let members: Vec<(String, Point)> =
            arrange::children(page, l).into_iter().filter_map(|m| page.shape(&m).map(|ms| (m, ms.center() - Point::new(lane.x, lane.y)))).collect();
        offsets.push((l.clone(), members));
    }
    let (len, thick_sum) = if c.vertical { (s.h, s.w) } else { (s.w, s.h) };
    c.vertical = !c.vertical;
    let vertical = c.vertical;
    if let Some(sm) = page.shape_mut(set) {
        // The set swaps its length and thickness; layout settles the exact thickness.
        (sm.w, sm.h) = if vertical { (thick_sum, len) } else { (len, thick_sum) };
        sm.container = Some(c);
        sm.sanitize();
    }
    for l in &ids {
        if let Some(ls) = page.shape_mut(l) {
            (ls.w, ls.h) = (ls.h, ls.w);
        }
    }
    layout_lanes(page, set);
    for (l, members) in offsets {
        let Some(origin) = page.shape(&l).map(|ls| Point::new(ls.x, ls.y)) else { continue };
        for (m, off) in members {
            let Some(c) = page.shape(&m).map(Shape::center) else { continue };
            let want = origin + Point::new(off.y, off.x);
            shift(page, std::slice::from_ref(&m), want.x - c.x, want.y - c.y);
        }
    }
    arrange::refit_groups(page);
    Ok(vertical)
}

// ---------------------------------------------------------------------------------------------
// Hooks for the arrange code

/// Bring every container on `page` in step after a change: lane sets restack their lanes,
/// auto-fit containers refit (innermost first) and headings follow their bands. Called by
/// [`arrange::refit_groups`], so every move, resize, rotate and flip ends here.
pub fn refit(page: &mut Page) {
    if !page.shapes.iter().any(is_container) {
        return;
    }
    let mut sets: Vec<(usize, String)> =
        page.shapes.iter().filter(|s| role(s) == Some(Role::LaneSet)).map(|s| (arrange::ancestors(page, &s.id).len(), s.id.clone())).collect();
    sets.sort_by_key(|g| std::cmp::Reverse(g.0));
    for (_, set) in sets {
        layout_lanes(page, &set);
    }
    let mut fits: Vec<(usize, String)> = page
        .shapes
        .iter()
        .filter(|s| s.container.as_ref().is_some_and(|c| c.role == Role::Container && c.auto_fit))
        .map(|s| (arrange::ancestors(page, &s.id).len(), s.id.clone()))
        .collect();
    fits.sort_by_key(|g| std::cmp::Reverse(g.0));
    for (_, id) in &fits {
        fit(page, id);
    }
    for s in page.shapes.iter_mut().filter(|s| is_container(s)) {
        refresh(s);
    }
}

/// After shape `old.id` got a new frame: a lane's new length becomes its set's, and a lane set's
/// change in thickness goes to its last lane. Called by [`arrange::set_frame`] before refitting.
pub fn after_set_frame(page: &mut Page, old: &Shape) {
    let Some(new) = page.shape(&old.id).cloned() else { return };
    match role(old) {
        Some(Role::Lane) => {
            let Some(set) = set_of(page, &old.id) else { return };
            let Some(title) = page.shape(&set).map(|s| heading_rect(s).map_or(0.0, |r| r.height())) else { return };
            let vertical = old.container.as_ref().is_some_and(|c| c.vertical);
            if let Some(s) = page.shape_mut(&set) {
                if vertical && ((new.y - old.y).abs() > 1e-9 || (new.h - old.h).abs() > 1e-9) {
                    (s.y, s.h) = (coord(new.y - title), (new.h + title).min(MAX_EXTENT));
                } else if !vertical && ((new.x - old.x).abs() > 1e-9 || (new.w - old.w).abs() > 1e-9) {
                    (s.x, s.w) = (new.x, new.w);
                }
                s.sanitize();
            }
        }
        Some(Role::LaneSet) => {
            let vertical = old.container.as_ref().is_some_and(|c| c.vertical);
            let d = if vertical { new.w - old.w } else { new.h - old.h };
            if d.abs() <= 1e-9 {
                return;
            }
            if let Some(last) = lanes(page, &old.id).last().cloned()
                && let Some(l) = page.shape_mut(&last)
            {
                if vertical {
                    l.w = (l.w + d).max(MIN_LANE);
                } else {
                    l.h = (l.h + d).max(MIN_LANE);
                }
                l.sanitize();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
