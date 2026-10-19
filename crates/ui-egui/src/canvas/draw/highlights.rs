//! Analysis highlights (chrome, in `theme::Tokens` colours): the stored result of an
//! `analyze.*` command or `model.diff` (`Session.analysis`) drawn over the page.
//!
//! Each marked shape gets a rounded halo and each marked connector a band along its route, in the
//! mark's colour, with a chip that says what the mark means (a level number, a step, "Cycle") so
//! colour is never the only signal. With Dim on, everything the result does not mark is washed
//! out. A result taken before the latest edit draws faded. A compare also draws ghosts: the items
//! the compared file had and the model has lost, as dashed red outlines where that file drew
//! them, their names struck through.
//!
//! [`targets`] and [`ghosts`] are pure, so `ui.inspect` reports exactly what is drawn.

use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use serde_json::Value;
use vizcraft_analysis::{AnalysisResult, Mark};
use vizcraft_diagram::{Page, route_polyline};
use vizcraft_engine::Session;

use super::pos;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::theme::{Tokens, regular};

/// Most points one ghost route keeps (the engine caps them at the same number).
const MAX_GHOST_POINTS: usize = 256;
/// Below this zoom the chips are left out: they would cover the shapes they label.
const CHIP_MIN_ZOOM: f32 = 0.4;

/// One drawing item the result marks.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The shape or connector id on the page.
    pub item: String,
    /// The element or relationship it draws.
    pub id: String,
    /// How it is marked.
    pub mark: Mark,
    /// The chip text: the level for dependencies, the step for a path, else the mark's name.
    pub chip: Option<String>,
    /// It is a connector (a band), not a shape (a halo).
    pub connector: bool,
}

/// One item the compared file drew and the current model no longer has.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    /// The element or relationship id.
    pub id: String,
    /// Its name in the compared file, else its id.
    pub name: String,
    /// The page id the compared file drew it on.
    pub page: String,
    /// That page's name.
    pub page_name: String,
    /// A shape's bounds `[x, y, w, h]`.
    pub rect: Option<vizcraft_diagram::Rect>,
    /// A connector's route.
    pub points: Vec<vizcraft_diagram::Point>,
}

impl Ghost {
    /// The world point to show it at: the middle of its bounds or route.
    pub fn centre(&self) -> Option<vizcraft_diagram::Point> {
        self.rect.map(|r| r.center()).or_else(|| mid(&self.points))
    }
}

/// The middle point along a polyline, by length.
fn mid(pts: &[vizcraft_diagram::Point]) -> Option<vizcraft_diagram::Point> {
    let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = len / 2.0;
    for w in pts.windows(2) {
        let d = w[0].distance(w[1]);
        if d >= left && d > 0.0 {
            let t = left / d;
            return Some(vizcraft_diagram::Point::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t));
        }
        left -= d;
    }
    pts.first().copied()
}

/// The token colour of a mark.
pub fn mark_color(t: &Tokens, m: Mark) -> Color32 {
    match m {
        Mark::Focus | Mark::Path => t.accent,
        Mark::Upstream => t.blue,
        Mark::Downstream | Mark::Changed => t.amber,
        Mark::Cycle | Mark::Cut | Mark::Removed => t.red,
        Mark::Unreachable => t.violet,
        Mark::Added => t.green,
    }
}

/// The short name a chip or legend gives a mark.
pub fn mark_name(m: Mark) -> &'static str {
    match m {
        Mark::Focus => "Start",
        Mark::Upstream => "Upstream",
        Mark::Downstream => "Downstream",
        Mark::Cycle => "Cycle",
        Mark::Unreachable => "Unreachable",
        Mark::Cut => "Cut",
        Mark::Path => "Path",
        Mark::Added => "New",
        Mark::Removed => "Removed",
        Mark::Changed => "Changed",
    }
}

/// What the legend calls a mark in a result of `kind`: the links a dependencies walk followed
/// and a metrics critical path are both marked Path, which would read as a path search there.
pub fn legend_name(kind: &str, m: Mark) -> &'static str {
    match (kind, m) {
        ("dependencies", Mark::Path) => "Link followed",
        ("metrics", Mark::Path) => "Critical path",
        _ => mark_name(m),
    }
}

/// The level of each element a dependencies result reached (`detail.levels`).
fn levels(r: &AnalysisResult) -> std::collections::HashMap<&str, usize> {
    let mut out = std::collections::HashMap::new();
    if let Some(ls) = r.detail.get("levels").and_then(Value::as_array) {
        for (i, l) in ls.iter().enumerate() {
            for id in l.as_array().into_iter().flatten().filter_map(Value::as_str) {
                out.entry(id).or_insert(i);
            }
        }
    }
    out
}

/// The chip of `id` under `r`.
fn chip(r: &AnalysisResult, id: &str, mark: Mark, lv: &std::collections::HashMap<&str, usize>, connector: bool) -> Option<String> {
    match r.kind.as_str() {
        // Links walked carry no level; their band and colour say what they are.
        "dependencies" if connector => None,
        "dependencies" => lv.get(id).map(|n| n.to_string()),
        "path" if connector => None,
        "path" => r.elements.iter().position(|e| e == id).map(|n| n.to_string()),
        // A metrics result marks its critical path; the table carries the numbers.
        "metrics" if connector => None,
        "metrics" => Some("Critical".into()),
        _ => Some(mark_name(mark).to_string()),
    }
}

