//! The entity-relationship IR that the DBML and Mermaid ER formats read into and write from, and
//! the two bridges to drawings: [`to_page`] draws a schema as a page of ER shapes ready for a
//! merging import, and [`from_doc`] reads the ER elements drawn on a page back into a schema.
//!
//! Direction follows the ER pack: a relationship runs from the parent (the referenced table) to
//! the child (the table holding the foreign key), `source` cardinality at the parent's end and
//! `target` at the child's. A formats-only round trip (DBML in, DBML out) keeps everything the IR
//! holds; what a drawing cannot hold comes back as warnings.

use std::collections::{BTreeMap, HashMap, HashSet};

use vizcraft_diagram::compartment::{self as cp, Column as CpColumn, Compartment, CompartmentKind};
use vizcraft_diagram::vizcraft_model::{self, Element, PropValue};
use vizcraft_diagram::{Connector, Document, End, Page, Route, Shape, ShapeKind, is_plain_id};

use crate::csv::{PROP_PREFIX, TYPE_KEY};

/// `ext` key on an imported entity shape: its columns as a JSON list of
/// `{"id": "<table>.<column>", "type": "er.Attribute", "name", "props": {key: text}}`, which a
/// merging import turns into elements the entity owns.
pub const CHILDREN_KEY: &str = "import.children";

/// Most tables a schema holds.
pub const MAX_TABLES: usize = 5_000;
/// Most columns one table holds.
pub const MAX_COLUMNS: usize = 256;
/// Most relationships a schema holds.
pub const MAX_REFS: usize = 20_000;
/// Most enums a schema holds.
pub const MAX_ENUMS: usize = 1_000;
/// Most values one enum holds.
pub const MAX_VALUES: usize = 1_000;
/// Longest name, type, note or default kept, in bytes.
pub const MAX_TEXT: usize = 4 * 1024;

/// The cardinality at one end of a relationship.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cardinality {
    /// Exactly one.
    #[default]
    One,
    /// Zero or one.
    ZeroOrOne,
    /// One or more.
    OneOrMore,
    /// Zero or more.
    ZeroOrMore,
}

impl Cardinality {
    /// The ER pack's property value.
    pub fn name(self) -> &'static str {
        match self {
            Cardinality::One => "one",
            Cardinality::ZeroOrOne => "zeroOrOne",
            Cardinality::OneOrMore => "oneOrMore",
            Cardinality::ZeroOrMore => "zeroOrMore",
        }
    }
    /// The value for a property value, `None` for anything else.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim() {
            "one" => Some(Cardinality::One),
            "zeroOrOne" => Some(Cardinality::ZeroOrOne),
            "oneOrMore" => Some(Cardinality::OneOrMore),
            "zeroOrMore" => Some(Cardinality::ZeroOrMore),
            _ => None,
        }
    }
    /// Whether more than one is allowed.
    pub fn many(self) -> bool {
        matches!(self, Cardinality::OneOrMore | Cardinality::ZeroOrMore)
    }
    /// Whether none is allowed.
    pub fn optional(self) -> bool {
        matches!(self, Cardinality::ZeroOrOne | Cardinality::ZeroOrMore)
    }
}

/// A column.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// Column name.
    pub name: String,
    /// Data type as written (`varchar(255)`).
    pub data_type: String,
    /// Part of the primary key.
    pub pk: bool,
    /// Part of a foreign key.
    pub fk: bool,
    /// Unique.
    pub unique: bool,
    /// Auto increment.
    pub increment: bool,
    /// May be null (DBML's default).
    pub nullable: bool,
    /// Default value as written (`'active'`, `` `now()` ``, `1`).
    pub default: Option<String>,
    /// Note.
    pub note: Option<String>,
    /// The column it references, `table.column`.
    pub references: Option<String>,
}

impl Default for Column {
    fn default() -> Self {
        Column {
            name: String::new(),
            data_type: String::new(),
            pk: false,
            fk: false,
            unique: false,
            increment: false,
            nullable: true,
            default: None,
            note: None,
            references: None,
        }
    }
}

