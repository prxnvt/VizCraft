//! Templates: starting points a pack offers, as data. A template names a viewpoint, a rendering
//! variant, a drawing sheet and a symbol grid module, a seed of commands that draws the starting
//! content, a layout to tidy it with, and whether a title block and a key belong on the page. The
//! engine's `template.*` commands turn one into a document or a page; this module holds the
//! schema and the checks the loader runs on it.
//!
//! The seed is a command list in the `script.run` form. A step marked `optional` is skipped when
//! its command is not registered, so a template can use a command a later build adds. A step can
//! name its result (`as`), and a later param string `${name.path}` stands for a value in it, so a
//! seed can connect shapes whose ids the engine hands out (see [`reference`]).
//!
//! Older packs wrote `templates` as a list of id strings; such entries are dropped when the pack
//! loads, so those packs still load.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::schema::Pack;

/// Most templates one pack may declare.
pub const MAX_TEMPLATES: usize = 128;

/// Most seed steps one template may have.
pub const MAX_SEED_STEPS: usize = 512;

/// Longest id, command id, viewpoint, variant, sheet or code form, in bytes.
pub const MAX_ID: usize = 128;

/// Longest label, in bytes.
pub const MAX_LABEL: usize = 128;

/// Longest description, in bytes.
pub const MAX_DESCRIPTION: usize = 2048;

/// Most audience values, keywords or code forms on one template.
pub const MAX_TAGS: usize = 32;

/// Longest audience value or keyword, in bytes.
pub const MAX_TAG: usize = 64;

/// The layout algorithms `layout.full` accepts (the engine's tests keep the two lists in step).
pub const ALGORITHMS: &[&str] = &["layered", "tree", "radial", "force"];

/// The flow directions the layout commands accept.
pub const DIRECTIONS: &[&str] = &["TB", "TD", "BT", "LR", "RL"];

/// The layout primitives `layout.primitive` accepts (the engine's tests keep the two lists in
/// step).
pub const PRIMITIVES: &[&str] = &["pipeline", "bands", "chain", "matrix", "nested", "hubSpoke", "rings", "timeline", "voter"];

/// The named symbol grid modules (`page.setModule`): none, the ISO 2.5 mm module and the 0.1 inch
/// module. A number is a module in page units (100 per inch).
pub const MODULES: &[&str] = &["none", "iso2_5mm", "inch0_1"];

/// The smallest and largest module given as a number of page units.
pub const MODULE_UNITS: (f64, f64) = (1.0, 500.0);

/// Most bytes of seed params a template may carry (all steps together).
pub const MAX_SEED_BYTES: usize = 256 * 1024;

/// A template: a named starting point of a pack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Template {
    /// `<pack>.<name>` (`flowchart.decision`).
    pub id: String,
    /// Name shown on the gallery card.
    pub label: String,
    /// One or two sentences on what it starts.
    pub description: String,
    /// Who it is for (`general`, `engineering`); empty: the pack's audience.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub audience: Vec<String>,
    /// Words a search matches besides the label.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// The page's viewpoint, a viewpoint of this pack; empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub viewpoint: String,
    /// Commands that draw the starting content, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seed: Vec<SeedStep>,
    /// How to lay out what the seed drew; none leaves it where the seed put it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<TemplateLayout>,
    /// Whether a key (legend) goes on the page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub key: bool,
    /// Whether a title block goes on the page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub title_block: bool,
    /// Text forms the diagram can also be written in (`mermaid.flowchart`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub code_forms: Vec<String>,
    /// The rendering variant it is drawn with by default, a variant of this pack; empty: the
    /// project's symbol family, else the default drawing.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub variant: String,
    /// The sheet preset, `<standard>.<size>[.<orientation>]` (`iso.a3.landscape`); empty for no
    /// sheet. The project's chosen sheet replaces it on new pages.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sheet: String,
    /// The symbol grid module: a name from [`MODULES`] or a number of page units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<Module>,
}

/// A symbol grid module: a named one or a size in page units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Module {
    /// A name from [`MODULES`].
    Named(String),
    /// A module in page units (100 per inch).
    Units(f64),
}

