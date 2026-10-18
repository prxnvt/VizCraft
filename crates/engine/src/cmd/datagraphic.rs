//! Data graphic commands (`datagraphic.*`): define, list, remove and apply the document's data
//! graphics, and build a legend for one.
//!
//! The graphic model, its layout and its evaluation live in `vizcraft_diagram::data_graphic`.
//! Definitions are checked strictly here (an unknown kind, operator, position, icon or colour is
//! a params error) so a typo is reported rather than drawn as nothing.

use std::collections::HashSet;

use serde_json::{Map, Value, json};
use vizcraft_diagram::arrange as ar;
use vizcraft_diagram::data_graphic::{
    DataGraphic, DataItem, ICONS, ItemKind, LEGEND_EXT, MAX_GRAPHICS, MAX_ITEMS, MAX_NAME, MAX_RULES, Op, Position, Rule, Source, legend_rows,
};
use vizcraft_diagram::{Color, HAlign, Page, Shape, ShapeKind, data};

use super::{MAX_IDS, ids_param, opt_str, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Data > Display Data";

/// The data graphic commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("datagraphic.define", "Define Data Graphic", LOCATION, define).params(
            r##"{"id"?: string (an existing id replaces that graphic), "name": string, "items": [{"kind": "text|bar|icons|color", "key": data key or property, "from"?: "data|property", "position"?: "top|bottom|left|right|center|topLeft|topRight|bottomLeft|bottomRight", "label"?: string, "format"?: "0.00|#,##0|0%|$#,##0.00", "min"?: n, "max"?: n, "color"?: "#rrggbb", "rules"?: [{"op": "eq|ne|lt|le|gt|ge|between|contains|empty|notEmpty|any" (or = != < <= > >=), "value"?: string|number, "to"?: string|number, "icon"?: "check|close|info|plus|minus|chevronUp|chevronDown", "fill"?: "#rrggbb", "line"?: "#rrggbb", "label"?: string}]}]} -> {id, graphic}"##,
        ),
        CommandSpec::new("datagraphic.list", "Data Graphics", LOCATION, list)
            .params(r#"{} -> {graphics: [{id, name, items, shapes}], current: id of the primary selection's graphic or null}"#),
        CommandSpec::new("datagraphic.remove", "Delete Data Graphic", LOCATION, remove)
            .params(r#"{"id": graphic id} -> {id, cleared: [shape id]}; shapes showing it show none"#),
        CommandSpec::new("datagraphic.apply", "Apply Data Graphic", LOCATION, apply).params(
            r#"{"ids"?: [shape id] (default: the selected shapes), "graphic": graphic id or null (removes the graphic from the shapes)} -> {graphic, changed}"#,
        ),
        CommandSpec::new("datagraphic.legend", "Insert Legend", LOCATION, legend).params(
            r#"{"page"?: id|name|index, "graphic"?: graphic id (default: the selection's, else the first one shapes on the page show, else the first)} -> {id, graphic, rows}; a group (title, then a swatch and label per rule) at the right edge of the page frame, replacing that graphic's earlier legend on the page"#,
        ),
    ]
}

fn params(msg: impl Into<String>) -> EngineError {
    EngineError::Params(msg.into())
}

/// A required string param.
fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
    opt_str(v, k)?.ok_or_else(|| params(format!("`{k}` (string) is required")))
}

/// An optional text field of an item or rule: a string, or a number or boolean as its text.
fn text_field(o: &Map<String, Value>, k: &str, max: usize) -> Result<String, EngineError> {
    let t = match o.get(k) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => {
            n.as_f64().filter(|f| f.is_finite()).map(data::fmt_number).ok_or_else(|| params(format!("`{k}` is out of range")))?
        }
        Some(Value::Bool(b)) => b.to_string(),
        Some(_) => return Err(params(format!("`{k}` must be a string or a number"))),
    };
    if t.len() > max {
        return Err(params(format!("`{k}` is at most {max} bytes")));
    }
    Ok(t)
}

