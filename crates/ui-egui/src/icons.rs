//! The icon set, drawn in code: original artwork, no icon assets to attribute.
//!
//! Icons are designed on a 20 by 20 grid in two colours: line work (`c`) and an accent (`a`).
//! They are painted at three sizes: 16 (small buttons, menus), 20 (title and status bar) and
//! 32 (big ribbon buttons). The stroke grows with the size but less than linearly, so a 32 px
//! icon reads as a drawing rather than a thick blob and a 16 px one stays legible (see
//! [`stroke_width`]). A name that is not known draws a tile with the first letter, so a missing
//! icon is visible rather than a hole.

use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Every icon [`paint`] knows, for tests and the gallery.
pub const NAMES: &[&str] = &[
    // File
    "new",
    "open",
    "save",
    "saveAs",
    "print",
    // Edit
    "undo",
    "redo",
    "cut",
    "copy",
    "paste",
    "selectAll",
    "find",
    // View
    "zoom",
    "zoomIn",
    "zoomOut",
    "zoomFit",
    "panelLeft",
    "panelRight",
    "dark",
    "menuBar",
    "ribbon",
    // App
    "settings",
    "info",
    "help",
    "note",
    "noteNew",
    "noteDelete",
    "commands",
    // Glyphs
    "plus",
    "minus",
    "close",
    "check",
    "chevronDown",
    "chevronUp",
    "chevronLeft",
    "chevronRight",
    "dropdown",
    "more",
    "search",
];

/// Reference size of the design grid, in points. At this size the stroke is `BASE_STROKE`.
pub const GRID: f32 = 20.0;
/// Stroke width at the reference size.
pub const BASE_STROKE: f32 = 1.25;

/// Stroke width for an icon drawn `size` points wide: 1.06 at 16, 1.25 at 20, 1.78 at 32.
pub fn stroke_width(size: f32) -> f32 {
    BASE_STROKE * (size / GRID).max(0.1).powf(0.75)
}

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    c: Color32,
    w: f32,
}

impl Pen<'_> {
    /// Grid units to points.
    fn s(&self) -> f32 {
        self.r.width().min(self.r.height()) / GRID
    }
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        let s = self.s();
        let o = self.r.center() - vec2(10.0 * s, 10.0 * s);
        pos2(o.x + x * s, o.y + y * s)
    }
    fn stroke(&self, c: Color32) -> Stroke {
        Stroke::new(self.w, c)
    }
    fn line(&self, pts: &[(f32, f32)]) {
        self.line_c(pts, self.c);
    }
    fn line_c(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(v, self.stroke(c)));
    }
    fn closed(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::closed_line(v, self.stroke(c)));
    }
    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::convex_polygon(v, c, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_stroke(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), self.s(), self.stroke(c), egui::StrokeKind::Middle);
    }
    fn frect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 0.5 * self.s(), c);
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s(), self.stroke(c));
    }
    fn fcircle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.s(), c);
    }
    /// Horizontal text lines (a paragraph).
    fn lines(&self, x0: f32, x1: f32, ys: &[f32]) {
        for y in ys {
            self.line(&[(x0, *y), (x1, *y)]);
        }
    }
    /// A page outline with a folded corner.
    fn page(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.closed(&[(x0, y0), (x1 - 4.0, y0), (x1, y0 + 4.0), (x1, y1), (x0, y1)], self.c);
        self.line(&[(x1 - 4.0, y0), (x1 - 4.0, y0 + 4.0), (x1, y0 + 4.0)]);
    }
    /// A magnifier with the lens centred at (8.5, 8.5); the handle is drawn in the accent.
    fn magnifier(&self, a: Color32) {
        self.circle(8.5, 8.5, 5.5, self.c);
        self.line_c(&[(12.5, 12.5), (17.5, 17.5)], a);
    }
    /// An arc of radius `rad` around (cx, cy), from angle `from` sweeping `dir` radians.
    fn arc(&self, cx: f32, cy: f32, rad: f32, from: f32, dir: f32, c: Color32) {
        let pts = (0..=12)
            .map(|i| {
                let t = from + dir * i as f32 / 12.0;
                self.pt(cx + rad * t.cos(), cy + rad * t.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke(c)));
    }
    /// A window frame with a title band.
    fn window(&self) {
        self.rect(3.0, 3.5, 17.0, 16.5, self.c);
        self.line(&[(3.0, 6.5), (17.0, 6.5)]);
    }
    /// A chevron pointing in direction (dx, dy) with its tip at (x, y).
    fn chevron(&self, x: f32, y: f32, dx: f32, dy: f32) {
        // Perpendicular to the direction, 5 units each side, 5 units back.
        let (px, py) = (-dy, dx);
        self.line(&[(x - dx * 5.0 + px * 5.0, y - dy * 5.0 + py * 5.0), (x, y), (x - dx * 5.0 - px * 5.0, y - dy * 5.0 - py * 5.0)]);
    }
}

