//! The sync pass: keeps the model and its views in step after every document change.
//!
//! The engine calls [`sync`] after each command that changed the document, inside the same undo
//! step, before connector rerouting. It only touches items that are already bound or that join
//! bound items, so free drawing (and every Cascade 1 to 3 document) is left exactly as it was.
//!
//! Running it twice with the same `before` changes nothing the second time. Lookups are indexed
//! once per step, so the pass is linear in the document except for the few new ids it hands out.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::{Color, Connector, Document, End, Style};
use vizcraft_model::{Element, PropValue, Registry, Relationship};

use crate::schema::ConditionalStyle;
use crate::{Catalog, legal};

/// Bring views and model into agreement after a command turned `before` into `doc`, in this order:
///
/// 0. The net pass ([`crate::bind_nets::nets`]) keeps nets, wire segments and junctions
///    consistent; it does nothing in a document without wiring.
/// 1. A shape whose `element` is missing from the model, and a connector whose `relationship` is
///    missing, become free drawing (the reference is cleared).
/// 2. A second shape for the same element on one page is a copy: it gets a new element (fresh id
///    from `Document::next_id("e")`, same type and properties, name kept, tag re-issued by the
///    identity registry when the scope numbers automatically, else cleared).
/// 3. A connector glued at both ends to bound shapes gets a relationship when it has none: type
///    from the relationship its connector `kind` names, if it names one, else from
///    [`crate::legal::relationship_for`] (an illegal pair still binds, so the rules engine
///    can report it), id from `next_id("r")`, the connector's text as its name. A bound
///    connector's relationship follows its glue (source and target from the glued shapes'
///    elements, ports from glued connection-point indexes when the type declares ports); a bound
///    connector with a free end keeps its relationship as it was.
/// 4. A relationship that some connector drew in `before` and no connector draws now is removed
///    from the model: deleting the last connector view deletes the relationship. Relationships
///    that never had a view (made by `relationship.create` between elements not on one page) stay,
///    and so does a relationship whose source or target differs from `before` (reconnected by this
///    command, which may drop views on pages that do not draw the new end).
/// 5. Every bound shape's text becomes its element's name, and every bound connector's text its
///    relationship's name.
/// 6. Style fields named by conditional styles (see `ConditionalStyle`) are set on bound shapes
///    when the element's match changes, or once on a new element or view.
/// 7. Packs of every element and relationship type in use are recorded in `doc.model.packs`.
/// 8. A bound shape whose element type declares compartments (`ElementType::compartments`)
///    gets each one it lacks (appended in declaration order), and a declared list's rows (a
///    paragraph's lines) become the element's named properties as `name: value`, or with
///    `rowsFrom`, the lines of that property read as member rows and written in the page
///    variant's member style.
/// 9. A bound connector whose relationship type declares `endLabels` shows those properties
///    beside its ends; an end label the command changed writes back to its properties first.
/// 10. A bound shape whose `parent` changed (or that is newly bound), of a type that declares
///     `allowedParents`, takes as owner the element of the container shape it now sits in when
///     that type is allowed, and loses the owner it had from a container it left for the top
///     ([`crate::nesting::owners`]).
///
/// 11. The process pass ([`crate::process_pass`]) runs last: pipe segments on process pages are
///     numbered into lines, live process tags are recomposed and line numbers are written on
///     their segments. It runs on a model without elements too, since a pipe alone starts a line.
///
/// Between steps 6 and 7, the end pass ([`crate::end_pass`]) draws the ends of connectors whose
/// kind reads them from the relationship's properties, and the satellite pass
/// ([`crate::satellites`]) draws owned elements around their owner where the page's variant asks
/// for it. Step 3 makes no relationship of a satellite's line.
///
/// Details of each step:
///
/// - Step 2 keeps the element on the first shape (z order) that already drew it in `before`, else
///   on the first shape; the other shapes on that page get copies. A copy that cannot be added
///   (the model is full) leaves its shape as free drawing.
/// - Step 3: when several connectors draw one relationship, the first whose glued elements equal
///   the relationship's ends keeps it (else the first glued at both ends moves it); a connector
///   glued between other elements gets a copy of the relationship with a fresh id. Ports: when an
///   end's element type declares ports, a connector glued at connection point `i` sets that end's
///   port to the id of port `i` (dynamic glue or an index past the ports: none); a type with no
///   ports clears the port.
/// - Step 5 decides between text and name per element (and relationship): a leading `«...»`
///   line is taken off the text first, and the view shows the type's `keyword` (or the
///   element's `stereotype` property) as `«keyword»` on a line above the name. A name changed
///   by the command wins; else a view whose text the command changed gives the name; else a new item
///   with no name takes its first non-empty view text; else the name stays. Every view then shows
///   the name.
/// - Step 6 changes style only when a match changes, so hand styling wins otherwise. Per style
///   field (fill, line, text colour, line width), the first matching conditional style of the
///   element's type that names the field is compared between `before` and `doc`. For a shape that
///   already drew an element that already existed, a field whose match changed takes the new
///   style's value, or the glyph's value when nothing matches now (fill and line from the type's
///   glyph, text colour and line width from the default style); unchanged fields are left alone.
///   A new element or a newly bound shape gets the fields that match now, and keeps its glyph
///   style where nothing matches. A style matches when the element's property (or the property's
///   schema default) equals `equals`; a style with no `equals` matches when the property has any
///   value.
/// - Step 8: the name is the property schema's label (its key when it has none) and the value
///   the element's, else the schema default; a property with neither is left out. Rows change
///   only when the text differs, so the pass stays idempotent; the shape grows to fit when its
///   content no longer does. A declaration with no `fromProperties` only ensures the compartment
///   exists, and the user's own compartments are left alone. A `rowsFrom` declaration reads its
///   property through `crate::uml_text::parse_member` and writes rows with `format_member` in
///   the style of the page variant's `memberVisibility` option (`symbols` by default, `keywords`
///   or `hidden`); the option `members: suppressed` leaves the rows empty.
/// - Step 9 joins the values of the `endLabels` keys (else their schema defaults) with a space.
///   A connector end label that differs from `before` while the relationship's properties for
///   that end did not change writes back: the last word to the last key, and so on, the first
///   key taking the rest; an empty part removes the property.
pub fn sync(doc: &mut Document, before: &Document, cat: &Catalog) {
    vizcraft_diagram::views::sanitize_refs(doc);
    // Wiring needs no elements, so the net pass runs before the early return below.
    crate::bind_nets::nets(doc, before, cat);
    conformance(doc, cat);
    // Nothing can be bound in a model with no items, and nothing drawn in `before` can need
    // removing: free drawing leaves here untouched.
    if doc.model.elements.is_empty() && doc.model.relationships.is_empty() {
        // Pipes on a process page are numbered into lines before anything is bound.
        crate::process_pass::run(doc, before, cat);
        return;
    }
    copies(doc, before, cat);
    connectors(doc, cat);
    drop_unviewed(doc, before);
    names(doc, before, cat);
    styles(doc, before, cat);
    crate::end_pass::ends(doc, cat);
    crate::satellites::satellites(doc, cat);
    record_packs(doc, cat);
    compartments(doc, cat);
    end_labels(doc, before, cat);
    crate::sheet_pass::run(doc, cat);
    crate::nesting::owners(doc, before, cat);
    crate::process_pass::run(doc, before, cat);
}