/// An optional named field: `parse` turns the name into a value.
fn named<T>(o: &Map<String, Value>, k: &str, parse: impl Fn(&str) -> Option<T>, allowed: &str) -> Result<Option<T>, EngineError> {
    match o.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => parse(s.trim()).map(Some).ok_or_else(|| params(format!("`{k}` must be {allowed}, not `{}`", short(s)))),
        Some(_) => Err(params(format!("`{k}` must be {allowed}"))),
    }
}

fn color_field(o: &Map<String, Value>, k: &str) -> Result<Option<Color>, EngineError> {
    named(o, k, Color::parse, "a colour such as #22aa44")
}

fn num_field(o: &Map<String, Value>, k: &str) -> Result<Option<f64>, EngineError> {
    match o.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x.as_f64().filter(|f| f.is_finite() && f.abs() <= 1e15).map(Some).ok_or_else(|| params(format!("`{k}` must be a number"))),
    }
}

fn parse_rule(v: &Value, kind: ItemKind) -> Result<Rule, EngineError> {
    let o = v.as_object().ok_or_else(|| params("each rule must be an object"))?;
    let op = named(o, "op", Op::parse, "eq, ne, lt, le, gt, ge, between, contains, empty, notEmpty or any")?.unwrap_or(Op::Eq);
    let icon = text_field(o, "icon", MAX_NAME)?;
    if kind == ItemKind::Icons && !ICONS.contains(&icon.as_str()) {
        return Err(params(format!("an icon set rule needs `icon`: one of {}", ICONS.join(", "))));
    }
    let rule = Rule {
        op,
        value: text_field(o, "value", MAX_NAME)?,
        to: text_field(o, "to", MAX_NAME)?,
        icon,
        fill: color_field(o, "fill")?,
        line: color_field(o, "line")?,
        label: text_field(o, "label", MAX_NAME)?,
    };
    if kind == ItemKind::Color && rule.fill.is_none() && rule.line.is_none() {
        return Err(params("a colour by value rule needs `fill` or `line`"));
    }
    Ok(rule)
}

/// One item from its JSON form.
pub fn parse_item(v: &Value) -> Result<DataItem, EngineError> {
    let o = v.as_object().ok_or_else(|| params("each item must be an object"))?;
    let kind = named(o, "kind", ItemKind::from_name, "text, bar, icons or color")?.ok_or_else(|| params("each item needs `kind`"))?;
    let from = named(o, "from", Source::from_name, "data or property")?.unwrap_or_default();
    let key = text_field(o, "key", data::MAX_KEY)?;
    if !data::is_key(&key) {
        return Err(params(format!("each item needs `key`: a data key or property name, not `{}`", short(&key))));
    }
    let mut item = DataItem {
        kind,
        from,
        key,
        position: named(o, "position", Position::from_name, "top, bottom, left, right, center, topLeft, topRight, bottomLeft or bottomRight")?
            .unwrap_or(if kind == ItemKind::Icons { Position::TopRight } else { Position::Bottom }),
        label: text_field(o, "label", MAX_NAME)?,
        format: text_field(o, "format", MAX_NAME)?,
        ..DataItem::default()
    };
    if let Some(m) = num_field(o, "min")? {
        item.min = m;
    }
    if let Some(m) = num_field(o, "max")? {
        item.max = m;
    }
    if kind == ItemKind::Bar && item.max <= item.min {
        return Err(params("a data bar needs `max` above `min`"));
    }
    if let Some(c) = color_field(o, "color")? {
        item.color = c;
    }
    match o.get("rules") {
        None | Some(Value::Null) => {}
        Some(Value::Array(a)) if a.len() <= MAX_RULES => {
            item.rules = a.iter().map(|r| parse_rule(r, kind)).collect::<Result<_, _>>()?;
        }
        Some(_) => return Err(params(format!("`rules` must be a list of at most {MAX_RULES} rules"))),
    }
    if matches!(kind, ItemKind::Icons | ItemKind::Color) && item.rules.is_empty() {
        return Err(params(format!("a {} item needs `rules`", kind.name())));
    }
    item.sanitize();
    Ok(item)
}

/// A free graphic id: `dg1`, `dg2`, ...
fn next_id(s: &Session) -> String {
    let used: HashSet<&str> = s.doc.data_graphics.iter().map(|g| g.id.as_str()).collect();
    (1..).map(|n| format!("dg{n}")).find(|id| !used.contains(id.as_str())).unwrap_or_default()
}

