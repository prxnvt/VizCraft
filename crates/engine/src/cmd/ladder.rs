//! Ladder commands (`ladder.*`): insert a ladder frame with its rails, change it, set devices on
//! its rungs and number its wires from the rungs.
//!
//! A ladder is an `electrical.Ladder` element drawn by the `ladder` builder: rung numbers down
//! the left, one rung every `spacing` modules, rails `width` modules apart. The rails are wires
//! (named potential nets L1 and L2, drawn as buses) from the frame's two ports to its bottom, so
//! devices wire to them like to any net. The sync pass gives each device inside a frame its
//! `rung`. Each command is one undo step.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};
use vizcraft_diagram::net::NetIndex;
use vizcraft_diagram::{Document, End, Point, Shape};
use vizcraft_model::{NetKind, PropValue};

use super::electrical::{call, coord, draw_now, place_device, str_list};
use super::model::{element_of, page_param};
use super::{opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The ladder frame type.
const LADDER: &str = "electrical.Ladder";

/// The wire number format `ladder.numberWires` uses by default: sheet, then the rung in two
/// digits (sheet 2, rung 3 gives 203).
pub const DEFAULT_FORMAT: &str = "{page}{rung:2}";

/// Suffix letters for second and later wires on one rung; I and O are skipped, as they read as
/// one and zero.
const SUFFIXES: &[char] = &['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z'];

/// Return the ladder commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ladder.insert", "Ladder", "Electrical > Ladder", insert).params(
            r#"{"page"?: page, "x": n, "y": n, "rungs"?: 1..200 (default 10), "spacing"?: 2..40 modules (default 4), "first"?: n (default 1), "increment"?: n (default 1), "width"?: 8..400 modules (default 40), "rails"?: bool (default true)} -> {"ladder", "shape", "rails": [connector id], "nets": [net id]}; a ladder frame with numbered rungs and its two rails, the potentials L1 and L2 (an existing net of that name is joined)"#,
        ),
        CommandSpec::new("ladder.set", "Ladder Settings", "Electrical > Ladder", set).params(
            r#"{"ladder": id, "rungs"?, "spacing"?, "first"?, "increment"?, "width"?} -> {"ladder", "rungs", "spacing", "first", "increment", "width"}; changes the frame and stretches its rails to the new bottom"#,
        ),
        CommandSpec::new("ladder.place", "Place on Rung", "Electrical > Ladder", place).params(
            r#"{"shapes"?: [shape id] (default: the selection), "rung"?: rung number, "ladder"?: id} -> {"moved": [{"shape", "rung"}]}; moves each device onto a rung of the ladder frame it sits in (or the one named): a device across the rung puts its pins on the rung line, an upright one its centre; without rung, the nearest"#,
        ),
        CommandSpec::new("ladder.numberWires", "Number Wires", "Electrical > Ladder", number_wires).params(
            r#"{"ladder"?: id, "page"?: page, "format"?: string (default "{page}{rung:2}")} -> {"numbered": [{"net", "number"}]}; numbers each unnamed wire inside a ladder frame from its sheet and rung ({page}, {rung}, with :W to zero-pad); a second wire on one rung gets A, the next B, skipping I and O"#,
        ),
    ]
}

/// A ladder frame's settings with defaults, clamped to the builder's ranges.
#[derive(Clone, Copy, Debug)]
struct Settings {
    rungs: i64,
    spacing: i64,
    first: i64,
    increment: i64,
    width: i64,
}

impl Settings {
    fn of(props: &BTreeMap<String, PropValue>) -> Self {
        let n = |k: &str, d: i64, lo: i64, hi: i64| {
            props.get(k).and_then(PropValue::as_f64).filter(|x| x.is_finite()).map(|x| x.round() as i64).unwrap_or(d).clamp(lo, hi)
        };
        Settings {
            rungs: n("rungs", 10, 1, vizcraft_stencils::builders::ladder::MAX_RUNGS),
            spacing: n("spacing", 4, 2, 40),
            first: n("first", 1, 0, 99_999),
            increment: n("increment", 1, 1, 1000),
            width: n("width", 40, 8, 400),
        }
    }
    fn json(&self) -> Value {
        json!({"rungs": self.rungs, "spacing": self.spacing, "first": self.first, "increment": self.increment, "width": self.width})
    }
    /// The rung number of rung index `i`.
    fn number(&self, i: i64) -> i64 {
        self.first.saturating_add(self.increment.saturating_mul(i))
    }
}

/// A drawn frame: page, shape and settings.
struct Frame {
    page: usize,
    shape: Shape,
    settings: Settings,
}