/// Every shape and connector of `page` that draws an item the stored result marks, in page
/// order. Ids the model no longer has are skipped, so a result taken before a delete marks
/// nothing in its place.
pub fn targets(session: &Session, page: &Page) -> Vec<Target> {
    let Some(r) = &session.analysis else { return Vec::new() };
    let model = &session.doc.model;
    let lv = levels(r);
    let marked = |id: Option<&String>| id.and_then(|id| r.marks.get(id).map(|m| (id.clone(), *m))).filter(|(id, _)| model.contains_id(id));
    let mut out = Vec::new();
    for s in &page.shapes {
        if let Some((id, mark)) = marked(s.element.as_ref()) {
            let chip = chip(r, &id, mark, &lv, false);
            out.push(Target { item: s.id.clone(), id, mark, chip, connector: false });
        }
    }
    for c in &page.connectors {
        if let Some((id, mark)) = marked(c.relationship.as_ref()) {
            let chip = chip(r, &id, mark, &lv, true);
            out.push(Target { item: c.id.clone(), id, mark, chip, connector: true });
        }
    }
    out
}

/// Every ghost of a stored compare (`detail.ghosts`), on any page.
pub fn all_ghosts(session: &Session) -> Vec<Ghost> {
    let Some(r) = session.analysis.as_ref().filter(|r| r.kind == "diff") else { return Vec::new() };
    let num = |v: &Value| v.as_f64().filter(|x| x.is_finite());
    let s = |g: &Value, k: &str| g.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    r.detail
        .get("ghosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(vizcraft_analysis::MAX_RESULT_IDS)
        .map(|g| {
            let rect = g.get("rect").and_then(Value::as_array).and_then(|a| match a.as_slice() {
                [x, y, w, h] => {
                    let (x, y, w, h) = (num(x)?, num(y)?, num(w)?, num(h)?);
                    Some(vizcraft_diagram::Rect::new(x, y, x + w.max(0.0), y + h.max(0.0)))
                }
                _ => None,
            });
            let points = g
                .get("points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(MAX_GHOST_POINTS)
                .filter_map(|p| match p.as_array().map(Vec::as_slice) {
                    Some([x, y]) => Some(vizcraft_diagram::Point::new(num(x)?, num(y)?)),
                    _ => None,
                })
                .collect();
            let id = s(g, "id");
            let name = Some(s(g, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone());
            Ghost { id, name, page: s(g, "page"), page_name: s(g, "pageName"), rect, points }
        })
        .collect()
}

/// The ghosts of a stored compare that the compared file drew on `page` (matched by page id).
pub fn ghosts(session: &Session, page: &Page) -> Vec<Ghost> {
    all_ghosts(session).into_iter().filter(|g| g.page == page.id).collect()
}

/// The stored result is older than the document.
pub fn stale(session: &Session) -> bool {
    session.analysis.as_ref().is_some_and(|r| r.rev != session.rev())
}

/// How the highlights are drawn: the user's choices from the Analysis tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opts {
    /// Wash out what the result does not mark (never for a compare).
    pub dim: bool,
}

/// A dashed closed outline through `pts` (screen space).
fn dashed(painter: &Painter, mut pts: Vec<Pos2>, closed: bool, stroke: Stroke) {
    if closed && let Some(first) = pts.first().copied() {
        pts.push(first);
    }
    painter.extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0));
}

/// A world rectangle on screen.
fn on_screen(vp: &Viewport2D, b: &vizcraft_diagram::Rect) -> Rect {
    Rect::from_min_max(vp.world_to_screen(pos2(b.x0 as f32, b.y0 as f32)), vp.world_to_screen(pos2(b.x1 as f32, b.y1 as f32)))
}

fn rect_points(r: Rect) -> Vec<Pos2> {
    vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
}

/// A small rounded label with its left edge at `at.x`, centred on `at.y`.
fn chip_label(painter: &Painter, at: Pos2, text: &str, fill: Color32, t: &Tokens) {
    let galley = painter.layout_no_wrap(text.to_string(), regular(10.0), t.on_accent);
    let r = Rect::from_min_size(pos2(at.x, at.y - galley.size().y / 2.0 - 1.0), galley.size() + vec2(8.0, 2.0));
    painter.rect_filled(r, 4.0, fill);
    painter.galley(r.min + vec2(4.0, 1.0), galley, t.on_accent);
}

