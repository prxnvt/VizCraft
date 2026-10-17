//! Automatic placement of shapes, built on `vizcraft-layout`.
//!
//! The page's shapes become layout nodes (sized by their bounding boxes) and the connectors glued
//! at both ends to two of them become layout edges. The `layout.*` commands and the Mermaid and
//! CSV importers all place shapes through [`layout_items`] (rearrange everything) or
//! [`layout_incremental`] (place new shapes around ones that stay put), so they agree on what a
//! layout does.

use std::collections::{HashMap, HashSet, VecDeque};

pub use vizcraft_layout::{Algorithm, Direction, Options};

use crate::MAX_EXTENT;
use crate::geom::{Point, Rect};
use crate::model::Page;

/// Where a whole-page layout puts its top-left corner, so the drawing does not touch the page edge.
pub const PAGE_MARGIN: f64 = 40.0;

/// Most pushes one new shape gets while clearing overlaps before it jumps past all content, so a
/// crowded page cannot make placement quadratic in the push count.
const MAX_PUSHES: usize = 64;

/// The layout graph for the shapes at `chosen` (indices into `page.shapes`): one node per shape,
/// one edge per connector glued at both ends to two different chosen shapes.
fn graph_of(page: &Page, chosen: &[usize]) -> (vizcraft_layout::Graph, Vec<(usize, usize)>) {
    let mut node_of: HashMap<&str, usize> = HashMap::new();
    let mut graph = vizcraft_layout::Graph::default();
    for (n, &i) in chosen.iter().enumerate() {
        if let Some(s) = page.shapes.get(i) {
            let b = s.bounds();
            node_of.insert(s.id.as_str(), n);
            graph.nodes.push(vizcraft_layout::Node { width: (b.x1 - b.x0).clamp(1.0, 1.0e5), height: (b.y1 - b.y0).clamp(1.0, 1.0e5) });
        }
    }
    let mut pairs = Vec::new();
    for c in &page.connectors {
        let (Some(a), Some(b)) = (c.start.shape().and_then(|s| node_of.get(s)), c.end.shape().and_then(|s| node_of.get(s))) else { continue };
        if a != b {
            graph.edges.push(vizcraft_layout::Edge::new(*a, *b));
            pairs.push((*a, *b));
        }
    }
    (graph, pairs)
}

/// Move shape `i` so its bounding box's top-left lands on `at` (the bounding box differs from the
/// frame when the shape is rotated).
fn place(page: &mut Page, i: usize, at: Point) {
    if let Some(s) = page.shapes.get_mut(i) {
        let b = s.bounds();
        s.x += at.x - b.x0;
        s.y += at.y - b.y0;
        s.sanitize();
    }
}

/// Drop the stored bends of connectors touching a moved shape: they were drawn for the old
/// positions (connectors that avoid shapes get new ones from the engine's routing refresh).
fn clear_bends(page: &mut Page, moved: &HashSet<String>) {
    for c in &mut page.connectors {
        if c.start.shape().is_some_and(|s| moved.contains(s)) || c.end.shape().is_some_and(|s| moved.contains(s)) {
            c.waypoints.clear();
        }
    }
}

/// Grow the page so every shape fits with [`PAGE_MARGIN`] to spare on the right and bottom. The
/// page never shrinks, and never grows past [`MAX_EXTENT`].
pub fn grow_page(page: &mut Page) {
    let Some(b) =
        page.shapes.iter().map(|s| s.bounds()).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
    else {
        return;
    };
    let (w, h) = ((b.x1 + PAGE_MARGIN).min(MAX_EXTENT), (b.y1 + PAGE_MARGIN).min(MAX_EXTENT));
    if w > page.width {
        page.width = w;
    }
    if h > page.height {
        page.height = h;
    }
}

