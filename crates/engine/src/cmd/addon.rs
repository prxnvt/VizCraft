//! View > Macros > Add-Ons: Number Shapes, Array Shapes and Move Shapes. Each is one command and
//! so one undo step, and each takes `ids` or acts on the selection.

use serde_json::{Value, json};
use vizcraft_diagram::End;

use super::shape::coord;
use super::{Kinds, MAX_DELTA, MAX_IDS, arrange, num, opt_bool, opt_str, select, targets, with_sel};
use crate::fragment::Fragment;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Longest separator between a number and the shape's text.
const MAX_SEPARATOR: usize = 16;
/// Largest start number; the count is added to it, so it must leave room in an `i64`.
const MAX_START: i64 = 1_000_000_000_000;

/// The add-on commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("addon.numberShapes", "Number Shapes", "View > Macros > Add-Ons", number_shapes).params(
            r#"{"ids"?: [string], "start"?: n (default 1), "position"?: "before"|"after" (default before: the number leads the text), "separator"?: string (default one space), "renumber"?: bool (strip a number already there first)} -> {numbered: [{id, number}]}; numbers run in z-order"#,
        ),
        CommandSpec::new("addon.arrayShapes", "Array Shapes", "View > Macros > Add-Ons", array_shapes).params(
            r#"{"ids"?: [string], "rows": n, "cols": n, "dx": n, "dy": n} -> {copies}; the original is the top-left cell and each cell is one pitch (dx right, dy down) from its neighbour; at most 10000 copies"#,
        ),
        CommandSpec::new("addon.moveShapes", "Move Shapes", "View > Macros > Add-Ons", move_shapes).params(
            r#"{"ids"?: [string], "dx"?: n, "dy"?: n} or {"distance": n, "angle": degrees counterclockwise from right, 90 is up}, "copy"?: bool (leave the originals and move copies)"#,
        ),
    ]
}

/// Which side of the text the number goes on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Before,
    After,
}

fn position(v: &Value) -> Result<Position, EngineError> {
    match opt_str(v, "position")? {
        None | Some("before") => Ok(Position::Before),
        Some("after") => Ok(Position::After),
        Some(_) => Err(EngineError::Params("`position` must be before or after".into())),
    }
}

/// `text` with a number put on the `pos` side, joined by `sep`; an empty text is the number alone.
fn numbered(text: &str, n: i64, pos: Position, sep: &str) -> String {
    if text.is_empty() {
        return n.to_string();
    }
    match pos {
        Position::Before => format!("{n}{sep}{text}"),
        Position::After => format!("{text}{sep}{n}"),
    }
}

/// `text` without a number a previous run put on the `pos` side: the digits and the separator
/// next to them. Text that is only digits becomes empty. Anything else is returned unchanged.
fn without_number<'a>(text: &'a str, pos: Position, sep: &str) -> &'a str {
    match pos {
        Position::Before => {
            let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if digits == 0 {
                return text;
            }
            let rest = &text[digits..];
            if rest.is_empty() { "" } else { rest.strip_prefix(sep).unwrap_or(text) }
        }
        Position::After => {
            let digits = text.len() - text.trim_end_matches(|c: char| c.is_ascii_digit()).len();
            if digits == 0 {
                return text;
            }
            let head = &text[..text.len() - digits];
            if head.is_empty() { "" } else { head.strip_suffix(sep).unwrap_or(text) }
        }
    }
}