/// Element index by id; the model is appended to only during the pass, so indexes stay valid.
/// Turn the conformance profile on when the document starts using a notation with rules (a
/// page gets a viewpoint, the model gains an element of a pack); see `rules::auto_enable`.
fn conformance(doc: &mut Document, cat: &Catalog) {
    crate::rules::auto_enable(doc, cat);
}

fn element_index(doc: &Document) -> HashMap<String, usize> {
    let mut m = HashMap::with_capacity(doc.model.elements.len());
    for (i, e) in doc.model.elements.iter().enumerate() {
        m.entry(e.id.clone()).or_insert(i);
    }
    m
}

/// Step 2: a second shape of one element on one page gets a copy of the element.
fn copies(doc: &mut Document, before: &Document, cat: &Catalog) {
    let before_el: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.shapes.iter()).filter_map(|s| s.element.as_deref().map(|e| (s.id.as_str(), e))).collect();
    let mut plan: Vec<(usize, usize, String)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        let mut first: HashMap<&str, usize> = HashMap::new();
        let mut prior: HashMap<&str, usize> = HashMap::new();
        for (si, s) in page.shapes.iter().enumerate() {
            let Some(e) = s.element.as_deref() else { continue };
            first.entry(e).or_insert(si);
            if before_el.get(s.id.as_str()) == Some(&e) {
                prior.entry(e).or_insert(si);
            }
        }
        for (si, s) in page.shapes.iter().enumerate() {
            let Some(e) = s.element.as_deref() else { continue };
            let keep = prior.get(e).or_else(|| first.get(e)).copied();
            if keep != Some(si) {
                plan.push((pi, si, e.to_string()));
            }
        }
    }
    if plan.is_empty() {
        return;
    }
    let index = element_index(doc);
    let scopes = cat.scopes();
    for (pi, si, e) in plan {
        let Some(src) = index.get(&e).and_then(|&i| doc.model.elements.get(i)).cloned() else { continue };
        let id = doc.next_id("e");
        let mut copy = Element { id: id.clone(), tag: String::new(), ..src };
        copy.tag = Registry::new(&doc.model, &scopes).next_tag(&copy).ok().flatten().unwrap_or_default();
        let bound = doc.model.add_element(copy).is_ok().then_some(id);
        if let Some(shape) = doc.pages.get_mut(pi).and_then(|p| p.shapes.get_mut(si)) {
            shape.element = bound;
        }
    }
}

