//! Wire commands (`wire.*`) and the page's symbol grid module (`page.setModule`).
//!
//! `wire.draw` draws a wire segment between pins, junctions, points on other wires (which it
//! splits at a new junction) and free points, on the page's module grid; `wire.splice` cuts a
//! wire where a device is dropped on it and glues the halves to the device's pins; `wire.heal`
//! takes the device out again and joins the halves. Nets are worked out by the net pass of the
//! sync step afterwards (`vizcraft_notation::bind_nets`), so these commands only set the net a new
//! segment starts in. Each command is one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::net::{NetIndex, project, split_at};
use vizcraft_diagram::snap::{module_step, round_to};
use vizcraft_diagram::{Arrow, Connector, End, Point, Route, route_polyline};
use vizcraft_model::{Module, Net, NetKind, View};

use super::model::{check_len, page_param};
use super::{MAX_DELTA, page_mut, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return the wire commands and `page.setModule`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("wire.draw", "Draw Wire", "Insert > Wire", draw).params(
            r#"{"from": end, "to": end, "points"?: [{"x", "y"}], "kind"?: "wire"|"potential"|"pipe"|"fluid"|"signal", "net"?: id} -> {"id", "net", "junctions": [id]}; an end is {"shape", "point"} (a pin), {"connector", "x", "y"} (splits that wire at a new junction), {"junction"} or {"x", "y"}; free points and bends snap to the page's module; the wire joins the net at either end, else a new net of kind (default wire; pipe and fluid junctions draw no dot)"#,
        ),
        CommandSpec::new("wire.splice", "Splice Into Wire", "Insert > Wire", splice).params(
            r#"{"shape": id, "connector"?: id} -> {"connectors": [id, id], "shape"}; the device's two outermost pins are projected onto the wire (default: the nearest segment), the stretch between them is cut away and the halves are glued to the pins, both keeping the net; refused when the pins fall on different runs of the wire"#,
        ),
        CommandSpec::new("wire.heal", "Heal Wire", "Home > Editing", heal).params(
            r#"{"shape": id} -> {"connector", "deleted": [id]}; removes a device that has exactly one wire segment on each of two pins and joins the two segments through where its pins were; refused when they are on two different named nets"#,
        ),
        CommandSpec::new("page.setModule", "Symbol Grid Module", "Design > Page Setup", set_module).params(
            r#"{"page"?: page id, name or index, "module": "none"|"iso2_5mm"|"inch0_1"|page units (100 per inch)} -> {"page", "module"}; the grid ports and wires snap to (2.5 mm is IEC 81714-2, 0.1 inch the ANSI practice); shapes placed from a typed glyph put their first port on it"#,
        ),
    ]
}

/// One end of a wire as `wire.draw` takes it.
enum Spec {
    Pin(String, usize),
    OnConnector(String, Point),
    Junction(String),
    Free(Point),
}

/// A finite coordinate param, clamped like a move.
fn coord(v: &Value, k: &str) -> Result<f64, EngineError> {
    Ok(super::req_num(v, k)?.clamp(-MAX_DELTA, MAX_DELTA))
}

fn snap(p: Point, module: Option<f64>) -> Point {
    match module {
        Some(m) => Point::new(round_to(p.x, m), round_to(p.y, m)),
        None => p,
    }
}

fn end_spec(v: &Value, k: &str) -> Result<Spec, EngineError> {
    let e = v.get(k).filter(|e| e.is_object()).ok_or_else(|| EngineError::Params(format!("`{k}` (an end object) is required")))?;
    if let Some(c) = e.get("connector") {
        let c = c.as_str().ok_or_else(|| EngineError::Params(format!("`{k}.connector` must be a connector id")))?;
        return Ok(Spec::OnConnector(check_len("connector", c)?.to_string(), Point::new(coord(e, "x")?, coord(e, "y")?)));
    }
    if let Some(j) = e.get("junction") {
        let j = j.as_str().ok_or_else(|| EngineError::Params(format!("`{k}.junction` must be a shape id")))?;
        return Ok(Spec::Junction(check_len("junction", j)?.to_string()));
    }
    if let Some(sh) = e.get("shape") {
        let sh = sh.as_str().ok_or_else(|| EngineError::Params(format!("`{k}.shape` must be a shape id")))?;
        let point = p::usize(e, "point").ok_or_else(|| EngineError::Params(format!("`{k}.point` (connection point index) is required")))?;
        return Ok(Spec::Pin(check_len("shape", sh)?.to_string(), point));
    }
    Ok(Spec::Free(Point::new(coord(e, "x")?, coord(e, "y")?)))
}