/// One seed step: a command and its params.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeedStep {
    /// The command id.
    pub command: String,
    /// Its params: an object, or null for none.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
    /// Skip the step when the command is not registered, instead of failing.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// A name for the step's result, so later steps and the layout roles can use it: a param
    /// string `${name.path}` (`${start.id}`, `${split.members.join}`) is replaced by the value at
    /// that path of the result. Empty: the result is not kept.
    #[serde(rename = "as", skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// Longest step result name, in bytes.
pub const MAX_NAME: usize = 64;

/// The reference a whole param string makes to an earlier step's result: `${name.path}` gives
/// `(name, path)`, the path possibly empty.
pub fn reference(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix("${")?.strip_suffix('}')?;
    let (name, path) = inner.split_once('.').unwrap_or((inner, ""));
    (!name.is_empty()).then_some((name, path))
}

/// Every reference made anywhere inside `v`, in document order.
fn references<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(t) => out.extend(reference(t).map(|(n, _)| n)),
        Value::Array(a) => a.iter().for_each(|x| references(x, out)),
        Value::Object(o) => o.values().for_each(|x| references(x, out)),
        _ => {}
    }
}

/// How a template lays out its seed: an algorithm (`layout.full`) or a primitive
/// (`layout.primitive`), not both.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TemplateLayout {
    /// A name from [`ALGORITHMS`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    /// A name from [`DIRECTIONS`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// A name from [`PRIMITIVES`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primitive: Option<String>,
    /// The primitive's roles, naming shapes by the ids the seed gave them.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub roles: Value,
    /// Space between shapes in one layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_spacing: Option<f64>,
    /// Space between layers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_spacing: Option<f64>,
}

/// Read `templates`: template objects, with old bare-string entries dropped. A malformed object
/// is still an error, so a typo in a template is reported.
pub fn entries<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Template>, D::Error> {
    let raw = Vec::<Value>::deserialize(d)?;
    let mut out = Vec::with_capacity(raw.len().min(MAX_TEMPLATES.saturating_add(1)));
    for v in raw {
        if v.is_string() {
            continue;
        }
        out.push(serde_json::from_value::<Template>(v).map_err(serde::de::Error::custom)?);
    }
    Ok(out)
}

/// The variant of `pack` that draws symbol family `family`: `<pack>.<family>`, when the pack
/// declares it. The family is the variant's local name by convention.
pub fn variant_for<'a>(pack: &'a Pack, family: &str) -> Option<&'a crate::schema::Variant> {
    if family.is_empty() {
        return None;
    }
    pack.variant(&format!("{}.{family}", pack.id))
}

/// The family a variant id draws: its local name (`electrical.jic` is `jic`).
pub fn family_of(variant: &str) -> &str {
    variant.split_once('.').map(|(_, f)| f).unwrap_or(variant)
}

/// Whether `sheet` reads as a sheet preset id: two or three dot-separated parts of lowercase
/// letters, digits and `_`, at most [`MAX_ID`] bytes.
pub fn is_sheet_id(sheet: &str) -> bool {
    let parts: Vec<&str> = sheet.split('.').collect();
    sheet.len() <= MAX_ID
        && (2..=3).contains(&parts.len())
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
}

/// The template with this id in `pack`.
pub fn find<'a>(pack: &'a Pack, id: &str) -> Option<&'a Template> {
    pack.templates.iter().find(|t| t.id == id)
}

