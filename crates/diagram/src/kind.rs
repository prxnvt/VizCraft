//! Shape kinds: the built-in outlines, drawn in code from simple geometry.
//!
//! Each kind produces its outline as a [`Path`] in local coordinates (`(0, 0)` to `(w, h)`),
//! optional interior detail lines (the front rim of a cylinder, the side bars of a predefined
//! process), and default connection points in normalized `0..1` coordinates.

use crate::geom::{Path, Point, Rect, Transform, ray_hit};

/// Bezier handle length for a quarter ellipse (4/3 (sqrt 2 - 1)).
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// The built-in outline kinds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// Plain rectangle. Also what an unknown kind in a file loads as, so newer files still open.
    #[default]
    Rectangle,
    /// Rectangle with corners rounded by 15% of the shorter side.
    RoundedRectangle,
    /// Ellipse filling the frame.
    Ellipse,
    /// Rhombus touching the four side midpoints.
    Diamond,
    /// Isosceles triangle pointing up.
    Triangle,
    /// Parallelogram leaning right.
    Parallelogram,
    /// Hexagon with pointed left and right ends.
    Hexagon,
    /// Upright cylinder (a database drum).
    Cylinder,
    /// Rectangle with a wavy bottom edge (a printed page).
    Document,
    /// Cloud made of rounded lobes.
    Cloud,
    /// Stadium: a rectangle whose short ends are fully rounded (flowchart start and end).
    Terminator,
    /// Rectangle with a vertical bar inside each side (flowchart predefined process).
    PredefinedProcess,
    /// Quadrilateral whose top edge slopes up to the right (flowchart manual input).
    ManualInput,
    /// Elongated hexagon (flowchart preparation).
    Preparation,
    /// Trapezoid narrowing downward (flowchart manual operation).
    Trapezoid,
    /// Pentagon pointing down (flowchart off-page reference).
    OffPageReference,
}

crate::serde_util::named_enum!(ShapeKind, ShapeKind::Rectangle, {
    ShapeKind::Rectangle => "rectangle",
    ShapeKind::RoundedRectangle => "roundedRectangle",
    ShapeKind::Ellipse => "ellipse",
    ShapeKind::Diamond => "diamond",
    ShapeKind::Triangle => "triangle",
    ShapeKind::Parallelogram => "parallelogram",
    ShapeKind::Hexagon => "hexagon",
    ShapeKind::Cylinder => "cylinder",
    ShapeKind::Document => "document",
    ShapeKind::Cloud => "cloud",
    ShapeKind::Terminator => "terminator",
    ShapeKind::PredefinedProcess => "predefinedProcess",
    ShapeKind::ManualInput => "manualInput",
    ShapeKind::Preparation => "preparation",
    ShapeKind::Trapezoid => "trapezoid",
    ShapeKind::OffPageReference => "offPageReference",
});

impl ShapeKind {
    /// Every kind, in catalog order.
    pub const ALL: [ShapeKind; 16] = [
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Diamond,
        ShapeKind::Triangle,
        ShapeKind::Parallelogram,
        ShapeKind::Hexagon,
        ShapeKind::Cylinder,
        ShapeKind::Document,
        ShapeKind::Cloud,
        ShapeKind::Terminator,
        ShapeKind::PredefinedProcess,
        ShapeKind::ManualInput,
        ShapeKind::Preparation,
        ShapeKind::Trapezoid,
        ShapeKind::OffPageReference,
    ];

