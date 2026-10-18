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
    "zoomActual",
    "panelLeft",
    "panelRight",
    "dark",
    "menuBar",
    "ribbon",
    // View > Show and Visual Aids
    "ruler",
    "grid",
    "guides",
    "pageBreaks",
    "connectionPoints",
    "autoConnect",
    "snap",
    // Text
    "bold",
    "italic",
    "fontColor",
    "underline",
    "strikethrough",
    "textCase",
    "highlight",
    "fontGrow",
    "fontShrink",
    "alignLeft",
    "alignCenter",
    "alignRight",
    "justify",
    "alignTop",
    "alignMiddle",
    "alignBottom",
    "bullets",
    "indentMore",
    "indentLess",
    "textDirection",
    "textBlock",
    "textBox",
    "symbol",
    "field",
    // Diagram
    "pointer",
    "connector",
    "text",
    "fill",
    "lineColor",
    "lineWeight",
    "rotateLeft",
    "rotateRight",
    "delete",
    "routeStraight",
    "routeOrthogonal",
    "routeCurved",
    "arrowStart",
    "arrowEnd",
    "shape",
    "layout",
    "routeAround",
    "lineJumps",
    "board",
    "connectorKind",
    "compartment",
    // Validation
    "rules",
    "liveChecking",
    "nextIssue",
    "prevIssue",
    "problems",
    "dock",
    // Arrange
    "align",
    "position",
    "bringFront",
    "sendBack",
    "group",
    // App
    "settings",
    "info",
    "help",
    "note",
    "noteNew",
    "noteDelete",
    "commands",
    // Links, comments and subprocesses
    "link",
    "comment",
    "commentNew",
    "commentDelete",
    "commentPrev",
    "commentNext",
    "subprocess",
    "subprocessNew",
    "subprocessLink",
    // Containers, swimlanes and callouts
    "container",
    "callout",
    "swimlanes",
    "headingPosition",
    "headingStyle",
    "fitContents",
    "margins",
    "autoFit",
    "lock",
    "disband",
    "laneAdd",
    "phase",
    "flipLanes",
    "leader",
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
    // View: presentation, windows, add-ons and macros
    "present",
    "pageWidth",
    "taskPanes",
    "newWindow",
    "arrangeAll",
    "cascade",
    "switchWindows",
    "macroRecord",
    "macroStop",
    "macroSave",
    "macroRun",
    "macroDelete",
    "addOns",
    "numberShapes",
    "arrayShapes",
    "moveShapes",
    // Developer
    "shapeOps",
    "shapeName",
    "behavior",
    "stencilPane",
    "drawingExplorer",
    // Picture and Ink
    "picture",
    "crop",
    "brightness",
    "contrast",
    "recolor",
    "pictureTransparency",
    "compress",
    "changePicture",
    "resetPicture",
    "pen",
    "highlighter",
    "eraser",
    "inkToShapes",
    "inkColor",
    "inkThickness",
    "lasso",
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
        "zoomActual" => {
            // A magnifier reading one to one: two bars with a colon between them.
            pen.magnifier(a);
            pen.line(&[(6.5, 6.5), (6.5, 10.5)]);
            pen.line(&[(10.5, 6.5), (10.5, 10.5)]);
            pen.line_c(&[(8.5, 7.3), (8.5, 7.7)], a);
            pen.line_c(&[(8.5, 9.3), (8.5, 9.7)], a);
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
        // View > Show and Visual Aids
        "ruler" => {
            // A rule along the top and left, with ticks.
            pen.closed(&[(2.5, 2.5), (17.5, 2.5), (17.5, 6.5), (6.5, 6.5), (6.5, 17.5), (2.5, 17.5)], c);
            for x in [9.0, 12.0, 15.0] {
                pen.line_c(&[(x, 2.5), (x, 4.5)], a);
            }
            for y in [9.0, 12.0, 15.0] {
                pen.line_c(&[(2.5, y), (4.5, y)], a);
            }
        }
        "grid" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            for v in [7.67, 12.33] {
                pen.line_c(&[(v, 3.0), (v, 17.0)], a);
                pen.line_c(&[(3.0, v), (17.0, v)], a);
            }
        }
        "guides" => {
            pen.page(4.0, 2.5, 16.0, 17.5);
            pen.line_c(&[(1.5, 8.0), (18.5, 8.0)], a);
            pen.line_c(&[(11.0, 1.0), (11.0, 19.0)], a);
        }
        "pageBreaks" => {
            pen.rect(2.5, 3.0, 17.5, 17.0, c);
            // Dashes where the sheet splits.
            for (y0, y1) in [(3.0, 5.5), (7.5, 10.0), (12.0, 14.5)] {
                pen.line_c(&[(10.0, y0), (10.0, y1)], a);
            }
            pen.line_c(&[(10.0, 16.0), (10.0, 17.0)], a);
        }
        "connectionPoints" => {
            pen.rect(5.0, 5.0, 15.0, 15.0, c);
            for (x, y) in [(10.0, 5.0), (15.0, 10.0), (10.0, 15.0), (5.0, 10.0)] {
                pen.line_c(&[(x - 1.8, y - 1.8), (x + 1.8, y + 1.8)], a);
                pen.line_c(&[(x - 1.8, y + 1.8), (x + 1.8, y - 1.8)], a);
            }
        }
        "autoConnect" => {
            pen.rect(2.5, 6.5, 9.0, 13.5, c);
            // An arrow out of the shape's side.
            pen.line_c(&[(9.0, 10.0), (17.5, 10.0)], a);
            pen.line_c(&[(14.5, 7.0), (17.5, 10.0), (14.5, 13.0)], a);
        }
        "snap" => {
            // A magnet.
            pen.line(&[(5.0, 3.0), (5.0, 11.0)]);
            pen.line(&[(15.0, 3.0), (15.0, 11.0)]);
            pen.arc(10.0, 11.0, 5.0, 0.0, PI, c);
            pen.line_c(&[(3.5, 4.5), (6.5, 4.5)], a);
            pen.line_c(&[(13.5, 4.5), (16.5, 4.5)], a);
        }
        // Text
        "bold" => {
            // A B built from strokes, drawn heavier than the rest of the set.
            let b = Pen { p, r, c, w: pen.w * 1.7 };
            b.line(&[(11.5, 10.0), (6.0, 10.0), (6.0, 3.5), (10.5, 3.5)]);
            b.arc(10.5, 6.75, 3.25, -FRAC_PI_2, PI, c);
            b.line(&[(6.0, 10.0), (6.0, 16.5), (11.5, 16.5)]);
            b.arc(11.5, 13.25, 3.25, FRAC_PI_2, -PI, c);
        }
        "italic" => {
            pen.line(&[(12.5, 3.5), (7.5, 16.5)]);
            pen.line(&[(9.5, 3.5), (15.5, 3.5)]);
            pen.line(&[(4.5, 16.5), (10.5, 16.5)]);
        }
        "fontColor" => {
            pen.line(&[(5.0, 13.5), (10.0, 2.5), (15.0, 13.5)]);
            pen.line(&[(7.0, 9.5), (13.0, 9.5)]);
            pen.frect(3.0, 15.5, 17.0, 18.5, a);
        }
        "underline" => {
            pen.line(&[(6.0, 3.0), (6.0, 10.0)]);
            pen.arc(10.0, 10.0, 4.0, 0.0, PI, c);
            pen.line(&[(14.0, 10.0), (14.0, 3.0)]);
            pen.frect(4.0, 16.0, 16.0, 18.0, a);
        }
        "strikethrough" => {
            pen.line(&[(5.0, 16.5), (10.0, 3.5), (15.0, 16.5)]);
            pen.line(&[(7.0, 12.5), (13.0, 12.5)]);
            pen.line_c(&[(3.0, 9.5), (17.0, 9.5)], a);
        }
        "textCase" => {
            pen.line(&[(2.5, 16.0), (7.0, 4.0), (11.5, 16.0)]);
            pen.line(&[(4.2, 11.5), (9.8, 11.5)]);
            pen.circle(14.75, 13.25, 2.5, a);
            pen.line_c(&[(17.25, 10.25), (17.25, 16.0)], a);
        }
        "highlight" => {
            pen.closed(&[(11.0, 2.5), (16.0, 7.5), (10.0, 13.5), (5.0, 8.5)], c);
            pen.line(&[(5.0, 8.5), (4.0, 12.5), (6.0, 14.5), (10.0, 13.5)]);
            pen.frect(3.0, 16.0, 17.0, 18.5, a);
        }
        "fontGrow" | "fontShrink" => {
            pen.line(&[(2.5, 17.0), (8.0, 4.0), (13.5, 17.0)]);
            pen.line(&[(4.8, 12.5), (11.2, 12.5)]);
            if name == "fontGrow" {
                pen.line_c(&[(14.0, 8.0), (16.25, 5.0), (18.5, 8.0)], a);
            } else {
                pen.line_c(&[(14.0, 5.0), (16.25, 8.0), (18.5, 5.0)], a);
            }
        }
        "alignLeft" | "alignCenter" | "alignRight" | "justify" => {
            // Four lines, the second and fourth shorter and set by the alignment.
            let (long, short) = match name {
                "alignLeft" => ((3.0, 17.0), [(3.0, 12.0), (3.0, 10.0)]),
                "alignCenter" => ((3.0, 17.0), [(5.5, 14.5), (6.5, 13.5)]),
                "alignRight" => ((3.0, 17.0), [(8.0, 17.0), (10.0, 17.0)]),
                _ => ((3.0, 17.0), [(3.0, 17.0), (3.0, 17.0)]),
            };
            pen.line(&[(long.0, 4.0), (long.1, 4.0)]);
            pen.line_c(&[(short[0].0, 8.0), (short[0].1, 8.0)], a);
            pen.line(&[(long.0, 12.0), (long.1, 12.0)]);
            pen.line_c(&[(short[1].0, 16.0), (short[1].1, 16.0)], a);
        }
        "alignTop" | "alignMiddle" | "alignBottom" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            let y = match name {
                "alignTop" => 6.5,
                "alignMiddle" => 8.5,
                _ => 10.5,
            };
            pen.line_c(&[(6.0, y), (14.0, y)], a);
            pen.line_c(&[(6.0, y + 3.0), (12.0, y + 3.0)], a);
        }
        "bullets" => {
            for y in [4.5, 10.0, 15.5] {
                pen.fcircle(4.5, y, 1.4, a);
                pen.line(&[(8.0, y), (17.0, y)]);
            }
        }
        "indentMore" | "indentLess" => {
            pen.lines(9.0, 17.0, &[4.0, 8.0, 12.0]);
            pen.line(&[(3.0, 16.0), (17.0, 16.0)]);
            if name == "indentMore" {
                pen.line_c(&[(3.0, 5.5), (6.0, 8.0), (3.0, 10.5)], a);
            } else {
                pen.line_c(&[(6.0, 5.5), (3.0, 8.0), (6.0, 10.5)], a);
            }
        }
        "textDirection" => {
            pen.line(&[(5.0, 4.0), (5.0, 15.0)]);
            pen.line(&[(9.5, 4.0), (9.5, 12.0)]);
            pen.line_c(&[(15.0, 3.0), (15.0, 16.5)], a);
            pen.line_c(&[(12.5, 14.0), (15.0, 16.5), (17.5, 14.0)], a);
        }
        "textBlock" => {
            // Corner brackets around a T.
            for (x, y, dx, dy) in [(3.0, 3.0, 1.0, 1.0), (17.0, 3.0, -1.0, 1.0), (17.0, 17.0, -1.0, -1.0), (3.0, 17.0, 1.0, -1.0)] {
                pen.line(&[(x + 3.5 * dx, y), (x, y), (x, y + 3.5 * dy)]);
            }
            pen.line_c(&[(6.5, 7.0), (13.5, 7.0)], a);
            pen.line_c(&[(10.0, 7.0), (10.0, 14.0)], a);
        }
        "textBox" => {
            pen.rect(2.5, 3.5, 17.5, 16.5, c);
            pen.line_c(&[(6.0, 7.0), (14.0, 7.0)], a);
            pen.line_c(&[(10.0, 7.0), (10.0, 13.5)], a);
        }
        "symbol" => {
            // An omega: an open ring on two feet.
            pen.arc(10.0, 8.5, 5.0, 0.75 * PI, 1.5 * PI, c);
            pen.line_c(&[(6.46, 12.04), (7.0, 16.0), (3.5, 16.0)], a);
            pen.line_c(&[(13.54, 12.04), (13.0, 16.0), (16.5, 16.0)], a);
        }
        "field" => {
            pen.line(&[(7.0, 3.0), (5.5, 4.0), (5.5, 8.5), (4.0, 10.0), (5.5, 11.5), (5.5, 16.0), (7.0, 17.0)]);
            pen.line(&[(13.0, 3.0), (14.5, 4.0), (14.5, 8.5), (16.0, 10.0), (14.5, 11.5), (14.5, 16.0), (13.0, 17.0)]);
            pen.fcircle(10.0, 10.0, 1.6, a);
        }
        // Diagram
        "pointer" => {
            pen.closed(&[(5.0, 2.5), (5.0, 15.5), (8.5, 12.5), (11.0, 17.5), (13.0, 16.5), (10.5, 11.5), (15.0, 11.0)], c);
        }
        "connector" => {
            pen.rect(2.5, 2.5, 7.5, 7.5, c);
            pen.rect(12.5, 12.5, 17.5, 17.5, c);
            pen.line_c(&[(7.5, 5.0), (10.0, 5.0), (10.0, 15.0), (12.5, 15.0)], a);
        }
        "text" => {
            pen.line(&[(4.5, 4.0), (15.5, 4.0)]);
            pen.line(&[(4.5, 4.0), (4.5, 6.0)]);
            pen.line(&[(15.5, 4.0), (15.5, 6.0)]);
            pen.line(&[(10.0, 4.0), (10.0, 16.0)]);
            pen.line_c(&[(7.5, 16.0), (12.5, 16.0)], a);
        }
        "fill" => {
            // A tipped bucket with a drop running off it, over the colour bar.
            pen.closed(&[(3.5, 8.0), (9.0, 2.5), (14.5, 8.0), (9.0, 13.5)], c);
            pen.line(&[(3.5, 8.0), (14.5, 8.0)]);
            pen.fcircle(16.0, 11.0, 1.4, a);
            pen.frect(3.0, 15.5, 17.0, 18.5, a);
        }
        "lineColor" => {
            // A pencil over the colour bar.
            pen.closed(&[(4.0, 13.5), (12.0, 3.5), (15.0, 6.0), (7.0, 14.0), (4.0, 14.0)], c);
            pen.line(&[(10.5, 5.0), (13.5, 7.5)]);
            pen.frect(3.0, 15.5, 17.0, 18.5, a);
        }
        "lineWeight" => {
            pen.frect(3.0, 4.0, 17.0, 4.8, c);
            pen.frect(3.0, 8.5, 17.0, 10.0, c);
            pen.frect(3.0, 13.5, 17.0, 16.0, a);
        }
        "rotateLeft" => {
            pen.arc(10.0, 10.0, 6.0, 0.0, -PI, c);
            pen.fill(&[(1.5, 9.5), (6.5, 9.5), (4.0, 13.5)], a);
            pen.rect(7.5, 13.0, 12.5, 18.0, c);
        }
        "rotateRight" => {
            pen.arc(10.0, 10.0, 6.0, PI, PI, c);
            pen.fill(&[(13.5, 9.5), (18.5, 9.5), (16.0, 13.5)], a);
            pen.rect(7.5, 13.0, 12.5, 18.0, c);
        }
        "delete" => {
            pen.line(&[(3.5, 5.5), (16.5, 5.5)]);
            pen.line(&[(8.0, 5.5), (8.0, 3.0), (12.0, 3.0), (12.0, 5.5)]);
            pen.closed(&[(5.5, 5.5), (6.5, 17.0), (13.5, 17.0), (14.5, 5.5)], c);
            pen.line_c(&[(8.5, 8.5), (8.5, 14.0)], a);
            pen.line_c(&[(11.5, 8.5), (11.5, 14.0)], a);
        }
        "routeStraight" => {
            pen.line(&[(4.0, 16.0), (16.0, 4.0)]);
            pen.fcircle(4.0, 16.0, 1.8, a);
            pen.fcircle(16.0, 4.0, 1.8, a);
        }
        "routeOrthogonal" => {
            pen.line(&[(4.0, 16.0), (10.0, 16.0), (10.0, 4.0), (16.0, 4.0)]);
            pen.fcircle(4.0, 16.0, 1.8, a);
            pen.fcircle(16.0, 4.0, 1.8, a);
        }
        "routeCurved" => {
            let (p0, p1, p2, p3) = ((4.0f32, 16.0f32), (13.0f32, 16.0f32), (7.0f32, 4.0f32), (16.0f32, 4.0f32));
            let pts: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    let u = 1.0 - t;
                    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    (b0 * p0.0 + b1 * p1.0 + b2 * p2.0 + b3 * p3.0, b0 * p0.1 + b1 * p1.1 + b2 * p2.1 + b3 * p3.1)
                })
                .collect();
            pen.line(&pts);
            pen.fcircle(4.0, 16.0, 1.8, a);
            pen.fcircle(16.0, 4.0, 1.8, a);
        }
        "arrowStart" => {
            pen.line(&[(7.0, 10.0), (17.0, 10.0)]);
            pen.fill(&[(2.5, 10.0), (8.0, 6.5), (8.0, 13.5)], a);
        }
        "arrowEnd" => {
            pen.line(&[(3.0, 10.0), (13.0, 10.0)]);
            pen.fill(&[(17.5, 10.0), (12.0, 6.5), (12.0, 13.5)], a);
        }
        "shape" => {
            pen.rect(3.0, 8.0, 12.0, 17.0, c);
            pen.circle(12.5, 7.5, 4.5, a);
        }
        "layout" => {
            // A small tree: one box over two, joined by an elbowed fork.
            pen.rect(7.0, 2.5, 13.0, 6.5, c);
            pen.rect(2.5, 13.5, 8.5, 17.5, c);
            pen.rect(11.5, 13.5, 17.5, 17.5, c);
            pen.line_c(&[(10.0, 6.5), (10.0, 10.0)], a);
            pen.line_c(&[(5.5, 13.5), (5.5, 10.0), (14.5, 10.0), (14.5, 13.5)], a);
        }
        "routeAround" => {
            // A route stepping over a block instead of through it.
            pen.rect(7.0, 7.5, 13.0, 15.5, c);
            pen.line_c(&[(2.5, 11.5), (4.5, 11.5), (4.5, 4.0), (15.5, 4.0), (15.5, 11.5), (17.5, 11.5)], a);
            pen.fcircle(2.5, 11.5, 1.5, a);
            pen.fcircle(17.5, 11.5, 1.5, a);
        }
        "lineJumps" => {
            // A vertical line hopping over a horizontal one.
            pen.line(&[(2.5, 12.0), (17.5, 12.0)]);
            pen.line_c(&[(10.0, 17.5), (10.0, 15.0)], a);
            pen.arc(10.0, 12.0, 3.0, FRAC_PI_2, PI, a);
            pen.line_c(&[(10.0, 9.0), (10.0, 2.5)], a);
        }
        "board" => {
            // A ruled sheet: rulers along the top and left edge, one mark placed on the board.
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            for t in [7.0, 10.0, 13.0] {
                let len = if t == 10.0 { 3.0 } else { 2.0 };
                pen.line_c(&[(t, 3.0), (t, 3.0 + len)], a);
                pen.line_c(&[(3.0, t), (3.0 + len, t)], a);
            }
            pen.fcircle(12.5, 12.5, 1.8, a);
        }
        "connectorKind" => {
            // Two boxes joined by an elbowed connector that ends in a head: a connector as a kind.
            pen.rect(2.5, 11.5, 7.5, 16.5, c);
            pen.rect(12.5, 3.5, 17.5, 8.5, c);
            pen.line_c(&[(7.5, 14.0), (10.0, 14.0), (10.0, 6.0)], a);
            pen.fill(&[(12.0, 6.0), (8.5, 4.0), (8.5, 8.0)], a);
        }
        "compartment" => {
            // A box cut into bands, text rows in the lower two.
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            pen.line(&[(3.0, 7.5), (17.0, 7.5)]);
            pen.line(&[(3.0, 12.5), (17.0, 12.5)]);
            pen.line_c(&[(6.0, 10.0), (14.0, 10.0)], a);
            pen.line_c(&[(6.0, 15.0), (12.0, 15.0)], a);
        }
        "rules" => {
            // A checklist: ticks beside three rule lines.
            for y in [5.0, 10.0, 15.0] {
                pen.line_c(&[(2.5, y), (4.0, y + 1.5), (6.5, y - 1.8)], a);
                pen.line(&[(9.5, y), (17.5, y)]);
            }
        }
        "liveChecking" => {
            // A tick in a ring with rays: a check that keeps running.
            pen.circle(10.0, 10.0, 5.0, c);
            pen.line_c(&[(7.5, 10.2), (9.3, 12.0), (12.8, 8.0)], a);
            for i in 0..8 {
                let t = TAU * (i as f32 + 0.5) / 8.0;
                let (sx, sy) = (t.cos(), t.sin());
                pen.line_c(&[(10.0 + 7.2 * sx, 10.0 + 7.2 * sy), (10.0 + 8.8 * sx, 10.0 + 8.8 * sy)], a);
            }
        }
        "problems" => {
            // A warning triangle with a mark.
            pen.closed(&[(10.0, 3.0), (17.0, 16.0), (3.0, 16.0)], c);
            pen.line_c(&[(10.0, 8.0), (10.0, 12.0)], a);
            pen.fcircle(10.0, 14.0, 0.8, a);
        }
        "dock" => {
            // A window with a bottom band.
            pen.rect(3.0, 4.0, 17.0, 16.0, c);
            pen.line_c(&[(3.0, 11.5), (17.0, 11.5)], a);
        }
        "nextIssue" => {
            // An issue marker with a chevron leading on.
            pen.circle(7.0, 10.0, 4.5, c);
            pen.line_c(&[(7.0, 7.8), (7.0, 10.6)], a);
            pen.fcircle(7.0, 12.4, 0.8, a);
            pen.line_c(&[(13.5, 6.5), (17.0, 10.0), (13.5, 13.5)], a);
        }
        "prevIssue" => {
            // An issue marker with a chevron leading back.
            pen.circle(13.0, 10.0, 4.5, c);
            pen.line_c(&[(13.0, 7.8), (13.0, 10.6)], a);
            pen.fcircle(13.0, 12.4, 0.8, a);
            pen.line_c(&[(6.5, 6.5), (3.0, 10.0), (6.5, 13.5)], a);
        }
        // Arrange
        "align" => {
            // Three bars of different lengths flush against a left edge.
            pen.line(&[(3.0, 2.5), (3.0, 17.5)]);
            pen.rect(5.0, 4.0, 15.0, 7.5, c);
            pen.frect(5.0, 9.5, 11.0, 12.5, a);
            pen.rect(5.0, 14.0, 17.0, 16.5, c);
        }
        "position" => {
            // Three boxes with equal gaps, the gaps marked.
            pen.rect(2.0, 6.5, 6.0, 13.5, c);
            pen.rect(8.0, 6.5, 12.0, 13.5, c);
            pen.rect(14.0, 6.5, 18.0, 13.5, c);
            pen.line_c(&[(6.5, 16.5), (7.5, 16.5)], a);
            pen.line_c(&[(12.5, 16.5), (13.5, 16.5)], a);
        }
        "bringFront" => {
            // A filled square overlapping an outlined one from the front.
            pen.rect(2.5, 2.5, 11.5, 11.5, c);
            pen.frect(8.5, 8.5, 17.5, 17.5, a);
        }
        "sendBack" => {
            // An outlined square in front of a filled one behind it.
            pen.frect(2.5, 2.5, 11.5, 11.5, a);
            pen.rect(8.5, 8.5, 17.5, 17.5, c);
        }
        "group" => {
            // Two shapes inside a dashed-looking frame with corner marks.
            for (x, y) in [(2.5, 2.5), (17.5, 2.5), (17.5, 17.5), (2.5, 17.5)] {
                pen.frect(x - 1.2, y - 1.2, x + 1.2, y + 1.2, c);
            }
            pen.rect(5.0, 5.0, 11.0, 10.0, c);
            pen.circle(12.5, 12.5, 3.0, a);
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
        "link" => {
            // Two chain links, joined by a bar.
            pen.rect(2.5, 7.0, 10.0, 13.0, c);
            pen.rect(10.0, 7.0, 17.5, 13.0, c);
            pen.line_c(&[(7.0, 10.0), (13.0, 10.0)], a);
        }
        "comment" => {
            pen.closed(&[(3.0, 4.0), (17.0, 4.0), (17.0, 13.0), (9.5, 13.0), (5.5, 17.0), (5.5, 13.0), (3.0, 13.0)], c);
            pen.lines(6.5, 13.5, &[7.5, 10.0]);
        }
        "commentNew" => {
            pen.closed(&[(2.5, 3.5), (15.5, 3.5), (15.5, 11.5), (8.5, 11.5), (5.0, 15.0), (5.0, 11.5), (2.5, 11.5)], c);
            pen.fcircle(15.0, 15.0, 4.0, a);
            let w = Color32::WHITE;
            pen.line_c(&[(15.0, 12.8), (15.0, 17.2)], w);
            pen.line_c(&[(12.8, 15.0), (17.2, 15.0)], w);
        }
        "commentDelete" => {
            pen.closed(&[(2.5, 3.5), (15.5, 3.5), (15.5, 11.5), (8.5, 11.5), (5.0, 15.0), (5.0, 11.5), (2.5, 11.5)], c);
            pen.fcircle(15.0, 15.0, 4.0, a);
            pen.line_c(&[(12.9, 15.0), (17.1, 15.0)], Color32::WHITE);
        }
        "commentPrev" => {
            pen.closed(&[(6.0, 3.5), (17.5, 3.5), (17.5, 11.5), (11.5, 11.5), (8.5, 14.5), (8.5, 11.5), (6.0, 11.5)], c);
            pen.line_c(&[(4.5, 14.5), (1.5, 11.0), (4.5, 7.5)], a);
        }
        "commentNext" => {
            pen.closed(&[(2.5, 3.5), (14.0, 3.5), (14.0, 11.5), (8.0, 11.5), (5.0, 14.5), (5.0, 11.5), (2.5, 11.5)], c);
            pen.line_c(&[(15.5, 7.5), (18.5, 11.0), (15.5, 14.5)], a);
        }
        "subprocess" => {
            // A predefined process box (side bars) with a tick marking where it leads.
            pen.rect(3.0, 5.0, 17.0, 15.0, c);
            pen.line(&[(5.5, 5.0), (5.5, 15.0)]);
            pen.line(&[(14.5, 5.0), (14.5, 15.0)]);
            pen.line_c(&[(8.0, 10.0), (12.0, 10.0)], a);
        }
        "subprocessNew" => {
            pen.rect(3.0, 4.0, 15.0, 14.0, c);
            pen.line(&[(5.5, 4.0), (5.5, 14.0)]);
            pen.line(&[(12.5, 4.0), (12.5, 14.0)]);
            pen.fcircle(15.0, 15.0, 4.0, a);
            let w = Color32::WHITE;
            pen.line_c(&[(15.0, 12.8), (15.0, 17.2)], w);
            pen.line_c(&[(12.8, 15.0), (17.2, 15.0)], w);
        }
        "subprocessLink" => {
            pen.rect(3.0, 4.0, 15.0, 14.0, c);
            pen.line(&[(5.5, 4.0), (5.5, 14.0)]);
            pen.line(&[(12.5, 4.0), (12.5, 14.0)]);
            pen.line_c(&[(10.0, 17.5), (14.0, 17.5), (17.5, 14.0)], a);
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
        // Containers, swimlanes and callouts
        "container" => {
            pen.rect(2.5, 3.5, 17.5, 16.5, c);
            pen.frect(3.0, 4.0, 17.0, 7.5, a);
            pen.rect(5.5, 10.0, 9.5, 14.0, c);
            pen.rect(11.5, 10.0, 15.5, 14.0, c);
        }
        "callout" => {
            pen.rect(7.0, 3.0, 18.0, 11.0, c);
            pen.lines(9.0, 16.0, &[6.0, 8.5]);
            pen.line_c(&[(8.5, 11.0), (3.5, 16.5)], a);
            pen.fcircle(3.5, 16.5, 1.4, a);
        }
        "swimlanes" => {
            pen.rect(2.5, 3.5, 17.5, 16.5, c);
            pen.line(&[(2.5, 7.8), (17.5, 7.8)]);
            pen.line(&[(2.5, 12.2), (17.5, 12.2)]);
            pen.frect(3.0, 4.0, 6.0, 16.0, a);
        }
        "headingPosition" => {
            pen.rect(3.0, 3.5, 17.0, 16.5, c);
            pen.frect(3.5, 4.0, 7.0, 16.0, a);
            pen.line_c(&[(10.0, 10.0), (14.5, 10.0)], c);
        }
        "headingStyle" => {
            pen.rect(3.0, 3.5, 17.0, 16.5, c);
            pen.frect(3.5, 4.0, 16.5, 7.5, a);
            pen.lines(5.5, 14.5, &[11.0, 13.5]);
        }
        "fitContents" => {
            pen.rect(5.5, 5.5, 14.5, 14.5, c);
            pen.line_c(&[(2.5, 5.0), (2.5, 2.5), (5.0, 2.5)], a);
            pen.line_c(&[(15.0, 2.5), (17.5, 2.5), (17.5, 5.0)], a);
            pen.line_c(&[(17.5, 15.0), (17.5, 17.5), (15.0, 17.5)], a);
            pen.line_c(&[(5.0, 17.5), (2.5, 17.5), (2.5, 15.0)], a);
        }
        "margins" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            pen.rect(7.0, 7.0, 13.0, 13.0, a);
        }
        "autoFit" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            pen.line_c(&[(7.0, 10.0), (13.0, 10.0)], a);
            pen.fill(&[(13.5, 7.5), (16.0, 10.0), (13.5, 12.5)], a);
            pen.fill(&[(6.5, 7.5), (4.0, 10.0), (6.5, 12.5)], a);
        }
        "lock" => {
            pen.rect(4.5, 9.0, 15.5, 17.0, c);
            pen.arc(10.0, 9.0, 3.5, PI, PI, c);
            pen.fcircle(10.0, 13.0, 1.3, a);
        }
        "disband" => {
            pen.rect(5.0, 5.0, 15.0, 15.0, c);
            pen.line_c(&[(2.5, 2.5), (6.0, 6.0)], a);
            pen.line_c(&[(17.5, 2.5), (14.0, 6.0)], a);
            pen.line_c(&[(17.5, 17.5), (14.0, 14.0)], a);
            pen.line_c(&[(2.5, 17.5), (6.0, 14.0)], a);
        }
        "laneAdd" => {
            pen.rect(2.5, 3.5, 17.5, 12.5, c);
            pen.line(&[(2.5, 8.0), (17.5, 8.0)]);
            pen.line_c(&[(10.0, 13.5), (10.0, 18.5)], a);
            pen.line_c(&[(7.5, 16.0), (12.5, 16.0)], a);
        }
        "phase" => {
            pen.rect(2.5, 3.5, 17.5, 16.5, c);
            pen.line_c(&[(10.0, 3.5), (10.0, 6.0)], a);
            pen.line_c(&[(10.0, 8.5), (10.0, 11.0)], a);
            pen.line_c(&[(10.0, 13.5), (10.0, 16.5)], a);
        }
        "flipLanes" => {
            pen.rect(2.5, 2.5, 9.0, 9.0, c);
            pen.line(&[(2.5, 5.75), (9.0, 5.75)]);
            pen.rect(11.0, 11.0, 17.5, 17.5, c);
            pen.line(&[(14.25, 11.0), (14.25, 17.5)]);
            pen.arc(10.0, 10.0, 6.0, -FRAC_PI_2 * 0.2, FRAC_PI_2 * 1.4, a);
        }
        "leader" => {
            pen.rect(9.0, 3.0, 18.0, 9.0, c);
            pen.line_c(&[(10.0, 9.0), (4.0, 16.0)], a);
            pen.fcircle(4.0, 16.0, 1.6, a);
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
        // View: presentation, windows, add-ons and macros
        "present" => {
            pen.rect(2.5, 3.5, 17.5, 13.5, c);
            pen.fill(&[(8.5, 6.0), (8.5, 11.0), (12.5, 8.5)], a);
            pen.line(&[(10.0, 13.5), (10.0, 16.5)]);
            pen.line(&[(6.5, 16.5), (13.5, 16.5)]);
        }
        "pageWidth" => {
            pen.rect(5.0, 3.0, 15.0, 17.0, c);
            pen.line_c(&[(2.0, 10.0), (18.0, 10.0)], a);
            pen.line_c(&[(4.5, 7.5), (2.0, 10.0), (4.5, 12.5)], a);
            pen.line_c(&[(15.5, 7.5), (18.0, 10.0), (15.5, 12.5)], a);
        }
        "taskPanes" => {
            pen.rect(3.0, 3.5, 17.0, 16.5, c);
            pen.line(&[(11.0, 3.5), (11.0, 16.5)]);
            pen.frect(12.5, 5.5, 15.5, 8.0, a);
            pen.frect(12.5, 10.0, 15.5, 14.5, a.linear_multiply(0.55));
        }
        "newWindow" => {
            pen.window();
            pen.line_c(&[(10.0, 9.0), (10.0, 14.0)], a);
            pen.line_c(&[(7.5, 11.5), (12.5, 11.5)], a);
        }
        "arrangeAll" => {
            pen.rect(2.5, 3.0, 9.5, 9.5, c);
            pen.rect(10.5, 3.0, 17.5, 9.5, c);
            pen.rect(2.5, 10.5, 9.5, 17.0, c);
            pen.frect(10.5, 10.5, 17.5, 17.0, a.linear_multiply(0.55));
        }
        "cascade" => {
            pen.rect(2.5, 2.5, 12.5, 11.0, c);
            pen.rect(5.5, 6.0, 15.5, 14.5, c);
            pen.rect(8.0, 9.5, 17.5, 17.5, a);
        }
        "switchWindows" => {
            pen.rect(2.5, 4.0, 11.5, 12.5, c);
            pen.rect(8.5, 8.0, 17.5, 16.5, c);
            pen.line_c(&[(13.0, 11.0), (15.0, 13.0), (13.0, 15.0)], a);
        }
        "macroRecord" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.fcircle(10.0, 10.0, 3.5, a);
        }
        "macroStop" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.frect(7.0, 7.0, 13.0, 13.0, a);
        }
        "macroSave" => {
            pen.circle(8.5, 8.5, 5.5, c);
            pen.fcircle(8.5, 8.5, 2.5, a);
            pen.line_c(&[(13.0, 14.0), (17.0, 14.0)], a);
            pen.line_c(&[(15.0, 12.0), (15.0, 16.0)], a);
        }
        "macroRun" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.fill(&[(8.0, 6.5), (8.0, 13.5), (13.5, 10.0)], a);
        }
        "macroDelete" => {
            pen.circle(10.0, 10.0, 7.0, c);
            pen.line_c(&[(7.0, 7.0), (13.0, 13.0)], a);
            pen.line_c(&[(13.0, 7.0), (7.0, 13.0)], a);
        }
        "addOns" => {
            pen.rect(3.0, 6.0, 12.0, 15.0, c);
            pen.frect(12.0, 3.0, 17.0, 8.0, a);
            pen.line(&[(7.5, 10.5), (7.5, 10.6)]);
        }
        "numberShapes" => {
            pen.rect(2.5, 3.0, 9.5, 9.0, c);
            pen.rect(2.5, 11.0, 9.5, 17.0, c);
            pen.line_c(&[(12.0, 6.0), (17.5, 6.0)], a);
            pen.line_c(&[(12.0, 14.0), (17.5, 14.0)], a);
            pen.line_c(&[(5.5, 4.5), (5.5, 7.5)], a);
            pen.line_c(&[(4.5, 13.0), (6.5, 13.0), (4.5, 15.0), (6.5, 15.0)], a);
        }
        "arrayShapes" => {
            for (x, y) in [(2.5, 2.5), (11.0, 2.5), (2.5, 11.0)] {
                pen.rect(x, y, x + 6.5, y + 6.5, c);
            }
            pen.rect(11.0, 11.0, 17.5, 17.5, a);
        }
        "moveShapes" => {
            pen.rect(2.5, 9.0, 9.5, 17.0, c);
            pen.rect(11.0, 3.0, 17.5, 9.5, a);
            pen.line_c(&[(6.0, 7.5), (6.0, 4.0), (9.5, 4.0)], a);
            pen.line_c(&[(8.0, 2.5), (9.5, 4.0), (8.0, 5.5)], a);
        }
        // Developer
        "shapeOps" => {
            pen.rect(2.5, 2.5, 12.5, 12.5, c);
            pen.rect(7.5, 7.5, 17.5, 17.5, c);
            pen.frect(7.5, 7.5, 12.5, 12.5, a);
        }
        "shapeName" => {
            pen.closed(&[(2.5, 6.0), (13.0, 6.0), (17.5, 10.0), (13.0, 14.0), (2.5, 14.0)], c);
            pen.fcircle(13.0, 10.0, 1.2, a);
            pen.line_c(&[(5.0, 10.0), (10.0, 10.0)], a);
        }
        "behavior" => {
            pen.rect(2.5, 4.5, 12.5, 14.5, c);
            pen.circle(14.0, 14.0, 3.5, a);
            pen.fcircle(14.0, 14.0, 1.2, a);
        }
        "stencilPane" => {
            pen.rect(3.0, 2.5, 17.0, 17.5, c);
            pen.rect(5.5, 5.0, 9.0, 8.5, a);
            pen.circle(13.0, 6.75, 1.9, a);
            pen.fill(&[(7.25, 11.0), (9.25, 14.5), (5.25, 14.5)], a);
            pen.line(&[(11.0, 13.0), (15.0, 13.0)]);
        }
        "drawingExplorer" => {
            pen.rect(2.5, 2.5, 7.5, 6.5, c);
            pen.line(&[(5.0, 6.5), (5.0, 15.0), (10.0, 15.0)]);
            pen.line(&[(5.0, 10.5), (10.0, 10.5)]);
            pen.frect(10.5, 9.0, 17.5, 12.0, a);
            pen.frect(10.5, 13.5, 17.5, 16.5, a);
        }
        "picture" => {
            pen.rect(2.5, 4.0, 17.5, 16.0, c);
            pen.fill(&[(4.5, 14.0), (9.0, 8.5), (13.5, 14.0)], a);
            pen.fcircle(13.5, 7.5, 1.6, a);
        }
        "crop" => {
            pen.line(&[(5.5, 2.0), (5.5, 14.5), (18.0, 14.5)]);
            pen.line_c(&[(2.0, 5.5), (14.5, 5.5), (14.5, 18.0)], a);
        }
        "brightness" => {
            pen.fcircle(10.0, 10.0, 3.2, a);
            for k in 0..8 {
                let t = k as f32 * std::f32::consts::FRAC_PI_4;
                let (sn, cs) = t.sin_cos();
                pen.line(&[(10.0 + 5.2 * cs, 10.0 + 5.2 * sn), (10.0 + 7.5 * cs, 10.0 + 7.5 * sn)]);
            }
        }
        "contrast" => {
            let half: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let t = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 12.0;
                    (10.0 + 6.5 * t.cos(), 10.0 + 6.5 * t.sin())
                })
                .collect();
            pen.fill(&half, a);
            pen.circle(10.0, 10.0, 6.5, c);
        }
        "recolor" => {
            pen.fcircle(7.5, 8.0, 3.6, a);
            pen.circle(12.5, 8.0, 3.6, c);
            pen.circle(10.0, 12.5, 3.6, c);
        }
        "pictureTransparency" => {
            pen.rect(3.0, 3.0, 17.0, 17.0, c);
            for (x, y) in [(3.0, 3.0), (10.0, 10.0)] {
                pen.frect(x, y, x + 7.0, y + 7.0, a);
            }
        }
        "compress" => {
            pen.rect(2.5, 2.5, 17.5, 17.5, c);
            pen.line_c(&[(5.0, 5.0), (8.5, 8.5)], a);
            pen.line_c(&[(15.0, 15.0), (11.5, 11.5)], a);
            pen.line_c(&[(8.5, 5.5), (8.5, 8.5), (5.5, 8.5)], a);
            pen.line_c(&[(11.5, 14.5), (11.5, 11.5), (14.5, 11.5)], a);
        }
        "changePicture" => {
            pen.rect(2.5, 2.5, 13.0, 11.5, c);
            pen.fill(&[(4.0, 10.0), (7.0, 6.0), (10.0, 10.0)], c);
            pen.arc(13.0, 13.0, 4.5, 0.0, std::f32::consts::PI * 1.4, a);
            pen.fill(&[(16.0, 12.0), (19.0, 12.0), (17.5, 14.5)], a);
        }
        "resetPicture" => {
            pen.rect(6.5, 6.5, 17.5, 16.5, c);
            pen.fill(&[(8.0, 15.0), (11.5, 10.5), (15.0, 15.0)], c);
            pen.arc(6.5, 6.5, 4.0, std::f32::consts::PI, std::f32::consts::PI * 1.3, a);
            pen.fill(&[(1.0, 6.0), (4.5, 6.0), (2.75, 8.5)], a);
        }
        "pen" => {
            pen.closed(&[(13.5, 2.5), (17.5, 6.5), (7.5, 16.5), (3.5, 12.5)], c);
            pen.fill(&[(3.5, 12.5), (7.5, 16.5), (2.0, 18.0)], a);
        }
        "highlighter" => {
            pen.closed(&[(12.5, 2.0), (18.0, 7.5), (10.0, 15.5), (4.5, 10.0)], c);
            pen.fill(&[(4.5, 10.0), (10.0, 15.5), (7.0, 17.0), (3.0, 13.0)], a);
            pen.frect(2.0, 17.5, 18.0, 19.0, a);
        }
        "eraser" => {
            pen.closed(&[(11.0, 3.0), (18.0, 10.0), (11.0, 17.0), (4.0, 10.0)], c);
            pen.fill(&[(4.0, 10.0), (7.5, 6.5), (14.5, 13.5), (11.0, 17.0)], a);
            pen.line(&[(11.0, 17.5), (18.0, 17.5)]);
        }
        "inkToShapes" => {
            pen.line(&[(2.0, 8.0), (4.0, 4.0), (6.0, 9.0), (8.0, 5.0), (9.5, 8.0)]);
            pen.line_c(&[(9.0, 14.0), (12.0, 14.0)], a);
            pen.fill(&[(12.0, 12.5), (14.0, 14.0), (12.0, 15.5)], a);
            pen.rect(13.5, 2.5, 18.0, 7.0, c);
            pen.rect(14.5, 11.0, 18.0, 17.0, a);
        }
        "inkColor" => {
            pen.closed(&[(13.0, 2.0), (16.5, 5.5), (8.0, 14.0), (4.5, 10.5)], c);
            pen.fill(&[(4.5, 10.5), (8.0, 14.0), (3.0, 15.0)], c);
            pen.frect(2.5, 16.5, 17.5, 19.0, a);
        }
        "inkThickness" => {
            pen.frect(3.0, 4.0, 17.0, 4.8, c);
            pen.frect(3.0, 8.0, 17.0, 9.6, c);
            pen.frect(3.0, 12.5, 17.0, 15.5, a);
        }
        "lasso" => {
            let loop_pts: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 16.0;
                    (10.5 + 7.0 * t.cos(), 7.5 + 4.5 * t.sin())
                })
                .collect();
            pen.line(&loop_pts);
            pen.line_c(&[(6.5, 11.0), (5.5, 14.5), (7.5, 18.0)], a);
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

