//! Arrange commands: align, distribute, auto space, z order, flip, rotate, size and position,
//! group and ungroup. The geometry lives in `vizcraft_diagram::arrange`; these commands parse
//! params, pick the shapes and report what changed. Each is one undo step, and connectors follow
//! moved shapes on their own (avoiding ones are rerouted by `Session::run`).
//!
//! A command acts on `ids`, else on the selected shapes. A shape inside a group that is also
//! named moves once, with its group.

use serde_json::{Value, json};
use vizcraft_diagram::arrange::{self as ar, Axis, Edge, Order};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{MAX_EXTENT, Page};

use super::{Kinds, MAX_DELTA, MIN_SIZE, has_shapes, num, opt_str, page_mut, prune_selection, req_num, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all arrange commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("arrange.align", "Align", "Home > Arrange > Align", align)
            .params(r#"{"ids"?: [string], "edge": "left"|"center"|"right"|"top"|"middle"|"bottom", "to"?: "selection"|"page"} -> {"moved"}; to the first shape (default) or the page"#)
            .when(has_shapes),
        CommandSpec::new("arrange.distribute", "Distribute", "Home > Arrange > Position", distribute)
            .params(r#"{"ids"?: [string], "axis": "horizontal"|"vertical"} -> {"moved"}; three or more shapes, equal gaps, the outer two stay"#)
            .when(has_shapes),
        CommandSpec::new("arrange.autoSpace", "Auto Space", "Home > Arrange > Position", auto_space)
            .params(r#"{"ids"?: [string], "gap"?: n (default 40)} -> {"moved"}; even gaps between connected shapes; with nothing selected, the whole page"#)
            .when(has_shapes),
        CommandSpec::new("arrange.order", "Order", "Home > Arrange", order)
            .params(r#"{"ids"?: [string], "how": "front"|"back"|"forward"|"backward"} -> {"order": [ids in z order]}"#)
            .when(has_shapes),
        CommandSpec::new("arrange.flip", "Flip", "Home > Arrange > Rotate", flip)
            .params(r#"{"ids"?: [string], "axis": "horizontal"|"vertical"} -> {"flipped"}; mirrors positions and rotation across the shapes' centre line"#)
            .when(has_shapes),
        CommandSpec::new("arrange.rotate", "Rotate By", "Home > Arrange > Rotate", rotate)
            .params(r#"{"ids"?: [string], "deg": n} -> {"rotated"}; relative, clockwise; several shapes turn as one piece about their centre"#)
            .when(has_shapes),
        CommandSpec::new("arrange.setBounds", "Size and Position", "Home > Arrange", set_bounds)
            .params(r#"{"id": string, "x"?: n, "y"?: n, "w"?: n, "h"?: n, "angle"?: degrees} -> {"id", "x", "y", "w", "h", "angle"}; x, y is the top-left; a group scales and turns its members"#)
            .when(has_shapes),
        CommandSpec::new("arrange.group", "Group", "Home > Arrange > Group", group)
            .params(r#"{"ids"?: [string], "id"?: string} -> {"id", "members"}; the group is a shape with stencil "vizcraft.group" and is selected"#)
            .when(has_shapes),
        CommandSpec::new("arrange.ungroup", "Ungroup", "Home > Arrange > Group", ungroup)
            .params(r#"{"ids"?: [string]} -> {"ungrouped", "members"}; members are selected"#)
            .when(has_groups),
    ]
}

/// Enabled when the active page has a group.
fn has_groups(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.shapes.iter().any(ar::is_group)) { None } else { Some("group some shapes first") }
}

/// A required string param that must be one of the names `parse` knows.
fn choice<T>(v: &Value, k: &str, parse: fn(&str) -> Option<T>, names: &str) -> Result<T, EngineError> {
    let t = p::req_str(v, k)?;
    parse(t).ok_or_else(|| EngineError::Params(format!("`{k}` must be {names}, not `{}`", short(t))))
}

fn page(s: &Session) -> Result<&Page, EngineError> {
    s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))
}

/// The target shapes reduced to the outermost ones (a member named with its group moves once).
fn shapes(s: &Session, v: &Value) -> Result<Vec<String>, EngineError> {
    let ids = targets(s, v, Kinds::Shapes)?;
    Ok(ar::top_level(page(s)?, &ids))
}

fn align(s: &mut Session, v: &Value) -> CmdResult {
    let edge = choice(v, "edge", Edge::parse, "left, center, right, top, middle or bottom")?;
    let to_page = match opt_str(v, "to")? {
        None | Some("selection") => false,
        Some("page") => true,
        Some(t) => return Err(EngineError::Params(format!("`to` must be selection or page, not `{}`", short(t)))),
    };
    let ids = shapes(s, v)?;
    let pg = page(s)?;
    let anchor = if to_page {
        pg.rect()
    } else {
        if ids.len() < 2 {
            return Err(EngineError::Params("select two or more shapes to align (or pass `to: \"page\"`)".into()));
        }
        ids.first().and_then(|id| pg.shape(id)).map(|sh| sh.bounds()).ok_or_else(|| EngineError::Failed("no shape to align to".into()))?
    };
    let moved = ar::align(page_mut(s)?, &ids, edge, anchor);
    with_sel(s, json!({"moved": moved}))
}

fn distribute(s: &mut Session, v: &Value) -> CmdResult {
    let axis = choice(v, "axis", Axis::parse, "horizontal or vertical")?;
    let ids = shapes(s, v)?;
    if ids.len() < 3 {
        return Err(EngineError::Params("select three or more shapes to distribute".into()));
    }
    let moved = ar::distribute(page_mut(s)?, &ids, axis);
    with_sel(s, json!({"moved": moved}))
}

fn auto_space(s: &mut Session, v: &Value) -> CmdResult {
    let gap = num(v, "gap")?.map_or(ar::DEFAULT_GAP, |g| g.clamp(0.0, MAX_EXTENT));
    let explicit = super::ids_param(v)?.is_some();
    let selected = s.sel.items.iter().any(|id| s.doc.page().is_some_and(|p| p.shape(id).is_some()));
    // Like Visio, auto space with nothing selected spaces the whole page.
    let ids = if explicit || selected {
        shapes(s, v)?
    } else {
        let pg = page(s)?;
        let all: Vec<String> = pg.shapes.iter().map(|sh| sh.id.clone()).collect();
        ar::top_level(pg, &all)
    };
    if ids.len() < 2 {
        return Err(EngineError::Params("select two or more connected shapes to space".into()));
    }
    let moved = ar::auto_space(page_mut(s)?, &ids, gap);
    with_sel(s, json!({"moved": moved}))
}

fn order(s: &mut Session, v: &Value) -> CmdResult {
    let how = choice(v, "how", Order::parse, "front, back, forward or backward")?;
    let ids = shapes(s, v)?;
    let pg = page_mut(s)?;
    ar::order(pg, &ids, how);
    let z: Vec<String> = pg.shapes.iter().map(|sh| sh.id.clone()).collect();
    with_sel(s, json!({"order": z}))
}

fn flip(s: &mut Session, v: &Value) -> CmdResult {
    let axis = choice(v, "axis", Axis::parse, "horizontal or vertical")?;
    let ids = shapes(s, v)?;
    let flipped = ar::flip(page_mut(s)?, &ids, axis);
    with_sel(s, json!({"flipped": flipped}))
}

fn rotate(s: &mut Session, v: &Value) -> CmdResult {
    let deg = req_num(v, "deg")?.rem_euclid(360.0);
    let ids = shapes(s, v)?;
    ar::rotate_by(page_mut(s)?, &ids, deg);
    with_sel(s, json!({"rotated": ids}))
}

fn set_bounds(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?.to_string();
    let coord = |k: &str| num(v, k).map(|o| o.map(|x| x.clamp(-MAX_COORD, MAX_COORD)));
    let size = |k: &str| num(v, k).map(|o| o.map(|x| x.clamp(MIN_SIZE, MAX_EXTENT)));
    let (x, y, w, h) = (coord("x")?, coord("y")?, size("w")?, size("h")?);
    let angle = num(v, "angle")?;
    if [x, y, w, h, angle].iter().all(Option::is_none) {
        return Err(EngineError::Params("give at least one of x, y, w, h, angle".into()));
    }
    let pg = page_mut(s)?;
    let sh = pg.shape(&id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(&id))))?;
    let (nx, ny, nw, nh) = (x.unwrap_or(sh.x), y.unwrap_or(sh.y), w.unwrap_or(sh.w), h.unwrap_or(sh.h));
    if x.is_some() || y.is_some() || w.is_some() || h.is_some() {
        ar::set_frame(pg, &id, nx, ny, nw, nh);
    }
    if let Some(a) = angle {
        ar::set_angle(pg, &id, a.rem_euclid(360.0));
    }
    let sh = pg.shape(&id).ok_or_else(|| EngineError::Failed("the shape went away".into()))?;
    let out = json!({"id": id, "x": sh.x, "y": sh.y, "w": sh.w, "h": sh.h, "angle": sh.angle_deg});
    with_sel(s, out)
}

fn group(s: &mut Session, v: &Value) -> CmdResult {
    let ids = shapes(s, v)?;
    let gid = super::new_id(s, v, "g")?;
    let members = ar::group(page_mut(s)?, &ids, &gid).map_err(|e| EngineError::Params(e.to_string()))?;
    select(s, vec![gid.clone()]);
    with_sel(s, json!({"id": gid, "members": members}))
}

fn ungroup(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Shapes)?;
    let pg = page(s)?;
    let groups: Vec<String> = ids.iter().filter(|id| pg.shape(id).is_some_and(ar::is_group)).cloned().collect();
    if groups.is_empty() {
        return Err(EngineError::Params("select a group to ungroup".into()));
    }
    let members = ar::ungroup(page_mut(s)?, &groups);
    select(s, members.clone());
    prune_selection(s);
    with_sel(s, json!({"ungrouped": groups, "members": members}))
}

/// Shapes a move, resize or rotate must also carry: used by `shape.move` so moving a group moves
/// its members. Clamps the delta like `shape.move` does.
pub fn move_shapes(page: &mut Page, ids: &[String], dx: f64, dy: f64) -> Vec<String> {
    ar::translate(page, ids, dx.clamp(-MAX_DELTA, MAX_DELTA), dy.clamp(-MAX_DELTA, MAX_DELTA))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Session;

    /// A blank document with shapes `a` (0,0 40x20), `b` (200,100 40x40), `c` (500,0 20x60).
    fn session() -> Session {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        for (id, x, y, w, h) in [("a", 0, 0, 40, 20), ("b", 200, 100, 40, 40), ("c", 500, 0, 20, 60)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": y, "w": w, "h": h})).unwrap();
        }
        s.run("edit.selectNone", &json!({})).unwrap();
        s
    }

    fn frame(s: &Session, id: &str) -> (f64, f64, f64, f64) {
        let sh = s.doc.page().unwrap().shape(id).unwrap_or_else(|| panic!("no {id}"));
        (sh.x, sh.y, sh.w, sh.h)
    }

    fn z(s: &Session) -> String {
        s.doc.page().unwrap().shapes.iter().map(|x| x.id.clone()).collect::<Vec<_>>().join(",")
    }

    /// Run `id` with `params`, check it made one undo step, undo it and check the document is back.
    fn one_step(s: &mut Session, id: &str, params: Value) -> Value {
        let before = s.doc.clone();
        let depth = s.transactions().len();
        let r = s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_ne!(s.doc, before, "{id} changed nothing");
        assert_eq!(s.transactions().len(), depth + 1, "{id} is one undo step");
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc == before, "{id} undone");
        s.run("edit.redo", &json!({})).unwrap();
        r
    }

    #[test]
    fn align_to_first_shape_and_to_page() {
        let mut s = session();
        one_step(&mut s, "arrange.align", json!({"ids": ["b", "a", "c"], "edge": "left"}));
        assert_eq!(frame(&s, "a").0, 200.0);
        assert_eq!(frame(&s, "c").0, 200.0);
        one_step(&mut s, "arrange.align", json!({"ids": ["a", "b"], "edge": "bottom"}));
        assert_eq!(frame(&s, "b").1, -20.0);
        s.run("edit.select", &json!({"ids": ["c"]})).unwrap();
        one_step(&mut s, "arrange.align", json!({"edge": "middle", "to": "page"}));
        let page_h = s.doc.page().unwrap().height;
        assert_eq!(frame(&s, "c").1, page_h / 2.0 - 30.0);
        assert!(s.run("arrange.align", &json!({"ids": ["a"], "edge": "left"})).is_err());
        assert!(s.run("arrange.align", &json!({"ids": ["a", "b"], "edge": "diagonal"})).is_err());
        assert!(s.run("arrange.align", &json!({"ids": ["a", "b"], "edge": "left", "to": "moon"})).is_err());
    }

    #[test]
    fn distribute_and_auto_space() {
        let mut s = session();
        one_step(&mut s, "arrange.distribute", json!({"ids": ["a", "b", "c"], "axis": "horizontal"}));
        // Span 0..520, widths 100: gaps of 210.
        assert_eq!(frame(&s, "b").0, 250.0);
        one_step(&mut s, "arrange.distribute", json!({"ids": ["a", "b", "c"], "axis": "vertical"}));
        assert!(s.run("arrange.distribute", &json!({"ids": ["a", "b"], "axis": "vertical"})).is_err());

        let mut s = session();
        s.run("connector.add", &json!({"from": {"shape": "a"}, "to": {"shape": "b"}})).unwrap();
        s.run("connector.add", &json!({"from": {"shape": "b"}, "to": {"shape": "c"}})).unwrap();
        s.run("edit.selectNone", &json!({})).unwrap();
        // Nothing selected: the whole page.
        one_step(&mut s, "arrange.autoSpace", json!({"gap": 30}));
        assert_eq!(frame(&s, "b").0, 70.0);
        assert_eq!(frame(&s, "c").0, 140.0);
    }

    #[test]
    fn order_changes_z_order() {
        let mut s = session();
        let r = one_step(&mut s, "arrange.order", json!({"ids": ["a"], "how": "front"}));
        assert_eq!(r["order"], json!(["b", "c", "a"]));
        one_step(&mut s, "arrange.order", json!({"ids": ["a"], "how": "backward"}));
        assert_eq!(z(&s), "b,a,c");
        one_step(&mut s, "arrange.order", json!({"ids": ["b"], "how": "forward"}));
        assert_eq!(z(&s), "a,b,c");
        one_step(&mut s, "arrange.order", json!({"ids": ["c"], "how": "back"}));
        assert_eq!(z(&s), "c,a,b");
        assert!(s.run("arrange.order", &json!({"ids": ["c"], "how": "sideways"})).is_err());
    }

    #[test]
    fn flip_rotate_and_set_bounds() {
        let mut s = session();
        one_step(&mut s, "arrange.flip", json!({"ids": ["a", "c"], "axis": "horizontal"}));
        assert_eq!(frame(&s, "a").0, 480.0);
        assert_eq!(frame(&s, "c").0, 0.0);
        one_step(&mut s, "arrange.flip", json!({"ids": ["a", "c"], "axis": "vertical"}));
        assert_eq!(frame(&s, "a").1, 40.0);
        one_step(&mut s, "arrange.rotate", json!({"ids": ["b"], "deg": 90}));
        assert_eq!(s.doc.page().unwrap().shape("b").unwrap().angle_deg, 90.0);
        one_step(&mut s, "arrange.rotate", json!({"ids": ["b"], "deg": -90}));
        assert_eq!(s.doc.page().unwrap().shape("b").unwrap().angle_deg, 0.0);
        let r = one_step(&mut s, "arrange.setBounds", json!({"id": "b", "x": 10, "w": 80, "angle": 45}));
        let got: Vec<f64> = ["x", "y", "w", "angle"].iter().map(|k| r[*k].as_f64().unwrap().round()).collect();
        assert_eq!(got, [10.0, 100.0, 80.0, 45.0]);
        assert!(s.run("arrange.setBounds", &json!({"id": "b"})).is_err());
        assert!(s.run("arrange.setBounds", &json!({"id": "zz", "x": 1})).is_err());
    }

    #[test]
    fn group_moves_resizes_rotates_and_deletes_members() {
        let mut s = session();
        let r = one_step(&mut s, "arrange.group", json!({"ids": ["a", "b"], "id": "g"}));
        assert_eq!(r["id"], "g");
        assert_eq!(s.sel.items, vec!["g".to_string()]);
        assert_eq!(z(&s), "g,a,b,c");
        assert_eq!(frame(&s, "g"), (0.0, 0.0, 240.0, 140.0));
        let g = s.doc.page().unwrap().shape("g").unwrap();
        assert!(vizcraft_diagram::arrange::is_group(g) && g.parent.is_none());
        // Moving the group moves its members; moving a member refits the group.
        one_step(&mut s, "shape.move", json!({"ids": ["g"], "dx": 10, "dy": 5}));
        assert_eq!(frame(&s, "a"), (10.0, 5.0, 40.0, 20.0));
        one_step(&mut s, "shape.move", json!({"id": "g", "x": 0, "y": 0}));
        assert_eq!(frame(&s, "b"), (200.0, 100.0, 40.0, 40.0));
        one_step(&mut s, "shape.move", json!({"ids": ["b"], "dx": 100, "dy": 0}));
        assert_eq!(frame(&s, "g"), (0.0, 0.0, 340.0, 140.0));
        one_step(&mut s, "shape.resize", json!({"id": "g", "w": 170, "h": 140}));
        assert_eq!(frame(&s, "b"), (150.0, 100.0, 20.0, 40.0));
        one_step(&mut s, "shape.rotate", json!({"ids": ["g"], "angle": 90}));
        assert_eq!(s.doc.page().unwrap().shape("a").unwrap().angle_deg, 90.0);
        // Deleting the group deletes its members; undo brings all back.
        let before = s.doc.clone();
        s.run("shape.delete", &json!({"ids": ["g"]})).unwrap();
        assert_eq!(z(&s), "c");
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc == before);
        // Deleting the last member deletes the group.
        s.run("shape.delete", &json!({"ids": ["a", "b"]})).unwrap();
        assert_eq!(z(&s), "c");
    }

    #[test]
    fn nested_groups_move_and_ungroup() {
        let mut s = session();
        s.run("arrange.group", &json!({"ids": ["a", "b"], "id": "g1"})).unwrap();
        s.run("arrange.group", &json!({"ids": ["g1", "c"], "id": "g2"})).unwrap();
        assert_eq!(z(&s), "g2,g1,a,b,c");
        one_step(&mut s, "shape.move", json!({"ids": ["g2"], "dx": 0, "dy": 50}));
        assert_eq!(frame(&s, "a").1, 50.0);
        assert_eq!(frame(&s, "c").1, 50.0);
        assert_eq!(frame(&s, "g1").1, 50.0);
        let r = one_step(&mut s, "arrange.ungroup", json!({"ids": ["g2"]}));
        assert_eq!(r["members"], json!(["g1", "c"]));
        assert_eq!(s.sel.items, vec!["g1".to_string(), "c".to_string()]);
        assert!(s.doc.page().unwrap().shape("g2").is_none());
        assert_eq!(s.doc.page().unwrap().shape("a").unwrap().parent.as_deref(), Some("g1"));
        one_step(&mut s, "arrange.ungroup", json!({"ids": ["g1"]}));
        assert!(s.doc.page().unwrap().shapes.iter().all(|x| x.parent.is_none()));
        assert!(s.run("arrange.ungroup", &json!({"ids": ["a"]})).is_err());
        // A group copied and pasted keeps its members.
        s.run("arrange.group", &json!({"ids": ["a", "b"], "id": "g3"})).unwrap();
        s.run("edit.copy", &json!({})).unwrap();
        let n = s.doc.page().unwrap().shapes.len();
        s.run("edit.paste", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), n + 3);
    }

    #[test]
    fn group_saves_and_loads() {
        let mut s = session();
        s.run("arrange.group", &json!({"ids": ["a", "b"], "id": "g"})).unwrap();
        let bytes = vizcraft_formats::native::save(&s.doc).unwrap();
        let back = vizcraft_formats::native::load(&bytes).unwrap();
        assert!(back.page().unwrap().shapes == s.doc.page().unwrap().shapes);
    }

    #[test]
    fn hostile_ids_and_params_change_nothing() {
        let mut s = session();
        s.run("arrange.group", &json!({"ids": ["a", "b"], "id": "g"})).unwrap();
        let before = s.doc.clone();
        let cmds = [
            "arrange.align",
            "arrange.distribute",
            "arrange.autoSpace",
            "arrange.order",
            "arrange.flip",
            "arrange.rotate",
            "arrange.setBounds",
            "arrange.group",
            "arrange.ungroup",
        ];
        let junk = [
            json!({"ids": ["nope"], "edge": "left", "axis": "vertical", "how": "front", "deg": 5, "id": "nope", "x": 1}),
            json!({"ids": "a", "edge": 5}),
            json!({"ids": ["a", "x".repeat(100_000)], "axis": "vertical", "how": "back", "deg": "1"}),
            json!({"ids": ["a", "b"], "id": "c"}),
            json!({"ids": ["a", "c"]}),
            json!({"ids": ["g", "nope"], "id": "bad id!"}),
            json!({"ids": [], "deg": 1e308, "gap": f64::NAN}),
        ];
        for c in cmds {
            for j in &junk {
                let _ = s.run(c, j);
                assert!(s.doc == before, "{c} with {j} changed the document");
            }
        }
        // Huge but valid numbers are clamped.
        for (c, p) in [
            ("arrange.setBounds", json!({"id": "g", "x": 1e308, "y": -1e308, "w": 1e308, "h": -1e308, "angle": 1e308})),
            ("arrange.rotate", json!({"ids": ["g", "c"], "deg": 1e308})),
            ("arrange.autoSpace", json!({"ids": ["g", "c"], "gap": 1e308})),
        ] {
            s.run(c, &p).unwrap_or_else(|e| panic!("{c}: {e}"));
            for sh in &s.doc.page().unwrap().shapes {
                assert!([sh.x, sh.y, sh.w, sh.h, sh.angle_deg].iter().all(|v| v.is_finite()), "{c}: {sh:?}");
            }
        }
    }
}
