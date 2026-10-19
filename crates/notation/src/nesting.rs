//! Nesting: which element types may sit in which containers, how a container type's views are
//! bordered, the sync step that keeps owners in step with the drawing, and the checks about
//! nesting, names and port ends.
//!
//! - `ElementType.allowed_parents` lists the container types an element may sit in (qualified
//!   ids, supertypes count), `*` for any owner and `top` for none. A type that lists nothing does
//!   not nest by drawing, so packs written before this behave as before. A subtype that lists
//!   nothing takes its nearest supertype's list.
//! - `Glyph.container` ([`ContainerLook`]) makes a glyph a container master: shapes drawn from it
//!   are real containers with their own dash, corner and heading (see `vizcraft-stencils`).
//! - [`owners`] is sync step 9: a bound shape that moved into a container shape whose element's
//!   type it allows takes that element as owner; one that left a container for the top loses
//!   the owner that container gave it.
//! - [`check`] runs `nestingAllowed`, `labelled` and `endsOnPorts`; [`check_pack`] checks the
//!   pack's own nesting data when it loads.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vizcraft_diagram::container::{Container, HeadingPosition, HeadingStyle};
use vizcraft_diagram::style_fields::Dash;
use vizcraft_diagram::{Color, Document, Page};
use vizcraft_model::{Element, Issue};

use crate::Catalog;
use crate::rules::{MAX_LABEL, cut, message};
use crate::schema::{Check, Pack, RuleDecl};

/// The `allowedParents` entry meaning "no owner".
pub const TOP: &str = "top";

/// Most `allowedParents` entries one type may list.
pub const MAX_PARENTS: usize = 64;

/// Largest corner radius, margin or line width a look may set, in world units.
pub const MAX_LOOK: f64 = 1000.0;

/// Deepest parent chain walked to find the container a shape sits in.
const MAX_WALK: usize = 64;

/// The border of a container type's views. Every field has a default, so a look lists only what
/// differs from a plain container.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContainerLook {
    /// Dash name (`solid`, `dashed`, `dotted`, `dashDot`, `longDash`); empty: solid.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dash: String,
    /// Line width; none keeps the glyph's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_width: Option<f64>,
    /// Corner radius in world units.
    #[serde(skip_serializing_if = "is_zero")]
    pub corner: f64,
    /// How the heading is drawn: `band`, `line` or `plain`; empty: band.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub heading: String,
    /// Which edge the heading runs along: `top`, `bottom`, `left`, `right`; empty: top.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub heading_position: String,
    /// Fill of the heading band, `#rrggbb`; empty: the default band colour.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub band: String,
    /// Space between the members and the frame; none keeps the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn clamp(v: f64, max: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, max) } else { 0.0 }
}

impl ContainerLook {
    /// The dash the border is drawn with (solid for an unknown name).
    pub fn dash(&self) -> Dash {
        Dash::from_name(&self.dash).unwrap_or_default()
    }

    /// The corner radius, clamped.
    pub fn corner(&self) -> f64 {
        clamp(self.corner, MAX_LOOK)
    }

    /// The line width, clamped, when the look sets one.
    pub fn line_width(&self) -> Option<f64> {
        self.line_width.map(|w| clamp(w, 100.0))
    }

    /// The container settings a shape drawn with this look carries. `keep` is the shape's
    /// current settings: its margin, auto fit and lock stay.
    pub fn container(&self, keep: Option<&Container>) -> Container {
        let mut c = Container {
            heading_style: HeadingStyle::from_name(&self.heading).unwrap_or_default(),
            heading_position: HeadingPosition::from_name(&self.heading_position).unwrap_or_default(),
            style: "pack".into(),
            ..Container::default()
        };
        if let Some(b) = Color::parse(&self.band) {
            c.band = b;
        }
        if let Some(m) = self.margin {
            c.margin = clamp(m, MAX_LOOK);
        }
        if let Some(k) = keep {
            c.margin = k.margin;
            c.auto_fit = k.auto_fit;
            c.locked = k.locked;
        }
        c.sanitize();
        c
    }
}

