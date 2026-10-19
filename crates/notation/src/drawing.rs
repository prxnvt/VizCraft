//! Drawing-level checks: what is wrong with the picture rather than the model.
//!
//! The built-in `drawing` pack (`packs/drawing.json`) has no glyphs and no types, only rules any
//! page can be held to by enabling its ruleset: a connector crossing a shape, a connector end
//! not glued, two shapes overlapping, a shape off the page, text overflowing its block. Their
//! issues name no model items (`subjects` is empty); `items` are the shapes and connectors and
//! `page` is the page they are on.
//!
//! Two facts come from one place each so later work can swap them: whether a connector crosses
//! a shape ([`crosses`], today `autoroute::clear_of_shapes`) and the page rectangle ([`board`],
//! today the fixed page size).

use std::collections::HashSet;

use serde_json::json;
use vizcraft_diagram::{Connector, Document, End, Page, Point, Rect, Shape};
use vizcraft_model::Issue;
use vizcraft_model::issue::Fix;

use crate::rules::{MAX_LABEL, MAX_PER_RULE, cut, message};
use crate::schema::{Check, RuleDecl};

/// Farthest a free connector end may be from a connection point for the glue fix, in world
/// units; past it the nearest point is too far away to be what was meant.
pub const MAX_GLUE_DISTANCE: f64 = 400.0;

/// Slack when comparing frames, in world units, so shapes snapped edge to edge or to the page
/// border do not count as overlapping or off the page.
const SLACK: f64 = 0.5;

/// Deepest parent chain walked (groups and containers inside each other).
const MAX_PARENTS: usize = 64;

/// Average glyph width as a share of the font size, for the line-count estimate.
const CHAR_WIDTH: f64 = 0.55;

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f64 = 1.2;

/// The connectors on `page` whose drawn route runs through a shape they are not glued to, in
/// page order, each once: the routing crate's own oracle (`autoroute::crossings`), so the check
/// and the router agree on what a crossing is.
pub fn crossing_connectors(page: &Page) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    vizcraft_diagram::autoroute::crossings(page).into_iter().map(|x| x.connector).filter(|id| seen.insert(id.clone())).collect()
}

/// The page rectangle shapes should stay inside, or `None` when the page has no edge: an
/// infinite board has none, and a fixed board's edge is its frame. The one place drawing checks
/// read the page size.
pub fn board(page: &Page) -> Option<Rect> {
    if page.board != vizcraft_diagram::Board::Fixed {
        return None;
    }
    let f = page.frame();
    (f.width().is_finite() && f.height().is_finite() && f.width() > 0.0 && f.height() > 0.0).then_some(f)
}

/// Run one drawing check; other checks are ignored.
pub(crate) fn run(doc: &Document, rule: &RuleDecl, out: &mut Vec<Issue>) {
    for page in &doc.pages {
        if out.len() >= MAX_PER_RULE {
            return;
        }
        match rule.check {
            Check::CrossesShape => crossing(page, rule, out),
            Check::UngluedEnd => unglued(page, rule, out),
            Check::Overlap => overlap(page, rule, out),
            Check::OffBoard => off_board(page, rule, out),
            Check::TextOverflow => overflow(page, rule, out),
            _ => {}
        }
    }
}

/// A drawing issue on `page` pointing at `items`.
fn issue(rule: &RuleDecl, what: &str, page: &Page, items: Vec<String>) -> Issue {
    Issue::new(rule.id.clone(), rule.severity, message(&rule.description, what)).with_items(items).on_page(page.id.clone())
}

/// A shape as a message names it: its quoted text, else its id.
fn shape_label(s: &Shape) -> String {
    let t = s.text.trim();
    if t.is_empty() { format!("shape {}", cut(&s.id, MAX_LABEL)) } else { quote(t) }
}

fn connector_label(c: &Connector) -> String {
    let t = c.text.trim();
    if t.is_empty() { format!("Connector {}", cut(&c.id, MAX_LABEL)) } else { format!("Connector {}", quote(t)) }
}

fn quote(t: &str) -> String {
    let one_line: String = t.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    format!("\"{}\"", cut(&one_line, MAX_LABEL))
}