fn define(s: &mut Session, v: &Value) -> CmdResult {
    let name = req_str(v, "name")?.trim().to_string();
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(params(format!("`name` must be 1 to {MAX_NAME} bytes")));
    }
    let items = match v.get("items") {
        Some(Value::Array(a)) if a.len() <= MAX_ITEMS => a.iter().map(parse_item).collect::<Result<Vec<_>, _>>()?,
        _ => return Err(params(format!("`items` (a list of at most {MAX_ITEMS} items) is required"))),
    };
    let id = match opt_str(v, "id")? {
        Some(id) if vizcraft_diagram::is_plain_id(id) => id.to_string(),
        Some(id) => return Err(params(format!("`id` must be 1 to 64 letters, digits, `_`, `-` or `.`, not `{}`", short(id)))),
        None => next_id(s),
    };
    let graphic = DataGraphic { id: id.clone(), name, items };
    match s.doc.data_graphics.iter_mut().find(|g| g.id == id) {
        Some(g) => *g = graphic.clone(),
        None => {
            if s.doc.data_graphics.len() >= MAX_GRAPHICS {
                return Err(params(format!("the document already has {MAX_GRAPHICS} data graphics")));
            }
            s.doc.data_graphics.push(graphic.clone());
        }
    }
    Ok(json!({"id": id, "graphic": graphic}))
}

/// The graphic the primary selection shows.
pub fn current(s: &Session) -> Option<String> {
    let id = s.sel.primary.as_deref()?;
    s.doc.page()?.shape(id)?.data_graphic.clone()
}

fn list(s: &mut Session, _: &Value) -> CmdResult {
    let graphics: Vec<Value> = s
        .doc
        .data_graphics
        .iter()
        .map(|g| {
            let shapes = s.doc.pages.iter().flat_map(|p| &p.shapes).filter(|sh| sh.data_graphic.as_deref() == Some(g.id.as_str())).count();
            json!({"id": g.id, "name": g.name, "items": g.items, "shapes": shapes})
        })
        .collect();
    Ok(json!({"graphics": graphics, "current": current(s)}))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_str(v, "id")?.to_string();
    let n = s.doc.data_graphics.len();
    s.doc.data_graphics.retain(|g| g.id != id);
    if s.doc.data_graphics.len() == n {
        return Err(params(format!("no data graphic `{}`", short(&id))));
    }
    let mut cleared = Vec::new();
    for sh in s.doc.pages.iter_mut().flat_map(|p| &mut p.shapes) {
        if sh.data_graphic.as_deref() == Some(id.as_str()) {
            sh.data_graphic = None;
            cleared.push(sh.id.clone());
        }
    }
    Ok(json!({"id": id, "cleared": cleared}))
}

fn apply(s: &mut Session, v: &Value) -> CmdResult {
    let graphic = match v.get("graphic") {
        Some(Value::Null) => None,
        Some(Value::String(g)) if g.is_empty() => None,
        Some(Value::String(g)) => {
            if !s.doc.data_graphics.iter().any(|d| d.id == *g) {
                return Err(params(format!("no data graphic `{}`", short(g))));
            }
            Some(g.clone())
        }
        _ => return Err(params("`graphic` (a graphic id, or null to remove) is required")),
    };
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let ids: Vec<String> = match ids_param(v)? {
        Some(ids) => {
            for id in &ids {
                if page.shape(id).is_none() {
                    return Err(params(format!("`{}` is not a shape on this page", short(id))));
                }
            }
            ids
        }
        None => s.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect(),
    };
    if ids.is_empty() {
        return Err(params("select a shape first (or pass `ids`)"));
    }
    for id in &ids {
        if let Some(sh) = page.shape(id)
            && vizcraft_diagram::layer::locked(page, sh)
        {
            return Err(params(format!("`{}` is on a locked layer", short(id))));
        }
    }
    let mut changed = Vec::new();
    if let Some(p) = s.doc.page_mut() {
        for id in ids.iter().take(MAX_IDS) {
            if let Some(sh) = p.shape_mut(id)
                && sh.data_graphic != graphic
            {
                sh.data_graphic = graphic.clone();
                changed.push(id.clone());
            }
        }
    }
    with_sel(s, json!({"graphic": graphic, "changed": changed}))
}