/// The first 64 chars of `t`, for messages about hostile text.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// Problems with the pack's templates: counts, ids, lengths and every reference.
pub fn check(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    if pack.templates.len() > MAX_TEMPLATES {
        out.push(format!("at most {MAX_TEMPLATES} templates"));
    }
    let prefix = format!("{}.", pack.id);
    let mut seen = std::collections::HashSet::new();
    for t in pack.templates.iter().take(MAX_TEMPLATES) {
        let ctx = format!("template `{}`", short(&t.id));
        if !t.id.starts_with(&prefix) || t.id.len() <= prefix.len() || t.id.len() > MAX_ID {
            out.push(format!("{ctx}: the id must be `{prefix}<name>`"));
        }
        if !seen.insert(t.id.as_str()) {
            out.push(format!("{ctx}: declared twice"));
        }
        if t.label.trim().is_empty() {
            out.push(format!("{ctx}: needs a label"));
        }
        if t.label.len() > MAX_LABEL {
            out.push(format!("{ctx}: the label is longer than {MAX_LABEL} bytes"));
        }
        if t.description.len() > MAX_DESCRIPTION {
            out.push(format!("{ctx}: the description is longer than {MAX_DESCRIPTION} bytes"));
        }
        for (what, list, cap) in [("audience", &t.audience, MAX_TAG), ("keywords", &t.keywords, MAX_TAG), ("codeForms", &t.code_forms, MAX_ID)] {
            if list.len() > MAX_TAGS || list.iter().any(|x| x.is_empty() || x.len() > cap) {
                out.push(format!("{ctx}: `{what}` takes at most {MAX_TAGS} non-empty entries of at most {cap} bytes"));
            }
        }
        if !t.viewpoint.is_empty() && pack.viewpoint(&t.viewpoint).is_none() {
            out.push(format!("{ctx}: `{}` is not a viewpoint of this pack", short(&t.viewpoint)));
        }
        if !t.variant.is_empty() && pack.variant(&t.variant).is_none() {
            out.push(format!("{ctx}: `{}` is not a variant of this pack", short(&t.variant)));
        }
        if !t.sheet.is_empty() && !is_sheet_id(&t.sheet) {
            out.push(format!("{ctx}: `{}` is not a sheet preset id (`<standard>.<size>[.<orientation>]`)", short(&t.sheet)));
        }
        match &t.module {
            Some(Module::Named(m)) if !MODULES.contains(&m.as_str()) => {
                out.push(format!("{ctx}: `module` must be one of {} or a number of page units", MODULES.join(", ")));
            }
            Some(Module::Units(n)) if !n.is_finite() || *n < MODULE_UNITS.0 || *n > MODULE_UNITS.1 => {
                out.push(format!("{ctx}: a `module` in page units must be between {} and {}", MODULE_UNITS.0, MODULE_UNITS.1));
            }
            _ => {}
        }
        if t.title_block && t.sheet.is_empty() {
            out.push(format!("{ctx}: a title block needs a sheet; set `sheet` too"));
        }
        out.extend(seed(&ctx, t));
        if let Some(l) = &t.layout {
            out.extend(layout(&ctx, l));
        }
    }
    out
}

/// Problems with a template's seed.
fn seed(ctx: &str, t: &Template) -> Vec<String> {
    let mut out = Vec::new();
    if t.seed.len() > MAX_SEED_STEPS {
        out.push(format!("{ctx}: the seed has more than {MAX_SEED_STEPS} steps"));
    }
    let mut bytes = 0usize;
    let mut named: Vec<&str> = Vec::new();
    for (i, step) in t.seed.iter().take(MAX_SEED_STEPS).enumerate() {
        let mut used = Vec::new();
        references(&step.params, &mut used);
        for n in used.into_iter().filter(|n| !named.contains(n)) {
            out.push(format!("{ctx}: seed[{i}] uses `{}`, which no earlier step names with `as`", short(n)));
        }
        if !step.name.is_empty() {
            if step.name.len() > MAX_NAME || !step.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push(format!("{ctx}: seed[{i}] `as` must be at most {MAX_NAME} letters, digits or `_`"));
            }
            if step.optional {
                out.push(format!("{ctx}: seed[{i}] is optional, so it cannot name its result"));
            }
            named.push(&step.name);
        }
        let id = step.command.trim();
        if id.is_empty() || id.len() > MAX_ID || id != step.command {
            out.push(format!("{ctx}: seed[{i}] needs a command id of at most {MAX_ID} bytes"));
        }
        if !matches!(step.params, Value::Null | Value::Object(_)) {
            out.push(format!("{ctx}: seed[{i}] params must be an object"));
        }
        bytes = bytes.saturating_add(serde_json::to_string(&step.params).map(|s| s.len()).unwrap_or(0));
    }
    if bytes > MAX_SEED_BYTES {
        out.push(format!("{ctx}: the seed params are larger than {MAX_SEED_BYTES} bytes"));
    }
    if let Some(l) = &t.layout {
        let mut used = Vec::new();
        references(&l.roles, &mut used);
        for n in used.into_iter().filter(|n| !named.contains(n)) {
            out.push(format!("{ctx}: the layout roles use `{}`, which no seed step names with `as`", short(n)));
        }
    }
    out
}

