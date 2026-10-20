//! The process part of the sync pass (`crate::bind::sync`, run last): pipe lines, live tags and
//! line labels for the `process` pack.
//!
//! After every change, in the same undo step:
//!
//! 1. **Line assignment.** A pipe segment (a connector of a pipe or fluid net) on a P&ID, HVAC,
//!    plumbing or isometric page names its line in `ext["process.line"]`. A segment without one
//!    takes the line of a segment on the other port of an inline component it is glued to (a
//!    valve, a fitting, an inline primary element), so a line number runs on through valves; else
//!    it gets a new line element (`process.PipingNetworkSystem`, or `process.DuctRun` for air) with
//!    the next number, so a branch starts a new sequence as PIP PIC001 asks. Splitting a segment
//!    copies its `ext`, so both halves keep the line.
//! 2. **Line removal.** A line element some segment named before the command and none names now is
//!    removed, as deleting the last connector of a relationship removes it.
//! 3. **Live tags.** A tag of a process identifier scope whose format reads properties or an
//!    owner's tag, and nothing of where it is drawn, is recomposed now, keeping its number: a
//!    line's size, a loop's number or an instrument's letters show in every tag built from them
//!    at once (tags with page, zone or rung parts keep the sheet's renumbering on request).
//! 4. **Line labels.** A line's tag is written on its longest segment on each page (or on every
//!    segment, or none, by the line's `label` property), once every property its format reads is
//!    filled in: as the relationship's name on a bound segment, as the connector's text otherwise.
//!
//! Running it twice changes nothing the second time; a document without process pages or
//! elements is left alone.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::{Document, End, Page};
use vizcraft_model::identity::{ComposeCtx, IdFormat};
use vizcraft_model::{Element, NetKind, PropValue, Registry};

use crate::Catalog;
use crate::infer::Facts;

/// `ext` key on a pipe segment: the id of its line element.
pub const EXT_LINE: &str = "process.line";
/// The line element type.
pub const LINE: &str = "process.PipingNetworkSystem";
/// The duct run element type (a line of air).
pub const DUCT: &str = "process.DuctRun";
/// Inline components: a line runs on through these.
pub const INLINE: &[&str] = &["process.PipingComponent", "process.ProcessSignalGeneratingFunction"];
/// The viewpoints whose pipes are numbered into lines.
pub const LINE_VIEWPOINTS: &[&str] = &["process.PID", "process.HVAC", "process.Plumbing", "process.Isometric"];
/// Connector types whose segments make duct runs rather than pipe lines.
pub const AIR_KINDS: &[&str] = &["process.supplyAir", "process.returnAir", "process.exhaustAir"];
/// Most passes the tag recomposition makes (an owner's new tag feeds its members').
const MAX_ROUNDS: usize = 6;

/// Run the process pass over `doc`; `before` is the document before the command.
pub fn run(doc: &mut Document, before: &Document, cat: &Catalog) {
    if cat.pack("process").is_none() {
        return;
    }
    let process_pages = doc.pages.iter().any(is_line_page);
    let process_elements = doc.model.elements.iter().any(|e| e.pack() == "process");
    if !process_pages && !process_elements && !before.model.elements.iter().any(|e| is_line_type(&e.type_id)) {
        return;
    }
    if process_pages {
        assign_lines(doc, cat);
    }
    drop_lines(doc, before);
    recompose(doc, before, cat);
    label_lines(doc, cat);
}

/// Whether `page` shows a viewpoint whose pipes are numbered into lines.
pub fn is_line_page(page: &Page) -> bool {
    page.view.as_ref().is_some_and(|v| LINE_VIEWPOINTS.contains(&v.viewpoint.as_str()))
}

/// Whether `type_id` is a line element type.
pub fn is_line_type(type_id: &str) -> bool {
    type_id == LINE || type_id == DUCT
}

/// The net kinds of connector types, by id, for the types that declare one.
fn kind_nets(cat: &Catalog) -> HashMap<&str, NetKind> {
    cat.packs().iter().flat_map(|p| p.connector_types.iter()).filter_map(|t| Some((t.id.as_str(), NetKind::from_name(t.net.as_deref()?)?))).collect()
}

