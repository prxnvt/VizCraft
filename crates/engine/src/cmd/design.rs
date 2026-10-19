//! Design commands: document themes and variants, quick styles, page backgrounds, borders and
//! titles, and the page-wide connector style.
//!
//! Themes restyle only what has no hand styling (see `vizcraft_diagram::theme`). New shapes and
//! connectors take the theme too: [`after_change`] runs after every command that changes the
//! document, inside its undo step, and also rebuilds the generated decoration pages
//! (`vizcraft_diagram::decor`) so title blocks follow the titles.

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::decor::{self, BackgroundKind, BorderKind, Decoration};
use vizcraft_diagram::theme::{self, DocTheme, QUICK_STYLES, VARIANTS};
use vizcraft_diagram::{Color, Connector, Document, Route, Shape, Style};

use super::{Kinds, has_shapes, num, opt_str, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all design commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("design.themes", "Themes", "Design > Themes", themes).params(
            r#"{} -> {"current": {"theme", "variant"}, "themes": [{"id", "name", "font", "variants": [{"name", "fill", "line", "connector", "text", "background"}]}], "quickStyles": [{"index", "name", "fill", "line", "text"}]}"#,
        ),
        CommandSpec::new("design.setTheme", "Theme", "Design > Themes", set_theme).params(
            r#"{"theme": id|"none", "variant"?: 0..3} -> {"theme", "variant", "restyled": n}; restyles shapes and connectors that have no hand styling"#,
        ),
        CommandSpec::new("design.setVariant", "Variant", "Design > Variants", set_variant)
            .params(r#"{"variant": 0..3} -> {"theme", "variant", "restyled": n}"#)
            .when(has_theme),
        CommandSpec::new("design.applyQuickStyle", "Quick Style", "Home > Shape Styles", apply_quick_style)
            .params(r#"{"style": 0..11, "ids"?: [string]} (default: selection); row style / 4 is subtle, moderate, intense; column style % 4 the accent"#)
            .when(has_shapes),
        CommandSpec::new("design.setBackground", "Backgrounds", "Design > Backgrounds", set_background).params(
            r##"{"kind": "none"|"solid"|"gradient"|"pattern", "color"?: "#rrggbb", "page"?: id|name|index (default: active)}; drawn on a generated background page"##,
        ),
        CommandSpec::new("design.setBorderTitle", "Borders and Titles", "Design > Backgrounds", set_border_title).params(
            r#"{"kind": "none"|"frame"|"banner"|"margin", "page"?: id|name|index (default: active)}; the title block shows the document title and the page name; on a page with a sheet (sheet.set) the sheet frame and title block are drawn instead"#,
        ),
        CommandSpec::new("design.setConnectorStyle", "Connector Style", "Design > Connectors", set_connector_style).params(
            r#"{"style": "rightAngle"|"straight"|"curved", "page"?: id|name|index (default: active)} -> {"page", "ids", "style"}; re-routes every connector on the page"#,
        ),
    ]
}

/// Enabled when the document has a theme.
fn has_theme(s: &Session) -> Option<&'static str> {
    if s.doc.theme.theme().is_some() { None } else { Some("choose a theme first") }
}

/// The style a shape has with no theme: its master's style, else the default.
fn shape_base(s: &Shape) -> Style {
    vizcraft_stencils::master(&s.stencil).map(|m| m.style.clone()).unwrap_or(Style::DEFAULT)
}

/// The style a connector has with no theme.
fn connector_base() -> Style {
    Connector::default().style
}

/// Move every shape and connector from theme `from` to theme `to`, keeping hand styling.
/// Generated decoration pages are rebuilt instead. Returns how many items changed.
fn restyle_all(doc: &mut Document, from: &DocTheme, to: &DocTheme) -> usize {
    let cbase = connector_base();
    let (c_was, c_now) = (theme::connector_style(from, &cbase), theme::connector_style(to, &cbase));
    let mut n = 0usize;
    for page in doc.pages.iter_mut().filter(|p| !decor::is_generated(p)) {
        for s in &mut page.shapes {
            let base = shape_base(s);
            if theme::restyle(&mut s.style, &theme::shape_style(from, &base), &theme::shape_style(to, &base)) {
                s.style.sanitize();
                n = n.saturating_add(1);
            }
        }
        for c in &mut page.connectors {
            if theme::restyle(&mut c.style, &c_was, &c_now) {
                c.style.sanitize();
                n = n.saturating_add(1);
            }
        }
    }
    n
}

