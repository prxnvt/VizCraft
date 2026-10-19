//! Wire segments, junctions and nets on the page.
//!
//! A segment is a connector whose `net` names a net of the model (`vizcraft_model::Net`). Segments
//! join only through glue: two ends glued to the same junction, or point-glued to the same pin
//! (connection point) of the same shape. Geometry never joins them, so two wires that merely cross
//! stay two nets, and dynamic glue to a shape joins nothing.
//!
//! A junction is an ordinary ellipse shape with the stencil [`JUNCTION_STENCIL`] and a single
//! connection point at its centre, the way a group is an ordinary shape with `vizcraft.group`.
//! Wires glue to it with point glue, so moving it moves every wire on it through the normal glue
//! resolution, and every export draws it with no special case. A wire drawn to the middle of
//! another wire splits that wire at a new junction ([`split_at`]); a junction left between two
//! wires is healed away by joining them ([`join`]).
//!
//! The net pass of the sync step (`vizcraft-notation`) keeps nets and segments consistent;
//! [`NetIndex`] is the read side the engine keeps beside the document, and [`label_spots`] places
//! the wire numbers every renderer writes.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::geom::{Point, distance_to_segment, nearest_on_segment, polyline_length};
use crate::kind::ShapeKind;
use crate::model::{Color, Connector, Document, End, Page, Shape};
use crate::route::route_polyline;

/// The stencil that marks a shape as a junction.
pub const JUNCTION_STENCIL: &str = "vizcraft.junction";

/// Diameter of a junction dot, in line widths.
pub const JUNCTION_WIDTHS: f64 = 3.0;

/// Whether `s` is a junction.
pub fn is_junction(s: &Shape) -> bool {
    s.stencil == JUNCTION_STENCIL
}

/// A junction centred at `at` for a line of `line_width` in `color`: a dot of
/// [`JUNCTION_WIDTHS`] line widths filled with the line colour when `dot` is set (wires), else
/// an invisible point of the same size with no fill and no line (pipe tees, which draw no dot).
/// Its one connection point is its centre.
pub fn junction(id: impl Into<String>, at: Point, dot: bool, line_width: f64, color: Color) -> Shape {
    let lw = if line_width.is_finite() { line_width.clamp(1.0, 100.0) } else { 1.0 };
    let d = JUNCTION_WIDTHS * lw;
    let at = at.sanitized();
    let mut s = Shape::new(id, ShapeKind::Ellipse, at.x - d / 2.0, at.y - d / 2.0, d, d);
    s.stencil = JUNCTION_STENCIL.to_string();
    s.connection_points = vec![Point::new(0.5, 0.5)];
    s.style.line_width = 0.0;
    if dot {
        s.style.fill = color;
        s.style.line = color;
    } else {
        s.style.fill = Color::TRANSPARENT;
        s.style.line = Color::TRANSPARENT;
    }
    s.sanitize();
    s
}

/// The pin an end is point-glued to: the shape and its connection point index, for an end glued
/// to an existing shape that is not a junction by point or port. Dynamic glue, free ends and glue
/// to a missing shape have no pin.
pub fn pin_of(page: &Page, e: &End) -> Option<(String, usize)> {
    let End::Glued { shape, point, port } = e else { return None };
    let s = page.shape(shape)?;
    if is_junction(s) || (point.is_none() && port.is_none()) {
        return None;
    }
    Some((s.id.clone(), e.resolve_port(s)?))
}

/// The junction an end is glued to, if any.
pub fn junction_of<'a>(page: &'a Page, e: &End) -> Option<&'a Shape> {
    page.shape(e.shape()?).filter(|s| is_junction(s))
}

/// What one end of a segment joins.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Node {
    Junction(String),
    Pin(String, usize),
}

fn node_of(page: &Page, e: &End) -> Option<Node> {
    if let Some(j) = junction_of(page, e) {
        return Some(Node::Junction(j.id.clone()));
    }
    pin_of(page, e).map(|(s, i)| Node::Pin(s, i))
}

