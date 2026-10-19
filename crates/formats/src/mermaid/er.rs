//! Mermaid `erDiagram` through the ER IR ([`crate::er::Schema`]): read into ER shapes ready for
//! a merging import, and written back from a schema.
//!
//! Reading: each entity is a table (its id kept, its alias as the name), each attribute a column
//! (`PK`, `FK` and `UK` set the keys, the comment is the note), and each relationship runs from
//! the left entity (the parent) to the right one (the child), `--` identifying and `..` not, the
//! two cardinalities as written. Mermaid has no foreign key targets, so no column references
//! another; classes and styles are not kept.
//!
//! Writing: entities first with their attribute blocks, then relationships with `||`, `|o`, `}|`
//! and `}o` ends, `--` for identifying and `..` for non-identifying, and the label Mermaid
//! requires (`""` when the relationship has no name). An entity whose name is not a Mermaid name
//! gets an id with the name as alias, `id["Name"]`. Characters Mermaid does not take in a type or
//! attribute name are replaced, with a warning.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::Page;
use vizcraft_mermaid::er::{Cardinality as MCard, ErDiagram, KeyKind};

use super::common::Warnings;
use crate::FormatError;
use crate::er::{self, Cardinality, Column, Ref, Schema, Table};

fn card(c: MCard) -> Cardinality {
    match c {
        MCard::ZeroOrOne => Cardinality::ZeroOrOne,
        MCard::ExactlyOne => Cardinality::One,
        MCard::ZeroOrMore => Cardinality::ZeroOrMore,
        MCard::OneOrMore => Cardinality::OneOrMore,
    }
}

/// The schema a Mermaid ER diagram says, with warnings for what it cannot hold.
pub fn schema(d: &ErDiagram, warnings: &mut Vec<String>) -> Schema {
    let mut s = Schema::default();
    for e in d.entities.iter().take(er::MAX_TABLES) {
        if !e.classes.is_empty() || !e.style.is_empty() {
            warnings.push(format!("entity {}: classes and styles are not kept", e.id));
        }
        let columns = e
            .attributes
            .iter()
            .take(er::MAX_COLUMNS)
            .map(|a| Column {
                name: er::cut(&a.name),
                data_type: er::cut(&a.ty),
                pk: a.keys.contains(&KeyKind::Primary),
                fk: a.keys.contains(&KeyKind::Foreign),
                unique: a.keys.contains(&KeyKind::Unique),
                nullable: !a.keys.contains(&KeyKind::Primary),
                note: a.comment.as_deref().map(er::cut),
                ..Column::default()
            })
            .collect();
        let name = e.alias.clone().filter(|a| !a.trim().is_empty()).unwrap_or_else(|| e.id.clone());
        s.tables.push(Table { id: Some(er::cut(&e.id)), name: er::cut(&name), columns, ..Table::default() });
    }
    for r in d.relationships.iter().take(er::MAX_REFS) {
        s.refs.push(Ref {
            name: r.label.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(er::cut),
            parent: r.left.clone(),
            child: r.right.clone(),
            source: card(r.left_cardinality),
            target: card(r.right_cardinality),
            identifying: r.identifying,
            ..Ref::default()
        });
    }
    s
}

/// Read a Mermaid ER diagram as a laid-out page of ER shapes.
pub(super) fn build(d: &ErDiagram, warnings: &mut Warnings) -> Result<Page, FormatError> {
    let mut w = Vec::new();
    let s = schema(d, &mut w);
    let mut page = er::to_page(&s, &mut w);
    super::common::fit_page(&mut page);
    for x in w {
        warnings.push(x);
    }
    Ok(page)
}

/// Whether `w` is a Mermaid entity name: letters, digits, `_` and inner `-`, not starting with a
/// digit or `-`.
fn is_name(w: &str) -> bool {
    !w.is_empty()
        && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && w.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && !w.ends_with('-')
}

/// `w` with every character outside `ok` replaced by `_`, and a leading letter when it must have
/// one; `None` when nothing changed.
fn clean(w: &str, ok: impl Fn(char) -> bool) -> Option<String> {
    let mut out: String = w.trim().chars().map(|c| if ok(c) { c } else { '_' }).collect();
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert(0, 't');
    }
    (out != w).then_some(out)
}

const LEFT: [(Cardinality, &str); 4] =
    [(Cardinality::One, "||"), (Cardinality::ZeroOrOne, "|o"), (Cardinality::OneOrMore, "}|"), (Cardinality::ZeroOrMore, "}o")];
const RIGHT: [(Cardinality, &str); 4] =
    [(Cardinality::One, "||"), (Cardinality::ZeroOrOne, "o|"), (Cardinality::OneOrMore, "|{"), (Cardinality::ZeroOrMore, "o{")];