/// Switch the document to `to`, restyling and rebuilding decorations.
fn switch_theme(s: &mut Session, to: DocTheme) -> CmdResult {
    let from = s.doc.theme.clone();
    let restyled = restyle_all(&mut s.doc, &from, &to);
    s.doc.theme = to;
    decor::refresh(&mut s.doc);
    let t = &s.doc.theme;
    let id = if t.id.is_empty() { "none" } else { t.id.as_str() };
    Ok(json!({"theme": id, "variant": t.variant, "restyled": restyled}))
}

/// The `variant` param: `None` when absent, else an index in `0..VARIANTS`.
fn variant_param(v: &Value) -> Result<Option<usize>, EngineError> {
    match num(v, "variant")? {
        None => Ok(None),
        Some(f) if f.fract() == 0.0 && (0.0..VARIANTS as f64).contains(&f) => Ok(Some(f as usize)),
        Some(_) => Err(EngineError::Params(format!("`variant` must be 0 to {}", VARIANTS - 1))),
    }
}

fn set_theme(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "theme")?;
    let variant = variant_param(v)?;
    let to = if id == "none" {
        DocTheme::default()
    } else {
        if theme::theme(id).is_none() {
            let ids: Vec<&str> = theme::themes().iter().map(|t| t.id).collect();
            return Err(EngineError::Params(format!("unknown theme `{}`; one of none, {}", short(id), ids.join(", "))));
        }
        // Keep the variant when only the theme changes, as a gallery click does.
        DocTheme::new(id, variant.unwrap_or(usize::from(s.doc.theme.variant)))
    };
    switch_theme(s, to)
}

fn set_variant(s: &mut Session, v: &Value) -> CmdResult {
    let variant = variant_param(v)?.ok_or_else(|| EngineError::Params(format!("`variant` (0 to {}) is required", VARIANTS - 1)))?;
    let to = DocTheme::new(s.doc.theme.id.clone(), variant);
    switch_theme(s, to)
}

fn color_json(c: Color) -> String {
    c.to_hex()
}

fn themes(s: &mut Session, _v: &Value) -> CmdResult {
    let list: Vec<Value> = theme::themes()
        .iter()
        .map(|t| {
            let variants: Vec<Value> = (0..VARIANTS)
                .map(|i| {
                    let c = t.variant(i);
                    json!({"name": c.name, "fill": color_json(c.fill), "line": color_json(c.line), "connector": color_json(c.connector),
                           "text": color_json(c.text), "background": color_json(c.background)})
                })
                .collect();
            json!({"id": t.id, "name": t.name, "font": t.font, "variants": variants})
        })
        .collect();
    let quick: Vec<Value> = (0..QUICK_STYLES)
        .filter_map(|n| theme::quick_style(&s.doc.theme, n).map(|q| (n, q)))
        .map(|(n, q)| json!({"index": n, "name": q.name, "fill": color_json(q.fill), "line": color_json(q.line), "text": color_json(q.text)}))
        .collect();
    let t = &s.doc.theme;
    let id = if t.id.is_empty() { "none" } else { t.id.as_str() };
    Ok(json!({"current": {"theme": id, "variant": t.variant}, "themes": list, "quickStyles": quick}))
}

fn apply_quick_style(s: &mut Session, v: &Value) -> CmdResult {
    let n = num(v, "style")?.ok_or_else(|| EngineError::Params(format!("`style` (0 to {}) is required", QUICK_STYLES - 1)))?;
    if n.fract() != 0.0 || !(0.0..QUICK_STYLES as f64).contains(&n) {
        return Err(EngineError::Params(format!("`style` must be 0 to {}", QUICK_STYLES - 1)));
    }
    let q = theme::quick_style(&s.doc.theme, n as usize).ok_or_else(|| EngineError::Params("no such quick style".into()))?;
    let ids = targets(s, v, Kinds::Both)?;
    let page = super::page_mut(s)?;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            q.apply(&mut sh.style, true);
        } else if let Some(c) = page.connector_mut(id) {
            q.apply(&mut c.style, false);
        }
    }
    with_sel(s, json!({"styled": ids, "style": q.name}))
}

/// The page a command acts on: the `page` param (id, name or index), else the active page.
fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    match v.get("page") {
        None | Some(Value::Null) => Ok(s.doc.active_index()),
        Some(r) => {
            let r = match r {
                Value::String(t) => t.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
            };
            s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))
        }
    }
}

fn decorate(s: &mut Session, index: usize, deco: Decoration) -> CmdResult {
    decor::set(&mut s.doc, index, deco).map_err(|e| EngineError::Params(e.to_string()))?;
    let page = s.doc.pages.get(index).map(|p| p.id.clone()).unwrap_or_default();
    let background = decor::generated_page(&s.doc, &page).and_then(|g| s.doc.pages.get(g)).map(|g| g.id.clone());
    Ok(json!({"page": page, "backgroundPage": background, "background": deco.background.name(), "border": deco.border.name()}))
}

