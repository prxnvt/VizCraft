//! The parts of a [`Shape`](crate::Shape) beyond frame, kind, text and style: the text block, links,
//! protection locks, behaviour flags, a picture and the freeform outline. All plain data.

use serde::{Deserialize, Serialize};

use crate::MAX_EXTENT;
use crate::geom::{Path, PathEl, Point, Transform, finite};
use crate::model::{MAX_ID, truncate};
use crate::serde_util::{lenient, lenient_vec, num};

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

/// The outline of a [`ShapeKind::Freeform`](crate::ShapeKind::Freeform) shape: path commands in
/// the shape's local frame, from `(0, 0)` to `(w, h)`. A boolean operation, join or trim writes it
/// with the frame fitted to its bounds; a later resize stretches it with the frame (see
/// [`Freeform::outline`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Freeform {
    /// The commands, at most [`Freeform::MAX_ELS`]. Every subpath of a closed outline ends with a
    /// `Close`; an open one has none.
    #[serde(deserialize_with = "lenient_vec")]
    pub els: Vec<PathEl>,
    /// Whether the outline is closed (filled). An open path draws its line only.
    #[serde(deserialize_with = "lenient")]
    pub closed: bool,
}

impl Freeform {
    /// Most path commands one freeform keeps.
    pub const MAX_ELS: usize = 4096;

    /// A freeform from `path` (already in local coordinates), sanitised.
    pub fn new(path: &Path, closed: bool) -> Freeform {
        let mut f = Freeform { els: path.els.clone(), closed };
        f.sanitize();
        f
    }

    /// Cap the command count, keep every point finite and in range, start every subpath with a
    /// `MoveTo`, and make the `Close` commands agree with `closed`: a closed outline closes every
    /// subpath, an open one closes none.
    pub fn sanitize(&mut self) {
        self.els.truncate(Self::MAX_ELS);
        let fix = |p: Point| Point::new(finite(p.x, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT), finite(p.y, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT));
        let mut out: Vec<PathEl> = Vec::with_capacity(self.els.len());
        // Whether a subpath is open (has a pen position) and has drawn anything.
        let mut open = false;
        let mut drawn = false;
        for el in &self.els {
            match *el {
                PathEl::MoveTo(p) => {
                    if self.closed && open && drawn {
                        out.push(PathEl::Close);
                    }
                    // A MoveTo right after another replaces it: an empty subpath draws nothing.
                    if open && !drawn {
                        out.pop();
                    }
                    out.push(PathEl::MoveTo(fix(p)));
                    open = true;
                    drawn = false;
                }
                PathEl::LineTo(p) if open => {
                    out.push(PathEl::LineTo(fix(p)));
                    drawn = true;
                }
                PathEl::CubicTo(a, b, p) if open => {
                    out.push(PathEl::CubicTo(fix(a), fix(b), fix(p)));
                    drawn = true;
                }
                PathEl::Close => {
                    if self.closed && open && drawn {
                        out.push(PathEl::Close);
                    }
                    if open && !drawn {
                        out.pop();
                    }
                    open = false;
                    drawn = false;
                }
                // A line or curve with no MoveTo before it has no start: dropped.
                PathEl::LineTo(_) | PathEl::CubicTo(..) => {}
            }
        }
        if open && !drawn {
            out.pop();
        } else if self.closed && open && drawn {
            out.push(PathEl::Close);
        }
        out.truncate(Self::MAX_ELS);
        // Cutting at the cap may leave a dangling MoveTo or an unclosed subpath; neither breaks
        // drawing, which treats a trailing MoveTo as nothing.
        self.els = out;
    }

    /// The stored path.
    pub fn path(&self) -> Path {
        Path { els: self.els.clone() }
    }

    /// The outline for a `w` by `h` frame: the stored path with its bounds stretched onto the
    /// frame, so it follows a resize. A side of the path with no extent (a straight line) is moved,
    /// not stretched. An empty path is the frame's rectangle.
    pub fn outline(&self, w: f64, h: f64) -> Path {
        let (w, h) = (finite(w, 0.0).clamp(0.0, MAX_EXTENT), finite(h, 0.0).clamp(0.0, MAX_EXTENT));
        let path = self.path();
        let Some(b) = path.bounds() else {
            return Path::polygon(&[Point::new(0.0, 0.0), Point::new(w, 0.0), Point::new(w, h), Point::new(0.0, h)]);
        };
        let sx = if b.width() > 1e-9 { w / b.width() } else { 1.0 };
        let sy = if b.height() > 1e-9 { h / b.height() } else { 1.0 };
        let fit = Transform::translate(-b.x0, -b.y0).then(&Transform::scale(sx, sy));
        path.transformed(&fit)
    }
}
