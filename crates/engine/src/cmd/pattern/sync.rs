//! Table-driven patterns in the engine: the diagram work their emissions cannot say (RACI lanes
//! and shape data, the tree layout) and `pattern.sync`, which moves rows between a table and an
//! instance both ways.
//!
//! `pattern.sync {instance, direction: fromTable}` regenerates the instance from new rows (given
//! as `table: {columns?, rows}` or read from a CSV or XLSX file at `path`), keeping what
//! `pattern.reparameterize` keeps. `direction: toTable` reads the model back into rows: a RACI
//! step's lane is its R role and its shape data its A, C and I roles; a tree node's parent is the
//! source of its incoming relationship of the instance's type (or its owner); a matrix cell is a
//! relationship from a row element to a column element. When that reading is ambiguous (a step
//! in two lanes, a node with two parents, a cell with two relationships) the command refuses
//! and says which items, with the rule id `pattern.syncAmbiguous`.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::container::{self as ct, Role};
use vizcraft_diagram::data::DataRow;
use vizcraft_diagram::data_graphic::TableFormat;
use vizcraft_diagram::{Document, arrange as ar};
use vizcraft_model::PatternInstance;
use vizcraft_patterns::tables::{self, MATRIX, RACI, RACI_DATA_KEYS, TREE};
use vizcraft_patterns::{Args, ParamKind, Pattern};

use super::{Realized, instance_param, offered, pattern_err, reparameterize};
use crate::cmd::{opt_str, short};
use crate::{CmdResult, EngineError, Session};

/// Space between a RACI lane's edge and the steps in it.
const PAD: f64 = 30.0;

/// The rule id a refused, ambiguous `toTable` names.
pub const AMBIGUOUS: &str = "pattern.syncAmbiguous";

/// Finish an instance of a table-driven pattern after the generic layout: lanes and shape data
/// for RACI, the tree layout for a tree. Returns the top-level shapes to gather.
pub(super) fn after_arrange(
    doc: &mut Document,
    page: usize,
    pattern: &str,
    args: &Args,
    r: &mut Realized,
    top: Vec<String>,
) -> Result<Vec<String>, EngineError> {
    match pattern {
        RACI => raci_lanes(doc, page, args, r).map(|t| t.unwrap_or(top)),
        TREE => {
            let ids: Vec<String> = r.shapes.values().cloned().collect();
            let pg = doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
            let o = Options { algorithm: Algorithm::Tree, direction: Direction::TopBottom, ..Options::default() };
            autolayout::layout_items(pg, Some(&ids), &o).map_err(EngineError::Failed)?;
            Ok(ar::top_level(pg, &ids))
        }
        _ => Ok(top),
    }
}