/// Union-find over `0..n`, iterative so a long chain cannot overflow the stack.
struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu { parent: (0..n).collect() }
    }
    fn find(&mut self, mut x: usize) -> usize {
        let mut root = x;
        while let Some(&p) = self.parent.get(root) {
            if p == root {
                break;
            }
            root = p;
        }
        while let Some(&p) = self.parent.get(x) {
            if p == root {
                break;
            }
            if let Some(slot) = self.parent.get_mut(x) {
                *slot = root;
            }
            x = p;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb
            && let Some(slot) = self.parent.get_mut(rb)
        {
            *slot = ra;
        }
    }
}

/// A set of segments on one page joined by glue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    /// Index of the page in `Document::pages`.
    pub page: usize,
    /// Indexes of the segments in the page's connectors, in z order.
    pub segments: Vec<usize>,
    /// Junction shape ids, in first-seen order.
    pub junctions: Vec<String>,
    /// Pins (shape id, connection point index), in first-seen order.
    pub pins: Vec<(String, usize)>,
}

/// The connected components of the connectors of page `page_index` that `is_segment` accepts.
/// Two segments are in one component when a chain of shared junctions and shared pins links
/// them; nothing else joins them. Components come in the order of their first segment; linear in
/// the page.
pub fn components(page_index: usize, page: &Page, is_segment: impl Fn(&Connector) -> bool) -> Vec<Component> {
    let segs: Vec<usize> = page.connectors.iter().enumerate().filter(|(_, c)| is_segment(c)).map(|(i, _)| i).collect();
    let mut nodes: HashMap<Node, usize> = HashMap::new();
    let mut node_list: Vec<Node> = Vec::new();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    for (k, &ci) in segs.iter().enumerate() {
        let Some(c) = page.connectors.get(ci) else { continue };
        for e in [&c.start, &c.end] {
            let Some(n) = node_of(page, e) else { continue };
            let next = segs.len().saturating_add(node_list.len());
            let id = *nodes.entry(n.clone()).or_insert_with(|| {
                node_list.push(n);
                next
            });
            ends.push((k, id));
        }
    }
    let mut dsu = Dsu::new(segs.len().saturating_add(node_list.len()));
    for &(k, n) in &ends {
        dsu.union(k, n);
    }
    let mut by_root: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<Component> = Vec::new();
    for (k, &ci) in segs.iter().enumerate() {
        let r = dsu.find(k);
        let at = *by_root.entry(r).or_insert_with(|| {
            out.push(Component { page: page_index, ..Component::default() });
            out.len() - 1
        });
        if let Some(comp) = out.get_mut(at) {
            comp.segments.push(ci);
        }
    }
    for (i, n) in node_list.iter().enumerate() {
        let r = dsu.find(segs.len().saturating_add(i));
        let Some(comp) = by_root.get(&r).and_then(|&at| out.get_mut(at)) else { continue };
        match n {
            Node::Junction(j) => comp.junctions.push(j.clone()),
            Node::Pin(s, p) => comp.pins.push((s.clone(), *p)),
        }
    }
    out
}

/// Where a point lands on a polyline: the segment index, the nearest point on it and its
/// distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    /// Index of the segment (`pts[i]` to `pts[i + 1]`).
    pub segment: usize,
    /// The nearest point on that segment.
    pub point: Point,
    /// Distance from the projected point.
    pub distance: f64,
}

/// The nearest point to `p` on the polyline `pts` (first segment wins a tie); `None` for fewer
/// than two points or a non-finite `p`.
pub fn project(pts: &[Point], p: Point) -> Option<Projection> {
    if !p.is_finite() {
        return None;
    }
    let mut best: Option<Projection> = None;
    for (i, w) in pts.windows(2).enumerate() {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
        let d = distance_to_segment(p, a, b);
        if d.is_finite() && best.is_none_or(|bp| d < bp.distance) {
            best = Some(Projection { segment: i, point: nearest_on_segment(p, a, b), distance: d });
        }
    }
    best
}