/// A table (an entity).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// The id the source gave it (a Mermaid entity id); `None`: the name is the id.
    pub id: Option<String>,
    /// Table name.
    pub name: String,
    /// Schema; `None` for the default.
    pub schema: Option<String>,
    /// Alias (`Table users as U`).
    pub alias: Option<String>,
    /// Note.
    pub note: Option<String>,
    /// Columns in order.
    pub columns: Vec<Column>,
    /// Index definitions as written inside DBML's `indexes { }`, one per entry.
    pub indexes: Vec<String>,
    /// Weak: identified through a parent (the target of an identifying relationship).
    pub weak: bool,
}

impl Table {
    /// The id shapes and references use: the source id, else the name.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.name)
    }
}

/// A relationship from a parent table to a child table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ref {
    /// Name; `None` for none.
    pub name: Option<String>,
    /// Parent table (its key).
    pub parent: String,
    /// The referenced columns of the parent.
    pub parent_columns: Vec<String>,
    /// Child table (its key).
    pub child: String,
    /// The foreign key columns of the child.
    pub child_columns: Vec<String>,
    /// Cardinality at the parent's end.
    pub source: Cardinality,
    /// Cardinality at the child's end.
    pub target: Cardinality,
    /// Identifying: the child is identified through the parent.
    pub identifying: bool,
    /// DBML `delete:` action as written (`cascade`, `set null`).
    pub on_delete: Option<String>,
    /// DBML `update:` action as written.
    pub on_update: Option<String>,
}

/// An enum.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Enum {
    /// Name.
    pub name: String,
    /// Schema; `None` for the default.
    pub schema: Option<String>,
    /// Values with their notes.
    pub values: Vec<(String, Option<String>)>,
}

/// A table group.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    /// Name.
    pub name: String,
    /// Member table names.
    pub tables: Vec<String>,
}

/// A schema: what a DBML file or a Mermaid ER diagram says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schema {
    /// Tables in order.
    pub tables: Vec<Table>,
    /// Relationships in order.
    pub refs: Vec<Ref>,
    /// Enums in order.
    pub enums: Vec<Enum>,
    /// Table groups in order.
    pub groups: Vec<Group>,
}

impl Schema {
    /// The table with this key (or name).
    pub fn table(&self, key: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.key() == key).or_else(|| self.tables.iter().find(|t| t.name == key))
    }
}

/// `t` cut to [`MAX_TEXT`] at a character boundary.
pub(crate) fn cut(t: &str) -> String {
    let mut end = t.len().min(MAX_TEXT);
    while end > 0 && !t.is_char_boundary(end) {
        end -= 1;
    }
    t.get(..end).unwrap_or_default().to_string()
}

// ------------------------------------------------------------------------------- to a page

/// ER entity master.
const ENTITY: &str = "er.entity";
/// ER weak entity master.
const WEAK_ENTITY: &str = "er.weakEntity";
/// ER enum master.
const ENUM: &str = "er.enum";
/// Space around the drawing.
const MARGIN: f64 = 40.0;

/// The key compartments an imported entity shows before a merge binds it (the IE drawing).
fn preview(table: &Table) -> Vec<Compartment> {
    let cols = |titles: [&str; 3]| -> Vec<CpColumn> {
        titles.iter().zip([40.0, 110.0, 90.0]).map(|(t, w)| CpColumn { title: (*t).to_string(), width: w }).collect()
    };
    let row = |c: &Column, mark: &str| format!("{mark}\t{}\t{}", c.name, c.data_type).replace(['\n', '\r'], " ");
    let mut keys = Compartment::new("keys", CompartmentKind::List);
    keys.columns = cols(["Key", "Column", "Type"]);
    keys.rows = table.columns.iter().filter(|c| c.pk).map(|c| row(c, "PK")).take(cp::MAX_ROWS).collect();
    let mut attrs = Compartment::new("attrs", CompartmentKind::List);
    attrs.columns = cols(["", "Column", "Type"]);
    attrs.rows = table.columns.iter().filter(|c| !c.pk).map(|c| row(c, if c.fk { "FK" } else { "" })).take(cp::MAX_ROWS).collect();
    let mut out = vec![keys, attrs];
    for c in &mut out {
        c.sanitize();
    }
    out
}