/// The first 64 chars of `t`, for messages about hostile text.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// Problems with the pack's nesting data: `allowedParents` entries of this pack that are not
/// container types, container looks with unknown names, bad colours or numbers, a variant that
/// maps a container type (variants resize shapes to the master's size, which would undo a
/// container's fit), and the type lists of the nesting, label and port rules.
pub fn check_pack(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    let own = format!("{}.", pack.id);
    for t in &pack.element_types {
        let ctx = format!("element type `{}`", short(&t.id));
        if t.allowed_parents.len() > MAX_PARENTS {
            out.push(format!("{ctx}: at most {MAX_PARENTS} `allowedParents`"));
        }
        for p in t.allowed_parents.iter().take(MAX_PARENTS) {
            if p == "*" || p == TOP {
                continue;
            }
            if !p.contains('.') {
                out.push(format!("{ctx}: `allowedParents` entry `{}` is not `<pack>.<Type>`, `*` or `top`", short(p)));
            } else if p.starts_with(&own) && !pack.element_type(p).is_some_and(|c| c.container) {
                out.push(format!("{ctx}: `allowedParents` entry `{}` is not a container type of this pack", short(p)));
            }
        }
    }
    for g in &pack.glyphs {
        let Some(l) = &g.container else { continue };
        let ctx = format!("glyph `{}`", short(&g.id));
        if !l.dash.is_empty() && Dash::from_name(&l.dash).is_none() {
            out.push(format!("{ctx}: unknown container dash `{}`", short(&l.dash)));
        }
        if !l.heading.is_empty() && HeadingStyle::from_name(&l.heading).is_none() {
            out.push(format!("{ctx}: container heading must be band, line or plain"));
        }
        if !l.heading_position.is_empty() && HeadingPosition::from_name(&l.heading_position).is_none() {
            out.push(format!("{ctx}: container headingPosition must be top, bottom, left or right"));
        }
        if !l.band.is_empty() && Color::parse(&l.band).is_none() {
            out.push(format!("{ctx}: container band `{}` is not a colour like #rrggbb", short(&l.band)));
        }
        let bad = |v: f64, max: f64| !v.is_finite() || !(0.0..=max).contains(&v);
        if bad(l.corner, MAX_LOOK) || l.margin.is_some_and(|m| bad(m, MAX_LOOK)) || l.line_width.is_some_and(|w| bad(w, 100.0)) {
            out.push(format!("{ctx}: container corner and margin must be 0 to {MAX_LOOK}, lineWidth 0 to 100"));
        }
    }
    let is_container_type =
        |id: &str| pack.element_type(id).is_some_and(|t| t.container || pack.glyph(&t.glyph).is_some_and(|g| g.container.is_some()));
    for v in &pack.variants {
        for ty in v.glyphs.keys().filter(|ty| is_container_type(ty)) {
            out.push(format!("variant `{}`: maps container type `{}`, which keeps its own glyph", short(&v.id), short(ty)));
        }
    }
    for r in &pack.rules {
        let ctx = format!("rule `{}`", short(&r.id));
        let unknown = |id: &str, rel: bool| {
            id != "*" && id.starts_with(&own) && if rel { pack.relationship_type(id).is_none() } else { pack.element_type(id).is_none() }
        };
        match &r.check {
            Check::NestingAllowed { types, .. } => {
                out.extend(types.iter().filter(|t| unknown(t, false)).map(|t| format!("{ctx}: unknown element type `{}`", short(t))))
            }
            Check::Labelled { types } => out.extend(
                types
                    .iter()
                    .filter(|t| unknown(t, false) && unknown(t, true))
                    .map(|t| format!("{ctx}: unknown element or relationship type `{}`", short(t))),
            ),
            Check::EndsOnPorts { relationship } if !relationship.is_empty() && unknown(relationship, true) => {
                out.push(format!("{ctx}: unknown relationship type `{}`", short(relationship)))
            }
            _ => {}
        }
    }
    out
}

/// The `allowedParents` list that governs `type_id`: its own, else its nearest supertype's.
/// Empty when no type in the chain lists any.
pub fn allowed_parents<'a>(cat: &'a Catalog, type_id: &'a str) -> &'a [String] {
    for t in crate::infer::chain_of(cat, type_id) {
        if let Some((_, et)) = cat.element_type(t)
            && !et.allowed_parents.is_empty()
        {
            return &et.allowed_parents;
        }
    }
    &[]
}

/// Whether `list` allows an owner of type `owner` (`None`: no owner).
pub fn allows(cat: &Catalog, list: &[String], owner: Option<&str>) -> bool {
    match owner {
        None => list.iter().any(|p| p == TOP),
        Some(t) => list.iter().any(|p| p == "*") || crate::infer::chain_of(cat, t).iter().any(|c| list.iter().any(|p| p == c)),
    }
}

/// The nearest container shape around `id` on `page` (walking `parent`), if any.
fn container_around<'p>(page: &'p Page, id: &str) -> Option<&'p vizcraft_diagram::Shape> {
    let mut at = page.shape(id)?.parent.as_deref();
    for _ in 0..MAX_WALK {
        let p = page.shape(at?)?;
        if vizcraft_diagram::container::is_container(p) {
            return Some(p);
        }
        at = p.parent.as_deref();
    }
    None
}

/// The element of the nearest bound container shape around `id`, if any.
fn bound_container<'p>(page: &'p Page, id: &str) -> Option<&'p str> {
    let mut at = id.to_string();
    for _ in 0..MAX_WALK {
        let c = container_around(page, &at)?;
        if let Some(e) = c.element.as_deref() {
            return Some(e);
        }
        at = c.id.clone();
    }
    None
}