/// Draw the RACI lanes around the laid-out steps: a lane set with one lane per role, each step
/// moved into the lane of its R role, and its A, C and I roles written as shape data. Returns
/// the lane set as the one top-level shape, or `None` when there are no steps to hold.
fn raci_lanes(doc: &mut Document, page: usize, args: &Args, r: &mut Realized) -> Result<Option<Vec<String>>, EngineError> {
    let plan = tables::raci_plan(args).map_err(pattern_err)?;
    let steps: Vec<(String, usize)> =
        plan.steps.iter().filter_map(|st| Some((r.shapes.get(&st.role)?.clone(), plan.lanes.iter().position(|l| *l == st.responsible)?))).collect();
    let set_id = doc.next_id("lanes");
    let lane_ids: Vec<String> = plan.lanes.iter().map(|_| doc.next_id("lane")).collect();
    let pg = doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
    let ids: Vec<String> = steps.iter().map(|(s, _)| s.clone()).collect();
    let Some(b) = ar::bounds_of(pg, &ids) else { return Ok(None) };
    let tallest = ids.iter().filter_map(|id| pg.shape(id)).map(|s| s.h).fold(0.0, f64::max);
    let thickness = (tallest + 2.0 * PAD).max(ct::MIN_LANE);
    let title = args.text("title").trim();
    pg.shapes.push(ct::new_lane_set(set_id.clone(), b.x0, b.y0, false, if title.is_empty() { "Process" } else { title }));
    for (lid, name) in lane_ids.iter().zip(&plan.lanes) {
        let mut lane = ct::new_lane(lid.clone(), name, false);
        lane.h = thickness;
        ct::add_lane(pg, &set_id, lane, None).map_err(|e| EngineError::Failed(e.to_string()))?;
    }
    ct::layout_lanes(pg, &set_id);
    let heading = lane_ids.first().and_then(|l| pg.shape(l)).map(|l| ct::content_rect(l).x0 - l.x).unwrap_or(0.0).max(0.0);
    let length = heading + b.width() + 2.0 * PAD;
    if let Some(set) = pg.shape_mut(&set_id) {
        set.w = length.max(ct::MIN_LANE);
        set.sanitize();
        ct::refresh(set);
    }
    ct::layout_lanes(pg, &set_id);
    for (sid, k) in &steps {
        let (Some(lane), Some(step)) = (lane_ids.get(*k).and_then(|l| pg.shape(l)), pg.shape(sid)) else { continue };
        let c = ct::content_rect(lane);
        let (dx, dy) = (c.x0 + PAD + (step.x - b.x0) - step.x, c.center().y - step.h / 2.0 - step.y);
        crate::cmd::arrange::move_shapes(pg, std::slice::from_ref(sid), dx, dy);
        if let Some(l) = lane_ids.get(*k) {
            // A step that cannot join (a locked lane) stays where it was drawn.
            let _ = ct::add_members(pg, l, std::slice::from_ref(sid));
        }
    }
    for st in &plan.steps {
        let Some(sh) = r.shapes.get(&st.role).and_then(|id| pg.shape_mut(id)) else { continue };
        for (key, roles) in RACI_DATA_KEYS.iter().zip(&st.others) {
            sh.data.rows.retain(|row| row.key != *key);
            if !roles.is_empty() {
                sh.data.rows.push(DataRow { key: key.to_string(), value: roles.join(", "), ..DataRow::default() });
            }
        }
        sh.sanitize();
    }
    r.frames.insert("lanes".into(), set_id.clone());
    r.shapes.insert("lanes".into(), set_id.clone());
    for (k, lid) in lane_ids.iter().enumerate() {
        let role = format!("lane.{:03}", k.saturating_add(1));
        r.frames.insert(role.clone(), lid.clone());
        r.shapes.insert(role, lid.clone());
    }
    Ok(Some(vec![set_id]))
}

/// Rows from the `table` param (`{columns?, rows}`) or the file at `path`, in the order of
/// `columns` (matched by name in any case; a missing column reads as empty).
fn rows_param(v: &Value, columns: &[String]) -> Result<Value, EngineError> {
    let bad = |m: &str| EngineError::Params(m.to_string());
    let (given, rows): (Option<Vec<String>>, Vec<Value>) = match (v.get("table"), opt_str(v, "path")?) {
        (Some(t), _) if !t.is_null() => {
            let o = t.as_object().ok_or_else(|| bad("`table` must be {\"columns\"?: [names], \"rows\": [rows]}"))?;
            let given = match o.get("columns") {
                None | Some(Value::Null) => None,
                Some(Value::Array(a)) if a.len() <= vizcraft_patterns::schema::MAX_COLUMNS * 4 => Some(
                    a.iter()
                        .map(|c| c.as_str().map(|c| c.trim().to_string()).ok_or_else(|| bad("`table.columns` holds column names")))
                        .collect::<Result<_, _>>()?,
                ),
                Some(_) => return Err(bad("`table.columns` must be a short list of column names")),
            };
            let rows = o.get("rows").and_then(Value::as_array).ok_or_else(|| bad("`table.rows` must be a list of rows"))?;
            if rows.len() > vizcraft_patterns::schema::MAX_ROWS {
                return Err(EngineError::Params(format!("`table.rows` holds at most {} rows", vizcraft_patterns::schema::MAX_ROWS)));
            }
            (given, rows.clone())
        }
        (_, Some(path)) => {
            let t = crate::cmd::datalink::read_table(path, TableFormat::for_path(path), "")?;
            let rows = t.rows.iter().take(vizcraft_patterns::schema::MAX_ROWS.saturating_add(1)).map(|r| json!(r)).collect();
            (Some(t.columns.iter().map(|c| c.name.trim().to_string()).collect()), rows)
        }
        _ => return Err(bad("`fromTable` needs a `table` or a `path`")),
    };
    let Some(given) = given else { return Ok(Value::Array(rows)) };
    let at: Vec<Option<usize>> = columns.iter().map(|c| given.iter().position(|g| g.eq_ignore_ascii_case(c))).collect();
    if at.iter().all(Option::is_none) {
        return Err(EngineError::Params(format!("the table has none of the columns {}", columns.join(", "))));
    }
    let mapped = rows
        .iter()
        .map(|row| match row {
            Value::Array(cells) => Value::Array(at.iter().map(|i| i.and_then(|i| cells.get(i)).cloned().unwrap_or_else(|| json!(""))).collect()),
            other => other.clone(),
        })
        .collect();
    Ok(Value::Array(mapped))
}