/// Shapes that are drawn, not just grouping: everything but group shapes and generated keys.
fn solid(page: &Page) -> impl Iterator<Item = &Shape> {
    page.shapes.iter().filter(|s| !vizcraft_diagram::arrange::is_group(s) && !vizcraft_diagram::key::is_key(s))
}

fn finite(r: &Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())
}

fn crossing(page: &Page, rule: &RuleDecl, out: &mut Vec<Issue>) {
    for id in crossing_connectors(page) {
        if out.len() >= MAX_PER_RULE {
            return;
        }
        let Some(c) = page.connector(&id).filter(|c| !vizcraft_diagram::key::is_key_connector(c)) else { continue };
        let what = format!("{} runs through a shape it is not glued to.", connector_label(c));
        let fix = Fix::new("Reroute around shapes", "connector.reroute", json!({"ids": [c.id], "avoid": true}));
        out.push(issue(rule, &what, page, vec![c.id.clone()]).with_fix(fix));
    }
}

/// The connection point nearest `at` within [`MAX_GLUE_DISTANCE`]: shape and point index.
fn nearest_point(page: &Page, at: Point) -> Option<(&Shape, usize)> {
    let mut best: Option<(&Shape, usize, f64)> = None;
    for s in solid(page) {
        for (i, p) in s.connection_points_world().into_iter().enumerate() {
            let d = ((p.x - at.x).powi(2) + (p.y - at.y).powi(2)).sqrt();
            if d.is_finite() && d <= MAX_GLUE_DISTANCE && best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((s, i, d));
            }
        }
    }
    best.map(|(s, i, _)| (s, i))
}

fn unglued(page: &Page, rule: &RuleDecl, out: &mut Vec<Issue>) {
    for c in page.connectors.iter().filter(|c| !vizcraft_diagram::key::is_key_connector(c)) {
        if out.len() >= MAX_PER_RULE {
            return;
        }
        let ends = [("start", &c.start), ("end", &c.end)];
        let free: Vec<(&str, Point)> = ends
            .iter()
            .filter_map(|(name, e)| match e {
                End::Free { x, y } => Some((*name, Point::new(*x, *y))),
                End::Glued { .. } => None,
            })
            .collect();
        let which = match free.as_slice() {
            [] => continue,
            [(name, _)] => format!("its {name} free"),
            _ => "both ends free".to_string(),
        };
        let mut is = issue(rule, &format!("{} has {which}.", connector_label(c)), page, vec![c.id.clone()]);
        for (name, at) in free {
            if let Some((s, i)) = nearest_point(page, at) {
                let label = format!("Glue the {name} to {}", shape_label(s));
                is = is.with_fix(Fix::new(label, "connector.glue", json!({"id": c.id, "end": name, "shape": s.id, "point": i})));
            }
        }
        out.push(is);
    }
}

/// The ids of the groups and containers `s` sits in, nearest first.
fn parents<'p>(page: &'p Page, s: &'p Shape) -> Vec<&'p str> {
    let mut out: Vec<&str> = Vec::new();
    let mut at = s.parent.as_deref();
    while let Some(p) = at {
        if out.len() >= MAX_PARENTS || out.contains(&p) || p == s.id {
            break;
        }
        out.push(p);
        at = page.shape(p).and_then(|ps| ps.parent.as_deref());
    }
    out
}