/// Sync step 9: owners follow the drawing for types that declare `allowedParents`. See the
/// module documentation; a cycle `Model::set_owner` refuses leaves the owner as it was.
pub fn owners(doc: &mut Document, before: &Document, cat: &Catalog) {
    // A document with no nesting type is left alone, so every older document stays as it was.
    let mut lists: HashMap<String, Vec<String>> = HashMap::new();
    for e in &doc.model.elements {
        if !lists.contains_key(&e.type_id) {
            let l = allowed_parents(cat, &e.type_id).to_vec();
            lists.insert(e.type_id.clone(), l);
        }
    }
    if lists.values().all(Vec::is_empty) {
        return;
    }
    let types: HashMap<&str, &str> = doc.model.elements.iter().map(|e| (e.id.as_str(), e.type_id.as_str())).collect();
    let mut plan: Vec<(String, Change)> = Vec::new();
    for page in &doc.pages {
        let prev_page = before.pages.iter().find(|p| p.id == page.id);
        for s in &page.shapes {
            let Some(e) = s.element.as_deref() else { continue };
            let Some(list) = types.get(e).and_then(|t| lists.get(*t)).filter(|l| !l.is_empty()) else { continue };
            let prev = prev_page.and_then(|p| p.shape(&s.id));
            if prev.is_some_and(|p| p.parent == s.parent && p.element == s.element) {
                continue;
            }
            match bound_container(page, &s.id) {
                Some(c) => {
                    let ct = types.get(c).copied();
                    if ct.is_some_and(|t| allows(cat, list, Some(t))) {
                        plan.push((e.to_string(), Change::Set(c.to_string())));
                    }
                }
                None => {
                    // Left a container for the top: drop the owner that container gave.
                    let left = prev.filter(|p| p.element.as_deref() == Some(e)).and_then(|p| prev_page.and_then(|pp| bound_container(pp, &p.id)));
                    if let Some(old) = left {
                        plan.push((e.to_string(), Change::ClearIf(old.to_string())));
                    }
                }
            }
        }
    }
    let mut done: HashSet<String> = HashSet::new();
    for (e, owner) in plan {
        if !done.insert(e.clone()) {
            continue;
        }
        let current = doc.model.element(&e).and_then(|x| x.owner.clone());
        match owner {
            Change::ClearIf(o) if current.as_deref() == Some(o.as_str()) => {
                let _ = doc.model.set_owner(&e, None);
            }
            Change::Set(o) if current.as_deref() != Some(o.as_str()) => {
                let _ = doc.model.set_owner(&e, Some(&o));
            }
            _ => {}
        }
    }
}

/// One owner change step 9 plans.
enum Change {
    /// Own by this element.
    Set(String),
    /// Clear the owner when it is still this element.
    ClearIf(String),
}

