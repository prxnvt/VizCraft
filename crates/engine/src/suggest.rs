//! The suggestion engine behind insertion on the canvas: what may come next after a shape, which
//! relationships may join two shapes, and a search over commands, element types and elements for
//! the command palette.
//!
//! Everything here reads the document and the notation catalog and changes nothing; the
//! `suggest.*` and `insert.*` commands in `cmd/suggest.rs` wrap it. Suggestions come from the
//! pack's connection table (`legal::legal_from`), so a pack loaded at run time suggests its own
//! types without code here knowing about it.

use std::collections::HashMap;

use serde::Serialize;
use vizcraft_diagram::{Document, Page, Shape};
use vizcraft_notation::{Catalog, legal};

/// The connector kind a free shape's next step is joined with, and the fallback for a
/// relationship type no kind draws.
pub const BASIC_ARROW: &str = "basic.arrow";

/// Longest query looked at; the rest is ignored, so a hostile caller cannot make a search slow.
pub const MAX_QUERY_CHARS: usize = 200;

/// Most hits one search returns.
pub const MAX_HITS: usize = 200;

/// One thing that may follow a shape: a new shape of `type_id` joined by `relationship`, drawn
/// as connector kind `kind`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    /// The element type of the new shape (`flowchart.Process`), or the stencil id of a free
    /// shape's next step (`basic.rectangle`).
    #[serde(rename = "type")]
    pub type_id: String,
    /// The type's label as the palette shows it.
    pub label: String,
    /// The relationship type joining the two, `None` between free shapes.
    pub relationship: Option<String>,
    /// The connector kind the new connector is drawn as.
    pub kind: String,
}

/// A relationship type that may join two shapes, as the magic connector lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RelationshipChoice {
    /// Qualified relationship type, or empty for a plain connector between free shapes.
    pub id: String,
    /// Label to show.
    pub label: String,
    /// Whether the pack's connection table allows it; illegal choices come last and a rule
    /// reports them once drawn.
    pub legal: bool,
}

/// What a search hit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HitKind {
    /// A command: running it is the action.
    Command,
    /// An element type of a pack in use: inserting one is the action.
    Type,
    /// An element of the model: selecting and revealing it is the action.
    Element,
}

impl HitKind {
    /// The name the `kinds` filter uses.
    pub fn name(self) -> &'static str {
        match self {
            HitKind::Command => "command",
            HitKind::Type => "type",
            HitKind::Element => "element",
        }
    }
    /// The kind with this name.
    pub fn parse(name: &str) -> Option<HitKind> {
        match name {
            "command" => Some(HitKind::Command),
            "type" => Some(HitKind::Type),
            "element" => Some(HitKind::Element),
            _ => None,
        }
    }
    /// Every kind, in the order a search lists them on a tie.
    pub const ALL: [HitKind; 3] = [HitKind::Command, HitKind::Type, HitKind::Element];
}

/// One search hit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHit {
    /// What the hit is.
    pub kind: HitKind,
    /// Command id, element type id or element id.
    pub id: String,
    /// Label to show.
    pub label: String,
    /// A second line: the command's location and shortcut, the type's pack, the element's tag
    /// and type.
    pub detail: String,
    /// For an element: the index of the first page drawing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    /// For an element: the first shape drawing it on that page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// 0 for a label prefix match, 1 for a label substring, 2 for any other field.
    pub rank: u8,
}

/// A command as [`search`] sees it: the engine registry's rows plus any the caller adds (the UI
/// layer's `ui.*` commands).
#[derive(Clone, Copy, Debug)]
pub struct CommandRow<'a> {
    /// Command id.
    pub id: &'a str,
    /// Label.
    pub label: &'a str,
    /// Where it lives in the UI.
    pub location: &'a str,
    /// Default shortcut, portable form.
    pub shortcut: &'a str,
}

/// The connector kind the catalog draws `relationship` with: the first kind that becomes that
/// relationship type, else [`BASIC_ARROW`].
pub fn kind_for(relationship: Option<&str>) -> String {
    relationship
        .and_then(|r| vizcraft_stencils::connector_kinds().iter().find(|k| k.relationship_type == Some(r)))
        .map(|k| k.id.to_string())
        .unwrap_or_else(|| BASIC_ARROW.to_string())
}

