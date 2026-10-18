//! Pictures (Insert > Illustrations > Picture and the contextual Picture tab): insert, change,
//! crop, adjust, reset and compress.
//!
//! A picture is a Rectangle shape with no fill, a transparent outline and `image` pointing at an
//! asset in `Document.images`. The bytes are stored once: inserting the same picture again
//! reuses its asset (compared byte for byte; the id is `img-` and a content hash). PNG and JPEG
//! give their size from the header; SVG is kept as text and sized by parsing it. Drawing (canvas,
//! SVG, PNG and PDF export) lives in `vizcraft_formats::svg::pictures`.

use serde_json::{Value, json};
use vizcraft_diagram::doc_fields::ImageAsset;
use vizcraft_diagram::shape_fields::{Adjust, CropRect, Recolor};
use vizcraft_diagram::{Color, ImageRef, Shape, ShapeKind};
use vizcraft_formats::svg::pictures as pic;

use super::shape::{coord, size};
use super::{Kinds, has_shapes, num, opt_bool, opt_str, page_mut, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

pub use pic::{Compressed, raster};

/// Default `maxDimension` of `picture.compress`, in pixels.
pub const COMPRESS_MAX_DIMENSION: u32 = 1024;
/// Default JPEG `quality` of `picture.compress`.
pub const COMPRESS_QUALITY: u8 = 80;

/// The picture commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("picture.insert", "Picture", "Insert > Illustrations", insert)
            .params(r#"{"path"?: string or "bytes"?: base64 (a data: URI is fine), "mediaType"?: "image/png|image/jpeg|image/svg+xml" (default: sniffed), "x"?: n, "y"?: n (top-left; default: centred in the view), "w"?: n, "h"?: n (default: the picture's size, scaled to fit a quarter of the page frame), "id"?: string} -> {"id", "asset", "reused"}"#),
        CommandSpec::new("picture.change", "Change Picture", "Picture > Adjust", change)
            .params(r#"{"id"?: picture shape (default: the selected one), "path"? or "bytes"?, "mediaType"?} -> {"id", "asset"}; keeps the frame and adjustments, clears the crop"#)
            .when(has_shapes),
        CommandSpec::new("picture.crop", "Crop", "Picture > Size", crop)
            .params(r#"{"id"?: picture shape, "crop": {"x", "y", "w", "h"} (fractions of the source picture, 0 to 1)} -> {"id", "crop"}"#)
            .when(has_shapes),
        CommandSpec::new("picture.adjust", "Adjust Picture", "Picture > Adjust", adjust)
            .params(r##"{"id"?: picture shape, "brightness"?: -1 to 1, "contrast"?: -1 to 1, "transparency"?: 0 to 1, "recolor"?: "none|grayscale|sepia|#rrggbb"} -> {"id", "adjust"}; unnamed settings are kept"##)
            .when(has_shapes),
        CommandSpec::new("picture.reset", "Reset Picture", "Picture > Adjust", reset)
            .params(r#"{"id"?: picture shape} -> {"id"}; clears the crop and every adjustment"#)
            .when(has_shapes),
        CommandSpec::new("picture.compress", "Compress Pictures", "Picture > Adjust", compress)
            .params(r#"{"ids"?: [picture shapes] (default: the selected ones) or "all"?: true (every picture in the document), "maxDimension"?: n (pixels, default 1024), "quality"?: 1 to 100 (JPEG, default 80)} -> {"compressed": [{"asset", "before", "after", "mediaType"}], "unchanged": [asset ids]}; opaque pictures become JPEG, the rest PNG; SVG is left alone"#)
            .when(has_shapes),
    ]
}

/// Whether `s` shows a picture.
pub fn is_picture(s: &Shape) -> bool {
    s.image.is_some()
}

/// FNV-1a, 64 bits: a stable content hash for asset ids.
fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The media type a caller names, normalised; `None` for an unsupported one.
fn media_param(t: &str) -> Option<&'static str> {
    match t.trim().to_ascii_lowercase().as_str() {
        "image/png" | "png" => Some(pic::PNG),
        "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Some(pic::JPEG),
        "image/svg+xml" | "svg" => Some(pic::SVG),
        _ => None,
    }
}

/// The picture bytes and media type from `path` or `bytes` (base64), checked: at most
/// [`ImageAsset::MAX_BYTES`], a readable PNG, JPEG or SVG. Also returns its pixel size.
fn source(v: &Value) -> Result<(Vec<u8>, &'static str, (f64, f64)), EngineError> {
    let bytes = match (opt_str(v, "path")?, opt_str(v, "bytes")?) {
        (Some(path), _) => crate::io::read_limited(path)?,
        (None, Some(text)) => {
            // A base64 text longer than the largest picture would decode to is refused unread.
            if text.len() > ImageAsset::MAX_BYTES / 3 * 4 + 1024 {
                return Err(EngineError::Params(format!("`bytes` is larger than {} MiB", ImageAsset::MAX_BYTES / (1024 * 1024))));
            }
            let body = match text.strip_prefix("data:") {
                Some(rest) => rest.split_once(',').map(|(_, b)| b).unwrap_or_default(),
                None => text,
            };
            vizcraft_diagram::base64::decode(body).ok_or_else(|| EngineError::Params("`bytes` is not base64".into()))?
        }
        (None, None) => return Err(EngineError::Params("`path` or `bytes` (base64) is required".into())),
    };
    if bytes.is_empty() {
        return Err(EngineError::Params("the picture is empty".into()));
    }
    if bytes.len() > ImageAsset::MAX_BYTES {
        return Err(EngineError::Params(format!("the picture is larger than {} MiB", ImageAsset::MAX_BYTES / (1024 * 1024))));
    }
    let media = match opt_str(v, "mediaType")? {
        Some(t) => media_param(t).ok_or_else(|| EngineError::Params(format!("unsupported media type `{}`; use PNG, JPEG or SVG", short(t))))?,
        None => pic::sniff(&bytes).ok_or_else(|| EngineError::Params("not a PNG, JPEG or SVG picture".into()))?,
    };
    let size = pic::pixel_size(&bytes, media).ok_or_else(|| EngineError::Params(format!("not a readable {media} picture")))?;
    Ok((bytes, media, size))
}

/// The id of an asset holding `bytes`: an existing one with the same bytes, else a new one added
/// to the store. Returns the id and whether it was reused.
fn store(s: &mut Session, bytes: Vec<u8>, media: &'static str) -> Result<(String, bool), EngineError> {
    if let Some(a) = s.doc.images.iter().find(|a| a.media_type == media && a.bytes == bytes) {
        return Ok((a.id.clone(), true));
    }
    if s.doc.images.len() >= ImageAsset::MAX_ASSETS {
        return Err(EngineError::Params(format!("the document already holds {} pictures", ImageAsset::MAX_ASSETS)));
    }
    let total: usize = s.doc.images.iter().map(|a| a.bytes.len()).fold(0, usize::saturating_add);
    if total.saturating_add(bytes.len()) > ImageAsset::MAX_STORE_BYTES {
        return Err(EngineError::Params(format!("the document's pictures would pass {} MiB", ImageAsset::MAX_STORE_BYTES / (1024 * 1024))));
    }
    let base = format!("img-{:016x}", content_hash(&bytes));
    let mut id = base.clone();
    let mut n = 1u32;
    while s.doc.images.iter().any(|a| a.id == id) {
        n = n.saturating_add(1);
        id = format!("{base}-{n}");
    }
    s.doc.images.push(ImageAsset { id: id.clone(), media_type: media.into(), bytes });
    Ok((id, false))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let (bytes, media, (pw, ph)) = source(v)?;
    let (w, h) = match (num(v, "w")?, num(v, "h")?) {
        (Some(w), Some(h)) => (size(w), size(h)),
        (Some(w), None) => (size(w), size(w * ph / pw)),
        (None, Some(h)) => (size(h * pw / ph), size(h)),
        (None, None) => {
            // A picture larger than a quarter of the frame (half its width or height) is scaled
            // down to fit there.
            let f = s.doc.page().map(|p| p.frame()).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
            let (lw, lh) = ((f.width() / 2.0).max(16.0), (f.height() / 2.0).max(16.0));
            let k = (lw / pw).min(lh / ph).min(1.0);
            (size(pw * k), size(ph * k))
        }
    };
    let (cx, cy) = match (s.view.canvas, s.doc.page()) {
        (Some(_), _) => (f64::from(s.view.pan[0]), f64::from(s.view.pan[1])),
        (None, Some(p)) => {
            let c = p.frame().center();
            (c.x, c.y)
        }
        (None, None) => (0.0, 0.0),
    };
    let x = coord(num(v, "x")?.unwrap_or(cx - w / 2.0));
    let y = coord(num(v, "y")?.unwrap_or(cy - h / 2.0));
    let id = super::new_id(s, v, "s")?;
    let (asset, reused) = store(s, bytes, media)?;
    let mut sh = Shape::new(id.clone(), ShapeKind::Rectangle, x, y, w, h);
    sh.style.fill = Color::TRANSPARENT;
    sh.style.line = Color::TRANSPARENT;
    sh.name = "Picture".into();
    sh.image = Some(ImageRef { asset: asset.clone(), ..ImageRef::default() });
    sh.sanitize();
    page_mut(s)?.shapes.push(sh);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id, "asset": asset, "reused": reused}))
}

/// The picture shape a command acts on: `id`, else the primary selected shape, else the first
/// selected picture. It must show a picture and not be locked.
fn target(s: &Session, v: &Value) -> Result<String, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let id = match v.get("id") {
        None | Some(Value::Null) => s
            .sel
            .primary
            .iter()
            .chain(s.sel.items.iter())
            .find(|id| page.shape(id).is_some_and(is_picture))
            .cloned()
            .ok_or_else(|| EngineError::Params("select a picture first (or pass `id`)".into()))?,
        Some(_) => p::req_str(v, "id")?.to_string(),
    };
    let sh = page.shape(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(&id))))?;
    if !is_picture(sh) {
        return Err(EngineError::Params(format!("`{}` is not a picture", short(&id))));
    }
    super::layer::ensure_unlocked(s, [id.as_str()])?;
    Ok(id)
}