/// Paint icon `name` into `r`. `c` is the line colour, `a` the accent.
pub fn paint(p: &Painter, r: Rect, name: &str, c: Color32, a: Color32) {
    let pen = Pen { p, r, c, w: stroke_width(r.width().min(r.height())) };
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    match name {
        // File
        "new" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.line_c(&[(10.0, 8.0), (10.0, 14.0)], a);
            pen.line_c(&[(7.0, 11.0), (13.0, 11.0)], a);
        }
        "open" => {
            pen.line(&[(3.0, 16.0), (3.0, 5.0), (8.0, 5.0), (10.0, 7.0), (16.0, 7.0), (16.0, 9.5)]);
            pen.fill(&[(3.0, 16.0), (5.5, 9.5), (18.5, 9.5), (16.0, 16.0)], a);
        }
        "save" => {
            pen.closed(&[(3.0, 3.0), (14.0, 3.0), (17.0, 6.0), (17.0, 17.0), (3.0, 17.0)], c);
            pen.frect(6.0, 3.0, 13.0, 7.5, a);
            pen.rect(6.0, 11.0, 14.0, 17.0, c);
        }
        "saveAs" => {
            pen.closed(&[(3.0, 3.0), (14.0, 3.0), (17.0, 6.0), (17.0, 10.5), (12.0, 17.0), (3.0, 17.0)], c);
            pen.frect(6.0, 3.0, 13.0, 7.5, a);
            // A pencil over the corner.
            pen.line_c(&[(11.0, 18.0), (18.5, 10.5)], a);
            pen.line_c(&[(13.0, 18.5), (18.5, 13.0)], a);
        }
        "print" => {
            pen.rect(3.0, 8.0, 17.0, 15.0, c);
            pen.line(&[(6.0, 8.0), (6.0, 3.5), (14.0, 3.5), (14.0, 8.0)]);
            pen.frect(6.5, 12.5, 13.5, 18.0, a);
            pen.lines(8.0, 12.0, &[14.5, 16.0]);
        }
        // Edit
        "undo" => {
            pen.line(&[(5.0, 8.0), (13.0, 8.0)]);
            pen.arc(13.0, 12.5, 4.5, -FRAC_PI_2, PI, c);
            pen.line(&[(13.0, 17.0), (8.0, 17.0)]);
            pen.fill(&[(2.5, 8.0), (6.5, 4.5), (6.5, 11.5)], a);
        }
        "redo" => {
            pen.line(&[(15.0, 8.0), (7.0, 8.0)]);
            pen.arc(7.0, 12.5, 4.5, -FRAC_PI_2, -PI, c);
            pen.line(&[(7.0, 17.0), (12.0, 17.0)]);
            pen.fill(&[(17.5, 8.0), (13.5, 4.5), (13.5, 11.5)], a);
        }
        "cut" => {
            pen.circle(6.0, 15.0, 2.5, a);
            pen.circle(14.0, 15.0, 2.5, a);
            pen.line(&[(7.5, 13.0), (14.0, 3.0)]);
            pen.line(&[(12.5, 13.0), (6.0, 3.0)]);
        }
        "copy" => {
            pen.rect(3.5, 3.0, 12.5, 14.0, c);
            pen.rect(7.5, 6.5, 16.5, 17.5, a);
        }
        "paste" => {
            pen.rect(4.0, 3.5, 14.0, 17.0, c);
            pen.frect(7.0, 2.0, 11.0, 5.0, a);
            pen.frect(9.0, 8.0, 17.5, 18.5, a.linear_multiply(0.18));
            pen.rect(9.0, 8.0, 17.5, 18.5, a);
            pen.lines(10.8, 15.8, &[11.0, 13.5, 16.0]);
        }
        "selectAll" => {
            // A dashed selection frame around a block of text.
            for (x0, y0, x1, y1) in [
                (3.0, 3.0, 6.0, 3.0),
                (9.0, 3.0, 11.0, 3.0),
                (14.0, 3.0, 17.0, 3.0),
                (3.0, 17.0, 6.0, 17.0),
                (9.0, 17.0, 11.0, 17.0),
                (14.0, 17.0, 17.0, 17.0),
            ] {
                pen.line_c(&[(x0, y0), (x1, y1)], a);
            }
            for (x, y0, y1) in [(3.0, 3.0, 6.0), (3.0, 9.0, 11.0), (3.0, 14.0, 17.0), (17.0, 3.0, 6.0), (17.0, 9.0, 11.0), (17.0, 14.0, 17.0)] {
                pen.line_c(&[(x, y0), (x, y1)], a);
            }
            pen.lines(6.5, 13.5, &[7.0, 10.0, 13.0]);
        }
        "find" | "search" => pen.magnifier(a),
        // View
        "zoom" => {
            pen.magnifier(a);
            pen.line(&[(6.0, 8.5), (11.0, 8.5)]);
            pen.line(&[(8.5, 6.0), (8.5, 11.0)]);
        }
        "zoomIn" => {
            pen.magnifier(a);
            pen.line_c(&[(6.0, 8.5), (11.0, 8.5)], a);
            pen.line_c(&[(8.5, 6.0), (8.5, 11.0)], a);
        }
        "zoomOut" => {
            pen.magnifier(a);
            pen.line_c(&[(6.0, 8.5), (11.0, 8.5)], a);
        }
        "zoomFit" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            // Arrows from the centre to two corners.
            pen.line_c(&[(8.5, 8.5), (5.5, 5.5)], a);
            pen.line_c(&[(5.5, 8.5), (5.5, 5.5), (8.5, 5.5)], a);
            pen.line_c(&[(11.5, 11.5), (14.5, 14.5)], a);
            pen.line_c(&[(14.5, 11.5), (14.5, 14.5), (11.5, 14.5)], a);
        }
        "panelLeft" => {
            pen.rect(3.0, 3.5, 17.0, 16.5, c);
            pen.frect(3.0, 3.5, 8.0, 16.5, a.linear_multiply(0.55));
            pen.line(&[(8.0, 3.5), (8.0, 16.5)]);
        }
        "panelRight" => {
            pen.rect(3.0, 3.5, 17.0, 16.5, c);
            pen.frect(12.0, 3.5, 17.0, 16.5, a.linear_multiply(0.55));
            pen.line(&[(12.0, 3.5), (12.0, 16.5)]);
        }
        "dark" => {
            // A crescent: the outer arc of the moon's disc and an inner arc that bites into it,
            // as one closed outline.
            let outer = (0..=16).map(|i| {
                let t = 0.65 + (TAU - 1.3) * i as f32 / 16.0;
                pen.pt(10.0 + 7.0 * t.cos(), 10.0 + 7.0 * t.sin())
            });
            let inner = (0..=12).map(|i| {
                let t = -1.12 - 4.04 * i as f32 / 12.0;
                pen.pt(13.5 + 4.8 * t.cos(), 10.0 + 4.8 * t.sin())
            });
            pen.p.add(Shape::closed_line(outer.chain(inner).collect(), pen.stroke(c)));
            pen.fcircle(15.5, 4.5, 0.9, a);
        }
        "menuBar" => {
            pen.window();
            pen.lines(5.0, 15.0, &[9.5, 12.5]);
            pen.line_c(&[(5.0, 5.0), (8.0, 5.0)], a);
        }
        "ribbon" => {
            pen.window();
            pen.line(&[(3.0, 10.0), (17.0, 10.0)]);
            pen.frect(5.0, 7.5, 7.5, 9.0, a);
            pen.frect(9.0, 7.5, 11.5, 9.0, a);
        }
        // App
        "settings" => {
            pen.circle(10.0, 10.0, 3.0, c);
            for i in 0..8 {
                let t = TAU * i as f32 / 8.0;
                let (sx, sy) = (t.cos(), t.sin());
                pen.line(&[(10.0 + 5.5 * sx, 10.0 + 5.5 * sy), (10.0 + 7.5 * sx, 10.0 + 7.5 * sy)]);
            }
            pen.circle(10.0, 10.0, 5.5, c);
        }
        "info" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.line_c(&[(10.0, 9.0), (10.0, 14.5)], a);
            pen.fcircle(10.0, 6.3, 1.0, a);
        }
        "help" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.arc(10.0, 8.0, 2.5, PI, PI * 1.25, a);
            pen.line_c(&[(10.0, 10.5), (10.0, 12.0)], a);
            pen.fcircle(10.0, 14.5, 1.0, a);
        }
        "note" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.lines(7.0, 13.0, &[8.0, 11.0]);
            pen.line_c(&[(7.0, 14.0), (11.0, 14.0)], a);
        }
        "noteNew" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.lines(7.0, 13.0, &[8.0, 11.0]);
            pen.fcircle(15.0, 15.0, 4.0, a);
            let w = Color32::WHITE;
            pen.line_c(&[(15.0, 12.8), (15.0, 17.2)], w);
            pen.line_c(&[(12.8, 15.0), (17.2, 15.0)], w);
        }
        "noteDelete" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.lines(7.0, 13.0, &[8.0, 11.0]);
            pen.fcircle(15.0, 15.0, 4.0, a);
            let w = Color32::WHITE;
            pen.line_c(&[(13.2, 13.2), (16.8, 16.8)], w);
            pen.line_c(&[(16.8, 13.2), (13.2, 16.8)], w);
        }
        "commands" => {
            pen.rect(3.0, 4.0, 17.0, 16.0, c);
            pen.line_c(&[(6.0, 8.0), (8.5, 10.0), (6.0, 12.0)], a);
            pen.line(&[(10.0, 12.0), (14.0, 12.0)]);
        }
        // Glyphs
        "plus" => {
            pen.line(&[(10.0, 4.0), (10.0, 16.0)]);
            pen.line(&[(4.0, 10.0), (16.0, 10.0)]);
        }
        "minus" => pen.line(&[(4.0, 10.0), (16.0, 10.0)]),
        "close" => {
            pen.line(&[(5.0, 5.0), (15.0, 15.0)]);
            pen.line(&[(15.0, 5.0), (5.0, 15.0)]);
        }
        "check" => pen.line(&[(4.0, 10.5), (8.5, 15.0), (16.0, 6.0)]),
        "chevronDown" => pen.chevron(10.0, 13.0, 0.0, 1.0),
        "chevronUp" => pen.chevron(10.0, 7.0, 0.0, -1.0),
        "chevronLeft" => pen.chevron(7.0, 10.0, -1.0, 0.0),
        "chevronRight" => pen.chevron(13.0, 10.0, 1.0, 0.0),
        "dropdown" => pen.fill(&[(5.5, 7.5), (14.5, 7.5), (10.0, 12.5)], c),
        "more" => {
            for x in [5.0, 10.0, 15.0] {
                pen.fcircle(x, 10.0, 1.3, c);
            }
        }
        _ => {
            // Fallback: a rounded tile with the first letter.
            p.rect_stroke(r.shrink(r.width() * 0.12), 3.0, Stroke::new(1.0, c), egui::StrokeKind::Middle);
            let ch = name.chars().next().map(|x| x.to_ascii_uppercase().to_string()).unwrap_or_default();
            p.text(r.center(), egui::Align2::CENTER_CENTER, ch, FontId::proportional(r.height() * 0.45), a);
        }
    }
}

/// Whether `name` is a drawn icon (not the letter-tile fallback).
pub fn known(name: &str) -> bool {
    NAMES.contains(&name)
}