impl Frame {
    /// Height of one rung band in page units.
    fn step(&self) -> f64 {
        self.shape.h / self.settings.rungs as f64
    }
    /// The y of rung index `i`'s line.
    fn line_y(&self, i: i64) -> f64 {
        self.shape.y + self.step() * (i as f64 + 0.5)
    }
    /// The rung index nearest `y`, clamped to the frame.
    fn index_at(&self, y: f64) -> i64 {
        let s = self.step();
        if !(s.is_finite() && s > 0.0) {
            return 0;
        }
        (((y - self.shape.y) / s).floor() as i64).clamp(0, self.settings.rungs - 1)
    }
    fn contains(&self, p: Point) -> bool {
        p.x >= self.shape.x && p.x <= self.shape.x + self.shape.w && p.y >= self.shape.y && p.y <= self.shape.y + self.shape.h
    }
}

/// Every ladder frame drawn in `doc`.
fn frames(doc: &Document) -> Vec<Frame> {
    let mut out = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            if e.type_id == LADDER {
                out.push(Frame { page: pi, shape: s.clone(), settings: Settings::of(&e.properties) });
            }
        }
    }
    out
}

/// The frame of ladder element (or shape) `v[k]`.
fn frame_param(doc: &Document, v: &Value, k: &str) -> Result<Option<Frame>, EngineError> {
    let Some(raw) = opt_str(v, k)? else { return Ok(None) };
    let id = element_of(doc, raw).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element", short(raw))))?;
    if doc.model.element(&id).is_none_or(|e| e.type_id != LADDER) {
        return Err(EngineError::Params(format!("`{k}`: `{}` is not a ladder", short(raw))));
    }
    frames(doc)
        .into_iter()
        .find(|f| f.shape.element.as_deref() == Some(id.as_str()))
        .map(Some)
        .ok_or_else(|| EngineError::Params(format!("`{k}`: the ladder `{}` is not drawn", short(raw))))
}

fn setting(v: &Value, k: &str, lo: f64, hi: f64) -> Result<Option<f64>, EngineError> {
    Ok(super::num(v, k)?.map(|n| n.round().clamp(lo, hi)))
}

/// The settings params as element properties.
fn settings_props(v: &Value) -> Result<BTreeMap<String, PropValue>, EngineError> {
    let mut props = BTreeMap::new();
    for (k, lo, hi) in [("rungs", 1.0, 200.0), ("spacing", 2.0, 40.0), ("first", 0.0, 99_999.0), ("increment", 1.0, 1000.0), ("width", 8.0, 400.0)] {
        if let Some(n) = setting(v, k, lo, hi)? {
            props.insert(k.to_string(), PropValue::Number(n));
        }
    }
    Ok(props)
}

/// The named potential net `name`, if one exists.
fn net_named(doc: &Document, name: &str) -> Option<String> {
    doc.model.nets.iter().find(|n| n.name == name).map(|n| n.id.clone())
}

