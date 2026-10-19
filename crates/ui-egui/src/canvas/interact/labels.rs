//! Hit testing connector labels: the main label and the two end labels (a role and multiplicity
//! beside an association end). A label is measured as `draw::connectors` lays it out, so a wide
//! label off a thin line still edits; an empty end label is reached by a double-click on the
//! route's first or last stretch when the connector's relationship type shows labels at that end.

use egui::{Pos2, Rect, Ui};
use vizcraft_diagram::{self as dg, Connector, Point};

use super::edit_text::LabelEnd;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::draw::pos;
use crate::canvas::draw::text::{label_layout, style_font};

/// Which label of a connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The label halfway along the route.
    Main,
    /// The label beside the start end.
    Start,
    /// The label beside the end end.
    End,
}

impl Part {
    /// The end an end label belongs to; `None` for the main label.
    pub fn end(self) -> Option<LabelEnd> {
        match self {
            Part::Main => None,
            Part::Start => Some(LabelEnd::Start),
            Part::End => Some(LabelEnd::End),
        }
    }
}

/// Slack around a label box that still counts as on it, in screen points.
const SLACK: f32 = 3.0;
/// The share of the route, from each end, that an empty end label answers to.
const END_SHARE: f64 = 0.15;

/// The label of a connector at screen point `p`, if any: a drawn label's box (main first), else an
/// empty end label's stretch of the route. The topmost connector wins.
pub fn label_at(app: &CraftApp, ui: &Ui, vp: &Viewport2D, p: Pos2) -> Option<(String, Part)> {
    let page = app.session.doc.page()?;
    let painter = ui.painter();
    let zoom = vp.zoom;
    for c in page.connectors.iter().rev() {
        let g = dg::connector_geometry(page, c);
        let st = &c.style;
        let font = style_font(st, zoom);
        let boxed = |text: &str, centre: Point| -> Option<Rect> {
            let font = font.clone()?;
            let galley = label_layout(painter, text, st, font, 200.0 * zoom);
            let centre = vp.world_to_screen(pos(centre));
            Some(galley.rect.translate(centre - galley.rect.center().to_vec2().to_pos2()).expand(2.0 * zoom + SLACK))
        };
        if !c.text.trim().is_empty() && boxed(&c.text, g.label_pos).is_some_and(|r| r.contains(p)) {
            return Some((c.id.clone(), Part::Main));
        }
        for (text, at_start, part) in [(&c.start_label, true, Part::Start), (&c.end_label, false, Part::End)] {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let Some(anchor) = dg::arrows::end_label_anchor(&g.polyline, at_start, st.font_size, st.line_width) else { continue };
            if boxed(text, anchor).is_some_and(|r| r.contains(p)) {
                return Some((c.id.clone(), part));
            }
        }
        if let Some(part) = empty_end_at(
            app,
            c,
            &g.polyline,
            dg::Point::new(f64::from(vp.screen_to_world(p).x), f64::from(vp.screen_to_world(p).y)),
            f64::from(super::HIT / zoom),
        ) {
            return Some((c.id.clone(), part));
        }
    }
    None
}

/// The empty end label a point on the route reaches: within `tol` of the route and in its first
/// or last [`END_SHARE`], on a connector whose relationship type declares labels at that end.
fn empty_end_at(app: &CraftApp, c: &Connector, line: &[Point], at: Point, tol: f64) -> Option<Part> {
    let rt = c.relationship.as_deref().and_then(|r| app.session.doc.model.relationship(r))?;
    let (_, ty) = vizcraft_notation::catalog().relationship_type(&rt.type_id)?;
    let (start_ok, end_ok) =
        (!ty.end_labels.start.is_empty() && c.start_label.trim().is_empty(), !ty.end_labels.end.is_empty() && c.end_label.trim().is_empty());
    if !start_ok && !end_ok {
        return None;
    }
    let (along, dist, total) = project(line, at)?;
    if dist > tol || total <= 0.0 {
        return None;
    }
    let share = along / total;
    if start_ok && share <= END_SHARE {
        Some(Part::Start)
    } else if end_ok && share >= 1.0 - END_SHARE {
        Some(Part::End)
    } else {
        None
    }
}

/// Where `p` projects onto the polyline: the distance along it, the distance from it and its
/// total length.
fn project(line: &[Point], p: Point) -> Option<(f64, f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    let (mut run, mut total) = (0.0, 0.0);
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt();
        let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (qx, qy) = (a.x + t * dx, a.y + t * dy);
        let d = ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt();
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((run + t * len, d));
        }
        run += len;
        total += len;
    }
    best.map(|(along, d)| (along, d, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_measures_along_and_off_the_route() {
        let line = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0)];
        let (along, d, total) = project(&line, Point::new(50.0, 3.0)).unwrap();
        assert!((along - 50.0).abs() < 1e-9 && (d - 3.0).abs() < 1e-9 && (total - 200.0).abs() < 1e-9);
        let (along, ..) = project(&line, Point::new(110.0, 100.0)).unwrap();
        assert!((along - 200.0).abs() < 1e-9, "clamped to the end");
        assert!(project(&line[..1], Point::ZERO).is_none());
    }
}