fn number_shapes(s: &mut Session, v: &Value) -> CmdResult {
    let pos = position(v)?;
    let sep = match opt_str(v, "separator")? {
        Some(t) if t.chars().count() > MAX_SEPARATOR => {
            return Err(EngineError::Params(format!("`separator` has more than {MAX_SEPARATOR} characters")));
        }
        Some(t) => t.to_string(),
        None => " ".to_string(),
    };
    let start = num(v, "start")?.map_or(1, |n| (n.round() as i64).clamp(-MAX_START, MAX_START));
    let renumber = opt_bool(v, "renumber")?.unwrap_or(false);
    let wanted = targets(s, v, Kinds::Shapes)?;
    super::layer::ensure_unlocked(s, &wanted)?;
    // Z-order is the page's shape order, bottom first, whatever order the ids came in.
    let ordered: Vec<String> = match s.doc.page() {
        Some(page) => page.shapes.iter().filter(|sh| wanted.contains(&sh.id)).map(|sh| sh.id.clone()).collect(),
        None => return Err(EngineError::Failed("the document has no page".into())),
    };
    let mut out = Vec::new();
    for (i, id) in ordered.iter().enumerate() {
        let n = start + i as i64;
        let Some(sh) = s.doc.page().and_then(|p| p.shape(id)) else { continue };
        let element = sh.element.clone().filter(|e| s.doc.model.element(e).is_some());
        let current = match &element {
            Some(e) => s.doc.model.element(e).map(|e| e.name.clone()).unwrap_or_default(),
            None => sh.text.clone(),
        };
        let base = if renumber { without_number(&current, pos, &sep) } else { current.as_str() };
        let text = numbered(base, n, pos, &sep);
        match element {
            // A shape that draws an element shows the element's name, so the number goes there.
            Some(e) => super::model::rename_element(&mut s.doc, &e, &text)?,
            None => {
                if let Some(sh) = s.doc.page_mut().and_then(|p| p.shape_mut(id)) {
                    sh.text = text;
                    sh.sanitize();
                }
            }
        }
        out.push(json!({"id": id, "number": n}));
    }
    with_sel(s, json!({"numbered": out}))
}

fn array_shapes(s: &mut Session, v: &Value) -> CmdResult {
    let count = |k: &str| -> Result<usize, EngineError> {
        let n = super::req_num(v, k)?;
        if n < 1.0 || n > MAX_IDS as f64 {
            return Err(EngineError::Params(format!("`{k}` must be a whole number from 1 to {MAX_IDS}")));
        }
        Ok(n as usize)
    };
    let (rows, cols) = (count("rows")?, count("cols")?);
    let dx = super::req_num(v, "dx")?.clamp(-MAX_DELTA, MAX_DELTA);
    let dy = super::req_num(v, "dy")?.clamp(-MAX_DELTA, MAX_DELTA);
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
    // Checked before anything is copied: the cap is on the cells, not on what a cell holds.
    let cells = rows.saturating_mul(cols);
    if cells > MAX_IDS {
        return Err(EngineError::Params(format!("an array of {rows} by {cols} would make more than {MAX_IDS} copies")));
    }
    let frag = Fragment::of_items(&s.doc, &ids);
    if frag.is_empty() {
        return Err(EngineError::Params("nothing to copy".into()));
    }
    let mut made = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            if r == 0 && c == 0 {
                continue;
            }
            let ox = (c as f64 * dx).clamp(-MAX_DELTA, MAX_DELTA);
            let oy = (r as f64 * dy).clamp(-MAX_DELTA, MAX_DELTA);
            made.extend(frag.insert(&mut s.doc, ox, oy)?);
        }
    }
    let copies = cells - 1;
    if !made.is_empty() {
        select(s, made.clone());
    }
    with_sel(s, json!({"copies": copies, "ids": made}))
}

