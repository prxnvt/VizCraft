//! Swimlane commands: a lane set (a cross-functional flowchart frame) of lanes, lanes added,
//! removed and reordered, phase separators, and flipping between horizontal and vertical. The
//! layout lives in `vizcraft_diagram::container`; a lane carries its members wherever the set puts
//! it. Each command is one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::container::{self as ct, Role};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{MAX_EXTENT, Shape, arrange};

use super::container::pick;
use super::{num, opt_str, page_mut, req_num, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all lane commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("lane.addSet", "Swimlanes", "Insert > Diagram Parts", add_set)
            .params(r#"{"x": n, "y": n, "lanes"?: n (default 3, 1 to 64), "orientation"?: "horizontal"|"vertical", "title"?: string, "id"?: string} -> {"id", "lanes"}; x, y is the top-left; horizontal lanes are bands stacked top to bottom"#),
        CommandSpec::new("lane.add", "Add Lane", "Container > Swimlanes", add)
            .params(r#"{"set"?: string, "index"?: n, "title"?: string, "id"?: string} -> {"id", "index"}; at the end by default; the set grows"#)
            .when(has_sets),
        CommandSpec::new("lane.remove", "Remove Lane", "Container > Swimlanes", remove)
            .params(r#"{"lane"?: string} -> {"deleted"}; the lane and its members go, the others close up"#)
            .when(has_sets),
        CommandSpec::new("lane.move", "Move Lane", "Container > Swimlanes", move_lane)
            .params(r#"{"lane"?: string, "index": n} -> {"lane", "index"}; its members move with it"#)
            .when(has_sets),
        CommandSpec::new("lane.addPhase", "Add Phase", "Container > Swimlanes", add_phase)
            .params(r#"{"set"?: string, "at": n, "title"?: string} -> {"set", "phases"}; a separator across the lanes, `at` from the set's left (top when vertical)"#)
            .when(has_sets),
        CommandSpec::new("lane.flip", "Flip Orientation", "Container > Swimlanes", flip)
            .params(r#"{"set"?: string} -> {"set", "orientation"}; lanes keep their members and thickness"#)
            .when(has_sets),
    ]
}

/// Enabled when the active page has a lane set.
fn has_sets(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(|x| ct::role(x) == Some(Role::LaneSet))) { None } else { Some("add swimlanes first") }
}

fn ct_err(e: ct::ContainerError) -> EngineError {
    EngineError::Params(e.to_string())
}

fn is_set(sh: &Shape) -> bool {
    ct::role(sh) == Some(Role::LaneSet)
}

fn is_lane(sh: &Shape) -> bool {
    ct::role(sh) == Some(Role::Lane)
}

/// The lane set named by `set`, else the selected set or the set of a selected lane.
fn the_set(s: &Session, v: &Value) -> Result<String, EngineError> {
    let id = match pick(s, v, "set", is_set, "lane set") {
        Ok(id) => id,
        Err(e) if opt_str(v, "set")?.is_none() => {
            let lane = pick(s, v, "lane", is_lane, "lane").map_err(|_| e)?;
            s.doc.page().and_then(|p| ct::set_of(p, &lane)).ok_or_else(|| EngineError::Params("the lane is in no set".into()))?
        }
        Err(e) => return Err(e),
    };
    super::layer::ensure_unlocked(s, [&id])?;
    Ok(id)
}

/// Refuse a change to a locked set.
fn ensure_open(s: &Session, set: &str) -> Result<(), EngineError> {
    if s.doc.shape(set).is_some_and(ct::is_locked) { Err(ct_err(ct::ContainerError::Locked)) } else { Ok(()) }
}

fn add_set(s: &mut Session, v: &Value) -> CmdResult {
    let x = req_num(v, "x")?.clamp(-MAX_COORD, MAX_COORD);
    let y = req_num(v, "y")?.clamp(-MAX_COORD, MAX_COORD);
    let n = num(v, "lanes")?.unwrap_or(3.0);
    if !(1.0..=ct::MAX_LANES as f64).contains(&n) || n.fract() != 0.0 {
        return Err(EngineError::Params(format!("`lanes` must be a whole number from 1 to {}", ct::MAX_LANES)));
    }
    let vertical = match opt_str(v, "orientation")? {
        None | Some("horizontal") => false,
        Some("vertical") => true,
        Some(t) => return Err(EngineError::Params(format!("`orientation` must be horizontal or vertical, not `{}`", short(t)))),
    };
    let title = opt_str(v, "title")?.unwrap_or("Process").to_string();
    let id = super::new_id(s, v, "lanes")?;
    let set = ct::new_lane_set(id.clone(), x, y, vertical, &title);
    page_mut(s)?.shapes.push(set);
    let mut lanes = Vec::new();
    for i in 0..n as usize {
        let lid = s.doc.next_id("lane");
        let lane = ct::new_lane(lid.clone(), &format!("Lane {}", i + 1), vertical);
        ct::add_lane(page_mut(s)?, &id, lane, None).map_err(ct_err)?;
        lanes.push(lid);
    }
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id, "lanes": lanes}))
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let set = the_set(s, v)?;
    ensure_open(s, &set)?;
    let index = match num(v, "index")? {
        Some(i) if i >= 0.0 && i.fract() == 0.0 => Some(i.min(ct::MAX_LANES as f64) as usize),
        Some(_) => return Err(EngineError::Params("`index` must be a whole number from 0".into())),
        None => None,
    };
    let vertical = s.doc.shape(&set).and_then(|x| x.container.as_ref()).is_some_and(|c| c.vertical);
    let count = s.doc.page().map_or(0, |p| ct::lanes(p, &set).len());
    let title = opt_str(v, "title")?.map_or_else(|| format!("Lane {}", count + 1), str::to_string);
    let id = super::new_id(s, v, "lane")?;
    let lane = ct::new_lane(id.clone(), &title, vertical);
    let index = ct::add_lane(page_mut(s)?, &set, lane, index).map_err(ct_err)?;
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id, "index": index}))
}

