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
    /// Brightness, contrast, transparency and recolouring, applied when the picture is drawn.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Adjust::is_neutral")]
    pub adjust: Adjust,
}

fn is_full(c: &CropRect) -> bool {
    *c == CropRect::FULL
}

impl ImageRef {
    /// Cap the id length, keep the crop inside the picture and the adjustments in range.
    pub fn sanitize(&mut self) {
        truncate(&mut self.asset, MAX_ID);
        self.crop.sanitize();
        self.adjust.sanitize();
    }
}

/// How a picture's colours are changed before it is drawn (Picture > Adjust). The neutral value
/// (all zero, no recolour) draws the picture as stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Adjust {
    /// Added to every colour channel, from -1 (black) to 1 (white); 0 leaves it.
    #[serde(deserialize_with = "num")]
    pub brightness: f64,
    /// From -1 (flat grey) to 1 (twice the contrast); 0 leaves it. Channels are stretched about
    /// the middle grey by `1 + contrast`.
    #[serde(deserialize_with = "num")]
    pub contrast: f64,
    /// From 0 (opaque) to 1 (invisible).
    #[serde(deserialize_with = "num")]
    pub transparency: f64,
    /// The colour mapping applied first.
    #[serde(deserialize_with = "lenient")]
    pub recolor: Recolor,
}

impl Adjust {
    /// Whether the adjustments leave the picture as stored.
    pub fn is_neutral(&self) -> bool {
        *self == Adjust::default()
    }

    /// Keep every number finite and in range.
    pub fn sanitize(&mut self) {
        self.brightness = finite(self.brightness, 0.0).clamp(-1.0, 1.0);
        self.contrast = finite(self.contrast, 0.0).clamp(-1.0, 1.0);
        self.transparency = finite(self.transparency, 0.0).clamp(0.0, 1.0);
        if let Recolor::Tint(c) = &mut self.recolor {
            c.a = 255;
        }
    }

    /// The colour matrix the adjustments make, as SVG's `feColorMatrix type="matrix"` reads it:
    /// four rows of five numbers (red, green, blue, alpha, offset) on channels in `0..=1`. The
    /// recolour comes first, then the contrast, then the brightness; the transparency scales alpha.
    pub fn matrix(&self) -> [[f64; 5]; 4] {
        const L: [f64; 3] = [0.2126, 0.7152, 0.0722];
        let mut rgb: [[f64; 4]; 3] = match self.recolor {
            Recolor::None => [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
            Recolor::Grayscale => [[L[0], L[1], L[2], 0.0]; 3],
            Recolor::Sepia => [[0.393, 0.769, 0.189, 0.0], [0.349, 0.686, 0.168, 0.0], [0.272, 0.534, 0.131, 0.0]],
            // Black takes the tint, white stays white: `tint + (1 - tint) * luminance`.
            Recolor::Tint(c) => {
                let row = |v: u8| {
                    let t = f64::from(v) / 255.0;
                    [L[0] * (1.0 - t), L[1] * (1.0 - t), L[2] * (1.0 - t), t]
                };
                [row(c.r), row(c.g), row(c.b)]
            }
        };
        let k = 1.0 + self.contrast;
        for row in &mut rgb {
            for v in row.iter_mut().take(3) {
                *v *= k;
            }
            row[3] = row[3] * k + 0.5 * (1.0 - k) + self.brightness;
        }
        let r = |i: usize| rgb.get(i).copied().unwrap_or_default();
        let (a, b, c) = (r(0), r(1), r(2));
        [[a[0], a[1], a[2], 0.0, a[3]], [b[0], b[1], b[2], 0.0, b[3]], [c[0], c[1], c[2], 0.0, c[3]], [0.0, 0.0, 0.0, 1.0 - self.transparency, 0.0]]
    }
}

/// A colour mapping for a picture. On disk `none`, `grayscale`, `sepia` or a `#rrggbb` tint;
/// anything else loads as `none`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Recolor {
    /// The picture's own colours.
    #[default]
    None,
    /// Shades of grey by luminance.
    Grayscale,
    /// Warm brown tones.
    Sepia,
    /// Black becomes this colour and white stays white, through the luminance in between.
    Tint(crate::Color),
}

impl Recolor {
    /// The name used on disk and in `picture.adjust`.
    pub fn name(&self) -> String {
        match self {
            Recolor::None => "none".into(),
            Recolor::Grayscale => "grayscale".into(),
            Recolor::Sepia => "sepia".into(),
            Recolor::Tint(c) => crate::Color::rgb(c.r, c.g, c.b).to_hex(),
        }
    }