/// Split connector `connector` of `page` at the point of its route nearest `at`: a junction
/// `junction_id` ([`junction`], dotted when `dot`) goes there, the connector keeps the run up
/// to it (its bends before the cut become explicit bends) and a new connector `new_id` takes the
/// rest, copying the style, kind, net and marker; both halves are point-glued to the junction.
/// The end arrow and end label move to the new half. Returns the junction's centre, or `None`
/// (and no change) when the connector is missing, the ids are taken, or the cut falls on an end.
pub fn split_at(page: &mut Page, connector: &str, at: Point, new_id: &str, junction_id: &str, dot: bool) -> Option<Point> {
    if page.contains_id(new_id) || page.contains_id(junction_id) || new_id == junction_id {
        return None;
    }
    let c = page.connector(connector)?;
    let poly = route_polyline(page, c);
    let hit = project(&poly, at)?;
    let (first, last) = (*poly.first()?, *poly.last()?);
    if hit.point.distance(first) < 1e-6 || hit.point.distance(last) < 1e-6 {
        return None;
    }
    let q = hit.point;
    let before: Vec<Point> = poly.get(1..=hit.segment).unwrap_or_default().to_vec();
    let after: Vec<Point> = poly.get(hit.segment.saturating_add(1)..poly.len().saturating_sub(1)).unwrap_or_default().to_vec();
    let mut tail = c.clone();
    tail.id = new_id.to_string();
    tail.start = End::point(junction_id, 0);
    tail.waypoints = after;
    tail.start_arrow = crate::Arrow::None;
    tail.start_label.clear();
    tail.text.clear();
    tail.relationship = None;
    tail.avoid = false;
    let j = junction(junction_id, q, dot, c.style.line_width, c.style.effective_line());
    let c = page.connector_mut(connector)?;
    c.end = End::point(junction_id, 0);
    c.waypoints = before;
    c.end_arrow = crate::Arrow::None;
    c.end_label.clear();
    c.avoid = false;
    page.shapes.push(j);
    page.connectors.push(tail);
    Some(q)
}

/// The world position of end `e`: its glued connection point, else the shape centre, else the
/// free point.
fn end_pos(page: &Page, e: &End) -> Option<Point> {
    match e {
        End::Free { x, y } => Some(Point::new(*x, *y)),
        End::Glued { shape, .. } => {
            let s = page.shape(shape)?;
            Some(e.resolve_port(s).and_then(|i| s.connection_point_world(i)).unwrap_or_else(|| s.center()))
        }
    }
}

/// Turn a connector around: ends, bends, arrows and end labels swap.
pub fn reverse(c: &mut Connector) {
    std::mem::swap(&mut c.start, &mut c.end);
    c.waypoints.reverse();
    std::mem::swap(&mut c.start_arrow, &mut c.end_arrow);
    std::mem::swap(&mut c.start_label, &mut c.end_label);
}

/// Join connectors `a` and `b`, which each have an end glued to shape `via` (a junction, or a
/// device being removed), into `a`: `a` runs from its far end through its bends, the two glued
/// points, then `b`'s bends to `b`'s far end, and `b` is removed. `a` keeps its id, text, style,
/// kind and net and takes `b`'s far end arrow and label. `via` itself is left on the page.
/// Returns false (and no change) when either connector is missing, they are the same connector or
/// either has no end on `via`.
pub fn join(page: &mut Page, a: &str, b: &str, via: &str) -> bool {
    if a == b {
        return false;
    }
    let (Some(ca), Some(cb)) = (page.connector(a), page.connector(b)) else { return false };
    let (mut ca, mut cb) = (ca.clone(), cb.clone());
    // `a` must end at `via` and `b` start there.
    if ca.end.shape() != Some(via) {
        if ca.start.shape() != Some(via) {
            return false;
        }
        reverse(&mut ca);
    }
    if cb.start.shape() != Some(via) {
        if cb.end.shape() != Some(via) {
            return false;
        }
        reverse(&mut cb);
    }
    let mut through: Vec<Point> = Vec::new();
    for p in [end_pos(page, &ca.end), end_pos(page, &cb.start)].into_iter().flatten() {
        if through.last().is_none_or(|l: &Point| l.distance(p) > 1e-9) {
            through.push(p);
        }
    }
    let mut wps = std::mem::take(&mut ca.waypoints);
    wps.extend(through);
    wps.extend(cb.waypoints.iter().copied());
    wps.truncate(crate::MAX_WAYPOINTS);
    ca.waypoints = wps;
    ca.end = cb.end.clone();
    ca.end_arrow = cb.end_arrow;
    ca.end_label = std::mem::take(&mut cb.end_label);
    ca.avoid = false;
    if let Some(slot) = page.connector_mut(a) {
        *slot = ca;
    }
    page.remove_connector(b);
    true
}

