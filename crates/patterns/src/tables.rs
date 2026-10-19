//! Table-driven patterns: a table argument becomes model elements, and the engine's
//! `pattern.sync` reads the model back into rows.
//!
//! | Pattern | Table columns | Drawing |
//! |---|---|---|
//! | `table.raciLanes` | Activity, R, A, C, I | one swimlane per role, one step per activity in the lane of its R role, flows in row order; A, C and I kept as the step's shape data |
//! | `table.tree` | Id, Name, Parent | one element per row joined to its parent by the named relationship (or owned by it when the relationship is empty), laid out as a tree |
//! | `table.matrix` | From, To | one element per distinct From and To value and a relationship per pair, the cells of the Tables tab's matrix |
//!
//! Roles are stable across regenerations: a RACI step is `step.<row>`, a tree node
//! `node.<id>`, a matrix element `row.<name>` or `col.<name>`. The Flowchart pack has no role
//! type, so the RACI roles live on the diagram (lanes and shape data); [`raci_plan`] gives the
//! engine what to draw. The types are parameters whose defaults are the Flowchart pack's.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::json;
use vizcraft_layout::{Direction, Primitive};

use crate::emit::{Emission, EmittedRelationship};
use crate::schema::{Args, Param, ParamKind, Pattern, quote};
use crate::{Builtin, PatternError};

/// The RACI swimlane pattern.
pub const RACI: &str = "table.raciLanes";
/// The parent-id tree pattern.
pub const TREE: &str = "table.tree";
/// The two-column matrix pattern.
pub const MATRIX: &str = "table.matrix";

/// The columns of the RACI table.
pub const RACI_COLUMNS: [&str; 5] = ["Activity", "R", "A", "C", "I"];
/// The columns of the tree table.
pub const TREE_COLUMNS: [&str; 3] = ["Id", "Name", "Parent"];
/// The columns of the matrix table.
pub const MATRIX_COLUMNS: [&str; 2] = ["From", "To"];

/// The shape data keys a RACI step keeps its A, C and I roles under.
pub const RACI_DATA_KEYS: [&str; 3] = ["Accountable", "Consulted", "Informed"];

/// Longest key taken from a cell into a role name, in characters.
const KEY: usize = 24;

const PROCESS: &str = "flowchart.Process";
const DATA: &str = "flowchart.Data";
const FLOW: &str = "flowchart.Flow";

/// Whether `id` is one of the table-driven patterns.
pub fn is_table_pattern(id: &str) -> bool {
    [RACI, TREE, MATRIX].contains(&id)
}

/// The key of the table parameter of a table-driven pattern.
pub fn table_param(p: &Pattern) -> Option<&str> {
    p.params.iter().find(|q| matches!(q.kind, ParamKind::Table { .. })).map(|q| q.key.as_str())
}

fn pattern(id: &str, name: &str, description: &str, params: Vec<Param>) -> Pattern {
    Pattern { id: id.into(), name: name.into(), description: description.into(), pack: "flowchart".into(), audience: vec!["general".into()], params }
}

/// A table parameter with example rows as its default.
fn table(key: &str, label: &str, columns: &[&str], example: &[&[&str]]) -> Param {
    let mut p = Param::table(key, label, columns);
    p.default = json!(example);
    p
}

/// The table-driven patterns with their generators.
pub fn builtins() -> Vec<Builtin> {
    vec![
        Builtin {
            pattern: pattern(
                RACI,
                "RACI Swimlanes",
                "A RACI table becomes swimlanes: one lane per role and one step per activity, placed in the lane of the role responsible for it.",
                vec![
                    table(
                        "rows",
                        "RACI table",
                        &RACI_COLUMNS,
                        &[
                            &["Draft request", "Requester", "Manager", "", ""],
                            &["Review", "Manager", "Director", "Finance", "Requester"],
                            &["Approve", "Director", "Director", "", "Requester"],
                        ],
                    ),
                    Param::text("title", "Title", "Process"),
                ],
            ),
            generate: raci,
        },
        Builtin {
            pattern: pattern(
                TREE,
                "Tree from Table",
                "A table of ids and parent ids becomes a tree of elements joined to their parents, laid out as a tree.",
                vec![
                    table(
                        "rows",
                        "Tree table",
                        &TREE_COLUMNS,
                        &[&["ceo", "Chief Executive", ""], &["cto", "Technology", "ceo"], &["cfo", "Finance", "ceo"], &["eng", "Engineering", "cto"]],
                    ),
                    Param::text("type", "Element type", PROCESS),
                    Param::text("relationship", "Relationship (empty: ownership)", FLOW),
                ],
            ),
            generate: tree,
        },
        Builtin {
            pattern: pattern(
                MATRIX,
                "Matrix from Table",
                "A two-column table becomes elements of two types and a relationship for each pair, as the Tables tab's matrix shows them.",
                vec![
                    table("rows", "Pairs", &MATRIX_COLUMNS, &[&["Take order", "Orders"], &["Take order", "Customers"], &["Bill", "Orders"]]),
                    Param::text("rowType", "Row type", PROCESS),
                    Param::text("colType", "Column type", DATA),
                    Param::text("relationship", "Relationship", FLOW),
                ],
            ),
            generate: matrix,
        },
    ]
}

