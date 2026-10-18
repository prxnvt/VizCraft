//! Pictures: the `<image>` a picture shape draws in SVG export, and the picture helpers the
//! engine and the canvas share (sniffing, pixel sizes, rasterising, compressing).
//!
//! A picture is written just before its shape's group, so the shape's outline (the picture's
//! border) draws over it: a `<g>` in the shape's frame (moved and turned like the shape) holding
//! one `<image>` whose `href` is a `data:` URI of the stored bytes. A crop stretches the image so
//! the visible part fills the frame and clips it to the frame with a `<clipPath>`; adjustments are
//! one `feColorMatrix` filter (see [`Adjust::matrix`]) in sRGB. PNG and PDF export draw the same
//! SVG, so they show the same crop and colours, and the canvas rasterises this element too
//! ([`raster`]), so the three agree.

use std::fmt::Write as _;

use vizcraft_diagram::doc_fields::ImageAsset;
use vizcraft_diagram::shape_fields::{Adjust, CropRect};
use vizcraft_diagram::{Document, ImageRef, Shape};

use resvg::usvg;

use super::{Ctx, escape, num};
use vizcraft_diagram::base64 as serde_b64;

/// PNG.
pub const PNG: &str = "image/png";
/// JPEG.
pub const JPEG: &str = "image/jpeg";
/// SVG, kept as text.
pub const SVG: &str = "image/svg+xml";
/// Largest side a rasterised picture gets, in pixels.
pub const MAX_RASTER_SIDE: u32 = 4096;
/// Largest side of a picture this module decodes, in pixels.
pub const MAX_DECODE_SIDE: u32 = 16_384;

/// The media type of `bytes`, from their first bytes: PNG, JPEG or SVG text. `None` for anything
/// else.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(PNG);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(JPEG);
    }
    let head = bytes.get(..bytes.len().min(4096)).unwrap_or_default();
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    (t.starts_with("<svg") || ((t.starts_with("<?xml") || t.starts_with("<!--") || t.starts_with("<!DOCTYPE")) && t.contains("<svg"))).then_some(SVG)
}

/// The size of a picture in pixels (SVG: its width and height in user units), read from the
/// header without decoding the pixels. `None` when the bytes are not a readable PNG, JPEG or SVG.
pub fn pixel_size(bytes: &[u8], media_type: &str) -> Option<(f64, f64)> {
    let (w, h) = match media_type {
        PNG => png_size(bytes)?,
        JPEG => jpeg_size(bytes)?,
        SVG => {
            let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
            let s = tree.size();
            (f64::from(s.width()), f64::from(s.height()))
        }
        _ => return None,
    };
    (w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()).then_some((w, h))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at.checked_add(1)?)?]))
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(b: &[u8]) -> Option<(f64, f64)> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let (w, h) = (be32(b, 16)?, be32(b, 20)?);
    (w > 0 && h > 0 && w <= MAX_DECODE_SIDE && h <= MAX_DECODE_SIDE).then_some((f64::from(w), f64::from(h)))
}

/// Width and height from a JPEG's first start-of-frame marker.
fn jpeg_size(b: &[u8]) -> Option<(f64, f64)> {
    let mut at = 2usize;
    // Each segment is a marker and a length; at most a few thousand before the frame.
    for _ in 0..10_000 {
        while b.get(at) == Some(&0xff) && b.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        if *b.get(at)? != 0xff {
            return None;
        }
        let marker = *b.get(at + 1)?;
        if matches!(marker, 0xd8 | 0x01 | 0xd0..=0xd7) {
            at = at.checked_add(2)?;
            continue;
        }
        let len = usize::from(be16(b, at + 2)?);
        let is_sof = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if is_sof {
            let (h, w) = (be16(b, at + 5)?, be16(b, at + 7)?);
            return (w > 0 && h > 0).then_some((f64::from(w), f64::from(h)));
        }
        if marker == 0xd9 || len < 2 {
            return None;
        }
        at = at.checked_add(2)?.checked_add(len)?;
    }
    None
}

/// The `data:` URI of a stored picture.
pub fn data_uri(asset: &ImageAsset) -> String {
    format!("data:{};base64,{}", asset.media_type, serde_b64::encode(&asset.bytes))
}

