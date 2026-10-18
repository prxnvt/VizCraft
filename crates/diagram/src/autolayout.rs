//! Automatic placement of shapes, built on `vizcraft-layout`.
//!
//! The page's shapes become layout nodes (sized by their bounding boxes) and the connectors glued
//! at both ends to two of them become layout edges. A whole page or selection is laid out in
//! units: groups, containers and lane sets move as one node with everything inside them, and
//! callouts follow their targets. The `layout.*` commands and the Mermaid and
//! CSV importers all place shapes through [`layout_items`] (rearrange everything) or
//! [`layout_incremental`] (place new shapes around ones that stay put), so they agree on what a
//! layout does.

use std::collections::{HashMap, HashSet, VecDeque};

pub use vizcraft_layout::{Algorithm, Direction, Options};

use crate::MAX_EXTENT;
use crate::geom::{Point, Rect};
use crate::model::{End, Page, Shape};
use crate::{arrange, container, layer};

/// Where a whole-page layout puts its top-left corner, so the drawing does not touch the page edge.
pub const PAGE_MARGIN: f64 = 40.0;

/// Most pushes one new shape gets while clearing overlaps before it jumps past all content, so a
/// crowded page cannot make placement quadratic in the push count.
const MAX_PUSHES: usize = 64;

/// Grow the page so every shape fits with [`PAGE_MARGIN`] to spare on the right and bottom. The
/// page never shrinks, and never grows past [`MAX_EXTENT`]. An infinite board has no sheet to grow.
pub fn grow_page(page: &mut Page) {
    if page.board != crate::board::Board::Fixed {
        return;
    }
    let Some(b) =
        page.shapes.iter().map(|s| s.bounds()).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
    else {
        return;
    };
    let (w, h) = ((b.x1 + PAGE_MARGIN).min(MAX_EXTENT), (b.y1 + PAGE_MARGIN).min(MAX_EXTENT));
    let sheet = page.rect();
    if w > sheet.width() {
        page.width = w;
    }
    if h > sheet.height() {
        page.height = h;
    }
}

/// The layout node for a unit with bounding box `b`.
fn node(b: &Rect) -> vizcraft_layout::Node {
    vizcraft_layout::Node { width: (b.x1 - b.x0).clamp(1.0, 1.0e5), height: (b.y1 - b.y0).clamp(1.0, 1.0e5) }
}

/// The top-left corner of `b`.
fn corner(b: &Rect) -> Point {
    Point::new(b.x0, b.y0)
}

/// The shape a layout moves in order to move `id`, or `None` for a callout, which follows its
/// target instead of being laid out. A group member is moved by its outermost group and a lane by
/// its lane set (lanes stack inside their set), so neither is ever pulled apart. Members of an
/// ordinary container stay themselves: they can be laid out on their own, inside the container.
fn unit_root(page: &Page, id: &str) -> Option<String> {
    let mut at = id.to_string();
    for _ in 0..=arrange::MAX_GROUP_DEPTH {
        if let Some(group) = arrange::group_chain(page, &at).pop() {
            at = group;
        } else if let Some(set) = container::set_of(page, &at) {
            at = set;
        } else {
            break;
        }
    }
    page.shape(&at).filter(|s| !container::is_callout(s)).map(|s| s.id.clone())
}

/// One node of a layout: a shape and everything inside it (a group with its members, a container
/// with its members, a lane set with its lanes and their members), moved together.
struct Unit {
    /// The shapes moved together, the unit's own shape first.
    ids: Vec<String>,
    /// Their bounding box before the layout.
    bounds: Rect,
}

/// The unit headed by `root` (it and everything inside it), with whether it must stay put because
/// it holds a shape on a locked layer or with its position locked. `None` without finite bounds.
fn unit_at(page: &Page, root: &str) -> Option<(Unit, bool)> {
    let ids = arrange::with_descendants(page, &[root.to_string()]);
    let pinned = ids.iter().filter_map(|id| page.shape(id)).any(|m| m.locks.position || layer::locked(page, m));
    let bounds = arrange::bounds_of(page, &ids)?;
    Some((Unit { ids, bounds }, pinned))
}

/// The heads of the units a layout of `ids` (every shape when `None`) moves, in page order: the
/// shapes lifted by [`unit_root`], outermost only.
fn unit_roots(page: &Page, ids: Option<&[String]>) -> Vec<String> {
    let asked: Vec<String> = match ids {
        Some(ids) => ids.to_vec(),
        None => page.shapes.iter().map(|s| s.id.clone()).collect(),
    };
    let mut roots: Vec<String> = Vec::new();
    for id in &asked {
        if let Some(r) = unit_root(page, id)
            && !roots.contains(&r)
        {
            roots.push(r);
        }
    }
    let roots: HashSet<String> = arrange::top_level(page, &roots).into_iter().collect();
    page.shapes.iter().filter(|s| roots.contains(&s.id)).map(|s| s.id.clone()).collect()
}

/// The units a layout of `ids` (every shape when `None`) moves, in page order ([`unit_roots`],
/// each with its descendants). A unit holding a shape that must not move is left out, so it stays
/// put.
fn units(page: &Page, ids: Option<&[String]>) -> Vec<Unit> {
    unit_roots(page, ids).iter().filter_map(|r| unit_at(page, r)).filter_map(|(u, pinned)| (!pinned).then_some(u)).collect()
}