fn bad(why: String) -> PatternError {
    PatternError::Args(format!("`rows`: {why}"))
}

/// The cells of one text: split on `,` and `;`, trimmed, empty ones left out.
pub fn names(cell: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    cell.split([',', ';']).map(str::trim).filter(|n| !n.is_empty() && seen.insert(n.to_string())).map(str::to_string).collect()
}

/// One RACI step: its role, activity and the roles of each letter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaciStep {
    /// The step's role in the pattern (`step.001`).
    pub role: String,
    /// The activity, the step's name.
    pub activity: String,
    /// The responsible role: the lane the step sits in.
    pub responsible: String,
    /// The accountable, consulted and informed roles, in that order.
    pub others: [Vec<String>; 3],
}

/// What a RACI table draws: the lanes (role names, first appearance first) and the steps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaciPlan {
    /// One lane per role named anywhere in the table.
    pub lanes: Vec<String>,
    /// One step per row, in row order.
    pub steps: Vec<RaciStep>,
}

/// The role of RACI step `i` (from 0), padded so the roles sort in row order.
pub fn step_role(i: usize) -> String {
    format!("step.{:03}", i.saturating_add(1))
}

/// Read the RACI table of checked `args`: every row needs an activity and exactly one R role.
pub fn raci_plan(args: &Args) -> Result<RaciPlan, PatternError> {
    let rows = args.table("rows");
    if rows.is_empty() {
        return Err(bad("the RACI table needs at least one row".into()));
    }
    let mut plan = RaciPlan::default();
    let lane = |name: &str, plan: &mut RaciPlan| {
        if !plan.lanes.iter().any(|l| l == name) {
            plan.lanes.push(name.to_string());
        }
    };
    for (i, row) in rows.iter().enumerate() {
        let cell = |c: usize| row.get(c).map(String::as_str).unwrap_or_default();
        let activity = cell(0).trim();
        if activity.is_empty() {
            return Err(bad(format!("row {} names no activity", i.saturating_add(1))));
        }
        let r = names(cell(1));
        let [responsible] = r.as_slice() else {
            return Err(bad(format!("row {} (`{}`) needs exactly one responsible role in R, not {}", i.saturating_add(1), quote(activity), r.len())));
        };
        lane(responsible, &mut plan);
        let others = [names(cell(2)), names(cell(3)), names(cell(4))];
        for n in others.iter().flatten() {
            lane(n, &mut plan);
        }
        plan.steps.push(RaciStep { role: step_role(i), activity: activity.to_string(), responsible: responsible.clone(), others });
    }
    if plan.lanes.len() > 64 {
        return Err(bad(format!("names {} roles; a lane set holds at most 64", plan.lanes.len())));
    }
    Ok(plan)
}

fn raci(args: &Args) -> Result<Emission, PatternError> {
    let plan = raci_plan(args)?;
    let mut e = Emission::new(RACI, Primitive::Pipeline { parallel: None }, Direction::LeftRight);
    for s in &plan.steps {
        if let Some(x) = e.element(s.role.clone(), PROCESS, s.activity.clone()) {
            x.tagged = false;
        }
    }
    for w in plan.steps.windows(2) {
        if let [a, b] = w {
            e.relate("flow", FLOW, &a.role, &b.role, "");
        }
    }
    Ok(e)
}

/// `t` cut to [`KEY`] characters, for a role name.
fn key(t: &str) -> String {
    t.trim().chars().take(KEY).collect()
}

/// The role of the tree node with id `id`.
pub fn node_role(id: &str) -> String {
    format!("node.{}", key(id))
}