/// A column's properties as `import.children` writes them.
fn column_props(c: &Column) -> BTreeMap<String, String> {
    let mut p = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        p.insert(k.to_string(), v);
    };
    if !c.data_type.is_empty() {
        put("dataType", c.data_type.clone());
    }
    put("pk", c.pk.to_string());
    put("fk", c.fk.to_string());
    put("unique", c.unique.to_string());
    put("increment", c.increment.to_string());
    put("nullable", c.nullable.to_string());
    if let Some(d) = &c.default {
        put("default", d.clone());
    }
    if let Some(n) = &c.note {
        put("note", n.clone());
    }
    if let Some(r) = &c.references {
        put("references", r.clone());
    }
    p
}

/// A shape from master `master`, or a plain rectangle when the master is unknown.
fn shape_from(master: &str, id: &str) -> Shape {
    vizcraft_stencils::master(master)
        .map(|m| m.instantiate(id.to_string(), 0.0, 0.0, None, None))
        .unwrap_or_else(|| Shape::new(id.to_string(), ShapeKind::Rectangle, 0.0, 0.0, 200.0, 40.0))
}

/// Set `ext[import.prop.<key>]` when `value` is not empty.
fn prop(ext: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
        ext.insert(format!("{PROP_PREFIX}{key}"), v.to_string());
    }
}

/// An id for `wanted` that is plain and unused, else `<prefix><n>`.
fn claim(wanted: &str, prefix: &str, used: &mut HashSet<String>) -> String {
    if is_plain_id(wanted) && !used.contains(wanted) {
        used.insert(wanted.to_string());
        return wanted.to_string();
    }
    let mut n = used.len();
    loop {
        n = n.saturating_add(1);
        let id = format!("{prefix}{n}");
        if used.insert(id.clone()) {
            return id;
        }
    }
}