/// Where a connector glued at both ends to bound shapes attaches.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Glue {
    source: String,
    target: String,
    source_point: Option<usize>,
    target_point: Option<usize>,
    /// The port ids a glyph builder gave the glued shapes' connection points, which win over
    /// the types' ports.
    source_glyph: Option<String>,
    target_glyph: Option<String>,
}

fn glue_of(shape_el: &HashMap<&str, &str>, glyph_ports: &HashMap<&str, &[String]>, c: &Connector) -> Option<Glue> {
    let (End::Glued { shape: s, point: sp, .. }, End::Glued { shape: t, point: tp, .. }) = (&c.start, &c.end) else { return None };
    let built = |shape: &str, point: Option<usize>| glyph_ports.get(shape).and_then(|ports| ports.get(point?)).cloned();
    Some(Glue {
        source: (*shape_el.get(s.as_str())?).to_string(),
        target: (*shape_el.get(t.as_str())?).to_string(),
        source_point: *sp,
        target_point: *tp,
        source_glyph: built(s, *sp),
        target_glyph: built(t, *tp),
    })
}

/// The port an end attaches to: the glyph builder's port id at the glued point when the shape
/// has one, else by connection-point index when the element's type declares ports.
fn port(
    doc: &Document,
    index: &HashMap<String, usize>,
    cat: &Catalog,
    element: &str,
    point: Option<usize>,
    built: &Option<String>,
) -> Option<String> {
    if built.is_some() {
        return built.clone();
    }
    let e = index.get(element).and_then(|&i| doc.model.elements.get(i))?;
    let (_, t) = cat.element_type(&e.type_id)?;
    t.ports.get(point?).map(|p| p.id.clone())
}

/// What step 3 does to one connector.
enum Bind {
    /// Move the relationship's ends to this glue.
    Follow(String, Glue),
    /// Give the connector a copy of this relationship, glued here.
    Copy(usize, usize, String, Glue),
    /// Give the connector a new relationship.
    New(usize, usize, Glue),
}

/// Step 3: connectors glued between bound shapes get relationships; bound ones follow glue.
fn connectors(doc: &mut Document, cat: &Catalog) {
    let mut plan: Vec<Bind> = Vec::new();
    {
        let mut rels: HashMap<&str, &Relationship> = HashMap::with_capacity(doc.model.relationships.len());
        for r in &doc.model.relationships {
            rels.entry(r.id.as_str()).or_insert(r);
        }
        // Glued connectors per relationship, in document order.
        let mut by_rel: HashMap<&str, Vec<(usize, usize, Glue)>> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (pi, page) in doc.pages.iter().enumerate() {
            let shape_el: HashMap<&str, &str> = page.shapes.iter().filter_map(|s| s.element.as_deref().map(|e| (s.id.as_str(), e))).collect();
            if shape_el.is_empty() {
                continue;
            }
            let glyph_ports: HashMap<&str, &[String]> = page
                .shapes
                .iter()
                .filter_map(|s| s.glyph.as_ref().filter(|g| !g.ports.is_empty()).map(|g| (s.id.as_str(), g.ports.as_slice())))
                .collect();
            for (ci, c) in page.connectors.iter().enumerate() {
                // A satellite's line draws ownership, not a relationship.
                if c.ext.contains_key(crate::satellites::EXT_SATELLITE_OF) {
                    continue;
                }
                let Some(g) = glue_of(&shape_el, &glyph_ports, c) else { continue };
                match c.relationship.as_deref() {
                    Some(r) if rels.contains_key(r) => {
                        let list = by_rel.entry(r).or_default();
                        if list.is_empty() {
                            order.push(r);
                        }
                        list.push((pi, ci, g));
                    }
                    Some(_) => {}
                    None => plan.push(Bind::New(pi, ci, g)),
                }
            }
        }
        for r in order {
            let Some(rel) = rels.get(r) else { continue };
            let Some(list) = by_rel.get(r) else { continue };
            let winner = list.iter().find(|(_, _, g)| g.source == rel.source && g.target == rel.target).or_else(|| list.first());
            let Some((wp, wc, wg)) = winner else { continue };
            plan.push(Bind::Follow(r.to_string(), wg.clone()));
            for (pi, ci, g) in list {
                if (pi, ci) != (wp, wc) && (g.source != wg.source || g.target != wg.target) {
                    plan.push(Bind::Copy(*pi, *ci, r.to_string(), g.clone()));
                }
            }
        }
    }
    if plan.is_empty() {
        return;
    }
    let index = element_index(doc);
    let rel_index: HashMap<String, usize> = doc.model.relationships.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect();
    for step in plan {
        match step {
            Bind::Follow(r, g) => {
                let sp = port(doc, &index, cat, &g.source, g.source_point, &g.source_glyph);
                let tp = port(doc, &index, cat, &g.target, g.target_point, &g.target_glyph);
                if let Some(rel) = rel_index.get(&r).and_then(|&i| doc.model.relationships.get_mut(i)) {
                    set_if_changed(&mut rel.source, g.source);
                    set_if_changed(&mut rel.target, g.target);
                    set_if_changed(&mut rel.source_port, sp);
                    set_if_changed(&mut rel.target_port, tp);
                }
            }
            Bind::Copy(pi, ci, r, g) => {
                let Some(template) = rel_index.get(&r).and_then(|&i| doc.model.relationships.get(i)).cloned() else { continue };
                let rel = Relationship {
                    source_port: port(doc, &index, cat, &g.source, g.source_point, &g.source_glyph),
                    target_port: port(doc, &index, cat, &g.target, g.target_point, &g.target_glyph),
                    source: g.source,
                    target: g.target,
                    ..template
                };
                add_relationship(doc, pi, ci, rel);
            }
            Bind::New(pi, ci, g) => {
                let type_of = |id: &str| index.get(id).and_then(|&i| doc.model.elements.get(i)).map(|e| e.type_id.clone());
                let (Some(st), Some(tt)) = (type_of(&g.source), type_of(&g.target)) else { continue };
                let connector = doc.pages.get(pi).and_then(|p| p.connectors.get(ci));
                let kind_type = connector.and_then(|c| kind_relationship(cat, &c.kind));
                let Some(type_id) = kind_type.or_else(|| legal::relationship_for(cat, &st, &tt).map(|(t, _)| t)) else { continue };
                let name = connector.map(|c| c.text.clone()).unwrap_or_default();
                let rel = Relationship {
                    type_id,
                    source_port: port(doc, &index, cat, &g.source, g.source_point, &g.source_glyph),
                    target_port: port(doc, &index, cat, &g.target, g.target_point, &g.target_glyph),
                    source: g.source,
                    target: g.target,
                    name: cut_text(name),
                    ..Relationship::default()
                };
                add_relationship(doc, pi, ci, rel);
            }
        }
    }
}