/// Lay out the shapes named by `ids` (or every shape on the page when `None`) with `opts`.
///
/// The layout works on units ([`units`]): a group, container or lane set is one node and moves
/// with everything inside it, so groups stay whole and lanes keep their members; a callout is not
/// laid out but moves with its target. Connectors between units count as edges between them.
/// A selection keeps its previous top-left corner; a whole page starts at [`PAGE_MARGIN`].
/// The moves finish as [`apply_moves`] says (callouts, glue, refits, page size). Returns the
/// layout's notes (for example a tree layout falling back to layered), or the reason the layout
/// was refused.
pub fn layout_items(page: &mut Page, ids: Option<&[String]>, opts: &Options) -> Result<Vec<String>, String> {
    let units = units(page, ids);
    if units.is_empty() {
        return Ok(Vec::new());
    }
    let mut unit_of: HashMap<&str, usize> = HashMap::new();
    let mut graph = vizcraft_layout::Graph::default();
    for (n, u) in units.iter().enumerate() {
        for id in &u.ids {
            unit_of.insert(id.as_str(), n);
        }
        graph.nodes.push(node(&u.bounds));
    }
    for c in &page.connectors {
        let (Some(a), Some(b)) = (c.start.shape().and_then(|s| unit_of.get(s)), c.end.shape().and_then(|s| unit_of.get(s))) else { continue };
        if a != b {
            graph.edges.push(vizcraft_layout::Edge::new(*a, *b));
        }
    }
    let origin = if ids.is_some() { units.iter().map(|u| corner(&u.bounds)).reduce(|p, q| Point::new(p.x.min(q.x), p.y.min(q.y))) } else { None }
        .unwrap_or(Point::new(PAGE_MARGIN, PAGE_MARGIN));
    let out = vizcraft_layout::layout(&graph, opts).map_err(|e| e.to_string())?;
    let mut delta: HashMap<String, Point> = HashMap::new();
    for (u, pos) in units.iter().zip(&out.nodes) {
        let d = Point::new(origin.x + pos.x - u.bounds.x0, origin.y + pos.y - u.bounds.y0);
        for id in &u.ids {
            delta.insert(id.clone(), d);
        }
    }
    apply_moves(page, delta);
    Ok(out.notes)
}

/// Move every shape named in `delta` by its offset and tidy up after a layout. Callouts left out
/// (not locked) follow their target and then settle on a free side of it. Connectors between
/// shapes that moved apart lose their stored bends and fixed connection points (dynamic glue picks
/// the nearest side for the new arrangement); connectors whose ends moved together keep both,
/// moved along. Groups and containers refit, and the page grows to fit the result.
fn apply_moves(page: &mut Page, mut delta: HashMap<String, Point>) {
    let mut followers: Vec<(String, String)> = Vec::new();
    for s in &page.shapes {
        if container::is_callout(s)
            && !delta.contains_key(&s.id)
            && !s.locks.position
            && !layer::locked(page, s)
            && let Some(t) = s.container.as_ref().and_then(|c| c.target.as_deref())
            && let Some(d) = delta.get(t).copied()
        {
            delta.insert(s.id.clone(), d);
            followers.push((s.id.clone(), t.to_string()));
        }
    }
    for s in &mut page.shapes {
        if let Some(d) = delta.get(&s.id) {
            s.x += d.x;
            s.y += d.y;
            s.sanitize();
        }
    }
    let moved = |e: &End| e.shape().and_then(|s| delta.get(s)).copied().unwrap_or(Point::ZERO);
    for c in &mut page.connectors {
        let (ds, de) = (moved(&c.start), moved(&c.end));
        if ds == de {
            // Both ends moved together (or neither did): the drawing is the same, only shifted.
            for w in &mut c.waypoints {
                *w = Point::new(w.x + ds.x, w.y + ds.y);
            }
        } else {
            c.waypoints.clear();
            for e in [&mut c.start, &mut c.end] {
                if let End::Glued { point, .. } = e {
                    *point = None;
                }
            }
        }
    }
    // Groups refit and lane sets restack (a no-op for whole units, needed when some members of a
    // group or container moved on their own); auto-fit containers grow to hold new members.
    arrange::refit_groups(page);
    for (callout, target) in &followers {
        settle_callout(page, callout, target);
    }
    grow_page(page);
}

/// Put callout `id`, already moved along with its target, where it covers no other shape: its old
/// offset from the target first, then that offset mirrored left to right, top to bottom and both.
/// A free spot on the page (not left of or above its origin) wins over one off it; when every
/// spot covers something the callout keeps its old offset. The shapes that hold the target or
/// the callout (containers, lanes) and group frames do not count as covered.
fn settle_callout(page: &mut Page, id: &str, target: &str) {
    let (Some(c), Some(t)) = (page.shape(id), page.shape(target)) else { return };
    let (cb, tb) = (c.bounds(), t.bounds());
    let mut holders: HashSet<String> = arrange::ancestors(page, target).into_iter().collect();
    holders.extend(arrange::ancestors(page, id));
    let others: Vec<Rect> = page
        .shapes
        .iter()
        .filter(|s| s.id != id && !holders.contains(&s.id) && !arrange::is_group(s) && page.shape_visible(s))
        .map(|s| s.bounds())
        .collect();
    let (tc, cc) = (tb.center(), cb.center());
    let (ox, oy) = (cc.x - tc.x, cc.y - tc.y);
    let spots: Vec<Point> = [(ox, oy), (-ox, oy), (ox, -oy), (-ox, -oy)].iter().map(|&(x, y)| Point::new(tc.x + x - cc.x, tc.y + y - cc.y)).collect();
    let moved = |d: &Point| Rect::new(cb.x0 + d.x, cb.y0 + d.y, cb.x1 + d.x, cb.y1 + d.y);
    let free = |r: &Rect| !others.iter().any(|o| overlaps_strictly(r, o));
    let on_page = |r: &Rect| r.x0 >= 0.0 && r.y0 >= 0.0;
    let best = spots.iter().find(|d| free(&moved(d)) && on_page(&moved(d))).or_else(|| spots.iter().find(|d| free(&moved(d))));
    if let Some(d) = best.copied()
        && let Some(c) = page.shape_mut(id)
    {
        c.x += d.x;
        c.y += d.y;
        c.sanitize();
    }
}

