//! Arrange geometry: align, distribute, auto space, z order, flip and rotate, and groups.
//!
//! A group is an ordinary [`Shape`] whose `stencil` is [`GROUP_STENCIL`]: it draws nothing of its
//! own (transparent fill and line, no text), its members name it as their `parent`, and its frame
//! is kept as the bounds of its members ([`refit_groups`]). It sits in z order just below its
//! first member, so a click on a member hits the member and the selection code walks up to the
//! group, while a click on blank space inside the group hits the group itself.
//!
//! Every function works on one [`Page`] and never panics: ids that name nothing are skipped,
//! parent chains are walked at most [`MAX_GROUP_DEPTH`] steps, and coordinates are clamped to
//! [`MAX_COORD`]. Moving, resizing or rotating a group carries its members (nested groups too).

use std::collections::{HashMap, HashSet};

use crate::geom::{MAX_COORD, Point, Rect, Transform};
use crate::model::{Color, Page, Shape};
use crate::{MAX_EXTENT, ShapeKind};

/// The `stencil` that marks a shape as a group.
pub const GROUP_STENCIL: &str = "vizcraft.group";

/// Most parent steps walked from one shape; deeper nesting is treated as top level.
pub const MAX_GROUP_DEPTH: usize = 64;

/// Gap auto space leaves between connected shapes when none is given, in world units.
pub const DEFAULT_GAP: f64 = 40.0;

/// Whether `s` is a group.
pub fn is_group(s: &Shape) -> bool {
    s.stencil == GROUP_STENCIL
}

/// A new, empty group shape: no fill, no line, no text. Its frame is set by [`refit_groups`].
pub fn new_group(id: impl Into<String>) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 0.0, 0.0);
    s.stencil = GROUP_STENCIL.to_string();
    s.style.fill = Color::TRANSPARENT;
    s.style.line = Color::TRANSPARENT;
    s
}

/// A coordinate clamped to the model's range.
fn coord(v: f64) -> f64 {
    if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 }
}

/// Parent of each shape by id.
fn parents(page: &Page) -> HashMap<&str, &str> {
    page.shapes.iter().filter_map(|s| s.parent.as_deref().map(|p| (s.id.as_str(), p))).collect()
}

/// Ancestors of `id`, nearest first, at most [`MAX_GROUP_DEPTH`] and never repeating.
pub fn ancestors(page: &Page, id: &str) -> Vec<String> {
    ancestors_in(&parents(page), id).into_iter().map(str::to_string).collect()
}

fn ancestors_in<'a>(parent: &HashMap<&'a str, &'a str>, id: &str) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    let mut at = parent.get(id).copied();
    while let Some(p) = at {
        if out.len() >= MAX_GROUP_DEPTH || out.contains(&p) || p == id {
            break;
        }
        out.push(p);
        at = parent.get(p).copied();
    }
    out
}

/// The group ancestors of `id` that enclose it directly, nearest first: the walk stops at the
/// first parent that is not a group (a container keeps its members individually selectable).
pub fn group_chain(page: &Page, id: &str) -> Vec<String> {
    ancestors(page, id).into_iter().take_while(|a| page.shape(a).is_some_and(is_group)).collect()
}

/// Ids of the shapes directly inside `id`, in z order.
pub fn children(page: &Page, id: &str) -> Vec<String> {
    page.shapes.iter().filter(|s| s.parent.as_deref() == Some(id)).map(|s| s.id.clone()).collect()
}

/// Ids of every shape inside `id` at any depth, in z order.
pub fn descendants(page: &Page, id: &str) -> Vec<String> {
    let roots: HashSet<&str> = std::iter::once(id).collect();
    under(page, &roots)
}

/// Shapes (in z order) with an ancestor in `roots`.
fn under(page: &Page, roots: &HashSet<&str>) -> Vec<String> {
    let parent = parents(page);
    page.shapes.iter().filter(|s| ancestors_in(&parent, &s.id).iter().any(|a| roots.contains(a))).map(|s| s.id.clone()).collect()
}