/// Draw `schema` as a page: an entity shape per table (stencil `er.entity`, or `er.weakEntity`
/// for a weak one; the table name as its id when it is a plain id), carrying its type, properties
/// and columns in `ext` for a merging import and preview compartments so a plain import draws;
/// an enum shape per enum; a glued connector per relationship with its type and cardinalities;
/// all laid out with the er layout primitive.
pub fn to_page(schema: &Schema, warnings: &mut Vec<String>) -> Page {
    let mut page = Page::new("p1", "ER");
    let mut used: HashSet<String> = HashSet::new();
    let mut shape_of: HashMap<&str, String> = HashMap::new();
    let weak: HashSet<&str> = schema.refs.iter().filter(|r| r.identifying).map(|r| r.child.as_str()).collect();
    let groups: HashMap<&str, &str> = schema.groups.iter().flat_map(|g| g.tables.iter().map(move |t| (t.as_str(), g.name.as_str()))).collect();
    for t in &schema.tables {
        let is_weak = t.weak || weak.contains(t.key());
        let id = claim(t.key(), "t", &mut used);
        let mut s = shape_from(if is_weak { WEAK_ENTITY } else { ENTITY }, &id);
        s.text = t.name.clone();
        s.ext.insert(TYPE_KEY.into(), if is_weak { "er.WeakEntity" } else { "er.Entity" }.into());
        prop(&mut s.ext, "schema", t.schema.as_deref());
        prop(&mut s.ext, "alias", t.alias.as_deref());
        prop(&mut s.ext, "note", t.note.as_deref());
        let indexes = t.indexes.join("\n");
        prop(&mut s.ext, "indexes", Some(&indexes));
        prop(&mut s.ext, "group", groups.get(t.name.as_str()).or_else(|| groups.get(t.key())).copied());
        let children: Vec<serde_json::Value> = t
            .columns
            .iter()
            .map(|c| serde_json::json!({"id": format!("{id}.{}", c.name), "type": "er.Attribute", "name": c.name, "props": column_props(c)}))
            .collect();
        let json = serde_json::Value::Array(children).to_string();
        if json.len() <= vizcraft_model::MAX_TEXT {
            s.ext.insert(CHILDREN_KEY.into(), json);
        } else {
            warnings.push(format!("table {} has too many columns to carry into the model; its columns were left out", t.name));
        }
        s.compartments = preview(t);
        s.w = s.w.max(240.0);
        s.h = cp::fit_height(&s).max(s.h);
        s.sanitize();
        shape_of.insert(t.key(), id);
        page.shapes.push(s);
    }
    for e in &schema.enums {
        let id = claim(&e.name, "enum", &mut used);
        let mut s = shape_from(ENUM, &id);
        s.text = e.name.clone();
        s.ext.insert(TYPE_KEY.into(), "er.Enum".into());
        let values: Vec<&str> = e.values.iter().map(|(v, _)| v.as_str()).collect();
        prop(&mut s.ext, "values", Some(&values.join("\n")));
        prop(&mut s.ext, "schema", e.schema.as_deref());
        if e.values.iter().any(|(_, n)| n.is_some()) {
            warnings.push(format!("enum {}: value notes are not drawn", e.name));
        }
        let mut c = Compartment::new("values", CompartmentKind::List);
        c.rows = values.iter().map(|v| (*v).to_string()).take(cp::MAX_ROWS).collect();
        c.sanitize();
        s.compartments = vec![c];
        s.h = cp::fit_height(&s).max(s.h);
        s.sanitize();
        page.shapes.push(s);
    }
    for (i, r) in schema.refs.iter().enumerate() {
        let (Some(a), Some(b)) = (shape_of.get(r.parent.as_str()), shape_of.get(r.child.as_str())) else {
            warnings.push(format!("a relationship joins {} and {}, which are not both tables; it was left out", r.parent, r.child));
            continue;
        };
        let id = claim(&format!("ref{}", i.saturating_add(1)), "ref", &mut used);
        let mut c = Connector::new(id, End::dynamic(a.clone()), End::dynamic(b.clone()));
        let (kind, ty) = if r.identifying { ("er.identifying", "er.Identifying") } else { ("er.relationship", "er.Relationship") };
        if let Some(k) = vizcraft_stencils::connector_kind(kind) {
            k.apply(&mut c);
        }
        c.route = Route::Orthogonal;
        c.start_arrow = ie_arrow(r.source);
        c.end_arrow = ie_arrow(r.target);
        c.text = r.name.clone().unwrap_or_default();
        c.ext.insert(TYPE_KEY.into(), ty.into());
        prop(&mut c.ext, "sourceCardinality", Some(r.source.name()));
        prop(&mut c.ext, "targetCardinality", Some(r.target.name()));
        prop(&mut c.ext, "onDelete", r.on_delete.as_deref().map(action_value).as_deref());
        prop(&mut c.ext, "onUpdate", r.on_update.as_deref().map(action_value).as_deref());
        c.sanitize();
        page.connectors.push(c);
    }
    lay_out(&mut page, warnings);
    page
}

/// The ER pack's value for a DBML referential action (`set null` is `setNull`).
pub fn action_value(a: &str) -> String {
    let mut out = String::new();
    for (i, w) in a.split_whitespace().enumerate() {
        let w = w.to_ascii_lowercase();
        if i == 0 {
            out.push_str(&w);
        } else {
            let mut cs = w.chars();
            if let Some(f) = cs.next() {
                out.extend(f.to_uppercase());
                out.push_str(cs.as_str());
            }
        }
    }
    out
}