/// Draw the stored result over `page`, visible items only. Called after the net highlight and
/// before the data graphic marks, so the selection chrome stays on top.
pub fn draw(painter: &Painter, vp: &Viewport2D, session: &Session, page: &Page, opts: Opts, t: &Tokens) {
    let Some(r) = &session.analysis else { return };
    let visible = vp.visible_world_rect().inflate(40.0);
    let fade = if stale(session) { 0.4 } else { 1.0 };
    let list = targets(session, page);
    let diff = r.kind == "diff";
    if opts.dim && !diff {
        let marked: std::collections::HashSet<&str> = list.iter().map(|x| x.item.as_str()).collect();
        let wash = t.page.gamma_multiply(0.55 * fade);
        // A container around a marked item is not washed: its wash would fade what it holds.
        let held: Vec<vizcraft_diagram::Rect> = page.shapes.iter().filter(|s| marked.contains(s.id.as_str())).map(|s| s.bounds()).collect();
        for s in page.shapes.iter().filter(|s| !marked.contains(s.id.as_str()) && s.bounds().intersects(&visible)) {
            let b = s.bounds();
            if held.iter().any(|h| b.contains_rect(h)) {
                continue;
            }
            let sr = on_screen(vp, &b).expand(2.0);
            painter.rect_filled(sr, 0.0, wash);
        }
        for c in page.connectors.iter().filter(|c| !marked.contains(c.id.as_str())) {
            let pts = route_polyline(page, c);
            if !vizcraft_diagram::Rect::bounding(pts.iter().copied()).is_some_and(|b| b.intersects(&visible)) {
                continue;
            }
            let w = ((c.style.line_width.clamp(0.0, 100.0) as f32) * vp.zoom + 6.0).max(6.0);
            painter.add(egui::Shape::line(pts.into_iter().map(|p| vp.world_to_screen(pos(p))).collect(), Stroke::new(w, wash)));
        }
    }
    let chips = vp.zoom >= CHIP_MIN_ZOOM;
    for x in &list {
        let col = mark_color(t, x.mark).gamma_multiply(fade);
        if x.connector {
            let Some(c) = page.connector(&x.item) else { continue };
            let pts = route_polyline(page, c);
            if pts.len() < 2 || !vizcraft_diagram::Rect::bounding(pts.iter().copied()).is_some_and(|b| b.intersects(&visible)) {
                continue;
            }
            let at = mid(&pts);
            painter.add(egui::Shape::line(pts.into_iter().map(|p| vp.world_to_screen(pos(p))).collect(), Stroke::new(6.0, col.gamma_multiply(0.45))));
            if chips && let (Some(text), Some(at)) = (&x.chip, at) {
                chip_label(painter, vp.world_to_screen(pos(at)) + vec2(6.0, -10.0), text, col, t);
            }
        } else {
            let Some(s) = page.shape(&x.item) else { continue };
            let b = s.bounds();
            if !b.intersects(&visible) {
                continue;
            }
            let halo = on_screen(vp, &b).expand(4.0);
            let width = if x.mark == Mark::Focus { 3.5 } else { 2.5 };
            painter.rect_filled(halo, 6.0, col.gamma_multiply(0.12));
            if x.mark == Mark::Unreachable {
                dashed(painter, rect_points(halo), true, Stroke::new(width, col));
            } else {
                painter.rect_stroke(halo, 6.0, Stroke::new(width, col), StrokeKind::Middle);
            }
            if chips && let Some(text) = &x.chip {
                chip_label(painter, halo.left_top() + vec2(-2.0, 0.0), text, col, t);
            }
        }
    }
    if diff {
        draw_ghosts(painter, vp, session, page, &visible, fade, t);
    }
}

/// The ghosts of a compare on `page`: dashed red outlines or routes, the name struck through.
fn draw_ghosts(painter: &Painter, vp: &Viewport2D, session: &Session, page: &Page, visible: &vizcraft_diagram::Rect, fade: f32, t: &Tokens) {
    let red = t.red.gamma_multiply(fade);
    let stroke = Stroke::new(1.75, red);
    for g in ghosts(session, page) {
        let label_at = if let Some(b) = g.rect {
            if !b.intersects(visible) {
                continue;
            }
            let r = on_screen(vp, &b);
            painter.rect_filled(r, 0.0, red.gamma_multiply(0.06));
            dashed(painter, rect_points(r), true, stroke);
            r.center()
        } else {
            if g.points.len() < 2 || !vizcraft_diagram::Rect::bounding(g.points.iter().copied()).is_some_and(|b| b.intersects(visible)) {
                continue;
            }
            let pts: Vec<Pos2> = g.points.iter().map(|p| vp.world_to_screen(pos(*p))).collect();
            dashed(painter, pts, false, stroke);
            let Some(m) = mid(&g.points) else { continue };
            vp.world_to_screen(pos(m)) + vec2(0.0, -10.0)
        };
        if vp.zoom < CHIP_MIN_ZOOM {
            continue;
        }
        let r = painter.text(label_at, Align2::CENTER_CENTER, &g.name, regular(11.0), red);
        painter.line_segment([pos2(r.left() - 1.0, r.center().y), pos2(r.right() + 1.0, r.center().y)], Stroke::new(1.25, red));
    }
}

/// What the status bar shows about the stored result: "Analysis: <summary>", and whether it is
/// out of date.
pub fn status(app: &CraftApp) -> Option<String> {
    let r = app.session.analysis.as_ref()?;
    let old = if stale(&app.session) { " (out of date)" } else { "" };
    Some(format!("Analysis: {}{old}", r.summary))
}
