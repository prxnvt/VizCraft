//! `fluid.powerUnit`: a hydraulic power unit drawn in one step: an enclosure holding a reservoir,
//! a pump driven by an electric motor, a relief valve, a return filter and, as asked, a cooler, an
//! accumulator and a pressure gauge, joined pin to pin by typed working lines.
//!
//! It is a command rather than a `crates/patterns` pattern because a pattern's emissions carry no
//! element properties and no port ids. The steps first run in a scratch session, which hands out
//! the ids and reads where each built symbol put its ports; the resolved steps then run in the
//! person's session as one transaction: one undo step, and nothing changed if a step fails.
//!
//! The layout is on the page's module (2.5 mm unless the page has another): the pressure line
//! leaves the enclosure at the top right, the return line enters at the bottom right, so the unit
//! connects to a circuit drawn above it.

use serde_json::{Value, json};
use vizcraft_diagram::Point;

use super::super::model::page_param;
use super::super::{num, opt_bool, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return `fluid.powerUnit`.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("fluid.powerUnit", "Power Unit", "Circuit > Symbols", power_unit).params(
        r#"{"page"?: page id, name or index, "x"?: n, "y"?: n, "pump"?: "fixed"|"variable", "cooler"?: bool, "accumulator"?: bool, "gauge"?: bool (default true), "circuit"?: n (default 0)} -> {"enclosure", "elements": {role: element}, "shapes": {role: shape}, "outlet": connector, "inlet": connector}; an enclosure with a reservoir, a pump and its electric motor, a relief valve, a return filter and the parts asked for, every port joined; the pressure line ends free at the right (outlet), the return line starts free there (inlet); one undo step"#,
    )]
}

/// The scratch run: steps resolved so far and what they returned.
struct Plan {
    scratch: Session,
    steps: Vec<(String, Value)>,
}

impl Plan {
    fn run(&mut self, id: &str, params: Value) -> Result<Value, EngineError> {
        let r = self.scratch.run(id, &params).map_err(|e| EngineError::Failed(format!("power unit: {id} failed: {e}")))?;
        self.steps.push((id.to_string(), params));
        Ok(r)
    }

    /// The line segment `at` lies on, away from its ends (a tap splits it there).
    fn line_at(&self, at: Point) -> Result<String, EngineError> {
        let page = self.scratch.doc.page().ok_or_else(|| EngineError::Failed("power unit: no page".into()))?;
        page.connectors
            .iter()
            .filter(|c| c.net.is_some())
            .find(|c| {
                let poly = vizcraft_diagram::route_polyline(page, c);
                let ends = [poly.first(), poly.last()];
                vizcraft_diagram::net::project(&poly, at).is_some_and(|h| h.distance < 0.5) && ends.iter().flatten().all(|e| e.distance(at) > 0.5)
            })
            .map(|c| c.id.clone())
            .ok_or_else(|| EngineError::Failed("power unit: no line to tap".into()))
    }

    /// Where connection point `point` of `shape` is on the active page.
    fn pin(&self, shape: &str, point: usize) -> Result<Point, EngineError> {
        self.scratch
            .doc
            .page()
            .and_then(|p| p.shape(shape))
            .and_then(|s| s.connection_point_world(point))
            .ok_or_else(|| EngineError::Failed(format!("power unit: `{}` has no point {point}", short(shape))))
    }
}

/// The index of port `id` on `shape` (its built port ids).
fn port(p: &Plan, shape: &str, id: &str) -> Result<usize, EngineError> {
    p.scratch
        .doc
        .page()
        .and_then(|pg| pg.shape(shape))
        .and_then(|s| s.glyph.as_ref())
        .and_then(|g| g.ports.iter().position(|x| x == id))
        .ok_or_else(|| EngineError::Failed(format!("power unit: `{}` has no port {id}", short(shape))))
}

fn xy(p: Point) -> Value {
    json!({"x": p.x, "y": p.y})
}

