//! The parts of a [`Shape`](crate::Shape) beyond frame, kind, text and style: the text block, links,
//! protection locks, behaviour flags and a picture. All plain data; no command reads them yet.

use serde::{Deserialize, Serialize};

use crate::MAX_EXTENT;
use crate::geom::finite;
use crate::model::{MAX_ID, truncate};
use crate::serde_util::{lenient, num};

/// Where a shape's text sits, relative to the shape. `None` on a shape means the text block is the
/// shape's own frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextBlock {
    /// Offset of the block centre from the frame centre, x, in the shape's unrotated frame.
    #[serde(deserialize_with = "num")]
    pub dx: f64,
    /// Offset of the block centre from the frame centre, y.
    #[serde(deserialize_with = "num")]
    pub dy: f64,
    /// Block width in world units.
    #[serde(deserialize_with = "num")]
    pub w: f64,
    /// Block height in world units.
    #[serde(deserialize_with = "num")]
    pub h: f64,
    /// Rotation of the block relative to the shape, degrees clockwise.
    #[serde(deserialize_with = "num")]
    pub angle_deg: f64,
}

impl TextBlock {
    /// Clamp offsets to `±MAX_EXTENT`, the size to `0..=MAX_EXTENT` and the angle to `0..360`.
    pub fn sanitize(&mut self) {
        self.dx = finite(self.dx, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT);
        self.dy = finite(self.dy, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT);
        self.w = finite(self.w, 0.0).clamp(0.0, MAX_EXTENT);
        self.h = finite(self.h, 0.0).clamp(0.0, MAX_EXTENT);
        self.angle_deg = finite(self.angle_deg, 0.0).rem_euclid(360.0);
    }
}

/// What a hyperlink points at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// A web address. Unknown names load as this.
    #[default]
    Url,
    /// A file path.
    File,
    /// A page of this document, by id.
    Page,
    /// A shape of this document, by id.
    Shape,
    /// A model element, by id.
    Element,
}

crate::serde_util::named_enum!(LinkKind, LinkKind::Url, {
    LinkKind::Url => "url",
    LinkKind::File => "file",
    LinkKind::Page => "page",
    LinkKind::Shape => "shape",
    LinkKind::Element => "element",
});

/// One hyperlink on a shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hyperlink {
    /// What the target names.
    #[serde(deserialize_with = "lenient")]
    pub kind: LinkKind,
    /// The address, path or id, at most [`Hyperlink::MAX_TARGET`] bytes.
    #[serde(deserialize_with = "lenient")]
    pub target: String,
    /// Text shown for the link, at most [`Hyperlink::MAX_LABEL`] bytes; empty shows the target.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl Hyperlink {
    /// Longest target kept, in bytes.
    pub const MAX_TARGET: usize = 4096;
    /// Longest label kept, in bytes.
    pub const MAX_LABEL: usize = 1024;
    /// Most links one shape keeps.
    pub const MAX_PER_SHAPE: usize = 32;

    /// Cap the target and label lengths.
    pub fn sanitize(&mut self) {
        truncate(&mut self.target, Self::MAX_TARGET);
        truncate(&mut self.label, Self::MAX_LABEL);
    }
}

/// Protection: what editing a shape refuses. All off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Locks {
    /// The width cannot change.
    #[serde(deserialize_with = "lenient")]
    pub width: bool,
    /// The height cannot change.
    #[serde(deserialize_with = "lenient")]
    pub height: bool,
    /// Resizing keeps the width to height ratio.
    #[serde(deserialize_with = "lenient")]
    pub aspect: bool,
    /// The shape cannot move.
    #[serde(deserialize_with = "lenient")]
    pub position: bool,
    /// The shape cannot rotate.
    #[serde(deserialize_with = "lenient")]
    pub rotation: bool,
    /// The shape cannot be deleted.
    #[serde(deserialize_with = "lenient")]
    pub delete: bool,
    /// The shape cannot be selected on the canvas.
    #[serde(deserialize_with = "lenient")]
    pub select: bool,
    /// The text cannot be edited.
    #[serde(deserialize_with = "lenient")]
    pub text: bool,
}

/// How a shape behaves on the page. All off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Behavior {
    /// Shapes dropped on it become its members (their `parent` is this shape).
    #[serde(deserialize_with = "lenient")]
    pub container: bool,
    /// Left out of printing.
    #[serde(deserialize_with = "lenient")]
    pub non_printing: bool,
    /// Not drawn at all (still in the document and its data).
    #[serde(deserialize_with = "lenient")]
    pub hidden: bool,
    /// Connectors cannot glue to it.
    #[serde(deserialize_with = "lenient")]
    pub no_glue: bool,
    /// Other shapes do not snap to it.
    #[serde(deserialize_with = "lenient")]
    pub no_snap: bool,
}

/// The visible part of a picture, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CropRect {
    /// Left edge, 0 to 1.
    #[serde(deserialize_with = "num")]
    pub x: f64,
    /// Top edge, 0 to 1.
    #[serde(deserialize_with = "num")]
    pub y: f64,
    /// Width, 0 to 1.
    #[serde(deserialize_with = "num")]
    pub w: f64,
    /// Height, 0 to 1.
    #[serde(deserialize_with = "num")]
    pub h: f64,
}

impl Default for CropRect {
    fn default() -> Self {
        CropRect::FULL
    }
}

impl CropRect {
    /// The whole picture.
    pub const FULL: CropRect = CropRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    /// Keep the rectangle inside the picture: `x` and `y` in `0..=1`, the size no larger than
    /// what is left.
    pub fn sanitize(&mut self) {
        self.x = finite(self.x, 0.0).clamp(0.0, 1.0);
        self.y = finite(self.y, 0.0).clamp(0.0, 1.0);
        self.w = finite(self.w, 1.0).clamp(0.0, 1.0 - self.x);
        self.h = finite(self.h, 1.0).clamp(0.0, 1.0 - self.y);
    }
}

/// A picture drawn in a shape's frame, from the document's image store.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImageRef {
    /// The [`ImageAsset`](crate::doc_fields::ImageAsset) id.
    #[serde(deserialize_with = "lenient")]
    pub asset: String,
    /// The visible part of the picture.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_full")]
    pub crop: CropRect,
}

fn is_full(c: &CropRect) -> bool {
    *c == CropRect::FULL
}

impl ImageRef {
    /// Cap the id length and keep the crop inside the picture.
    pub fn sanitize(&mut self) {
        truncate(&mut self.asset, MAX_ID);
        self.crop.sanitize();
    }
}