/// The page the `page` param names (id, name or index), else the active page.
fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let r = match v.get("page") {
        None | Some(Value::Null) => return Ok(s.doc.active_index()),
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(params("`page` must be a page id, name or index")),
    };
    s.doc.find_page(&r).ok_or_else(|| params(format!("no page `{}`", short(&r))))
}

/// Remove `graphic`'s legend groups (and everything in them) from `page`.
fn remove_legends(page: &mut Page, graphic: &str) {
    let roots: Vec<String> =
        page.shapes.iter().filter(|s| s.ext.get(LEGEND_EXT).map(String::as_str) == Some(graphic)).map(|s| s.id.clone()).collect();
    if roots.is_empty() {
        return;
    }
    let mut gone: HashSet<String> = HashSet::new();
    for r in &roots {
        gone.insert(r.clone());
        gone.extend(ar::descendants(page, r));
    }
    page.shapes.retain(|s| !gone.contains(&s.id));
    page.connectors.retain(|c| !c.start.shape().is_some_and(|id| gone.contains(id)) && !c.end.shape().is_some_and(|id| gone.contains(id)));
}

/// A legend text box: no fill, no line, left aligned.
fn label_shape(id: String, x: f64, y: f64, w: f64, h: f64, text: &str, bold: bool) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, w, h);
    s.text = text.to_string();
    s.style.fill = Color::TRANSPARENT;
    s.style.line = Color::TRANSPARENT;
    s.style.h_align = HAlign::Left;
    s.style.bold = bold;
    s.style.font_size = 11.0;
    s
}