fn ambiguous(what: String) -> EngineError {
    EngineError::Failed(format!("{AMBIGUOUS}: {what}; the table was not read back"))
}

/// The page index an instance was drawn on.
fn page_of(doc: &Document, inst: &PatternInstance) -> usize {
    doc.find_page(&inst.page).unwrap_or_else(|| doc.active_index())
}

/// The RACI rows of an instance: activity, the lane of its shapes, and its A, C and I data.
fn raci_rows(doc: &Document, inst: &PatternInstance) -> Result<Vec<Vec<String>>, EngineError> {
    let pg = doc.pages.get(page_of(doc, inst)).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
    let mut out = Vec::new();
    for (role, id) in inst.members.iter().filter(|(r, _)| r.starts_with("step.")) {
        let Some(e) = doc.model.element(id) else { continue };
        let shapes: Vec<_> = pg.shapes.iter().filter(|s| s.element.as_deref() == Some(id.as_str())).collect();
        let mut lanes: Vec<String> = Vec::new();
        for s in &shapes {
            let lane = s.parent.as_ref().and_then(|p| pg.shape(p)).filter(|l| ct::role(l) == Some(Role::Lane)).map(|l| l.text.trim().to_string());
            if let Some(l) = lane
                && !lanes.contains(&l)
            {
                lanes.push(l);
            }
        }
        if lanes.len() > 1 {
            return Err(ambiguous(format!("the step {} ({role}) is in {} lanes: {}", short(&e.name), lanes.len(), lanes.join(", "))));
        }
        let mut row = vec![e.name.clone(), lanes.pop().unwrap_or_default()];
        for key in RACI_DATA_KEYS {
            row.push(shapes.first().and_then(|s| s.data.rows.iter().find(|d| d.key == key)).map(|d| d.value.clone()).unwrap_or_default());
        }
        out.push(row);
    }
    Ok(out)
}

/// The tree rows of an instance: id, name and parent id, in the order of its last table.
fn tree_rows(doc: &Document, inst: &PatternInstance, args: &Args) -> Result<Vec<Vec<String>>, EngineError> {
    let rel = args.text("relationship").trim();
    let node_of: BTreeMap<&str, String> =
        inst.members.iter().filter_map(|(role, id)| role.strip_prefix("node.").map(|k| (id.as_str(), k.to_string()))).collect();
    let mut out = Vec::new();
    for row in args.table("rows") {
        let Some(key) = row.first().map(|k| k.trim().to_string()) else { continue };
        let Some(id) = inst.members.get(&tables::node_role(&key)) else { continue };
        let Some(e) = doc.model.element(id) else { continue };
        let parents: Vec<String> = if rel.is_empty() {
            e.owner.as_deref().and_then(|o| node_of.get(o)).cloned().into_iter().collect()
        } else {
            let mut ps: Vec<String> =
                doc.model.incoming(id).filter(|r| r.type_id == rel).filter_map(|r| node_of.get(r.source.as_str()).cloned()).collect();
            ps.dedup();
            ps
        };
        if parents.len() > 1 {
            return Err(ambiguous(format!("{} has {} parents: {}", short(&e.name), parents.len(), parents.join(", "))));
        }
        out.push(vec![key, e.name.clone(), parents.into_iter().next().unwrap_or_default()]);
    }
    Ok(out)
}

