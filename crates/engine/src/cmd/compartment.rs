//! Compartment commands: add, set, remove and move the sections stacked inside a shape below its
//! main text (see `vizcraft_diagram::compartment`). The layout and the slot clamp live in the
//! diagram crate; these commands parse params, check layer locks, keep slot membership and
//! connector ports consistent, and grow the shape when its content no longer fits. Each is one
//! undo step.
//!
//! The `text.*` commands reach a compartment through [`each`] and [`fit_shape`] when given a
//! `compartment` id.

use serde_json::{Value, json};
use vizcraft_diagram::compartment::{self as cp, Compartment, CompartmentKind, Height};
use vizcraft_diagram::container::{self as ct, Role};
use vizcraft_diagram::{Color, End, HAlign, MAX_EXTENT, Page, Shape, Style, arrange};

use super::{has_shapes, num, opt_bool, opt_str, page_mut, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all compartment commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("compartment.add", "Compartment", "Insert > Diagram Parts", add)
            .params(r#"{"id": shape, "kind"?: "paragraph"|"list"|"slot", "title"?: string, "text"?: string, "rows"?: [string], "index"?: n, "compartmentId"?: string, "members"?: [shape]} -> {"id", "compartment", "index"}; a band below the main text (appended by default); the shape grows to fit; a slot's members become children of the shape and stay inside the slot"#)
            .when(has_shapes),
        CommandSpec::new("compartment.set", "Compartment Settings", "Format > Compartment", set)
            .params(r##"{"id": shape, "compartment": id, "title"?: string|null, "text"?: string, "rows"?: [string], "align"?: "left"|"center"|"right"|"justify", "height"?: "auto"|n, "fontSize"?: n|null, "bold"?: bool|null, "fill"?: "#rrggbb"|"none"|null, "fontColor"?: "#rrggbb"|null, "members"?: [shape]} -> {"id", "compartment"}; null clears an override; members replace a slot's members"##)
            .when(has_compartments),
        CommandSpec::new("compartment.remove", "Remove Compartment", "Format > Compartment", remove)
            .params(r#"{"id": shape, "compartment": id} -> {"id", "removed", "freed"}; a slot's members move up to the shape's parent; connectors glued to its ports keep dynamic glue to the shape"#)
            .when(has_compartments),
        CommandSpec::new("compartment.move", "Move Compartment", "Format > Compartment", move_to)
            .params(r#"{"id": shape, "compartment": id, "index": n} -> {"id", "compartment", "index"}; 0 is just below the main text"#)
            .when(has_compartments),
    ]
}

/// Enabled when a shape on the active page has a compartment.
fn has_compartments(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(|x| !x.compartments.is_empty())) { None } else { Some("add a compartment first") }
}

fn bad(k: &str, what: &str) -> EngineError {
    EngineError::Params(format!("`{k}` must be {what}"))
}

/// Whether a shape may hold compartments: not a group (it draws nothing) and not a container,
/// lane set, lane or callout (their text is a heading and their members are their own).
fn may_hold(sh: &Shape) -> bool {
    !arrange::is_group(sh) && sh.container.is_none()
}

/// The shape named by `id`, checked to hold compartments and to be unlocked.
fn the_shape(s: &Session, v: &Value) -> Result<String, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let id = p::req_str(v, "id")?;
    match page.shape(id) {
        Some(sh) if may_hold(sh) => {}
        Some(_) => return Err(EngineError::Params(format!("`{}` is a group or container; compartments go on plain shapes", short(id)))),
        None => return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(id)))),
    }
    super::layer::ensure_unlocked(s, [id])?;
    Ok(id.to_string())
}

/// Index of compartment `cid` in shape `id`.
fn index_of(page: &Page, id: &str, cid: &str) -> Result<usize, EngineError> {
    page.shape(id)
        .and_then(|sh| sh.compartments.iter().position(|c| c.id == cid))
        .ok_or_else(|| EngineError::Params(format!("`{}` has no compartment `{}`", short(id), short(cid))))
}

