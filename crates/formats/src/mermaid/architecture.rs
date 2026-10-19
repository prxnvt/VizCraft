//! Mermaid `architecture-beta` read and write, mapped onto the Cloud Topology pack.
//!
//! Reading lays the diagram out like any Mermaid graph, then draws each service with the Cloud
//! master its icon names (database, disk, internet, server and cloud are the five documented
//! icons), groups as Cloud containers holding their members (`parent` set), and glues each edge
//! to the connection point on the side it names. Every shape carries its element type under
//! [`crate::csv::TYPE_KEY`], so `file.import {merge: true}` makes typed elements. A comment line
//! `%% type: <id> <type>` restores a type the five icons cannot carry. An icon pack name (such as
//! `logos:...`) is never fetched or drawn: the service becomes Compute, the name is kept in
//! `ext` under `mermaid.icon` and a warning says so.
//!
//! Writing turns a page back into text: ids from element tags, else element or shape ids; sides
//! read back from where each connector meets its shapes. Connector labels and steps have no place
//! in the grammar and come back as warnings.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::{Arrow, Document, End, Page, Point, Shape};
use vizcraft_mermaid::architecture::{ArchitectureDiagram, Side};

use super::common::Warnings;
use crate::FormatError;
use crate::csv::TYPE_KEY;

/// The `ext` key holding an icon name the pack has no glyph for.
pub const ICON_EXT: &str = "mermaid.icon";

/// The type a service or group becomes when nothing else names one.
const FALLBACK: &str = "cloud.Compute";

/// The documented icons and the Cloud types they stand for (services, then the group icon).
const ICONS: &[(&str, &str)] = &[
    ("database", "cloud.Database"),
    ("disk", "cloud.Bucket"),
    ("internet", "cloud.Internet"),
    ("server", "cloud.Compute"),
    ("cloud", "cloud.Cloud"),
];

/// The master that draws `type_id`: its local name with a lowercase first letter.
fn master_of(type_id: &str) -> String {
    let (pack, local) = type_id.split_once('.').unwrap_or(("", type_id));
    let mut c = local.chars();
    let first = c.next().map(|f| f.to_lowercase().collect::<String>()).unwrap_or_default();
    format!("{pack}.{first}{}", c.as_str())
}

/// The type a master draws, the reverse of [`master_of`].
fn type_of_master(master: &str) -> Option<String> {
    let (pack, local) = master.split_once('.')?;
    let mut c = local.chars();
    let first = c.next()?.to_uppercase().collect::<String>();
    Some(format!("{pack}.{first}{}", c.as_str()))
}

/// The `%% type: <id> <type>` comments of `text`, id to type (a later line wins).
fn type_comments(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("%%").map(str::trim_start).and_then(|r| r.strip_prefix("type:")) else { continue };
        let mut words = rest.split_whitespace();
        if let (Some(id), Some(t)) = (words.next(), words.next())
            && t.len() <= 128
        {
            out.insert(id.to_string(), t.to_string());
        }
    }
    out
}

/// Which connection point of `s` lies on `side` (the one nearest the middle of that edge).
fn point_on(s: &Shape, side: Side) -> Option<usize> {
    let want = match side {
        Side::Left => (0.0, 0.5),
        Side::Right => (1.0, 0.5),
        Side::Top => (0.5, 0.0),
        Side::Bottom => (0.5, 1.0),
    };
    let d = |p: &Point| (p.x - want.0).powi(2) + (p.y - want.1).powi(2);
    s.connection_points.iter().enumerate().filter(|(_, p)| d(p) < 0.1).min_by(|a, b| d(a.1).total_cmp(&d(b.1))).map(|(i, _)| i)
}

/// Redraw `s` with the master of `type_id`, keeping its id, frame, text and parent.
fn redraw(s: &mut Shape, type_id: &str) {
    let Some(m) = vizcraft_stencils::master(&master_of(type_id)) else { return };
    let mut n = m.instantiate(s.id.clone(), s.x, s.y, Some(s.w), Some(s.h));
    n.text = std::mem::take(&mut s.text);
    n.parent = s.parent.take();
    n.ext = std::mem::take(&mut s.ext);
    n.ext.remove("mermaid.shape");
    n.ext.insert(TYPE_KEY.into(), type_id.to_string());
    n.sanitize();
    *s = n;
}