fn the_lane(s: &Session, v: &Value) -> Result<String, EngineError> {
    let lane = pick(s, v, "lane", is_lane, "lane")?;
    super::layer::ensure_unlocked(s, [&lane])?;
    if let Some(set) = s.doc.page().and_then(|p| ct::set_of(p, &lane)) {
        ensure_open(s, &set)?;
    }
    Ok(lane)
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let lane = the_lane(s, v)?;
    super::delete_items(s, &[lane])
}

fn move_lane(s: &mut Session, v: &Value) -> CmdResult {
    let lane = the_lane(s, v)?;
    let i = req_num(v, "index")?;
    if i < 0.0 || i.fract() != 0.0 {
        return Err(EngineError::Params("`index` must be a whole number from 0".into()));
    }
    let index = ct::move_lane(page_mut(s)?, &lane, i.min(ct::MAX_LANES as f64) as usize).map_err(ct_err)?;
    with_sel(s, json!({"lane": lane, "index": index}))
}

fn add_phase(s: &mut Session, v: &Value) -> CmdResult {
    let set = the_set(s, v)?;
    ensure_open(s, &set)?;
    let at = req_num(v, "at")?.clamp(0.0, MAX_EXTENT);
    let title = p::str(v, "title").unwrap_or("Phase").to_string();
    if v.get("title").is_some_and(|t| !t.is_null() && !t.is_string()) {
        return Err(EngineError::Params("`title` must be a string".into()));
    }
    let page = page_mut(s)?;
    ct::add_phase(page, &set, at, &title).map_err(ct_err)?;
    let phases = page.shape(&set).and_then(|x| x.container.as_ref()).map(|c| c.phases.len()).unwrap_or(0);
    with_sel(s, json!({"set": set, "phases": phases}))
}

fn flip(s: &mut Session, v: &Value) -> CmdResult {
    let set = the_set(s, v)?;
    let vertical = ct::flip(page_mut(s)?, &set).map_err(ct_err)?;
    arrange::refit_groups(page_mut(s)?);
    with_sel(s, json!({"set": set, "orientation": if vertical { "vertical" } else { "horizontal" }}))
}