/// `ids` followed by every shape inside any of them, each once: what moving, deleting or copying
/// `ids` must carry along.
pub fn with_descendants(page: &Page, ids: &[String]) -> Vec<String> {
    let roots: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for id in ids.iter().cloned().chain(under(page, &roots)) {
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

/// The shapes among `ids` that are not inside another of them, in the given order, each once.
/// Acting on these (with their descendants) moves every shape exactly once.
pub fn top_level(page: &Page, ids: &[String]) -> Vec<String> {
    let parent = parents(page);
    let set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| page.shape(id).is_some() && !ancestors_in(&parent, id).iter().any(|a| set.contains(a)) && seen.insert(id.as_str()))
        .cloned()
        .collect()
}

/// Union of the bounds of the shapes named by `ids` (rotation included).
pub fn bounds_of(page: &Page, ids: &[String]) -> Option<Rect> {
    ids.iter()
        .filter_map(|id| page.shape(id))
        .map(Shape::bounds)
        .filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()))
        .reduce(|a, b| a.union(&b))
}

/// The frame `(x, y, w, h)` a group with rotation `angle` needs to hold the shapes `members`:
/// their outlines measured in the group's own turned frame. `None` without members.
pub fn group_frame(page: &Page, members: &[String], angle: f64) -> Option<(f64, f64, f64, f64)> {
    let unturn = Transform::rotate(-angle);
    let r = members
        .iter()
        .filter_map(|id| page.shape(id))
        .filter_map(|s| s.outline_world().transformed(&unturn).bounds())
        .filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()))
        .reduce(|a, b| a.union(&b))?;
    let c = Transform::rotate(angle).apply(r.center());
    let (w, h) = (r.width().min(MAX_EXTENT), r.height().min(MAX_EXTENT));
    Some((coord(c.x - w / 2.0), coord(c.y - h / 2.0), w, h))
}

/// Set every group's frame to the bounds of its members, innermost groups first, then bring
/// containers in step ([`crate::container::refit`]). Groups with no members keep their frame
/// (see [`tidy_groups`]).
pub fn refit_groups(page: &mut Page) {
    let parent = parents(page);
    let mut groups: Vec<(usize, String)> =
        page.shapes.iter().filter(|s| is_group(s)).map(|s| (ancestors_in(&parent, &s.id).len(), s.id.clone())).collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.0));
    for (_, g) in groups {
        let members = children(page, &g);
        let angle = page.shape(&g).map_or(0.0, |s| s.angle_deg);
        if let Some((x, y, w, h)) = group_frame(page, &members, angle)
            && let Some(s) = page.shape_mut(&g)
        {
            (s.x, s.y, s.w, s.h) = (x, y, w, h);
            s.sanitize();
        }
    }
    crate::container::refit(page);
}

/// Remove groups left with no members (repeatedly, so a group holding only empty groups goes
/// too), then refit the rest. Returns the removed ids.
pub fn tidy_groups(page: &mut Page) -> Vec<String> {
    let mut removed = Vec::new();
    for _ in 0..=MAX_GROUP_DEPTH {
        let used: HashSet<String> = page.shapes.iter().filter_map(|s| s.parent.clone()).collect();
        let empty: Vec<String> = page.shapes.iter().filter(|s| is_group(s) && !used.contains(&s.id)).map(|s| s.id.clone()).collect();
        if empty.is_empty() {
            break;
        }
        for id in empty {
            page.detach_shape(&id);
            if page.remove_shape(&id).is_some() {
                removed.push(id);
            }
        }
    }
    refit_groups(page);
    removed
}

/// Move the shapes `ids` and everything inside them by `dx, dy`, then refit groups. Returns the
/// ids moved.
pub fn translate(page: &mut Page, ids: &[String], dx: f64, dy: f64) -> Vec<String> {
    let all = with_descendants(page, ids);
    let set: HashSet<&str> = all.iter().map(String::as_str).collect();
    for s in page.shapes.iter_mut().filter(|s| set.contains(s.id.as_str())) {
        s.x = coord(s.x + dx);
        s.y = coord(s.y + dy);
        s.sanitize();
    }
    refit_groups(page);
    all
}

/// Turn the shapes `ids` and everything inside them by `deg` about `pivot`: centres swing round
/// it and each angle grows by `deg`. Groups are refitted afterwards.
pub fn rotate_about(page: &mut Page, ids: &[String], pivot: Point, deg: f64) {
    if !deg.is_finite() || !pivot.is_finite() {
        return;
    }
    let all = with_descendants(page, ids);
    let set: HashSet<&str> = all.iter().map(String::as_str).collect();
    let t = Transform::rotate_about(pivot, deg);
    for s in page.shapes.iter_mut().filter(|s| set.contains(s.id.as_str())) {
        let c = t.apply(s.center());
        s.x = coord(c.x - s.w / 2.0);
        s.y = coord(c.y - s.h / 2.0);
        s.angle_deg += deg;
        s.sanitize();
    }
    refit_groups(page);
}