/// The relationship type the connector kind `kind` names, when the kind exists, names one, and the
/// catalog knows it. It wins over the connection table; the pair it joins may still be illegal,
/// and then the rules engine reports it.
fn kind_relationship(cat: &Catalog, kind: &str) -> Option<String> {
    if kind.is_empty() {
        return None;
    }
    let rel = cat.packs().iter().find_map(|p| p.connector_type(kind))?.relationship_type.as_ref()?;
    cat.relationship_type(rel).map(|_| rel.clone())
}

/// Add `rel` under a fresh id and bind connector `ci` of page `pi` to it; when the model refuses
/// it (full), the connector stays as it was.
fn add_relationship(doc: &mut Document, pi: usize, ci: usize, mut rel: Relationship) {
    let id = doc.next_id("r");
    rel.id = id.clone();
    if doc.model.add_relationship(rel).is_ok()
        && let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci))
    {
        c.relationship = Some(id);
    }
}

fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

/// Step 4: relationships whose last connector view went with this command.
fn drop_unviewed(doc: &mut Document, before: &Document) {
    let drawn_before: HashSet<&str> = before.pages.iter().flat_map(|p| p.connectors.iter()).filter_map(|c| c.relationship.as_deref()).collect();
    if drawn_before.is_empty() {
        return;
    }
    let drawn_now: HashSet<&str> = doc.pages.iter().flat_map(|p| p.connectors.iter()).filter_map(|c| c.relationship.as_deref()).collect();
    let ends_before: HashMap<&str, (&str, &str)> =
        before.model.relationships.iter().map(|r| (r.id.as_str(), (r.source.as_str(), r.target.as_str()))).collect();
    // A relationship reconnected by this command lost its old views on purpose; it is not an
    // orphan, so only one whose ends are unchanged goes with its last connector.
    let unchanged = |r: &Relationship| ends_before.get(r.id.as_str()).is_some_and(|&(s, t)| s == r.source && t == r.target);
    doc.model.relationships.retain(|r| drawn_now.contains(r.id.as_str()) || !drawn_before.contains(r.id.as_str()) || !unchanged(r));
}

/// A name or text cut to the model's text limit at a character boundary.
fn cut_text(mut t: String) -> String {
    if t.len() > vizcraft_model::MAX_TEXT {
        let mut end = vizcraft_model::MAX_TEXT;
        while end > 0 && !t.is_char_boundary(end) {
            end -= 1;
        }
        t.truncate(end);
    }
    t
}

/// The name an item and its views agree on (see step 5 in [`sync`]).
fn agreed_name(name: &str, before_name: Option<&str>, views: &[(&str, &str)], before_text: &HashMap<&str, &str>) -> String {
    if before_name.is_some_and(|b| b != name) {
        return name.to_string();
    }
    if let Some((_, t)) = views.iter().find(|(id, t)| before_text.get(id).is_some_and(|b| b != t)) {
        return (*t).to_string();
    }
    if before_name.is_none()
        && name.is_empty()
        && let Some((_, t)) = views.iter().find(|(_, t)| !t.is_empty())
    {
        return (*t).to_string();
    }
    name.to_string()
}

