//! Template previews for the gallery: the template drawn in a scratch document with the choices
//! the gallery shows, exported to PNG through the engine (the same drawing path as File > Export),
//! shrunk and kept as a texture.
//!
//! Pipeline: [`template::resolved`] gives the variant and sheet, [`template::build`] draws the
//! document, `export_page_bytes` rasterises its page so the longest side is about twice the
//! card's preview width in physical pixels, and the PNG is decoded and shrunk into a texture.
//!
//! Budget: at most one build per frame, so opening the gallery never stalls; a card still waiting
//! shows a drawn placeholder and asks for another frame. A failed build is remembered as failed
//! and shows "No preview". The cache lives in egui's memory (one per window, as the picture cache
//! does), keyed by template, variant, sheet and catalog address, and keeps at most [`KEEP`]
//! entries, dropping the one drawn longest ago.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, ColorImage, Rect, Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2};
use vizcraft_diagram::standards::Standards;
use vizcraft_engine::cmd::template::{self, Choices};

use crate::theme::{Tokens, regular};

/// Entries kept before the one drawn longest ago is dropped.
pub const KEEP: usize = 64;

/// Longest side of a preview texture, in pixels.
const MAX_SIDE: f32 = 1024.0;

/// What one preview is drawn from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    template: String,
    variant: String,
    sheet: String,
    catalog: usize,
}

struct Entry {
    /// `None` when the build failed.
    texture: Option<TextureHandle>,
    last: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    /// The frame the last build ran in (one build per frame).
    built: Option<u64>,
}

type Shared = Arc<Mutex<Cache>>;

fn cache(ctx: &egui::Context) -> Shared {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Shared>(egui::Id::new("vizcraft.galleryPreviews")).clone())
}

/// The address of the notation catalog: loading a pack replaces it, which retires every preview.
pub fn catalog_address() -> usize {
    std::ptr::from_ref(vizcraft_notation::catalog()) as usize
}

/// What a preview call drew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// The preview texture.
    Ready,
    /// The placeholder: the build waits for a later frame.
    Pending,
    /// "No preview": the template failed to build.
    Failed,
}

/// Textures cached (built previews), for tests.
pub fn textures(ctx: &egui::Context) -> usize {
    cache(ctx).lock().map(|c| c.entries.values().filter(|e| e.texture.is_some()).count()).unwrap_or(0)
}

/// Draw template `id`'s preview into `r` with the person's `choices` and the project's
/// `standards`, building it first when this frame's build is still free. `pack` names the
/// placeholder.
pub fn draw(ui: &Ui, r: Rect, id: &str, choices: &Choices, standards: &Standards, pack: &str) -> Drawn {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let Ok((variant, sheet)) = template::resolved(id, choices, Some(standards)) else {
        no_preview(ui, r, &t);
        return Drawn::Failed;
    };
    let key = Key { template: id.to_string(), variant, sheet, catalog: catalog_address() };
    let frame = ctx.cumulative_frame_nr();
    let shared = cache(&ctx);
    let Ok(mut c) = shared.lock() else {
        placeholder(ui, r, pack, &t);
        return Drawn::Pending;
    };
    if !c.entries.contains_key(&key) {
        if c.built == Some(frame) {
            drop(c);
            placeholder(ui, r, pack, &t);
            ctx.request_repaint();
            return Drawn::Pending;
        }
        c.built = Some(frame);
        let px = (r.width().max(r.height()) * 2.0 * ctx.pixels_per_point()).clamp(32.0, MAX_SIDE);
        let texture = build(&ctx, &key, px);
        if c.entries.len() >= KEEP
            && let Some(oldest) = c.entries.iter().min_by_key(|(_, e)| e.last).map(|(k, _)| k.clone())
        {
            c.entries.remove(&oldest);
        }
        c.entries.insert(key.clone(), Entry { texture, last: frame });
    }
    let texture = match c.entries.get_mut(&key) {
        Some(e) => {
            e.last = frame;
            e.texture.clone()
        }
        None => None,
    };
    drop(c);
    match texture {
        Some(tex) => {
            paint(ui, r, &tex, &t);
            Drawn::Ready
        }
        None => {
            no_preview(ui, r, &t);
            Drawn::Failed
        }
    }
}

/// Build the preview texture of `key`, its longest side `px` pixels; `None` when the template
/// fails or the picture cannot be made.
fn build(ctx: &egui::Context, key: &Key, px: f32) -> Option<TextureHandle> {
    let (_, t) = template::lookup(&key.template).ok()?;
    let choices = Choices { variant: Some(key.variant.clone()), sheet: Some(key.sheet.clone()) };
    let doc = template::build(t, &choices).ok()?;
    let index = doc.active_index();
    let frame = doc.pages.get(index)?.frame();
    let longest = frame.width().max(frame.height());
    let scale = if longest.is_finite() && longest > 0.0 { f64::from(px) / longest } else { 1.0 };
    let png = vizcraft_engine::io::export_page_bytes("p.png", None, &doc, index, scale).ok()?;
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let k = (px / w.max(h).max(1) as f32).min(1.0);
    let (tw, th) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
    let small = if (tw, th) == (w, h) { img } else { image::imageops::thumbnail(&img, tw, th) };
    let image = ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], small.as_raw());
    Some(ctx.load_texture(format!("gallery-{}-{}-{}", key.template, key.variant, key.sheet), image, TextureOptions::LINEAR))
}

/// The texture fitted into `r`, keeping its proportions, on the page colour.
fn paint(ui: &Ui, r: Rect, tex: &TextureHandle, t: &Tokens) {
    ui.painter().rect_filled(r, 3.0, Color32::WHITE);
    let [w, h] = tex.size();
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let k = (r.width() / w).min(r.height() / h);
    let fit = Rect::from_center_size(r.center(), vec2(w * k, h * k));
    ui.painter().image(tex.id(), fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
}

/// The waiting card: the pack's name on the panel fill.
pub fn placeholder(ui: &Ui, r: Rect, pack: &str, t: &Tokens) {
    ui.painter().rect(r, 3.0, t.panel, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    ui.painter().with_clip_rect(r).text(r.center(), Align2::CENTER_CENTER, pack, regular(12.0), t.text_dim);
}

fn no_preview(ui: &Ui, r: Rect, t: &Tokens) {
    ui.painter().rect(r, 3.0, t.panel, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, "No preview", regular(12.0), t.text_dim);
}