/// Turn the shapes by `deg`: one shape (or group) about its own centre, several about the centre
/// of their combined bounds, as one piece.
pub fn rotate_by(page: &mut Page, ids: &[String], deg: f64) {
    let top = top_level(page, ids);
    let pivot = match top.as_slice() {
        [one] => page.shape(one).map(Shape::center),
        _ => bounds_of(page, &top).map(|r| r.center()),
    };
    if let Some(p) = pivot {
        rotate_about(page, &top, p, deg);
    }
}

/// Set one shape's absolute angle; a group turns its members with it.
pub fn set_angle(page: &mut Page, id: &str, angle: f64) {
    let Some(s) = page.shape(id) else { return };
    if !angle.is_finite() {
        return;
    }
    let (pivot, delta) = (s.center(), angle - s.angle_deg);
    rotate_about(page, &[id.to_string()], pivot, delta);
}

/// Set one shape's frame (top-left `x, y`, size `w, h`, its angle unchanged). A group scales its
/// members from the old frame to the new one in its own turned frame; members turned a quarter
/// relative to the group swap the scale factors.
pub fn set_frame(page: &mut Page, id: &str, x: f64, y: f64, w: f64, h: f64) {
    let Some(old) = page.shape(id).cloned() else { return };
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return;
    }
    let (w, h) = (w.clamp(0.0, MAX_EXTENT), h.clamp(0.0, MAX_EXTENT));
    let mut new = old.clone();
    (new.x, new.y, new.w, new.h) = (coord(x), coord(y), w, h);
    if is_group(&old) {
        let sx = if old.w > 1e-9 { w / old.w } else { 1.0 };
        let sy = if old.h > 1e-9 { h / old.h } else { 1.0 };
        let (from, to) = (old.transform().inverse(), new.transform());
        let inside: HashSet<String> = descendants(page, id).into_iter().collect();
        if let Some(from) = from {
            for d in page.shapes.iter_mut().filter(|s| inside.contains(&s.id)) {
                let lc = from.apply(d.center());
                let c = to.apply(Point::new(lc.x * sx, lc.y * sy));
                let rel = (d.angle_deg - old.angle_deg).rem_euclid(180.0);
                let quarter = (rel - 90.0).abs() < 45.0;
                let (fx, fy) = if quarter { (sy, sx) } else { (sx, sy) };
                d.w = (d.w * fx).clamp(0.0, MAX_EXTENT);
                d.h = (d.h * fy).clamp(0.0, MAX_EXTENT);
                d.x = coord(c.x - d.w / 2.0);
                d.y = coord(c.y - d.h / 2.0);
                d.sanitize();
            }
        }
    }
    new.sanitize();
    if let Some(s) = page.shape_mut(id) {
        *s = new;
    }
    crate::container::after_set_frame(page, &old);
    refit_groups(page);
}

/// An edge or centre line to align to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Left edges.
    Left,
    /// Vertical centre lines.
    Center,
    /// Right edges.
    Right,
    /// Top edges.
    Top,
    /// Horizontal centre lines.
    Middle,
    /// Bottom edges.
    Bottom,
}

impl Edge {
    /// Parse `left`, `center` (or `centre`), `right`, `top`, `middle`, `bottom`.
    pub fn parse(s: &str) -> Option<Edge> {
        Some(match s {
            "left" => Edge::Left,
            "center" | "centre" => Edge::Center,
            "right" => Edge::Right,
            "top" => Edge::Top,
            "middle" => Edge::Middle,
            "bottom" => Edge::Bottom,
            _ => return None,
        })
    }
    /// Where this line is on `r`, and whether it is an x (true) or y position.
    fn of(self, r: &Rect) -> (f64, bool) {
        match self {
            Edge::Left => (r.x0, true),
            Edge::Center => ((r.x0 + r.x1) / 2.0, true),
            Edge::Right => (r.x1, true),
            Edge::Top => (r.y0, false),
            Edge::Middle => ((r.y0 + r.y1) / 2.0, false),
            Edge::Bottom => (r.y1, false),
        }
    }
}

/// Horizontal or vertical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