/// Place the shapes named by `new_ids` and leave every other shape exactly where it is.
///
/// Naming a group, container or lane set names everything inside it, and a group whose members
/// are all new is new itself. Placement follows the unit rules of [`layout_items`]. On the page's
/// top level the nodes are
/// units: a new group, container or lane set is placed as one node with everything inside it, and
/// a unit that holds any shape that is not new, or one that must not move (position locked, on a
/// locked layer), stays put. New shapes inside a unit that stays (a new member of an existing
/// group, container or lane) are placed in the same way among the other shapes directly inside
/// it, anchored also on shapes outside it they connect to, and stay inside: in a lane or a
/// container that keeps its size they land within its content area (less its margin), so its
/// frame does not change; an auto-fit container keeps its top-left corner and grows to hold them.
/// Lanes are never placed (their set stacks them) and callouts are never placed: a callout
/// follows its target when the target moves.
///
/// Within each of those scopes the whole graph is laid out once to learn where each new unit
/// belongs relative to its neighbours. A new unit connected (directly or through other new units)
/// to one that stays is put at that neighbour's real position plus the layout's offset between
/// the two, so in a top-to-bottom layered layout a child lands below its parent, in left-to-right
/// to its right, and so on. New units with no path to one that stays keep their layout
/// arrangement and go to the right of the scope's existing content (or, with none, to the top-left
/// of the box's content area, or where they already are). Finally each new unit is pushed sideways
/// (across the flow) until it overlaps nothing. The moves finish as [`apply_moves`] says. With
/// every shape new this is [`layout_items`] on the whole page. Unknown ids are ignored. Returns
/// the layout's notes, or the reason the layout was refused (nothing is moved then).
pub fn layout_incremental(page: &mut Page, new_ids: &[String], opts: &Options) -> Result<Vec<String>, String> {
    // Naming a group, container or lane set names everything inside it, and a group whose members
    // are all new is new too (it is only their frame).
    let mut new: HashSet<String> = arrange::with_descendants(page, new_ids).into_iter().filter(|id| page.shape(id).is_some()).collect();
    for _ in 0..=arrange::MAX_GROUP_DEPTH {
        let frames: Vec<String> = page
            .shapes
            .iter()
            .filter(|s| arrange::is_group(s) && !new.contains(&s.id))
            .filter(|s| {
                let members = arrange::children(page, &s.id);
                !members.is_empty() && members.iter().all(|m| new.contains(m))
            })
            .map(|s| s.id.clone())
            .collect();
        if frames.is_empty() {
            break;
        }
        new.extend(frames);
    }
    if new.is_empty() {
        return Ok(Vec::new());
    }
    if page.shapes.iter().all(|s| new.contains(s.id.as_str())) {
        return layout_items(page, None, opts);
    }
    let mut delta: HashMap<String, Point> = HashMap::new();
    let mut notes: Vec<String> = Vec::new();
    // Scopes still to place: the top level first, then each box or group that stays but gained
    // new shapes, at most MAX_GROUP_DEPTH deep.
    let mut scopes: Vec<(Option<String>, usize)> = vec![(None, 0)];
    while let Some((scope, depth)) = scopes.pop() {
        let roots: Vec<String> = match scope.as_deref() {
            None => unit_roots(page, None),
            Some(p) => arrange::children(page, p).into_iter().filter(|id| page.shape(id).is_some_and(|s| !container::is_callout(s))).collect(),
        };
        let mut slots: Vec<(Unit, bool)> = Vec::new();
        for root in roots {
            let Some((unit, pinned)) = unit_at(page, &root) else { continue };
            let fresh = unit.ids.iter().filter(|id| new.contains(id.as_str())).count();
            let placed = !pinned && fresh == unit.ids.len();
            if !placed
                && fresh > 0
                && depth < arrange::MAX_GROUP_DEPTH
                && page.shape(&root).is_some_and(|s| arrange::is_group(s) || container::is_container(s))
            {
                scopes.push((Some(root.clone()), depth + 1));
            }
            slots.push((unit, placed));
        }
        // A lane set stacks its lanes itself; only their members are placed.
        let in_set = scope.as_deref().and_then(|p| page.shape(p)).is_some_and(|s| container::role(s) == Some(container::Role::LaneSet));
        if in_set || !slots.iter().any(|(_, placed)| *placed) {
            continue;
        }
        for n in place_scope(page, scope.as_deref(), &slots, &new, opts, &mut delta)? {
            if !notes.contains(&n) {
                notes.push(n);
            }
        }
    }
    // Nothing to place (only callouts or shapes that must stay were named): the page is untouched.
    if !delta.is_empty() {
        apply_moves(page, delta);
    }
    Ok(notes)
}

/// Where new members of box `s` must land, or `None` for no limit (a group refits around whatever
/// it holds). A lane or a container that keeps its size takes them inside its content area, less
/// its margin where that leaves room; an auto-fit container takes them right of and below that
/// area's top-left corner, so its frame grows from there.
fn room_in(s: &Shape) -> Option<Rect> {
    let c = s.container.as_ref().filter(|c| c.holds_members())?;
    let r = container::content_rect(s);
    let m = c.margin;
    let inner = if r.width() > 2.0 * m && r.height() > 2.0 * m { r.inflate(-m) } else { r };
    Some(if c.role == container::Role::Container && c.auto_fit { Rect::new(inner.x0, inner.y0, f64::INFINITY, f64::INFINITY) } else { inner })
}