/// The world position of connection point `i` of shape `sid` on page `page`.
fn pin(doc: &Document, page: usize, sid: &str, i: usize) -> Option<Point> {
    doc.pages.get(page)?.shape(sid)?.connection_points_world().get(i).copied()
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let page = page_param(&s.doc, v)?;
    let x = coord(v, "x")?.ok_or_else(|| EngineError::Params("`x` is required".into()))?;
    let y = coord(v, "y")?.ok_or_else(|| EngineError::Params("`y` is required".into()))?;
    let props = settings_props(v)?;
    let rails = super::opt_bool(v, "rails")?.unwrap_or(true);
    let (ladder, _, shape) = place_device(&mut s.doc, LADDER, None, props, None, Some((page, x, y)))?;
    let shape = shape.ok_or_else(|| EngineError::Failed("the ladder was not drawn".into()))?;
    let mut wires = Vec::new();
    let mut nets = Vec::new();
    if rails {
        let active = s.doc.active_index();
        if page != active {
            s.doc.active_page = s.doc.pages.get(page).map(|p| p.id.clone()).unwrap_or_default();
        }
        let bottom = s.doc.pages.get(page).and_then(|p| p.shape(&shape)).map(|sh| sh.y + sh.h).unwrap_or(y);
        for (i, name) in [(0usize, "L1"), (1usize, "L2")] {
            let Some(at) = pin(&s.doc, page, &shape, i) else { continue };
            let mut params = json!({
                "from": {"shape": shape, "point": i},
                "to": {"x": at.x, "y": bottom},
                "points": [{"x": at.x, "y": (at.y + bottom) / 2.0}],
                "type": "electrical.bus",
            });
            let existing = net_named(&s.doc, name);
            if let (Some(n), Some(o)) = (existing.as_ref(), params.as_object_mut()) {
                o.insert("net".into(), json!(n));
            }
            let r = call(s, "wire.draw", params)?;
            let (Some(c), Some(net)) = (r["id"].as_str(), r["net"].as_str()) else { continue };
            // A rail runs straight down from its port; an orthogonal route would leave the port
            // outwards first.
            if let Some(conn) = s.doc.pages.get_mut(page).and_then(|p| p.connector_mut(c)) {
                conn.route = vizcraft_diagram::Route::Straight;
            }
            if existing.is_none()
                && let Some(n) = s.doc.model.net_mut(net)
            {
                n.name = name.to_string();
                n.kind = NetKind::Potential;
            }
            wires.push(c.to_string());
            nets.push(net.to_string());
        }
        if page != active {
            s.doc.active_page = s.doc.pages.get(active).map(|p| p.id.clone()).unwrap_or_default();
        }
    }
    if s.doc.active_index() == page {
        select(s, vec![shape.clone()]);
    }
    with_sel(s, json!({"ladder": ladder, "shape": shape, "rails": wires, "nets": nets}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let frame = frame_param(&s.doc, v, "ladder")?.ok_or_else(|| EngineError::Params("`ladder` (a ladder id) is required".into()))?;
    let id = frame.shape.element.clone().unwrap_or_default();
    let props = settings_props(v)?;
    if let Some(e) = s.doc.model.element_mut(&id) {
        for (k, val) in props {
            e.properties.insert(k, val);
        }
    }
    draw_now(&mut s.doc, frame.page, &frame.shape.id);
    // The rails follow the frame: their free ends go to its new bottom under their ports.
    let bottom = s.doc.pages.get(frame.page).and_then(|p| p.shape(&frame.shape.id)).map(|sh| sh.y + sh.h);
    let ports: Vec<Point> =
        s.doc.pages.get(frame.page).and_then(|p| p.shape(&frame.shape.id)).map(|sh| sh.connection_points_world()).unwrap_or_default();
    if let (Some(bottom), Some(page)) = (bottom, s.doc.pages.get_mut(frame.page)) {
        for c in page.connectors.iter_mut() {
            let (glued, free_end) = match (&c.start, &c.end) {
                (End::Glued { shape, point, .. }, End::Free { .. }) if *shape == frame.shape.id => (*point, true),
                _ => continue,
            };
            let Some(top) = glued.and_then(|i| ports.get(i)).copied() else { continue };
            if free_end {
                c.end = End::free(Point::new(top.x, bottom));
                c.waypoints = vec![Point::new(top.x, (top.y + bottom) / 2.0)];
                c.sanitize();
            }
        }
    }
    let settings = s.doc.model.element(&id).map(|e| Settings::of(&e.properties)).unwrap_or(frame.settings);
    let mut out = settings.json();
    if let Some(o) = out.as_object_mut() {
        o.insert("ladder".into(), json!(id));
    }
    Ok(out)
}

fn place(s: &mut Session, v: &Value) -> CmdResult {
    let shapes = match str_list(v, "shapes")? {
        Some(l) => l,
        None => s.sel.items.clone(),
    };
    if shapes.is_empty() {
        return Err(EngineError::Params("no shapes to place: give `shapes` or select them".into()));
    }
    let named = frame_param(&s.doc, v, "ladder")?;
    let rung = super::num(v, "rung")?.map(|n| n.round() as i64);
    let all = frames(&s.doc);
    let page = s.doc.active_index();
    let mut moved = Vec::new();
    for sid in shapes {
        let Some(sh) = s.doc.pages.get(page).and_then(|p| p.shape(&sid)).cloned() else {
            return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(&sid))));
        };
        if sh.element.as_deref().and_then(|e| s.doc.model.element(e)).is_some_and(|e| e.type_id == LADDER) {
            continue;
        }
        let c = sh.center();
        let frame = match &named {
            Some(f) => Some(f),
            None => all.iter().find(|f| f.page == page && f.contains(c)),
        };
        let Some(frame) = frame else { continue };
        let index = match rung {
            Some(r) => {
                let i = (r - frame.settings.first).div_euclid(frame.settings.increment.max(1));
                if !(0..frame.settings.rungs).contains(&i) {
                    return Err(EngineError::Params(format!("rung {r} is not on this ladder")));
                }
                i
            }
            None => frame.index_at(c.y),
        };
        let target = frame.line_y(index);
        let pins = sh.connection_points_world();
        let across = matches!((pins.first(), pins.get(1)), (Some(a), Some(b)) if (a.y - b.y).abs() < 0.5);
        let from = if across { pins.first().map(|p| p.y).unwrap_or(c.y) } else { c.y };
        let dy = target - from;
        if dy.abs() > 1e-9 {
            call(s, "shape.move", json!({"ids": [sid], "dx": 0.0, "dy": dy}))?;
        }
        moved.push(json!({"shape": sid, "rung": frame.settings.number(index)}));
    }
    with_sel(s, json!({"moved": moved}))
}

