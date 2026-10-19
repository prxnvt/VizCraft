//! Mermaid class diagrams as UML: read into `uml.*` shapes with typed data, and written back.
//!
//! Reading lays the diagram out like any Mermaid graph, then draws each class with the UML
//! master of its kind (`<<interface>>` and `<<enumeration>>` pick the type, `<<abstract>>` sets
//! `isAbstract`, another annotation becomes the stereotype), fills its member compartments, and
//! stores the type and properties in `ext` (`import.type`, `import.prop.*`) for a merging import.
//! Namespaces become package containers that own their classes. Relationships take the UML type
//! their ends mean, with source and target as UML reads them: `A <|-- B` is B specializing A,
//! `A *-- B` is A the whole of part B; cardinalities become the end multiplicities.
//!
//! Writing uses `class <id>["<name>"]` so ids survive a round trip, members in Mermaid's
//! `Type name` order (a multiplicity rides on the type as `Type[0..*]`), `$` and `*` for static
//! and abstract members.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::compartment::{self as cp, Compartment, CompartmentKind};
use vizcraft_diagram::{Connector, Document, Page, Shape};
use vizcraft_mermaid::class::{ClassDiagram, Classifier, MemberKind as MMemberKind, RelationEnd, RelationKind, Visibility as MVisibility};
use vizcraft_notation::uml_text::{self, Member, MemberKind, Shown, Visibility, format_member};

use super::common::Warnings;
use crate::FormatError;
use crate::csv::{PROP_PREFIX, TYPE_KEY};
use crate::uml::{self, UmlElement, UmlPage, alias, one_line};

/// Longest name or label written, in bytes.
const MAX_NAME: usize = 256;

/// Draw `s` with UML master `master`, keeping its frame (or taking the master's size about the
/// same centre when `resize`).
pub(super) fn restyle(s: &mut Shape, master: &str, resize: bool) {
    let Some(m) = vizcraft_stencils::master(master) else { return };
    if resize {
        let (cx, cy) = (s.x + s.w / 2.0, s.y + s.h / 2.0);
        (s.w, s.h) = (m.w, m.h);
        (s.x, s.y) = (cx - m.w / 2.0, cy - m.h / 2.0);
    }
    m.redraw(s);
    s.ext.remove("mermaid.line");
}

/// Give `c` the look of the connector type that draws `rel_type`, and its type in `ext`.
pub(super) fn type_connector(c: &mut Connector, rel_type: &str) {
    if let Some(k) = vizcraft_stencils::connector_kind_for_relationship(rel_type, "") {
        k.apply(c);
    }
    for key in ["mermaid.line", "mermaid.startMarker", "mermaid.endMarker"] {
        c.ext.remove(key);
    }
    c.ext.insert(TYPE_KEY.into(), rel_type.into());
}

/// Put `value` in `ext` as property `key` when it is not empty.
pub(super) fn set_prop(ext: &mut std::collections::BTreeMap<String, String>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        ext.insert(format!("{PROP_PREFIX}{key}"), value.trim().to_string());
    }
}

/// Make `s` a container its members can belong to.
pub(super) fn make_container(s: &mut Shape) {
    if s.container.is_none() {
        s.container = Some(vizcraft_diagram::container::Container::default());
    }
}

/// A Mermaid member as a UML member row.
fn member(m: &vizcraft_mermaid::class::Member) -> Member {
    let visibility = m.visibility.map(|v| match v {
        MVisibility::Public => Visibility::Public,
        MVisibility::Private => Visibility::Private,
        MVisibility::Protected => Visibility::Protected,
        MVisibility::Package => Visibility::Package,
    });
    let modifiers = match m.classifier {
        Some(Classifier::Static) => vec!["static".to_string()],
        Some(Classifier::Abstract) => vec!["abstract".to_string()],
        None => Vec::new(),
    };
    let ty = m.ty.clone().unwrap_or_default();
    match m.kind {
        MMemberKind::Method => Member {
            visibility,
            name: m.name.clone(),
            params: m.parameters.clone().unwrap_or_default(),
            ret: ty,
            modifiers,
            kind: MemberKind::Operation,
            ..Member::default()
        },
        MMemberKind::Attribute => {
            // A multiplicity written on the type: `Item[0..*]`.
            let (ty, multiplicity) = match (ty.rfind('['), ty.ends_with(']')) {
                (Some(open), true) => {
                    (ty.get(..open).unwrap_or_default().trim().to_string(), ty.get(open + 1..ty.len() - 1).unwrap_or_default().to_string())
                }
                _ => (ty, String::new()),
            };
            Member { visibility, name: m.name.clone(), ty, multiplicity, modifiers, kind: MemberKind::Attribute, ..Member::default() }
        }
    }
}

