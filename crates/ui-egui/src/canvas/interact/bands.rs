//! Canvas edits of bands that a sync would overwrite. A compartment whose rows come from an
//! element's property (`rowsFrom`: OMG UML attributes, operations, literals) or from the elements
//! the element owns (an ER table's columns) is redrawn from the model on every change, so editing
//! the drawn text with `compartment.set` is lost. These edits write the model instead:
//!
//! - a `rowsFrom` band edits the property's raw text (`element.setProperty`), so what a variant
//!   hides (visibility marks) is not lost;
//! - an owned-table band edits the cell under the pointer: the name cell renames the column
//!   (`element.rename`), a flag cell (PK, FK) toggles on the double-click, other cells set the
//!   column's property (`element.setProperty`);
//! - a paragraph filled from properties opens the Inspector's property section, where the edit
//!   belongs.
//!
//! Each is one command, so one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::compartment as cp;
use vizcraft_diagram::vizcraft_model::PropValue;
use vizcraft_diagram::{Point, Rect, Shape};
use vizcraft_notation::schema::PropKind;

use super::run;
use crate::CraftApp;

/// What an open editor writes when it commits.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelEdit {
    /// The raw text of property `key` of element `element` (a `rowsFrom` band).
    Property {
        /// The element holding the property.
        element: String,
        /// The property key.
        key: String,
    },
    /// One cell of an owned table: column `column` of the attribute element `attr`, in the drawn
    /// row `row` and column index `col` of the compartment.
    Cell {
        /// The owned element the row shows.
        attr: String,
        /// The column's key (`name`, `dataType`, ...).
        column: String,
        /// The drawn row.
        row: usize,
        /// The drawn column.
        col: usize,
    },
}

/// What a double-click on a compartment band does instead of the plain compartment editor.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Open the editor over the band (or cell) with this text, committing through `edit`.
    Edit {
        /// What commits the text.
        edit: ModelEdit,
        /// The text to start from.
        text: String,
    },
    /// The double-click was the whole edit (a flag cell toggled) or has no editor (a table header).
    Done,
    /// The band is filled from properties: the Inspector's property section is opened.
    Inspector,
}

/// The plan for a double-click at world point `at` on compartment `cid` of shape `shape`, or
/// `None` when the band is a plain compartment edited with `compartment.set`.
pub fn plan(app: &mut CraftApp, shape: &str, cid: &str, at: Point) -> Option<Plan> {
    let found = {
        let doc = &app.session.doc;
        let page = doc.page()?;
        let s = page.shape(shape)?;
        let el = s.element.as_deref().and_then(|e| doc.model.element(e))?;
        let variant = page.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
        let cat = vizcraft_notation::catalog();
        let decl = cat.compartments_in(&el.type_id, &variant).iter().find(|d| d.id == cid)?;
        if !decl.rows_from.is_empty() {
            let text = el.prop(&decl.rows_from).map(PropValue::display).unwrap_or_default();
            return Some(Plan::Edit { edit: ModelEdit::Property { element: el.id.clone(), key: decl.rows_from.clone() }, text });
        }
        match &decl.query {
            Some(q) if q.owned => {
                let Some(row) = vizcraft_diagram::hit::band_of(s, at, 0.0).and_then(|h| h.row) else { return Some(Plan::Done) };
                let col = column_at(s, cid, at)?;
                let attr = vizcraft_notation::sheet_pass::table_row_ids(doc, cat, el, q).get(row).cloned()?;
                let column = q.columns.get(col)?.clone();
                let a = doc.model.element(&attr)?;
                let kind = cat.element_type(&a.type_id).and_then(|(_, t)| t.property(&column)).map(|p| p.kind);
                if kind == Some(PropKind::Bool) {
                    let now = a.prop(&column).and_then(PropValue::as_bool).unwrap_or(false);
                    Found::Toggle { attr, column, now }
                } else if column == "name" {
                    Found::Edit(ModelEdit::Cell { attr, column, row, col }, a.name.clone())
                } else {
                    let text = a.prop(&column).map(PropValue::display).unwrap_or_default();
                    Found::Edit(ModelEdit::Cell { attr, column, row, col }, text)
                }
            }
            Some(_) => return None,
            None if !decl.from_properties.is_empty() => Found::Inspector,
            None => return None,
        }
    };
    Some(match found {
        Found::Toggle { attr, column, now } => {
            run(app, "element.setProperty", json!({"id": attr, "key": column, "value": !now}));
            Plan::Done
        }
        Found::Edit(edit, text) => Plan::Edit { edit, text },
        Found::Inspector => {
            run(app, "ui.pane.shapeData", json!({"value": true}));
            run(app, "ui.inspector", json!({"tab": "properties"}));
            Plan::Inspector
        }
    })
}

enum Found {
    Toggle { attr: String, column: String, now: bool },
    Edit(ModelEdit, String),
    Inspector,
}

/// The table column of compartment `cid` under world point `at`.
fn column_at(s: &Shape, cid: &str, at: Point) -> Option<usize> {
    let local = s.transform().inverse()?.apply(at);
    let band = cp::layout(s).into_iter().find(|b| b.compartment.and_then(|i| s.compartments.get(i)).is_some_and(|c| c.id == cid))?;
    band.columns.windows(2).position(|w| local.x >= w[0] && local.x < w[1]).or_else(|| band.columns.len().checked_sub(2))
}

/// The cell `(row, col)` of compartment `cid` in the shape's unrotated frame.
pub fn cell_rect(s: &Shape, cid: &str, row: usize, col: usize) -> Option<Rect> {
    let band = cp::layout(s).into_iter().find(|b| b.compartment.and_then(|i| s.compartments.get(i)).is_some_and(|c| c.id == cid))?;
    let r = band.rows.get(row)?;
    let (x0, x1) = (*band.columns.get(col)?, *band.columns.get(col + 1)?);
    Some(Rect::new(x0, r.y0, x1, r.y1))
}

/// Commit `text` through `edit`: one command.
pub fn commit(app: &mut CraftApp, edit: &ModelEdit, text: &str) {
    match edit {
        ModelEdit::Property { element, key } => {
            run(app, "element.setProperty", json!({"id": element, "key": key, "value": text}));
        }
        ModelEdit::Cell { attr, column, .. } if column == "name" => {
            run(app, "element.rename", json!({"id": attr, "name": text.trim()}));
        }
        ModelEdit::Cell { attr, column, .. } => {
            let kind = app
                .session
                .doc
                .model
                .element(attr)
                .and_then(|a| vizcraft_notation::catalog().element_type(&a.type_id).and_then(|(_, t)| t.property(column)).map(|p| p.kind));
            let value: Value = match kind {
                Some(PropKind::Number | PropKind::Integer) => text.trim().parse::<f64>().map_or_else(|_| json!(text.trim()), |n| json!(n)),
                _ if text.trim().is_empty() => Value::Null,
                _ => json!(text.trim()),
            };
            run(app, "element.setProperty", json!({"id": attr, "key": column, "value": value}));
        }
    }
}
