//! Protection and behaviour: shape locks, behaviour flags, shape names and document protection
//! (Developer > Shape Design), and the one check every editing command asks.
//!
//! A shape's [`Locks`](vizcraft_diagram::Locks) refuse single kinds of edit; document protection
//! ([`Protection`](vizcraft_diagram::Protection)) with `shapes` set refuses every edit a lock
//! can refuse on every shape, and with `backgrounds` set refuses edits to the shapes of
//! background pages and changes to which background a page uses. [`check`] is the single place
//! those rules are read; `shape.move`, `shape.resize`, `shape.rotate`, `shape.delete`,
//! `delete_items`, `connector.glue` and the shape operations call it.

use serde_json::{Value, json};
use vizcraft_diagram::{Behavior, Locks, Page, Shape};

use super::{Kinds, has_shapes, opt_str, page_mut, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The protection commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("shape.setLocks", "Protection", "Developer > Shape Design", set_locks)
            .params(r#"{"ids"?: [string], "width"?: bool|"toggle", "height"?: bool|"toggle", "aspect"?: bool|"toggle", "position"?: bool|"toggle", "rotation"?: bool|"toggle", "delete"?: bool|"toggle", "select"?: bool|"toggle", "text"?: bool|"toggle"} -> {"ids", "locks"}; "toggle" flips the first shape's value for all; select stops canvas clicks only (selecting by id still works)"#)
            .when(has_shapes),
        CommandSpec::new("shape.setBehavior", "Behavior", "Developer > Shape Design", set_behavior)
            .params(r#"{"ids"?: [string], "container"?: bool|"toggle", "nonPrinting"?: bool|"toggle", "hidden"?: bool|"toggle", "noGlue"?: bool|"toggle", "noSnap"?: bool|"toggle"} -> {"ids", "behavior"}"#)
            .when(has_shapes),
        CommandSpec::new("shape.rename", "Shape Name", "Developer > Shape Design", rename)
            .params(r#"{"id"?: string (default: the primary selection), "name"?: string} -> {"id", "name"}; an empty name clears it; with no name the Shape Name dialog opens"#)
            .when(has_shapes),
        CommandSpec::new("doc.protect", "Protect Document", "Developer > Shape Design", protect).params(
            r#"{"shapes"?: bool|"toggle", "backgrounds"?: bool|"toggle"} -> {"shapes", "backgrounds"}; shapes refuses moving, sizing, turning, deleting and text edits of every shape; backgrounds refuses edits on background pages and changing a page's background"#,
        ),
    ]
}

/// What an edit would change, for [`check`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lock {
    /// Moving the shape.
    Position,
    /// Giving the shape this width and height.
    Size {
        /// The new width.
        w: f64,
        /// The new height.
        h: f64,
    },
    /// Turning the shape.
    Rotation,
    /// Deleting the shape.
    Delete,
    /// Editing the shape's text.
    Text,
    /// Gluing a connector to the shape.
    Glue,
}

impl Lock {
    fn verb(self) -> &'static str {
        match self {
            Lock::Position => "moving",
            Lock::Size { .. } => "resizing",
            Lock::Rotation => "rotation",
            Lock::Delete => "deletion",
            Lock::Text => "text editing",
            Lock::Glue => "glue",
        }
    }
}

/// Whether document protection covers every edit of the shapes on `page`.
fn protected(s: &Session, page: &Page) -> bool {
    let p = s.doc.protection;
    p.shapes || (p.backgrounds && page.is_background)
}

/// Whether `lock` is refused for `sh` (`all`: document protection covers it).
fn refuses(sh: &Shape, lock: Lock, all: bool) -> bool {
    let l = &sh.locks;
    match lock {
        Lock::Position => l.position || all,
        Lock::Size { w, h } => {
            let dw = (w - sh.w).abs() > 1e-9;
            let dh = (h - sh.h).abs() > 1e-9;
            let ratio = |a: f64, b: f64| if b.abs() > 1e-9 { a / b } else { 0.0 };
            let aspect = l.aspect && (dw || dh) && (ratio(w, h) - ratio(sh.w, sh.h)).abs() > 1e-3 * ratio(sh.w, sh.h).abs().max(1e-9);
            (dw && (l.width || all)) || (dh && (l.height || all)) || aspect
        }
        Lock::Rotation => l.rotation || all,
        Lock::Delete => l.delete || all,
        Lock::Text => l.text || all,
        Lock::Glue => sh.behavior.no_glue,
    }
}