fn power_unit(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let variable = match super::super::opt_str(v, "pump")? {
        None | Some("fixed") => false,
        Some("variable") => true,
        Some(x) => return Err(EngineError::Params(format!("`pump` is fixed or variable, not `{}`", short(x)))),
    };
    let cooler = opt_bool(v, "cooler")?.unwrap_or(false);
    let accumulator = opt_bool(v, "accumulator")?.unwrap_or(false);
    let gauge = opt_bool(v, "gauge")?.unwrap_or(true);
    let circuit = match num(v, "circuit")? {
        None => 0.0,
        Some(n) if (0.0..=999.0).contains(&n) && n.fract() == 0.0 => n,
        Some(_) => return Err(EngineError::Params("`circuit` is a whole number from 0 to 999".into())),
    };
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let m = vizcraft_diagram::snap::module_step(page).filter(|m| *m > 0.0).unwrap_or(vizcraft_stencils::builders::MODULE_METRIC);
    let centre = page.frame().center();
    let snap = |v: f64| (v / m).round() * m;
    let ox = snap(super::super::shape::coord(num(v, "x")?.unwrap_or(centre.x - 20.0 * m)));
    let oy = snap(super::super::shape::coord(num(v, "y")?.unwrap_or(centre.y - 15.0 * m)));
    let at = |dx: f64, dy: f64| Point::new(ox + dx * m, oy + dy * m);
    let mut scratch = Session::new(s.doc.clone());
    scratch.registry = s.registry.clone();
    scratch.view = s.view.clone();
    let mut p = Plan { scratch, steps: Vec::new() };
    if index != s.doc.active_index() {
        // Wires are drawn on the active page, so that page is shown first.
        let id = s.doc.pages.get(index).map(|pg| pg.id.clone()).unwrap_or_default();
        p.run("page.setActive", json!({"page": id}))?;
    }
    let props = json!({"circuit": circuit});
    let make = |p: &mut Plan, t: &str, dx: f64, dy: f64, extra: Value| -> Result<(String, String), EngineError> {
        let pos = at(dx, dy);
        let mut params = json!({"type": format!("fluidpower.{t}"), "x": pos.x, "y": pos.y, "properties": props});
        if let (Some(o), Value::Object(e)) = (params.as_object_mut(), extra) {
            for (k, x) in e {
                if let Some(Value::Object(pr)) = o.get_mut("properties") {
                    pr.insert(k, x);
                }
            }
        }
        let r = p.run("element.create", params)?;
        let e = r["id"].as_str().unwrap_or_default().to_string();
        let sh = r["shape"].as_str().unwrap_or_default().to_string();
        Ok((e, sh))
    };
    let right = if cooler { 44.0 } else { 38.0 };
    let (enc_e, enc) = make(&mut p, "Enclosure", -6.0, -2.0, json!({}))?;
    p.run("shape.resize", json!({"id": enc, "w": (right + 6.0) * m, "h": 36.0 * m}))?;
    let (tank_e, tank) = make(&mut p, "Reservoir", 8.0, 28.0, json!({}))?;
    let (pump_e, pump) = make(&mut p, "HydraulicPump", 6.0, 18.0, json!({"variable": variable}))?;
    let (motor_e, motor) = make(&mut p, "ElectricMotor", -4.0, 19.0, json!({}))?;
    let (relief_e, relief) = make(&mut p, "ReliefValve", 16.0, 14.0, json!({}))?;
    p.run("shape.mirror", json!({"ids": [relief], "axis": "y"}))?;
    let (filter_e, filter) = make(&mut p, "Filter", 24.0, 22.0, json!({}))?;
    let mut extras: Vec<(&str, String, String)> = Vec::new();
    let wire = |p: &mut Plan, from: Value, to: Value, points: &[Point]| -> Result<String, EngineError> {
        let pts: Vec<Value> = points.iter().map(|q| xy(*q)).collect();
        let r = p.run("wire.draw", json!({"from": from, "to": to, "points": pts, "type": "fluidpower.working"}))?;
        Ok(r["id"].as_str().unwrap_or_default().to_string())
    };
    let pin = |shape: &str, i: usize| json!({"shape": shape, "point": i});
    // The pressure line: pump P up, then right out of the enclosure.
    let pump_p = port(&p, &pump, "P")?;
    let outlet_at = at(right + 4.0, 10.0);
    let outlet = wire(&mut p, pin(&pump, pump_p), xy(outlet_at), &[at(10.0, 10.0)])?;
    let relief_p = port(&p, &relief, "P")?;
    let rp = p.pin(&relief, relief_p)?;
    let tap = |p: &Plan, x: f64| -> Result<Value, EngineError> {
        let q = Point::new(x, at(0.0, 10.0).y);
        Ok(json!({"connector": p.line_at(q)?, "x": q.x, "y": q.y}))
    };
    let to = tap(&p, rp.x)?;
    wire(&mut p, pin(&relief, relief_p), to, &[Point::new(rp.x, at(0.0, 12.0).y)])?;
    if gauge {
        let (e, sh) = make(&mut p, "PressureGauge", 22.0, 2.0, json!({}))?;
        let g = p.pin(&sh, 0)?;
        let to = tap(&p, g.x)?;
        wire(&mut p, pin(&sh, 0), to, &[Point::new(g.x, at(0.0, 9.0).y)])?;
        extras.push(("gauge", e, sh));
    }
    if accumulator {
        let (e, sh) = make(&mut p, "Accumulator", 27.0, 0.0, json!({}))?;
        let a = p.pin(&sh, 0)?;
        let to = tap(&p, a.x)?;
        wire(&mut p, pin(&sh, 0), to, &[Point::new(a.x, at(0.0, 9.0).y)])?;
        extras.push(("accumulator", e, sh));
    }
    // The suction and the relief's return to the reservoir.
    let pump_s = port(&p, &pump, "S")?;
    let t1 = port(&p, &tank, "T1")?;
    wire(&mut p, pin(&pump, pump_s), pin(&tank, t1), &[at(10.0, 27.0)])?;
    let relief_t = port(&p, &relief, "T")?;
    let t2 = port(&p, &tank, "T2")?;
    let rt = p.pin(&relief, relief_t)?;
    wire(&mut p, pin(&relief, relief_t), pin(&tank, t2), &[Point::new(rt.x, at(0.0, 25.0).y), at(12.0, 25.0)])?;
    // The return line: in at the right, through the cooler, through the filter, into the tank.
    let f2 = port(&p, &filter, "2")?;
    let f1 = port(&p, &filter, "1")?;
    let inlet_at = at(right + 4.0, 24.0);
    let inlet = if cooler {
        let (e, sh) = make(&mut p, "Cooler", 32.0, 22.0, json!({}))?;
        let c2 = port(&p, &sh, "2")?;
        let c1 = port(&p, &sh, "1")?;
        let w = wire(&mut p, xy(inlet_at), pin(&sh, c2), &[at(right + 2.0, 24.0)])?;
        wire(&mut p, pin(&sh, c1), pin(&filter, f2), &[at(31.0, 24.0)])?;
        extras.push(("cooler", e, sh));
        w
    } else {
        wire(&mut p, xy(inlet_at), pin(&filter, f2), &[at(right + 2.0, 24.0)])?
    };
    let t3 = port(&p, &tank, "T3")?;
    wire(&mut p, pin(&filter, f1), pin(&tank, t3), &[at(24.0, 27.0), at(14.0, 27.0)])?;
    // The motor drives the pump through the shaft.
    let shaft = port(&p, &pump, "shaft")?;
    p.run("connector.add", json!({"from": pin(&motor, 0), "to": pin(&pump, shaft), "kind": "fluidpower.mechanical"}))?;
    let mut members = vec![tank.clone(), pump.clone(), motor.clone(), relief.clone(), filter.clone()];
    members.extend(extras.iter().map(|(_, _, sh)| sh.clone()));
    p.run("container.addMember", json!({"container": enc, "ids": members}))?;
    s.transaction("Power Unit", &p.steps)?;
    let mut elements = json!({"reservoir": tank_e, "pump": pump_e, "motor": motor_e, "relief": relief_e, "filter": filter_e});
    let mut shapes = json!({"enclosure": enc, "reservoir": tank, "pump": pump, "motor": motor, "relief": relief, "filter": filter});
    for (role, e, sh) in &extras {
        elements[*role] = json!(e);
        shapes[*role] = json!(sh);
    }
    super::super::select(s, vec![enc.clone()]);
    Ok(json!({"enclosure": enc_e, "elements": elements, "shapes": shapes, "outlet": outlet, "inlet": inlet}))
}
