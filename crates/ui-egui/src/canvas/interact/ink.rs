//! The Pen, Highlighter and Eraser tools (Review > Ink and the Ink tab).
//!
//! Pen and Highlighter collect the points of one drag (in world coordinates, a point every half
//! screen point or so) and commit one `ink.add` on release, so a stroke is one undo step; the
//! stroke in progress is drawn over the page. The Eraser collects the strokes under a click or a
//! drag (marked while dragging) and commits one `ink.erase`. The pen's colour and thickness are
//! app-wide settings, as a real pen keeps its ink between documents; `ui.inkPen` sets them.

use std::sync::Mutex;

use egui::{Color32, Pos2, Response, Stroke, Ui};
use serde_json::{Value, json};
use vizcraft_diagram::geom::distance_to_polyline;
use vizcraft_diagram::ink::{self, is_stroke};
use vizcraft_diagram::{Color, Point};

use super::HIT;
use crate::canvas::Viewport2D;
use crate::canvas::draw::{color, point, pos};
use crate::{CraftApp, Tool};

/// The Pen and Highlighter settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSettings {
    /// Pen colour.
    pub color: Color,
    /// Highlighter colour.
    pub highlighter: Color,
    /// Pen thickness in world units.
    pub width: f64,
}

/// Thicknesses offered by the Thickness menu.
pub const WIDTHS: &[f64] = &[1.0, 2.0, 3.5, 5.0, 8.0];

static SETTINGS: Mutex<PenSettings> = Mutex::new(PenSettings {
    color: vizcraft_engine::cmd::ink::PEN_COLOR,
    highlighter: vizcraft_engine::cmd::ink::HIGHLIGHTER_COLOR,
    width: ink::PEN_WIDTH,
});

/// The current pen settings.
pub fn settings() -> PenSettings {
    *SETTINGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn set(f: impl FnOnce(&mut PenSettings)) {
    f(&mut SETTINGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
}

/// What one drag has collected so far.
#[derive(Clone, Debug, Default, PartialEq)]
struct Draft {
    points: Vec<Point>,
    erase: Vec<String>,
    /// Where the eraser was last frame.
    last: Option<Point>,
}

fn key() -> egui::Id {
    egui::Id::new("vizcraft.inkDraft")
}

/// `ui.inkPen`: set the colour (of the highlighter while it is the tool, else of the pen) and the
/// pen's thickness. `None` for any other id.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "ui.inkPen" {
        return None;
    }
    if let Some(c) = p.get("color").and_then(Value::as_str) {
        let Some(c) = Color::parse(c) else { return Some(Err(format!("`{}` is not a colour", c.chars().take(32).collect::<String>()))) };
        let c = Color { a: 255, ..c };
        let highlighter = app.tool == Tool::Highlighter;
        set(|s| if highlighter { s.highlighter = c } else { s.color = c });
    }
    if let Some(w) = p.get("width").and_then(Value::as_f64).filter(|w| w.is_finite()) {
        set(|s| s.width = w.clamp(0.25, 100.0));
    }
    let s = settings();
    Some(Ok(json!({"color": s.color.to_hex(), "highlighterColor": s.highlighter.to_hex(), "width": s.width})))
}

/// The ink strokes on the active page within reach of `w`.
fn strokes_at(app: &CraftApp, w: Point, zoom: f32) -> Vec<String> {
    let Some(page) = app.session.doc.page() else { return Vec::new() };
    let tol = f64::from(HIT / zoom.max(0.01));
    page.shapes
        .iter()
        .filter(|s| is_stroke(s) && page.shape_visible(s) && s.bounds().inflate(s.style.line_width + tol).contains(w))
        .filter(|s| distance_to_polyline(w, &ink::stroke_points(s)) <= s.style.line_width / 2.0 + tol)
        .map(|s| s.id.clone())
        .collect()
}

