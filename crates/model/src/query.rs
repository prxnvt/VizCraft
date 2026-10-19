//! Tables derived from the model: a list of elements (or relationships) with their properties, and
//! a matrix of the relationships between two sets of elements.
//!
//! Both are plain reads, deterministic (model order) and capped, so a hostile or huge model cannot
//! make a table grow without bound. The model knows nothing about packs beyond the type prefix, so
//! a caller that wants supertypes covered passes every type id to match ([`Selector::Types`]).

use std::collections::{BTreeMap, HashMap};

use crate::{Element, Model, PropValue, Relationship};

/// Most rows a list or a matrix holds; past it the result says `truncated`.
pub const MAX_ROWS: usize = 10_000;

/// Most columns a matrix holds; past it the result says `truncated`.
pub const MAX_COLS: usize = 256;

/// Which elements (or relationships) a query takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selector {
    /// Every one.
    #[default]
    All,
    /// Those whose type is one of these pack-qualified ids.
    Types(Vec<String>),
    /// Those whose type belongs to this pack.
    Pack(String),
}

impl Selector {
    /// Whether a thing of `type_id` is taken.
    pub fn matches(&self, type_id: &str) -> bool {
        match self {
            Selector::All => true,
            Selector::Types(t) => t.iter().any(|x| x == type_id),
            Selector::Pack(p) => crate::pack_of(type_id) == p,
        }
    }
}

/// What a list is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// One row per element.
    Elements,
    /// One row per relationship.
    Relationships,
}

/// A list: named columns and one row of cell text per element or relationship.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// The column names: fixed ones (`id`, `tag`, `name`, `type`, `source`, `target`) or property keys.
    pub columns: Vec<String>,
    /// The id of the item each row shows.
    pub ids: Vec<String>,
    /// The cells, every row as long as `columns`.
    pub rows: Vec<Vec<String>>,
    /// The cap cut rows off.
    pub truncated: bool,
}

/// The columns a list shows when none are named: the fixed ones of the subject, `type` when more
/// than one type can appear, then `props`.
pub fn default_columns(subject: Subject, sel: &Selector, props: &[String]) -> Vec<String> {
    let mut out: Vec<String> = match subject {
        Subject::Elements => vec!["id".into(), "tag".into(), "name".into()],
        Subject::Relationships => vec!["id".into(), "name".into(), "source".into(), "target".into()],
    };
    if !matches!(sel, Selector::Types(t) if t.len() == 1) {
        out.insert(1, "type".into());
    }
    for p in props {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

/// The property keys in use on the selected items, sorted: the columns of a list over a selector
/// whose declared properties the caller does not know.
pub fn keys_in_use(model: &Model, subject: Subject, sel: &Selector) -> Vec<String> {
    let mut keys: Vec<String> = match subject {
        Subject::Elements => model.elements.iter().filter(|e| sel.matches(&e.type_id)).flat_map(|e| e.properties.keys().cloned()).collect(),
        Subject::Relationships => {
            model.relationships.iter().filter(|r| sel.matches(&r.type_id)).flat_map(|r| r.properties.keys().cloned()).collect()
        }
    };
    keys.sort();
    keys.dedup();
    keys
}

/// What an element or relationship is called in a table: its name, else its id.
pub fn label_of(model: &Model, id: &str) -> String {
    match model.element(id) {
        Some(e) if !e.name.is_empty() => e.name.clone(),
        _ => id.to_string(),
    }
}

fn element_cell(e: &Element, col: &str) -> String {
    match col {
        "id" => e.id.clone(),
        "tag" => e.tag.clone(),
        "name" => e.name.clone(),
        "type" => e.type_id.clone(),
        k => e.prop(k).map(PropValue::display).unwrap_or_default(),
    }
}

fn relationship_cell(model: &Model, r: &Relationship, col: &str) -> String {
    match col {
        "id" => r.id.clone(),
        "name" => r.name.clone(),
        "type" => r.type_id.clone(),
        "source" => label_of(model, &r.source),
        "target" => label_of(model, &r.target),
        k => r.prop(k).map(PropValue::display).unwrap_or_default(),
    }
}

/// The list of the selected elements or relationships. `columns` names the columns (any fixed
/// name or property key); empty takes [`default_columns`] over `props`. `filter` keeps the rows
/// with a cell containing the text, ignoring case; empty keeps all.
pub fn list(model: &Model, subject: Subject, sel: &Selector, columns: &[String], props: &[String], filter: &str) -> Table {
    let columns: Vec<String> = if columns.is_empty() { default_columns(subject, sel, props) } else { columns.to_vec() };
    let needle = filter.trim().to_lowercase();
    let mut t = Table { columns, ..Table::default() };
    let push = |t: &mut Table, id: &str, row: Vec<String>| -> bool {
        if !needle.is_empty() && !row.iter().any(|c| c.to_lowercase().contains(&needle)) {
            return true;
        }
        if t.rows.len() >= MAX_ROWS {
            t.truncated = true;
            return false;
        }
        t.ids.push(id.to_string());
        t.rows.push(row);
        true
    };
    match subject {
        Subject::Elements => {
            for e in model.elements.iter().filter(|e| sel.matches(&e.type_id)) {
                let row = t.columns.iter().map(|c| element_cell(e, c)).collect();
                if !push(&mut t, &e.id, row) {
                    break;
                }
            }
        }
        Subject::Relationships => {
            for r in model.relationships.iter().filter(|r| sel.matches(&r.type_id)) {
                let row = t.columns.iter().map(|c| relationship_cell(model, r, c)).collect();
                if !push(&mut t, &r.id, row) {
                    break;
                }
            }
        }
    }
    t
}

/// One row or column of a matrix: an element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis {
    /// The element id.
    pub id: String,
    /// What the header shows: the element's name, else its id.
    pub label: String,
}

/// What sits at a row and column of a matrix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The relationships from the row's element to the column's.
    pub relationships: Vec<String>,
    /// The cell text: `x` for a link, or the named property's values when there is a property.
    pub text: String,
}

