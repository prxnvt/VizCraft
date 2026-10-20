//! Document types and examples, as data.
//!
//! A **document type** (a pack's `templates`) is an empty document set up for a kind of work: a
//! Class Diagram, an Entity Relationship Diagram, a Network Diagram. It draws no content. What it
//! sets up is what differs between kinds of work:
//!
//! - the page's viewpoint, which decides the shapes and connectors the palette offers first, and
//!   the other viewpoints the person may pick instead (`viewpoints`);
//! - the rendering variant (notation or symbol standard) it starts with;
//! - the drawing sheet, title block and symbol grid module, where the discipline uses them;
//! - the compliance rules that run (`rulesets`, and `liveChecks` to start with checking on or off);
//! - the layout Re-Layout offers first, whose direction is where Tab and Enter grow a chain;
//! - whether the page keeps a key, and the text forms the diagram can also be written in.
//!
//! Every one of those is only where the document starts: the person can pick another viewpoint,
//! variant or sheet at creation (`template.new` params) and change any of them afterwards.
//!
//! An **example** (a pack's `examples`) has the same fields plus a `seed`: commands that draw a
//! filled-in sample, and a `layout` that tidies it. Examples open from Help > Examples and are
//! the test suite's fixtures. An id is unique across both lists of a pack.
//!
//! What a pack author writes for a new document type (wave 4 packs follow this):
//!
//! ```json
//! {
//!   "id": "electrical.schematic",
//!   "label": "Schematic",
//!   "description": "A circuit diagram on a sheet with a title block and the IEC symbols.",
//!   "discipline": "engineering",
//!   "audience": ["engineering"],
//!   "keywords": ["circuit", "wiring"],
//!   "viewpoint": "electrical.Schematic",
//!   "viewpoints": [
//!     {"viewpoint": "electrical.Schematic"},
//!     {"viewpoint": "electrical.Ladder", "rulesets": ["electrical.ladder"]}
//!   ],
//!   "variant": "electrical.iec",
//!   "sheet": "iso.a3.landscape",
//!   "titleBlock": true,
//!   "module": "iso2_5mm",
//!   "rulesets": ["electrical.iec60617"],
//!   "liveChecks": true,
//!   "layout": {"algorithm": "layered", "direction": "LR"},
//!   "key": false,
//!   "codeForms": []
//! }
//! ```
//!
//! Only `id` (`<pack>.<name>`) and `label` are required. `discipline` groups the New page
//! (`software`, `data`, `infrastructure`, `general`, `engineering`, or another lowercase word,
//! listed after those); empty is `general`. `viewpoints` lists the choices offered at creation,
//! the default `viewpoint` among them; a choice's `rulesets` replace the type's own when it is
//! picked (an Entity Relationship Diagram holds a conceptual model to fewer rules than a physical
//! one). `rulesets` name rulesets of this pack, or of another pack by their qualified id;
//! empty leaves the rules following the notation (the pack's first ruleset). `liveChecks`
//! absent leaves checking as the notation has it. A document type has no `seed`; a `layout`
//! on one never runs at creation (there is nothing to lay out), it is the notation's preferred
//! layout.
//!
//! The seed is a command list in the `script.run` form. A step marked `optional` is skipped when
//! its command is not registered, so an example can use a command a later build adds. A step can
//! name its result (`as`), and a later param string `${name.path}` stands for a value in it, so a
//! seed can connect shapes whose ids the engine hands out (see [`reference`]).
//!
//! The engine's `template.*` commands make a document or a page of a type and `example.*` open
//! the examples; this module holds the schema and the checks the loader runs on both lists.
//! Older packs wrote `templates` as a list of id strings; such entries are dropped when the pack
//! loads, so those packs still load.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::schema::Pack;

/// Most templates one pack may declare.
pub const MAX_TEMPLATES: usize = 128;

/// The disciplines the New page groups document types under, in its order; any other lowercase
/// word is a discipline of its own, listed after these.
pub const DISCIPLINES: &[&str] = &["general", "software", "data", "infrastructure", "engineering"];

/// The discipline an entry without one belongs to.
pub const DEFAULT_DISCIPLINE: &str = "general";