impl Axis {
    /// Parse `horizontal` or `vertical`.
    pub fn parse(s: &str) -> Option<Axis> {
        match s {
            "horizontal" => Some(Axis::Horizontal),
            "vertical" => Some(Axis::Vertical),
            _ => None,
        }
    }
}

/// Align the shapes `ids` (with their members) so `edge` of each lines up with `edge` of
/// `anchor`: the first shape's bounds, or the page. Returns the ids moved.
pub fn align(page: &mut Page, ids: &[String], edge: Edge, anchor: Rect) -> Vec<String> {
    let (target, horizontal) = edge.of(&anchor);
    let mut moved = Vec::new();
    for id in top_level(page, ids) {
        let Some(b) = page.shape(&id).map(Shape::bounds) else { continue };
        let d = target - edge.of(&b).0;
        if d.abs() > 1e-9 && d.is_finite() {
            let (dx, dy) = if horizontal { (d, 0.0) } else { (0.0, d) };
            moved.extend(translate(page, std::slice::from_ref(&id), dx, dy));
        }
    }
    moved
}

/// Space the shapes `ids` with equal gaps along `axis`, keeping the first and last (by centre)
/// in place. Fewer than three shapes are left alone. Returns the ids moved.
pub fn distribute(page: &mut Page, ids: &[String], axis: Axis) -> Vec<String> {
    let mut items: Vec<(String, Rect)> = top_level(page, ids).into_iter().filter_map(|id| page.shape(&id).map(|s| (id, s.bounds()))).collect();
    if items.len() < 3 {
        return Vec::new();
    }
    let span = |r: &Rect| if axis == Axis::Horizontal { (r.x0, r.x1) } else { (r.y0, r.y1) };
    items.sort_by(|a, b| {
        let (a0, a1) = span(&a.1);
        let (b0, b1) = span(&b.1);
        (a0 + a1).total_cmp(&(b0 + b1))
    });
    let (Some(first), Some(last)) = (items.first(), items.last()) else { return Vec::new() };
    let (start, end) = (span(&first.1).0, span(&last.1).1);
    let sizes: f64 = items.iter().map(|(_, r)| span(r).1 - span(r).0).sum();
    let n = items.len();
    let gap = (end - start - sizes) / (n - 1) as f64;
    let mut at = span(&first.1).1 + gap;
    let mut moved = Vec::new();
    for (id, r) in items.iter().skip(1).take(n - 2) {
        let (lo, hi) = span(r);
        let d = at - lo;
        at += hi - lo + gap;
        if d.abs() > 1e-9 && d.is_finite() {
            let (dx, dy) = if axis == Axis::Horizontal { (d, 0.0) } else { (0.0, d) };
            moved.extend(translate(page, std::slice::from_ref(id), dx, dy));
        }
    }
    moved
}

/// Even out the space between connected shapes among `ids`: along the direction the shapes
/// spread most, each shape connected to one placed before it starts `gap` past the far edge of
/// the furthest such shape. Unconnected shapes stay. Returns the ids moved.
pub fn auto_space(page: &mut Page, ids: &[String], gap: f64) -> Vec<String> {
    let gap = if gap.is_finite() { gap.clamp(0.0, MAX_EXTENT) } else { DEFAULT_GAP };
    let top = top_level(page, ids);
    let parent = parents(page);
    // Each glued connector end counts for the target shape that holds it (itself or a group).
    let owner = |sid: &str| -> Option<String> {
        if top.iter().any(|t| t == sid) {
            return Some(sid.to_string());
        }
        ancestors_in(&parent, sid).into_iter().find(|a| top.iter().any(|t| t == a)).map(str::to_string)
    };
    let mut links: HashSet<(String, String)> = HashSet::new();
    for c in &page.connectors {
        if let (Some(a), Some(b)) = (c.start.shape().and_then(owner), c.end.shape().and_then(owner))
            && a != b
        {
            links.insert((a.clone(), b.clone()));
            links.insert((b, a));
        }
    }
    let centres: Vec<Point> = top.iter().filter_map(|id| page.shape(id)).map(|s| s.bounds().center()).collect();
    let spread = |f: fn(&Point) -> f64| {
        let v: Vec<f64> = centres.iter().map(f).collect();
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max) - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let horizontal = spread(|p| p.x) >= spread(|p| p.y);
    let mut order: Vec<(String, f64)> = top
        .iter()
        .filter_map(|id| page.shape(id).map(|s| (id.clone(), if horizontal { s.bounds().center().x } else { s.bounds().center().y })))
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut moved = Vec::new();
    for k in 1..order.len() {
        let Some((id, _)) = order.get(k).cloned() else { continue };
        let far = order
            .iter()
            .take(k)
            .filter(|(p, _)| links.contains(&(p.clone(), id.clone())))
            .filter_map(|(p, _)| page.shape(p).map(|s| if horizontal { s.bounds().x1 } else { s.bounds().y1 }))
            .reduce(f64::max);
        let (Some(far), Some(b)) = (far, page.shape(&id).map(Shape::bounds)) else { continue };
        let d = far + gap - if horizontal { b.x0 } else { b.y0 };
        if d.abs() > 1e-9 && d.is_finite() {
            let (dx, dy) = if horizontal { (d, 0.0) } else { (0.0, d) };
            moved.extend(translate(page, std::slice::from_ref(&id), dx, dy));
        }
    }
    moved
}