fn set_background(s: &mut Session, v: &Value) -> CmdResult {
    let kind_name = p::req_str(v, "kind")?;
    let kind = BackgroundKind::from_name(kind_name)
        .ok_or_else(|| EngineError::Params(format!("unknown background `{}`; one of none, solid, gradient, pattern", short(kind_name))))?;
    let color = match opt_str(v, "color")? {
        None => None,
        Some(t) => Some(Color::parse(t).ok_or_else(|| EngineError::Params(format!("`color`: `{}` is not a #rrggbb colour", short(t))))?),
    };
    let index = page_index(s, v)?;
    let mut deco = decor::decoration(&s.doc, index);
    deco.background = kind;
    // A new colour replaces the old; none given keeps the one chosen before (or the theme's).
    if color.is_some() {
        deco.color = color;
    }
    if kind == BackgroundKind::None {
        deco.color = None;
    }
    decorate(s, index, deco)
}

fn set_border_title(s: &mut Session, v: &Value) -> CmdResult {
    let kind_name = p::req_str(v, "kind")?;
    let kind = BorderKind::from_name(kind_name)
        .ok_or_else(|| EngineError::Params(format!("unknown border `{}`; one of none, frame, banner, margin", short(kind_name))))?;
    let index = page_index(s, v)?;
    let mut deco = decor::decoration(&s.doc, index);
    deco.border = kind;
    decorate(s, index, deco)
}

fn set_connector_style(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "style")?;
    let route = match name {
        "rightAngle" => Route::Orthogonal,
        "straight" => Route::Straight,
        "curved" => Route::Curved,
        _ => return Err(EngineError::Params(format!("unknown connector style `{}`; one of rightAngle, straight, curved", short(name)))),
    };
    let index = page_index(s, v)?;
    let page = s.doc.pages.get_mut(index).ok_or_else(|| EngineError::Failed("no such page".into()))?;
    if page.connectors.is_empty() {
        return Err(EngineError::Params("the page has no connectors".into()));
    }
    let mut avoiding: Vec<String> = Vec::new();
    for c in &mut page.connectors {
        c.route = route;
        // Bends made for the old style would fight the new one.
        c.waypoints.clear();
        if route != Route::Orthogonal {
            // Avoiding shapes routes at right angles, so it ends with any other style.
            c.avoid = false;
        } else if c.avoid {
            avoiding.push(c.id.clone());
        }
    }
    if !avoiding.is_empty() {
        vizcraft_diagram::autoroute::reroute(page, Some(&avoiding));
    }
    let ids: Vec<String> = page.connectors.iter().map(|c| c.id.clone()).collect();
    let page_id = page.id.clone();
    Ok(json!({"page": page_id, "ids": ids, "style": name}))
}