/// Problems with a template's layout.
fn layout(ctx: &str, l: &TemplateLayout) -> Vec<String> {
    let mut out = Vec::new();
    let named = |v: &Option<String>, list: &[&str]| v.as_deref().is_none_or(|x| list.contains(&x));
    if !named(&l.algorithm, ALGORITHMS) {
        out.push(format!("{ctx}: `algorithm` must be one of {}", ALGORITHMS.join(", ")));
    }
    if !named(&l.direction, DIRECTIONS) {
        out.push(format!("{ctx}: `direction` must be one of {}", DIRECTIONS.join(", ")));
    }
    if !named(&l.primitive, PRIMITIVES) {
        out.push(format!("{ctx}: `primitive` must be one of {}", PRIMITIVES.join(", ")));
    }
    if l.algorithm.is_some() && l.primitive.is_some() {
        out.push(format!("{ctx}: give a layout `algorithm` or a `primitive`, not both"));
    }
    if !matches!(l.roles, Value::Null | Value::Object(_)) || (l.primitive.is_none() && !l.roles.is_null()) {
        out.push(format!("{ctx}: `roles` is an object, and only with a `primitive`"));
    }
    for (k, v) in [("nodeSpacing", l.node_spacing), ("layerSpacing", l.layer_spacing)] {
        if v.is_some_and(|n| !n.is_finite() || !(0.0..=10_000.0).contains(&n)) {
            out.push(format!("{ctx}: `{k}` must be between 0 and 10000"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::loader::{NotationError, load_json};

    /// A small pack with a viewpoint, two variants and one template using every field.
    fn pack() -> Value {
        json!({
            "id": "tt", "version": "1.0.0", "name": "Template test",
            "glyphs": [
                {"id": "tt.box", "label": "Box", "category": "T", "outline": "rectangle", "w": 100, "h": 60, "fill": "#ffffff", "line": "#000000", "elementType": "tt.Node"},
                {"id": "tt.round", "label": "Round", "category": "T", "outline": "ellipse", "w": 80, "h": 80, "fill": "#ffffff", "line": "#000000"}
            ],
            "elementTypes": [{"id": "tt.Node", "glyph": "tt.box"}],
            "viewpoints": [{"id": "tt.Main", "label": "Main", "types": ["*"]}],
            "variants": [
                {"id": "tt.iec", "label": "IEC"},
                {"id": "tt.jic", "label": "JIC", "glyphs": {"tt.Node": "tt.round"}}
            ],
            "templates": [{
                "id": "tt.start", "label": "Start", "description": "A start.", "audience": ["engineering"], "keywords": ["k"],
                "viewpoint": "tt.Main", "variant": "tt.iec", "sheet": "iso.a3.landscape", "module": "iso2_5mm",
                "key": true, "titleBlock": true, "codeForms": ["mermaid.flowchart"],
                "seed": [{"command": "shape.add", "params": {"stencil": "tt.box", "x": 0, "y": 0}}, {"command": "later.thing", "optional": true}],
                "layout": {"algorithm": "layered", "direction": "LR"}
            }]
        })
    }

    fn problems(edit: &dyn Fn(&mut Value)) -> String {
        let mut v = pack();
        edit(&mut v);
        match load_json(&v.to_string()) {
            Err(NotationError::Invalid(p)) => p.join("; "),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_full_template_loads_and_round_trips() {
        let p = load_json(&pack().to_string()).unwrap();
        let t = find(&p, "tt.start").unwrap();
        assert_eq!(t.module, Some(Module::Named("iso2_5mm".into())));
        assert!(t.key && t.title_block && t.seed[1].optional);
        assert_eq!(variant_for(&p, "jic").map(|v| v.id.as_str()), Some("tt.jic"));
        assert!(variant_for(&p, "ansi").is_none() && variant_for(&p, "").is_none());
        assert_eq!(family_of("tt.jic"), "jic");
        let again = load_json(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p);
        let mut v = pack();
        v["templates"][0]["module"] = json!(10.0);
        assert_eq!(find(&load_json(&v.to_string()).unwrap(), "tt.start").unwrap().module, Some(Module::Units(10.0)));
    }

    #[test]
    fn bad_templates_are_reported() {
        assert!(problems(&|v| v["templates"][0]["id"] = json!("other.start")).contains("must be `tt.<name>`"));
        assert!(problems(&|v| v["templates"][0]["viewpoint"] = json!("tt.Ghost")).contains("not a viewpoint"));
        assert!(problems(&|v| v["templates"][0]["variant"] = json!("tt.ansi")).contains("not a variant"));
        assert!(problems(&|v| v["templates"][0]["layout"]["algorithm"] = json!("spiral")).contains("`algorithm` must be"));
        assert!(problems(&|v| v["templates"][0]["layout"] = json!({"primitive": "spiral"})).contains("`primitive` must be"));
        assert!(problems(&|v| v["templates"][0]["layout"]["primitive"] = json!("pipeline")).contains("not both"));
        assert!(problems(&|v| v["templates"][0]["layout"]["direction"] = json!("up")).contains("`direction` must be"));
        assert!(problems(&|v| v["templates"][0]["label"] = json!(" ")).contains("needs a label"));
        assert!(problems(&|v| v["templates"][0]["module"] = json!("huge")).contains("`module` must be"));
        assert!(problems(&|v| v["templates"][0]["module"] = json!(0.01)).contains("page units must be"));
        assert!(problems(&|v| v["templates"][0]["sheet"] = json!("A3")).contains("sheet preset"));
        assert!(problems(&|v| v["templates"][0]["seed"][0]["command"] = json!("")).contains("seed[0] needs a command"));
        assert!(problems(&|v| v["templates"][0]["seed"][0]["params"] = json!([1])).contains("seed[0] params"));
        assert!(
            problems(&|v| v["templates"][0]["seed"] = Value::Array(vec![json!({"command": "edit.selectAll"}); MAX_SEED_STEPS + 1]))
                .contains("more than 512 steps")
        );
        assert!(
            problems(&|v| {
                let t = v["templates"][0].clone();
                v["templates"].as_array_mut().unwrap().push(t);
            })
            .contains("declared twice")
        );
        assert!(
            problems(&|v| v["templates"] = Value::Array((0..=MAX_TEMPLATES).map(|i| json!({"id": format!("tt.t{i}"), "label": "T"})).collect()))
                .contains("at most 128 templates")
        );
        assert!(problems(&|v| v["templates"][0]["keywords"] = json!(["x".repeat(65)])).contains("`keywords` takes"));
        assert!(problems(&|v| v["templates"][0]["seed"][0]["params"]["text"] = json!("${ghost.id}")).contains("no earlier step names"));
        assert!(
            problems(&|v| v["templates"][0]["layout"] = json!({"primitive": "hubSpoke", "roles": {"hub": "${ghost.id}"}}))
                .contains("layout roles use `ghost`")
        );
        assert!(problems(&|v| v["templates"][0]["seed"][1]["as"] = json!("later")).contains("cannot name its result"));
        assert!(problems(&|v| v["templates"][0]["seed"][0]["as"] = json!("a b")).contains("`as` must be"));
    }

    #[test]
    fn bare_string_entries_are_dropped_and_a_malformed_object_is_an_error() {
        let mut v = pack();
        v["templates"].as_array_mut().unwrap().insert(0, json!("tt.oldStyle"));
        let p = load_json(&v.to_string()).unwrap();
        assert_eq!(p.templates.len(), 1);
        v["templates"] = json!(["a", "b"]);
        assert!(load_json(&v.to_string()).unwrap().templates.is_empty());
        v["templates"] = json!([{"id": "tt.x", "label": 5}]);
        assert!(load_json(&v.to_string()).is_err());
    }

    #[test]
    fn references_read_whole_strings_only() {
        assert_eq!(reference("${start.id}"), Some(("start", "id")));
        assert_eq!(reference("${ci.members.split}"), Some(("ci", "members.split")));
        assert_eq!(reference("${ci}"), Some(("ci", "")));
        for no in ["start.id", "${}", "x ${a.id}", "${a.id} x", "$start"] {
            assert_eq!(reference(no), None, "{no}");
        }
        let mut v = pack();
        v["templates"][0]["seed"][0]["as"] = json!("box");
        v["templates"][0]["layout"] = json!({"primitive": "hubSpoke", "roles": {"hub": "${box.id}"}});
        assert!(load_json(&v.to_string()).is_ok());
    }

    #[test]
    fn sheet_ids() {
        for ok in ["iso.a3.landscape", "ansi.b", "asme.d.portrait", "iso.a0_plus.landscape"] {
            assert!(is_sheet_id(ok), "{ok}");
        }
        for bad in ["", "iso", "ISO.A3", "iso..a3", "iso.a3.landscape.x", "iso.a 3"] {
            assert!(!is_sheet_id(bad), "{bad}");
        }
    }
}