/// A z-order change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// To the top of the page.
    Front,
    /// To the bottom of the page.
    Back,
    /// One item up.
    Forward,
    /// One item down.
    Backward,
}

impl Order {
    /// Parse `front`, `back`, `forward`, `backward`.
    pub fn parse(s: &str) -> Option<Order> {
        Some(match s {
            "front" => Order::Front,
            "back" => Order::Back,
            "forward" => Order::Forward,
            "backward" => Order::Backward,
            _ => return None,
        })
    }
}

/// Restack the shapes `ids`. Each moves as a block with everything inside it (a group keeps its
/// members just above it). Forward and backward step over one neighbouring block: the next (or
/// previous) shape together with its own members, never out of a group the block is in.
pub fn order(page: &mut Page, ids: &[String], how: Order) {
    let mut top = top_level(page, ids);
    let index: HashMap<&str, usize> = page.shapes.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    top.sort_by_key(|id| index.get(id.as_str()).copied().unwrap_or(0));
    drop(index);
    match how {
        Order::Front | Order::Back => {
            let all: HashSet<String> = with_descendants(page, &top).into_iter().collect();
            let (block, rest): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut page.shapes).into_iter().partition(|s| all.contains(&s.id));
            page.shapes = if how == Order::Front { rest.into_iter().chain(block).collect() } else { block.into_iter().chain(rest).collect() };
        }
        Order::Forward => {
            for id in top.iter().rev() {
                step(page, id, true);
            }
        }
        Order::Backward => {
            for id in &top {
                step(page, id, false);
            }
        }
    }
}

/// Move `id`'s block one neighbouring block up (`up`) or down.
fn step(page: &mut Page, id: &str, up: bool) {
    let mine: HashSet<String> = with_descendants(page, &[id.to_string()]).into_iter().collect();
    let Some(first) = page.shapes.iter().position(|s| mine.contains(&s.id)) else { return };
    let parent = parents(page);
    let own_anc: HashSet<String> = ancestors_in(&parent, id).into_iter().map(str::to_string).collect();
    let own_parent = parent.get(id).map(|p| p.to_string());
    drop(parent);
    let (block, rest): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut page.shapes).into_iter().partition(|s| mine.contains(&s.id));
    // Where the block's first shape was, counted in the shapes that stay.
    let at = first.min(rest.len());
    page.shapes = rest;
    let neighbour = if up { page.shapes.get(at) } else { at.checked_sub(1).and_then(|i| page.shapes.get(i)) };
    let insert = match neighbour.map(|s| s.id.clone()) {
        None => at,
        // A member never steps out of its own group: past the group's last member it stays.
        Some(n) if own_parent.as_ref().is_some_and(|p| !ancestors(page, &n).contains(p)) => at,
        Some(n) => {
            // The neighbour's outermost ancestor that is not also around the block, so stepping
            // past a group steps past all of it, and a member stays inside its own group.
            let parent = parents(page);
            let mut outer = n.clone();
            for a in ancestors_in(&parent, &n) {
                if own_anc.contains(a) {
                    break;
                }
                outer = a.to_string();
            }
            drop(parent);
            let span: HashSet<String> = with_descendants(page, &[outer]).into_iter().collect();
            let positions = page.shapes.iter().enumerate().filter(|(_, s)| span.contains(&s.id)).map(|(i, _)| i);
            if up { positions.max().map_or(at, |i| i + 1) } else { positions.min().unwrap_or(at) }
        }
    };
    let insert = insert.min(page.shapes.len());
    let tail = page.shapes.split_off(insert);
    page.shapes.extend(block);
    page.shapes.extend(tail);
}