fn legend(s: &mut Session, v: &Value) -> CmdResult {
    let pi = page_index(s, v)?;
    let graphic = match opt_str(v, "graphic")? {
        Some(g) => g.to_string(),
        None => {
            let on_page = s.doc.pages.get(pi).and_then(|p| p.shapes.iter().find_map(|sh| sh.data_graphic.clone()));
            current(s)
                .or(on_page)
                .or_else(|| s.doc.data_graphics.first().map(|g| g.id.clone()))
                .ok_or_else(|| params("the document has no data graphics"))?
        }
    };
    let g = s.doc.data_graphics.iter().find(|d| d.id == graphic).cloned().ok_or_else(|| params(format!("no data graphic `{}`", short(&graphic))))?;
    let rows = legend_rows(&g);
    if rows.is_empty() {
        return Err(params(format!("`{}` has no icon set or colour by value rules to explain", short(&graphic))));
    }
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    remove_legends(page, &graphic);
    let frame = page.frame();
    let (x, mut y) = (frame.x1 + 24.0, frame.y0);
    let mut ids = Vec::new();
    let mut shapes = Vec::new();
    let title = s.doc.next_id("s");
    shapes.push(label_shape(title.clone(), x, y, 180.0, 22.0, &format!("Legend: {}", g.title()), true));
    ids.push(title);
    y += 26.0;
    for row in &rows {
        let sw = s.doc.next_id("s");
        let mut swatch =
            Shape::new(sw.clone(), if row.icon.is_some() { ShapeKind::Ellipse } else { ShapeKind::Rectangle }, x + 2.0, y + 4.0, 12.0, 12.0);
        swatch.style.fill = row.swatch;
        swatch.style.line_width = 0.75;
        shapes.push(swatch);
        let lb = s.doc.next_id("s");
        let text = match &row.icon {
            Some(icon) => format!("{} ({icon})", row.label),
            None => row.label.clone(),
        };
        shapes.push(label_shape(lb.clone(), x + 20.0, y, 160.0, 20.0, &text, false));
        ids.push(sw);
        ids.push(lb);
        y += 22.0;
    }
    let gid = s.doc.next_id("g");
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    page.shapes.extend(shapes);
    ar::group(page, &ids, &gid).map_err(|e| EngineError::Failed(e.to_string()))?;
    if let Some(grp) = page.shape_mut(&gid) {
        grp.ext.insert(LEGEND_EXT.into(), graphic.clone());
    }
    Ok(json!({"id": gid, "graphic": graphic, "rows": rows.len()}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::Document;

    use crate::Session;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": "a", "x": 0, "y": 0, "w": 100, "h": 60})).unwrap();
        s.run("data.set", &json!({"ids": ["a"], "key": "cost", "value": 75})).unwrap();
        s
    }

    fn status_graphic(s: &mut Session) -> String {
        let r = s
            .run(
                "datagraphic.define",
                &json!({"name": "Status", "items": [
                    {"kind": "text", "key": "cost", "label": "Cost"},
                    {"kind": "icons", "key": "cost", "rules": [{"op": ">", "value": 50, "icon": "check"}, {"op": "any", "icon": "close"}]},
                    {"kind": "color", "key": "cost", "rules": [{"op": "between", "value": 0, "to": 50, "fill": "#fee2e2"}, {"op": "gt", "value": 50, "fill": "#dcfce7", "line": "#166534"}, {"op": "empty", "fill": "#eeeeee"}]},
                ]}),
            )
            .unwrap();
        r["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn define_apply_and_colour_by_value_leaves_the_style_alone() {
        let mut s = session();
        let id = status_graphic(&mut s);
        assert_eq!(id, "dg1");
        let style = s.doc.shape("a").unwrap().style.clone();
        let r = s.run("datagraphic.apply", &json!({"ids": ["a"], "graphic": id})).unwrap();
        assert_eq!(r["changed"], json!(["a"]));
        let page = s.doc.page().unwrap();
        let rendered = vizcraft_diagram::data_graphic::render(&s.doc, page, page.shape("a").unwrap()).unwrap();
        assert_eq!(rendered.fill.map(|c| c.to_hex()).as_deref(), Some("#dcfce7"));
        assert_eq!(s.doc.shape("a").unwrap().style, style, "colour by value never writes the style");
        let list = s.run("datagraphic.list", &json!({})).unwrap();
        assert_eq!(list["graphics"][0]["shapes"], 1);
        s.run("datagraphic.apply", &json!({"ids": ["a"], "graphic": null})).unwrap();
        assert_eq!(s.doc.shape("a").unwrap().data_graphic, None);
    }

    #[test]
    fn bad_definitions_are_refused() {
        let mut s = session();
        for items in [
            json!([{"kind": "sparkle", "key": "cost"}]),
            json!([{"kind": "text"}]),
            json!([{"kind": "icons", "key": "cost", "rules": [{"op": "gt", "value": 1, "icon": "smiley"}]}]),
            json!([{"kind": "color", "key": "cost", "rules": [{"op": "gt", "value": 1}]}]),
            json!([{"kind": "bar", "key": "cost", "min": 5, "max": 1}]),
            json!([{"kind": "text", "key": "cost", "position": "nowhere"}]),
            json!("x"),
        ] {
            assert!(s.run("datagraphic.define", &json!({"name": "x", "items": items})).is_err(), "{items}");
        }
        assert!(s.doc.data_graphics.is_empty());
    }

    #[test]
    fn the_legend_has_one_row_per_rule_and_replaces_itself() {
        let mut s = session();
        let id = status_graphic(&mut s);
        let r = s.run("datagraphic.legend", &json!({"graphic": id})).unwrap();
        assert_eq!(r["rows"], 5, "two icon rules and three colour rules");
        let gid = r["id"].as_str().unwrap().to_string();
        let page = s.doc.page().unwrap();
        let members = vizcraft_diagram::arrange::children(page, &gid);
        assert_eq!(members.len(), 1 + 2 * 5, "a title, then a swatch and a label per rule");
        let frame_right = page.shape("a").unwrap().bounds().x1;
        assert!(page.shape(&gid).unwrap().x > frame_right, "at the right of the drawing");
        let n = page.shapes.len();
        let again = s.run("datagraphic.legend", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), n, "regenerated, not added");
        assert!(s.doc.page().unwrap().shape(&gid).is_none());
        assert_eq!(again["graphic"], json!(id));
        s.run("datagraphic.remove", &json!({"id": id})).unwrap();
        assert!(s.run("datagraphic.legend", &json!({})).is_err());
    }
}
