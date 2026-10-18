//! Page setup beyond the width and height: orientation, units, drawing scale, background pages,
//! header and footer, guides and layers. All plain data; no command reads them yet.

use serde::{Deserialize, Serialize};

use crate::MAX_EXTENT;
use crate::geom::finite;
use crate::model::{Color, MAX_ID, truncate};
use crate::serde_util::{lenient, lenient_true, num};

/// Which way round the page is printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// Taller than wide.
    Portrait,
    /// Wider than tall. Unknown names load as this.
    #[default]
    Landscape,
}

crate::serde_util::named_enum!(Orientation, Orientation::Landscape, {
    Orientation::Portrait => "portrait",
    Orientation::Landscape => "landscape",
});

/// Units of measure for rulers and the drawing scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Units {
    /// Inches (100 world units). Unknown names load as this.
    #[default]
    Inches,
    /// Feet.
    Feet,
    /// Millimetres.
    Millimeters,
    /// Centimetres.
    Centimeters,
    /// Metres.
    Meters,
    /// Typographic points (1/72 inch).
    Points,
    /// World units (pixels at 100% zoom).
    Pixels,
}

crate::serde_util::named_enum!(Units, Units::Inches, {
    Units::Inches => "in",
    Units::Feet => "ft",
    Units::Millimeters => "mm",
    Units::Centimeters => "cm",
    Units::Meters => "m",
    Units::Points => "pt",
    Units::Pixels => "px",
});

/// A drawing scale: `page` page units stand for `drawing` drawing units (1 in = 10 ft).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DrawingScale {
    /// The page side of the ratio, in the page's units.
    #[serde(deserialize_with = "num")]
    pub page: f64,
    /// The drawing side of the ratio, in `units`.
    #[serde(deserialize_with = "num")]
    pub drawing: f64,
    /// Units of the drawing side.
    #[serde(deserialize_with = "lenient")]
    pub units: Units,
}

impl Default for DrawingScale {
    fn default() -> Self {
        DrawingScale::ONE_TO_ONE
    }
}

impl DrawingScale {
    /// No scaling.
    pub const ONE_TO_ONE: DrawingScale = DrawingScale { page: 1.0, drawing: 1.0, units: Units::Inches };
    /// Smallest ratio side kept.
    pub const MIN: f64 = 1.0e-6;
    /// Largest ratio side kept.
    pub const MAX: f64 = 1.0e6;

    /// Keep both sides of the ratio in `MIN..=MAX`; zero, negative or non-finite sides become 1.
    pub fn sanitize(&mut self) {
        let side = |v: f64| if v.is_finite() && v > 0.0 { v.clamp(Self::MIN, Self::MAX) } else { 1.0 };
        self.page = side(self.page);
        self.drawing = side(self.drawing);
    }
}

/// Header or footer text in three slots, with an optional distance from the page edge.
///
/// The slots may hold fields (`{page}`, `{pages}`, `{pageName}`, `{title}`, `{date}`) that exports
/// and printing expand (`crate::print_fields`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HeaderFooter {
    /// Left-aligned text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub left: String,
    /// Centred text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub center: String,
    /// Right-aligned text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub right: String,
    /// Distance of the text from the top (header) or bottom (footer) page edge, and from the
    /// side edges, in world units; `None` is [`HeaderFooter::DEFAULT_MARGIN`].
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
}

impl HeaderFooter {
    /// Longest slot text kept, in bytes.
    pub const MAX: usize = 1024;
    /// The margin when none is set: a quarter inch.
    pub const DEFAULT_MARGIN: f64 = 25.0;
    /// Largest margin kept, in world units (five inches).
    pub const MAX_MARGIN: f64 = 500.0;