/// Run one of the nesting, label and port checks.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let elements: HashMap<&str, &Element> = doc.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let covers = |types: &[String], type_id: &str| {
        let chain = crate::infer::chain_of(cat, type_id);
        if types.is_empty() {
            chain.iter().any(|t| pack.element_type(t).is_some())
        } else {
            let star = types.iter().any(|t| t == "*");
            chain.iter().any(|t| types.iter().any(|l| l == t) || (star && pack.element_type(t).is_some()))
        }
    };
    match &rule.check {
        Check::NestingAllowed { types, drawn } => {
            for e in doc.model.elements.iter().filter(|e| covers(types, &e.type_id)) {
                let list = allowed_parents(cat, &e.type_id);
                if list.is_empty() {
                    continue;
                }
                let owner = e.owner.as_deref().and_then(|o| elements.get(o));
                if !allows(cat, list, owner.map(|o| o.type_id.as_str())) {
                    let what = match owner {
                        Some(o) => format!("{} may not sit in {}.", el_label(cat, e), el_label(cat, o)),
                        None => format!("{} must sit in {}.", el_label(cat, e), list_label(cat, list)),
                    };
                    out.push(issue(doc, rule, what, vec![e.id.clone()], None));
                }
                if !drawn {
                    continue;
                }
                for (pi, page) in doc.pages.iter().enumerate() {
                    for s in page.shapes.iter().filter(|s| s.element.as_deref() == Some(e.id.as_str())) {
                        let Some(c) = bound_container(page, &s.id).and_then(|c| elements.get(c)) else { continue };
                        if Some(c.id.as_str()) == e.owner.as_deref() || allows(cat, list, Some(&c.type_id)) {
                            continue;
                        }
                        let what = format!("{} is drawn inside {}.", el_label(cat, e), el_label(cat, c));
                        out.push(issue(doc, rule, what, vec![e.id.clone()], Some((pi, s.id.clone()))));
                    }
                }
            }
        }
        Check::Labelled { types } => {
            for e in doc.model.elements.iter().filter(|e| e.name.trim().is_empty() && covers(types, &e.type_id)) {
                out.push(issue(doc, rule, format!("{} has no name.", el_label(cat, e)), vec![e.id.clone()], None));
            }
            for r in &doc.model.relationships {
                let listed = if types.is_empty() { pack.relationship_type(&r.type_id).is_some() } else { types.iter().any(|t| t == &r.type_id) };
                if listed && r.name.trim().is_empty() {
                    let (a, b) = (elements.get(r.source.as_str()), elements.get(r.target.as_str()));
                    let ends = match (a, b) {
                        (Some(a), Some(b)) => format!(" from {} to {}", el_label(cat, a), el_label(cat, b)),
                        _ => String::new(),
                    };
                    out.push(issue(doc, rule, format!("{}{ends} has no label.", rel_label(cat, &r.type_id)), vec![r.id.clone()], None));
                }
            }
        }
        Check::EndsOnPorts { relationship } => {
            for r in &doc.model.relationships {
                let wanted = if relationship.is_empty() { r.pack() == pack.id } else { &r.type_id == relationship };
                if !wanted {
                    continue;
                }
                // One issue per relationship, naming every end that is on no port.
                let mut loose: Vec<String> = Vec::new();
                for (end, port) in [(r.source.as_str(), &r.source_port), (r.target.as_str(), &r.target_port)] {
                    let Some(el) = elements.get(end) else { continue };
                    if port.is_none() && has_ports(cat, &el.type_id) {
                        loose.push(el_label(cat, el));
                    }
                }
                if !loose.is_empty() {
                    let what = format!("{} meets {} on no port.", rel_label(cat, &r.type_id), loose.join(" and "));
                    out.push(issue(doc, rule, what, vec![r.id.clone()], None));
                }
            }
        }
        _ => {}
    }
}

/// Whether `type_id` or a supertype declares ports.
fn has_ports(cat: &Catalog, type_id: &str) -> bool {
    crate::infer::chain_of(cat, type_id).iter().any(|t| cat.element_type(t).is_some_and(|(_, et)| !et.ports.is_empty()))
}

/// An issue about `subjects`, pointing at one view (`at`) or every view of them.
fn issue(doc: &Document, rule: &RuleDecl, what: String, subjects: Vec<String>, at: Option<(usize, String)>) -> Issue {
    let mut items: Vec<(usize, String)> = Vec::new();
    match at {
        Some(a) => items.push(a),
        None => {
            for s in &subjects {
                for v in vizcraft_diagram::views::views_of(doc, s) {
                    items.extend(v.items.into_iter().map(|i| (v.page, i)));
                }
            }
        }
    }
    let page = match items.first() {
        Some((p, _)) if items.iter().all(|(q, _)| q == p) => doc.pages.get(*p).map(|pg| pg.id.clone()),
        _ => None,
    };
    let mut i = Issue::new(rule.id.clone(), rule.severity, message(&rule.description, &what))
        .with_subjects(subjects)
        .with_items(items.into_iter().map(|(_, id)| id).collect());
    i.page = page;
    i
}

fn type_label(cat: &Catalog, type_id: &str) -> String {
    match cat.element_type(type_id) {
        Some((_, t)) if !t.label.is_empty() => t.label.clone(),
        _ => cut(vizcraft_model::local_of(type_id), MAX_LABEL),
    }
}

fn rel_label(cat: &Catalog, type_id: &str) -> String {
    match cat.relationship_type(type_id) {
        Some((_, t)) if !t.label.is_empty() => t.label.clone(),
        _ => cut(vizcraft_model::local_of(type_id), MAX_LABEL),
    }
}

/// An element as a message names it: its type, then its quoted name, else tag, else id.
fn el_label(cat: &Catalog, e: &Element) -> String {
    let handle = if !e.name.trim().is_empty() {
        let one: String = e.name.trim().chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        format!("\"{}\"", cut(&one, MAX_LABEL))
    } else if !e.tag.is_empty() {
        cut(&e.tag, MAX_LABEL)
    } else {
        cut(&e.id, MAX_LABEL)
    };
    format!("{} {handle}", type_label(cat, &e.type_id))
}

/// The allowed parents as words: `a Region or an Account`.
fn list_label(cat: &Catalog, list: &[String]) -> String {
    let words: Vec<String> =
        list.iter().filter(|p| *p != TOP).take(8).map(|p| if p == "*" { "a container".into() } else { type_label(cat, p) }).collect();
    if words.is_empty() { "a container".into() } else { words.join(" or ") }
}

#[cfg(test)]
#[path = "nesting_tests.rs"]
mod tests;