/// Whether connector `c` is a pipe segment: its kind or its net carries a pipe or a fluid.
pub fn is_pipe(doc: &Document, kinds: &HashMap<&str, NetKind>, c: &vizcraft_diagram::Connector) -> bool {
    let by_kind = kinds.get(c.kind.as_str()).copied();
    let by_net = c.net.as_deref().and_then(|n| doc.model.net(n)).map(|n| n.kind);
    match by_kind.or(by_net) {
        Some(k) => matches!(k, NetKind::Pipe | NetKind::Fluid),
        None => false,
    }
}

/// The line a segment names, when that element exists and is a line.
fn line_of(doc: &Document, c: &vizcraft_diagram::Connector) -> Option<String> {
    let id = c.ext.get(EXT_LINE)?;
    doc.model.element(id).filter(|e| is_line_type(&e.type_id)).map(|e| e.id.clone())
}

/// Step 1: every pipe segment on a line page names a line.
fn assign_lines(doc: &mut Document, cat: &Catalog) {
    let kinds = kind_nets(cat);
    // The segments that need a line, by (page, connector index).
    let mut todo: Vec<(usize, usize)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        if !is_line_page(page) {
            continue;
        }
        for (ci, c) in page.connectors.iter().enumerate() {
            if is_pipe(doc, &kinds, c) && line_of(doc, c).is_none() {
                todo.push((pi, ci));
            }
        }
    }
    if todo.is_empty() {
        return;
    }
    // Inheritance through inline components, repeated while it reaches new segments (a run of
    // valves drawn in one command), at most once per segment.
    let inline: HashSet<String> = {
        let facts = Facts::new(&doc.model, cat);
        doc.model.elements.iter().filter(|e| INLINE.iter().any(|t| facts.is_a(&e.type_id, t))).map(|e| e.id.clone()).collect()
    };
    for _ in 0..=todo.len() {
        let mut found: Vec<(usize, usize, String)> = Vec::new();
        for &(pi, ci) in &todo {
            let Some(page) = doc.pages.get(pi) else { continue };
            let Some(c) = page.connectors.get(ci) else { continue };
            if line_of(doc, c).is_some() {
                continue;
            }
            if let Some(line) = inherited(doc, page, c, &inline, &kinds) {
                found.push((pi, ci, line));
            }
        }
        if found.is_empty() {
            break;
        }
        for (pi, ci, line) in found {
            if let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci)) {
                c.ext.insert(EXT_LINE.to_string(), line);
            }
        }
    }
    // The rest start lines of their own, in page and connector order.
    for (pi, ci) in todo {
        let Some(c) = doc.pages.get(pi).and_then(|p| p.connectors.get(ci)) else { continue };
        if line_of(doc, c).is_some() {
            continue;
        }
        let air = AIR_KINDS.contains(&c.kind.as_str());
        let seed = c.net.as_deref().and_then(|n| doc.model.net(n)).map(|n| n.properties.clone()).unwrap_or_default();
        let Some(line) = new_line(doc, cat, if air { DUCT } else { LINE }, &seed) else { continue };
        if let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci)) {
            c.ext.insert(EXT_LINE.to_string(), line);
        }
    }
}

/// The line of a segment glued to the other port of an inline component that `c` is glued to.
fn inherited(
    doc: &Document,
    page: &Page,
    c: &vizcraft_diagram::Connector,
    inline: &HashSet<String>,
    kinds: &HashMap<&str, NetKind>,
) -> Option<String> {
    for end in [&c.start, &c.end] {
        let End::Glued { shape, point, .. } = end else { continue };
        let Some(sh) = page.shape(shape) else { continue };
        if !sh.element.as_deref().is_some_and(|e| inline.contains(e)) || sh.connection_points().len() > 2 {
            continue;
        }
        for other in page.connectors.iter().filter(|o| o.id != c.id && is_pipe(doc, kinds, o)) {
            let touches = [&other.start, &other.end].into_iter().any(|e| match e {
                End::Glued { shape: s2, point: p2, .. } => s2 == shape && (point.is_none() || p2.is_none() || p2 != point),
                End::Free { .. } => false,
            });
            if touches && let Some(line) = line_of(doc, other) {
                return Some(line);
            }
        }
    }
    None
}