/// Lay the diagram out as a page of Cloud shapes; `text` is read again for type comments.
pub(super) fn build(a: &ArchitectureDiagram, text: &str, warnings: &mut Warnings) -> Result<Page, FormatError> {
    let mut page = super::graph::build(&a.to_graph(), warnings)?;
    let comments = type_comments(text);
    let known = |t: &str| vizcraft_stencils::master(&master_of(t)).is_some();
    let typed = |id: &str, icon: Option<&str>, group: bool, warnings: &mut Warnings| -> (String, Option<String>) {
        if let Some(t) = comments.get(id).filter(|t| known(t)) {
            // An icon pack name is kept for the way back even when a comment gives the type.
            let foreign = icon.filter(|i| !ICONS.iter().any(|(name, _)| name == i)).map(str::to_string);
            return (t.clone(), foreign);
        }
        let by_icon = icon.and_then(|i| ICONS.iter().find(|(name, _)| *name == i)).map(|(_, t)| (*t).to_string());
        match (by_icon, icon) {
            (Some(t), _) if group == (t == "cloud.Cloud") => (t, None),
            (_, None) if group => ("cloud.Group".into(), None),
            (_, None) => (FALLBACK.into(), None),
            (_, Some(i)) => {
                warnings.push(format!(
                    "icon `{}` of `{}` has no Cloud glyph; drawn as {}",
                    super::common::short(i),
                    super::common::short(id),
                    if group { "a group" } else { "compute" }
                ));
                (if group { "cloud.Group".into() } else { FALLBACK.into() }, Some(i.to_string()))
            }
        }
    };
    let mut parents: HashMap<String, String> = HashMap::new();
    for (items, group) in [(&a.groups, true), (&a.services, false)] {
        for it in items.iter() {
            let (t, icon) = typed(&it.id, it.icon.as_deref(), group, warnings);
            if let Some(s) = page.shapes.iter_mut().find(|s| s.id == it.id) {
                // The graph view adds the icon as an annotation line; the title alone is the name.
                s.text = it.title.clone().unwrap_or_else(|| it.id.clone());
                redraw(s, &t);
                if let Some(i) = icon {
                    s.ext.insert(ICON_EXT.into(), i);
                }
                s.sanitize();
            }
            if let Some(p) = &it.parent {
                parents.insert(it.id.clone(), p.clone());
            }
        }
    }
    for j in &a.junctions {
        if let Some(s) = page.shapes.iter_mut().find(|s| s.id == j.id) {
            redraw(s, "cloud.Junction");
        }
        if let Some(p) = &j.parent {
            parents.insert(j.id.clone(), p.clone());
        }
    }
    let ids: HashSet<String> = page.shapes.iter().map(|s| s.id.clone()).collect();
    for s in &mut page.shapes {
        s.parent = parents.get(&s.id).filter(|p| ids.contains(*p)).cloned();
    }
    // Edges: the page's connectors are the diagram's edges in order.
    if page.connectors.len() == a.edges.len() {
        for (c, e) in page.connectors.iter_mut().zip(&a.edges) {
            for (end, at) in [(&mut c.start, &e.from), (&mut c.end, &e.to)] {
                let target = if at.group { parents.get(&at.id).cloned().unwrap_or_else(|| at.id.clone()) } else { at.id.clone() };
                if let Some(s) = page.shapes.iter().find(|s| s.id == target) {
                    *end = match point_on(s, at.side) {
                        Some(i) => End::point(target, i),
                        None => End::dynamic(target),
                    };
                }
            }
            if !e.from.arrow && !e.to.arrow {
                c.end_arrow = Arrow::None;
            }
        }
    } else {
        warnings.push("edges could not all be matched to their sides".to_string());
    }
    Ok(page)
}