/// The connector ends glued to shape `shape` on `page`: `(connector id, true for the start)`,
/// in z order.
pub fn ends_on(page: &Page, shape: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for c in &page.connectors {
        if c.start.shape() == Some(shape) {
            out.push((c.id.clone(), true));
        }
        if c.end.shape() == Some(shape) {
            out.push((c.id.clone(), false));
        }
    }
    out
}

/// Everything known about one net from the drawing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetEntry {
    /// Its segments (connector ids), page by page in z order.
    pub segments: Vec<String>,
    /// The pins its segments are glued to, `(shape id, connection point index)`.
    pub pins: Vec<(String, usize)>,
    /// The junctions on it.
    pub junctions: Vec<String>,
    /// The pages it is drawn on, in page order.
    pub pages: Vec<String>,
}

/// Two or more named nets joined by wiring, which the net pass leaves unmerged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conflict {
    /// The page id.
    pub page: String,
    /// The named nets that meet, in model order.
    pub nets: Vec<String>,
    /// The segments of the joined wiring.
    pub segments: Vec<String>,
}

/// Which net every segment, pin and junction belongs to, built from the document in one linear
/// pass. The engine keeps one beside the document (`Session.nets`), rebuilt after every change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetIndex {
    /// Each net of the model, with what the drawing has of it (empty entries for nets drawn
    /// nowhere).
    pub nets: BTreeMap<String, NetEntry>,
    /// The net of each segment.
    pub of_connector: HashMap<String, String>,
    /// The net at each pin `(shape id, connection point index)`; with two nets on one pin the
    /// first wins (the net pass merges them or reports a conflict).
    pub of_pin: HashMap<(String, usize), String>,
    /// The net of each junction.
    pub of_junction: HashMap<String, String>,
    /// Named nets joined by wiring.
    pub conflicts: Vec<Conflict>,
}

impl NetIndex {
    /// Index `doc`.
    pub fn build(doc: &Document) -> Self {
        let mut ix = NetIndex::default();
        if doc.model.nets.is_empty() {
            return ix;
        }
        for n in &doc.model.nets {
            ix.nets.entry(n.id.clone()).or_default();
        }
        let named: HashSet<&str> = doc.model.nets.iter().filter(|n| n.named()).map(|n| n.id.as_str()).collect();
        let order: HashMap<&str, usize> = doc.model.nets.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
        for (pi, page) in doc.pages.iter().enumerate() {
            for c in &page.connectors {
                let Some(net) = c.net.as_deref().filter(|n| ix.nets.contains_key(*n)) else { continue };
                ix.of_connector.insert(c.id.clone(), net.to_string());
                let entry = ix.nets.entry(net.to_string()).or_default();
                entry.segments.push(c.id.clone());
                if entry.pages.last() != Some(&page.id) {
                    entry.pages.push(page.id.clone());
                }
                for e in [&c.start, &c.end] {
                    if let Some(j) = junction_of(page, e) {
                        if !entry.junctions.contains(&j.id) {
                            entry.junctions.push(j.id.clone());
                        }
                        ix.of_junction.entry(j.id.clone()).or_insert_with(|| net.to_string());
                    } else if let Some(pin) = pin_of(page, e) {
                        if !entry.pins.contains(&pin) {
                            entry.pins.push(pin.clone());
                        }
                        ix.of_pin.entry(pin).or_insert_with(|| net.to_string());
                    }
                }
            }
            if named.len() < 2 {
                continue;
            }
            for comp in components(pi, page, |c| c.net.is_some()) {
                let mut ids: Vec<&str> =
                    comp.segments.iter().filter_map(|&i| page.connectors.get(i)?.net.as_deref()).filter(|n| named.contains(n)).collect();
                ids.sort_by_key(|n| order.get(n).copied().unwrap_or(usize::MAX));
                ids.dedup();
                if ids.len() >= 2 {
                    ix.conflicts.push(Conflict {
                        page: page.id.clone(),
                        nets: ids.into_iter().map(str::to_string).collect(),
                        segments: comp.segments.iter().filter_map(|&i| page.connectors.get(i).map(|c| c.id.clone())).collect(),
                    });
                }
            }
        }
        ix
    }
    /// The net of connector `id`.
    pub fn net_of_connector(&self, id: &str) -> Option<&str> {
        self.of_connector.get(id).map(String::as_str)
    }
}