/// Work out where the new units of one scope go (see [`layout_incremental`]) and record their
/// offsets in `delta`. `slots` are the scope's units, each with whether it is placed; `scope` is
/// the box or group they sit in (`None` for the page's top level). Returns the layout's notes.
fn place_scope(
    page: &Page,
    scope: Option<&str>,
    slots: &[(Unit, bool)],
    new: &HashSet<String>,
    opts: &Options,
    delta: &mut HashMap<String, Point>,
) -> Result<Vec<String>, String> {
    let placed = |n: usize| slots.get(n).is_some_and(|(_, p)| *p);
    let mut node_of: HashMap<&str, usize> = HashMap::new();
    let mut boxes: Vec<Rect> = Vec::new();
    for (n, (u, _)) in slots.iter().enumerate() {
        for id in &u.ids {
            node_of.insert(id.as_str(), n);
        }
        boxes.push(u.bounds);
    }
    // Inside a box, a shape outside it that connects to a new one anchors it too, so a step
    // added to a lane follows the step in the next lane that leads to it.
    let scope_shape = scope.and_then(|p| page.shape(p));
    if let Some(p) = scope {
        let mut holders: HashSet<String> = arrange::ancestors(page, p).into_iter().collect();
        holders.insert(p.to_string());
        for c in &page.connectors {
            let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
            for (inner, outer) in [(a, b), (b, a)] {
                if node_of.get(inner).is_some_and(|&n| placed(n))
                    && !node_of.contains_key(outer)
                    && !holders.contains(outer)
                    && let Some(s) = page.shape(outer)
                    && !container::is_callout(s)
                {
                    node_of.insert(outer, boxes.len());
                    boxes.push(s.bounds());
                }
            }
        }
    }
    let mut graph = vizcraft_layout::Graph { nodes: boxes.iter().map(node).collect(), ..vizcraft_layout::Graph::default() };
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); boxes.len()];
    for c in &page.connectors {
        let (Some(&a), Some(&b)) = (c.start.shape().and_then(|s| node_of.get(s)), c.end.shape().and_then(|s| node_of.get(s))) else { continue };
        if a != b {
            graph.edges.push(vizcraft_layout::Edge::new(a, b));
            for (x, y) in [(a, b), (b, a)] {
                if let Some(v) = neighbours.get_mut(x) {
                    v.push(y);
                }
            }
        }
    }
    let out = vizcraft_layout::layout(&graph, opts).map_err(|e| e.to_string())?;
    let rel = |n: usize| out.nodes.get(n).map_or(Point::ZERO, |p| Point::new(p.x, p.y));
    // Breadth first out from the units that stay, so each new unit is anchored on the nearest
    // already placed neighbour (those that stay first, in page order: deterministic).
    let mut at: Vec<Option<Point>> = boxes.iter().enumerate().map(|(n, b)| (!placed(n)).then(|| corner(b))).collect();
    let mut queue: VecDeque<usize> = (0..boxes.len()).filter(|&n| !placed(n)).collect();
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
    let room = scope_shape.and_then(room_in);
    let staying: Vec<Rect> = slots.iter().filter(|(_, p)| !p).map(|(u, _)| u.bounds).collect();
    // Unconnected new units: their own arrangement, to the right of what stays.
    let loose: Vec<usize> = (0..slots.len()).filter(|&n| at.get(n).is_some_and(Option::is_none)).collect();
    if !loose.is_empty() {
        let lx = loose.iter().map(|&n| rel(n).x).fold(f64::INFINITY, f64::min);
        let ly = loose.iter().map(|&n| rel(n).y).fold(f64::INFINITY, f64::min);
        let origin = match (staying.iter().copied().reduce(|a, b| a.union(&b)), room) {
            (Some(s), _) => Point::new(s.x1 + spacing(opts.component_spacing), s.y0),
            (None, Some(r)) => corner(&r),
            // Nothing stays in this scope: the new units keep their corner.
            (None, None) => {
                loose.iter().filter_map(|&n| boxes.get(n)).map(corner).reduce(|p, q| Point::new(p.x.min(q.x), p.y.min(q.y))).unwrap_or(Point::ZERO)
            }
        };
        for &n in &loose {
            if let Some(slot) = at.get_mut(n) {
                *slot = Some(Point::new(origin.x + rel(n).x - lx, origin.y + rel(n).y - ly));
            }
        }
    }
    // Clear overlaps: settled are the units that stay, callouts that are not new (on the scope's
    // box only, inside one) and the new units already cleared.
    let gap = spacing(opts.node_spacing);
    let across_x = !(opts.direction.is_horizontal() && matches!(opts.algorithm, Algorithm::Layered | Algorithm::Tree));
    let mut settled = staying;
    settled.extend(
        page.shapes
            .iter()
            .filter(|s| container::is_callout(s) && !new.contains(s.id.as_str()))
            .map(|s| s.bounds())
            .filter(|b| scope_shape.is_none_or(|p| p.bounds().intersects(b))),
    );
    for (n, (u, _)) in slots.iter().enumerate() {
        let Some(Some(to)) = at.get(n).copied().filter(|_| placed(n)) else { continue };
        let mut b = Rect::new(to.x, to.y, to.x + u.bounds.width(), to.y + u.bounds.height());
        if let Some(r) = &room {
            b = clamp_into(b, r);
        }
        let mut done = clear_of(b, &settled, across_x, gap);
        if let Some(r) = &room
            && !r.contains_rect(&done)
        {
            // Pushed out of its box: try the other way, else stay in the box.
            let side = clear_of(b, &settled, !across_x, gap);
            done = if r.contains_rect(&side) { side } else { clamp_into(done, r) };
        }
        settled.push(done);
        let d = Point::new(done.x0 - u.bounds.x0, done.y0 - u.bounds.y0);
        for id in &u.ids {
            delta.insert(id.clone(), d);
        }
    }
    Ok(out.notes)
}