/// A tree row: id, name and parent id, all trimmed.
fn tree_rows(args: &Args) -> Result<Vec<(String, String, String)>, PatternError> {
    let rows = args.table("rows");
    if rows.is_empty() {
        return Err(bad("the tree table needs at least one row".into()));
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut keys = HashSet::new();
    for (i, row) in rows.iter().enumerate() {
        let cell = |c: usize| row.get(c).map(|s| s.trim().to_string()).unwrap_or_default();
        let id = cell(0);
        if id.is_empty() {
            return Err(bad(format!("row {} has no id", i.saturating_add(1))));
        }
        if id.chars().count() > KEY || !keys.insert(id.clone()) {
            return Err(bad(format!("the id `{}` is used twice or longer than {KEY} characters", quote(&id))));
        }
        out.push((id, cell(1), cell(2)));
    }
    let ids: HashSet<&str> = out.iter().map(|r| r.0.as_str()).collect();
    if let Some(r) = out.iter().find(|r| !r.2.is_empty() && !ids.contains(r.2.as_str())) {
        return Err(bad(format!("`{}` names the parent `{}`, which no row has", quote(&r.0), quote(&r.2))));
    }
    // A chain of parents longer than the table is a cycle.
    let parent: HashMap<&str, &str> = out.iter().filter(|r| !r.2.is_empty()).map(|r| (r.0.as_str(), r.2.as_str())).collect();
    for r in &out {
        let mut at = r.0.as_str();
        for _ in 0..=out.len() {
            match parent.get(at) {
                Some(p) => at = p,
                None => break,
            }
        }
        if parent.contains_key(at) {
            return Err(bad(format!("the parents of `{}` form a cycle", quote(&r.0))));
        }
    }
    Ok(out)
}

fn tree(args: &Args) -> Result<Emission, PatternError> {
    let rows = tree_rows(args)?;
    let ty = args.text("type").trim();
    let rel = args.text("relationship").trim();
    let mut e = Emission::new(TREE, Primitive::Pipeline { parallel: None }, Direction::TopBottom);
    for (id, name, _) in &rows {
        let shown = if name.is_empty() { id.clone() } else { name.clone() };
        if let Some(x) = e.element(node_role(id), ty, shown) {
            x.tagged = false;
        }
    }
    for (id, _, parent) in rows.iter().filter(|r| !r.2.is_empty()) {
        if rel.is_empty() {
            let owner = node_role(parent);
            if let Some(x) = e.elements.iter_mut().find(|x| x.role == node_role(id)) {
                x.owner = Some(owner);
            }
        } else {
            e.relationships.push(EmittedRelationship {
                role: format!("link.{}", key(id)),
                type_id: rel.to_string(),
                from: node_role(parent),
                to: node_role(id),
                name: String::new(),
            });
        }
    }
    Ok(e)
}

/// Role keys for the distinct values of one matrix column: each value's cut text, with `~n`
/// added where two values cut to the same key.
fn matrix_keys<'a>(values: impl Iterator<Item = &'a str>, prefix: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut used = HashSet::new();
    for v in values {
        if out.contains_key(v) {
            continue;
        }
        let base = format!("{prefix}.{}", key(v));
        let mut role = base.clone();
        let mut n = 2u32;
        while !used.insert(role.clone()) {
            role = format!("{base}~{n}");
            n = n.saturating_add(1);
        }
        out.insert(v.to_string(), role);
    }
    out
}

/// The distinct pairs of a matrix table, in row order, with their row and column roles.
pub fn matrix_plan(args: &Args) -> Result<Vec<(String, String, String, String)>, PatternError> {
    let rows = args.table("rows");
    if rows.is_empty() {
        return Err(bad("the matrix table needs at least one pair".into()));
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let cell = |c: usize| row.get(c).map(|s| s.trim().to_string()).unwrap_or_default();
        let (a, b) = (cell(0), cell(1));
        if a.is_empty() || b.is_empty() {
            return Err(bad(format!("row {} needs both a From and a To", i.saturating_add(1))));
        }
        if !pairs.contains(&(a.clone(), b.clone())) {
            pairs.push((a, b));
        }
    }
    let from = matrix_keys(pairs.iter().map(|p| p.0.as_str()), "row");
    let to = matrix_keys(pairs.iter().map(|p| p.1.as_str()), "col");
    Ok(pairs
        .into_iter()
        .map(|(a, b)| {
            let (ra, rb) = (from.get(&a).cloned().unwrap_or_default(), to.get(&b).cloned().unwrap_or_default());
            (a, b, ra, rb)
        })
        .collect())
}

