//! Drawing pictures: the picture of a shape (`Shape.image`), drawn just under the shape so its
//! outline (the picture's border) shows on top, as SVG export draws it.
//!
//! A picture is rasterised by `vizcraft_engine::cmd::picture::raster`, the same SVG element the
//! export writes (crop, colour matrix), so the canvas matches an exported page. Rasters live in a
//! texture cache keyed by the picture's content, its crop and adjustments and the drawn size
//! (rounded to steps of a half power of two of the zoom, so a small zoom change reuses the
//! texture); entries not drawn for a few seconds are dropped. The cache is kept in egui's memory,
//! one per window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::epaint::Mesh;
use egui::{Color32, ColorImage, Painter, Pos2, Shape as EShape, Stroke, TextureHandle, TextureOptions, pos2};
use serde_json::{Value, json};
use vizcraft_diagram::{Document, Point, Shape};

use super::pos;
use crate::CraftApp;
use crate::canvas::Viewport2D;

/// Longest side of a picture texture, in pixels.
const MAX_SIDE: f32 = 2048.0;
/// Textures kept before unused ones are dropped.
const KEEP: usize = 48;
/// Frames an unused texture survives once the cache is over [`KEEP`].
const STALE_FRAMES: u64 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    content: u64,
    crop: [u64; 4],
    adjust: [u64; 4],
    px: u32,
    py: u32,
}

struct Entry {
    texture: Option<TextureHandle>,
    last: u64,
}

/// The texture cache and the content hash of each asset, remembered by the address and length of
/// its bytes so the hash runs once per picture, not once per frame.
#[derive(Default)]
struct Cache {
    textures: HashMap<Key, Entry>,
    hashes: HashMap<String, (usize, usize, u64)>,
}

type Shared = Arc<Mutex<Cache>>;

fn cache(ctx: &egui::Context) -> Shared {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Shared>(egui::Id::new("vizcraft.pictures")).clone())
}

/// FNV-1a over the bytes.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// Textures cached, for tests and the performance trace.
pub fn cached(ctx: &egui::Context) -> usize {
    cache(ctx).lock().map(|c| c.textures.len()).unwrap_or(0)
}

/// Draw the picture of `s`, if it shows one from `doc`'s store.
pub fn picture(painter: &Painter, vp: &Viewport2D, doc: &Document, s: &Shape) {
    let Some(img) = &s.image else { return };
    let Some(asset) = doc.image(&img.asset) else { return };
    if !(s.w > 0.0 && s.h > 0.0) {
        return;
    }
    let ctx = painter.ctx();
    // Rasterise at the drawn size in physical pixels, the zoom rounded to half powers of two.
    let zoom = vp.zoom.max(0.01) * ctx.pixels_per_point();
    let step = 2f32.powf((zoom.log2() * 2.0).ceil() / 2.0);
    let (mut px, mut py) = ((s.w as f32 * step).ceil().max(1.0), (s.h as f32 * step).ceil().max(1.0));
    let k = (MAX_SIDE / px.max(py)).min(1.0);
    px = (px * k).max(1.0);
    py = (py * k).max(1.0);
    let shared = cache(ctx);
    let Ok(mut c) = shared.lock() else { return };
    let ptr = asset.bytes.as_ptr() as usize;
    let content = match c.hashes.get(&asset.id) {
        Some(&(p, n, h)) if p == ptr && n == asset.bytes.len() => h,
        _ => {
            let h = hash(&asset.bytes) ^ hash(asset.media_type.as_bytes());
            c.hashes.insert(asset.id.clone(), (ptr, asset.bytes.len(), h));
            h
        }
    };
    let crop = img.crop;
    let a = img.adjust;
    let tint = match a.recolor {
        vizcraft_diagram::shape_fields::Recolor::None => 0,
        vizcraft_diagram::shape_fields::Recolor::Grayscale => 1,
        vizcraft_diagram::shape_fields::Recolor::Sepia => 2,
        vizcraft_diagram::shape_fields::Recolor::Tint(t) => 3 | (u64::from(t.r) << 8) | (u64::from(t.g) << 16) | (u64::from(t.b) << 24),
    };
    let key = Key {
        content,
        crop: [crop.x.to_bits(), crop.y.to_bits(), crop.w.to_bits(), crop.h.to_bits()],
        adjust: [a.brightness.to_bits(), a.contrast.to_bits(), a.transparency.to_bits(), tint],
        px: px as u32,
        py: py as u32,
    };
    let frame = ctx.cumulative_frame_nr();
    let texture = match c.textures.get_mut(&key) {
        Some(e) => {
            e.last = frame;
            e.texture.clone()
        }
        None => {
            let texture = vizcraft_engine::cmd::picture::raster(asset, img, s.w, s.h, key.px, key.py).map(|rgba| {
                let image = ColorImage::from_rgba_premultiplied([key.px as usize, key.py as usize], &rgba);
                ctx.load_texture(format!("picture-{}-{}x{}", asset.id, key.px, key.py), image, TextureOptions::LINEAR)
            });
            c.textures.insert(key, Entry { texture: texture.clone(), last: frame });
            texture
        }
    };
    if c.textures.len() > KEEP {
        c.textures.retain(|_, e| frame.saturating_sub(e.last) < STALE_FRAMES);
    }
    drop(c);
    let t = s.transform();
    let corner = |x: f64, y: f64| vp.world_to_screen(pos(t.apply(Point::new(x, y))));
    let quad = [corner(0.0, 0.0), corner(s.w, 0.0), corner(s.w, s.h), corner(0.0, s.h)];
    match texture {
        Some(tex) => {
            let mut mesh = Mesh::with_texture(tex.id());
            let uv = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
            for (p, u) in quad.iter().zip(uv) {
                mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv: u, color: Color32::WHITE });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(EShape::mesh(mesh));
        }
        // An unreadable picture shows as a crossed box, so it can still be found and replaced.
        None => missing(painter, &quad),
    }
}

fn missing(painter: &Painter, quad: &[Pos2; 4]) {
    let stroke = Stroke::new(1.0, Color32::from_gray(160));
    painter.add(EShape::closed_line(quad.to_vec(), stroke));
    painter.line_segment([quad[0], quad[2]], stroke);
    painter.line_segment([quad[1], quad[3]], stroke);
}

/// `ui.insertPicture` and `ui.changePicture`: pick a file, then run `picture.insert` or
/// `picture.change` with its path. `None` for any other id.
pub fn ui_command(app: &mut CraftApp, id: &str, _p: &Value) -> Option<Result<Value, String>> {
    let cmd = match id {
        "ui.insertPicture" => "picture.insert",
        "ui.changePicture" => "picture.change",
        _ => return None,
    };
    // Without a file picker (the web build, tests) nothing is asked and nothing changes.
    let Some(pick) = app.services.pick_open.as_ref() else {
        app.status("This front end cannot pick a picture file yet");
        return Some(Ok(json!({"picked": false})));
    };
    let Some(path) = pick("picture") else { return Some(Ok(json!({"picked": false}))) };
    Some(app.run(cmd, json!({"path": path})))
}