    /// Cap every slot at [`HeaderFooter::MAX`] bytes and keep the margin finite and in range.
    pub fn sanitize(&mut self) {
        for s in [&mut self.left, &mut self.center, &mut self.right] {
            truncate(s, Self::MAX);
        }
        self.margin = self.margin.filter(|m| m.is_finite()).map(|m| m.clamp(0.0, Self::MAX_MARGIN));
    }
    /// The margin in use: the set one, else [`HeaderFooter::DEFAULT_MARGIN`].
    pub fn margin(&self) -> f64 {
        self.margin.filter(|m| m.is_finite()).map_or(Self::DEFAULT_MARGIN, |m| m.clamp(0.0, Self::MAX_MARGIN))
    }
    /// Whether every slot is blank.
    pub fn is_empty(&self) -> bool {
        self.left.trim().is_empty() && self.center.trim().is_empty() && self.right.trim().is_empty()
    }
}

/// Whether a guide is a horizontal or a vertical line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GuideOrientation {
    /// A horizontal line at a y position. Unknown names load as this.
    #[default]
    Horizontal,
    /// A vertical line at an x position.
    Vertical,
}

crate::serde_util::named_enum!(GuideOrientation, GuideOrientation::Horizontal, {
    GuideOrientation::Horizontal => "horizontal",
    GuideOrientation::Vertical => "vertical",
});

/// A guide line shapes snap to; never drawn in exports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Guide {
    /// Horizontal or vertical.
    #[serde(deserialize_with = "lenient")]
    pub orientation: GuideOrientation,
    /// The y (horizontal) or x (vertical) position in world units.
    #[serde(deserialize_with = "num")]
    pub position: f64,
}

/// A named layer: shapes on it share visibility, locking, printing, snapping and glue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layer {
    /// Unique id within the page; shapes name it in `layers`.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Display name.
    #[serde(deserialize_with = "lenient")]
    pub name: String,
    /// Shapes on it are drawn (a shape on several layers is drawn when any of them is visible).
    #[serde(deserialize_with = "lenient_true")]
    pub visible: bool,
    /// Shapes on it cannot be selected or changed.
    #[serde(deserialize_with = "lenient")]
    pub locked: bool,
    /// Shapes on it print.
    #[serde(deserialize_with = "lenient_true")]
    pub print: bool,
    /// Other shapes snap to shapes on it.
    #[serde(deserialize_with = "lenient_true")]
    pub snap: bool,
    /// Connectors glue to shapes on it.
    #[serde(deserialize_with = "lenient_true")]
    pub glue: bool,
    /// Colour that overrides its shapes' colours on the canvas; `None` keeps their own.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

impl Default for Layer {
    fn default() -> Self {
        Layer { id: String::new(), name: String::new(), visible: true, locked: false, print: true, snap: true, glue: true, color: None }
    }
}

impl Layer {
    /// Longest layer name kept, in bytes.
    pub const MAX_NAME: usize = 256;

    /// Cap the id and name lengths.
    pub fn sanitize(&mut self) {
        truncate(&mut self.id, MAX_ID);
        truncate(&mut self.name, Self::MAX_NAME);
    }
}

/// Most guides one page keeps.
pub const MAX_GUIDES: usize = 1000;

/// Most layers one page keeps, and most layer ids one shape names.
pub const MAX_LAYERS: usize = 256;

/// Clamp guide positions to `±MAX_EXTENT` and keep at most [`MAX_GUIDES`].
pub(crate) fn sanitize_guides(guides: &mut Vec<Guide>) {
    guides.truncate(MAX_GUIDES);
    for g in guides {
        g.position = finite(g.position, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT);
    }
}

/// Keep at most [`MAX_LAYERS`] layers with non-empty, unique ids.
pub(crate) fn sanitize_layers(layers: &mut Vec<Layer>) {
    layers.iter_mut().for_each(Layer::sanitize);
    let mut seen = std::collections::HashSet::new();
    layers.retain(|l| !l.id.is_empty() && seen.insert(l.id.clone()));
    layers.truncate(MAX_LAYERS);
}