/// A matrix of the relationships from each row element to each column element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matrix {
    /// The row elements, in model order.
    pub rows: Vec<Axis>,
    /// The column elements, in model order.
    pub cols: Vec<Axis>,
    /// The cells that hold at least one relationship, by (row, column) index.
    pub cells: BTreeMap<(usize, usize), Cell>,
    /// The caps cut rows or columns off.
    pub truncated: bool,
}

impl Matrix {
    /// The cell at `row`, `col`, if any relationship joins the pair.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(&(row, col))
    }
}

/// The matrix of the elements `rows` selects against those `cols` selects. A cell holds the
/// relationships from the row's element to the column's, of type `relationship` when given; with
/// `property` its text is the values of that property across them, else `x`.
pub fn matrix(model: &Model, rows: &Selector, cols: &Selector, relationship: Option<&str>, property: Option<&str>) -> Matrix {
    let mut m = Matrix::default();
    let mut row_at: HashMap<&str, usize> = HashMap::new();
    let mut col_at: HashMap<&str, usize> = HashMap::new();
    for e in &model.elements {
        let axis = || Axis { id: e.id.clone(), label: label_of(model, &e.id) };
        if rows.matches(&e.type_id) {
            if m.rows.len() < MAX_ROWS {
                row_at.insert(e.id.as_str(), m.rows.len());
                m.rows.push(axis());
            } else {
                m.truncated = true;
            }
        }
        if cols.matches(&e.type_id) {
            if m.cols.len() < MAX_COLS {
                col_at.insert(e.id.as_str(), m.cols.len());
                m.cols.push(axis());
            } else {
                m.truncated = true;
            }
        }
    }
    for r in model.relationships.iter().filter(|r| relationship.is_none_or(|t| r.type_id == t)) {
        let (Some(&i), Some(&j)) = (row_at.get(r.source.as_str()), col_at.get(r.target.as_str())) else { continue };
        let cell = m.cells.entry((i, j)).or_insert_with(|| Cell { relationships: Vec::new(), text: String::new() });
        cell.relationships.push(r.id.clone());
        match property {
            None => cell.text = "x".into(),
            Some(k) => {
                let v = r.prop(k).map(PropValue::display).unwrap_or_default();
                if !v.is_empty() {
                    if !cell.text.is_empty() {
                        cell.text.push_str(", ");
                    }
                    cell.text.push_str(&v);
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Model {
        let mut m = Model::new();
        for (id, ty, name) in [("a", "flowchart.Process", "Gather"), ("b", "flowchart.Process", "Save"), ("c", "flowchart.Decision", "Valid?")] {
            let mut e = Element::new(id, ty);
            e.name = name.into();
            if id == "a" {
                e.properties.insert("owner".into(), PropValue::Text("Ana".into()));
            }
            m.add_element(e).unwrap();
        }
        let mut r = Relationship::new("r1", "flowchart.Flow", "a", "b");
        r.properties.insert("role".into(), PropValue::Text("R".into()));
        m.add_relationship(r).unwrap();
        let mut r = Relationship::new("r2", "flowchart.Flow", "a", "b");
        r.properties.insert("role".into(), PropValue::Text("A".into()));
        m.add_relationship(r).unwrap();
        m.add_relationship(Relationship::new("r3", "flowchart.Flow", "b", "c")).unwrap();
        m
    }

    #[test]
    fn a_list_has_a_row_per_element_of_the_type_with_its_properties() {
        let m = sample();
        let sel = Selector::Types(vec!["flowchart.Process".into()]);
        let t = list(&m, Subject::Elements, &sel, &[], &["owner".into()], "");
        assert_eq!(t.columns, ["id", "tag", "name", "owner"]);
        assert_eq!(t.rows, [["a", "", "Gather", "Ana"], ["b", "", "Save", ""]]);
        assert_eq!(t.ids, ["a", "b"]);
        let all = list(&m, Subject::Elements, &Selector::Pack("flowchart".into()), &[], &[], "");
        assert_eq!(all.columns, ["id", "type", "tag", "name"]);
        assert_eq!(all.rows.len(), 3);
    }

    #[test]
    fn named_columns_and_filter_apply() {
        let m = sample();
        let cols = ["name".to_string(), "owner".to_string()];
        let t = list(&m, Subject::Elements, &Selector::All, &cols, &[], "ANA");
        assert_eq!(t.rows, [["Gather", "Ana"]]);
        let r = list(&m, Subject::Relationships, &Selector::All, &[], &["role".into()], "");
        assert_eq!(r.rows[0], ["r1", "flowchart.Flow", "", "Gather", "Save", "R"]);
    }

    #[test]
    fn the_matrix_marks_pairs_and_reads_a_property() {
        let m = sample();
        let p = Selector::Types(vec!["flowchart.Process".into()]);
        let x = matrix(&m, &p, &p, Some("flowchart.Flow"), None);
        assert_eq!((x.rows.len(), x.cols.len()), (2, 2));
        assert_eq!(x.cell(0, 1).unwrap().text, "x");
        assert_eq!(x.cell(0, 1).unwrap().relationships, ["r1", "r2"]);
        assert!(x.cell(1, 0).is_none() && x.cell(0, 0).is_none());
        let raci = matrix(&m, &p, &p, None, Some("role"));
        assert_eq!(raci.cell(0, 1).unwrap().text, "R, A");
    }

    #[test]
    fn rows_are_capped() {
        let mut m = Model::new();
        for i in 0..(MAX_ROWS + 5) {
            m.add_element(Element::new(format!("e{i}"), "x.T")).unwrap();
        }
        let t = list(&m, Subject::Elements, &Selector::All, &[], &[], "");
        assert_eq!((t.rows.len(), t.truncated), (MAX_ROWS, true));
        let x = matrix(&m, &Selector::All, &Selector::All, None, None);
        assert_eq!((x.rows.len(), x.cols.len(), x.truncated), (MAX_ROWS, MAX_COLS, true));
    }
}