/// One wire number or name written beside a segment.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSpot {
    /// The segment it is written on.
    pub connector: String,
    /// The text: the net's number, else its name.
    pub text: String,
    /// Centre of the text, beside the line.
    pub pos: Point,
    /// Text angle in degrees: 0 along horizontal runs, -90 along vertical runs (read from the
    /// bottom, IEC 61082).
    pub angle: f64,
}

/// The longest straight run of `pts` (first wins a tie), as its two ends.
fn longest_run(pts: &[Point]) -> Option<(Point, Point)> {
    let mut best: Option<(Point, Point, f64)> = None;
    for w in pts.windows(2) {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
        let d = a.distance(b);
        if d.is_finite() && best.is_none_or(|(_, _, bd)| d > bd + 1e-9) {
            best = Some((a, b, d));
        }
    }
    best.map(|(a, b, _)| (a, b))
}

/// Where a label of font size `font_size` goes beside the polyline `pts` drawn `line_width` wide:
/// the middle of its longest run, on the upper side of a horizontal run or the left side of a
/// vertical one, with the text upright.
pub fn label_spot(pts: &[Point], font_size: f64, line_width: f64) -> Option<(Point, f64)> {
    let (a, b) = longest_run(pts)?;
    let m = a.midpoint(b);
    let fs = if font_size.is_finite() { font_size.clamp(1.0, 1000.0) } else { 10.0 };
    let lw = if line_width.is_finite() { line_width.clamp(0.0, 100.0) } else { 1.0 };
    let gap = lw / 2.0 + fs * 0.7;
    let d = b - a;
    if d.y.abs() > d.x.abs() {
        // Vertical: read from the bottom, to the left of the line.
        Some((Point::new(m.x - gap, m.y), -90.0))
    } else {
        Some((Point::new(m.x, m.y - gap), 0.0))
    }
}

/// Every net label on `page` (a page of `doc`, whose model holds the nets): for each net with a number or name whose label
/// mode is not `off`, on its longest segment of the page (`once`) or on every segment (`each`).
pub fn label_spots(doc: &Document, page: &Page) -> Vec<LabelSpot> {
    if doc.model.nets.is_empty() {
        return Vec::new();
    }
    let nets: HashMap<&str, &vizcraft_model::Net> = doc.model.nets.iter().map(|n| (n.id.as_str(), n)).collect();
    // Per net: the segments with their polylines.
    let mut per_net: BTreeMap<&str, Vec<(&Connector, Vec<Point>)>> = BTreeMap::new();
    for c in &page.connectors {
        let Some(n) = c.net.as_deref().and_then(|id| nets.get(id)) else { continue };
        if n.label == vizcraft_model::NetLabel::Off || n.label_text().is_empty() {
            continue;
        }
        per_net.entry(n.id.as_str()).or_default().push((c, route_polyline(page, c)));
    }
    let mut out = Vec::new();
    for (id, segs) in per_net {
        let Some(n) = nets.get(id) else { continue };
        let chosen: Vec<&(&Connector, Vec<Point>)> = match n.label {
            vizcraft_model::NetLabel::Each => segs.iter().collect(),
            _ => segs
                .iter()
                .fold(None::<&(&Connector, Vec<Point>)>, |best, s| match best {
                    Some(b) if polyline_length(&b.1) >= polyline_length(&s.1) => Some(b),
                    _ => Some(s),
                })
                .into_iter()
                .collect(),
        };
        for (c, pts) in chosen {
            if let Some((pos, angle)) = label_spot(pts, c.style.font_size, c.style.line_width) {
                out.push(LabelSpot { connector: c.id.clone(), text: n.label_text().to_string(), pos, angle });
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
