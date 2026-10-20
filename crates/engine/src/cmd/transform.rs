//! `shape.transform`: move, scale and turn a set of shapes and connectors as one object.
//!
//! The geometry is `vizcraft_diagram::arrange::rigid`, shared with the canvas so a group resize
//! or rotate drag previews exactly what this command makes. Glued connectors are rerouted by the
//! engine's routing pass after the command, inside the same undo step.

use serde_json::{Value, json};
use vizcraft_diagram::arrange::rigid::{self, Rigid};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{Point, Rect};

use super::protect::{self, Lock};
use super::{Kinds, MAX_DELTA, has_shapes, page_mut, targets};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Largest scale factor one command takes; a size is clamped to the model's extent anyway.
const MAX_SCALE: f64 = 1.0e6;
/// Smallest scale factor: anything nearer zero would collapse the selection to a point.
const MIN_SCALE: f64 = 1.0e-6;

/// The command.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("shape.transform", "Transform Selection", "Home > Arrange", transform)
            .params(r#"{"ids"?: [string] (default: selection), "move"?: {"dx": n, "dy": n}, "scale"?: {"sx": n, "sy": n, "about": {"x": n, "y": n}}, "rotate"?: {"angle": degrees (clockwise), "about": {"x": n, "y": n}}} -> {"ids", "bounds": {"x", "y", "w", "h"}}; applies move, then scale, then rotate to the items as one object: centres move, scale and turn about the pivots, sizes scale, angles grow; engineering symbols keep their size and turn in quarter turns (text upright); connector bends and free ends follow, ends glued inside stay glued, a connector glued to a shape outside keeps that end and is routed again; on a page with a symbol grid module the symbols' first pins and the bends land on the module; one undo step"#)
            .when(has_shapes),
    ]
}

/// A required object param `k` of `v`, `None` when absent or null.
fn part<'a>(v: &'a Value, k: &str) -> Result<Option<&'a serde_json::Map<String, Value>>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(o)),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be an object"))),
    }
}

/// A finite number `k` inside part `of`.
fn number(o: &serde_json::Map<String, Value>, of: &str, k: &str) -> Result<f64, EngineError> {
    o.get(k).and_then(Value::as_f64).filter(|f| f.is_finite()).ok_or_else(|| EngineError::Params(format!("`{of}.{k}` (a finite number) is required")))
}

/// The pivot `about: {x, y}` of part `of`.
fn pivot(o: &serde_json::Map<String, Value>, of: &str) -> Result<Point, EngineError> {
    let about = match o.get("about") {
        Some(Value::Object(a)) => a,
        _ => return Err(EngineError::Params(format!("`{of}.about` ({{\"x\", \"y\"}}) is required"))),
    };
    let x = number(about, &format!("{of}.about"), "x")?;
    let y = number(about, &format!("{of}.about"), "y")?;
    Ok(Point::new(x.clamp(-MAX_COORD, MAX_COORD), y.clamp(-MAX_COORD, MAX_COORD)))
}

/// The transform the params describe.
fn parse(v: &Value) -> Result<Rigid, EngineError> {
    let (mv, sc, rot) = (part(v, "move")?, part(v, "scale")?, part(v, "rotate")?);
    if mv.is_none() && sc.is_none() && rot.is_none() {
        return Err(EngineError::Params("give at least one of `move`, `scale` and `rotate`".into()));
    }
    let mut t = Rigid::default();
    if let Some(o) = mv {
        t.dx = number(o, "move", "dx")?.clamp(-MAX_DELTA, MAX_DELTA);
        t.dy = number(o, "move", "dy")?.clamp(-MAX_DELTA, MAX_DELTA);
    }
    if let Some(o) = sc {
        let (sx, sy) = (number(o, "scale", "sx")?, number(o, "scale", "sy")?);
        if sx < MIN_SCALE || sy < MIN_SCALE {
            return Err(EngineError::Params("`scale.sx` and `scale.sy` must be greater than zero".into()));
        }
        (t.sx, t.sy) = (sx.min(MAX_SCALE), sy.min(MAX_SCALE));
        t.scale_about = pivot(o, "scale")?;
    }
    if let Some(o) = rot {
        t.deg = number(o, "rotate", "angle")?.rem_euclid(360.0);
        t.rotate_about = pivot(o, "rotate")?;
    }
    Ok(t)
}

fn transform(s: &mut Session, v: &Value) -> CmdResult {
    let t = parse(v)?;
    let ids = targets(s, v, Kinds::Both)?;
    let all = s.doc.page().map(|p| vizcraft_diagram::arrange::with_descendants(p, &ids)).unwrap_or_default();
    super::layer::ensure_unlocked(s, &all)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let module = vizcraft_diagram::snap::module_step(page);
    // Worked out on a copy first, so the locks are judged on what actually changes: a shape at
    // the pivot of a scale does not move, a symbol's size does not change.
    let mut next = page.clone();
    let out = rigid::apply(&mut next, &ids, &t, module);
    for id in &out.shapes {
        let (Some(a), Some(b)) = (page.shape(id), next.shape(id)) else { continue };
        if a.center().distance(b.center()) > 1e-9 {
            protect::check(s, [id], Lock::Position)?;
        }
        if (a.w - b.w).abs() > 1e-9 || (a.h - b.h).abs() > 1e-9 {
            protect::check(s, [id], Lock::Size { w: b.w, h: b.h })?;
        }
        if (a.angle_deg - b.angle_deg).abs() > 1e-9 || a.orient != b.orient {
            protect::check(s, [id], Lock::Rotation)?;
        }
    }
    let bounds = items_bounds(&next, &ids, &out.carried);
    *page_mut(s)? = next;
    let mut changed = ids;
    for c in out.carried {
        if !changed.contains(&c) {
            changed.push(c);
        }
    }
    let bounds = bounds.map(|r| json!({"x": r.x0, "y": r.y0, "w": r.width(), "h": r.height()}));
    Ok(json!({"ids": changed, "bounds": bounds}))
}

/// Union of the bounds of the shapes among `ids` and the routes of the connectors among `ids`
/// and `carried`.
fn items_bounds(page: &vizcraft_diagram::Page, ids: &[String], carried: &[String]) -> Option<Rect> {
    let shapes = vizcraft_diagram::arrange::bounds_of(page, ids);
    let lines = page
        .connectors
        .iter()
        .filter(|c| ids.contains(&c.id) || carried.contains(&c.id))
        .filter_map(|c| Rect::bounding(vizcraft_diagram::route_polyline(page, c).into_iter().filter(|p| p.is_finite())));
    shapes.into_iter().chain(lines).reduce(|a, b| a.union(&b))
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