/// Lay out the shapes named by `ids` (or every shape on the page when `None`) with `opts`.
///
/// A selection keeps its previous top-left corner; a whole page starts at [`PAGE_MARGIN`].
/// Connectors between moved shapes lose their stored bends, because those were drawn for the old
/// positions (connectors that avoid shapes get new ones from the engine's routing refresh). The
/// page grows to fit the result. Returns the layout's notes (for example a tree layout falling
/// back to layered), or the reason the layout was refused.
pub fn layout_items(page: &mut Page, ids: Option<&[String]>, opts: &Options) -> Result<Vec<String>, String> {
    let wanted: Option<HashSet<&str>> = ids.map(|ids| ids.iter().map(String::as_str).collect());
    let chosen: Vec<usize> =
        page.shapes.iter().enumerate().filter(|(_, s)| wanted.as_ref().is_none_or(|w| w.contains(s.id.as_str()))).map(|(i, _)| i).collect();
    if chosen.is_empty() {
        return Ok(Vec::new());
    }
    let (graph, _) = graph_of(page, &chosen);
    let origin = if ids.is_some() {
        chosen
            .iter()
            .filter_map(|&i| page.shapes.get(i))
            .map(|s| s.bounds())
            .fold(None, |acc: Option<Point>, b| Some(acc.map_or(Point::new(b.x0, b.y0), |p| Point::new(p.x.min(b.x0), p.y.min(b.y0)))))
    } else {
        None
    }
    .unwrap_or(Point::new(PAGE_MARGIN, PAGE_MARGIN));
    let out = vizcraft_layout::layout(&graph, opts).map_err(|e| e.to_string())?;
    let mut moved: HashSet<String> = HashSet::new();
    for (n, &i) in chosen.iter().enumerate() {
        let Some(pos) = out.nodes.get(n) else { continue };
        place(page, i, Point::new(origin.x + pos.x, origin.y + pos.y));
        if let Some(s) = page.shapes.get(i) {
            moved.insert(s.id.clone());
        }
    }
    clear_bends(page, &moved);
    grow_page(page);
    Ok(out.notes)
}