/// The picture of shape `id` on the active page, mutably.
fn image_mut<'a>(s: &'a mut Session, id: &str) -> Result<&'a mut ImageRef, EngineError> {
    page_mut(s)?.shape_mut(id).and_then(|sh| sh.image.as_mut()).ok_or_else(|| EngineError::Params(format!("`{}` is not a picture", short(id))))
}

fn change(s: &mut Session, v: &Value) -> CmdResult {
    let id = target(s, v)?;
    let (bytes, media, _) = source(v)?;
    let (asset, _) = store(s, bytes, media)?;
    let img = image_mut(s, &id)?;
    img.asset = asset.clone();
    img.crop = CropRect::FULL;
    drop_unused_images(s);
    with_sel(s, json!({"id": id, "asset": asset}))
}

/// Remove assets no shape on any page or master shows any more.
fn drop_unused_images(s: &mut Session) {
    let doc = &s.doc;
    let used: std::collections::HashSet<String> = doc
        .pages
        .iter()
        .flat_map(|p| p.shapes.iter())
        .chain(doc.masters.iter().flat_map(|m| m.shapes.iter()))
        .filter_map(|sh| sh.image.as_ref().map(|i| i.asset.clone()))
        .collect();
    s.doc.images.retain(|a| used.contains(&a.id));
}