/// `format` with `{page}` and `{rung}` (each optionally `:W`, zero-padded to W digits) filled.
pub fn wire_number(format: &str, page: u32, rung: i64) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(rest.get(..open).unwrap_or_default());
        let after = rest.get(open + 1..).unwrap_or_default();
        let Some(close) = after.find('}') else {
            out.push_str(rest.get(open..).unwrap_or_default());
            return out;
        };
        let part = after.get(..close).unwrap_or_default();
        let (name, width) = match part.split_once(':') {
            Some((n, w)) => (n, w.parse::<usize>().unwrap_or(0).min(12)),
            None => (part, 0),
        };
        match name {
            "page" => out.push_str(&format!("{page:0width$}")),
            "rung" => out.push_str(&format!("{rung:0width$}")),
            _ => {
                out.push('{');
                out.push_str(part);
                out.push('}');
            }
        }
        rest = after.get(close + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

fn number_wires(s: &mut Session, v: &Value) -> CmdResult {
    let format = opt_str(v, "format")?.unwrap_or(DEFAULT_FORMAT);
    if format.len() > 64 {
        return Err(EngineError::Params("`format` is longer than 64 bytes".into()));
    }
    let named = frame_param(&s.doc, v, "ladder")?;
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let chosen: Vec<Frame> = match named {
        Some(f) => vec![f],
        None => frames(&s.doc).into_iter().filter(|f| page.is_none_or(|p| p == f.page)).collect(),
    };
    if chosen.is_empty() {
        return Err(EngineError::Params("no ladder to number: insert one first".into()));
    }
    let ix = NetIndex::build(&s.doc);
    // (net) -> (frame index, rung index, leftmost x), from the topmost point of the net inside a frame.
    let mut at: HashMap<String, (usize, i64, f64, f64)> = HashMap::new();
    for (fi, f) in chosen.iter().enumerate() {
        let Some(pg) = s.doc.pages.get(f.page) else { continue };
        for c in &pg.connectors {
            let Some(net) = ix.net_of_connector(&c.id) else { continue };
            if s.doc.model.net(net).is_some_and(|n| !n.name.is_empty()) {
                continue;
            }
            for p in vizcraft_diagram::route_polyline(pg, c) {
                if !f.contains(p) {
                    continue;
                }
                let e = at.entry(net.to_string()).or_insert((fi, f.index_at(p.y), p.x, p.y));
                if p.y < e.3 {
                    *e = (fi, f.index_at(p.y), e.2.min(p.x), p.y);
                } else {
                    e.2 = e.2.min(p.x);
                }
            }
        }
    }
    let mut by_rung: BTreeMap<(usize, i64), Vec<(f64, String)>> = BTreeMap::new();
    for (net, (fi, ri, x, _)) in at {
        by_rung.entry((fi, ri)).or_default().push((x, net));
    }
    let mut numbered = Vec::new();
    for ((fi, ri), mut nets) in by_rung {
        let Some(f) = chosen.get(fi) else { continue };
        nets.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let sheet = vizcraft_diagram::sheet::sheet_number(&s.doc, f.page);
        let base = wire_number(format, sheet, f.settings.number(ri));
        for (k, (_, net)) in nets.into_iter().enumerate() {
            let number = match k {
                0 => base.clone(),
                k => match SUFFIXES.get(k - 1) {
                    Some(c) => format!("{base}{c}"),
                    None => format!("{base}{k}"),
                },
            };
            if let Some(n) = s.doc.model.net_mut(&net) {
                n.number = number.clone();
            }
            numbered.push(json!({"net": net, "number": number}));
        }
    }
    Ok(json!({"numbered": numbered}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_numbers_compose_from_sheet_and_rung() {
        assert_eq!(wire_number(DEFAULT_FORMAT, 2, 3), "203");
        assert_eq!(wire_number("{page}{rung:3}", 2, 3), "2003");
        assert_eq!(wire_number("W{rung}", 1, 12), "W12");
        assert_eq!(wire_number("{page:2}-{rung:2}", 3, 7), "03-07");
        assert_eq!(wire_number("{other}{rung}", 1, 5), "{other}5");
        assert_eq!(wire_number("{rung", 1, 5), "{rung");
    }
}