/// The element type the shape draws, when it draws an element whose type the catalog knows.
pub fn element_type_of<'a>(doc: &'a Document, shape: &Shape) -> Option<&'a str> {
    let e = doc.model.element(shape.element.as_deref()?)?;
    Some(e.type_id.as_str())
}

/// The stencil a free shape's next step uses: its own master, else a master of the same outline,
/// else the basic rectangle.
pub fn stencil_of(shape: &Shape) -> String {
    if vizcraft_stencils::master(&shape.stencil).is_some() {
        return shape.stencil.clone();
    }
    vizcraft_stencils::sets()
        .iter()
        .flat_map(|s| s.masters.iter())
        .find(|m| m.kind == shape.kind)
        .map(|m| m.id.to_string())
        .unwrap_or_else(|| "basic.rectangle".to_string())
}

/// The label of an element type, or its id when the catalog does not know it.
fn type_label(cat: &Catalog, t: &str) -> String {
    cat.element_type(t).map(|(_, e)| e.label.clone()).unwrap_or_else(|| t.to_string())
}

/// How often each (source type, target type) pair is joined by a connector on `page`.
fn pair_counts<'a>(doc: &'a Document, page: &'a Page) -> HashMap<(&'a str, &'a str), usize> {
    let mut out: HashMap<(&str, &str), usize> = HashMap::new();
    let type_at = |id: Option<&str>| -> Option<&'a str> { page.shape(id?).and_then(|s| element_type_of(doc, s)) };
    for c in &page.connectors {
        if let (Some(a), Some(b)) = (type_at(c.start.shape()), type_at(c.end.shape())) {
            let n = out.entry((a, b)).or_insert(0);
            *n = n.saturating_add(1);
        }
    }
    out
}

/// Position of `t` in its pack's palette (groups in order, types in order); types not in a
/// palette sort after every type that is.
fn palette_index(cat: &Catalog, t: &str) -> usize {
    let Some((pack, _)) = cat.element_type(t) else { return usize::MAX };
    pack.palette.iter().flat_map(|g| g.types.iter()).position(|x| x == t).unwrap_or(usize::MAX)
}

/// The types the page's viewpoint allows, as a predicate; every type when the page has no
/// viewpoint or the catalog does not know it.
fn viewpoint_allows<'a>(cat: &'a Catalog, page: &'a Page) -> impl Fn(&str) -> bool + 'a {
    let vp = page.view.as_ref().map(|v| v.viewpoint.as_str()).filter(|v| !v.is_empty());
    let found = vp.and_then(|id| cat.pack(vizcraft_model::pack_of(id)).and_then(|p| p.viewpoint(id).map(|v| (p, v))));
    move |t: &str| match found {
        Some((pack, v)) if !v.types.is_empty() => pack.lists(&v.types, t),
        _ => true,
    }
}

/// What may follow shape `shape_id` on the active page. For a shape drawing an element: every
/// legal target type with its relationship type, from the connection table of the element's
/// pack, kept to the page's viewpoint, ordered by how often that pair already occurs on the page
/// and then by palette order. For a free shape: the same stencil joined by the Basic Arrow kind.
/// Empty when the shape is not on the active page.
pub fn next(doc: &Document, shape_id: &str) -> Vec<Suggestion> {
    let Some(page) = doc.page() else { return Vec::new() };
    let Some(shape) = page.shape(shape_id) else { return Vec::new() };
    let cat = vizcraft_notation::catalog();
    let Some(source) = element_type_of(doc, shape) else {
        let stencil = stencil_of(shape);
        let label = vizcraft_stencils::master(&stencil).map(|m| m.label.to_string()).unwrap_or_else(|| stencil.clone());
        return vec![Suggestion { type_id: stencil, label, relationship: None, kind: BASIC_ARROW.to_string() }];
    };
    let allows = viewpoint_allows(cat, page);
    let counts = pair_counts(doc, page);
    let mut out: Vec<(usize, usize, usize, Suggestion)> = Vec::new();
    for l in legal::legal_from(cat, source) {
        let kind = kind_for(Some(&l.relationship));
        for t in l.targets {
            // A target the catalog cannot draw is no use on a canvas.
            if !allows(&t)
                || cat.master_for_type(&t).is_none()
                || out.iter().any(|(.., s)| s.type_id == t && s.relationship.as_deref() == Some(&l.relationship))
            {
                continue;
            }
            let n = counts.get(&(source, t.as_str())).copied().unwrap_or(0);
            let order = out.len();
            let s = Suggestion { label: type_label(cat, &t), type_id: t.clone(), relationship: Some(l.relationship.clone()), kind: kind.clone() };
            out.push((n, palette_index(cat, &t), order, s));
        }
    }
    // Most used pair first, then palette order, then connection-table order.
    out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    out.into_iter().map(|(.., s)| s).collect()
}