/// Refuse when any of `ids` names a shape on the active page whose locks, or the document's
/// protection, refuse `lock`. Ids that name no shape (connectors, unknown ids) pass; the command
/// reports those itself.
pub fn check<I, S>(s: &Session, ids: I, lock: Lock) -> Result<(), EngineError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(page) = s.doc.page() else { return Ok(()) };
    let all = protected(s, page);
    for id in ids {
        if let Some(sh) = page.shape(id.as_ref())
            && refuses(sh, lock, all)
        {
            let why = if lock == Lock::Glue {
                "does not take glue".to_string()
            } else if all && !refuses(sh, lock, false) {
                format!("is protected against {} (Protect Document)", lock.verb())
            } else {
                format!("is locked against {}", lock.verb())
            };
            return Err(EngineError::Params(format!("`{}` {why}", short(&sh.id))));
        }
    }
    Ok(())
}

/// Refuse a change to which background a page uses while backgrounds are protected.
pub fn backgrounds(s: &Session) -> Result<(), EngineError> {
    if s.doc.protection.backgrounds { Err(EngineError::Params("background pages are protected (Protect Document)".into())) } else { Ok(()) }
}

/// A flag param: absent or null is `None`, `"toggle"` flips `current`, a boolean sets it.
fn flag(v: &Value, k: &str, current: bool) -> Result<Option<bool>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(Value::String(t)) if t == "toggle" => Ok(Some(!current)),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be true, false or \"toggle\""))),
    }
}

/// The flags given among `keys`, each resolved against `first`'s value. At least one is needed.
fn flags<T>(v: &Value, keys: &[(&'static str, fn(&T) -> bool)], first: &T) -> Result<Vec<(&'static str, bool)>, EngineError> {
    let mut out = Vec::new();
    for (k, get) in keys {
        if let Some(b) = flag(v, k, get(first))? {
            out.push((*k, b));
        }
    }
    if out.is_empty() {
        let names: Vec<&str> = keys.iter().map(|(k, _)| *k).collect();
        return Err(EngineError::Params(format!("give at least one of {}", names.join(", "))));
    }
    Ok(out)
}

const LOCK_KEYS: [(&str, fn(&Locks) -> bool); 8] = [
    ("width", |l| l.width),
    ("height", |l| l.height),
    ("aspect", |l| l.aspect),
    ("position", |l| l.position),
    ("rotation", |l| l.rotation),
    ("delete", |l| l.delete),
    ("select", |l| l.select),
    ("text", |l| l.text),
];

fn set_lock(l: &mut Locks, k: &str, b: bool) {
    match k {
        "width" => l.width = b,
        "height" => l.height = b,
        "aspect" => l.aspect = b,
        "position" => l.position = b,
        "rotation" => l.rotation = b,
        "delete" => l.delete = b,
        "select" => l.select = b,
        _ => l.text = b,
    }
}

const BEHAVIOR_KEYS: [(&str, fn(&Behavior) -> bool); 5] = [
    ("container", |b| b.container),
    ("nonPrinting", |b| b.non_printing),
    ("hidden", |b| b.hidden),
    ("noGlue", |b| b.no_glue),
    ("noSnap", |b| b.no_snap),
];

fn set_flag(b: &mut Behavior, k: &str, on: bool) {
    match k {
        "container" => b.container = on,
        "nonPrinting" => b.non_printing = on,
        "hidden" => b.hidden = on,
        "noGlue" => b.no_glue = on,
        _ => b.no_snap = on,
    }
}

fn first_shape<'a>(s: &'a Session, ids: &[String]) -> Result<&'a Shape, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    ids.first().and_then(|id| page.shape(id)).ok_or_else(|| EngineError::Params("select a shape first (or pass `ids`)".into()))
}

fn set_locks(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Shapes)?;
    let first = first_shape(s, &ids)?.locks;
    let changes = flags(v, &LOCK_KEYS, &first)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            for (k, b) in &changes {
                set_lock(&mut sh.locks, k, *b);
            }
        }
    }
    let locks = page.shape(ids.first().map_or("", String::as_str)).map(|sh| sh.locks).unwrap_or_default();
    with_sel(s, json!({"ids": ids, "locks": locks}))
}