/// Run after every command that changed the document, inside its undo step: shapes and
/// connectors that did not exist in `before` take the document theme (fields still at their base
/// value only, so a pasted or hand-styled item keeps its look), and generated decoration pages are
/// rebuilt.
pub fn after_change(doc: &mut Document, before: &Document) {
    if !doc.theme.id.is_empty() {
        let old: HashSet<&str> =
            before.pages.iter().flat_map(|p| p.shapes.iter().map(|s| s.id.as_str()).chain(p.connectors.iter().map(|c| c.id.as_str()))).collect();
        let t = doc.theme.clone();
        let none = DocTheme::default();
        let cbase = connector_base();
        let (c_was, c_now) = (theme::connector_style(&none, &cbase), theme::connector_style(&t, &cbase));
        for page in doc.pages.iter_mut().filter(|p| !decor::is_generated(p)) {
            for s in page.shapes.iter_mut().filter(|s| !old.contains(s.id.as_str())) {
                let base = shape_base(s);
                if theme::restyle(&mut s.style, &base, &theme::shape_style(&t, &base)) {
                    s.style.sanitize();
                }
            }
            for c in page.connectors.iter_mut().filter(|c| !old.contains(c.id.as_str())) {
                if theme::restyle(&mut c.style, &c_was, &c_now) {
                    c.style.sanitize();
                }
            }
        }
    }
    decor::refresh(doc);
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::theme::{DocTheme, quick_style, shape_style};
    use vizcraft_diagram::{Color, Route};

    use crate::Session;

    fn session() -> Session {
        Session::new(crate::sample::sample_document())
    }

    #[test]
    fn a_theme_restyles_unstyled_shapes_and_keeps_hand_styling() {
        let mut s = session();
        let ids: Vec<String> = s.doc.page().unwrap().shapes.iter().map(|x| x.id.clone()).collect();
        let (hand, plain) = (ids[0].clone(), ids[1].clone());
        s.run("shape.setStyle", &json!({"ids": [hand], "fill": "#123456"})).unwrap();
        let r = s.run("design.setTheme", &json!({"theme": "ember", "variant": 1})).unwrap();
        assert!(r["restyled"].as_u64().unwrap() > 0, "{r}");
        let page = s.doc.page().unwrap();
        let colors = s.doc.theme.colors().unwrap();
        assert_eq!(page.shape(&hand).unwrap().style.fill, Color::rgb(0x12, 0x34, 0x56), "hand fill stays");
        assert_eq!(page.shape(&hand).unwrap().style.line, colors.line, "unstyled fields follow the theme");
        assert_eq!(page.shape(&plain).unwrap().style.fill, colors.fill);
        assert_eq!(page.connectors[0].style.line, colors.connector);
        // A variant change moves themed fields again; one undo step per command.
        s.run("design.setVariant", &json!({"variant": 2})).unwrap();
        assert_eq!(s.doc.page().unwrap().shape(&plain).unwrap().style.fill, s.doc.theme.colors().unwrap().fill);
        assert_eq!(s.doc.page().unwrap().shape(&hand).unwrap().style.fill, Color::rgb(0x12, 0x34, 0x56));
        // No theme restores the masters' styles.
        let original = session();
        s.run("design.setTheme", &json!({"theme": "none"})).unwrap();
        let back = s.doc.page().unwrap().shape(&plain).unwrap();
        assert_eq!(back.style, original.doc.page().unwrap().shape(&plain).unwrap().style);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.theme, DocTheme::new("ember", 2));
    }

    #[test]
    fn backgrounds_and_borders_default_to_the_active_page() {
        let mut s = session();
        s.run("page.add", &json!({"name": "Second"})).unwrap();
        s.run("page.setActive", &json!({"page": 1})).unwrap();
        let second = s.doc.pages[1].id.clone();
        s.run("design.setBackground", &json!({"kind": "solid"})).unwrap();
        s.run("design.setBorderTitle", &json!({"kind": "frame"})).unwrap();
        assert!(vizcraft_diagram::decor::generated_page(&s.doc, &second).is_some(), "the page shown is decorated");
        let first = s.doc.pages[0].id.clone();
        assert!(vizcraft_diagram::decor::generated_page(&s.doc, &first).is_none(), "the first page is not");
    }

    #[test]
    fn new_shapes_and_connectors_take_the_theme() {
        let mut s = session();
        s.run("design.setTheme", &json!({"theme": "orchard"})).unwrap();
        let r = s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 10, "y": 10})).unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let st = s.doc.page().unwrap().shape(&id).unwrap().style.clone();
        let base = vizcraft_stencils::master("basic.rectangle").unwrap().style.clone();
        assert_eq!(st, shape_style(&s.doc.theme, &base));
        assert!(st.corner_radius > 0.0 && st.shadow.is_some());
        let r = s.run("connector.add", &json!({"from": {"x": 0, "y": 0}, "to": {"x": 50, "y": 50}})).unwrap();
        let cid = r["id"].as_str().unwrap().to_string();
        assert_eq!(s.doc.page().unwrap().connector(&cid).unwrap().style.line, s.doc.theme.colors().unwrap().connector);
    }

    #[test]
    fn theme_and_variant_params_are_checked() {
        let mut s = session();
        for bad in [
            json!({}),
            json!({"theme": "nope"}),
            json!({"theme": 5}),
            json!({"theme": "harbor", "variant": 4}),
            json!({"theme": "harbor", "variant": 0.5}),
        ] {
            assert!(s.run("design.setTheme", &bad).is_err(), "{bad}");
        }
        assert!(s.run("design.setVariant", &json!({"variant": 1})).is_err(), "no theme yet");
        s.run("design.setTheme", &json!({"theme": "slate"})).unwrap();
        assert!(s.run("design.setVariant", &json!({"variant": -1})).is_err());
        let r = s.run("design.themes", &json!({})).unwrap();
        assert_eq!(r["current"]["theme"], "slate");
        assert!(r["themes"].as_array().unwrap().len() >= 6);
        assert_eq!(r["quickStyles"].as_array().unwrap().len(), 12);
    }

    #[test]
    fn quick_styles_apply_to_the_selection_or_ids() {
        let mut s = session();
        s.run("design.setTheme", &json!({"theme": "meadow"})).unwrap();
        let id = s.doc.page().unwrap().shapes[0].id.clone();
        s.run("design.applyQuickStyle", &json!({"style": 5, "ids": [id]})).unwrap();
        let q = quick_style(&s.doc.theme, 5).unwrap();
        let st = &s.doc.page().unwrap().shape(&id).unwrap().style;
        assert_eq!((st.fill, st.line, st.font_color), (q.fill, q.line, q.text));
        s.run("design.applyQuickStyle", &json!({"style": 11, "ids": [id]})).unwrap();
        assert!(s.doc.page().unwrap().shape(&id).unwrap().style.gradient.is_some());
        for bad in [
            json!({"style": 12, "ids": [id]}),
            json!({"style": -1, "ids": [id]}),
            json!({"style": "x", "ids": [id]}),
            json!({"style": 1, "ids": ["nope"]}),
        ] {
            assert!(s.run("design.applyQuickStyle", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn connector_style_reroutes_every_connector_on_the_page() {
        let mut s = session();
        let n = s.doc.page().unwrap().connectors.len();
        let r = s.run("design.setConnectorStyle", &json!({"style": "curved"})).unwrap();
        assert_eq!(r["ids"].as_array().unwrap().len(), n);
        // Old bends are dropped; a curve the page routes may store a fresh route around shapes.
        let page = s.doc.page().unwrap();
        assert!(page.connectors.iter().all(|c| c.route == Route::Curved));
        assert!(page.connectors.iter().all(|c| c.waypoints.is_empty() || vizcraft_diagram::route::stored_route(page, c)));
        assert!(vizcraft_diagram::autoroute::crossings(page).is_empty());
        s.run("connector.reroute", &json!({"avoid": true})).unwrap();
        s.run("design.setConnectorStyle", &json!({"style": "straight", "page": 0})).unwrap();
        assert!(s.doc.page().unwrap().connectors.iter().all(|c| c.route == Route::Straight && !c.avoid));
        s.run("design.setConnectorStyle", &json!({"style": "rightAngle"})).unwrap();
        assert!(s.doc.page().unwrap().connectors.iter().all(|c| c.route == Route::Orthogonal));
        for bad in [json!({"style": "zigzag"}), json!({}), json!({"style": "curved", "page": "nope"}), json!({"style": "curved", "page": true})] {
            assert!(s.run("design.setConnectorStyle", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn backgrounds_and_borders_export_to_svg() {
        let mut s = session();
        s.doc.properties.title = "Order flow".into();
        s.run("design.setBackground", &json!({"kind": "gradient", "color": "#dde8f6"})).unwrap();
        let r = s.run("design.setBorderTitle", &json!({"kind": "frame"})).unwrap();
        let bg = r["backgroundPage"].as_str().unwrap().to_string();
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        assert!(svg.contains(&format!("<g class=\"background\" data-page=\"{bg}\">")), "{svg}");
        assert!(svg.contains("linearGradient"), "the gradient background is drawn");
        assert!(svg.contains("Order flow"), "the title block shows the document title");
        assert!(svg.contains(&s.doc.pages[0].name), "and the page name");
        // Changing the colour keeps the border; removing both removes the generated page.
        s.run("design.setBackground", &json!({"kind": "solid"})).unwrap();
        assert_eq!(s.doc.pages.iter().filter(|p| p.is_background).count(), 1);
        s.run("design.setBackground", &json!({"kind": "none"})).unwrap();
        s.run("design.setBorderTitle", &json!({"kind": "none"})).unwrap();
        assert!(s.doc.pages.iter().all(|p| !p.is_background));
        assert!(s.doc.pages[0].background.is_none());
        for bad in [json!({"kind": "plaid"}), json!({"kind": "solid", "color": "blue"}), json!({"kind": "solid", "page": 99})] {
            assert!(s.run("design.setBackground", &bad).is_err(), "{bad}");
        }
        assert!(s.run("design.setBorderTitle", &json!({"kind": "zz"})).is_err());
    }

    #[test]
    fn renaming_the_page_updates_the_title_block_in_the_same_step() {
        let mut s = session();
        s.run("design.setBorderTitle", &json!({"kind": "banner"})).unwrap();
        let name = "Renamed page";
        s.doc.pages[0].name = name.into();
        // Any document change rebuilds the decoration (here a no-op style change stands in for a rename command).
        let id = s.doc.page().unwrap().shapes[0].id.clone();
        s.run("shape.setStyle", &json!({"ids": [id], "bold": true})).unwrap();
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        assert!(svg.contains(name));
    }
}