/// A list compartment of rows.
fn rows_compartment(id: &str, rows: &[String]) -> Compartment {
    let mut c = Compartment::new(id, CompartmentKind::List);
    c.rows = rows.to_vec();
    c.sanitize();
    c
}

/// Read a Mermaid class diagram as a laid-out page of UML shapes.
pub(super) fn build(d: &ClassDiagram, warnings: &mut Warnings) -> Result<Page, FormatError> {
    let mut g = d.to_graph();
    let n = d.relations.len();
    for (i, e) in g.edges.iter_mut().enumerate() {
        e.id = Some(if i < n { format!("rel{}", i.saturating_add(1)) } else { format!("anchor{}", i.saturating_add(1).saturating_sub(n)) });
    }
    let mut page = super::graph::build(&g, warnings)?;
    let index: HashMap<String, usize> = page.shapes.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
    // Namespaces: package containers.
    for ns in &d.namespaces {
        let Some(s) = index.get(&ns.id).and_then(|&i| page.shapes.get_mut(i)) else { continue };
        restyle(s, "uml.package", false);
        make_container(s);
        s.ext.insert(TYPE_KEY.into(), "uml.Package".into());
        s.ext.remove("mermaid.group");
        s.parent = ns.parent.clone().filter(|p| index.contains_key(p));
        s.sanitize();
    }
    for c in &d.classes {
        let Some(s) = index.get(&c.id).and_then(|&i| page.shapes.get_mut(i)) else { continue };
        let lower: Vec<String> = c.annotations.iter().map(|a| a.trim().to_ascii_lowercase()).collect();
        let (ty, master, keyword) = if lower.iter().any(|a| a == "interface") {
            ("uml.Interface", "uml.interface", Some("interface"))
        } else if lower.iter().any(|a| a == "enumeration" || a == "enum") {
            ("uml.Enumeration", "uml.enumeration", Some("enumeration"))
        } else {
            ("uml.Class", "uml.class", None)
        };
        let stereotype =
            c.annotations.iter().find(|a| !matches!(a.trim().to_ascii_lowercase().as_str(), "interface" | "enumeration" | "enum" | "abstract"));
        let name = c.display_name();
        restyle(s, master, false);
        s.ext.insert(TYPE_KEY.into(), ty.into());
        let shown_kw = stereotype.map(|x| x.trim().to_string()).or(keyword.map(str::to_string));
        s.text = match &shown_kw {
            Some(k) => format!("\u{ab}{k}\u{bb}\n{name}"),
            None => name.clone(),
        };
        if lower.iter().any(|a| a == "abstract") && ty == "uml.Class" {
            set_prop(&mut s.ext, "isAbstract", "true");
            s.style.italic = true;
        }
        if let Some(st) = stereotype {
            set_prop(&mut s.ext, "stereotype", st);
        }
        s.compartments.clear();
        if ty == "uml.Enumeration" {
            let literals: Vec<String> = c.members.iter().map(|m| m.name.clone()).filter(|x| !x.is_empty()).collect();
            set_prop(&mut s.ext, "literals", &literals.join("\n"));
            s.compartments.push(rows_compartment("literals", &literals));
        } else {
            let attrs: Vec<String> =
                c.members.iter().filter(|m| m.kind == MMemberKind::Attribute).map(|m| format_member(&member(m), Shown::Symbols)).collect();
            let ops: Vec<String> =
                c.members.iter().filter(|m| m.kind == MMemberKind::Method).map(|m| format_member(&member(m), Shown::Symbols)).collect();
            set_prop(&mut s.ext, "attributes", &attrs.join("\n"));
            set_prop(&mut s.ext, "operations", &ops.join("\n"));
            s.compartments.push(rows_compartment("attrs", &attrs));
            s.compartments.push(rows_compartment("ops", &ops));
        }
        s.parent = c.namespace.clone().filter(|p| index.contains_key(p));
        s.h = s.h.max(cp::fit_height(s));
        s.sanitize();
    }
    for (i, _) in d.notes.iter().enumerate() {
        let Some(s) = index.get(&format!("note{i}")).and_then(|&i| page.shapes.get_mut(i)) else { continue };
        restyle(s, "uml.note", false);
        s.ext.insert(TYPE_KEY.into(), "uml.Note".into());
        s.sanitize();
    }
    let rels: HashMap<String, usize> = (0..n).map(|i| (format!("rel{}", i.saturating_add(1)), i)).collect();
    for c in &mut page.connectors {
        if c.id.starts_with("anchor") {
            type_connector(c, "uml.Anchor");
            c.text.clear();
            c.sanitize();
            continue;
        }
        let Some(r) = rels.get(&c.id).and_then(|&i| d.relations.get(i)) else { continue };
        let (ty, reversed, nav) = relation_type(r, warnings);
        if reversed {
            std::mem::swap(&mut c.start, &mut c.end);
        }
        type_connector(c, ty);
        let (src, tgt) = if reversed { (&r.to_cardinality, &r.from_cardinality) } else { (&r.from_cardinality, &r.to_cardinality) };
        let src = src.clone().unwrap_or_default();
        let tgt = tgt.clone().unwrap_or_default();
        if matches!(ty, "uml.Association" | "uml.Composition" | "uml.Aggregation") {
            set_prop(&mut c.ext, "sourceMultiplicity", &src);
            set_prop(&mut c.ext, "targetMultiplicity", &tgt);
            c.start_label = src;
            c.end_label = tgt;
        } else if !src.is_empty() || !tgt.is_empty() {
            warnings.add("cardinalities on a relationship that is not an association were left out");
        }
        if let Some(nav) = nav {
            set_prop(&mut c.ext, "navigability", nav);
        }
        c.text = r.label.clone().unwrap_or_default();
        c.sanitize();
    }
    Ok(page)
}

