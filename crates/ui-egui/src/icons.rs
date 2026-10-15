//! The icon set, drawn in code: original artwork, no icon assets to attribute.
//!
//! Icons are designed on a 20 by 20 grid in two colours: line work (`c`) and an accent (`a`).
//! A name that is not known draws a tile with the first letter, so a missing icon is visible
//! rather than a hole.

use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Every icon [`paint`] knows, for tests and the gallery.
pub const NAMES: &[&str] =
    &["save", "undo", "redo", "more", "search", "plus", "minus", "close", "new", "open", "settings", "info", "dropdown", "note"];

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    c: Color32,
    w: f32,
}

impl Pen<'_> {
    fn s(&self) -> f32 {
        self.r.width().min(self.r.height()) / 20.0
    }
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        let s = self.s();
        let o = self.r.center() - vec2(10.0 * s, 10.0 * s);
        pos2(o.x + x * s, o.y + y * s)
    }
    fn stroke(&self, c: Color32) -> Stroke {
        Stroke::new(self.w * self.s(), c)
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
        self.p.rect_stroke(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 1.0, self.stroke(c), egui::StrokeKind::Middle);
    }
    fn frect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 0.5, c);
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s(), self.stroke(c));
    }
    fn fcircle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.s(), c);
    }
    /// A page outline with a folded corner.
    fn page(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.closed(&[(x0, y0), (x1 - 4.0, y0), (x1, y0 + 4.0), (x1, y1), (x0, y1)], self.c);
        self.line(&[(x1 - 4.0, y0), (x1 - 4.0, y0 + 4.0), (x1, y0 + 4.0)]);
    }
    /// A half circle of radius 4.5 around (cx, cy), from angle `from` sweeping `dir` radians.
    fn arc(&self, cx: f32, cy: f32, from: f32, dir: f32) {
        let pts = (0..=12)
            .map(|i| {
                let t = from + dir * i as f32 / 12.0;
                self.pt(cx + 4.5 * t.cos(), cy + 4.5 * t.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke(self.c)));
    }
}

/// Paint icon `name` into `r`. `c` is the line colour, `a` the accent.
pub fn paint(p: &Painter, r: Rect, name: &str, c: Color32, a: Color32) {
    let pen = Pen { p, r, c, w: 1.25 };
    match name {
        "save" => {
            pen.closed(&[(3.0, 3.0), (14.0, 3.0), (17.0, 6.0), (17.0, 17.0), (3.0, 17.0)], c);
            pen.frect(6.0, 3.0, 13.0, 7.5, a);
            pen.rect(6.0, 11.0, 14.0, 17.0, c);
        }
        "undo" => {
            pen.line(&[(5.0, 8.0), (13.0, 8.0)]);
            pen.arc(13.0, 12.5, -std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
            pen.line(&[(13.0, 17.0), (8.0, 17.0)]);
            pen.fill(&[(2.5, 8.0), (6.5, 4.5), (6.5, 11.5)], a);
        }
        "redo" => {
            pen.line(&[(15.0, 8.0), (7.0, 8.0)]);
            pen.arc(7.0, 12.5, -std::f32::consts::FRAC_PI_2, -std::f32::consts::PI);
            pen.line(&[(7.0, 17.0), (12.0, 17.0)]);
            pen.fill(&[(17.5, 8.0), (13.5, 4.5), (13.5, 11.5)], a);
        }
        "more" => {
            for x in [5.0, 10.0, 15.0] {
                pen.fcircle(x, 10.0, 1.3, c);
            }
        }
        "search" => {
            pen.circle(8.5, 8.5, 5.0, c);
            pen.line(&[(12.0, 12.0), (16.5, 16.5)]);
        }
        "plus" => {
            pen.line(&[(10.0, 4.0), (10.0, 16.0)]);
            pen.line(&[(4.0, 10.0), (16.0, 10.0)]);
        }
        "minus" => pen.line(&[(4.0, 10.0), (16.0, 10.0)]),
        "close" => {
            pen.line(&[(5.0, 5.0), (15.0, 15.0)]);
            pen.line(&[(15.0, 5.0), (5.0, 15.0)]);
        }
        "new" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.line_c(&[(10.0, 8.0), (10.0, 14.0)], a);
            pen.line_c(&[(7.0, 11.0), (13.0, 11.0)], a);
        }
        "open" => {
            pen.line(&[(3.0, 16.0), (3.0, 5.0), (8.0, 5.0), (10.0, 7.0), (16.0, 7.0), (16.0, 9.5)]);
            pen.fill(&[(3.0, 16.0), (5.5, 9.5), (18.5, 9.5), (16.0, 16.0)], a);
        }
        "settings" => {
            pen.circle(10.0, 10.0, 3.0, c);
            for i in 0..8 {
                let t = std::f32::consts::TAU * i as f32 / 8.0;
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
        "dropdown" => pen.fill(&[(7.5, 8.8), (12.5, 8.8), (10.0, 11.5)], c),
        "note" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.line(&[(7.0, 8.0), (13.0, 8.0)]);
            pen.line(&[(7.0, 11.0), (13.0, 11.0)]);
            pen.line_c(&[(7.0, 14.0), (11.0, 14.0)], a);
        }
        _ => {
            // Fallback: a rounded tile with the first letter.
            p.rect_stroke(r.shrink(r.width() * 0.12), 3.0, Stroke::new(1.0, c), egui::StrokeKind::Middle);
            let ch = name.chars().next().map(|x| x.to_ascii_uppercase().to_string()).unwrap_or_default();
            p.text(r.center(), egui::Align2::CENTER_CENTER, ch, FontId::proportional(r.height() * 0.45), a);
        }
    }
}