/// The `compartment` param of a text command: `None` when absent.
pub(super) fn param(v: &Value) -> Result<Option<String>, EngineError> {
    Ok(opt_str(v, "compartment")?.map(str::to_string))
}

/// An optional list of strings, at most `max` long.
fn strings(v: &Value, k: &str, max: usize) -> Result<Option<Vec<String>>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= max => {
            a.iter().map(|x| x.as_str().map(str::to_string).ok_or_else(|| bad(k, "an array of strings"))).collect::<Result<Vec<_>, _>>().map(Some)
        }
        Some(Value::Array(_)) => Err(bad(k, &format!("an array of at most {max} strings"))),
        Some(_) => Err(bad(k, "an array of strings")),
    }
}

fn color(v: &Value, k: &str) -> Result<Option<Color>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => Color::parse(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not a #rrggbb colour", short(t)))),
    }
}

/// Grow shape `id` (never shrink it) so its main text and compartments fit, keeping its top
/// left; then settle slot members. Moving or resizing goes through `arrange`, so a shape inside a
/// group or container refits it.
pub(super) fn fit_shape(page: &mut Page, id: &str) {
    let Some(sh) = page.shape(id) else { return };
    let need = cp::fit_height(sh);
    if need > sh.h + 1e-9 {
        let (x, y, w) = (sh.x, sh.y, sh.w);
        arrange::set_frame(page, id, x, y, w, need.min(MAX_EXTENT));
    } else {
        arrange::refit_groups(page);
    }
}

/// Run `f` on compartment `cid` of each shape in `ids` that has one with that id, sanitise it and
/// grow the shape to fit. Returns the shapes changed; none is a params error.
pub(super) fn each(s: &mut Session, ids: &[String], cid: &str, mut f: impl FnMut(&mut Compartment, &Style)) -> Result<Vec<String>, EngineError> {
    super::layer::ensure_unlocked(s, ids)?;
    let page = page_mut(s)?;
    let mut changed = Vec::new();
    for id in ids {
        let Some(sh) = page.shape_mut(id) else { continue };
        let base = sh.style.clone();
        let Some(c) = sh.compartments.iter_mut().find(|c| c.id == cid) else { continue };
        f(c, &base);
        c.sanitize();
        changed.push(id.clone());
    }
    if changed.is_empty() {
        return Err(EngineError::Params(format!("no target shape has a compartment `{}`", short(cid))));
    }
    for id in &changed {
        fit_shape(page, id);
    }
    Ok(changed)
}

/// Check `ids` may join a slot of `owner`: shapes on the page, not the owner or one of its
/// ancestors, not a lane.
fn check_members(page: &Page, owner: &str, ids: &[String]) -> Result<(), EngineError> {
    let ancestors = arrange::ancestors(page, owner);
    for m in ids {
        let Some(ms) = page.shape(m) else { return Err(EngineError::Params(format!("`members`: `{}` is not a shape on this page", short(m)))) };
        if m == owner || ancestors.contains(m) || ct::role(ms) == Some(Role::Lane) {
            return Err(EngineError::Params(format!("`members`: `{}` cannot go inside `{}`", short(m), short(owner))));
        }
    }
    Ok(())
}