/// A view text without a leading `«keyword»` line (the whole text when that is all it holds).
pub fn strip_keyword(text: &str) -> &str {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let first = first.trim_end_matches('\r');
    if first.starts_with('\u{ab}') && first.ends_with('\u{bb}') { rest } else { text }
}

/// What a view shows for `name` under `keyword`: `«keyword»` above the name.
fn with_keyword(name: &str, keyword: Option<&String>) -> String {
    match keyword {
        Some(k) if name.is_empty() => format!("\u{ab}{k}\u{bb}"),
        Some(k) => format!("\u{ab}{k}\u{bb}\n{name}"),
        None => name.to_string(),
    }
}

/// The keyword an element's views show: its `stereotype` property, else its type's keyword.
fn element_keyword(cat: &Catalog, e: &Element) -> Option<String> {
    if let Some(PropValue::Text(t)) = e.prop("stereotype")
        && !t.trim().is_empty()
    {
        return Some(t.trim().to_string());
    }
    cat.element_type(&e.type_id).map(|(_, t)| t.keyword.clone()).filter(|k| !k.is_empty())
}

/// Step 5: names and view texts agree.
fn names(doc: &mut Document, before: &Document, cat: &Catalog) {
    let before_shape_text: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.shapes.iter()).map(|s| (s.id.as_str(), strip_keyword(&s.text))).collect();
    let before_conn_text: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.connectors.iter()).map(|c| (c.id.as_str(), strip_keyword(&c.text))).collect();
    let before_el: HashMap<&str, &str> = before.model.elements.iter().map(|e| (e.id.as_str(), e.name.as_str())).collect();
    let before_rel: HashMap<&str, &str> = before.model.relationships.iter().map(|r| (r.id.as_str(), r.name.as_str())).collect();

    // Agreed names, computed with shared borrows, then written.
    let mut el_names: HashMap<String, String> = HashMap::new();
    let mut rel_names: HashMap<String, String> = HashMap::new();
    {
        let mut shape_views: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        let mut conn_views: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for page in &doc.pages {
            for s in &page.shapes {
                if let Some(e) = s.element.as_deref() {
                    shape_views.entry(e).or_default().push((s.id.as_str(), strip_keyword(&s.text)));
                }
            }
            for c in &page.connectors {
                if let Some(r) = c.relationship.as_deref() {
                    conn_views.entry(r).or_default().push((c.id.as_str(), strip_keyword(&c.text)));
                }
            }
        }
        for e in &doc.model.elements {
            if let Some(v) = shape_views.get(e.id.as_str()) {
                el_names.insert(e.id.clone(), agreed_name(&e.name, before_el.get(e.id.as_str()).copied(), v, &before_shape_text));
            }
        }
        for r in &doc.model.relationships {
            if let Some(v) = conn_views.get(r.id.as_str()) {
                rel_names.insert(r.id.clone(), agreed_name(&r.name, before_rel.get(r.id.as_str()).copied(), v, &before_conn_text));
            }
        }
    }
    if el_names.is_empty() && rel_names.is_empty() {
        return;
    }
    let el_kw: HashMap<String, String> = doc
        .model
        .elements
        .iter()
        .filter(|e| el_names.contains_key(&e.id))
        .filter_map(|e| element_keyword(cat, e).map(|k| (e.id.clone(), k)))
        .collect();
    let rel_kw: HashMap<String, String> = doc
        .model
        .relationships
        .iter()
        .filter(|r| rel_names.contains_key(&r.id))
        .filter_map(|r| cat.relationship_type(&r.type_id).map(|(_, t)| t.keyword.clone()).filter(|k| !k.is_empty()).map(|k| (r.id.clone(), k)))
        .collect();
    for e in &mut doc.model.elements {
        if let Some(n) = el_names.get(&e.id)
            && e.name != *n
        {
            e.name = cut_text(n.clone());
        }
    }
    for r in &mut doc.model.relationships {
        if let Some(n) = rel_names.get(&r.id)
            && r.name != *n
        {
            r.name = cut_text(n.clone());
        }
    }
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            if let Some(e) = s.element.as_deref()
                && let Some(n) = el_names.get(e)
            {
                let shown = with_keyword(&cut_text(n.clone()), el_kw.get(e));
                if s.text != shown {
                    s.text = shown;
                }
            }
        }
        for c in &mut page.connectors {
            if let Some(r) = c.relationship.as_deref()
                && let Some(n) = rel_names.get(r)
            {
                let shown = with_keyword(&cut_text(n.clone()), rel_kw.get(r));
                if c.text != shown {
                    c.text = shown;
                }
            }
        }
    }
}

/// Style field values to set on a shape: `None` leaves the field alone.
#[derive(Clone, Debug, Default, PartialEq)]
struct Fields {
    fill: Option<Color>,
    line: Option<Color>,
    font_color: Option<Color>,
    line_width: Option<f64>,
    italic: Option<bool>,
}

impl Fields {
    fn is_empty(&self) -> bool {
        *self == Fields::default()
    }
}

/// A conditional style by position: (pack index in the catalog, index in its list).
type StyleId = (usize, usize);