/// The relationship types that may join shape `from` to shape `to` on the active page: legal ones
/// first in connection-table order, then the other relationship types of the source's pack (and
/// the target's), marked illegal. Between shapes where either is free, one plain connector choice
/// (empty id). Empty when either shape is not on the active page.
pub fn connect(doc: &Document, from: &str, to: &str) -> Vec<RelationshipChoice> {
    let Some(page) = doc.page() else { return Vec::new() };
    let (Some(a), Some(b)) = (page.shape(from), page.shape(to)) else { return Vec::new() };
    let cat = vizcraft_notation::catalog();
    let (Some(st), Some(tt)) = (element_type_of(doc, a), element_type_of(doc, b)) else {
        return vec![RelationshipChoice { id: String::new(), label: "Connector".into(), legal: true }];
    };
    let label = |r: &str| cat.relationship_type(r).map(|(_, t)| t.label.clone()).unwrap_or_else(|| r.to_string());
    let mut out: Vec<RelationshipChoice> = Vec::new();
    for l in legal::legal_from(cat, st) {
        if l.targets.iter().any(|t| t == tt) && !out.iter().any(|c| c.id == l.relationship) {
            out.push(RelationshipChoice { label: label(&l.relationship), id: l.relationship, legal: true });
        }
    }
    let packs = [cat.element_type(st).map(|(p, _)| p), cat.element_type(tt).map(|(p, _)| p)];
    for pack in packs.into_iter().flatten() {
        for r in &pack.relationship_types {
            if !out.iter().any(|c| c.id == r.id) {
                out.push(RelationshipChoice { id: r.id.clone(), label: r.label.clone(), legal: legal::is_legal(cat, st, &r.id, tt) });
            }
        }
    }
    // Legal first; the sort is stable, so each half keeps its order.
    out.sort_by_key(|c| !c.legal);
    out
}

/// How well `q` (lower case, trimmed, non-empty) matches: 0 when the label starts with it, 1 when
/// the label contains it, 2 when one of `others` does, `None` otherwise. An empty query matches
/// everything at 0.
fn rank(q: &str, label: &str, others: &[&str]) -> Option<u8> {
    if q.is_empty() {
        return Some(0);
    }
    let l = label.to_lowercase();
    if l.starts_with(q) {
        Some(0)
    } else if l.contains(q) {
        Some(1)
    } else if others.iter().any(|o| o.to_lowercase().contains(q)) {
        Some(2)
    } else {
        None
    }
}

/// The element types a search offers: those of the packs the model uses, or of every loaded pack
/// with element types when it uses none yet (so a blank document can start a flowchart), in pack
/// palette order and then declaration order.
pub fn types_in_use(doc: &Document) -> Vec<String> {
    let cat = vizcraft_notation::catalog();
    let used: Vec<&vizcraft_notation::Pack> =
        doc.model.packs.iter().filter_map(|p| cat.pack(&p.id)).filter(|p| !p.element_types.is_empty()).collect();
    let packs: Vec<&vizcraft_notation::Pack> =
        if used.is_empty() { cat.packs().iter().filter(|p| !p.element_types.is_empty()).collect() } else { used };
    let mut out: Vec<String> = Vec::new();
    for pack in packs {
        let palette = pack.palette.iter().flat_map(|g| g.types.iter());
        for t in palette.chain(pack.element_types.iter().map(|e| &e.id)) {
            if !out.contains(t) && cat.master_for_type(t).is_some() {
                out.push(t.clone());
            }
        }
    }
    out
}