/// `b` pushed right (`along_x`) or down past the rectangles in `settled` until it overlaps none of
/// them with `gap` to spare. After [`MAX_PUSHES`] pushes it jumps past all of them in one step.
fn clear_of(mut b: Rect, settled: &[Rect], along_x: bool, gap: f64) -> Rect {
    let mut pushes = 0;
    while let Some(o) = settled.iter().find(|o| overlaps_strictly(&b.inflate(gap / 2.0), &o.inflate(gap / 2.0))).copied() {
        let shift = if pushes < MAX_PUSHES {
            if along_x { o.x1 + gap - b.x0 } else { o.y1 + gap - b.y0 }
        } else {
            // Crowded: jump past everything settled in one step.
            let far = settled.iter().fold(f64::NEG_INFINITY, |m, r| m.max(if along_x { r.x1 } else { r.y1 }));
            if along_x { far + gap - b.x0 } else { far + gap - b.y0 }
        };
        if !(shift.is_finite() && shift > 0.0) {
            break;
        }
        b = if along_x { Rect::new(b.x0 + shift, b.y0, b.x1 + shift, b.y1) } else { Rect::new(b.x0, b.y0 + shift, b.x1, b.y1 + shift) };
        pushes += 1;
        if pushes > MAX_PUSHES + 1 {
            break;
        }
    }
    b
}