/// Per style field, the first matching conditional style that names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Matches {
    fill: Option<StyleId>,
    line: Option<StyleId>,
    font_color: Option<StyleId>,
    line_width: Option<StyleId>,
    italic: Option<StyleId>,
}

/// Whether `cs` matches element `e` (the property's schema default stands in for a missing value).
fn matches(cs: &ConditionalStyle, e: &Element, default: Option<&PropValue>) -> bool {
    let v = e.prop(&cs.property).or(default);
    match (&cs.equals, v) {
        (None, v) => v.is_some(),
        (Some(_), None) => false,
        (Some(a), Some(b)) => a == b,
    }
}

/// The styles matching `e` now, field by field.
fn matches_for(cat: &Catalog, e: &Element) -> Matches {
    let mut m = Matches::default();
    let Some((_, t)) = cat.element_type(&e.type_id) else { return m };
    for (pi, pack) in cat.packs().iter().enumerate() {
        for (ci, cs) in pack.conditional_styles.iter().enumerate() {
            if !pack.lists(&cs.types, &e.type_id) || !matches(cs, e, t.property(&cs.property).and_then(|p| p.default.as_ref())) {
                continue;
            }
            let id = Some((pi, ci));
            if m.fill.is_none() && cs.fill.is_some() {
                m.fill = id;
            }
            if m.line.is_none() && cs.line.is_some() {
                m.line = id;
            }
            if m.font_color.is_none() && cs.font_color.is_some() {
                m.font_color = id;
            }
            if m.line_width.is_none() && cs.line_width.is_some() {
                m.line_width = id;
            }
            if m.italic.is_none() && cs.italic.is_some() {
                m.italic = id;
            }
        }
    }
    m
}

fn style_at(cat: &Catalog, id: StyleId) -> Option<&ConditionalStyle> {
    cat.packs().get(id.0).and_then(|p| p.conditional_styles.get(id.1))
}

/// The values a shape of `e` gets. `changed_only`: only fields whose match differs from `was`,
/// set to the new match's value or, when nothing matches now, the glyph's value. Otherwise the
/// fields that match now, with their values. A colour that does not parse leaves its field alone.
fn fields_for(cat: &Catalog, e: &Element, now: Matches, was: Option<Matches>) -> Fields {
    let glyph = cat.element_type(&e.type_id).and_then(|(_, t)| cat.glyph(&t.glyph)).map(|(_, g)| g);
    let d = Style::DEFAULT;
    let glyph_fill = glyph.and_then(|g| Color::parse(&g.fill)).unwrap_or(d.fill);
    let glyph_line = glyph.and_then(|g| Color::parse(&g.line)).unwrap_or(d.line);
    let pick = |now: Option<StyleId>, was: Option<Option<StyleId>>| -> Option<Option<StyleId>> {
        match was {
            // Changed match: the new style, or none (the glyph value).
            Some(w) if w != now => Some(now),
            Some(_) => None,
            // New element or view: only a current match is applied.
            None => now.map(Some),
        }
    };
    let colour = |sel: Option<Option<StyleId>>, get: fn(&ConditionalStyle) -> Option<&String>, glyph_value: Color| -> Option<Color> {
        match sel? {
            Some(id) => style_at(cat, id).and_then(get).and_then(|t| Color::parse(t)),
            None => Some(glyph_value),
        }
    };
    let width = match pick(now.line_width, was.map(|w| w.line_width)) {
        None => None,
        Some(None) => Some(d.line_width),
        Some(Some(id)) => style_at(cat, id).and_then(|cs| cs.line_width).filter(|w| w.is_finite()).map(|w| w.clamp(0.0, Style::MAX_LINE_WIDTH)),
    };
    Fields {
        fill: colour(pick(now.fill, was.map(|w| w.fill)), |cs| cs.fill.as_ref(), glyph_fill),
        line: colour(pick(now.line, was.map(|w| w.line)), |cs| cs.line.as_ref(), glyph_line),
        font_color: colour(pick(now.font_color, was.map(|w| w.font_color)), |cs| cs.font_color.as_ref(), d.font_color),
        line_width: width,
        italic: match pick(now.italic, was.map(|w| w.italic)) {
            None => None,
            Some(None) => Some(d.italic),
            Some(Some(id)) => style_at(cat, id).and_then(|cs| cs.italic),
        },
    }
}