fn set_behavior(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Shapes)?;
    let first = first_shape(s, &ids)?.behavior;
    let changes = flags(v, &BEHAVIOR_KEYS, &first)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            for (k, b) in &changes {
                set_flag(&mut sh.behavior, k, *b);
            }
        }
    }
    let behavior = page.shape(ids.first().map_or("", String::as_str)).map(|sh| sh.behavior).unwrap_or_default();
    // A hidden shape cannot stay selected.
    super::prune_selection(s);
    let hidden: Vec<String> =
        s.doc.page().map(|p| ids.iter().filter(|id| p.shape(id).is_some_and(|sh| sh.behavior.hidden)).cloned().collect()).unwrap_or_default();
    s.sel.items.retain(|id| !hidden.contains(id));
    if s.sel.primary.as_ref().is_some_and(|p| hidden.contains(p)) {
        s.sel.primary = s.sel.items.first().cloned();
    }
    with_sel(s, json!({"ids": ids, "behavior": behavior}))
}

fn rename(s: &mut Session, v: &Value) -> CmdResult {
    let id = match opt_str(v, "id")? {
        Some(id) => id.to_string(),
        None => s
            .sel
            .primary
            .clone()
            .filter(|p| s.doc.page().is_some_and(|pg| pg.shape(p).is_some()))
            .ok_or_else(|| EngineError::Params("select a shape first (or pass `id`)".into()))?,
    };
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if page.shape(&id).is_none() {
        return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(&id))));
    }
    let Some(name) = opt_str(v, "name")? else {
        // No name: the UI asks for one (programmatic callers pass it).
        s.ui_requests.push(json!({"open": "shapeName", "id": id}));
        let name = page.shape(&id).map(|sh| sh.name.clone()).unwrap_or_default();
        return Ok(json!({"id": id, "name": name}));
    };
    super::layer::ensure_unlocked(s, [&id])?;
    let name = name.trim().to_string();
    let sh = page_mut(s)?.shape_mut(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(&id))))?;
    sh.name = name;
    sh.sanitize();
    let name = sh.name.clone();
    with_sel(s, json!({"id": id, "name": name}))
}