/// The bends param: a list of `{x, y}`.
fn points_param(v: &Value) -> Result<Vec<Point>, EngineError> {
    match v.get("points") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= vizcraft_diagram::MAX_WAYPOINTS => {
            a.iter().map(|p| Ok(Point::new(coord(p, "x")?, coord(p, "y")?))).collect()
        }
        Some(_) => Err(EngineError::Params(format!("`points` must be a list of at most {} {{x, y}}", vizcraft_diagram::MAX_WAYPOINTS))),
    }
}

/// The net an end already belongs to, and a check that what it names exists.
fn end_net(s: &Session, ix: &NetIndex, spec: &Spec) -> Result<Option<String>, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    match spec {
        Spec::Pin(sh, point) => {
            let shape = page.shape(sh).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(sh))))?;
            if *point >= shape.connection_points().len() {
                return Err(EngineError::Params(format!("`{}` has no connection point {point}", short(sh))));
            }
            super::protect::check(s, [sh.as_str()], super::protect::Lock::Glue)?;
            Ok(ix.of_pin.get(&(sh.clone(), *point)).cloned())
        }
        Spec::Junction(j) => {
            let shape = page.shape(j).filter(|x| vizcraft_diagram::net::is_junction(x));
            shape.ok_or_else(|| EngineError::Params(format!("`{}` is not a junction on this page", short(j))))?;
            Ok(ix.of_junction.get(j).cloned())
        }
        Spec::OnConnector(c, _) => {
            let conn = page.connector(c).ok_or_else(|| EngineError::Params(format!("`{}` is not a connector on this page", short(c))))?;
            Ok(conn.net.clone())
        }
        Spec::Free(_) => Ok(None),
    }
}

/// Split the wire `c` (or the half of it a previous split made, `also`) at `at`, returning the
/// junction id.
fn split_wire(s: &mut Session, c: &str, also: &[String], at: Point, net: &str, dot: bool) -> Result<String, EngineError> {
    let new_id = s.doc.next_id("c");
    let junction = s.doc.next_id("j");
    let page = page_mut(s)?;
    // A plain connector drawn onto becomes a segment of the wire's net.
    if let Some(conn) = page.connector_mut(c)
        && conn.net.is_none()
    {
        conn.net = Some(net.to_string());
    }
    let candidates: Vec<String> = std::iter::once(c.to_string()).chain(also.iter().cloned()).collect();
    let target = candidates
        .iter()
        .filter_map(|id| Some((id.clone(), project(&route_polyline(page, page.connector(id)?), at)?.distance)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
        .unwrap_or_else(|| c.to_string());
    split_at(page, &target, at, &new_id, &junction, dot)
        .ok_or_else(|| EngineError::Params(format!("({}, {}) is at an end of `{}`: draw to that end instead", at.x, at.y, short(c))))?;
    Ok(junction)
}

fn draw(s: &mut Session, v: &Value) -> CmdResult {
    let from = end_spec(v, "from")?;
    let to = end_spec(v, "to")?;
    let module = s.doc.page().and_then(module_step);
    let points: Vec<Point> = points_param(v)?.into_iter().map(|p| snap(p, module)).collect();
    let kind = match super::opt_str(v, "kind")? {
        None => None,
        Some(k) => Some(NetKind::from_name(k).ok_or_else(|| EngineError::Params(format!("unknown net kind `{}`", short(k))))?),
    };
    let ix = NetIndex::build(&s.doc);
    let given = match super::opt_str(v, "net")? {
        Some(n) => {
            Some(s.doc.model.net(check_len("net", n)?).map(|x| x.id.clone()).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(n))))?)
        }
        None => None,
    };
    let (nf, nt) = (end_net(s, &ix, &from)?, end_net(s, &ix, &to)?);
    let net = match given.or(nf).or(nt) {
        Some(n) => n,
        None => {
            if s.doc.model.nets.len() >= vizcraft_model::net::MAX_NETS {
                return Err(EngineError::Failed("the model already holds the most nets it can".into()));
            }
            let id = s.doc.next_id("n");
            s.doc.model.nets.push(Net::new(id.clone(), kind.unwrap_or_default()));
            id
        }
    };
    let dot = s.doc.model.net(&net).is_none_or(|n| n.kind.dotted()) && !kind.is_some_and(|k| !k.dotted());
    let mut junctions: Vec<String> = Vec::new();
    let mut made: Vec<String> = Vec::new();
    let mut ends: Vec<End> = Vec::new();
    for spec in [&from, &to] {
        let end = match spec {
            Spec::Pin(sh, i) => End::point(sh.clone(), *i),
            Spec::Junction(j) => End::point(j.clone(), 0),
            Spec::Free(p) => End::free(snap(*p, module)),
            Spec::OnConnector(c, p) => {
                super::layer::ensure_unlocked(s, [c.as_str()])?;
                let before: Vec<String> = s.doc.page().map(|pg| pg.connectors.iter().map(|x| x.id.clone()).collect()).unwrap_or_default();
                let j = split_wire(s, c, &made, snap(*p, module), &net, dot)?;
                if let Some(pg) = s.doc.page() {
                    made.extend(pg.connectors.iter().map(|x| x.id.clone()).filter(|id| !before.contains(id)));
                }
                junctions.push(j.clone());
                End::point(j, 0)
            }
        };
        ends.push(end);
    }
    let (Some(start), Some(end)) = (ends.first().cloned(), ends.get(1).cloned()) else {
        return Err(EngineError::Failed("the wire has no ends".into()));
    };
    let id = s.doc.next_id("c");
    let wire = Connector {
        route: Route::Orthogonal,
        start_arrow: Arrow::None,
        end_arrow: Arrow::None,
        waypoints: points,
        net: Some(net.clone()),
        ..Connector::new(id.clone(), start, end)
    };
    page_mut(s)?.connectors.push(wire);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id, "net": net, "junctions": junctions}))
}