/// The `<image>` of `img` drawn in a `w` by `h` frame at the origin, with its clip path and filter
/// added to `cx`'s defs.
fn image_element(out: &mut String, cx: &mut Ctx, asset: &ImageAsset, img: &ImageRef, w: f64, h: f64, indent: &str) {
    let crop = visible(img.crop);
    // The whole picture, scaled so the visible part fills the frame.
    let (iw, ih) = (w / crop.w, h / crop.h);
    let (ix, iy) = (-crop.x * iw, -crop.y * ih);
    let mut attrs = String::new();
    if crop != CropRect::FULL {
        let id = cx.id("c");
        let _ = writeln!(cx.defs, "    <clipPath id=\"{id}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/></clipPath>", num(w), num(h));
        let _ = write!(attrs, " clip-path=\"url(#{id})\"");
    }
    if !img.adjust.is_neutral() {
        let id = filter(cx, &img.adjust);
        let _ = write!(attrs, " filter=\"url(#{id})\"");
    }
    let _ = writeln!(
        out,
        "{indent}<g{attrs}><image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"{}\"/></g>",
        num(ix),
        num(iy),
        num(iw),
        num(ih),
        escape(&data_uri(asset))
    );
}

/// The crop as drawn: at least a thousandth of the picture each way, so the scale stays finite.
fn visible(c: CropRect) -> CropRect {
    let mut c = c;
    c.sanitize();
    CropRect { x: c.x.min(0.999), y: c.y.min(0.999), w: c.w.max(0.001), h: c.h.max(0.001) }
}

/// A colour matrix filter for `adjust`; returns its id.
fn filter(cx: &mut Ctx, adjust: &Adjust) -> String {
    let id = cx.id("p");
    let values: Vec<String> = adjust.matrix().iter().flat_map(|row| row.iter().map(|v| coefficient(*v))).collect();
    let _ = writeln!(
        cx.defs,
        "    <filter id=\"{id}\" x=\"0\" y=\"0\" width=\"1\" height=\"1\" color-interpolation-filters=\"sRGB\"><feColorMatrix type=\"matrix\" values=\"{}\"/></filter>",
        values.join(" ")
    );
    id
}

/// A matrix coefficient to four decimals (luminance weights need more than [`num`]'s three),
/// without trailing zeros.
fn coefficient(v: f64) -> String {
    let t = format!("{:.4}", if v.is_finite() { v } else { 0.0 });
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" || t.is_empty() { "0".into() } else { t.into() }
}

/// The picture of `s`, if it shows one from `doc`'s store; written before the shape's group.
pub(super) fn picture_svg(out: &mut String, cx: &mut Ctx, doc: &Document, s: &Shape) {
    let Some(img) = &s.image else { return };
    let Some(asset) = doc.image(&img.asset) else { return };
    if s.w <= 0.0 || s.h <= 0.0 {
        return;
    }
    let m = s.transform().m;
    let _ = writeln!(
        out,
        "  <g class=\"picture\" data-shape=\"{}\" transform=\"matrix({} {} {} {} {} {})\">",
        escape(&s.id),
        num(m[0]),
        num(m[1]),
        num(m[2]),
        num(m[3]),
        num(m[4]),
        num(m[5])
    );
    image_element(out, cx, asset, img, s.w, s.h, "    ");
    out.push_str("  </g>\n");
}

/// A picture rasterised as SVG export draws it: `img` (crop and adjustments) of `asset` filling a
/// `w` by `h` frame, rendered at `px` by `py` pixels (each at most [`MAX_RASTER_SIDE`]). Returns
/// premultiplied RGBA rows. `None` when the picture cannot be read.
pub fn raster(asset: &ImageAsset, img: &ImageRef, w: f64, h: f64, px: u32, py: u32) -> Option<Vec<u8>> {
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return None;
    }
    let (px, py) = (px.clamp(1, MAX_RASTER_SIDE), py.clamp(1, MAX_RASTER_SIDE));
    let mut cx = Ctx::default();
    let mut body = String::new();
    image_element(&mut body, &mut cx, asset, img, w, h, "  ");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{px}\" height=\"{py}\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\"><defs>{}</defs>{body}</svg>",
        num(w),
        num(h),
        cx.defs
    );
    let tree = usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, py)?;
    let size = tree.size();
    let t = resvg::tiny_skia::Transform::from_scale(px as f32 / size.width().max(1e-6), py as f32 / size.height().max(1e-6));
    resvg::render(&tree, t, &mut pixmap.as_mut());
    Some(pixmap.take())
}

/// What [`compress`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compressed {
    /// The new media type (`image/jpeg` for an opaque picture, else `image/png`).
    pub media_type: &'static str,
    /// The re-encoded picture.
    pub bytes: Vec<u8>,
}