    /// The outline for a `w` by `h` frame, in local coordinates. Sizes are clamped to be
    /// non-negative and finite first.
    pub fn outline(self, w: f64, h: f64) -> Path {
        let (w, h) = (size(w), size(h));
        let p = Point::new;
        match self {
            ShapeKind::Rectangle | ShapeKind::PredefinedProcess => Path::polygon(&[p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)]),
            ShapeKind::RoundedRectangle => rounded_rect(w, h, 0.15 * w.min(h)),
            ShapeKind::Terminator => rounded_rect(w, h, 0.5 * w.min(h)),
            ShapeKind::Ellipse => ellipse(Rect::from_xywh(0.0, 0.0, w, h)),
            ShapeKind::Diamond => Path::polygon(&[p(w / 2.0, 0.0), p(w, h / 2.0), p(w / 2.0, h), p(0.0, h / 2.0)]),
            ShapeKind::Triangle => Path::polygon(&[p(w / 2.0, 0.0), p(w, h), p(0.0, h)]),
            ShapeKind::Parallelogram => {
                let k = 0.2 * w;
                Path::polygon(&[p(k, 0.0), p(w, 0.0), p(w - k, h), p(0.0, h)])
            }
            ShapeKind::Hexagon => hexagon(w, h, 0.25 * w),
            ShapeKind::Preparation => hexagon(w, h, 0.15 * w),
            ShapeKind::Trapezoid => {
                let k = 0.2 * w;
                Path::polygon(&[p(0.0, 0.0), p(w, 0.0), p(w - k, h), p(k, h)])
            }
            ShapeKind::ManualInput => Path::polygon(&[p(0.0, 0.25 * h), p(w, 0.0), p(w, h), p(0.0, h)]),
            ShapeKind::OffPageReference => Path::polygon(&[p(0.0, 0.0), p(w, 0.0), p(w, 0.6 * h), p(w / 2.0, h), p(0.0, 0.6 * h)]),
            ShapeKind::Cylinder => {
                let ry = cylinder_ry(w, h);
                let mut path = Path::new();
                path.move_to(p(0.0, ry));
                half_ellipse(&mut path, w / 2.0, ry, w / 2.0, ry, true);
                path.line_to(p(w, h - ry));
                half_ellipse(&mut path, w / 2.0, h - ry, w / 2.0, ry, false);
                path.close();
                path
            }
            ShapeKind::Document => {
                let base = 0.85 * h;
                let mut path = Path::new();
                path.move_to(p(0.0, 0.0)).line_to(p(w, 0.0)).line_to(p(w, base));
                // The second handle sits below the frame so the trough reaches about 93% of the height.
                path.cubic_to(p(0.7 * w, 0.7 * h), p(0.3 * w, 1.1 * h), p(0.0, base));
                path.close();
                path
            }
            ShapeKind::Cloud => cloud(w, h),
        }
    }

    /// Interior lines drawn on top of the fill but not part of the outline (open subpaths).
    pub fn detail(self, w: f64, h: f64) -> Option<Path> {
        let (w, h) = (size(w), size(h));
        match self {
            ShapeKind::Cylinder => {
                let ry = cylinder_ry(w, h);
                let mut path = Path::new();
                // The front half of the top rim, which the outline leaves out so the lid reads as
                // an open ellipse.
                path.move_to(Point::new(w, ry));
                half_ellipse(&mut path, w / 2.0, ry, w / 2.0, ry, false);
                Some(path)
            }
            ShapeKind::PredefinedProcess => {
                let k = (0.1 * w).min(20.0);
                let mut path = Path::new();
                path.move_to(Point::new(k, 0.0)).line_to(Point::new(k, h));
                path.move_to(Point::new(w - k, 0.0)).line_to(Point::new(w - k, h));
                Some(path)
            }
            _ => None,
        }
    }

    /// Default connection points in normalized `0..1` frame coordinates: where the outline meets
    /// the rays from the centre to the top, right, bottom and left, in that order.
    pub fn connection_points(self) -> Vec<Point> {
        // A square reference frame is enough: every outline scales with its frame except the
        // corner radii and cylinder caps, which do not move the side midpoints.
        const REF: f64 = 100.0;
        let lines = self.outline(REF, REF).flatten(0.05);
        let c = Point::new(REF / 2.0, REF / 2.0);
        [Point::new(0.0, -1.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0), Point::new(-1.0, 0.0)]
            .into_iter()
            .map(|dir| {
                let hit = ray_hit(c, dir, &lines).unwrap_or(c + dir * (REF / 2.0));
                Point::new((hit.x / REF).clamp(0.0, 1.0), (hit.y / REF).clamp(0.0, 1.0))
            })
            .collect()
    }
}

/// Clamp a frame dimension to a finite, non-negative value.
fn size(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, crate::MAX_EXTENT) } else { 0.0 }
}

fn cylinder_ry(w: f64, h: f64) -> f64 {
    (0.1 * h).min(0.15 * w).min(h / 2.0)
}

fn hexagon(w: f64, h: f64, k: f64) -> Path {
    let p = Point::new;
    Path::polygon(&[p(k, 0.0), p(w - k, 0.0), p(w, h / 2.0), p(w - k, h), p(k, h), p(0.0, h / 2.0)])
}

fn rounded_rect(w: f64, h: f64, r: f64) -> Path {
    let r = r.clamp(0.0, w.min(h) / 2.0);
    let p = Point::new;
    let k = r * (1.0 - KAPPA);
    let mut path = Path::new();
    path.move_to(p(r, 0.0)).line_to(p(w - r, 0.0));
    path.cubic_to(p(w - k, 0.0), p(w, k), p(w, r)).line_to(p(w, h - r));
    path.cubic_to(p(w, h - k), p(w - k, h), p(w - r, h)).line_to(p(r, h));
    path.cubic_to(p(k, h), p(0.0, h - k), p(0.0, h - r)).line_to(p(0.0, r));
    path.cubic_to(p(0.0, k), p(k, 0.0), p(r, 0.0));
    path.close();
    path
}