    /// The recolour `s` names: `none`, `grayscale` (or `greyscale`), `sepia` or a colour.
    pub fn parse(s: &str) -> Option<Recolor> {
        match s.trim() {
            "none" | "" => Some(Recolor::None),
            "grayscale" | "greyscale" => Some(Recolor::Grayscale),
            "sepia" => Some(Recolor::Sepia),
            other => crate::Color::parse(other).map(|c| Recolor::Tint(crate::Color::rgb(c.r, c.g, c.b))),
        }
    }
}

impl Serialize for Recolor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Recolor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s: String = lenient(d)?;
        Ok(Recolor::parse(&s).unwrap_or_default())
    }
}

/// The outline of a [`ShapeKind::Freeform`](crate::ShapeKind::Freeform) shape: path commands in
/// the shape's local frame, from `(0, 0)` to `(w, h)`. A boolean operation, join or trim writes it
/// with the frame fitted to its bounds; a later resize stretches it with the frame (see
/// [`Freeform::outline`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Freeform {
    /// The commands, at most [`Freeform::MAX_ELS`]. Subpaths may mix: one that ends with a
    /// `Close` is filled (even-odd, so a ring inside another is a hole), one without is a line
    /// drawn over the fill. A path glyph's detail lines are such open subpaths.
    #[serde(deserialize_with = "lenient_vec")]
    pub els: Vec<PathEl>,
    /// Whether any subpath is closed (filled). An outline with none draws its lines only.
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
    /// `MoveTo`, and make the `Close` commands agree with `closed`: an open outline closes no
    /// subpath; a closed one keeps the `Close` commands it has, or closes every subpath when it
    /// has none (files from before subpaths could mix). `closed` then says whether any subpath
    /// is closed.
    pub fn sanitize(&mut self) {
        self.els.truncate(Self::MAX_ELS);
        let close_all = self.closed && !self.els.iter().any(|e| matches!(e, PathEl::Close));
        let fix = |p: Point| Point::new(finite(p.x, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT), finite(p.y, 0.0).clamp(-MAX_EXTENT, MAX_EXTENT));
        let mut out: Vec<PathEl> = Vec::with_capacity(self.els.len());
        // Whether a subpath is open (has a pen position) and has drawn anything.
        let mut open = false;
        let mut drawn = false;
        for el in &self.els {
            match *el {
                PathEl::MoveTo(p) => {
                    if close_all && open && drawn {
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
        } else if close_all && open && drawn {
            out.push(PathEl::Close);
        }
        out.truncate(Self::MAX_ELS);
        if out.iter().any(|e| matches!(e, PathEl::Close)) {
            self.closed = true;
        } else if out.iter().any(|e| matches!(e, PathEl::LineTo(_) | PathEl::CubicTo(..))) {
            self.closed = false;
        }
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

#[cfg(test)]
mod freeform_tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn closed_and_open_subpaths_mix_and_keep_their_closes() {
        let mut path = Path::polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]);
        path.els.extend(Path::polyline(&[p(2.0, 5.0), p(8.0, 5.0)]).els);
        let f = Freeform::new(&path, true);
        assert!(f.closed);
        assert_eq!(f.els.iter().filter(|e| matches!(e, PathEl::Close)).count(), 1, "the detail line stays open");
        let lines = f.path().flatten(0.5);
        assert_eq!(lines.iter().map(|l| l.closed).collect::<Vec<_>>(), [true, false]);
        // An open outline closes nothing, and a closed one with no Close (an older file) closes all.
        assert!(!Freeform::new(&path, false).els.iter().any(|e| matches!(e, PathEl::Close)));
        let old = Freeform::new(&Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(5.0, 5.0)]), true);
        assert_eq!((old.closed, old.els.last()), (true, Some(&PathEl::Close)));
        // A closed flag over subpaths that are all open is corrected.
        let mut lie = Freeform { els: Path::polyline(&[p(0.0, 0.0), p(5.0, 5.0)]).els, closed: true };
        lie.els.push(PathEl::MoveTo(p(9.0, 9.0)));
        lie.els.push(PathEl::LineTo(p(9.0, 1.0)));
        lie.els.push(PathEl::Close);
        lie.els.splice(0..0, [PathEl::MoveTo(p(1.0, 1.0)), PathEl::LineTo(p(2.0, 2.0))]);
        lie.sanitize();
        assert!(lie.closed);
    }
}