/// Make `ids` the members of slot `cid` of `owner`: they leave any other slot, take the owner as
/// their parent and draw above it; former members that left the list move up to the owner's
/// parent. The slot clamp runs when the shape is refitted.
fn set_members(page: &mut Page, owner: &str, cid: &str, ids: &[String]) {
    let mut wanted: Vec<String> = Vec::new();
    for m in ids {
        if !wanted.contains(m) {
            wanted.push(m.clone());
        }
    }
    wanted.truncate(cp::MAX_MEMBERS);
    let up = page.shape(owner).and_then(|o| o.parent.clone());
    let old: Vec<String> = page.shape(owner).and_then(|o| o.compartments.iter().find(|c| c.id == cid)).map(|c| c.members.clone()).unwrap_or_default();
    // One slot per member: take them out of every other slot on the page.
    for sh in &mut page.shapes {
        for c in &mut sh.compartments {
            if !(sh.id == owner && c.id == cid) {
                c.members.retain(|m| !wanted.contains(m));
            }
        }
    }
    if let Some(c) = page.shape_mut(owner).and_then(|o| o.compartments.iter_mut().find(|c| c.id == cid)) {
        c.members = wanted.clone();
    }
    let mut changes: Vec<(String, Option<String>)> = Vec::new();
    for m in &old {
        if !wanted.contains(m) && page.shape(m).is_some_and(|ms| ms.parent.as_deref() == Some(owner)) {
            changes.push((m.clone(), up.clone()));
        }
    }
    changes.extend(wanted.iter().map(|m| (m.clone(), Some(owner.to_string()))));
    ct::apply_changes(page, &changes);
}

/// The compartment as a result value.
fn json_of(page: &Page, id: &str, cid: &str) -> Value {
    page.shape(id).and_then(|sh| sh.compartments.iter().find(|c| c.id == cid)).map_or(Value::Null, |c| json!(c))
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    let kind = match opt_str(v, "kind")? {
        None => CompartmentKind::Paragraph,
        Some(k) => CompartmentKind::from_name(k).ok_or_else(|| bad("kind", "one of paragraph, list, slot"))?,
    };
    let title = opt_str(v, "title")?.map(str::to_string);
    let text = opt_str(v, "text")?.map(str::to_string);
    let rows = strings(v, "rows", cp::MAX_ROWS)?;
    let members = strings(v, "members", cp::MAX_MEMBERS)?;
    if members.is_some() && kind != CompartmentKind::Slot {
        return Err(bad("members", "given only for a slot"));
    }
    let index = num(v, "index")?;
    let chosen = opt_str(v, "compartmentId")?.map(str::to_string);
    let page = page_mut(s)?;
    let list = page.shape(&id).map(|sh| sh.compartments.clone()).unwrap_or_default();
    if list.len() >= cp::MAX_COMPARTMENTS {
        return Err(EngineError::Params(format!("`{}` already has the most compartments a shape holds ({})", short(&id), cp::MAX_COMPARTMENTS)));
    }
    let cid = match chosen {
        Some(c) if !cp::is_valid_id(&c) => return Err(bad("compartmentId", "1 to 64 letters, digits, `_`, `-` or `.`")),
        Some(c) if list.iter().any(|x| x.id == c) => {
            return Err(EngineError::Params(format!("`{}` already has a compartment `{}`", short(&id), short(&c))));
        }
        Some(c) => c,
        None => cp::free_id(&list, "c"),
    };
    if let Some(m) = &members {
        check_members(page, &id, m)?;
    }
    let mut c = Compartment::new(cid.clone(), kind);
    c.title = title;
    if let Some(t) = text {
        c.text = t;
    }
    if let Some(r) = rows {
        c.rows = r;
    }
    c.sanitize();
    let at = index.map_or(list.len(), |i| i.clamp(0.0, list.len() as f64) as usize);
    if let Some(sh) = page.shape_mut(&id) {
        sh.compartments.insert(at.min(sh.compartments.len()), c);
        sh.sanitize();
    }
    if let Some(m) = &members {
        set_members(page, &id, &cid, m);
    }
    fit_shape(page, &id);
    let out = json!({"id": id, "compartment": json_of(page, &id, &cid), "index": at});
    with_sel(s, out)
}

/// The settings `compartment.set` was given, parsed before anything changes. The outer `Option`
/// is "given", the inner one "set or cleared".
#[derive(Default)]
struct Patch {
    title: Option<Option<String>>,
    text: Option<String>,
    rows: Option<Vec<String>>,
    align: Option<HAlign>,
    height: Option<Height>,
    font_size: Option<Option<f64>>,
    bold: Option<Option<bool>>,
    fill: Option<Option<Color>>,
    font_color: Option<Option<Color>>,
    members: Option<Vec<String>>,
}