/// The UML type a Mermaid relationship means, whether its source and target are reversed from
/// how it is written, and the navigability of an association.
fn relation_type(r: &vizcraft_mermaid::class::Relation, warnings: &mut Warnings) -> (&'static str, bool, Option<&'static str>) {
    match r.kind() {
        RelationKind::Inheritance => ("uml.Generalization", r.from_end == RelationEnd::Inheritance, None),
        RelationKind::Realization => ("uml.Realization", r.from_end == RelationEnd::Inheritance, None),
        RelationKind::Composition => ("uml.Composition", r.from_end != RelationEnd::Composition, None),
        RelationKind::Aggregation => ("uml.Aggregation", r.from_end != RelationEnd::Aggregation, None),
        RelationKind::Association => {
            let nav = match (r.from_end == RelationEnd::Arrow, r.to_end == RelationEnd::Arrow) {
                (true, true) => "both",
                (true, false) => "source",
                _ => "target",
            };
            ("uml.Association", false, Some(nav))
        }
        RelationKind::Link => ("uml.Association", false, None),
        RelationKind::Dependency => ("uml.Dependency", r.from_end == RelationEnd::Arrow && r.to_end != RelationEnd::Arrow, None),
        RelationKind::DashedLink => {
            warnings.add("a dashed link with no arrow was read as a dependency");
            ("uml.Dependency", false, None)
        }
        RelationKind::Lollipop => {
            warnings.add("a lollipop interface link was read as an association");
            ("uml.Association", false, None)
        }
    }
}

/// A type written for Mermaid: generics with `~`, no spaces.
fn mermaid_type(t: &str) -> String {
    t.replace(['<', '>'], "~").replace(", ", ",").replace(' ', "")
}

/// One member row in Mermaid's order.
fn mermaid_member(m: &Member, warnings: &mut Vec<String>) -> String {
    let vis = m.visibility.map(Visibility::symbol).unwrap_or_default();
    let classifier = if m.is_static() {
        "$"
    } else if m.is_abstract() {
        "*"
    } else {
        ""
    };
    match m.kind {
        MemberKind::Operation => {
            let ret = if m.ret.is_empty() { String::new() } else { format!(" {}", mermaid_type(&m.ret)) };
            format!("{vis}{}({}){classifier}{ret}", m.name, m.params.replace(['<', '>'], "~"))
        }
        MemberKind::Attribute => {
            if !m.default.is_empty() {
                warnings.push(format!("the default value of {} cannot be written in Mermaid and was left out", m.name));
            }
            let mut ty = mermaid_type(&m.ty);
            if !m.multiplicity.is_empty() {
                let _ = write!(ty, "[{}]", m.multiplicity);
            }
            if ty.is_empty() { format!("{vis}{}{classifier}", m.name) } else { format!("{vis}{ty} {}{classifier}", m.name) }
        }
    }
}

/// A class label in Mermaid's `["..."]`, with characters that would end it replaced.
fn class_label(name: &str) -> String {
    one_line(name, MAX_NAME).replace(['"', ']'], "'")
}

/// Write page `page` of `doc` as a Mermaid class diagram.
pub(super) fn write(doc: &Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    let p = uml::read(doc, page)?;
    Ok(write_page(&p))
}