/// Place the shapes named by `new_ids` and leave every other shape exactly where it is.
///
/// The whole graph is laid out once to learn where each new shape belongs relative to its
/// neighbours. A new shape connected (directly or through other new shapes) to a pinned shape is
/// put at that neighbour's real position plus the layout's offset between the two, so in a
/// top-to-bottom layered layout a child lands below its parent, in left-to-right to its right,
/// and so on. New shapes with no path to a pinned shape keep their layout arrangement and go to
/// the right of the existing content. Finally each new shape is pushed sideways (across the flow)
/// until it overlaps nothing. With no pinned shapes this is [`layout_items`] on the new shapes
/// (or the whole page). Unknown ids are ignored. Returns the layout's notes.
pub fn layout_incremental(page: &mut Page, new_ids: &[String], opts: &Options) -> Result<Vec<String>, String> {
    let wanted: HashSet<&str> = new_ids.iter().map(String::as_str).collect();
    let is_new: Vec<bool> = page.shapes.iter().map(|s| wanted.contains(s.id.as_str())).collect();
    if !is_new.iter().any(|n| *n) {
        return Ok(Vec::new());
    }
    if is_new.iter().all(|n| *n) {
        return layout_items(page, None, opts);
    }
    let all: Vec<usize> = (0..page.shapes.len()).collect();
    let (graph, pairs) = graph_of(page, &all);
    let out = vizcraft_layout::layout(&graph, opts).map_err(|e| e.to_string())?;
    let rel = |i: usize| out.nodes.get(i).copied().map_or(Point::ZERO, |p| Point::new(p.x, p.y));
    let corner = |page: &Page, i: usize| {
        page.shapes.get(i).map_or(Point::ZERO, |s| {
            let b = s.bounds();
            Point::new(b.x0, b.y0)
        })
    };
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); page.shapes.len()];
    for &(a, b) in &pairs {
        if let Some(v) = neighbours.get_mut(a) {
            v.push(b);
        }
        if let Some(v) = neighbours.get_mut(b) {
            v.push(a);
        }
    }
    // Breadth first out from the pinned shapes, so each new shape is anchored on the nearest
    // already placed neighbour (pinned shapes first, in page order: deterministic).
    let mut at: Vec<Option<Point>> = (0..page.shapes.len()).map(|i| (!is_new.get(i).copied().unwrap_or(true)).then(|| corner(page, i))).collect();
    let mut queue: VecDeque<usize> = (0..page.shapes.len()).filter(|&i| at.get(i).is_some_and(Option::is_some)).collect();
    while let Some(m) = queue.pop_front() {
        let Some(Some(base)) = at.get(m).copied() else { continue };
        let mut next: Vec<usize> = neighbours.get(m).cloned().unwrap_or_default();
        next.sort_unstable();
        next.dedup();
        for n in next {
            if at.get(n).is_some_and(Option::is_none) {
                let (rn, rm) = (rel(n), rel(m));
                if let Some(slot) = at.get_mut(n) {
                    *slot = Some(Point::new(base.x + rn.x - rm.x, base.y + rn.y - rm.y));
                }
                queue.push_back(n);
            }
        }
    }
    // Unconnected new shapes: their own arrangement, to the right of everything placed so far.
    let pinned_bounds = (0..page.shapes.len())
        .filter(|&i| !is_new.get(i).copied().unwrap_or(true))
        .filter_map(|i| page.shapes.get(i).map(|s| s.bounds()))
        .reduce(|a, b| a.union(&b));
    let loose: Vec<usize> = (0..page.shapes.len()).filter(|&i| at.get(i).is_some_and(Option::is_none)).collect();
    if let (Some(pb), false) = (pinned_bounds, loose.is_empty()) {
        let gap = spacing(opts.component_spacing);
        let lx = loose.iter().map(|&i| rel(i).x).fold(f64::INFINITY, f64::min);
        let ly = loose.iter().map(|&i| rel(i).y).fold(f64::INFINITY, f64::min);
        for &i in &loose {
            if let Some(slot) = at.get_mut(i) {
                *slot = Some(Point::new(pb.x1 + gap + rel(i).x - lx, pb.y0 + rel(i).y - ly));
            }
        }
    }
    let mut moved: HashSet<String> = HashSet::new();
    let news: Vec<usize> = (0..page.shapes.len()).filter(|&i| is_new.get(i).copied().unwrap_or(false)).collect();
    for &i in &news {
        if let Some(Some(p)) = at.get(i).copied() {
            place(page, i, p);
        }
    }
    // Clear overlaps: settled shapes are the pinned ones plus the new ones already cleared.
    let gap = spacing(opts.node_spacing);
    let across = if opts.direction.is_horizontal() && matches!(opts.algorithm, Algorithm::Layered | Algorithm::Tree) {
        Point::new(0.0, 1.0)
    } else {
        Point::new(1.0, 0.0)
    };
    let mut settled: Vec<Rect> =
        (0..page.shapes.len()).filter(|&i| !is_new.get(i).copied().unwrap_or(true)).filter_map(|i| page.shapes.get(i).map(|s| s.bounds())).collect();
    for &i in &news {
        let Some(mut b) = page.shapes.get(i).map(|s| s.bounds()) else { continue };
        let mut pushes = 0;
        while let Some(o) = settled.iter().find(|o| overlaps_strictly(&b.inflate(gap / 2.0), &o.inflate(gap / 2.0))).copied() {
            let shift = if pushes < MAX_PUSHES {
                if across.x > 0.0 { o.x1 + gap - b.x0 } else { o.y1 + gap - b.y0 }
            } else {
                // Crowded: jump past everything settled in one step.
                let far = settled.iter().fold(f64::NEG_INFINITY, |m, r| m.max(if across.x > 0.0 { r.x1 } else { r.y1 }));
                if across.x > 0.0 { far + gap - b.x0 } else { far + gap - b.y0 }
            };
            if !(shift.is_finite() && shift > 0.0) {
                break;
            }
            b = Rect::new(b.x0 + across.x * shift, b.y0 + across.y * shift, b.x1 + across.x * shift, b.y1 + across.y * shift);
            pushes += 1;
            if pushes > MAX_PUSHES + 1 {
                break;
            }
        }
        place(page, i, Point::new(b.x0, b.y0));
        if let Some(s) = page.shapes.get(i) {
            settled.push(s.bounds());
            moved.insert(s.id.clone());
        }
    }
    clear_bends(page, &moved);
    grow_page(page);
    Ok(out.notes)
}