fn crop(s: &mut Session, v: &Value) -> CmdResult {
    let id = target(s, v)?;
    let c = v.get("crop").filter(|c| c.is_object()).ok_or_else(|| EngineError::Params("`crop` ({x, y, w, h} fractions) is required".into()))?;
    let mut rect =
        CropRect { x: num(c, "x")?.unwrap_or(0.0), y: num(c, "y")?.unwrap_or(0.0), w: num(c, "w")?.unwrap_or(1.0), h: num(c, "h")?.unwrap_or(1.0) };
    rect.sanitize();
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Err(EngineError::Params("the crop leaves nothing to show".into()));
    }
    image_mut(s, &id)?.crop = rect;
    with_sel(s, json!({"id": id, "crop": rect}))
}

fn adjust(s: &mut Session, v: &Value) -> CmdResult {
    let id = target(s, v)?;
    let recolor = match opt_str(v, "recolor")? {
        Some(r) => Some(
            Recolor::parse(r)
                .ok_or_else(|| EngineError::Params(format!("unknown recolor `{}`; use none, grayscale, sepia or a colour", short(r))))?,
        ),
        None => None,
    };
    let (b, c, t) = (num(v, "brightness")?, num(v, "contrast")?, num(v, "transparency")?);
    let img = image_mut(s, &id)?;
    let mut a: Adjust = img.adjust;
    if let Some(b) = b {
        a.brightness = b;
    }
    if let Some(c) = c {
        a.contrast = c;
    }
    if let Some(t) = t {
        a.transparency = t;
    }
    if let Some(r) = recolor {
        a.recolor = r;
    }
    a.sanitize();
    img.adjust = a;
    with_sel(s, json!({"id": id, "adjust": a}))
}