/// DBML's words for an ER pack action value (`setNull` is `set null`).
pub fn action_words(v: &str) -> String {
    let mut out = String::new();
    for c in v.trim().chars() {
        if c.is_ascii_uppercase() {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The crow's foot end for a cardinality (the IE drawing).
fn ie_arrow(c: Cardinality) -> vizcraft_diagram::Arrow {
    use vizcraft_diagram::Arrow;
    match c {
        Cardinality::One => Arrow::DoubleBar,
        Cardinality::ZeroOrOne => Arrow::BarCircle,
        Cardinality::OneOrMore => Arrow::CrowsFootBar,
        Cardinality::ZeroOrMore => Arrow::CrowsFootCircle,
    }
}

/// Place the shapes with the er layout primitive, parents above children.
fn lay_out(page: &mut Page, warnings: &mut Vec<String>) {
    use vizcraft_layout::{Direction, Edge, Graph, Node, Options, Primitive};
    if page.shapes.is_empty() {
        return;
    }
    let index: HashMap<&str, usize> = page.shapes.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    let graph = Graph {
        nodes: page.shapes.iter().map(|s| Node::new(s.w, s.h)).collect(),
        edges: page.connectors.iter().filter_map(|c| Some(Edge::new(*index.get(c.start.shape()?)?, *index.get(c.end.shape()?)?))).collect(),
        groups: Vec::new(),
    };
    let opts = Options { direction: Direction::TopBottom, ..Options::default() };
    match vizcraft_layout::layout_primitive(&graph, &Primitive::Er { satellites: Vec::new() }, &opts) {
        Ok(out) => {
            for (s, p) in page.shapes.iter_mut().zip(&out.nodes) {
                s.x = p.x + MARGIN;
                s.y = p.y + MARGIN;
                s.sanitize();
            }
        }
        Err(e) => {
            warnings.push(format!("the tables were not laid out: {e}"));
            let mut y = MARGIN;
            for s in &mut page.shapes {
                (s.x, s.y) = (MARGIN, y);
                y += s.h + 40.0;
                s.sanitize();
            }
        }
    }
}

// ------------------------------------------------------------------------------- from a page

/// Whether an element type is an ER table.
fn is_table(t: &str) -> bool {
    matches!(t, "er.Entity" | "er.WeakEntity")
}

fn text(e: &Element, key: &str) -> Option<String> {
    e.prop(key).map(|v| v.display()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn flag(e: &Element, key: &str, default: bool) -> bool {
    match e.prop(key) {
        Some(PropValue::Bool(b)) => *b,
        Some(PropValue::Text(t)) => t.trim().eq_ignore_ascii_case("true"),
        _ => default,
    }
}

/// Read the ER elements drawn on page `page` of `doc` as a schema: each table drawn there with
/// the columns it owns (model order), the enums drawn there, the relationships between drawn
/// tables (their foreign key columns found by `references`), foreign keys with no drawn
/// relationship as relationships of their own, and table groups from the tables' `group`.
pub fn from_doc(doc: &Document, page: usize) -> Result<(Schema, Vec<String>), crate::FormatError> {
    let p = doc.pages.get(page).ok_or(crate::FormatError::NoPage(page))?;
    let mut warnings = Vec::new();
    let model = &doc.model;
    let by_id: HashMap<&str, &Element> = model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let drawn: Vec<&Element> =
        p.shapes.iter().filter_map(|s| s.element.as_deref()).filter_map(|id| by_id.get(id).copied()).filter(|e| seen.insert(e.id.as_str())).collect();
    let mut children: HashMap<&str, Vec<&Element>> = HashMap::new();
    for e in model.elements.iter().filter(|e| e.type_id == "er.Attribute") {
        if let Some(o) = e.owner.as_deref() {
            children.entry(o).or_default().push(e);
        }
    }
    let mut schema = Schema::default();
    let mut groups: Vec<Group> = Vec::new();
    let mut table_of: HashMap<&str, usize> = HashMap::new();
    for e in drawn.iter().filter(|e| is_table(&e.type_id)).take(MAX_TABLES) {
        let mut t = Table {
            id: None,
            name: if e.name.trim().is_empty() { e.id.clone() } else { e.name.trim().to_string() },
            schema: text(e, "schema"),
            alias: text(e, "alias"),
            note: text(e, "note"),
            indexes: text(e, "indexes").map(|i| i.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            weak: e.type_id == "er.WeakEntity",
            ..Table::default()
        };
        for c in children.get(e.id.as_str()).map(Vec::as_slice).unwrap_or_default().iter().take(MAX_COLUMNS) {
            t.columns.push(Column {
                name: c.name.trim().to_string(),
                data_type: text(c, "dataType").unwrap_or_default(),
                pk: flag(c, "pk", false),
                fk: flag(c, "fk", false),
                unique: flag(c, "unique", false),
                increment: flag(c, "increment", false),
                nullable: flag(c, "nullable", true),
                default: text(c, "default"),
                note: text(c, "note"),
                references: text(c, "references"),
            });
        }
        if let Some(g) = text(e, "group") {
            match groups.iter_mut().find(|x| x.name == g) {
                Some(x) => x.tables.push(t.name.clone()),
                None => groups.push(Group { name: g, tables: vec![t.name.clone()] }),
            }
        }
        table_of.insert(e.id.as_str(), schema.tables.len());
        schema.tables.push(t);
    }
    for e in drawn.iter().filter(|e| e.type_id == "er.Enum").take(MAX_ENUMS) {
        let values = text(e, "values")
            .map(|v| v.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| (l.to_string(), None)).collect())
            .unwrap_or_default();
        schema.enums.push(Enum { name: e.name.trim().to_string(), schema: text(e, "schema"), values });
    }
    // Relationships between drawn tables, with the child columns that reference the parent.
    let mut covered: HashSet<(usize, String)> = HashSet::new();
    for r in model.relationships.iter().filter(|r| matches!(r.type_id.as_str(), "er.Relationship" | "er.Identifying")) {
        let (Some(&pi), Some(&ci)) = (table_of.get(r.source.as_str()), table_of.get(r.target.as_str())) else { continue };
        let (Some(parent), Some(child)) = (schema.tables.get(pi), schema.tables.get(ci)) else { continue };
        let card = |k: &str, d: Cardinality| r.prop(k).map(|v| v.display()).and_then(|v| Cardinality::from_name(&v)).unwrap_or(d);
        let mut out = Ref {
            name: Some(r.name.trim().to_string()).filter(|n| !n.is_empty()),
            parent: parent.name.clone(),
            child: child.name.clone(),
            source: card("sourceCardinality", Cardinality::One),
            target: card("targetCardinality", Cardinality::ZeroOrMore),
            identifying: r.type_id == "er.Identifying",
            on_delete: r.prop("onDelete").map(|v| v.display()).filter(|v| !v.trim().is_empty()).map(|v| action_words(&v)),
            on_update: r.prop("onUpdate").map(|v| v.display()).filter(|v| !v.trim().is_empty()).map(|v| action_words(&v)),
            ..Ref::default()
        };
        for c in &child.columns {
            let Some((t, col)) = c.references.as_deref().and_then(|x| x.rsplit_once('.')) else { continue };
            if t.trim() == parent.name || parent.alias.as_deref() == Some(t.trim()) {
                out.child_columns.push(c.name.clone());
                out.parent_columns.push(col.trim().to_string());
            }
        }
        if out.child_columns.is_empty() {
            warnings.push(format!("the relationship from {} to {} names no foreign key column", parent.name, child.name));
        }
        covered.insert((ci, parent.name.clone()));
        schema.refs.push(out);
    }
    // Foreign keys whose relationship is not drawn.
    let names: HashSet<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
    let mut extra: Vec<Ref> = Vec::new();
    for (ci, child) in schema.tables.iter().enumerate() {
        let mut by_parent: Vec<(String, Vec<String>, Vec<String>, bool)> = Vec::new();
        for c in &child.columns {
            let Some((t, col)) = c.references.as_deref().and_then(|x| x.rsplit_once('.')) else { continue };
            let t = t.trim().to_string();
            if covered.contains(&(ci, t.clone())) || !names.contains(t.as_str()) {
                continue;
            }
            match by_parent.iter_mut().find(|(p, ..)| *p == t) {
                Some((_, cc, pc, nullable)) => {
                    cc.push(c.name.clone());
                    pc.push(col.trim().to_string());
                    *nullable |= c.nullable;
                }
                None => by_parent.push((t, vec![c.name.clone()], vec![col.trim().to_string()], c.nullable)),
            }
        }
        for (parent, child_columns, parent_columns, nullable) in by_parent {
            let identifying = child_columns.iter().all(|n| child.columns.iter().any(|c| &c.name == n && c.pk));
            extra.push(Ref {
                name: None,
                parent,
                parent_columns,
                child: child.name.clone(),
                child_columns,
                source: if nullable { Cardinality::ZeroOrOne } else { Cardinality::One },
                target: Cardinality::ZeroOrMore,
                identifying,
                on_delete: None,
                on_update: None,
            });
        }
    }
    schema.refs.extend(extra);
    schema.groups = groups;
    Ok((schema, warnings))
}

/// Whether `child`'s `columns` are all part of its primary key (an identifying relationship).
pub(crate) fn all_in_pk(child: &Table, columns: &[String]) -> bool {
    !columns.is_empty() && columns.iter().all(|n| child.columns.iter().any(|c| &c.name == n && c.pk))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shop() -> Schema {
        let col = |name: &str, ty: &str, pk: bool| Column { name: name.into(), data_type: ty.into(), pk, nullable: !pk, ..Column::default() };
        let mut orders = Table { name: "orders".into(), columns: vec![col("id", "int", true), col("user_id", "int", false)], ..Table::default() };
        orders.columns[1].fk = true;
        orders.columns[1].references = Some("users.id".into());
        Schema {
            tables: vec![Table { name: "users".into(), columns: vec![col("id", "int", true)], ..Table::default() }, orders],
            refs: vec![Ref {
                parent: "users".into(),
                parent_columns: vec!["id".into()],
                child: "orders".into(),
                child_columns: vec!["user_id".into()],
                source: Cardinality::ZeroOrOne,
                target: Cardinality::ZeroOrMore,
                ..Ref::default()
            }],
            ..Schema::default()
        }
    }

    #[test]
    fn a_schema_draws_entities_with_their_columns_and_glued_typed_connectors() {
        let mut w = Vec::new();
        let page = to_page(&shop(), &mut w);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(page.shapes.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["users", "orders"]);
        let orders = &page.shapes[1];
        assert_eq!((orders.stencil.as_str(), orders.ext.get(TYPE_KEY).map(String::as_str)), ("er.entity", Some("er.Entity")));
        let kids: serde_json::Value = serde_json::from_str(orders.ext.get(CHILDREN_KEY).unwrap()).unwrap();
        assert_eq!(kids[1]["id"], "orders.user_id");
        assert_eq!(kids[1]["props"]["references"], "users.id");
        assert_eq!(orders.compartments[1].rows, ["FK\tuser_id\tint"]);
        let c = &page.connectors[0];
        assert_eq!((c.start.shape(), c.end.shape()), (Some("users"), Some("orders")));
        assert_eq!(c.ext.get("import.prop.sourceCardinality").map(String::as_str), Some("zeroOrOne"));
        assert!(page.shapes[0].y + page.shapes[0].h < orders.y, "the parent sits above the child");
    }

    #[test]
    fn actions_convert_between_dbml_words_and_pack_values() {
        assert_eq!(action_value("set null"), "setNull");
        assert_eq!(action_value("NO ACTION"), "noAction");
        assert_eq!(action_words("setDefault"), "set default");
        assert_eq!(action_words("cascade"), "cascade");
    }
}