/// The matrix rows of an instance: one pair per relationship from a row element to a column
/// element.
fn matrix_rows(doc: &Document, inst: &PatternInstance) -> Result<Vec<Vec<String>>, EngineError> {
    let side = |prefix: &str| -> HashSet<&str> { inst.members.iter().filter(|(r, _)| r.starts_with(prefix)).map(|(_, id)| id.as_str()).collect() };
    let (rows, cols) = (side("row."), side("col."));
    let name = |id: &str| doc.model.element(id).map(|e| e.name.clone()).unwrap_or_default();
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut out = Vec::new();
    for r in doc.model.relationships.iter().filter(|r| rows.contains(r.source.as_str()) && cols.contains(r.target.as_str())) {
        let n = seen.entry((r.source.clone(), r.target.clone())).or_insert(0);
        *n += 1;
        if *n > 1 {
            return Err(ambiguous(format!("the cell {} / {} has two relationships", short(&name(&r.source)), short(&name(&r.target)))));
        }
        out.push(vec![name(&r.source), name(&r.target)]);
    }
    Ok(out)
}

/// The columns of a pattern's table parameter.
fn columns_of(p: &Pattern) -> Option<(String, Vec<String>)> {
    let key = tables::table_param(p)?;
    match &p.param(key)?.kind {
        ParamKind::Table { columns } => Some((key.to_string(), columns.clone())),
        _ => None,
    }
}

/// `pattern.sync {instance, direction, table? or path?}`.
pub(super) fn sync(s: &mut Session, v: &Value) -> CmdResult {
    let iid = instance_param(s, v)?;
    let inst = s.doc.model.pattern(&iid).cloned().ok_or_else(|| EngineError::Failed("the instance went missing".into()))?;
    let p = offered(&inst.pattern)?;
    let (key, columns) = columns_of(p).ok_or_else(|| EngineError::Params(format!("{} takes no table to sync with", p.id)))?;
    match opt_str(v, "direction")? {
        Some("fromTable") => {
            let rows = rows_param(v, &columns)?;
            let mut r = reparameterize(s, &json!({"instance": iid, "args": {key: rows}}))?;
            if let Some(o) = r.as_object_mut() {
                o.insert("direction".into(), json!("fromTable"));
            }
            Ok(r)
        }
        Some("toTable") => {
            let args = p.check(&inst.args).map_err(pattern_err)?;
            let rows = match p.id.as_str() {
                RACI => raci_rows(&s.doc, &inst)?,
                TREE => tree_rows(&s.doc, &inst, &args)?,
                MATRIX => matrix_rows(&s.doc, &inst)?,
                other => return Err(EngineError::Params(format!("{} cannot be read back into a table", short(other)))),
            };
            Ok(json!({"instance": iid, "direction": "toTable", "table": {"columns": columns, "rows": rows}}))
        }
        Some(d) => Err(EngineError::Params(format!("`direction` must be fromTable or toTable, not `{}`", short(d)))),
        None => Err(EngineError::Params("`direction` (fromTable or toTable) is required".into())),
    }
}