/// Whether two rectangles overlap with positive area (touching edges do not count).
fn overlaps_strictly(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// A spacing option made safe to add to coordinates.
fn spacing(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, 1.0e4) } else { 40.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Connector, End, Shape};
    use crate::{MAX_EXTENT, ShapeKind};

    fn no_overlaps(page: &Page) {
        for (i, s) in page.shapes.iter().enumerate() {
            for t in page.shapes.iter().skip(i + 1) {
                assert!(!overlaps_strictly(&s.bounds(), &t.bounds()), "{} and {} overlap", s.id, t.id);
            }
        }
    }

    /// A small tree on top of itself: r -> a, r -> b, a -> c, b -> d, plus a loose shape.
    fn tree_page() -> Page {
        let mut page = Page::new("p", "p");
        for id in ["r", "a", "b", "c", "d", "loose"] {
            page.shapes.push(Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        }
        for (a, b) in [("r", "a"), ("r", "b"), ("a", "c"), ("b", "d")] {
            page.connectors.push(Connector::new(format!("{a}{b}"), End::dynamic(a), End::dynamic(b)));
        }
        page
    }

    #[test]
    fn each_algorithm_leaves_no_overlaps() {
        for algorithm in [Algorithm::Layered, Algorithm::Tree, Algorithm::Radial, Algorithm::Force] {
            for direction in [Direction::TopBottom, Direction::BottomTop, Direction::LeftRight, Direction::RightLeft] {
                let mut page = tree_page();
                layout_items(&mut page, None, &Options { algorithm, direction, ..Options::default() }).unwrap();
                no_overlaps(&page);
            }
        }
    }

    #[test]
    fn the_page_grows_to_fit_but_never_shrinks() {
        let mut page = tree_page();
        page.width = 100.0;
        page.height = 100.0;
        layout_items(&mut page, None, &Options::default()).unwrap();
        let right = page.shapes.iter().map(|s| s.bounds().x1).fold(0.0, f64::max);
        assert!((page.width - (right + PAGE_MARGIN)).abs() < 1e-9);
        assert!(page.height > 100.0);
        let mut big = tree_page();
        big.width = 5000.0;
        layout_items(&mut big, None, &Options::default()).unwrap();
        assert_eq!(big.width, 5000.0);
        big.shapes.push(Shape::new("far", ShapeKind::Rectangle, MAX_EXTENT * 3.0, 0.0, 10.0, 10.0));
        grow_page(&mut big);
        assert!(big.width <= MAX_EXTENT);
    }

    /// A pinned parent at (500, 400) with a new child glued below it, and a new loose shape.
    fn incremental(direction: Direction) -> Page {
        let mut page = Page::new("p", "p");
        page.shapes.push(Shape::new("p", ShapeKind::Rectangle, 500.0, 400.0, 100.0, 60.0));
        page.shapes.push(Shape::new("other", ShapeKind::Rectangle, 500.0, 600.0, 100.0, 60.0));
        page.shapes.push(Shape::new("kid", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("kid2", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("loose", ShapeKind::Rectangle, 0.0, 0.0, 80.0, 40.0));
        page.connectors.push(Connector::new("c1", End::dynamic("p"), End::dynamic("kid")));
        page.connectors.push(Connector::new("c2", End::dynamic("kid"), End::dynamic("kid2")));
        let opts = Options { direction, ..Options::default() };
        let ids: Vec<String> = ["kid", "kid2", "loose"].iter().map(|s| s.to_string()).collect();
        layout_incremental(&mut page, &ids, &opts).unwrap();
        page
    }

    #[test]
    fn incremental_layout_pins_old_shapes_and_puts_children_downstream() {
        for direction in [Direction::TopBottom, Direction::BottomTop, Direction::LeftRight, Direction::RightLeft] {
            let page = incremental(direction);
            let s = |id: &str| page.shape(id).unwrap().bounds();
            assert_eq!((page.shape("p").unwrap().x, page.shape("p").unwrap().y), (500.0, 400.0), "{direction:?}");
            assert_eq!((page.shape("other").unwrap().x, page.shape("other").unwrap().y), (500.0, 600.0), "{direction:?}");
            no_overlaps(&page);
            let (p, kid, kid2) = (s("p"), s("kid"), s("kid2"));
            match direction {
                Direction::TopBottom => assert!(kid.y0 >= p.y1 && kid2.y0 >= kid.y1, "{direction:?}"),
                Direction::BottomTop => assert!(kid.y1 <= p.y0 && kid2.y1 <= kid.y0, "{direction:?}"),
                Direction::LeftRight => assert!(kid.x0 >= p.x1 && kid2.x0 >= kid.x1, "{direction:?}"),
                Direction::RightLeft => assert!(kid.x1 <= p.x0 && kid2.x1 <= kid.x0, "{direction:?}"),
            }
            // The loose shape sits right of the pinned content.
            assert!(s("loose").x0 >= p.x1, "{direction:?}");
        }
    }

    #[test]
    fn incremental_with_nothing_pinned_or_nothing_new() {
        let mut page = tree_page();
        let before = page.clone();
        assert!(layout_incremental(&mut page, &["nope".to_string()], &Options::default()).unwrap().is_empty());
        assert_eq!(page, before);
        let all: Vec<String> = page.shapes.iter().map(|s| s.id.clone()).collect();
        layout_incremental(&mut page, &all, &Options::default()).unwrap();
        no_overlaps(&page);
        // Hostile spacing is refused, not a panic.
        let bad = Options { node_spacing: f64::NAN, ..Options::default() };
        assert!(layout_incremental(&mut page, &all[..2], &bad).is_err());
    }
}