/// Whether `s` reads as an architecture id.
fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A title the grammar can hold: one line, no `]`.
fn title(t: &str, what: &str, warnings: &mut Vec<String>) -> String {
    let clean: String = t.chars().map(|c| if c == ']' || c == '[' || c.is_control() { ' ' } else { c }).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean != t.trim() {
        warnings.push(format!("the name of `{what}` was simplified to fit the grammar"));
    }
    clean
}

/// The side of `s` that world point `p` lies on.
fn side_of(s: &Shape, p: Point) -> Side {
    let b = s.bounds();
    let (cx, cy) = (b.center().x, b.center().y);
    let (dx, dy) = ((p.x - cx) / b.width().max(1e-6), (p.y - cy) / b.height().max(1e-6));
    if dx.abs() >= dy.abs() {
        if dx < 0.0 { Side::Left } else { Side::Right }
    } else if dy < 0.0 {
        Side::Top
    } else {
        Side::Bottom
    }
}

/// What one written item is.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Group,
    Service,
    Junction,
}

/// Write page `index` of `doc` as `architecture-beta` text, with warnings for what the grammar
/// cannot hold.
pub fn write(doc: &Document, index: usize) -> Result<(String, Vec<String>), FormatError> {
    let page = doc.pages.get(index).ok_or(FormatError::NoPage(index))?;
    let mut warnings = Vec::new();
    // Each shape the topology draws: its id, role and type.
    let mut items: Vec<(&Shape, String, Role, String)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for s in &page.shapes {
        let element = s.element.as_deref().and_then(|e| doc.model.element(e));
        let t = element
            .map(|e| e.type_id.clone())
            .or_else(|| s.ext.get(TYPE_KEY).cloned())
            .or_else(|| type_of_master(&s.stencil).filter(|_| s.stencil.starts_with("cloud.")));
        let Some(t) = t.filter(|t| t.starts_with("cloud.")) else { continue };
        let Some(m) = vizcraft_stencils::master(&master_of(&t)) else { continue };
        let role = if m.container.is_some() {
            Role::Group
        } else if t == "cloud.Junction" {
            Role::Junction
        } else {
            Role::Service
        };
        let wanted = element.map(|e| if is_id(&e.tag) { e.tag.clone() } else { e.id.clone() }).unwrap_or_else(|| s.id.clone());
        let mut id: String = wanted.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).take(64).collect();
        if id.is_empty() {
            id = "n".into();
        }
        let base = id.clone();
        let mut n = 1usize;
        while !used.insert(id.clone()) {
            n += 1;
            id = format!("{base}_{n}");
        }
        items.push((s, id, role, t));
    }
    let by_shape: HashMap<&str, usize> = items.iter().enumerate().map(|(i, it)| (it.0.id.as_str(), i)).collect();
    // The group each item sits in: the nearest written group up its parent chain.
    let parent_of = |s: &Shape| -> Option<usize> {
        let mut at = s.parent.as_deref();
        for _ in 0..64 {
            let p = page.shape(at?)?;
            if let Some(&i) = by_shape.get(p.id.as_str())
                && items.get(i).is_some_and(|it| it.2 == Role::Group)
            {
                return Some(i);
            }
            at = p.parent.as_deref();
        }
        None
    };
    let parents: Vec<Option<usize>> = items.iter().map(|it| parent_of(it.0)).collect();
    let depth = |mut i: usize| {
        let mut d = 0usize;
        while let Some(Some(p)) = parents.get(i) {
            d += 1;
            if d > 64 {
                break;
            }
            i = *p;
        }
        d
    };
    let mut out = String::from("architecture-beta\n");
    for (s, id, role, t) in &items {
        let carried = match role {
            Role::Group => *t == "cloud.Cloud",
            Role::Junction => true,
            Role::Service => ICONS.iter().any(|(_, it)| it == t) && *t != "cloud.Cloud",
        };
        let has_icon = s.ext.contains_key(ICON_EXT);
        if !carried || has_icon {
            let _ = writeln!(out, "    %% type: {id} {t}");
        }
    }
    let line = |out: &mut String, i: usize, warnings: &mut Vec<String>| {
        let Some((s, id, role, t)) = items.get(i) else { return };
        let inside = parents.get(i).copied().flatten().and_then(|p| items.get(p)).map(|p| format!(" in {}", p.1)).unwrap_or_default();
        let name = s.element.as_deref().and_then(|e| doc.model.element(e)).map(|e| e.name.clone()).unwrap_or_else(|| s.text.clone());
        let icon = s.ext.get(ICON_EXT).cloned().unwrap_or_else(|| {
            ICONS
                .iter()
                .find(|(_, it)| it == t)
                .map(|(n, _)| (*n).to_string())
                .unwrap_or_else(|| if *role == Role::Group { "cloud".into() } else { "server".into() })
        });
        match role {
            Role::Junction => {
                let _ = writeln!(out, "    junction {id}{inside}");
            }
            Role::Group | Role::Service => {
                let kw = if *role == Role::Group { "group" } else { "service" };
                let title = title(&name, id, warnings);
                let title = if title.is_empty() { String::new() } else { format!("[{title}]") };
                let _ = writeln!(out, "    {kw} {id}({icon}){title}{inside}");
            }
        }
    };
    let mut groups: Vec<usize> = (0..items.len()).filter(|&i| items.get(i).is_some_and(|it| it.2 == Role::Group)).collect();
    groups.sort_by_key(|&i| depth(i));
    for i in groups {
        line(&mut out, i, &mut warnings);
    }
    for role in [Role::Service, Role::Junction] {
        for i in (0..items.len()).filter(|&i| items.get(i).is_some_and(|it| it.2 == role)) {
            line(&mut out, i, &mut warnings);
        }
    }
    for c in &page.connectors {
        let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
        let (Some(&ia), Some(&ib)) = (by_shape.get(a), by_shape.get(b)) else { continue };
        let g = vizcraft_diagram::connector_geometry(page, c);
        let (Some(pa), Some(pb)) = (g.polyline.first().copied(), g.polyline.last().copied()) else { continue };
        let end = |i: usize, p: Point, warnings: &mut Vec<String>| -> Option<String> {
            let (s, id, role, _) = items.get(i)?;
            let side = side_of(s, p).letter();
            match role {
                Role::Group => {
                    // An edge on a group is written from a service inside it, with `{group}`.
                    let member = items.iter().enumerate().find(|(k, it)| it.2 == Role::Service && parents.get(*k).copied().flatten() == Some(i));
                    match member {
                        Some((_, m)) => Some(format!("{}{{group}}:{side}", m.1)),
                        None => {
                            warnings
                                .push(format!("connector `{}` meets group `{id}`, which holds no service; left out", super::common::short(&c.id)));
                            None
                        }
                    }
                }
                _ => Some(format!("{id}:{side}")),
            }
        };
        let (Some(from), Some(to)) = (end(ia, pa, &mut warnings), end(ib, pb, &mut warnings)) else { continue };
        let (lt, gt) = (if c.start_arrow != Arrow::None { "<" } else { "" }, if c.end_arrow != Arrow::None { ">" } else { "" });
        // The right end is written side first.
        let to = match to.rsplit_once(':') {
            Some((who, side)) => format!("{side}:{who}"),
            None => to,
        };
        let _ = writeln!(out, "    {from} {lt}--{gt} {to}");
        let label = c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).map(|r| r.name.clone()).unwrap_or_else(|| c.text.clone());
        if !label.trim().is_empty() {
            warnings.push(format!("the label of connector `{}` has no place in the grammar", super::common::short(&c.id)));
        }
        if c.step > 0 {
            warnings.push(format!("step {} of connector `{}` has no place in the grammar", c.step, super::common::short(&c.id)));
        }
    }
    Ok((out, warnings))
}

#[cfg(test)]
#[path = "architecture/tests.rs"]
mod tests;