fn overlap(page: &Page, rule: &RuleDecl, out: &mut Vec<Issue>) {
    // Sweep along x: shapes sorted by left edge, each compared only with those that start
    // before it ends, so a tidy page costs about n log n.
    let shapes: Vec<(&Shape, Rect, Vec<&str>)> =
        solid(page).map(|s| (s, s.bounds(), parents(page, s))).filter(|(_, b, _)| finite(b) && b.width() > SLACK && b.height() > SLACK).collect();
    let mut order: Vec<usize> = (0..shapes.len()).collect();
    order.sort_by(|&a, &b| shapes.get(a).map(|s| s.1.x0).unwrap_or_default().total_cmp(&shapes.get(b).map(|s| s.1.x0).unwrap_or_default()));
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    for (k, &i) in order.iter().enumerate() {
        let Some((a, ra, pa)) = shapes.get(i) else { continue };
        for &j in order.iter().skip(k.saturating_add(1)) {
            let Some((b, rb, pb)) = shapes.get(j) else { continue };
            if rb.x0 >= ra.x1 - SLACK {
                break;
            }
            let overlaps = rb.x0 < ra.x1 - SLACK && ra.x0 < rb.x1 - SLACK && rb.y0 < ra.y1 - SLACK && ra.y0 < rb.y1 - SLACK;
            if !overlaps || pa.contains(&b.id.as_str()) || pb.contains(&a.id.as_str()) {
                continue;
            }
            // Items in z order, so the key does not depend on the sort.
            let (first, second) = if i < j { (a, b) } else { (b, a) };
            if !seen.insert((i.min(j), i.max(j))) {
                continue;
            }
            let what = format!("{} and {} overlap.", capital(&shape_label(first)), shape_label(second));
            out.push(issue(rule, &what, page, vec![first.id.clone(), second.id.clone()]));
            if out.len() >= MAX_PER_RULE {
                return;
            }
        }
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn off_board(page: &Page, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let Some(b) = board(page) else { return };
    let b = b.inflate(SLACK);
    for s in solid(page) {
        let r = s.bounds();
        if !finite(&r) || b.contains_rect(&r) {
            continue;
        }
        let where_ = if r.x1 < b.x0 || r.x0 > b.x1 || r.y1 < b.y0 || r.y0 > b.y1 { "is off the page" } else { "runs over the page edge" };
        let what = format!("{} {where_}.", capital(&shape_label(s)));
        let fix = Fix::new("Fit the page to the drawing", "page.fitToDrawing", json!({"page": page.id}));
        out.push(issue(rule, &what, page, vec![s.id.clone()]).with_fix(fix));
        if out.len() >= MAX_PER_RULE {
            return;
        }
    }
}

/// How many lines `text` takes in a block `width` wide at `font_size`, estimated from an average
/// glyph width; each paragraph takes at least one line.
pub fn estimated_lines(text: &str, width: f64, font_size: f64) -> usize {
    let per_char = (font_size * CHAR_WIDTH).max(f64::MIN_POSITIVE);
    let fit = (width / per_char).floor();
    let per_line = if fit >= 1.0 { fit.min(1.0e6) as usize } else { 1 };
    text.split('\n').map(|p| p.chars().count().div_ceil(per_line).max(1)).fold(0usize, usize::saturating_add)
}

fn overflow(page: &Page, rule: &RuleDecl, out: &mut Vec<Issue>) {
    for s in solid(page) {
        let font = s.style.font_size;
        if s.text.trim().is_empty() || !font.is_finite() || font <= 0.0 {
            continue;
        }
        let (r, _) = s.text_rect();
        let m = &s.style.text_margins;
        let (w, h) = (r.width() - m.left - m.right, r.height() - m.top - m.bottom);
        if !w.is_finite() || !h.is_finite() || w <= 0.0 {
            continue;
        }
        let lines = estimated_lines(&s.text, w, font);
        let needed = lines as f64 * font * LINE_HEIGHT;
        if needed > h.max(0.0) + SLACK {
            let what = format!("The text of {} needs about {lines} lines at {font} pt and does not fit its block.", shape_label(s));
            out.push(issue(rule, &what, page, vec![s.id.clone()]));
            if out.len() >= MAX_PER_RULE {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_estimate_counts_paragraphs_and_wraps() {
        assert_eq!(estimated_lines("Start", 100.0, 12.0), 1);
        // 100 / 6.6 = 15 characters a line.
        assert_eq!(estimated_lines(&"x".repeat(31), 100.0, 12.0), 3);
        assert_eq!(estimated_lines("a\n\nb", 100.0, 12.0), 3);
        assert_eq!(estimated_lines("abc", 0.001, 12.0), 3);
        assert_eq!(estimated_lines("abc", f64::INFINITY, 12.0), 1);
    }

    #[test]
    fn board_is_the_page_rectangle_of_a_fixed_board_only() {
        let mut p = Page::new("p1", "P");
        assert!(board(&p).is_none(), "a new page is an infinite board, which has no edge");
        p.board = vizcraft_diagram::Board::Fixed;
        assert!(board(&p).is_some());
        p.width = 0.0;
        assert!(board(&p).is_none());
    }
}