fn matrix(args: &Args) -> Result<Emission, PatternError> {
    let pairs = matrix_plan(args)?;
    let (rt, ct, rel) = (args.text("rowType").trim(), args.text("colType").trim(), args.text("relationship").trim());
    let mut e = Emission::new(MATRIX, Primitive::Bands { bands: vec![Vec::new(), Vec::new()] }, Direction::LeftRight);
    let (mut rows, mut cols): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for (a, b, ra, rb) in &pairs {
        if !rows.contains(ra) {
            rows.push(ra.clone());
            if let Some(x) = e.element(ra.clone(), rt, a.clone()) {
                x.tagged = false;
            }
        }
        if !cols.contains(rb) {
            cols.push(rb.clone());
            if let Some(x) = e.element(rb.clone(), ct, b.clone()) {
                x.tagged = false;
            }
        }
        if rel.is_empty() {
            return Err(PatternError::Args("`relationship` names no relationship type".into()));
        }
        e.relationships.push(EmittedRelationship {
            role: format!("cell.{}.{}", ra.trim_start_matches("row."), rb.trim_start_matches("col.")),
            type_id: rel.to_string(),
            from: ra.clone(),
            to: rb.clone(),
            name: String::new(),
        });
    }
    e.layout = Primitive::Bands { bands: vec![rows, cols] };
    Ok(e)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn emit(id: &str, args: serde_json::Value) -> Result<Emission, PatternError> {
        let p = crate::find(id).unwrap();
        let a = p.check(&args)?;
        crate::generate(p, &a, vizcraft_notation::catalog())
    }

    #[test]
    fn raci_rows_become_steps_with_lanes_for_every_role() {
        let e = emit(RACI, json!({})).unwrap();
        assert_eq!(e.elements.len(), 3);
        assert_eq!(e.relationships.len(), 2);
        let p = crate::find(RACI).unwrap();
        let plan = raci_plan(&p.check(&json!({})).unwrap()).unwrap();
        assert_eq!(plan.lanes, ["Requester", "Manager", "Director", "Finance"]);
        assert_eq!(plan.steps[1].responsible, "Manager");
        assert_eq!(plan.steps[1].others[1], ["Finance"]);
        for bad in [
            json!({"rows": []}),
            json!({"rows": [["", "A"]]}),
            json!({"rows": [["Do", ""]]}),
            json!({"rows": [["Do", "A, B"]]}),
            json!({"rows": [{"Nope": "x"}]}),
            json!({"rows": "x"}),
        ] {
            assert!(emit(RACI, bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn tree_rows_join_parents_and_refuse_cycles_and_unknown_parents() {
        let e = emit(TREE, json!({})).unwrap();
        assert_eq!(e.elements.len(), 4);
        assert_eq!(e.relationships.len(), 3);
        assert!(e.relationships.iter().any(|r| r.from == "node.cto" && r.to == "node.eng"));
        for bad in [
            json!({"rows": [["a", "", "b"], ["b", "", "a"]]}),
            json!({"rows": [["a", "", "zz"]]}),
            json!({"rows": [["a"], ["a"]]}),
            json!({"rows": [[""]]}),
            json!({"type": "nope.Type"}),
            json!({"relationship": "nope.Rel"}),
            json!({"relationship": ""}),
        ] {
            assert!(emit(TREE, bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn matrix_pairs_become_two_bands_and_one_relationship_each() {
        let e = emit(MATRIX, json!({"rows": [["A", "X"], ["A", "Y"], ["B", "X"], ["A", "X"]]})).unwrap();
        assert_eq!(e.elements.iter().filter(|x| x.type_id == PROCESS).count(), 2);
        assert_eq!(e.elements.iter().filter(|x| x.type_id == DATA).count(), 2);
        assert_eq!(e.relationships.len(), 3, "a repeated pair is one relationship");
        let long = "x".repeat(200);
        let e = emit(MATRIX, json!({"rows": [[long.clone() + "1", "X"], [long + "2", "X"]]})).unwrap();
        let roles: HashSet<&str> = e.elements.iter().map(|x| x.role.as_str()).collect();
        assert_eq!(roles.len(), 3, "long names cut to the same key stay apart");
        assert!(emit(MATRIX, json!({"rows": [["A", ""]]})).is_err());
    }
}