/// Search `commands`, the element types in use ([`types_in_use`]) and the model's elements by
/// name or tag for `query` (case-insensitive), keeping the hit kinds in `kinds` (every kind when
/// empty). Best first: label prefix matches, then label substrings, then other fields; ties keep
/// commands, types, elements in that order. At most `limit` hits (capped at [`MAX_HITS`]).
pub fn search(doc: &Document, commands: &[CommandRow<'_>], query: &str, kinds: &[HitKind], limit: usize) -> Vec<SearchHit> {
    let q: String = query.trim().chars().take(MAX_QUERY_CHARS).collect::<String>().to_lowercase();
    let wants = |k: HitKind| kinds.is_empty() || kinds.contains(&k);
    let limit = limit.min(MAX_HITS);
    let cat = vizcraft_notation::catalog();
    let mut hits: Vec<SearchHit> = Vec::new();
    if wants(HitKind::Command) {
        for c in commands {
            if hits.iter().any(|h| h.kind == HitKind::Command && h.id == c.id) {
                continue;
            }
            if let Some(r) = rank(&q, c.label, &[c.id, c.location]) {
                let detail = if c.shortcut.is_empty() { c.location.to_string() } else { format!("{}  {}", c.location, c.shortcut) };
                hits.push(SearchHit {
                    kind: HitKind::Command,
                    id: c.id.to_string(),
                    label: c.label.to_string(),
                    detail,
                    page: None,
                    shape: None,
                    rank: r,
                });
            }
        }
    }
    if wants(HitKind::Type) {
        for t in types_in_use(doc) {
            let Some((pack, et)) = cat.element_type(&t) else { continue };
            if let Some(r) = rank(&q, &et.label, &[&t, &pack.name, &et.description]) {
                hits.push(SearchHit {
                    kind: HitKind::Type,
                    label: et.label.clone(),
                    detail: format!("{} type", pack.name),
                    id: t,
                    page: None,
                    shape: None,
                    rank: r,
                });
            }
        }
    }
    if wants(HitKind::Element) {
        for e in &doc.model.elements {
            let shown = if e.name.is_empty() { e.tag.as_str() } else { e.name.as_str() };
            let Some(r) = rank(&q, shown, &[&e.tag, &e.id]) else { continue };
            let view = vizcraft_diagram::views::views_of(doc, &e.id).into_iter().next();
            let label = if shown.is_empty() { e.id.clone() } else { shown.to_string() };
            let detail = [e.tag.as_str(), &type_label(cat, &e.type_id)].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("  ");
            hits.push(SearchHit {
                kind: HitKind::Element,
                id: e.id.clone(),
                label,
                detail,
                page: view.as_ref().map(|v| v.page),
                shape: view.and_then(|v| v.items.into_iter().next()),
                rank: r,
            });
        }
    }
    // Stable, so each rank keeps commands, types, elements in that order.
    hits.sort_by_key(|h| h.rank);
    hits.truncate(limit);
    hits
}

/// Which side of the source a new step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// To the right.
    Right,
    /// To the left.
    Left,
    /// Above.
    Up,
    /// Below.
    Down,
}