/// The two connection points of `shape` farthest apart (the first 64 are looked at).
fn outermost(shape: &vizcraft_diagram::Shape) -> Option<(usize, usize, Point, Point)> {
    let pts: Vec<Point> = shape.connection_points_world().into_iter().take(64).collect();
    let mut best: Option<(usize, usize, f64)> = None;
    for (i, a) in pts.iter().enumerate() {
        for (j, b) in pts.iter().enumerate().skip(i + 1) {
            let d = a.distance(*b);
            if d.is_finite() && best.is_none_or(|(_, _, bd)| d > bd) {
                best = Some((i, j, d));
            }
        }
    }
    let (i, j, _) = best?;
    Some((i, j, *pts.get(i)?, *pts.get(j)?))
}

fn splice(s: &mut Session, v: &Value) -> CmdResult {
    let shape_id = check_len("shape", p::req_str(v, "shape")?)?.to_string();
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let shape = page.shape(&shape_id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(&shape_id))))?;
    let (ia, ib, pa, pb) =
        outermost(shape).ok_or_else(|| EngineError::Params("the shape needs two connection points to splice into a wire".into()))?;
    let reach = shape.w.max(shape.h).max(1.0);
    let wire_id = match super::opt_str(v, "connector")? {
        Some(c) => {
            let c = check_len("connector", c)?;
            page.connector(c).ok_or_else(|| EngineError::Params(format!("`{}` is not a connector on this page", short(c))))?;
            c.to_string()
        }
        None => page
            .connectors
            .iter()
            .filter(|c| c.net.is_some() && !c.touches(&shape_id))
            .filter_map(|c| {
                let poly = route_polyline(page, c);
                let d = project(&poly, pa)?.distance.max(project(&poly, pb)?.distance);
                (d <= reach).then(|| (c.id.clone(), d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
            .ok_or_else(|| EngineError::Params(format!("no wire runs under `{}`", short(&shape_id))))?,
    };
    super::layer::ensure_unlocked(s, [shape_id.as_str(), wire_id.as_str()])?;
    super::protect::check(s, [shape_id.as_str()], super::protect::Lock::Glue)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let wire = page.connector(&wire_id).cloned().ok_or_else(|| EngineError::Params("no such wire".into()))?;
    if wire.touches(&shape_id) {
        return Err(EngineError::Params(format!("`{}` is already glued to `{}`", short(&wire_id), short(&shape_id))));
    }
    let poly = route_polyline(page, &wire);
    let (Some(qa), Some(qb)) = (project(&poly, pa), project(&poly, pb)) else {
        return Err(EngineError::Params("the wire has no route to splice into".into()));
    };
    if qa.segment != qb.segment {
        return Err(EngineError::Params("the device's pins fall on different runs of the wire; line it up with one straight run".into()));
    }
    let seg_start = poly.get(qa.segment).copied().unwrap_or(qa.point);
    // The pin nearer the wire's start takes the first half.
    let ((first, _), (second, _)) =
        if qa.point.distance(seg_start) <= qb.point.distance(seg_start) { ((ia, qa), (ib, qb)) } else { ((ib, qb), (ia, qa)) };
    let tail_id = s.doc.next_id("c");
    let mut head = wire.clone();
    head.end = End::point(shape_id.clone(), first);
    head.waypoints = poly.get(1..=qa.segment).unwrap_or_default().to_vec();
    head.end_arrow = Arrow::None;
    head.end_label.clear();
    head.avoid = false;
    let mut tail = wire.clone();
    tail.id = tail_id.clone();
    tail.start = End::point(shape_id.clone(), second);
    tail.waypoints = poly.get(qa.segment.saturating_add(1)..poly.len().saturating_sub(1)).unwrap_or_default().to_vec();
    tail.start_arrow = Arrow::None;
    tail.start_label.clear();
    tail.text.clear();
    tail.relationship = None;
    tail.avoid = false;
    let page = page_mut(s)?;
    if let Some(slot) = page.connector_mut(&wire_id) {
        *slot = head;
    }
    page.connectors.push(tail);
    Ok(json!({"connectors": [wire_id, tail_id], "shape": shape_id}))
}

fn heal(s: &mut Session, v: &Value) -> CmdResult {
    let shape_id = check_len("shape", p::req_str(v, "shape")?)?.to_string();
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    page.shape(&shape_id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(&shape_id))))?;
    let (a, b) = super::net::heal_pair(&s.doc, page, &shape_id).ok_or_else(|| {
        EngineError::Params(format!(
            "`{}` needs exactly one wire segment on each of two pins, not on two different named nets, to heal",
            short(&shape_id)
        ))
    })?;
    super::layer::ensure_unlocked(s, [shape_id.as_str(), a.as_str(), b.as_str()])?;
    super::protect::check(s, [shape_id.as_str()], super::protect::Lock::Delete)?;
    let page = page_mut(s)?;
    if !vizcraft_diagram::net::join(page, &a, &b, &shape_id) {
        return Err(EngineError::Failed("the wires could not be joined".into()));
    }
    let r = super::delete_items_heal(s, std::slice::from_ref(&shape_id), Some(false))?;
    let deleted = r.get("deleted").cloned().unwrap_or(json!([]));
    with_sel(s, json!({"connector": a, "deleted": deleted}))
}

fn set_module(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let raw = v.get("module").ok_or_else(|| EngineError::Params("`module` is required".into()))?;
    let module = Module::from_value(raw).ok_or_else(|| {
        EngineError::Params(format!(
            "`module` must be \"none\", \"iso2_5mm\", \"inch0_1\" or a number of page units from {} to {}",
            Module::MIN_STEP,
            Module::MAX_STEP
        ))
    })?;
    let page = s.doc.pages.get_mut(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    match &mut page.view {
        Some(view) => view.module = module,
        None if module.is_none() => {}
        None => page.view = Some(View { module, ..View::default() }),
    }
    // A view that holds nothing any more goes, so the page reads as before.
    if page.view.as_ref().is_some_and(|v| *v == View::default()) {
        page.view = None;
    }
    let id = page.id.clone();
    Ok(json!({"page": id, "module": module.to_value()}))
}

/// Move a freshly placed shape so its first connection point sits on the page's module grid.
pub(crate) fn snap_first_port(page: &mut vizcraft_diagram::Page, shape: &str) {
    let Some(m) = module_step(page) else { return };
    let Some(s) = page.shape_mut(shape) else { return };
    let Some(p) = s.connection_point_world(0) else { return };
    let (dx, dy) = (round_to(p.x, m) - p.x, round_to(p.y, m) - p.y);
    if dx.is_finite() && dy.is_finite() {
        s.x += dx;
        s.y += dy;
        s.sanitize();
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