/// Add a new line element of `type_id` with the schema defaults, the pipe properties the net
/// carried (`size`, `service`, `class`) and the next tag; returns its id, or `None` when the model
/// is full.
fn new_line(doc: &mut Document, cat: &Catalog, type_id: &str, net_props: &std::collections::BTreeMap<String, PropValue>) -> Option<String> {
    let id = doc.next_id("e");
    let mut e = Element::new(id.clone(), type_id);
    if let Some((_, t)) = cat.element_type(type_id).or_else(|| cat.element_type(LINE)) {
        for p in &t.properties {
            if let Some(d) = &p.default {
                e.properties.insert(p.key.clone(), d.clone());
            }
        }
    }
    for (from, to) in [("size", "nominalDiameter"), ("service", "fluidCode"), ("class", "pipingClass")] {
        if let Some(v) = net_props.get(from) {
            e.properties.insert(to.to_string(), v.clone());
        }
    }
    let scopes = cat.scopes();
    e.tag = Registry::new(&doc.model, &scopes).next_tag(&e).ok().flatten().unwrap_or_default();
    doc.model.add_element(e).ok()?;
    if let Some(p) = cat.pack_ref("process") {
        doc.model.use_pack(p);
    }
    Some(id)
}

/// Step 2: a line whose last segment went in this command goes too.
fn drop_lines(doc: &mut Document, before: &Document) {
    let named =
        |d: &Document| -> HashSet<String> { d.pages.iter().flat_map(|p| p.connectors.iter()).filter_map(|c| c.ext.get(EXT_LINE).cloned()).collect() };
    let was = named(before);
    if was.is_empty() {
        return;
    }
    let now = named(doc);
    let gone: Vec<String> = was.into_iter().filter(|l| !now.contains(l) && doc.model.element(l).is_some_and(|e| is_line_type(&e.type_id))).collect();
    if !gone.is_empty() {
        let mut gone = gone;
        gone.sort();
        doc.model.remove(&gone);
    }
}

/// A scope format the pass keeps live: a process scope that reads properties or an owner, and
/// nothing of the drawing.
fn live_format(scope: &vizcraft_model::IdScope) -> Option<IdFormat> {
    if !scope.id.starts_with("process.") {
        return None;
    }
    let f = IdFormat::parse(&scope.format).ok()?;
    (!f.has_place() && (f.has_owner() || !f.property_keys().is_empty())).then_some(f)
}

/// The tag `e` should carry now, or `None` to leave it (no number to keep).
fn wanted(doc: &Document, before: &Document, f: &IdFormat, e: &Element, auto: bool, reg: &Registry<'_>) -> Option<String> {
    let none = ComposeCtx::default();
    let n = if f.has_number() {
        let old = before.model.element(&e.id).unwrap_or(e);
        let read = f
            .number_in_ctx(&e.tag, &before.model, old, &none)
            .or_else(|| f.number_in(&e.tag, old))
            .or_else(|| f.number_in_ctx(&e.tag, &doc.model, e, &none))
            .or_else(|| f.number_in(&e.tag, e));
        match read {
            Some(n) => n,
            None if e.tag.is_empty() && auto => return reg.next_tag(e).ok().flatten(),
            None => return None,
        }
    } else {
        0
    };
    let tag = f.compose_in(&doc.model, e, n, &none);
    // A tag built from nothing (no letters and no loop) is no tag.
    let blank = !f.has_number() && f.property_keys().iter().all(|k| e.prop(k).is_none_or(|v| v.display().trim().is_empty())) && {
        let empty_owner = f.compose(e, 0);
        tag == empty_owner
    };
    Some(if blank { String::new() } else { tag })
}