/// Paint the outline of a diagram shape kind fitted inside `r`, keeping the master's `w` to `h`
/// proportions (squashed towards square so a long, flat master still reads at 16 px). Used by the
/// Insert gallery, so every master gets its own icon from the same geometry the canvas draws.
pub fn paint_kind(p: &Painter, r: Rect, kind: vizcraft_diagram::ShapeKind, w: f32, h: f32, c: Color32, a: Color32) {
    let (w, h) = (if w.is_finite() && w > 0.0 { w } else { 1.0 }, if h.is_finite() && h > 0.0 { h } else { 1.0 });
    // Halfway between the true aspect and a square: a 140 by 50 terminator stays recognisable
    // without becoming a sliver.
    let aspect = ((w / h).clamp(0.25, 4.0)).sqrt();
    let side = r.width().min(r.height()) * 0.86;
    let (bw, bh) = if aspect >= 1.0 { (side, side / aspect) } else { (side * aspect, side) };
    let o = r.center() - vec2(bw / 2.0, bh / 2.0);
    let stroke = Stroke::new(stroke_width(r.width().min(r.height())), c);
    let to_screen = |pt: vizcraft_diagram::Point| pos2(o.x + pt.x as f32, o.y + pt.y as f32);
    for line in kind.outline(f64::from(bw), f64::from(bh)).flatten(0.25) {
        let pts: Vec<Pos2> = line.points.iter().map(|q| to_screen(*q)).collect();
        if line.closed {
            p.add(Shape::closed_line(pts, stroke));
        } else {
            p.add(Shape::line(pts, stroke));
        }
    }
    if let Some(detail) = kind.detail(f64::from(bw), f64::from(bh)) {
        for line in detail.flatten(0.25) {
            let pts: Vec<Pos2> = line.points.iter().map(|q| to_screen(*q)).collect();
            p.add(Shape::line(pts, Stroke::new(stroke.width, a)));
        }
    }
}