fn marker(table: &[(Cardinality, &'static str); 4], c: Cardinality) -> &'static str {
    table.iter().find(|(k, _)| *k == c).map_or("||", |(_, m)| *m)
}

/// Write the ER elements drawn on page `page` of `doc` as a Mermaid ER diagram.
pub fn write_page(doc: &vizcraft_diagram::Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    let (s, mut warnings) = er::from_doc(doc, page)?;
    let (text, more) = write(&s);
    warnings.extend(more);
    Ok((text, warnings))
}

/// Write `schema` as a Mermaid ER diagram. The warnings say what Mermaid cannot hold.
pub fn write(schema: &Schema) -> (String, Vec<String>) {
    let mut out = String::from("erDiagram\n");
    let mut warnings = Vec::new();
    let mut ids: HashMap<&str, String> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for t in &schema.tables {
        let wanted = t.id.clone().filter(|i| is_name(i)).unwrap_or_else(|| {
            if is_name(&t.name) {
                t.name.clone()
            } else {
                clean(&t.name, |c| c.is_ascii_alphanumeric() || c == '_').unwrap_or_else(|| t.name.clone())
            }
        });
        let mut id = wanted.clone();
        let mut n = 1usize;
        while !used.insert(id.clone()) {
            n += 1;
            id = format!("{wanted}_{n}");
        }
        ids.insert(t.key(), id.clone());
        ids.insert(t.name.as_str(), id.clone());
        let head = if id == t.name { id.clone() } else { format!("{id}[\"{}\"]", t.name.replace('"', "'")) };
        if t.columns.is_empty() {
            let _ = writeln!(out, "    {head}");
            continue;
        }
        let _ = writeln!(out, "    {head} {{");
        for c in &t.columns {
            let ty_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '(' | ')' | '[' | ']' | ',');
            let ty = if c.data_type.trim().is_empty() { "string".to_string() } else { c.data_type.clone() };
            let ty = match clean(&ty, ty_ok) {
                Some(x) => {
                    warnings.push(format!("{}.{}: type {:?} written as {x}", t.name, c.name, c.data_type));
                    x
                }
                None => ty,
            };
            let name = match clean(&c.name, |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) {
                Some(x) => {
                    warnings.push(format!("{}: column {:?} written as {x}", t.name, c.name));
                    x
                }
                None => c.name.clone(),
            };
            let keys: Vec<&str> = [(c.pk, "PK"), (c.fk, "FK"), (c.unique, "UK")].into_iter().filter(|(on, _)| *on).map(|(_, k)| k).collect();
            let mut line = format!("        {ty} {name}");
            if !keys.is_empty() {
                let _ = write!(line, " {}", keys.join(", "));
            }
            if let Some(n) = c.note.as_deref().filter(|n| !n.trim().is_empty()) {
                let _ = write!(line, " \"{}\"", n.replace('"', "'").replace(['\n', '\r'], " "));
            }
            let _ = writeln!(out, "{line}");
        }
        out.push_str("    }\n");
    }
    for r in &schema.refs {
        let (Some(a), Some(b)) = (ids.get(r.parent.as_str()), ids.get(r.child.as_str())) else {
            warnings.push(format!("a relationship joins {} and {}, which are not both tables", r.parent, r.child));
            continue;
        };
        let line = if r.identifying { "--" } else { ".." };
        let label = r.name.as_deref().unwrap_or_default().replace('"', "'").replace(['\n', '\r'], " ");
        let _ = writeln!(out, "    {a} {}{line}{} {b} : \"{label}\"", marker(&LEFT, r.source), marker(&RIGHT, r.target));
    }
    if schema.refs.iter().any(|r| !r.child_columns.is_empty()) {
        warnings.push("Mermaid ER diagrams cannot say which column a foreign key references".into());
    }
    if !schema.enums.is_empty() || !schema.groups.is_empty() {
        warnings.push("Mermaid ER diagrams have no enums or table groups; they were left out".into());
    }
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Schema {
        let parsed = vizcraft_mermaid::parse(text).unwrap();
        let vizcraft_mermaid::Diagram::Er(d) = &parsed.diagram else { panic!("not an ER diagram") };
        schema(d, &mut Vec::new())
    }

    fn golden(name: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/mermaid_er");
        let src = std::fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap();
        let (out, _) = write(&read(&src));
        let path = dir.join(format!("{name}.golden.mmd"));
        if std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, &out).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap();
        assert_eq!(out, want, "{name}: write(read(x)) differs from the golden");
        assert_eq!(write(&read(&want)).0, want, "{name}: the golden is not a fixpoint");
    }

    #[test]
    fn orders_round_trip_to_their_golden() {
        golden("orders");
    }

    #[test]
    fn aliases_and_odd_types_round_trip_to_their_golden() {
        golden("aliases");
    }

    #[test]
    fn names_mermaid_refuses_are_aliased_and_odd_types_replaced_with_a_warning() {
        let col = |n: &str, t: &str| Column { name: n.into(), data_type: t.into(), ..Column::default() };
        let s = Schema {
            tables: vec![Table { name: "Order Line".into(), columns: vec![col("unit price", "\"character varying\"")], ..Table::default() }],
            ..Schema::default()
        };
        let (out, w) = write(&s);
        assert!(out.contains("Order_Line[\"Order Line\"] {"), "{out}");
        assert!(out.contains("_character_varying_ unit_price") || out.contains("t_character_varying_ unit_price"), "{out}");
        assert_eq!(w.len(), 2, "{w:?}");
    }

    #[test]
    fn the_reader_never_panics_on_truncated_or_garbage_text() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/mermaid_er");
        let src = std::fs::read_to_string(dir.join("orders.mmd")).unwrap();
        for end in (0..src.len()).filter(|i| src.is_char_boundary(*i)) {
            let _ = crate::mermaid::import(src.get(..end).unwrap_or_default());
        }
        for junk in ["erDiagram\n  A ||--o{", "erDiagram\n  A {\n", "erDiagram\n  A }o..o{ B : x\n  A {\n string\n }", "erDiagram\n\u{0}é{{{"] {
            let _ = crate::mermaid::import(junk);
        }
    }
}