fn write_page(p: &UmlPage) -> (String, Vec<String>) {
    let mut warnings = Vec::new();
    let mut used = HashSet::new();
    let ids: HashMap<&str, String> = p.elements.iter().map(|e| (e.id.as_str(), alias(&e.id, &mut used))).collect();
    let id = |x: &str| ids.get(x).cloned().unwrap_or_else(|| x.to_string());
    let mut out = String::from("classDiagram\n");
    let classes = |e: &UmlElement| matches!(e.local(), "Class" | "Interface" | "Enumeration");
    let mut skipped = 0usize;
    let write_class = |e: &UmlElement, indent: &str, out: &mut String, warnings: &mut Vec<String>| {
        let mut lines: Vec<String> = Vec::new();
        match e.local() {
            "Interface" => lines.push("<<interface>>".into()),
            "Enumeration" => lines.push("<<enumeration>>".into()),
            _ if e.prop("isAbstract") == "true" => lines.push("<<abstract>>".into()),
            _ => {}
        }
        if !e.prop("stereotype").is_empty() {
            lines.push(format!("<<{}>>", one_line(e.prop("stereotype"), MAX_NAME)));
        }
        if e.local() == "Enumeration" {
            lines.extend(uml_text::rows(e.prop("literals")).map(|l| one_line(l, MAX_NAME)));
        } else {
            for m in e.members("attributes").iter().chain(e.members("operations").iter()) {
                lines.push(one_line(&mermaid_member(m, warnings), MAX_NAME));
            }
        }
        let head = format!("{indent}class {}[\"{}\"]", id(&e.id), class_label(&e.name));
        if lines.is_empty() {
            let _ = writeln!(out, "{head}");
        } else {
            let _ = writeln!(out, "{head} {{");
            for l in lines {
                let _ = writeln!(out, "{indent}  {l}");
            }
            let _ = writeln!(out, "{indent}}}");
        }
    };
    let packages: Vec<&UmlElement> = p.elements.iter().filter(|e| e.local() == "Package").collect();
    for e in p.elements.iter().filter(|e| classes(e)) {
        let in_package = e.owner.as_deref().and_then(|o| p.element(o)).filter(|o| o.local() == "Package");
        if in_package.is_none() {
            write_class(e, "  ", &mut out, &mut warnings);
        }
    }
    for pk in &packages {
        let members: Vec<&UmlElement> = p.children(Some(pk.id.as_str())).filter(|e| classes(e)).collect();
        if members.is_empty() {
            continue;
        }
        if pk.name != pk.id {
            warnings.push(format!(
                "package \"{}\" is written as namespace {}; Mermaid namespaces carry no separate name",
                one_line(&pk.name, 60),
                id(&pk.id)
            ));
        }
        let _ = writeln!(out, "  namespace {} {{", id(&pk.id));
        for e in members {
            write_class(e, "    ", &mut out, &mut warnings);
        }
        let _ = writeln!(out, "  }}");
    }
    for r in &p.relationships {
        let card = |m: &str| if m.is_empty() { String::new() } else { format!(" \"{}\"", one_line(m, 64).replace('"', "'")) };
        let (s, t) = (id(&r.source), id(&r.target));
        let (sm, tm) = (r.prop("sourceMultiplicity"), r.prop("targetMultiplicity"));
        let line = match r.local() {
            "Generalization" => format!("{t} <|-- {s}"),
            "Realization" => format!("{t} <|.. {s}"),
            "Composition" => format!("{s}{} *--{} {t}", card(sm), card(tm)),
            "Aggregation" => format!("{s}{} o--{} {t}", card(sm), card(tm)),
            "Association" => {
                let op = match r.prop("navigability") {
                    "target" => "-->",
                    "source" => "<--",
                    "both" => "<-->",
                    _ => "--",
                };
                format!("{s}{} {op}{} {t}", card(sm), card(tm))
            }
            "Dependency" => format!("{s} ..> {t}"),
            "Anchor" => {
                skipped += 1;
                continue;
            }
            other => {
                warnings.push(format!("a {other} relationship was written as a dependency"));
                format!("{s} ..> {t}")
            }
        };
        let label = one_line(r.name.trim(), MAX_NAME);
        if label.is_empty() {
            let _ = writeln!(out, "  {line}");
        } else {
            let _ = writeln!(out, "  {line} : {label}");
        }
    }
    let others = p.elements.iter().filter(|e| !classes(e) && e.local() != "Package").count();
    if others > 0 || skipped > 0 {
        warnings.push(format!("{} notes and other elements have no place in a Mermaid class diagram and were left out", others + skipped));
    }
    (out, warnings)
}