impl Direction {
    /// Every direction, clockwise from the top (the order the canvas handles are drawn in).
    pub const ALL: [Direction; 4] = [Direction::Up, Direction::Right, Direction::Down, Direction::Left];
    /// The direction with this name (`right`, `left`, `up`, `down`).
    pub fn parse(name: &str) -> Option<Direction> {
        match name {
            "right" => Some(Direction::Right),
            "left" => Some(Direction::Left),
            "up" => Some(Direction::Up),
            "down" => Some(Direction::Down),
            _ => None,
        }
    }
    /// The name `parse` reads.
    pub fn name(self) -> &'static str {
        match self {
            Direction::Right => "right",
            Direction::Left => "left",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
    /// The unit step in world coordinates (y grows downwards).
    pub fn unit(self) -> (f64, f64) {
        match self {
            Direction::Right => (1.0, 0.0),
            Direction::Left => (-1.0, 0.0),
            Direction::Up => (0.0, -1.0),
            Direction::Down => (0.0, 1.0),
        }
    }
}

/// The gap between a source and its next step: the layout's spacing between neighbouring nodes,
/// so a typed chain looks like what Re-Layout would draw.
pub fn layout_gap() -> f64 {
    vizcraft_diagram::autolayout::Options::default().node_spacing
}

/// How many steps along the direction placement tries before it jumps past everything.
const MAX_STEPS_ALONG: usize = 6;
/// How many sideways offsets (each way) placement tries at each step along.
const MAX_STEPS_ACROSS: usize = 8;

/// Whether `a` and `b` overlap with positive area once each grows by `pad`.
fn overlaps(a: &vizcraft_diagram::Rect, b: &vizcraft_diagram::Rect, pad: f64) -> bool {
    let (a, b) = (a.inflate(pad), b.inflate(pad));
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// Where a new `w` by `h` shape one step from shape `source` in `dir` goes on `page`: its
/// top-left corner. The step is the source's size plus [`layout_gap`]; a spot that overlaps
/// another shape, or the frame of a container the source is not in, is pushed sideways (across
/// the flow, alternating sides, as `layout.incremental` pushes a new unit) and then further
/// along, and past everything as a last resort. `None` when `source` is not on the page.
pub fn place(page: &Page, source: &str, w: f64, h: f64, dir: Direction) -> Option<(f64, f64)> {
    let src = page.shape(source)?;
    let b = src.bounds();
    let gap = layout_gap();
    let (w, h) = (
        vizcraft_diagram::geom::finite(w, 1.0).clamp(1.0, vizcraft_diagram::MAX_EXTENT),
        vizcraft_diagram::geom::finite(h, 1.0).clamp(1.0, vizcraft_diagram::MAX_EXTENT),
    );
    let (ux, uy) = dir.unit();
    let c = b.center();
    // Centre of the first spot: the source's half size, the gap and the new shape's half size.
    let along = if ux != 0.0 { b.width() / 2.0 + gap + w / 2.0 } else { b.height() / 2.0 + gap + h / 2.0 };
    let (cx, cy) = (c.x + ux * along, c.y + uy * along);
    // The source's own containers (and groups) hold it; every other shape is in the way.
    let holders = vizcraft_diagram::arrange::ancestors(page, source);
    let obstacles: Vec<vizcraft_diagram::Rect> =
        page.shapes.iter().filter(|s| !holders.contains(&s.id) && vizcraft_diagram::layer::visible(page, s)).map(Shape::bounds).collect();
    let (step_along, step_across) = if ux != 0.0 { (w + gap, (h + gap) / 2.0) } else { (h + gap, (w + gap) / 2.0) };
    let frame_at = |x: f64, y: f64| vizcraft_diagram::Rect::from_xywh(x - w / 2.0, y - h / 2.0, w, h);
    let clear = |r: &vizcraft_diagram::Rect| !obstacles.iter().any(|o| overlaps(r, o, gap / 4.0));
    for k in 0..MAX_STEPS_ALONG {
        let (bx, by) = (cx + ux * step_along * k as f64, cy + uy * step_along * k as f64);
        for j in 0..=(2 * MAX_STEPS_ACROSS) {
            // 0, +1, -1, +2, -2, ... half steps across the flow.
            let n = j.div_ceil(2) as f64 * if j % 2 == 1 { 1.0 } else { -1.0 };
            let (x, y) = if ux != 0.0 { (bx, by + n * step_across) } else { (bx + n * step_across, by) };
            let r = frame_at(x, y);
            if clear(&r) {
                return Some((r.x0.round(), r.y0.round()));
            }
        }
    }
    // Crowded: past every obstacle along the direction.
    let far = obstacles.iter().fold(None::<f64>, |m, o| {
        let edge = match dir {
            Direction::Right => o.x1,
            Direction::Left => -o.x0,
            Direction::Down => o.y1,
            Direction::Up => -o.y0,
        };
        Some(m.map_or(edge, |m| m.max(edge)))
    })?;
    let r = match dir {
        Direction::Right => vizcraft_diagram::Rect::from_xywh(far + gap, cy - h / 2.0, w, h),
        Direction::Left => vizcraft_diagram::Rect::from_xywh(-far - gap - w, cy - h / 2.0, w, h),
        Direction::Down => vizcraft_diagram::Rect::from_xywh(cx - w / 2.0, far + gap, w, h),
        Direction::Up => vizcraft_diagram::Rect::from_xywh(cx - w / 2.0, -far - gap - h, w, h),
    };
    Some((r.x0.round(), r.y0.round()))
}

/// The default size of a new shape of `type_id`: an element type's master size, or the
/// stencil's own. `None` when neither names a master.
pub fn size_of(type_id: &str) -> Option<(f64, f64)> {
    let master = vizcraft_notation::catalog().master_for_type(type_id).unwrap_or(type_id);
    vizcraft_stencils::master(master).map(|m| (m.w, m.h))
}