/// Step 6: conditional styles on bound shapes, applied only when an element's match changes (or
/// on a new element or view), so hand styling made with `shape.setStyle` sticks.
fn styles(doc: &mut Document, before: &Document, cat: &Catalog) {
    if cat.packs().iter().all(|p| p.conditional_styles.is_empty()) {
        return;
    }
    let before_el: HashMap<&str, &Element> = before.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let before_shapes: HashSet<&str> = before.pages.iter().flat_map(|p| p.shapes.iter()).map(|s| s.id.as_str()).collect();
    // Per element: what a shape that already drew it gets (changes only), and what a new view gets.
    let mut plans: HashMap<String, (Fields, Fields)> = HashMap::new();
    for e in &doc.model.elements {
        let now = matches_for(cat, e);
        let fresh = fields_for(cat, e, now, None);
        let changed = match before_el.get(e.id.as_str()) {
            Some(b) => fields_for(cat, e, now, Some(matches_for(cat, b))),
            // A new element: every view of it is new.
            None => fresh.clone(),
        };
        if !(changed.is_empty() && fresh.is_empty()) {
            plans.entry(e.id.clone()).or_insert((changed, fresh));
        }
    }
    if plans.is_empty() {
        return;
    }
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            let Some((changed, fresh)) = s.element.as_deref().and_then(|e| plans.get(e)) else { continue };
            let f = if before_shapes.contains(s.id.as_str()) { changed } else { fresh };
            if let Some(v) = f.fill {
                set_if_changed(&mut s.style.fill, v);
            }
            if let Some(v) = f.line {
                set_if_changed(&mut s.style.line, v);
            }
            if let Some(v) = f.font_color {
                set_if_changed(&mut s.style.font_color, v);
            }
            if let Some(v) = f.line_width {
                set_if_changed(&mut s.style.line_width, v);
            }
            if let Some(v) = f.italic {
                set_if_changed(&mut s.style.italic, v);
            }
        }
    }
}

/// Step 8: declared compartments on bound shapes, filled from the element's properties. The
/// declarations are those of the page's rendering variant (`Catalog::compartments_in`); a
/// compartment another declaration of the type has but this one does not is removed, and the
/// user's own compartments are left alone.
fn compartments(doc: &mut Document, cat: &Catalog) {
    use vizcraft_diagram::compartment::{self as cp, Compartment, CompartmentKind};
    if cat.packs().iter().all(|p| p.element_types.iter().all(|t| t.compartments.is_empty()) && p.variants.iter().all(|v| v.compartments.is_empty())) {
        return;
    }
    let index = element_index(doc);
    let model = &doc.model;
    for page in &mut doc.pages {
        let variant = page.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
        let options = cat.variant(&variant).map(|(_, v)| &v.options);
        let option = |k: &str| options.and_then(|o| o.get(k)).map(String::as_str).unwrap_or_default();
        let shown = crate::uml_text::Shown::from_option(option("memberVisibility"));
        let suppressed = option("members") == "suppressed";
        for s in &mut page.shapes {
            let Some(e) = s.element.as_deref().and_then(|el| index.get(el)).and_then(|&i| model.elements.get(i)) else { continue };
            let Some((_, t)) = cat.element_type(&e.type_id) else { continue };
            let decls = cat.compartments_in(&e.type_id, &variant);
            if decls.is_empty() && !s.compartments.iter().any(|c| cat.declares_compartment(&e.type_id, &c.id)) {
                continue;
            }
            // A shape that loses its compartments keeps its size: `variant.set` gives it the
            // variant's master size already.
            s.compartments.retain(|c| decls.iter().any(|d| d.id == c.id) || !cat.declares_compartment(&e.type_id, &c.id));
            for d in decls.iter().filter(|d| cp::is_valid_id(&d.id)).take(cp::MAX_COMPARTMENTS) {
                let lines: Option<Vec<String>> = if !d.rows_from.is_empty() {
                    let text = match e.properties.get(&d.rows_from).or_else(|| t.property(&d.rows_from).and_then(|p| p.default.as_ref())) {
                        Some(PropValue::Text(x)) => x.as_str(),
                        _ => "",
                    };
                    Some(if suppressed {
                        Vec::new()
                    } else {
                        crate::uml_text::rows(text)
                            .take(cp::MAX_ROWS)
                            .map(crate::uml_text::parse_member)
                            .map(|m| crate::uml_text::format_member(&m, shown))
                            .collect()
                    })
                } else if !d.from_properties.is_empty() {
                    Some(
                        d.from_properties
                            .iter()
                            .take(cp::MAX_ROWS)
                            .filter_map(|key| {
                                let schema = t.property(key);
                                let value = e.properties.get(key).or_else(|| schema.and_then(|p| p.default.as_ref()))?;
                                let name = schema.map(|p| p.label.trim()).filter(|l| !l.is_empty()).unwrap_or(key);
                                Some(format!("{name}: {}", value.display()))
                            })
                            .collect(),
                    )
                } else {
                    None
                };
                if !s.compartments.iter().any(|c| c.id == d.id) {
                    if s.compartments.len() >= cp::MAX_COMPARTMENTS {
                        continue;
                    }
                    let mut c = Compartment::new(d.id.clone(), d.kind);
                    c.title = d.title.clone();
                    s.compartments.push(c);
                }
                let (Some(lines), Some(c)) = (lines, s.compartments.iter_mut().find(|c| c.id == d.id)) else { continue };
                match c.kind {
                    CompartmentKind::List => set_if_changed(&mut c.rows, lines),
                    CompartmentKind::Paragraph => set_if_changed(&mut c.text, lines.join("\n")),
                    CompartmentKind::Slot => {}
                }
                c.sanitize();
            }
            let centred = decls.iter().any(|d| d.query.as_ref().is_some_and(|q| q.owned));
            crate::sheet_pass::grow_to_fit(s, centred);
        }
    }
}