/// Most viewpoint choices or rulesets on one document type.
pub const MAX_CHOICES: usize = 32;

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
pub const PRIMITIVES: &[&str] = &["pipeline", "bands", "chain", "matrix", "nested", "hubSpoke", "rings", "timeline", "voter", "sequence", "er"];

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
    /// The New page group (see [`DISCIPLINES`]); empty is [`DEFAULT_DISCIPLINE`].
    #[serde(skip_serializing_if = "String::is_empty")]
    pub discipline: String,
    /// The page's viewpoint, a viewpoint of this pack; empty for none. With `viewpoints`, the
    /// default choice.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub viewpoint: String,
    /// The viewpoints the person may pick instead at creation (the default among them); empty
    /// offers only `viewpoint`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub viewpoints: Vec<ViewpointChoice>,
    /// The rulesets the document is held to (the conformance profile); empty leaves the rules
    /// following the notation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rulesets: Vec<String>,
    /// Whether the rules check as the person draws (the profile is on); absent leaves it as the
    /// notation has it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_checks: Option<bool>,
    /// Commands that draw an example's content, in order; a document type has none.
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

/// One viewpoint a document type offers at creation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ViewpointChoice {
    /// A viewpoint of the pack.
    pub viewpoint: String,
    /// The rulesets that replace the type's own when this viewpoint is picked; empty keeps them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rulesets: Vec<String>,
}

impl Template {
    /// The discipline the New page groups it under.
    pub fn discipline(&self) -> &str {
        if self.discipline.is_empty() { DEFAULT_DISCIPLINE } else { &self.discipline }
    }

    /// Whether `viewpoint` may be picked for it: its own viewpoint or one of its choices.
    pub fn offers_viewpoint(&self, viewpoint: &str) -> bool {
        self.viewpoint == viewpoint || self.viewpoints.iter().any(|c| c.viewpoint == viewpoint)
    }

    /// The viewpoints offered at creation, the default first when it is not listed.
    pub fn viewpoint_choices(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        if !self.viewpoint.is_empty() && !self.viewpoints.iter().any(|c| c.viewpoint == self.viewpoint) {
            out.push(&self.viewpoint);
        }
        out.extend(self.viewpoints.iter().map(|c| c.viewpoint.as_str()));
        out
    }

    /// The rulesets a document of this type on `viewpoint` is held to: the choice's own when it
    /// lists some, else the type's.
    pub fn rulesets_for(&self, viewpoint: &str) -> &[String] {
        match self.viewpoints.iter().find(|c| c.viewpoint == viewpoint) {
            Some(c) if !c.rulesets.is_empty() => &c.rulesets,
            _ => &self.rulesets,
        }
    }
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

/// The document type with this id in `pack`.
pub fn find<'a>(pack: &'a Pack, id: &str) -> Option<&'a Template> {
    pack.templates.iter().find(|t| t.id == id)
}

/// The example with this id in `pack`.
pub fn find_example<'a>(pack: &'a Pack, id: &str) -> Option<&'a Template> {
    pack.examples.iter().find(|t| t.id == id)
}

/// The first 64 chars of `t`, for messages about hostile text.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// Problems with the pack's document types and examples: counts, ids (unique across both
/// lists), lengths and every reference; a document type with a seed.
pub fn check(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (list, what, cap_name) in [(&pack.templates, "document type", "templates"), (&pack.examples, "example", "examples")] {
        if list.len() > MAX_TEMPLATES {
            out.push(format!("at most {MAX_TEMPLATES} {cap_name}"));
        }
        for t in list.iter().take(MAX_TEMPLATES) {
            let ctx = format!("{what} `{}`", short(&t.id));
            if !seen.insert(t.id.as_str()) {
                out.push(format!("{ctx}: declared twice (ids are unique across `templates` and `examples`)"));
            }
            out.extend(entry(pack, &ctx, t));
            if what == "document type" && !t.seed.is_empty() {
                out.push(format!("{ctx}: a document type draws no content; move the seed to an example in `examples`"));
            }
        }
    }
    out
}