fn reset(s: &mut Session, v: &Value) -> CmdResult {
    let id = target(s, v)?;
    let img = image_mut(s, &id)?;
    img.crop = CropRect::FULL;
    img.adjust = Adjust::default();
    with_sel(s, json!({"id": id}))
}

fn compress(s: &mut Session, v: &Value) -> CmdResult {
    let max = match num(v, "maxDimension")? {
        Some(m) if m >= 1.0 => m.min(f64::from(pic::MAX_DECODE_SIDE)) as u32,
        Some(_) => return Err(EngineError::Params("`maxDimension` must be at least 1".into())),
        None => COMPRESS_MAX_DIMENSION,
    };
    let quality = match num(v, "quality")? {
        Some(q) => q.clamp(1.0, 100.0) as u8,
        None => COMPRESS_QUALITY,
    };
    let assets: Vec<String> = if opt_bool(v, "all")? == Some(true) {
        s.doc.images.iter().map(|a| a.id.clone()).collect()
    } else {
        let ids = targets(s, v, Kinds::Shapes)?;
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let mut out: Vec<String> = Vec::new();
        for id in &ids {
            if let Some(a) = page.shape(id).and_then(|sh| sh.image.as_ref()).map(|i| i.asset.clone())
                && !out.contains(&a)
            {
                out.push(a);
            }
        }
        if out.is_empty() {
            return Err(EngineError::Params("select a picture first (or pass `ids` or `all`)".into()));
        }
        out
    };
    let mut done = Vec::new();
    let mut unchanged = Vec::new();
    for id in assets {
        let Some(asset) = s.doc.images.iter_mut().find(|a| a.id == id) else { continue };
        match pic::compress(asset, max, quality) {
            Some(c) => {
                done.push(json!({"asset": id, "before": asset.bytes.len(), "after": c.bytes.len(), "mediaType": c.media_type}));
                asset.media_type = c.media_type.into();
                asset.bytes = c.bytes;
            }
            None => unchanged.push(id),
        }
    }
    with_sel(s, json!({"compressed": done, "unchanged": unchanged}))
}

#[cfg(test)]
mod tests;