/// Mirror the shapes `ids` (with their members) across the centre line of their combined bounds:
/// `Horizontal` swaps left and right, `Vertical` top and bottom. Each centre is mirrored and each
/// angle negated, so flipping twice gives back the original.
pub fn flip(page: &mut Page, ids: &[String], axis: Axis) -> Vec<String> {
    let top = top_level(page, ids);
    let Some(b) = bounds_of(page, &top) else { return Vec::new() };
    let c = b.center();
    let all = with_descendants(page, &top);
    let set: HashSet<&str> = all.iter().map(String::as_str).collect();
    for s in page.shapes.iter_mut().filter(|s| set.contains(s.id.as_str())) {
        let m = s.center();
        let m = if axis == Axis::Horizontal { Point::new(2.0 * c.x - m.x, m.y) } else { Point::new(m.x, 2.0 * c.y - m.y) };
        s.x = coord(m.x - s.w / 2.0);
        s.y = coord(m.y - s.h / 2.0);
        s.angle_deg = -s.angle_deg;
        s.sanitize();
    }
    refit_groups(page);
    all
}

/// Why [`group`] refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupError {
    /// None of the ids named a shape on the page.
    NoShapes,
    /// The shapes are in different groups or containers.
    MixedParents,
    /// The new id is already used on the page.
    IdTaken,
}

impl std::fmt::Display for GroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            GroupError::NoShapes => "select one or more shapes to group",
            GroupError::MixedParents => "the shapes are in different groups; ungroup them first",
            GroupError::IdTaken => "the group id is already used",
        })
    }
}

/// Group the shapes `ids` under a new group shape `group_id`. The members (each with its own
/// members) are stacked together at the place of the topmost one, just above the new group; the
/// group takes the members' shared parent. Returns the members.
pub fn group(page: &mut Page, ids: &[String], group_id: &str) -> Result<Vec<String>, GroupError> {
    if page.contains_id(group_id) {
        return Err(GroupError::IdTaken);
    }
    let members = top_level(page, ids);
    let first = members.first().ok_or(GroupError::NoShapes)?;
    let shared = page.shape(first).and_then(|s| s.parent.clone());
    if members.iter().any(|m| page.shape(m).and_then(|s| s.parent.clone()) != shared) {
        return Err(GroupError::MixedParents);
    }
    let all: HashSet<String> = with_descendants(page, &members).into_iter().collect();
    let last = page.shapes.iter().rposition(|s| all.contains(&s.id)).unwrap_or(0);
    let before = page.shapes.iter().take(last).filter(|s| !all.contains(&s.id)).count();
    let (mut block, mut rest): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut page.shapes).into_iter().partition(|s| all.contains(&s.id));
    for s in block.iter_mut().filter(|s| members.contains(&s.id)) {
        s.parent = Some(group_id.to_string());
    }
    let mut g = new_group(group_id);
    g.parent = shared;
    let tail = rest.split_off(before.min(rest.len()));
    page.shapes = rest;
    page.shapes.push(g);
    page.shapes.append(&mut block);
    page.shapes.extend(tail);
    refit_groups(page);
    Ok(members)
}