/// Problems with one document type or example.
fn entry(pack: &Pack, ctx: &str, t: &Template) -> Vec<String> {
    let mut out = Vec::new();
    let prefix = format!("{}.", pack.id);
    if !t.id.starts_with(&prefix) || t.id.len() <= prefix.len() || t.id.len() > MAX_ID {
        out.push(format!("{ctx}: the id must be `{prefix}<name>`"));
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
    if !t.discipline.is_empty() && (t.discipline.len() > MAX_TAG || !t.discipline.chars().all(|c| c.is_ascii_lowercase() || c == '_')) {
        out.push(format!("{ctx}: `discipline` is one lowercase word of at most {MAX_TAG} bytes (such as {})", DISCIPLINES.join(", ")));
    }
    if !t.viewpoint.is_empty() && pack.viewpoint(&t.viewpoint).is_none() {
        out.push(format!("{ctx}: `{}` is not a viewpoint of this pack", short(&t.viewpoint)));
    }
    if t.viewpoints.len() > MAX_CHOICES {
        out.push(format!("{ctx}: `viewpoints` offers at most {MAX_CHOICES} choices"));
    }
    if !t.viewpoints.is_empty() && t.viewpoint.is_empty() {
        out.push(format!("{ctx}: `viewpoints` needs a default `viewpoint` among them"));
    }
    let mut offered = std::collections::HashSet::new();
    for c in t.viewpoints.iter().take(MAX_CHOICES) {
        if pack.viewpoint(&c.viewpoint).is_none() {
            out.push(format!("{ctx}: `viewpoints` offers `{}`, which is not a viewpoint of this pack", short(&c.viewpoint)));
        }
        if !offered.insert(c.viewpoint.as_str()) {
            out.push(format!("{ctx}: `viewpoints` offers `{}` twice", short(&c.viewpoint)));
        }
        out.extend(rulesets(pack, ctx, &c.rulesets));
    }
    out.extend(rulesets(pack, ctx, &t.rulesets));
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
    out.extend(seed(ctx, t));
    if let Some(l) = &t.layout {
        out.extend(layout(ctx, l));
    }
    out
}

/// Problems with a list of ruleset ids: too many, malformed, or naming a ruleset this pack does
/// not declare (a ruleset of another pack is checked when a document is made).
fn rulesets(pack: &Pack, ctx: &str, list: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if list.len() > MAX_CHOICES {
        out.push(format!("{ctx}: at most {MAX_CHOICES} `rulesets`"));
    }
    let prefix = format!("{}.", pack.id);
    for r in list.iter().take(MAX_CHOICES) {
        if r.is_empty() || r.len() > MAX_ID || !r.contains('.') {
            out.push(format!("{ctx}: `{}` is not a ruleset id (`<pack>.<name>`)", short(r)));
        } else if r.starts_with(&prefix) && !pack.rulesets.iter().any(|x| x.id == *r) {
            out.push(format!("{ctx}: `{}` is not a ruleset of this pack", short(r)));
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
            "viewpoints": [{"id": "tt.Main", "label": "Main", "types": ["*"]}, {"id": "tt.Other", "label": "Other", "types": ["*"]}],
            "variants": [
                {"id": "tt.iec", "label": "IEC"},
                {"id": "tt.jic", "label": "JIC", "glyphs": {"tt.Node": "tt.round"}}
            ],
            "rules": [{"id": "tt.illegal", "check": {"kind": "illegalRelationship"}}],
            "rulesets": [{"id": "tt.core", "label": "Core", "rules": ["tt.illegal"]}, {"id": "tt.strict", "label": "Strict", "rules": ["tt.illegal"]}],
            "templates": [{
                "id": "tt.doc", "label": "Doc", "discipline": "engineering", "viewpoint": "tt.Main",
                "viewpoints": [{"viewpoint": "tt.Main"}, {"viewpoint": "tt.Other", "rulesets": ["tt.strict"]}],
                "rulesets": ["tt.core"], "liveChecks": false, "sheet": "iso.a3.landscape", "titleBlock": true,
                "layout": {"algorithm": "layered", "direction": "LR"}
            }],
            "examples": [{
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
        let t = find_example(&p, "tt.start").unwrap();
        assert!(find(&p, "tt.start").is_none() && find_example(&p, "tt.doc").is_none());
        assert_eq!(t.module, Some(Module::Named("iso2_5mm".into())));
        assert!(t.key && t.title_block && t.seed[1].optional);
        assert_eq!(variant_for(&p, "jic").map(|v| v.id.as_str()), Some("tt.jic"));
        assert!(variant_for(&p, "ansi").is_none() && variant_for(&p, "").is_none());
        assert_eq!(family_of("tt.jic"), "jic");
        let again = load_json(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p);
        let mut v = pack();
        v["examples"][0]["module"] = json!(10.0);
        assert_eq!(find_example(&load_json(&v.to_string()).unwrap(), "tt.start").unwrap().module, Some(Module::Units(10.0)));
    }

    #[test]
    fn bad_templates_are_reported() {
        assert!(problems(&|v| v["examples"][0]["id"] = json!("other.start")).contains("must be `tt.<name>`"));
        assert!(problems(&|v| v["examples"][0]["viewpoint"] = json!("tt.Ghost")).contains("not a viewpoint"));
        assert!(problems(&|v| v["examples"][0]["variant"] = json!("tt.ansi")).contains("not a variant"));
        assert!(problems(&|v| v["examples"][0]["layout"]["algorithm"] = json!("spiral")).contains("`algorithm` must be"));
        assert!(problems(&|v| v["examples"][0]["layout"] = json!({"primitive": "spiral"})).contains("`primitive` must be"));
        assert!(problems(&|v| v["examples"][0]["layout"]["primitive"] = json!("pipeline")).contains("not both"));
        assert!(problems(&|v| v["examples"][0]["layout"]["direction"] = json!("up")).contains("`direction` must be"));
        assert!(problems(&|v| v["examples"][0]["label"] = json!(" ")).contains("needs a label"));
        assert!(problems(&|v| v["examples"][0]["module"] = json!("huge")).contains("`module` must be"));
        assert!(problems(&|v| v["examples"][0]["module"] = json!(0.01)).contains("page units must be"));
        assert!(problems(&|v| v["examples"][0]["sheet"] = json!("A3")).contains("sheet preset"));
        assert!(problems(&|v| v["examples"][0]["seed"][0]["command"] = json!("")).contains("seed[0] needs a command"));
        assert!(problems(&|v| v["examples"][0]["seed"][0]["params"] = json!([1])).contains("seed[0] params"));
        assert!(
            problems(&|v| v["examples"][0]["seed"] = Value::Array(vec![json!({"command": "edit.selectAll"}); MAX_SEED_STEPS + 1]))
                .contains("more than 512 steps")
        );
        assert!(
            problems(&|v| {
                let t = v["examples"][0].clone();
                v["examples"].as_array_mut().unwrap().push(t);
            })
            .contains("declared twice")
        );
        assert!(
            problems(&|v| v["examples"] = Value::Array((0..=MAX_TEMPLATES).map(|i| json!({"id": format!("tt.t{i}"), "label": "T"})).collect()))
                .contains("at most 128 examples")
        );
        assert!(problems(&|v| v["examples"][0]["keywords"] = json!(["x".repeat(65)])).contains("`keywords` takes"));
        assert!(problems(&|v| v["examples"][0]["seed"][0]["params"]["text"] = json!("${ghost.id}")).contains("no earlier step names"));
        assert!(
            problems(&|v| v["examples"][0]["layout"] = json!({"primitive": "hubSpoke", "roles": {"hub": "${ghost.id}"}}))
                .contains("layout roles use `ghost`")
        );
        assert!(problems(&|v| v["examples"][0]["seed"][1]["as"] = json!("later")).contains("cannot name its result"));
        assert!(problems(&|v| v["examples"][0]["seed"][0]["as"] = json!("a b")).contains("`as` must be"));
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
        v["examples"][0]["seed"][0]["as"] = json!("box");
        v["examples"][0]["layout"] = json!({"primitive": "hubSpoke", "roles": {"hub": "${box.id}"}});
        assert!(load_json(&v.to_string()).is_ok());
    }

    #[test]
    fn a_document_type_reads_its_choices_and_rules() {
        let p = load_json(&pack().to_string()).unwrap();
        let t = find(&p, "tt.doc").unwrap();
        assert_eq!(t.discipline(), "engineering");
        assert_eq!(t.viewpoint_choices(), ["tt.Main", "tt.Other"]);
        assert!(t.offers_viewpoint("tt.Other") && !t.offers_viewpoint("tt.Ghost"));
        assert_eq!(t.rulesets_for("tt.Main"), ["tt.core"]);
        assert_eq!(t.rulesets_for("tt.Other"), ["tt.strict"]);
        assert_eq!(t.live_checks, Some(false));
        assert_eq!(find_example(&p, "tt.start").unwrap().discipline(), DEFAULT_DISCIPLINE);
    }

    #[test]
    fn bad_document_types_are_reported() {
        let ty = |edit: &dyn Fn(&mut Value)| problems(&|v| edit(&mut v["templates"][0]));
        assert!(ty(&|t| t["seed"] = json!([{"command": "shape.add"}])).contains("draws no content"));
        assert!(ty(&|t| t["viewpoints"][1]["viewpoint"] = json!("tt.Ghost")).contains("offers `tt.Ghost`"));
        assert!(ty(&|t| t["viewpoints"][1]["viewpoint"] = json!("tt.Main")).contains("twice"));
        assert!(ty(&|t| t["viewpoint"] = json!("")).contains("needs a default"));
        assert!(ty(&|t| t["rulesets"] = json!(["tt.ghost"])).contains("not a ruleset of this pack"));
        assert!(ty(&|t| t["viewpoints"][1]["rulesets"] = json!(["nodot"])).contains("not a ruleset id"));
        assert!(ty(&|t| t["discipline"] = json!("Two Words")).contains("`discipline`"));
        assert!(ty(&|t| t["id"] = json!("tt.start")).contains("unique across"));
        // A ruleset of another pack is checked when a document is made.
        let mut v = pack();
        v["templates"][0]["rulesets"] = json!(["drawing.tidy"]);
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