/// A full ellipse filling `r`, as four cubics starting at the right-hand point.
pub fn ellipse(r: Rect) -> Path {
    let c = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    let mut path = Path::new();
    path.move_to(p(c.x + rx, c.y));
    path.cubic_to(p(c.x + rx, c.y + ky), p(c.x + kx, c.y + ry), p(c.x, c.y + ry));
    path.cubic_to(p(c.x - kx, c.y + ry), p(c.x - rx, c.y + ky), p(c.x - rx, c.y));
    path.cubic_to(p(c.x - rx, c.y - ky), p(c.x - kx, c.y - ry), p(c.x, c.y - ry));
    path.cubic_to(p(c.x + kx, c.y - ry), p(c.x + rx, c.y - ky), p(c.x + rx, c.y));
    path.close();
    path
}

/// Append half an ellipse centred on `(cx, cy)`: the top half from left to right when `top`, the
/// bottom half from right to left otherwise. The pen must sit at the start point already.
fn half_ellipse(path: &mut Path, cx: f64, cy: f64, rx: f64, ry: f64, top: bool) {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    if top {
        path.cubic_to(p(cx - rx, cy - ky), p(cx - kx, cy - ry), p(cx, cy - ry));
        path.cubic_to(p(cx + kx, cy - ry), p(cx + rx, cy - ky), p(cx + rx, cy));
    } else {
        path.cubic_to(p(cx + rx, cy + ky), p(cx + kx, cy + ry), p(cx, cy + ry));
        path.cubic_to(p(cx - kx, cy + ry), p(cx - rx, cy + ky), p(cx - rx, cy));
    }
}

/// A cloud: lobes around an inner ellipse, scaled so its exact bounds fill the frame.
fn cloud(w: f64, h: f64) -> Path {
    const LOBES: usize = 9;
    let c = Point::new(0.5, 0.5);
    let on = |a: f64, k: f64| Point::new(c.x + 0.36 * k * a.cos(), c.y + 0.3 * k * a.sin());
    let step = std::f64::consts::TAU / LOBES as f64;
    // Start at an irregular angle so the cloud does not look like a flower.
    let start = -std::f64::consts::FRAC_PI_2 + 0.3;
    let mut unit = Path::new();
    unit.move_to(on(start, 1.0));
    for i in 0..LOBES {
        let a0 = start + step * i as f64;
        let a1 = a0 + step;
        let d = step * 0.15;
        // Alternate lobe heights so the outline is lumpy rather than regular.
        let k = if i % 2 == 0 { 1.55 } else { 1.4 };
        unit.cubic_to(on(a0 + d, k), on(a1 - d, k), on(a1, 1.0));
    }
    unit.close();
    let Some(b) = unit.bounds() else { return unit };
    let (bw, bh) = (b.width().max(1e-9), b.height().max(1e-9));
    let fit = Transform::translate(-b.x0, -b.y0).then(&Transform::scale(w / bw, h / bh));
    unit.transformed(&fit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_fill_their_frame() {
        for kind in ShapeKind::ALL {
            for (w, h) in [(120.0, 60.0), (40.0, 90.0), (1.0, 1.0)] {
                let b = kind.outline(w, h).bounds().unwrap();
                let eps = 1e-6 * w.max(h);
                assert!(b.x0 >= -eps && b.y0 >= -eps && b.x1 <= w + eps && b.y1 <= h + eps, "{kind:?} {w}x{h}: {b:?}");
                assert!(b.width() >= 0.999 * w, "{kind:?} width {b:?}");
                assert!(b.height() >= 0.9 * h, "{kind:?} height {b:?}");
            }
        }
    }

    #[test]
    fn zero_and_junk_sizes_do_not_panic() {
        for kind in ShapeKind::ALL {
            for (w, h) in [(0.0, 0.0), (f64::NAN, 5.0), (f64::INFINITY, -3.0), (1e300, 1e300)] {
                let path = kind.outline(w, h);
                let _ = path.bounds();
                let _ = path.flatten(0.5);
                let _ = kind.detail(w, h);
            }
        }
    }

    #[test]
    fn connection_points_sit_on_the_outline() {
        for kind in ShapeKind::ALL {
            let pts = kind.connection_points();
            assert_eq!(pts.len(), 4, "{kind:?}");
            for p in &pts {
                assert!((0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y), "{kind:?} {p:?}");
            }
        }
        let r = ShapeKind::Rectangle.connection_points();
        assert_eq!(r, vec![Point::new(0.5, 0.0), Point::new(1.0, 0.5), Point::new(0.5, 1.0), Point::new(0.0, 0.5)]);
        let par = ShapeKind::Parallelogram.connection_points();
        assert!((par[1].x - 0.9).abs() < 1e-9 && (par[3].x - 0.1).abs() < 1e-9, "{par:?}");
        let doc = ShapeKind::Document.connection_points();
        assert!(doc[2].y > 0.8 && doc[2].y < 0.95, "{doc:?}");
    }

    #[test]
    fn unknown_kind_loads_as_rectangle() {
        let k: ShapeKind = serde_json::from_str("\"hyperspaceTesseract\"").unwrap();
        assert_eq!(k, ShapeKind::Rectangle);
        assert_eq!(serde_json::to_string(&ShapeKind::PredefinedProcess).unwrap(), "\"predefinedProcess\"");
    }
}