/// `b` moved the least distance that puts it inside `r`; one larger than `r` keeps its top-left
/// corner at `r`'s.
fn clamp_into(b: Rect, r: &Rect) -> Rect {
    let fit = |lo: f64, hi: f64, a0: f64, a1: f64| {
        if a0 < lo {
            lo - a0
        } else if a1 > hi {
            (hi - a1).max(lo - a0)
        } else {
            0.0
        }
    };
    let (dx, dy) = (fit(r.x0, r.x1, b.x0, b.x1), fit(r.y0, r.y1, b.y0, b.y1));
    Rect::new(b.x0 + dx, b.y0 + dy, b.x1 + dx, b.y1 + dy)
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
        page.board = crate::board::Board::Fixed;
        page.width = 100.0;
        page.height = 100.0;
        layout_items(&mut page, None, &Options::default()).unwrap();
        let right = page.shapes.iter().map(|s| s.bounds().x1).fold(0.0, f64::max);
        assert!((page.width - (right + PAGE_MARGIN)).abs() < 1e-9);
        assert!(page.height > 100.0);
        let mut big = tree_page();
        big.board = crate::board::Board::Fixed;
        big.width = 5000.0;
        layout_items(&mut big, None, &Options::default()).unwrap();
        assert_eq!(big.width, 5000.0);
        big.shapes.push(Shape::new("far", ShapeKind::Rectangle, MAX_EXTENT * 3.0, 0.0, 10.0, 10.0));
        grow_page(&mut big);
        assert!(big.width <= MAX_EXTENT);
    }

    #[test]
    fn an_infinite_board_has_no_sheet_to_grow() {
        let mut page = tree_page();
        page.width = 100.0;
        page.height = 100.0;
        layout_items(&mut page, None, &Options::default()).unwrap();
        assert_eq!((page.width, page.height), (100.0, 100.0));
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
    /// A page with Cascade 2 parts, all piled up: a group `g` of `ga` and `gb` with a connector
    /// between them glued to fixed points, loose shapes `n1` and `n2`, a lane set `set` of three
    /// lanes holding `l0`, `l1` and `l2`, and a callout `co` up and right of `n1`, aimed at it. Connectors run
    /// ga -> n1 -> n2 -> l0 -> l1 -> l2 and gb -> l1.
    fn parts_page() -> Page {
        let mut page = Page::new("p", "p");
        page.shapes.push(Shape::new("ga", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("gb", ShapeKind::Rectangle, 140.0, 0.0, 100.0, 60.0));
        arrange::group(&mut page, &["ga".to_string(), "gb".to_string()], "g").unwrap();
        page.shapes.push(Shape::new("n1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("n2", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(container::new_lane_set("set", 0.0, 0.0, false, "Process"));
        for (i, lane) in ["lane0", "lane1", "lane2"].iter().enumerate() {
            container::add_lane(&mut page, "set", container::new_lane(*lane, lane, false), None).unwrap();
            let r = container::content_rect(page.shape(lane).unwrap());
            let member = format!("l{i}");
            page.shapes.push(Shape::new(member.clone(), ShapeKind::Rectangle, r.x0 + 20.0 + 100.0 * i as f64, r.y0 + 20.0, 100.0, 60.0));
            container::add_members(&mut page, lane, &[member]).unwrap();
        }
        page.shapes.push(container::new_callout("co", "n1", "Note", 130.0, -80.0));
        let mut inner = Connector::new("inner", End::point("ga", 1), End::point("gb", 3));
        inner.waypoints = vec![Point::new(120.0, 30.0)];
        page.connectors.push(inner);
        for (a, b) in [("ga", "n1"), ("n1", "n2"), ("n2", "l0"), ("l0", "l1"), ("l1", "l2"), ("gb", "l1")] {
            page.connectors.push(Connector::new(format!("{a}{b}"), End::point(a, 2), End::point(b, 0)));
        }
        page
    }

    /// Where `id` sits relative to `of`, top-left to top-left, rounded to a thousandth so moving
    /// both by the same amount compares equal.
    fn offset(page: &Page, id: &str, of: &str) -> (f64, f64) {
        let (a, b) = (page.shape(id).unwrap(), page.shape(of).unwrap());
        (((a.x - b.x) * 1000.0).round() / 1000.0, ((a.y - b.y) * 1000.0).round() / 1000.0)
    }

    #[test]
    fn groups_lanes_and_callouts_survive_every_algorithm() {
        for algorithm in [Algorithm::Layered, Algorithm::Tree, Algorithm::Radial, Algorithm::Force] {
            for direction in [Direction::TopBottom, Direction::LeftRight] {
                let mut page = parts_page();
                let before = page.clone();
                let opts = Options { algorithm, direction, ..Options::default() };
                layout_items(&mut page, None, &opts).unwrap();
                let why = format!("{algorithm:?} {direction:?}");
                // The group moved as one: its members kept their places relative to each other,
                // and it is still their group, fitted to them.
                assert_eq!(offset(&page, "gb", "ga"), offset(&before, "gb", "ga"), "{why}");
                assert_eq!(page.shape("ga").unwrap().parent.as_deref(), Some("g"), "{why}");
                let (members, frame) = (arrange::bounds_of(&page, &["ga".to_string(), "gb".to_string()]).unwrap(), page.shape("g").unwrap().bounds());
                assert!(
                    [frame.x0 - members.x0, frame.y0 - members.y0, frame.x1 - members.x1, frame.y1 - members.y1].iter().all(|v| v.abs() < 1e-6),
                    "{why}"
                );
                // The lane set moved as one: lanes still stacked in it, members still in their lanes.
                for (i, lane) in ["lane0", "lane1", "lane2"].iter().enumerate() {
                    let member = format!("l{i}");
                    assert_eq!(offset(&page, lane, "set"), offset(&before, lane, "set"), "{why}: {lane}");
                    assert_eq!(offset(&page, &member, lane), offset(&before, &member, lane), "{why}: {member}");
                    assert_eq!(page.shape(&member).unwrap().parent.as_deref(), Some(*lane), "{why}");
                    assert!(page.shape(lane).unwrap().bounds().contains_rect(&page.shape(&member).unwrap().bounds()), "{why}: {member}");
                }
                // The units do not overlap each other.
                let units = ["g", "n1", "n2", "set"].map(|id| page.shape(id).unwrap().bounds());
                for (i, a) in units.iter().enumerate() {
                    for b in units.iter().skip(i + 1) {
                        assert!(!overlaps_strictly(a, b), "{why}: units overlap");
                    }
                }
                // The callout was not laid out: it sits beside its target as before (perhaps
                // mirrored to a free side), still aimed at it, covering nothing.
                let (co, n1) = (page.shape("co").unwrap(), page.shape("n1").unwrap());
                let (was_co, was_n1) = (before.shape("co").unwrap(), before.shape("n1").unwrap());
                let d = |a: &Shape, b: &Shape| {
                    let (p, q) = (a.bounds().center(), b.bounds().center());
                    (((p.x - q.x).abs() * 1000.0).round(), ((p.y - q.y).abs() * 1000.0).round())
                };
                assert_eq!(d(co, n1), d(was_co, was_n1), "{why}");
                assert_eq!(co.container.as_ref().unwrap().target.as_deref(), Some("n1"));
                for s in page.shapes.iter().filter(|s| !["co", "g", "set", "lane0", "lane1", "lane2"].contains(&s.id.as_str())) {
                    assert!(!overlaps_strictly(&co.bounds(), &s.bounds()), "{why}: the callout covers {}", s.id);
                }
            }
        }
    }

    #[test]
    fn connectors_between_moved_shapes_get_dynamic_glue_and_ones_inside_a_unit_keep_theirs() {
        let mut page = parts_page();
        let before = page.clone();
        layout_items(&mut page, None, &Options::default()).unwrap();
        let c = page.connector("inner").unwrap();
        assert_eq!((c.start.clone(), c.end.clone()), (End::point("ga", 1), End::point("gb", 3)), "inside the group: glue kept");
        let (now, was) = (page.shape("ga").unwrap(), before.shape("ga").unwrap());
        let bend = c.waypoints.first().copied().unwrap();
        assert!(
            c.waypoints.len() == 1 && (bend.x - (120.0 + now.x - was.x)).abs() < 1e-6 && (bend.y - (30.0 + now.y - was.y)).abs() < 1e-6,
            "its bend moved with the group"
        );
        let c = page.connector("n1n2").unwrap();
        assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic("n1"), End::dynamic("n2")), "between units: dynamic glue");
        // Members of one lane set moved together, so their connector keeps its glue too.
        assert_eq!(page.connector("l0l1").unwrap().start, End::point("l0", 2));
    }

    #[test]
    fn a_selection_lifts_group_members_and_lanes_to_their_unit() {
        let mut page = parts_page();
        let before = page.clone();
        // Selecting one group member and one lane lays out the whole group and the whole set.
        layout_items(&mut page, Some(&["ga".to_string(), "lane1".to_string()]), &Options::default()).unwrap();
        assert_eq!(offset(&page, "gb", "ga"), offset(&before, "gb", "ga"));
        assert_eq!(offset(&page, "lane1", "set"), offset(&before, "lane1", "set"));
        assert_eq!(page.shape("n1").unwrap(), before.shape("n1").unwrap(), "unselected shapes stay");
        assert!(!overlaps_strictly(&page.shape("g").unwrap().bounds(), &page.shape("set").unwrap().bounds()));
        // A callout alone is nothing to lay out.
        let mut page = parts_page();
        let before = page.clone();
        assert!(layout_items(&mut page, Some(&["co".to_string()]), &Options::default()).unwrap().is_empty());
        assert_eq!(page, before);
    }

    #[test]
    fn locked_shapes_stay_put() {
        let mut page = tree_page();
        page.shapes[1].x = 900.0;
        page.shapes[1].locks.position = true;
        page.layers.push(crate::page_fields::Layer { id: "lk".into(), name: "Locked".into(), locked: true, ..Default::default() });
        page.shapes[2].x = 700.0;
        page.shapes[2].layers = vec!["lk".into()];
        let before = page.clone();
        layout_items(&mut page, None, &Options::default()).unwrap();
        assert_eq!(page.shapes[1], before.shapes[1], "position locked");
        assert_eq!(page.shapes[2], before.shapes[2], "on a locked layer");
        assert_ne!(page.shapes[0], before.shapes[0], "the rest is laid out");
    }

    const ALGORITHMS: [Algorithm; 4] = [Algorithm::Layered, Algorithm::Tree, Algorithm::Radial, Algorithm::Force];
    const DIRECTIONS: [Direction; 4] = [Direction::TopBottom, Direction::BottomTop, Direction::LeftRight, Direction::RightLeft];

    /// Whether the frame of group `g` is exactly the bounds of `members`.
    fn fitted(page: &Page, g: &str, members: &[&str]) -> bool {
        let ids: Vec<String> = members.iter().map(|m| m.to_string()).collect();
        let (b, f) = (arrange::bounds_of(page, &ids).unwrap(), page.shape(g).unwrap().bounds());
        [f.x0 - b.x0, f.y0 - b.y0, f.x1 - b.x1, f.y1 - b.y1].iter().all(|v| v.abs() < 1e-6)
    }

    /// Existing content: `p`, a callout `oco` on it, and a lane set `set` with lanes `lane0`
    /// (holding `l0`) and `lane1` (holding `l1`). New content, piled up near the origin: a group
    /// `g` of `ga` and `gb`, `n1` with a callout `co` aimed at it, `m` (a new member of `lane1`,
    /// outside it), a lane set `set2` whose lane `lane2a` holds `k`, and `lk`, position locked.
    /// Connectors run p -> ga, gb -> n1, n1 -> k, l1 -> m, l0 -> m and p -> lk. Returns the page
    /// and the ids named as new (a group and a lane set stand for what is inside them).
    fn incremental_parts_page() -> (Page, Vec<String>) {
        let mut page = Page::new("p", "p");
        page.shapes.push(Shape::new("p", ShapeKind::Rectangle, 400.0, 40.0, 100.0, 60.0));
        page.shapes.push(container::new_callout("oco", "p", "Old", 530.0, 0.0));
        page.shapes.push(container::new_lane_set("set", 40.0, 300.0, false, "Process"));
        for (i, lane) in ["lane0", "lane1"].iter().enumerate() {
            container::add_lane(&mut page, "set", container::new_lane(*lane, lane, false), None).unwrap();
            let r = container::content_rect(page.shape(lane).unwrap());
            let member = format!("l{i}");
            page.shapes.push(Shape::new(member.clone(), ShapeKind::Rectangle, r.x0 + 20.0, r.y0 + 20.0, 100.0, 60.0));
            container::add_members(&mut page, lane, &[member]).unwrap();
        }
        page.shapes.push(Shape::new("ga", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("gb", ShapeKind::Rectangle, 140.0, 0.0, 100.0, 60.0));
        arrange::group(&mut page, &["ga".to_string(), "gb".to_string()], "g").unwrap();
        page.shapes.push(Shape::new("n1", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(container::new_callout("co", "n1", "Note", 130.0, -80.0));
        page.shapes.push(Shape::new("m", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        container::add_members(&mut page, "lane1", &["m".to_string()]).unwrap();
        page.shapes.push(container::new_lane_set("set2", 0.0, 0.0, false, "More"));
        container::add_lane(&mut page, "set2", container::new_lane("lane2a", "lane2a", false), None).unwrap();
        let r = container::content_rect(page.shape("lane2a").unwrap());
        page.shapes.push(Shape::new("k", ShapeKind::Rectangle, r.x0 + 20.0, r.y0 + 20.0, 100.0, 60.0));
        container::add_members(&mut page, "lane2a", &["k".to_string()]).unwrap();
        let mut lk = Shape::new("lk", ShapeKind::Rectangle, 900.0, 900.0, 100.0, 60.0);
        lk.locks.position = true;
        page.shapes.push(lk);
        for (a, b) in [("p", "ga"), ("gb", "n1"), ("n1", "k"), ("l1", "m"), ("l0", "m"), ("p", "lk")] {
            page.connectors.push(Connector::new(format!("{a}{b}"), End::point(a, 2), End::point(b, 0)));
        }
        (page, ["g", "n1", "co", "m", "set2", "lk"].map(String::from).to_vec())
    }

    #[test]
    fn incremental_places_units_whole_keeps_lane_members_in_their_lane_and_callouts_follow() {
        for algorithm in ALGORITHMS {
            for direction in DIRECTIONS {
                let (mut page, new) = incremental_parts_page();
                let before = page.clone();
                layout_incremental(&mut page, &new, &Options { algorithm, direction, ..Options::default() }).unwrap();
                let why = format!("{algorithm:?} {direction:?}");
                // Existing shapes stay exactly where they were, and so does the locked new one.
                for id in ["p", "oco", "set", "lane0", "lane1", "l0", "l1", "lk"] {
                    assert_eq!(page.shape(id), before.shape(id), "{why}: {id} moved");
                }
                // The new group and lane set were placed whole.
                assert_eq!(offset(&page, "gb", "ga"), offset(&before, "gb", "ga"), "{why}");
                assert!(fitted(&page, "g", &["ga", "gb"]), "{why}: the group frame fits its members");
                assert_eq!(offset(&page, "lane2a", "set2"), offset(&before, "lane2a", "set2"), "{why}");
                assert_eq!(offset(&page, "k", "lane2a"), offset(&before, "k", "lane2a"), "{why}");
                assert_ne!(page.shape("ga").unwrap(), before.shape("ga").unwrap(), "{why}: the group was placed");
                // The new lane member moved into its lane, clear of the lane's other member.
                let (m, lane1) = (page.shape("m").unwrap().bounds(), page.shape("lane1").unwrap());
                assert_eq!(page.shape("m").unwrap().parent.as_deref(), Some("lane1"), "{why}");
                assert!(container::content_rect(lane1).contains_rect(&m), "{why}: m left its lane");
                assert!(!overlaps_strictly(&m, &page.shape("l1").unwrap().bounds()), "{why}: m covers l1");
                // Units do not overlap each other.
                let units = ["p", "set", "g", "n1", "set2", "lk"].map(|id| (id, page.shape(id).unwrap().bounds()));
                for (i, (a, ra)) in units.iter().enumerate() {
                    for (b, rb) in units.iter().skip(i + 1) {
                        assert!(!overlaps_strictly(ra, rb), "{why}: {a} and {b} overlap");
                    }
                }
                // The new callout was not placed: it followed its target and covers nothing.
                let (co, n1) = (page.shape("co").unwrap(), page.shape("n1").unwrap());
                let d = |a: &Shape, b: &Shape| {
                    let (p, q) = (a.bounds().center(), b.bounds().center());
                    (((p.x - q.x).abs() * 1000.0).round(), ((p.y - q.y).abs() * 1000.0).round())
                };
                assert_eq!(d(co, n1), d(before.shape("co").unwrap(), before.shape("n1").unwrap()), "{why}");
                for s in page.shapes.iter().filter(|s| !["co", "g"].contains(&s.id.as_str())) {
                    assert!(!overlaps_strictly(&co.bounds(), &s.bounds()), "{why}: the callout covers {}", s.id);
                }
                // In a layered layout the group lands downstream of its parent.
                if algorithm == Algorithm::Layered {
                    let (g, p) = (page.shape("g").unwrap().bounds(), page.shape("p").unwrap().bounds());
                    match direction {
                        Direction::TopBottom => assert!(g.y0 >= p.y1, "{why}"),
                        Direction::BottomTop => assert!(g.y1 <= p.y0, "{why}"),
                        Direction::LeftRight => assert!(g.x0 >= p.x1, "{why}"),
                        Direction::RightLeft => assert!(g.x1 <= p.x0, "{why}"),
                    }
                }
            }
        }
    }

    #[test]
    fn incremental_new_members_of_existing_boxes_and_groups_stay_inside_them() {
        for algorithm in ALGORITHMS {
            for direction in DIRECTIONS {
                let mut page = Page::new("p", "p");
                page.shapes.push(container::new_container("box", 0.0, 0.0, 400.0, 300.0, "Box"));
                page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 40.0, 60.0, 100.0, 60.0));
                container::add_members(&mut page, "box", &["a".to_string()]).unwrap();
                let mut fit = container::new_container("fit", 600.0, 0.0, 10.0, 10.0, "Fit");
                fit.container.as_mut().unwrap().auto_fit = true;
                page.shapes.push(fit);
                page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 640.0, 60.0, 100.0, 60.0));
                container::add_members(&mut page, "fit", &["b".to_string()]).unwrap();
                page.shapes.push(Shape::new("c", ShapeKind::Rectangle, 0.0, 500.0, 100.0, 60.0));
                page.shapes.push(Shape::new("d", ShapeKind::Rectangle, 200.0, 500.0, 100.0, 60.0));
                arrange::group(&mut page, &["c".to_string(), "d".to_string()], "g").unwrap();
                for (id, holder) in [("x", "box"), ("y", "fit"), ("z", "g")] {
                    let mut s = Shape::new(id, ShapeKind::Rectangle, 2000.0, 2000.0, 100.0, 60.0);
                    s.parent = Some(holder.to_string());
                    page.shapes.push(s);
                }
                for (a, b) in [("a", "x"), ("b", "y"), ("c", "z")] {
                    page.connectors.push(Connector::new(format!("{a}{b}"), End::dynamic(a), End::dynamic(b)));
                }
                let before = page.clone();
                layout_incremental(&mut page, &["x", "y", "z"].map(String::from), &Options { algorithm, direction, ..Options::default() }).unwrap();
                let why = format!("{algorithm:?} {direction:?}");
                let at = |id: &str| page.shape(id).unwrap().bounds();
                for id in ["box", "a", "b", "c", "d"] {
                    assert_eq!(page.shape(id), before.shape(id), "{why}: {id} moved");
                }
                // A container that keeps its size holds the new member inside its content area.
                assert!(container::content_rect(page.shape("box").unwrap()).contains_rect(&at("x")), "{why}: x left the box");
                assert!(!overlaps_strictly(&at("x"), &at("a")), "{why}");
                // An auto-fit container keeps its corner and grows to hold the new member.
                let (fit, was) = (page.shape("fit").unwrap(), before.shape("fit").unwrap());
                assert_eq!((fit.x, fit.y), (was.x, was.y), "{why}");
                assert!(fit.bounds().contains_rect(&at("y")) && !overlaps_strictly(&at("y"), &at("b")), "{why}");
                // A group refits around its new member, which stays in it.
                assert_eq!(page.shape("z").unwrap().parent.as_deref(), Some("g"), "{why}");
                assert!(fitted(&page, "g", &["c", "d", "z"]), "{why}");
                assert!(!overlaps_strictly(&at("z"), &at("c")) && !overlaps_strictly(&at("z"), &at("d")), "{why}");
            }
        }
    }

    #[test]
    fn incremental_lifts_named_members_to_their_group_and_never_places_callouts() {
        // Naming only the members of a new group places the group whole.
        let (mut page, _) = incremental_parts_page();
        let before = page.clone();
        layout_incremental(&mut page, &["ga".to_string(), "gb".to_string()], &Options::default()).unwrap();
        assert_eq!(offset(&page, "gb", "ga"), offset(&before, "gb", "ga"));
        assert!(page.shape("g").unwrap().bounds().y0 >= page.shape("p").unwrap().bounds().y1, "below its parent");
        assert!(fitted(&page, "g", &["ga", "gb"]));
        // A callout on its own is never placed, and a new callout on an existing shape stays.
        let (mut page, _) = incremental_parts_page();
        let before = page.clone();
        assert!(layout_incremental(&mut page, &["oco".to_string()], &Options::default()).unwrap().is_empty());
        assert_eq!(page, before);
        // Connectors from a moved shape to one that stayed switch to dynamic glue; ones inside the
        // moved lane set keep theirs.
        let (mut page, new) = incremental_parts_page();
        page.connectors.push(Connector::new("inside", End::point("lane2a", 1), End::point("k", 3)));
        layout_incremental(&mut page, &new, &Options::default()).unwrap();
        assert_eq!(page.connector("pga").unwrap().start, End::dynamic("p"));
        assert_eq!(page.connector("inside").unwrap().start, End::point("lane2a", 1));
    }
}