/// Step 3: recompose live tags, keeping their numbers.
fn recompose(doc: &mut Document, before: &Document, cat: &Catalog) {
    let scopes = cat.scopes();
    for _ in 0..MAX_ROUNDS {
        let changes: Vec<(usize, String)> = {
            let reg = Registry::new(&doc.model, &scopes);
            let formats: HashMap<&str, (IdFormat, bool)> =
                reg.effective().iter().filter_map(|s| live_format(s).map(|f| (s.id.as_str(), (f, s.auto_number)))).collect();
            if formats.is_empty() {
                return;
            }
            doc.model
                .elements
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    let scope = reg.scope_for(&e.type_id)?;
                    let (f, auto) = formats.get(scope.id.as_str())?;
                    let want = wanted(doc, before, f, e, *auto, &reg)?;
                    let want: String = cut(&want, vizcraft_model::MAX_TAG);
                    (want != e.tag).then_some((i, want))
                })
                .collect()
        };
        if changes.is_empty() {
            return;
        }
        for (i, tag) in changes {
            if let Some(e) = doc.model.elements.get_mut(i) {
                e.tag = tag;
            }
        }
    }
}

/// `t` cut to `max` bytes at a char boundary.
fn cut(t: &str, max: usize) -> String {
    if t.len() <= max {
        return t.to_string();
    }
    let mut end = max;
    while end > 0 && !t.is_char_boundary(end) {
        end -= 1;
    }
    t.get(..end).unwrap_or_default().to_string()
}

/// Whether every property the line scope's format reads is filled in on `line`.
fn complete(doc: &Document, cat: &Catalog, line: &Element) -> bool {
    let scopes = cat.scopes();
    let reg = Registry::new(&doc.model, &scopes);
    let Some(f) = reg.scope_for(&line.type_id).and_then(|s| IdFormat::parse(&s.format).ok()) else { return !line.tag.is_empty() };
    !line.tag.is_empty() && f.property_keys().iter().all(|k| line.prop(k).is_some_and(|v| !v.display().trim().is_empty()))
}

/// Step 4: write each line's tag on its segments.
fn label_lines(doc: &mut Document, cat: &Catalog) {
    let any = doc.pages.iter().any(|p| p.connectors.iter().any(|c| c.ext.contains_key(EXT_LINE)));
    if !any {
        return;
    }
    // The text each labelled segment should show, by page and connector index.
    let mut plan: Vec<(usize, usize, String)> = Vec::new();
    let texts: HashMap<String, (String, String)> = doc
        .model
        .elements
        .iter()
        .filter(|e| is_line_type(&e.type_id))
        .map(|e| {
            let mode = e.prop("label").and_then(PropValue::as_str).unwrap_or("once").to_string();
            let text = if complete(doc, cat, e) { e.tag.clone() } else { String::new() };
            (e.id.clone(), (text, mode))
        })
        .collect();
    for (pi, page) in doc.pages.iter().enumerate() {
        let mut by_line: HashMap<&str, Vec<(usize, f64)>> = HashMap::new();
        for (ci, c) in page.connectors.iter().enumerate() {
            let Some(l) = c.ext.get(EXT_LINE) else { continue };
            if !texts.contains_key(l) {
                continue;
            }
            let poly = vizcraft_diagram::route_polyline(page, c);
            let len: f64 = poly.windows(2).map(|w| w.first().zip(w.get(1)).map_or(0.0, |(a, b)| a.distance(*b))).sum();
            by_line.entry(l.as_str()).or_default().push((ci, if len.is_finite() { len } else { 0.0 }));
        }
        for (line, segs) in by_line {
            let Some((text, mode)) = texts.get(line) else { continue };
            // The longest segment, the first of equals.
            let longest = segs.iter().fold(None::<(usize, f64)>, |best, &(ci, len)| match best {
                Some((_, bl)) if bl >= len => best,
                _ => Some((ci, len)),
            });
            for &(ci, _) in &segs {
                let show = match mode.as_str() {
                    "off" => false,
                    "each" => true,
                    _ => longest.is_some_and(|(l, _)| l == ci),
                };
                plan.push((pi, ci, if show { text.clone() } else { String::new() }));
            }
        }
    }
    for (pi, ci, text) in plan {
        let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci)) else { continue };
        if c.text != text {
            c.text = text.clone();
        }
        if let Some(r) = c.relationship.clone()
            && let Some(rel) = doc.model.relationship_mut(&r)
            && rel.name != text
        {
            rel.name = text;
        }
    }
}