/// `Some(None)` when the param is an explicit null, `Some(Some(_))` when `get` finds a value.
fn clearable<T>(v: &Value, k: &str, get: impl Fn() -> Result<Option<T>, EngineError>) -> Result<Option<Option<T>>, EngineError> {
    match v.get(k) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(_) => Ok(get()?.map(Some)),
    }
}

impl Patch {
    fn parse(v: &Value) -> Result<Self, EngineError> {
        let height = match v.get("height") {
            None | Some(Value::Null) => None,
            Some(Value::String(t)) if t == "auto" => Some(Height::Auto),
            Some(Value::Number(_)) => num(v, "height")?.map(Height::fixed),
            Some(_) => return Err(bad("height", "\"auto\" or a number")),
        };
        let fill = match v.get("fill") {
            Some(Value::String(t)) if t == "none" => Some(None),
            _ => clearable(v, "fill", || color(v, "fill"))?,
        };
        let patch = Patch {
            title: clearable(v, "title", || Ok(opt_str(v, "title")?.map(str::to_string)))?,
            text: opt_str(v, "text")?.map(str::to_string),
            rows: strings(v, "rows", cp::MAX_ROWS)?,
            align: match opt_str(v, "align")? {
                None => None,
                Some(a) => Some(HAlign::from_name(a).ok_or_else(|| bad("align", "one of left, center, right, justify"))?),
            },
            height,
            font_size: clearable(v, "fontSize", || num(v, "fontSize"))?,
            bold: clearable(v, "bold", || opt_bool(v, "bold"))?,
            fill,
            font_color: clearable(v, "fontColor", || color(v, "fontColor"))?,
            members: strings(v, "members", cp::MAX_MEMBERS)?,
        };
        let none = patch.title.is_none()
            && patch.text.is_none()
            && patch.rows.is_none()
            && patch.align.is_none()
            && patch.height.is_none()
            && patch.font_size.is_none()
            && patch.bold.is_none()
            && patch.fill.is_none()
            && patch.font_color.is_none()
            && patch.members.is_none();
        if none {
            return Err(EngineError::Params(
                "give at least one of title, text, rows, align, height, fontSize, bold, fill, fontColor, members".into(),
            ));
        }
        Ok(patch)
    }

    fn apply(&self, c: &mut Compartment) {
        if let Some(t) = &self.title {
            c.title = t.clone();
        }
        if let Some(t) = &self.text {
            c.text = t.clone();
        }
        if let Some(r) = &self.rows {
            c.rows = r.clone();
        }
        if let Some(a) = self.align {
            c.align = a;
        }
        if let Some(h) = self.height {
            c.height = h;
        }
        if let Some(f) = self.font_size {
            c.font_size = f;
        }
        if let Some(b) = self.bold {
            c.bold = b;
        }
        if let Some(f) = self.fill {
            c.fill = f;
        }
        if let Some(f) = self.font_color {
            c.font_color = f;
        }
        c.sanitize();
    }
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    let cid = p::req_str(v, "compartment")?.to_string();
    let patch = Patch::parse(v)?;
    let page = page_mut(s)?;
    let i = index_of(page, &id, &cid)?;
    if let Some(m) = &patch.members {
        if page.shape(&id).and_then(|sh| sh.compartments.get(i)).is_some_and(|c| c.kind != CompartmentKind::Slot) {
            return Err(bad("members", "given only for a slot"));
        }
        check_members(page, &id, m)?;
    }
    if let Some(c) = page.shape_mut(&id).and_then(|sh| sh.compartments.get_mut(i)) {
        patch.apply(c);
    }
    if let Some(m) = &patch.members {
        set_members(page, &id, &cid, m);
    }
    fit_shape(page, &id);
    let out = json!({"id": id, "compartment": json_of(page, &id, &cid)});
    with_sel(s, out)
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    let cid = p::req_str(v, "compartment")?.to_string();
    let page = page_mut(s)?;
    let i = index_of(page, &id, &cid)?;
    let (removed, up) = match page.shape_mut(&id) {
        Some(sh) if i < sh.compartments.len() => (sh.compartments.remove(i), sh.parent.clone()),
        _ => return Err(EngineError::Failed("the compartment went away".into())),
    };
    let freed: Vec<String> =
        removed.members.iter().filter(|m| page.shape(m).is_some_and(|ms| ms.parent.as_deref() == Some(id.as_str()))).cloned().collect();
    let changes: Vec<(String, Option<String>)> = freed.iter().map(|m| (m.clone(), up.clone())).collect();
    ct::apply_changes(page, &changes);
    // Ends glued to its ports keep their shape with dynamic glue.
    let prefix = format!("{cid}.");
    for c in &mut page.connectors {
        for end in [&mut c.start, &mut c.end] {
            if let End::Glued { shape, port, .. } = end
                && shape == &id
                && port.as_deref().is_some_and(|p| p.strip_prefix(prefix.as_str()).is_some_and(|side| !side.contains('.')))
            {
                *port = None;
            }
        }
    }
    arrange::refit_groups(page);
    with_sel(s, json!({"id": id, "removed": cid, "freed": freed}))
}