/// Dissolve the groups among `ids`: their members move up to the group's own parent and the group
/// shapes go (connectors glued to a group keep a free end where it was). Ids that are not groups
/// are skipped. Returns the former members, in z order.
pub fn ungroup(page: &mut Page, ids: &[String]) -> Vec<String> {
    let groups: Vec<String> = ids.iter().filter(|id| page.shape(id).is_some_and(is_group)).cloned().collect();
    let mut freed = Vec::new();
    for g in &groups {
        let up = page.shape(g).and_then(|s| s.parent.clone());
        for s in page.shapes.iter_mut().filter(|s| s.parent.as_deref() == Some(g.as_str())) {
            s.parent = up.clone();
            freed.push(s.id.clone());
        }
        page.detach_shape(g);
        page.remove_shape(g);
    }
    freed.retain(|id| page.shape(id).is_some());
    refit_groups(page);
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape::new(id, ShapeKind::Rectangle, x, y, w, h)
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn page() -> Page {
        let mut p = Page::new("p1", "Page 1");
        p.shapes.push(rect("a", 0.0, 0.0, 10.0, 10.0));
        p.shapes.push(rect("b", 100.0, 50.0, 20.0, 20.0));
        p.shapes.push(rect("c", 300.0, 0.0, 10.0, 30.0));
        p
    }

    fn frame(p: &Page, id: &str) -> (f64, f64, f64, f64) {
        let s = p.shape(id).unwrap();
        (s.x, s.y, s.w, s.h)
    }

    #[test]
    fn group_fits_members_and_moves_resizes_and_rotates_them() {
        let mut p = page();
        group(&mut p, &ids(&["a", "b"]), "g").unwrap();
        assert_eq!(p.shapes.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["g", "a", "b", "c"]);
        assert_eq!(frame(&p, "g"), (0.0, 0.0, 120.0, 70.0));
        translate(&mut p, &ids(&["g"]), 5.0, 5.0);
        assert_eq!(frame(&p, "a"), (5.0, 5.0, 10.0, 10.0));
        assert_eq!(frame(&p, "g"), (5.0, 5.0, 120.0, 70.0));
        set_frame(&mut p, "g", 5.0, 5.0, 240.0, 70.0);
        assert_eq!(frame(&p, "b"), (205.0, 55.0, 40.0, 20.0));
        assert_eq!(frame(&p, "g"), (5.0, 5.0, 240.0, 70.0));
        set_angle(&mut p, "g", 90.0);
        assert_eq!(p.shape("a").unwrap().angle_deg, 90.0);
        let g = p.shape("g").unwrap();
        assert!((g.angle_deg - 90.0).abs() < 1e-9 && (g.w - 240.0).abs() < 1e-6, "{g:?}");
    }

    #[test]
    fn nested_groups_move_together_and_ungroup_one_level() {
        let mut p = page();
        group(&mut p, &ids(&["a", "b"]), "g1").unwrap();
        group(&mut p, &ids(&["g1", "c"]), "g2").unwrap();
        assert_eq!(p.shape("g1").unwrap().parent.as_deref(), Some("g2"));
        assert_eq!(frame(&p, "g2"), (0.0, 0.0, 310.0, 70.0));
        translate(&mut p, &ids(&["g2"]), 0.0, 100.0);
        assert_eq!(frame(&p, "a").1, 100.0);
        assert_eq!(frame(&p, "g1").1, 100.0);
        let freed = ungroup(&mut p, &ids(&["g2"]));
        assert_eq!(freed, ids(&["g1", "c"]));
        assert!(p.shape("g2").is_none());
        assert_eq!(p.shape("g1").unwrap().parent, None);
        assert_eq!(p.shape("a").unwrap().parent.as_deref(), Some("g1"));
        assert_eq!(top_level(&p, &ids(&["a", "g1", "zzz"])), ids(&["g1"]));
    }

    #[test]
    fn align_distribute_and_auto_space() {
        let mut p = page();
        let first = p.shape("a").unwrap().bounds();
        align(&mut p, &ids(&["a", "b", "c"]), Edge::Bottom, first);
        assert!(p.shapes.iter().all(|s| (s.bounds().y1 - 10.0).abs() < 1e-9));
        let r = p.rect();
        align(&mut p, &ids(&["a", "b", "c"]), Edge::Center, r);
        assert!(p.shapes.iter().all(|s| (s.bounds().center().x - 550.0).abs() < 1e-9));

        let mut p = page();
        distribute(&mut p, &ids(&["a", "b", "c"]), Axis::Horizontal);
        // Span 0..310, sizes 40: gaps of 135.
        assert_eq!(frame(&p, "b").0, 145.0);
        assert!(distribute(&mut p, &ids(&["a", "b"]), Axis::Vertical).is_empty());

        let mut p = page();
        p.connectors.push(crate::Connector::new("c1", crate::End::dynamic("a"), crate::End::dynamic("b")));
        p.connectors.push(crate::Connector::new("c2", crate::End::dynamic("b"), crate::End::dynamic("c")));
        auto_space(&mut p, &ids(&["a", "b", "c"]), 25.0);
        assert_eq!(frame(&p, "b").0, 35.0);
        assert_eq!(frame(&p, "c").0, 80.0);
    }

    #[test]
    fn order_moves_blocks_and_keeps_members_in_their_group() {
        let mut p = page();
        p.shapes.push(rect("d", 0.0, 0.0, 5.0, 5.0));
        group(&mut p, &ids(&["a", "b"]), "g").unwrap();
        let z = |p: &Page| p.shapes.iter().map(|s| s.id.clone()).collect::<Vec<_>>().join(",");
        assert_eq!(z(&p), "g,a,b,c,d");
        order(&mut p, &ids(&["g"]), Order::Front);
        assert_eq!(z(&p), "c,d,g,a,b");
        order(&mut p, &ids(&["g"]), Order::Backward);
        assert_eq!(z(&p), "c,g,a,b,d");
        order(&mut p, &ids(&["c"]), Order::Forward);
        assert_eq!(z(&p), "g,a,b,c,d");
        order(&mut p, &ids(&["a"]), Order::Forward);
        assert_eq!(z(&p), "g,b,a,c,d");
        // A member at the top of its group stays inside it.
        order(&mut p, &ids(&["a"]), Order::Forward);
        assert_eq!(z(&p), "g,b,a,c,d");
        order(&mut p, &ids(&["d"]), Order::Back);
        assert_eq!(z(&p), "d,g,b,a,c");
    }

    #[test]
    fn flip_mirrors_and_twice_is_identity() {
        let mut p = page();
        p.shape_mut("a").unwrap().angle_deg = 30.0;
        let before = p.clone();
        let mid = bounds_of(&p, &ids(&["a", "c"])).unwrap().center().x;
        let ca = p.shape("a").unwrap().center().x;
        flip(&mut p, &ids(&["a", "c"]), Axis::Horizontal);
        assert!((p.shape("a").unwrap().center().x - (2.0 * mid - ca)).abs() < 1e-9);
        assert_eq!(p.shape("a").unwrap().angle_deg, 330.0);
        flip(&mut p, &ids(&["a", "c"]), Axis::Horizontal);
        for (a, b) in p.shapes.iter().zip(&before.shapes) {
            assert!((a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.angle_deg - b.angle_deg).abs() < 1e-9, "{} moved", a.id);
        }
    }

    #[test]
    fn group_refuses_mixed_parents_and_taken_ids_and_tidy_drops_empty_groups() {
        let mut p = page();
        group(&mut p, &ids(&["a"]), "g").unwrap();
        assert_eq!(group(&mut p, &ids(&["a", "c"]), "h"), Err(GroupError::MixedParents));
        assert_eq!(group(&mut p, &ids(&["c"]), "a"), Err(GroupError::IdTaken));
        assert_eq!(group(&mut p, &ids(&["nope"]), "h"), Err(GroupError::NoShapes));
        p.remove_shape("a");
        assert_eq!(tidy_groups(&mut p), ids(&["g"]));
    }

    #[test]
    fn groups_are_not_routing_obstacles() {
        let mut p = page();
        p.shapes.push(rect("m", 150.0, 0.0, 40.0, 100.0));
        let mut c = crate::Connector::new("c1", crate::End::dynamic("a"), crate::End::dynamic("c"));
        (c.avoid, c.route) = (true, crate::Route::Orthogonal);
        p.connectors.push(c);
        crate::autoroute::reroute(&mut p, None);
        let before = crate::route::route_polyline(&p, &p.connectors[0]);
        group(&mut p, &ids(&["a", "c", "m"]), "g").unwrap();
        crate::autoroute::reroute(&mut p, None);
        assert_eq!(crate::route::route_polyline(&p, &p.connectors[0]), before);
        assert!(crate::autoroute::clear_of_shapes(&p, &p.connectors[0]));
    }

    #[test]
    fn hostile_numbers_never_escape_the_coordinate_range() {
        let mut p = page();
        group(&mut p, &ids(&["a", "b"]), "g").unwrap();
        translate(&mut p, &ids(&["g"]), f64::MAX, f64::NAN);
        set_frame(&mut p, "g", f64::INFINITY, 0.0, 1e300, -5.0);
        set_frame(&mut p, "g", 0.0, 0.0, 1e300, 0.0);
        set_angle(&mut p, "g", f64::NAN);
        rotate_by(&mut p, &ids(&["g", "c"]), 1e300);
        auto_space(&mut p, &ids(&["a", "c"]), f64::NAN);
        for s in &p.shapes {
            assert!([s.x, s.y, s.w, s.h, s.angle_deg].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD), "{s:?}");
        }
    }
}