fn protect(s: &mut Session, v: &Value) -> CmdResult {
    let cur = s.doc.protection;
    let shapes = flag(v, "shapes", cur.shapes)?;
    let bgs = flag(v, "backgrounds", cur.backgrounds)?;
    if shapes.is_none() && bgs.is_none() {
        return Err(EngineError::Params("give `shapes`, `backgrounds` or both".into()));
    }
    if let Some(b) = shapes {
        s.doc.protection.shapes = b;
    }
    if let Some(b) = bgs {
        s.doc.protection.backgrounds = b;
    }
    let p = s.doc.protection;
    Ok(json!({"shapes": p.shapes, "backgrounds": p.backgrounds}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Document, Shape, ShapeKind};

    use crate::Session;

    fn two() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 50.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 200.0, 0.0, 100.0, 50.0));
        Session::new(doc)
    }

    #[test]
    fn a_position_locked_shape_refuses_shape_move() {
        let mut s = two();
        s.run("shape.setLocks", &json!({"ids": ["a"], "position": true})).unwrap();
        assert!(s.run("shape.move", &json!({"ids": ["a"], "dx": 10, "dy": 0})).is_err());
        assert!(s.run("shape.move", &json!({"id": "a", "x": 10, "y": 0})).is_err());
        assert_eq!(s.doc.page().unwrap().shapes[0].x, 0.0);
        s.run("shape.move", &json!({"ids": ["b"], "dx": 10, "dy": 0})).unwrap();
        // Toggle flips the first shape's value.
        let r = s.run("shape.setLocks", &json!({"ids": ["a"], "position": "toggle"})).unwrap();
        assert_eq!(r["locks"]["position"], false);
        s.run("shape.move", &json!({"ids": ["a"], "dx": 10, "dy": 0})).unwrap();
    }

    #[test]
    fn size_rotation_and_delete_locks() {
        let mut s = two();
        s.run("shape.setLocks", &json!({"ids": ["a"], "width": true, "rotation": true, "delete": true})).unwrap();
        assert!(s.run("shape.resize", &json!({"id": "a", "w": 120, "h": 50})).is_err());
        s.run("shape.resize", &json!({"id": "a", "w": 100, "h": 80})).unwrap();
        assert!(s.run("shape.rotate", &json!({"ids": ["a"], "angle": 30})).is_err());
        assert!(s.run("shape.delete", &json!({"ids": ["a"]})).is_err());
        assert!(s.run("edit.delete", &json!({})).is_err() || s.doc.page().unwrap().shape("a").is_some());
        s.run("shape.setLocks", &json!({"ids": ["b"], "aspect": true})).unwrap();
        assert!(s.run("shape.resize", &json!({"id": "b", "w": 150, "h": 50})).is_err());
        s.run("shape.resize", &json!({"id": "b", "w": 200, "h": 100})).unwrap();
    }

    #[test]
    fn protect_document_shapes_refuses_shape_delete() {
        let mut s = two();
        s.run("doc.protect", &json!({"shapes": true})).unwrap();
        assert!(s.run("shape.delete", &json!({"ids": ["b"]})).is_err());
        s.run("edit.selectAll", &json!({})).unwrap();
        assert!(s.run("edit.delete", &json!({})).is_err());
        assert_eq!(s.doc.page().unwrap().shapes.len(), 2);
        s.run("doc.protect", &json!({"shapes": "toggle"})).unwrap();
        s.run("shape.delete", &json!({"ids": ["b"]})).unwrap();
        assert!(s.run("doc.protect", &json!({})).is_err());
    }

    #[test]
    fn protect_backgrounds_refuses_changing_a_background() {
        let mut s = two();
        s.run("page.add", &json!({"background": true, "name": "Back"})).unwrap();
        let fg = s.doc.pages[0].id.clone();
        s.run("doc.protect", &json!({"backgrounds": true})).unwrap();
        assert!(s.run("page.setBackground", &json!({"page": fg, "background": "Back"})).is_err());
        s.run("doc.protect", &json!({"backgrounds": false})).unwrap();
        s.run("page.setBackground", &json!({"page": fg, "background": "Back"})).unwrap();
    }

    #[test]
    fn no_glue_refuses_connector_glue() {
        let mut s = two();
        let c = s.run("connector.add", &json!({"from": {"x": 0, "y": 200}, "to": {"x": 50, "y": 200}})).unwrap()["id"].as_str().unwrap().to_string();
        s.run("shape.setBehavior", &json!({"ids": ["a"], "noGlue": true})).unwrap();
        assert!(s.run("connector.glue", &json!({"id": c, "end": "end", "shape": "a"})).is_err());
        s.run("connector.glue", &json!({"id": c, "end": "end", "shape": "b"})).unwrap();
    }

    #[test]
    fn a_text_lock_and_document_protection_refuse_every_text_edit() {
        let mut s = two();
        s.run("shape.setLocks", &json!({"ids": ["a"], "text": true})).unwrap();
        let before = s.doc.page().unwrap().shapes[0].text.clone();
        let e = s.run("shape.setText", &json!({"id": "a", "text": "changed"})).unwrap_err().to_string();
        assert!(e.contains("locked against text editing"), "{e}");
        for (cmd, params) in [
            ("text.setCase", json!({"ids": ["a"], "case": "upper"})),
            ("text.insertSymbol", json!({"ids": ["a"], "symbol": "x"})),
            ("text.insertField", json!({"ids": ["a"], "field": "date"})),
        ] {
            assert!(s.run(cmd, &params).is_err(), "{cmd} edits locked text");
        }
        assert_eq!(s.doc.page().unwrap().shapes[0].text, before);
        // The other shape is free, and a connector label takes the same path.
        s.run("shape.setText", &json!({"id": "b", "text": "free"})).unwrap();
        // Unlocked, the edit goes through; then document protection refuses it again.
        s.run("shape.setLocks", &json!({"ids": ["a"], "text": false})).unwrap();
        s.run("shape.setText", &json!({"id": "a", "text": "changed"})).unwrap();
        s.run("doc.protect", &json!({"shapes": true})).unwrap();
        let e = s.run("shape.setText", &json!({"id": "a", "text": "again"})).unwrap_err().to_string();
        assert!(e.contains("protected against text editing"), "{e}");
    }

    #[test]
    fn rename_sets_the_name_or_asks_for_one() {
        let mut s = two();
        let r = s.run("shape.rename", &json!({"id": "a", "name": "  Pump P-101 "})).unwrap();
        assert_eq!(r["name"], "Pump P-101");
        assert_eq!(s.doc.page().unwrap().shapes[0].name, "Pump P-101");
        let d = s.run("shape.describe", &json!({"id": "a", "detail": "full"})).unwrap();
        assert!(d.to_string().contains("Pump P-101"), "{d}");
        s.run("edit.select", &json!({"ids": ["b"]})).unwrap();
        s.run("shape.rename", &json!({})).unwrap();
        assert_eq!(s.ui_requests.last().unwrap()["open"], "shapeName");
        assert!(s.run("shape.rename", &json!({"id": "zz", "name": "x"})).is_err());
    }
}