fn move_to(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    let cid = p::req_str(v, "compartment")?.to_string();
    let index = num(v, "index")?.ok_or_else(|| EngineError::Params("`index` (number) is required".into()))?;
    let page = page_mut(s)?;
    let i = index_of(page, &id, &cid)?;
    let mut at = 0;
    if let Some(sh) = page.shape_mut(&id)
        && i < sh.compartments.len()
    {
        let c = sh.compartments.remove(i);
        at = (index.clamp(0.0, sh.compartments.len() as f64) as usize).min(sh.compartments.len());
        sh.compartments.insert(at, c);
    }
    arrange::refit_groups(page);
    with_sel(s, json!({"id": id, "compartment": cid, "index": at}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Document, ShapeKind};

    use super::*;

    fn session() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        let mut k = Shape::new("k", ShapeKind::Rectangle, 100.0, 100.0, 160.0, 40.0);
        k.text = "Order".into();
        page.shapes.push(k);
        page.shapes.push(Shape::new("o", ShapeKind::Rectangle, 500.0, 100.0, 60.0, 40.0));
        page.shapes.push(Shape::new("m", ShapeKind::Ellipse, 900.0, 900.0, 20.0, 20.0));
        doc.sanitize();
        Session::new(doc)
    }

    fn shape<'a>(s: &'a Session, id: &str) -> &'a Shape {
        s.doc.page().unwrap().shape(id).unwrap()
    }

    #[test]
    fn add_set_move_and_remove_build_a_class() {
        let mut s = session();
        let r =
            s.run("compartment.add", &json!({"id": "k", "kind": "list", "compartmentId": "attrs", "rows": ["+ id: int", "+ total: money"]})).unwrap();
        assert_eq!(r["compartment"]["rows"], json!(["+ id: int", "+ total: money"]));
        let r = s.run("compartment.add", &json!({"id": "k", "kind": "list", "rows": ["+ pay()"]})).unwrap();
        assert_eq!((r["compartment"]["id"].as_str(), r["index"].as_u64()), (Some("c1"), Some(1)));
        let k = shape(&s, "k");
        assert!(k.h >= cp::fit_height(k) - 1e-9 && k.h > 40.0, "the shape grew to fit: {}", k.h);
        s.run(
            "compartment.set",
            &json!({"id": "k", "compartment": "c1", "title": "Operations", "fontSize": 10, "bold": true, "fill": "#eef2f7", "align": "center"}),
        )
        .unwrap();
        let c = &shape(&s, "k").compartments[1];
        assert_eq!((c.title.as_deref(), c.font_size, c.bold, c.align), (Some("Operations"), Some(10.0), Some(true), HAlign::Center));
        s.run("compartment.set", &json!({"id": "k", "compartment": "c1", "title": null, "fontSize": null, "fill": "none", "height": 50})).unwrap();
        let c = &shape(&s, "k").compartments[1];
        assert_eq!((c.title.as_deref(), c.font_size, c.fill, c.height), (None, None, None, Height::Fixed(50.0)));
        let r = s.run("compartment.move", &json!({"id": "k", "compartment": "c1", "index": 0})).unwrap();
        assert_eq!(r["index"], json!(0));
        assert_eq!(shape(&s, "k").compartments[0].id, "c1");
        s.run("compartment.remove", &json!({"id": "k", "compartment": "c1"})).unwrap();
        assert_eq!(shape(&s, "k").compartments.len(), 1);
        // Undo brings it back.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(shape(&s, "k").compartments.len(), 2);
        // shape.get lists them.
        let r = s.run("shape.get", &json!({"id": "k"})).unwrap();
        assert_eq!(r["shape"]["compartments"][1]["id"], json!("attrs"));
    }

    #[test]
    fn bad_params_are_refused_without_changes() {
        let mut s = session();
        s.run("compartment.add", &json!({"id": "k", "compartmentId": "a"})).unwrap();
        let before = s.doc.clone();
        for v in [
            json!({"id": "k", "kind": "table"}),
            json!({"id": "k", "compartmentId": "a"}),
            json!({"id": "k", "compartmentId": "has space"}),
            json!({"id": "k", "rows": [1]}),
            json!({"id": "k", "members": ["m"]}),
            json!({"id": "k", "kind": "slot", "members": ["k"]}),
            json!({"id": "k", "kind": "slot", "members": ["nope"]}),
            json!({"id": "nope"}),
        ] {
            assert!(s.run("compartment.add", &v).is_err(), "{v}");
        }
        for v in [
            json!({"id": "k", "compartment": "a"}),
            json!({"id": "k", "compartment": "zz", "text": "x"}),
            json!({"id": "k", "compartment": "a", "height": "tall"}),
            json!({"id": "k", "compartment": "a", "fill": "red"}),
            json!({"id": "k", "compartment": "a", "align": "up"}),
            json!({"id": "k", "compartment": "a", "members": ["m"]}),
        ] {
            assert!(s.run("compartment.set", &v).is_err(), "{v}");
        }
        assert!(s.run("compartment.move", &json!({"id": "k", "compartment": "a"})).is_err());
        assert!(s.run("compartment.remove", &json!({"id": "k", "compartment": "zz"})).is_err());
        assert_eq!(s.doc, before);
        for _ in 1..cp::MAX_COMPARTMENTS {
            s.run("compartment.add", &json!({"id": "k"})).unwrap();
        }
        assert!(s.run("compartment.add", &json!({"id": "k"})).is_err(), "full");
    }

    #[test]
    fn a_slot_owns_its_members_and_frees_them_when_removed() {
        let mut s = session();
        s.run("compartment.add", &json!({"id": "k", "kind": "slot", "compartmentId": "pins", "members": ["m"]})).unwrap();
        let (k, m) = (shape(&s, "k").clone(), shape(&s, "m").clone());
        assert_eq!(m.parent.as_deref(), Some("k"));
        let area = cp::layout(&k)[1].slot_area();
        assert!(m.x >= k.x + area.x0 - 1e-9 && m.y >= k.y + area.y0 - 1e-9 && m.x + m.w <= k.x + area.x1 + 1e-9, "clamped: {m:?}");
        s.run("shape.move", &json!({"ids": ["k"], "dx": 50, "dy": 25})).unwrap();
        let m2 = shape(&s, "m");
        assert_eq!((m2.x, m2.y), (m.x + 50.0, m.y + 25.0), "moves with its parent");
        // Dragged out on its own, it is held in the slot.
        s.run("shape.move", &json!({"ids": ["m"], "dx": 800, "dy": 0})).unwrap();
        let (k, m) = (shape(&s, "k").clone(), shape(&s, "m").clone());
        assert!(m.x + m.w <= k.x + k.w, "{m:?}");
        // Members replaced: the old one moves up to the shape's parent (none).
        s.run("compartment.set", &json!({"id": "k", "compartment": "pins", "members": []})).unwrap();
        assert_eq!(shape(&s, "m").parent, None);
        s.run("compartment.set", &json!({"id": "k", "compartment": "pins", "members": ["m"]})).unwrap();
        let r = s.run("compartment.remove", &json!({"id": "k", "compartment": "pins"})).unwrap();
        assert_eq!(r["freed"], json!(["m"]));
        assert_eq!(shape(&s, "m").parent, None);
    }

    #[test]
    fn connectors_glue_to_ports_and_lose_them_with_the_compartment() {
        let mut s = session();
        s.run("compartment.add", &json!({"id": "k", "kind": "list", "compartmentId": "attrs", "rows": ["a", "b"]})).unwrap();
        let r = s.run("connector.add", &json!({"from": {"shape": "k", "port": "attrs.right"}, "to": {"shape": "o"}})).unwrap();
        let cid = r["id"].as_str().unwrap().to_string();
        let start = |s: &Session| s.doc.page().unwrap().connector(&cid).unwrap().start.clone();
        assert_eq!(start(&s), End::port("k", "attrs.right"));
        assert!(s.run("connector.add", &json!({"from": {"shape": "k", "port": "nope.left"}, "to": {"shape": "o"}})).is_err());
        // Gluing to a compartment's point index stores the port.
        let base = ShapeKind::Rectangle.connection_points().len();
        let r = s.run("connector.add", &json!({"from": {"shape": "k", "point": base}, "to": {"shape": "o"}})).unwrap();
        let other = s.doc.page().unwrap().connector(r["id"].as_str().unwrap()).unwrap().start.clone();
        assert_eq!(other, End::port("k", "attrs.left"));
        // The end follows the band.
        let page = s.doc.page().unwrap();
        let pos = vizcraft_diagram::resolve_ends(page, page.connector(&cid).unwrap()).start.pos;
        let k = shape(&s, "k");
        assert_eq!(pos, vizcraft_diagram::Point::new(k.x + k.w, k.y + cp::layout(k)[1].rect.center().y));
        s.run("compartment.remove", &json!({"id": "k", "compartment": "attrs"})).unwrap();
        assert_eq!(start(&s), End::dynamic("k"));
    }

    #[test]
    fn hostile_params_never_panic_on_compartment_commands() {
        let many = vec!["\u{0}"; 300];
        let junk = [
            json!(null),
            json!({"id": "k", "compartment": "a", "index": 1e300, "rows": many, "text": "x".repeat(70_000)}),
            json!({"id": "k", "kind": "slot", "members": ["k", "k"], "index": -1e300, "title": "t".repeat(5000)}),
            json!({"id": "k", "compartment": "a", "height": -1e308, "fontSize": 1e308, "fill": "#zzz", "fontColor": 3, "bold": "yes"}),
            json!({"id": "k", "compartment": "a", "height": f64::MAX, "members": ["m", 1]}),
            json!({"id": "k", "compartmentId": "a", "kind": "list", "rows": ["one\ntwo"]}),
            json!({"id": "k", "compartmentId": "x".repeat(100)}),
            json!({"id": 5, "compartment": [], "index": "NaN"}),
        ];
        let mut s = session();
        for _ in 0..2 {
            for cmd in ["compartment.add", "compartment.set", "compartment.move", "compartment.remove"] {
                for v in &junk {
                    let _ = s.run(cmd, v);
                }
            }
        }
        let k = shape(&s, "k");
        assert!(k.compartments.len() <= cp::MAX_COMPARTMENTS && k.h.is_finite() && k.h <= MAX_EXTENT);
    }
}