/// The label an end shows: the values of `keys` (else their schema defaults) joined by a space.
fn end_label(r: &Relationship, keys: &[String], t: &crate::schema::RelationshipType) -> String {
    let parts: Vec<String> = keys
        .iter()
        .filter_map(|k| r.prop(k).or_else(|| t.property(k).and_then(|p| p.default.as_ref())).map(PropValue::display))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    cut_text(parts.join(" "))
}

/// `label` split over `keys` (see step 9 in [`sync`]): the last word to the last key, the
/// first key taking what is left.
fn split_label(label: &str, keys: &[String]) -> Vec<(String, String)> {
    let mut words: Vec<&str> = label.split_whitespace().collect();
    let mut out = Vec::with_capacity(keys.len());
    for (i, k) in keys.iter().enumerate().rev() {
        let part = if i == 0 { words.join(" ") } else { words.pop().unwrap_or_default().to_string() };
        if i == 0 {
            words.clear();
        }
        out.push((k.clone(), part));
    }
    out
}

/// Step 9: end labels show the relationship's end properties; edited ones write back.
fn end_labels(doc: &mut Document, before: &Document, cat: &Catalog) {
    if cat.packs().iter().all(|p| p.relationship_types.iter().all(|t| t.end_labels.is_empty())) {
        return;
    }
    let before_conn: HashMap<&str, (&str, &str)> =
        before.pages.iter().flat_map(|p| p.connectors.iter()).map(|c| (c.id.as_str(), (c.start_label.as_str(), c.end_label.as_str()))).collect();
    let before_rel: HashMap<&str, &Relationship> = before.model.relationships.iter().map(|r| (r.id.as_str(), r)).collect();
    // Write-backs first: (relationship id, key, value).
    let mut writes: Vec<(String, String, String)> = Vec::new();
    let mut written: HashSet<(String, bool)> = HashSet::new();
    for c in doc.pages.iter().flat_map(|p| p.connectors.iter()) {
        let Some(rid) = c.relationship.as_deref() else { continue };
        let Some(r) = doc.model.relationship(rid) else { continue };
        let Some((_, t)) = cat.relationship_type(&r.type_id) else { continue };
        let (Some(was), Some(rb)) = (before_conn.get(c.id.as_str()), before_rel.get(rid)) else { continue };
        for (start, keys, now, then) in [(true, &t.end_labels.start, &c.start_label, was.0), (false, &t.end_labels.end, &c.end_label, was.1)] {
            if keys.is_empty() || now == then || written.contains(&(rid.to_string(), start)) {
                continue;
            }
            if keys.iter().any(|k| rb.prop(k) != r.prop(k)) {
                continue;
            }
            written.insert((rid.to_string(), start));
            writes.extend(split_label(now, keys).into_iter().map(|(k, v)| (rid.to_string(), k, v)));
        }
    }
    for (rid, k, v) in writes {
        let Some(r) = doc.model.relationships.iter_mut().find(|r| r.id == rid) else { continue };
        if v.is_empty() {
            r.properties.remove(&k);
        } else if r.properties.len() < vizcraft_model::MAX_PROPERTIES || r.properties.contains_key(&k) {
            r.properties.insert(k, PropValue::Text(cut_text(v)));
        }
    }
    let labels: HashMap<&str, (String, String)> = doc
        .model
        .relationships
        .iter()
        .filter_map(|r| {
            let (_, t) = cat.relationship_type(&r.type_id)?;
            (!t.end_labels.is_empty()).then(|| (r.id.as_str(), (end_label(r, &t.end_labels.start, t), end_label(r, &t.end_labels.end, t))))
        })
        .collect();
    if labels.is_empty() {
        return;
    }
    for page in &mut doc.pages {
        for c in &mut page.connectors {
            let Some((a, b)) = c.relationship.as_deref().and_then(|r| labels.get(r)) else { continue };
            if c.start_label != *a {
                c.start_label = a.clone();
            }
            if c.end_label != *b {
                c.end_label = b.clone();
            }
        }
    }
}

/// Step 7: every pack whose types are in use is listed in the model.
fn record_packs(doc: &mut Document, cat: &Catalog) {
    let mut seen: HashSet<&str> = doc.model.packs.iter().map(|p| p.id.as_str()).collect();
    let mut add = Vec::new();
    for pack in doc.model.elements.iter().map(|e| e.pack()).chain(doc.model.relationships.iter().map(|r| r.pack())) {
        if seen.insert(pack)
            && let Some(r) = cat.pack_ref(pack)
        {
            add.push(r);
        }
    }
    for r in add {
        doc.model.use_pack(r);
    }
}

#[cfg(test)]
#[path = "bind_tests.rs"]
mod tests;