fn move_shapes(s: &mut Session, v: &Value) -> CmdResult {
    let (dx, dy) = match (num(v, "distance")?, num(v, "angle")?) {
        (Some(d), a) => {
            let rad = a.unwrap_or(0.0).to_radians();
            (d * rad.cos(), -d * rad.sin())
        }
        (None, Some(_)) => return Err(EngineError::Params("`angle` needs a `distance`".into())),
        (None, None) => (num(v, "dx")?.unwrap_or(0.0), num(v, "dy")?.unwrap_or(0.0)),
    };
    let (dx, dy) = (dx.clamp(-MAX_DELTA, MAX_DELTA), dy.clamp(-MAX_DELTA, MAX_DELTA));
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
    if opt_bool(v, "copy")?.unwrap_or(false) {
        let frag = Fragment::of_items(&s.doc, &ids);
        if frag.is_empty() {
            return Err(EngineError::Params("nothing to copy".into()));
        }
        let made = frag.insert(&mut s.doc, dx, dy)?;
        select(s, made.clone());
        return with_sel(s, json!({"copied": made}));
    }
    let page = super::page_mut(s)?;
    // Shapes move through the arrange code so a group carries its members.
    let shape_ids: Vec<String> = ids.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
    arrange::move_shapes(page, &shape_ids, dx, dy);
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            for e in [&mut c.start, &mut c.end] {
                if let End::Free { x, y } = e {
                    *x = coord(*x + dx);
                    *y = coord(*y + dy);
                }
            }
            c.sanitize();
        }
    }
    with_sel(s, json!({"moved": ids}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Session;

    /// Three shapes added in the order c, a, b (so z-order is c, a, b), with texts.
    fn session() -> Session {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        for (id, x) in [("c", 300), ("a", 0), ("b", 150)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": 0, "w": 40, "h": 20, "text": id})).unwrap();
        }
        s.run("edit.selectNone", &json!({})).unwrap();
        s
    }

    fn text(s: &Session, id: &str) -> String {
        s.doc.page().unwrap().shape(id).unwrap().text.clone()
    }

    fn pos(s: &Session, id: &str) -> (f64, f64) {
        let sh = s.doc.page().unwrap().shape(id).unwrap();
        (sh.x, sh.y)
    }

    #[test]
    fn numbering_follows_z_order_and_puts_the_prefix_first() {
        let mut s = session();
        // The ids come in a different order than the z-order; the z-order wins.
        let r = s.run("addon.numberShapes", &json!({"ids": ["b", "a", "c"], "start": 5, "separator": ". "})).unwrap();
        assert_eq!(text(&s, "c"), "5. c");
        assert_eq!(text(&s, "a"), "6. a");
        assert_eq!(text(&s, "b"), "7. b");
        assert_eq!(r["numbered"][2]["number"], 7);
    }

    #[test]
    fn numbering_as_a_suffix_and_renumbering() {
        let mut s = session();
        s.run("addon.numberShapes", &json!({"ids": ["a", "b"], "position": "after", "separator": "-"})).unwrap();
        assert_eq!(text(&s, "a"), "a-1");
        assert_eq!(text(&s, "b"), "b-2");
        // Without `renumber` a second run stacks numbers; with it the old one is replaced.
        s.run("addon.numberShapes", &json!({"ids": ["a", "b"], "position": "after", "separator": "-"})).unwrap();
        assert_eq!(text(&s, "a"), "a-1-1");
        s.run("shape.setText", &json!({"id": "a", "text": "a-9"})).unwrap();
        s.run("addon.numberShapes", &json!({"ids": ["a"], "position": "after", "separator": "-", "renumber": true, "start": 3})).unwrap();
        assert_eq!(text(&s, "a"), "a-3");
        s.run("addon.numberShapes", &json!({"ids": ["a"], "renumber": true})).unwrap();
        assert_eq!(text(&s, "a"), "1 a-3", "no leading number to strip");
        s.run("addon.numberShapes", &json!({"ids": ["a"], "renumber": true, "start": 8})).unwrap();
        assert_eq!(text(&s, "a"), "8 a-3");
        assert!(s.run("addon.numberShapes", &json!({"ids": ["a"], "position": "middle"})).is_err());
        assert!(s.run("addon.numberShapes", &json!({"ids": ["a"], "separator": "x".repeat(40)})).is_err());
    }

    #[test]
    fn numbering_helpers_handle_edges() {
        assert_eq!(numbered("", 4, Position::Before, " "), "4");
        assert_eq!(without_number("12", Position::Before, " "), "");
        assert_eq!(without_number("12 box", Position::Before, " "), "box");
        assert_eq!(without_number("12box", Position::Before, " "), "12box");
        assert_eq!(without_number("box 12", Position::After, " "), "box");
        assert_eq!(without_number("box", Position::After, " "), "box");
        assert_eq!(without_number("", Position::After, " "), "");
        // Multi-byte separators and text never split a character.
        assert_eq!(without_number("3\u{2014}caf\u{e9}", Position::Before, "\u{2014}"), "caf\u{e9}");
    }

    #[test]
    fn array_makes_rows_by_cols_at_the_pitch_in_one_undo_step() {
        let mut s = session();
        let before = s.doc.page().unwrap().shapes.len();
        let r = s.run("addon.arrayShapes", &json!({"ids": ["a"], "rows": 2, "cols": 3, "dx": 60, "dy": 40})).unwrap();
        assert_eq!(r["copies"], 5);
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.len(), before + 5);
        let ids: Vec<String> = r["ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        let mut at: Vec<(i64, i64)> = ids.iter().map(|i| (pos(&s, i).0 as i64, pos(&s, i).1 as i64)).collect();
        at.sort();
        assert_eq!(at, [(0, 40), (60, 0), (60, 40), (120, 0), (120, 40)]);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), before, "one undo removes the whole array");
    }

    #[test]
    fn array_refuses_hostile_sizes_and_changes_nothing() {
        let mut s = session();
        for p in [
            json!({"ids": ["a"], "rows": 0, "cols": 2, "dx": 1, "dy": 1}),
            json!({"ids": ["a"], "rows": 2, "cols": -1, "dx": 1, "dy": 1}),
            json!({"ids": ["a"], "rows": 100000, "cols": 100000, "dx": 1, "dy": 1}),
            json!({"ids": ["a"], "rows": 200, "cols": 200, "dx": 1, "dy": 1}),
            json!({"ids": ["a"], "rows": 2, "cols": 2}),
            json!({"ids": ["nope"], "rows": 2, "cols": 2, "dx": 1, "dy": 1}),
        ] {
            assert!(s.run("addon.arrayShapes", &p).is_err(), "{p}");
        }
        assert_eq!(s.doc.page().unwrap().shapes.len(), 3);
    }

    #[test]
    fn move_by_distance_and_angle() {
        let mut s = session();
        // 90 degrees is up on the screen, which is negative y.
        s.run("addon.moveShapes", &json!({"ids": ["a"], "distance": 50, "angle": 90})).unwrap();
        let (x, y) = pos(&s, "a");
        assert!(x.abs() < 1e-6 && (y + 50.0).abs() < 1e-6, "{x} {y}");
        s.run("addon.moveShapes", &json!({"ids": ["b"], "distance": 10})).unwrap();
        assert_eq!(pos(&s, "b"), (160.0, 0.0), "no angle means to the right");
        s.run("addon.moveShapes", &json!({"ids": ["b"], "dx": -10, "dy": 5})).unwrap();
        assert_eq!(pos(&s, "b"), (150.0, 5.0));
        assert!(s.run("addon.moveShapes", &json!({"ids": ["b"], "angle": 30})).is_err());
    }

    #[test]
    fn move_with_copy_leaves_the_original() {
        let mut s = session();
        let r = s.run("addon.moveShapes", &json!({"ids": ["a"], "dx": 0, "dy": 100, "copy": true})).unwrap();
        assert_eq!(pos(&s, "a"), (0.0, 0.0));
        let copy = r["copied"][0].as_str().unwrap().to_string();
        assert_eq!(pos(&s, &copy), (0.0, 100.0));
        assert_eq!(s.doc.page().unwrap().shapes.len(), 4);
    }
}