/// Pointer handling for the ink tools; true when one of them is the tool (it takes every press).
pub fn pointer(app: &mut CraftApp, ui: &Ui, response: &Response, vp: &Viewport2D) -> bool {
    let tool = app.tool;
    if !matches!(tool, Tool::Pen | Tool::Highlighter | Tool::Eraser) {
        return false;
    }
    let ctx = ui.ctx().clone();
    let pan = ui.input(|i| i.modifiers.command || i.key_down(egui::Key::Space));
    let mut draft: Option<Draft> = ctx.data(|d| d.get_temp::<Draft>(key()));
    if response.drag_started_by(egui::PointerButton::Primary) && !pan {
        let o = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos());
        let mut d = Draft::default();
        if let Some(o) = o {
            let w = point(vp.screen_to_world(o));
            if tool == Tool::Eraser {
                d.erase = strokes_at(app, w, vp.zoom);
                d.last = Some(w);
            } else {
                d.points.push(w);
            }
        }
        draft = Some(d);
    }
    if let Some(d) = draft.as_mut()
        && response.dragged_by(egui::PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        let w = point(vp.screen_to_world(p));
        if tool == Tool::Eraser {
            // Every point of the sweep since the last frame, a hit tolerance apart, so a fast
            // swipe across a thin stroke still catches it.
            let from = d.last.unwrap_or(w);
            let step = f64::from(HIT / vp.zoom.max(0.01));
            let n = ((from.distance(w) / step).ceil() as usize).clamp(1, 256);
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let at = Point::new(from.x + (w.x - from.x) * t, from.y + (w.y - from.y) * t);
                for id in strokes_at(app, at, vp.zoom) {
                    if !d.erase.contains(&id) {
                        d.erase.push(id);
                    }
                }
            }
            d.last = Some(w);
        } else if d.points.last().is_none_or(|l| l.distance(w) * f64::from(vp.zoom) >= 0.75) && d.points.len() < ink::MAX_INPUT_POINTS {
            d.points.push(w);
        }
    }
    let released = response.drag_stopped();
    if released && let Some(d) = draft.take() {
        commit(app, tool, d);
    }
    if response.clicked_by(egui::PointerButton::Primary)
        && tool == Tool::Eraser
        && let Some(p) = response.interact_pointer_pos()
    {
        let hit = strokes_at(app, point(vp.screen_to_world(p)), vp.zoom);
        commit(app, tool, Draft { erase: hit, ..Draft::default() });
    }
    match &draft {
        Some(d) => {
            preview(&ctx, app, vp, tool, d);
            ctx.data_mut(|m| m.insert_temp(key(), d.clone()));
        }
        None => ctx.data_mut(|m| {
            m.remove::<Draft>(key());
        }),
    }
    if response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    true
}

fn commit(app: &mut CraftApp, tool: Tool, d: Draft) {
    if tool == Tool::Eraser {
        if !d.erase.is_empty() {
            let _ = app.run("ink.erase", json!({"ids": d.erase}));
        }
        return;
    }
    if d.points.is_empty() {
        return;
    }
    let s = settings();
    let pts: Vec<[f64; 2]> = d.points.iter().map(|p| [p.x, p.y]).collect();
    let params = if tool == Tool::Highlighter {
        json!({"points": pts, "highlighter": true, "color": s.highlighter.to_hex()})
    } else {
        json!({"points": pts, "width": s.width, "color": s.color.to_hex()})
    };
    let _ = app.run("ink.add", params);
}

/// The stroke being drawn, or the strokes about to be erased, over the page.
fn preview(ctx: &egui::Context, app: &CraftApp, vp: &Viewport2D, tool: Tool, d: &Draft) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("vizcraft.inkPreview"))).with_clip_rect(vp.screen_rect);
    let s = settings();
    match tool {
        Tool::Eraser => {
            let Some(page) = app.session.doc.page() else { return };
            for sh in d.erase.iter().filter_map(|id| page.shape(id)) {
                let pts: Vec<Pos2> = ink::stroke_points(sh).into_iter().map(|p| vp.world_to_screen(pos(p))).collect();
                let w = (sh.style.line_width as f32 * vp.zoom).max(2.0);
                painter.add(egui::Shape::line(pts, Stroke::new(w + 2.0, Color32::from_rgba_unmultiplied(220, 60, 60, 160))));
            }
        }
        _ => {
            let (c, w) = if tool == Tool::Highlighter {
                (color(s.highlighter).gamma_multiply(0.5), ink::HIGHLIGHTER_WIDTH)
            } else {
                (color(s.color), s.width)
            };
            let pts: Vec<Pos2> = d.points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            let width = (w as f32 * vp.zoom).max(1.0);
            if let [only] = pts.as_slice() {
                painter.circle_filled(*only, width / 2.0, c);
            } else {
                painter.add(egui::Shape::line(pts, Stroke::new(width, c)));
            }
        }
    }
}