/// Whether instance `instance` comes from a pattern with a table param, so it can be synced.
pub fn has_table(s: &Session, instance: &str) -> bool {
    s.doc.model.pattern(instance).and_then(|i| vizcraft_patterns::find(&i.pattern)).is_some_and(|p| tables::table_param(p).is_some())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{Document, Session};

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn raci() -> Value {
        json!([
            ["Draft request", "Requester", "Manager", "", ""],
            ["Review", "Manager", "Director", "Finance", "Requester"],
            ["Approve", "Director", "Director", "", "Requester, Manager"]
        ])
    }

    /// The shape drawing `element` on the active page.
    fn shape(s: &Session, element: &str) -> vizcraft_diagram::Shape {
        s.doc.page().unwrap().shapes.iter().find(|x| x.element.as_deref() == Some(element)).cloned().unwrap()
    }

    /// The title of the lane the shape of `element` sits in.
    fn lane_of(s: &Session, element: &str) -> String {
        let p = s.doc.page().unwrap();
        let parent = shape(s, element).parent.unwrap();
        p.shape(&parent).unwrap().text.clone()
    }

    #[test]
    fn a_raci_table_inserts_lanes_with_steps_in_their_r_lanes_and_reads_back_the_same_rows() {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "pattern.insert", json!({"pattern": "table.raciLanes", "args": {"rows": raci()}, "x": 40, "y": 40}));
        let iid = r["instance"].as_str().unwrap().to_string();
        let m = &r["members"];
        let set = s.doc.model.pattern(&iid).unwrap().frames["lanes"].clone();
        let lanes: Vec<String> = vizcraft_diagram::container::lanes(s.doc.page().unwrap(), &set)
            .iter()
            .map(|l| s.doc.page().unwrap().shape(l).unwrap().text.clone())
            .collect();
        assert_eq!(lanes, ["Requester", "Manager", "Director", "Finance"]);
        for (role, lane) in [("step.001", "Requester"), ("step.002", "Manager"), ("step.003", "Director")] {
            let e = m[role].as_str().unwrap();
            assert_eq!(lane_of(&s, e), lane, "{role}");
            let b = shape(&s, e).bounds();
            let l = s.doc.page().unwrap().shapes.iter().find(|x| x.text == lane && x.parent.as_deref() == Some(set.as_str())).unwrap().bounds();
            assert!(l.contains_rect(&b), "{role} sits inside its lane");
        }
        let review = shape(&s, m["step.002"].as_str().unwrap());
        assert_eq!(review.data.rows.iter().find(|d| d.key == "Consulted").unwrap().value, "Finance");
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"], raci());
        assert_eq!(back["table"]["columns"], json!(["Activity", "R", "A", "C", "I"]));

        // Moving a step to another lane by hand reads back as its new R role.
        let draft = shape(&s, m["step.001"].as_str().unwrap()).id;
        let finance = s.doc.page().unwrap().shapes.iter().find(|x| x.text == "Finance").unwrap().id.clone();
        let p = s.doc.pages.get_mut(0).unwrap();
        p.shape_mut(&draft).unwrap().parent = Some(finance);
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"][0][1], "Finance");

        // From a table with a new row and a changed R role, as one undo step.
        let before = s.doc.clone();
        let mut rows = raci();
        rows[2][1] = json!("Manager");
        rows.as_array_mut().unwrap().push(json!(["Archive", "Clerk", "", "", ""]));
        let r = run(
            &mut s,
            "pattern.sync",
            json!({"instance": iid, "direction": "fromTable", "table": {"columns": ["activity", "r", "a", "c", "i"], "rows": rows}}),
        );
        assert_eq!(r["direction"], "fromTable");
        assert_eq!(r["members"]["step.001"], m["step.001"], "steps keep their elements");
        assert_eq!(lane_of(&s, m["step.003"].as_str().unwrap()), "Manager");
        assert_eq!(lane_of(&s, r["members"]["step.004"].as_str().unwrap()), "Clerk");
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"], rows);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model, before.model);
    }

    #[test]
    fn a_parent_id_table_builds_a_tree_and_a_reparented_node_syncs_back() {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "pattern.insert", json!({"pattern": "table.tree", "x": 40, "y": 40}));
        let iid = r["instance"].as_str().unwrap().to_string();
        let m = r["members"].clone();
        let (ceo, cto, cfo, eng) = ["node.ceo", "node.cto", "node.cfo", "node.eng"].map(|k| m[k].as_str().unwrap().to_string()).into();
        let (sc, st, se) = (shape(&s, &ceo), shape(&s, &cto), shape(&s, &eng));
        assert!(sc.y + sc.h <= st.y && st.y + st.h <= se.y, "a tree from the top down");
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"][3], json!(["eng", "Engineering", "cto"]));

        // Reparent Engineering under Finance: delete its link and draw a new one.
        let link = s.doc.page().unwrap().connectors.iter().find(|c| c.end.shape() == Some(se.id.as_str())).unwrap().id.clone();
        run(&mut s, "edit.select", json!({"ids": [link]}));
        run(&mut s, "edit.delete", json!({}));
        let sf = shape(&s, &cfo);
        run(&mut s, "connector.add", json!({"from": {"shape": sf.id}, "to": {"shape": se.id}}));
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"][3], json!(["eng", "Engineering", "cfo"]));

        // A second parent makes the reading ambiguous: refused, nothing changes.
        run(&mut s, "connector.add", json!({"from": {"shape": sc.id}, "to": {"shape": se.id}}));
        let before = s.doc.clone();
        let e = s.run("pattern.sync", &json!({"instance": iid, "direction": "toTable"})).unwrap_err().to_string();
        assert!(e.contains("pattern.syncAmbiguous") && e.contains("2 parents"), "{e}");
        assert_eq!(s.doc, before);
    }

    #[test]
    fn a_matrix_cell_with_two_relationships_is_refused_and_tables_load_from_files() {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "pattern.insert", json!({"pattern": "table.matrix", "x": 40, "y": 40}));
        let iid = r["instance"].as_str().unwrap().to_string();
        let back = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "toTable"}));
        assert_eq!(back["table"]["rows"], json!([["Take order", "Orders"], ["Take order", "Customers"], ["Bill", "Orders"]]));
        let (a, b) = (shape(&s, r["members"]["row.Bill"].as_str().unwrap()), shape(&s, r["members"]["col.Orders"].as_str().unwrap()));
        run(&mut s, "connector.add", json!({"from": {"shape": a.id}, "to": {"shape": b.id}}));
        let e = s.run("pattern.sync", &json!({"instance": iid, "direction": "toTable"})).unwrap_err().to_string();
        assert!(e.contains("pattern.syncAmbiguous") && e.contains("two relationships"), "{e}");

        let dir = std::env::temp_dir().join(format!("vizcraft-sync-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pairs.csv");
        std::fs::write(&path, "To,From\nInvoices,Bill\n").unwrap();
        let r = run(&mut s, "pattern.sync", json!({"instance": iid, "direction": "fromTable", "path": path.to_string_lossy()}));
        assert!(r["members"]["row.Bill"].is_string() && r["members"]["col.Invoices"].is_string(), "{r}");
        assert!(r["members"].get("row.Take order").is_none());
    }

    #[test]
    fn junk_tables_and_paths_are_refused() {
        let mut s = Session::new(Document::new());
        let r = run(&mut s, "pattern.insert", json!({"pattern": "table.raciLanes"}));
        let iid = r["instance"].as_str().unwrap().to_string();
        let flow = run(&mut s, "pattern.insert", json!({"pattern": "workflow.parallelSplit"}));
        for v in [
            json!({"instance": iid}),
            json!({"instance": iid, "direction": "sideways"}),
            json!({"instance": iid, "direction": "fromTable"}),
            json!({"instance": iid, "direction": "fromTable", "table": 5}),
            json!({"instance": iid, "direction": "fromTable", "table": {"rows": 5}}),
            json!({"instance": iid, "direction": "fromTable", "table": {"columns": [1], "rows": []}}),
            json!({"instance": iid, "direction": "fromTable", "table": {"columns": ["Nope"], "rows": [["a"]]}}),
            json!({"instance": iid, "direction": "fromTable", "table": {"rows": [[1, 2]]}}),
            json!({"instance": iid, "direction": "fromTable", "table": {"rows": vec![json!(["a", "b"]); 300]}}),
            json!({"instance": iid, "direction": "fromTable", "table": {"rows": [["a", "b, c"]]}}),
            json!({"instance": iid, "direction": "fromTable", "path": "/nonexistent/\u{0}.csv"}),
            json!({"instance": iid, "direction": "fromTable", "path": ""}),
            json!({"instance": iid, "direction": "fromTable", "path": "/"}),
            json!({"instance": flow["instance"], "direction": "toTable"}),
            json!({"instance": "nope", "direction": "toTable"}),
        ] {
            let before = s.doc.clone();
            assert!(s.run("pattern.sync", &v).is_err(), "{v}");
            assert_eq!(s.doc, before, "{v}");
        }
    }
}
