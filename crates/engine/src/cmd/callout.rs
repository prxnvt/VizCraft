//! Callout commands: a text shape aimed at a target shape, with a leader line drawn from the
//! callout to the target that follows either when it moves (it is worked out when drawn, see
//! `vizcraft_diagram::container::leader`). Each command is one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::Shape;
use vizcraft_diagram::container::{self as ct, Side};
use vizcraft_diagram::geom::MAX_COORD;

use super::container::pick;
use super::{has_shapes, num, opt_bool, opt_str, page_mut, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return all callout commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("callout.add", "Callout", "Insert > Diagram Parts", add)
            .params(r#"{"target"?: string, "text"?: string, "x"?: n, "y"?: n, "style"?: string, "id"?: string} -> {"id"}; target defaults to the selected shape; x, y is the top-left (default: up and right of the target); styles: box, plain, rounded, note, bubble"#)
            .when(has_shapes),
        CommandSpec::new("callout.set", "Callout Settings", "Callout", set)
            .params(r#"{"callout"?: string, "target"?: string, "leader"?: bool, "style"?: string, "side"?: "auto"|"top"|"bottom"|"left"|"right"} -> {"callout"}"#)
            .when(has_callouts),
    ]
}

/// Enabled when the active page has a callout.
fn has_callouts(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(ct::is_callout)) { None } else { Some("add a callout first") }
}

/// Anything but a callout can be a callout's target.
fn targetable(sh: &Shape) -> bool {
    !ct::is_callout(sh)
}

fn style_of(v: &Value) -> Result<Option<&'static ct::Preset>, EngineError> {
    match opt_str(v, "style")? {
        Some(name) => ct::callout_style(name).map(Some).ok_or_else(|| {
            let names = ct::CALLOUT_STYLES.iter().map(|p| p.name).collect::<Vec<_>>().join(", ");
            EngineError::Params(format!("`style` must be one of {names}, not `{}`", short(name)))
        }),
        None => Ok(None),
    }
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let target = pick(s, v, "target", targetable, "shape to point at")?;
    let text = opt_str(v, "text")?.unwrap_or("Callout").to_string();
    let preset = style_of(v)?;
    let (x, y) = (num(v, "x")?, num(v, "y")?);
    let id = super::new_id(s, v, "callout")?;
    let page = page_mut(s)?;
    let b = page.shape(&target).map(Shape::bounds).ok_or_else(|| EngineError::Failed("the target went away".into()))?;
    let x = x.unwrap_or(b.x1 + 60.0).clamp(-MAX_COORD, MAX_COORD);
    let y = y.unwrap_or(b.y0 - 80.0).clamp(-MAX_COORD, MAX_COORD);
    let mut c = ct::new_callout(id.clone(), &target, &text, x.round(), y.round());
    if let Some(p) = preset {
        ct::apply_preset(&mut c, p);
    }
    page.shapes.push(c);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let id = pick(s, v, "callout", ct::is_callout, "callout")?;
    super::layer::ensure_unlocked(s, [&id])?;
    let target = match opt_str(v, "target")? {
        Some(t) => match s.doc.page().and_then(|p| p.shape(t)) {
            Some(sh) if targetable(sh) && t != id => Some(t.to_string()),
            _ => return Err(EngineError::Params(format!("`{}` is not a shape a callout can point at", short(t)))),
        },
        None => None,
    };
    let leader = opt_bool(v, "leader")?;
    let side = match opt_str(v, "side")? {
        Some(t) => Some(
            Side::from_name(t).ok_or_else(|| EngineError::Params(format!("`side` must be auto, top, bottom, left or right, not `{}`", short(t))))?,
        ),
        None => None,
    };
    let preset = style_of(v)?;
    if target.is_none() && leader.is_none() && side.is_none() && preset.is_none() {
        return Err(EngineError::Params("give at least one of target, leader, style, side".into()));
    }
    let page = page_mut(s)?;
    let sh = page.shape_mut(&id).ok_or_else(|| EngineError::Failed("the callout went away".into()))?;
    if let Some(p) = preset {
        ct::apply_preset(sh, p);
    }
    if let Some(c) = sh.container.as_mut() {
        if target.is_some() {
            c.target = target;
        }
        if let Some(l) = leader {
            c.leader = l;
        }
        if let Some(sd) = side {
            c.side = sd;
        }
    }
    sh.sanitize();
    with_sel(s, json!({"callout": id}))
}
