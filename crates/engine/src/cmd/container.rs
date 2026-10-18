//! Container commands: add (around shapes or empty), members in and out, fit, settings, disband,
//! select contents, and `container.drop`, which settles membership after the canvas moves or
//! drops shapes. The geometry and rules live in `vizcraft_diagram::container`; these commands
//! parse params, check layer locks and report what changed. Each is one undo step.
//!
//! A command that names one container (`container`) falls back to the selected one, so the
//! ribbon's Container tab can run them without params.

use serde_json::{Value, json};
use vizcraft_diagram::container::{self as ct, HeadingPosition, HeadingStyle, Role};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{MAX_EXTENT, Shape, arrange};

use super::{Kinds, MAX_DELTA, MIN_SIZE, has_shapes, num, opt_bool, opt_str, page_mut, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return all container commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("container.add", "Container", "Insert > Diagram Parts", add)
            .params(r#"{"ids"?: [string], "x"?: n, "y"?: n, "w"?: n, "h"?: n, "heading"?: string, "style"?: string, "id"?: string} -> {"id", "members"}; around `ids` (else the selected shapes) fitted with margins, or empty at x, y (default: the page centre); styles: classic, outline, slate, sage, dashed, amber"#),
        CommandSpec::new("container.addMember", "Add to Container", "Container > Membership", add_member)
            .params(r#"{"container"?: string, "ids": [string]} -> {"container", "added"}; members draw above the container and move with it"#)
            .when(has_containers),
        CommandSpec::new("container.removeMember", "Remove from Container", "Container > Membership", remove_member)
            .params(r#"{"container"?: string, "ids"?: [string]} -> {"container", "removed"}; they move up to the container's parent"#)
            .when(has_containers),
        CommandSpec::new("container.fit", "Fit to Contents", "Container > Size", fit)
            .params(r#"{"container"?: string} -> {"container", "x", "y", "w", "h"}; members plus margins and heading"#)
            .when(has_containers),
        CommandSpec::new("container.set", "Container Settings", "Container", set)
            .params(r#"{"container"?: string, "margins"?: n, "heading"?: string, "headingPosition"?: "top"|"bottom"|"left"|"right", "headingStyle"?: "band"|"line"|"plain", "autoFit"?: bool, "locked"?: bool, "style"?: string} -> {"container"}"#)
            .when(has_containers),
        CommandSpec::new("container.disband", "Disband Container", "Container > Membership", disband)
            .params(r#"{"container"?: string} -> {"disbanded", "members"}; the members stay and are selected"#)
            .when(has_containers),
        CommandSpec::new("container.selectContents", "Select Contents", "Container > Membership", select_contents)
            .params(r#"{"container"?: string} -> {"items"}; the members (of every lane, for a lane set)"#)
            .when(has_containers),
        CommandSpec::new("container.drop", "Move into Containers", "Container > Membership", drop)
            .params(r#"{"ids"?: [string], "dx"?: n, "dy"?: n} -> {"changes": [{"id", "container"}]}; settles membership as if the shapes moved by dx, dy: a shape fully inside a container joins the innermost one, a member fully outside its container leaves it"#)
            .when(has_shapes),
    ]
}

/// Enabled when the active page has a container, lane set or lane.
fn has_containers(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(ct::is_container)) { None } else { Some("add a container first") }
}

fn ct_err(e: ct::ContainerError) -> EngineError {
    EngineError::Params(e.to_string())
}

/// The shape named by param `key`, which must pass `want`; without the param, the first selected
/// shape (the primary first) that does. `noun` names the kind in errors.
pub(super) fn pick(s: &Session, v: &Value, key: &str, want: fn(&Shape) -> bool, noun: &str) -> Result<String, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    match opt_str(v, key)? {
        Some(id) => match page.shape(id) {
            Some(sh) if want(sh) => Ok(id.to_string()),
            _ => Err(EngineError::Params(format!("`{}` is not a {noun} on this page", short(id)))),
        },
        None => s
            .sel
            .primary
            .iter()
            .chain(s.sel.items.iter())
            .find(|id| page.shape(id).is_some_and(want))
            .cloned()
            .ok_or_else(|| EngineError::Params(format!("select a {noun} first (or pass `{key}`)"))),
    }
}

/// A container that holds members (not a lane set, not a callout).
fn holds(sh: &Shape) -> bool {
    sh.container.as_ref().is_some_and(ct::Container::holds_members)
}

/// The container to act on, checked against layer locks.
fn the_container(s: &Session, v: &Value, want: fn(&Shape) -> bool) -> Result<String, EngineError> {
    let id = pick(s, v, "container", want, "container")?;
    super::layer::ensure_unlocked(s, [&id])?;
    Ok(id)
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let heading = opt_str(v, "heading")?.unwrap_or("Container").to_string();
    let preset = match opt_str(v, "style")? {
        Some(name) => Some(
            ct::container_style(name)
                .ok_or_else(|| EngineError::Params(format!("`style` must be one of {}, not `{}`", style_names(), short(name))))?,
        ),
        None => None,
    };
    let coord = |k: &str| num(v, k).map(|o| o.map(|x| x.clamp(-MAX_COORD, MAX_COORD)));
    let size = |k: &str| num(v, k).map(|o| o.map(|x| x.clamp(MIN_SIZE, MAX_EXTENT)));
    let (x, y, w, h) = (coord("x")?, coord("y")?, size("w")?, size("h")?);
    // Around the given ids; else around the selected shapes unless a place is given.
    let around = match super::ids_param(v)? {
        Some(_) => targets(s, v, Kinds::Shapes)?,
        None if x.is_none() && y.is_none() => targets(s, &json!({}), Kinds::Shapes).unwrap_or_default(),
        None => Vec::new(),
    };
    super::layer::ensure_unlocked(s, &around)?;
    let id = super::new_id(s, v, "box")?;
    let page = page_mut(s)?;
    let (dw, dh) = ct::DEFAULT_SIZE;
    let (w, h) = (w.unwrap_or(dw), h.unwrap_or(dh));
    let centre = page.frame().center();
    let (x, y) = (x.unwrap_or((centre.x - w / 2.0).round()), y.unwrap_or((centre.y - h / 2.0).round()));
    let mut shape = ct::new_container(id.clone(), x, y, w, h, &heading);
    if let Some(p) = preset {
        ct::apply_preset(&mut shape, p);
        ct::refresh(&mut shape);
    }
    let members = if around.is_empty() {
        page.shapes.push(shape);
        Vec::new()
    } else {
        ct::wrap(page, shape, &around).map_err(ct_err)?
    };
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id, "members": members}))
}

fn style_names() -> String {
    ct::CONTAINER_STYLES.iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
}

fn add_member(s: &mut Session, v: &Value) -> CmdResult {
    let c = the_container(s, v, holds)?;
    let ids = super::ids_param(v)?.ok_or_else(|| EngineError::Params("`ids` (array of shape ids) is required".into()))?;
    let ids = targets(s, &json!({"ids": ids}), Kinds::Shapes)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let added = ct::add_members(page_mut(s)?, &c, &ids).map_err(ct_err)?;
    with_sel(s, json!({"container": c, "added": added}))
}

fn remove_member(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Shapes)?;
    // Without a container, the selected members' own container.
    let c = match opt_str(v, "container")? {
        Some(_) => the_container(s, v, holds)?,
        None => {
            let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
            let parent = ids.iter().find_map(|id| page.shape(id).and_then(|sh| sh.parent.clone())).filter(|p| page.shape(p).is_some_and(holds));
            let c = parent.ok_or_else(|| EngineError::Params("the shapes are in no container (or pass `container`)".into()))?;
            super::layer::ensure_unlocked(s, [&c])?;
            c
        }
    };
    super::layer::ensure_unlocked(s, &ids)?;
    let removed = ct::remove_members(page_mut(s)?, &c, &ids).map_err(ct_err)?;
    with_sel(s, json!({"container": c, "removed": removed}))
}

fn fit(s: &mut Session, v: &Value) -> CmdResult {
    let c = the_container(s, v, |sh| ct::role(sh) == Some(Role::Container))?;
    let page = page_mut(s)?;
    if arrange::children(page, &c).is_empty() {
        return Err(EngineError::Params("the container is empty; add shapes before fitting it".into()));
    }
    ct::fit(page, &c);
    arrange::refit_groups(page);
    let sh = page.shape(&c).ok_or_else(|| EngineError::Failed("the container went away".into()))?;
    let out = json!({"container": c, "x": sh.x, "y": sh.y, "w": sh.w, "h": sh.h});
    with_sel(s, out)
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let c = the_container(s, v, ct::is_container)?;
    let margins = num(v, "margins")?;
    let heading = opt_str(v, "heading")?;
    let position = match opt_str(v, "headingPosition")? {
        Some(t) => Some(
            HeadingPosition::from_name(t)
                .ok_or_else(|| EngineError::Params(format!("`headingPosition` must be top, bottom, left or right, not `{}`", short(t))))?,
        ),
        None => None,
    };
    let heading_style = match opt_str(v, "headingStyle")? {
        Some(t) => Some(
            HeadingStyle::from_name(t)
                .ok_or_else(|| EngineError::Params(format!("`headingStyle` must be band, line or plain, not `{}`", short(t))))?,
        ),
        None => None,
    };
    let auto_fit = opt_bool(v, "autoFit")?;
    let locked = opt_bool(v, "locked")?;
    let preset = match opt_str(v, "style")? {
        Some(name) => Some(
            ct::container_style(name)
                .ok_or_else(|| EngineError::Params(format!("`style` must be one of {}, not `{}`", style_names(), short(name))))?,
        ),
        None => None,
    };
    if margins.is_none()
        && heading.is_none()
        && position.is_none()
        && heading_style.is_none()
        && auto_fit.is_none()
        && locked.is_none()
        && preset.is_none()
    {
        return Err(EngineError::Params("give at least one of margins, heading, headingPosition, headingStyle, autoFit, locked, style".into()));
    }
    let page = page_mut(s)?;
    let sh = page.shape_mut(&c).ok_or_else(|| EngineError::Failed("the container went away".into()))?;
    let lane_role = ct::role(sh).is_some_and(|r| r != Role::Container);
    if position.is_some() && lane_role {
        return Err(EngineError::Params("a lane set's and its lanes' headings follow the orientation; use lane.flip".into()));
    }
    if let Some(p) = preset {
        // A lane keeps its see-through body so the set's phases show.
        let fill = sh.style.fill;
        ct::apply_preset(sh, p);
        if ct::role(sh) == Some(Role::Lane) {
            sh.style.fill = fill;
        }
    }
    if let Some(t) = heading {
        sh.text = t.to_string();
    }
    if let Some(cs) = sh.container.as_mut() {
        if let Some(m) = margins {
            cs.margin = m;
        }
        if let Some(p) = position {
            cs.heading_position = p;
        }
        if let Some(hs) = heading_style {
            cs.heading_style = hs;
        }
        if let Some(a) = auto_fit {
            cs.auto_fit = a;
        }
        if let Some(l) = locked {
            cs.locked = l;
        }
    }
    sh.sanitize();
    ct::refresh(sh);
    arrange::refit_groups(page);
    with_sel(s, json!({"container": c}))
}

fn disband(s: &mut Session, v: &Value) -> CmdResult {
    let c = the_container(s, v, ct::is_container)?;
    let members = ct::disband(page_mut(s)?, &c).map_err(ct_err)?;
    select(s, members.clone());
    super::prune_selection(s);
    with_sel(s, json!({"disbanded": c, "members": members}))
}

fn select_contents(s: &mut Session, v: &Value) -> CmdResult {
    let c = pick(s, v, "container", ct::is_container, "container")?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let items: Vec<String> = match page.shape(&c).and_then(ct::role) {
        Some(Role::LaneSet) => ct::lanes(page, &c).iter().flat_map(|l| arrange::children(page, l)).collect(),
        _ => arrange::children(page, &c),
    };
    if items.is_empty() {
        return Err(EngineError::Params("the container is empty".into()));
    }
    select(s, items.clone());
    Ok(json!({"items": items, "primary": s.sel.primary}))
}

fn drop(s: &mut Session, v: &Value) -> CmdResult {
    let dx = num(v, "dx")?.unwrap_or(0.0).clamp(-MAX_DELTA, MAX_DELTA);
    let dy = num(v, "dy")?.unwrap_or(0.0).clamp(-MAX_DELTA, MAX_DELTA);
    let ids: Vec<String> = {
        let all = targets(s, v, Kinds::Both)?;
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        all.into_iter().filter(|id| page.shape(id).is_some()).collect()
    };
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let changes = ct::drop_changes(page, &ids, dx, dy);
    // The containers a change touches must be on unlocked layers too.
    let touched: Vec<String> =
        changes.iter().flat_map(|(id, to)| [Some(id.clone()), to.clone(), page.shape(id).and_then(|sh| sh.parent.clone())]).flatten().collect();
    super::layer::ensure_unlocked(s, &touched)?;
    ct::apply_changes(page_mut(s)?, &changes);
    let out: Vec<Value> = changes.iter().map(|(id, to)| json!({"id": id, "container": to})).collect();
    with_sel(s, json!({"changes": out}))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod frame_tests {
    use serde_json::json;
    use vizcraft_diagram::Document;

    use crate::Session;

    #[test]
    fn a_container_on_an_empty_infinite_board_lands_at_the_frame_centre() {
        let mut s = Session::new(Document::new());
        let centre = s.doc.page().unwrap().frame().center();
        let r = s.run("container.add", &json!({})).unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let b = s.doc.page().unwrap().shape(&id).unwrap();
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        assert!((cx - centre.x).abs() <= 1.0 && (cy - centre.y).abs() <= 1.0, "centre ({cx}, {cy}) against frame centre {centre:?}");
    }
}