/// Downscale a PNG or JPEG so neither side passes `max_dimension` and re-encode it: an opaque
/// picture as JPEG at `quality` (1 to 100), one with transparency as PNG. `None` when the picture
/// is SVG, cannot be decoded, or the result would not be smaller.
pub fn compress(asset: &ImageAsset, max_dimension: u32, quality: u8) -> Option<Compressed> {
    use image::{GenericImageView as _, ImageFormat, ImageReader, Limits};
    let format = match asset.media_type.as_str() {
        PNG => ImageFormat::Png,
        JPEG => ImageFormat::Jpeg,
        _ => return None,
    };
    let mut reader = ImageReader::with_format(std::io::Cursor::new(&asset.bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let max = max_dimension.clamp(1, MAX_DECODE_SIDE);
    let (w, h) = img.dimensions();
    let img = if w > max || h > max { img.resize(max, max, image::imageops::FilterType::Triangle) } else { img };
    let opaque = !img.color().has_alpha() || img.to_rgba8().pixels().all(|p| p.0.get(3) == Some(&255));
    let mut out = Vec::new();
    let media_type = if opaque {
        let rgb = img.to_rgb8();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
        rgb.write_with_encoder(enc).ok()?;
        JPEG
    } else {
        img.write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png).ok()?;
        PNG
    };
    (out.len() < asset.bytes.len() && !out.is_empty()).then_some(Compressed { media_type, bytes: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing_and_sizes_read_headers_only() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&40u32.to_be_bytes());
        png.extend_from_slice(&30u32.to_be_bytes());
        assert_eq!(sniff(&png), Some(PNG));
        assert_eq!(pixel_size(&png, PNG), Some((40.0, 30.0)));
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 20, 0, 50, 3];
        assert_eq!(sniff(&jpeg), Some(JPEG));
        assert_eq!(pixel_size(&jpeg, JPEG), Some((50.0, 20.0)));
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"/>"#;
        assert_eq!(sniff(svg), Some(SVG));
        assert_eq!(pixel_size(svg, SVG), Some((64.0, 32.0)));
        assert_eq!(sniff(b"GIF89a"), None);
        assert_eq!(pixel_size(&[0xff, 0xd8, 0xff, 0xe0, 0xff, 0xff], JPEG), None);
        assert_eq!(pixel_size(&png[..18], PNG), None);
    }

    #[test]
    fn golden_picture_with_crop_and_adjustments() {
        use vizcraft_diagram::shape_fields::Recolor;
        use vizcraft_diagram::{Color, ShapeKind};
        let mut doc = Document::new();
        let bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        doc.images.push(ImageAsset { id: "img1".into(), media_type: PNG.into(), bytes: bytes.clone() });
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 50.0);
        s.style.fill = Color::TRANSPARENT;
        s.style.line = Color::TRANSPARENT;
        s.image = Some(ImageRef {
            asset: "img1".into(),
            crop: CropRect { x: 0.25, y: 0.0, w: 0.5, h: 1.0 },
            adjust: Adjust { brightness: 0.1, contrast: 0.0, transparency: 0.25, recolor: Recolor::Grayscale },
        });
        doc.page_mut().unwrap().shapes.push(s);
        let svg = String::from_utf8(super::super::export_page(&doc, 0).unwrap()).unwrap();
        let uri = format!("data:image/png;base64,{}", serde_b64::encode(&bytes));
        let expect_defs = "    <clipPath id=\"vc-c1\"><rect x=\"0\" y=\"0\" width=\"100\" height=\"50\"/></clipPath>\n    <filter id=\"vc-p2\" x=\"0\" y=\"0\" width=\"1\" height=\"1\" color-interpolation-filters=\"sRGB\"><feColorMatrix type=\"matrix\" values=\"0.2126 0.7152 0.0722 0 0.1 0.2126 0.7152 0.0722 0 0.1 0.2126 0.7152 0.0722 0 0.1 0 0 0 0.75 0\"/></filter>\n";
        assert!(svg.contains(expect_defs), "{svg}");
        let expect_body = format!(
            "  <g class=\"picture\" data-shape=\"s1\" transform=\"matrix(1 0 0 1 10 20)\">\n    <g clip-path=\"url(#vc-c1)\" filter=\"url(#vc-p2)\"><image x=\"-50\" y=\"0\" width=\"200\" height=\"50\" preserveAspectRatio=\"none\" href=\"{uri}\"/></g>\n  </g>\n  <g id=\"s1\">"
        );
        assert!(svg.contains(&expect_body), "{svg}");
    }
}
