//! Reading packs from JSON or TOML text.

use crate::schema::{Pack, SCHEMA_VERSION};

/// Largest pack text accepted, in bytes.
pub const MAX_PACK: usize = 4 * 1024 * 1024;

/// Why a pack could not be loaded.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum NotationError {
    /// The text is larger than [`MAX_PACK`].
    #[error("pack is too large ({len} bytes; the limit is {max})")]
    TooLarge {
        /// Size of the text in bytes.
        len: usize,
        /// The limit in bytes.
        max: usize,
    },
    /// The text is not valid JSON or TOML, or not shaped like a pack.
    #[error("not a notation pack: {0}")]
    Parse(String),
    /// A pack written for a newer schema than this build reads.
    #[error("pack schema {found} is newer than this build reads ({supported})")]
    TooNew {
        /// The pack's schema version.
        found: u32,
        /// The newest this build reads.
        supported: u32,
    },
    /// The pack parsed but is inconsistent; each entry says what is wrong.
    #[error("invalid pack: {}", .0.join("; "))]
    Invalid(Vec<String>),
}

/// Load a pack from JSON text, then check it ([`crate::validate::check`]).
pub fn load_json(text: &str) -> Result<Pack, NotationError> {
    if text.len() > MAX_PACK {
        return Err(NotationError::TooLarge { len: text.len(), max: MAX_PACK });
    }
    let pack: Pack = serde_json::from_str(text).map_err(|e| NotationError::Parse(e.to_string()))?;
    finish(pack)
}

/// Load a pack from TOML text, then check it ([`crate::validate::check`]).
pub fn load_toml(text: &str) -> Result<Pack, NotationError> {
    if text.len() > MAX_PACK {
        return Err(NotationError::TooLarge { len: text.len(), max: MAX_PACK });
    }
    let pack: Pack = toml::from_str(text).map_err(|e| NotationError::Parse(e.to_string()))?;
    finish(pack)
}

/// Load a pack from a file's bytes, choosing JSON or TOML by the extension of `name` (`.json` or
/// `.toml`, any case), then check it. The bytes must be UTF-8.
pub fn load(name: &str, bytes: &[u8]) -> Result<Pack, NotationError> {
    if bytes.len() > MAX_PACK {
        return Err(NotationError::TooLarge { len: bytes.len(), max: MAX_PACK });
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let text = std::str::from_utf8(bytes).map_err(|e| NotationError::Parse(format!("not UTF-8 text: {e}")))?;
    match ext.as_str() {
        "json" => load_json(text),
        "toml" => load_toml(text),
        _ => Err(NotationError::Parse("a pack file name ends in .json or .toml".into())),
    }
}

fn finish(mut pack: Pack) -> Result<Pack, NotationError> {
    let sections = crate::sections::flatten(&mut pack);
    if pack.schema_version > SCHEMA_VERSION {
        return Err(NotationError::TooNew { found: pack.schema_version, supported: SCHEMA_VERSION });
    }
    // Path and builder glyphs draw as freeform shapes; the general checks see them as such, and
    // their own fields are read here.
    let drawn: Vec<(usize, &'static str)> = pack
        .glyphs
        .iter()
        .enumerate()
        .filter_map(|(i, g)| [PATH_OUTLINE, BUILDER_OUTLINE].into_iter().find(|o| g.outline == *o).map(|o| (i, o)))
        .collect();
    for &(i, _) in &drawn {
        if let Some(g) = pack.glyphs.get_mut(i) {
            g.outline = FREEFORM.into();
        }
    }
    let mut problems = sections;
    problems.extend(crate::validate::check(&pack));
    for &(i, outline) in &drawn {
        if let Some(g) = pack.glyphs.get_mut(i) {
            g.outline = outline.into();
        }
    }
    problems.extend(glyph_paths(&pack));
    problems.extend(builder_glyphs(&pack));
    problems.extend(variants(&pack));
    problems.extend(look_fields(&pack));
    problems.extend(sheet_fields(&pack));
    problems.extend(crate::template::check(&pack));
    problems.extend(crate::nesting::check_pack(&pack));
    if !problems.is_empty() {
        return Err(NotationError::Invalid(problems));
    }
    Ok(pack)
}

/// Load a pack shipped with VizCraft (see [`crate::catalog::BUILTIN`]): [`load_json`], and every
/// path and builder glyph must say where its drawing came from ([`provenance`] finds none
/// missing). A pack read from disk only gets these as warnings, so packs written before the rule
/// still load.
pub fn load_shipped(name: &str, text: &str) -> Result<Pack, NotationError> {
    let pack = load_json(text)?;
    let missing = provenance(&pack);
    if !missing.is_empty() {
        return Err(NotationError::Invalid(missing.into_iter().map(|m| format!("{name}: {m}")).collect()));
    }
    Ok(pack)
}

/// Path and builder glyphs of `pack` that do not say where their drawing came from (an empty
/// `source`). For a shipped pack these are errors, for a pack from disk warnings.
pub fn provenance(pack: &Pack) -> Vec<String> {
    pack.glyphs
        .iter()
        .filter(|g| (g.outline == PATH_OUTLINE || g.outline == BUILDER_OUTLINE) && g.source.trim().is_empty())
        .map(|g| format!("glyph `{}`: a drawn glyph needs a `source` (`own design` or `drawn from ...`)", short(&g.id)))
        .collect()
}

/// The `outline` of a glyph drawn by its `path`.
pub const PATH_OUTLINE: &str = "path";

/// The `outline` of a glyph drawn by a Rust builder ([`crate::schema::Glyph::builder`]).
pub const BUILDER_OUTLINE: &str = "builder";

/// Most builder parameters one glyph may set.
pub const MAX_GLYPH_PARAMS: usize = 64;

/// Longest builder parameter key, in bytes.
pub const MAX_PARAM_KEY: usize = 64;

/// Longest text value of a builder parameter, in bytes.
pub const MAX_PARAM_TEXT: usize = 1024;

/// Longest `source` of a glyph, in bytes.
pub const MAX_SOURCE: usize = 256;

/// The shape kind a path or builder glyph is drawn as.
const FREEFORM: &str = "freeform";

/// Most variants one pack may declare.
pub const MAX_VARIANTS: usize = 64;

/// The first 64 chars of `t`, for messages about hostile text.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// Problems with path glyphs: path data that does not read, and path data on a glyph that
/// does not draw with it.
/// Most document properties one pack declares.
pub const MAX_DOCUMENT_PROPERTIES: usize = 64;

/// Most columns a table query names.
pub const MAX_QUERY_COLUMNS: usize = 16;

/// The drawing-sheet fields: document properties (keys, limits), compartment table queries
/// (types, columns, widths) and nothing to check on a relationship kind beyond its name, which
/// serde already read.
fn sheet_fields(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    if pack.document_properties.len() > MAX_DOCUMENT_PROPERTIES {
        out.push(format!("at most {MAX_DOCUMENT_PROPERTIES} document properties"));
    }
    let mut seen = std::collections::HashSet::new();
    for p in pack.document_properties.iter().take(MAX_DOCUMENT_PROPERTIES) {
        let ctx = format!("document property `{}`", short(&p.key));
        if !vizcraft_diagram::DocProperties::is_field_key(&p.key) {
            out.push(format!("{ctx}: the key must be letters, digits and `_`, at most {} bytes", vizcraft_diagram::DocProperties::MAX_KEY));
        }
        if !seen.insert(p.key.as_str()) {
            out.push(format!("{ctx}: declared twice"));
        }
        if p.max_chars == Some(0) {
            out.push(format!("{ctx}: `maxChars` must be at least 1"));
        }
        if p.kind == crate::schema::PropKind::Enum && p.values.is_empty() {
            out.push(format!("{ctx}: an enum needs values"));
        }
    }
    for t in &pack.element_types {
        for c in &t.compartments {
            let Some(q) = &c.query else { continue };
            let ctx = format!("element type `{}`, compartment `{}`", short(&t.id), short(&c.id));
            if c.kind != vizcraft_diagram::CompartmentKind::List {
                out.push(format!("{ctx}: a table query needs a list compartment"));
            }
            if q.columns.is_empty() || q.columns.len() > MAX_QUERY_COLUMNS {
                out.push(format!("{ctx}: a table query needs 1 to {MAX_QUERY_COLUMNS} columns"));
            }
            if q.headers.len() > q.columns.len() || q.widths.len() > q.columns.len() {
                out.push(format!("{ctx}: more headers or widths than columns"));
            }
            if q.widths.iter().any(|w| !w.is_finite() || *w <= 0.0) {
                out.push(format!("{ctx}: column widths must be positive numbers"));
            }
            if q.types.is_empty() {
                out.push(format!("{ctx}: a table query lists at least one element type"));
            }
            if !q.sort_by.is_empty() && !q.columns.contains(&q.sort_by) {
                out.push(format!("{ctx}: `sortBy` must be one of the columns"));
            }
        }
    }
    out
}

fn glyph_paths(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    for g in &pack.glyphs {
        let ctx = format!("glyph `{}`", short(&g.id));
        if g.outline == PATH_OUTLINE {
            if let Err(e) = crate::glyph_path::glyph_outline(&g.path, &g.detail) {
                out.push(format!("{ctx}: {e}"));
            }
        } else if !g.path.is_empty() || !g.detail.is_empty() {
            out.push(format!("{ctx}: `path` and `detail` are read only with outline `path`"));
        }
    }
    out
}

/// Problems with builder glyphs: a builder or params on a glyph that is not drawn by one, a
/// builder glyph with no builder, too many params or overlong keys and values, and an overlong
/// source. Whether the builder exists is checked above this crate (the engine knows the
/// builders), when a pack is loaded.
fn builder_glyphs(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    for g in &pack.glyphs {
        let ctx = format!("glyph `{}`", short(&g.id));
        if g.outline == BUILDER_OUTLINE {
            if g.builder.trim().is_empty() {
                out.push(format!("{ctx}: outline `builder` needs a `builder` name"));
            } else if g.builder.len() > MAX_PARAM_KEY {
                out.push(format!("{ctx}: the builder name is longer than {MAX_PARAM_KEY} bytes"));
            }
            if g.params.len() > MAX_GLYPH_PARAMS {
                out.push(format!("{ctx}: at most {MAX_GLYPH_PARAMS} params"));
            }
            for (k, v) in g.params.iter().take(MAX_GLYPH_PARAMS + 1) {
                if k.is_empty() || k.len() > MAX_PARAM_KEY {
                    out.push(format!("{ctx}: param key `{}` must be 1 to {MAX_PARAM_KEY} bytes", short(k)));
                }
                if v.as_str().is_some_and(|t| t.len() > MAX_PARAM_TEXT) {
                    out.push(format!("{ctx}: param `{}` is longer than {MAX_PARAM_TEXT} bytes", short(k)));
                }
            }
        } else if !g.builder.is_empty() || !g.params.is_empty() {
            out.push(format!("{ctx}: `builder` and `params` are read only with outline `builder`"));
        }
        if g.source.len() > MAX_SOURCE {
            out.push(format!("{ctx}: `source` is longer than {MAX_SOURCE} bytes"));
        }
    }
    out
}

/// Problems with the rendering variants: ids, and every glyph and connector type they map to.
fn variants(pack: &Pack) -> Vec<String> {
    let mut out = Vec::new();
    if pack.variants.len() > MAX_VARIANTS {
        out.push(format!("at most {MAX_VARIANTS} variants"));
    }
    let prefix = format!("{}.", pack.id);
    let mut seen = std::collections::HashSet::new();
    for v in pack.variants.iter().take(MAX_VARIANTS) {
        let ctx = format!("variant `{}`", short(&v.id));
        if !v.id.starts_with(&prefix) || v.id.len() <= prefix.len() || v.id.len() > 128 {
            out.push(format!("{ctx}: the id must be `{prefix}<name>`"));
        }
        if !seen.insert(v.id.as_str()) {
            out.push(format!("{ctx}: declared twice"));
        }
        if v.aliases.len() > MAX_ALIASES {
            out.push(format!("{ctx}: at most {MAX_ALIASES} aliases"));
        }
        for a in v.aliases.iter().take(MAX_ALIASES) {
            if !a.starts_with(&prefix) || a.len() <= prefix.len() || a.len() > 128 {
                out.push(format!("{ctx}: alias `{}` must be `{prefix}<name>`", short(a)));
            }
            if pack.variants.iter().any(|w| w.id == *a) || pack.variants.iter().filter(|w| w.aliases.contains(a)).count() > 1 {
                out.push(format!("{ctx}: alias `{}` is already a variant id or another variant's alias", short(a)));
            }
        }
        for (t, g) in &v.glyphs {
            if pack.element_type(t).is_none() {
                out.push(format!("{ctx}: `{}` is not an element type of this pack", short(t)));
            }
            if pack.glyph(g).is_none() {
                out.push(format!("{ctx}: `{}` is not a glyph of this pack", short(g)));
            }
        }
        for (r, k) in &v.connectors {
            if pack.relationship_type(r).is_none() {
                out.push(format!("{ctx}: `{}` is not a relationship type of this pack", short(r)));
            }
            if pack.connector_type(k).is_none() {
                out.push(format!("{ctx}: `{}` is not a connector type of this pack", short(k)));
            }
        }
        if v.satellites.len() > MAX_SATELLITE_TYPES {
            out.push(format!("{ctx}: at most {MAX_SATELLITE_TYPES} satellite types"));
        }
        for t in v.satellites.iter().take(MAX_SATELLITE_TYPES) {
            if pack.element_type(t).is_none() {
                out.push(format!("{ctx}: satellite `{}` is not an element type of this pack", short(t)));
            }
        }
        for (t, decls) in &v.compartments {
            if pack.element_type(t).is_none() {
                out.push(format!("{ctx}: compartments for `{}`, which is not an element type of this pack", short(t)));
            }
            if decls.len() > vizcraft_diagram::compartment::MAX_COMPARTMENTS {
                out.push(format!("{ctx}: `{}` has more compartments than a shape holds", short(t)));
            }
            for d in decls {
                if !vizcraft_diagram::compartment::is_valid_id(&d.id) {
                    out.push(format!("{ctx}: compartment id `{}` is not usable", short(&d.id)));
                }
            }
        }
    }
    out
}

/// Most aliases one variant lists.
pub const MAX_ALIASES: usize = 8;

/// Most satellite types one variant lists.
pub const MAX_SATELLITE_TYPES: usize = 8;

/// Most rules on one end of a connector type's `ends`.
pub const MAX_END_RULES: usize = 16;

/// Most values one end rule maps.
pub const MAX_END_VALUES: usize = 32;

/// Longest end label an end rule sets, in bytes.
pub const MAX_END_LABEL: usize = 32;

/// Whether `key` is a usable property key: ASCII letters, digits and `_`.
fn is_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The ER look fields: glyph `compound`, connector type `labelFrame` and `ends`, and the
/// `owned`, `filter` and `values` of compartment table queries.
fn look_fields(pack: &Pack) -> Vec<String> {
    use vizcraft_diagram::connector_look::LabelFrame;
    use vizcraft_diagram::{Arrow, Compound, Dash};
    let mut out = Vec::new();
    for g in &pack.glyphs {
        if !g.compound.is_empty() && Compound::from_name(&g.compound).is_none() {
            out.push(format!("glyph `{}`: compound `{}` is not `single` or `double`", short(&g.id), short(&g.compound)));
        }
    }
    for g in &pack.glyphs {
        if !g.text_place.is_empty() && vizcraft_diagram::shape_fields::TextPlace::from_name(&g.text_place).is_none() {
            out.push(format!("glyph `{}`: textPlace `{}` is not `inside` or `below`", short(&g.id), short(&g.text_place)));
        }
    }
    for k in &pack.connector_types {
        let ctx = format!("connector type `{}`", short(&k.id));
        if !k.label_frame.is_empty() && LabelFrame::from_name(&k.label_frame).is_none() {
            out.push(format!("{ctx}: label frame `{}` is not `box`, `diamond` or `doubleDiamond`", short(&k.label_frame)));
        }
        let Some(ends) = &k.ends else { continue };
        for rules in [&ends.start, &ends.end] {
            if rules.len() > MAX_END_RULES {
                out.push(format!("{ctx}: at most {MAX_END_RULES} end rules per end"));
            }
            for r in rules.iter().take(MAX_END_RULES) {
                if !is_key(&r.property) || r.when.as_ref().is_some_and(|w| !is_key(&w.property)) {
                    out.push(format!("{ctx}: end rule property `{}` must be letters, digits and `_`", short(&r.property)));
                }
                if r.values.len() > MAX_END_VALUES || r.when.as_ref().is_some_and(|w| w.any.len() > MAX_END_VALUES) {
                    out.push(format!("{ctx}: at most {MAX_END_VALUES} values per end rule"));
                }
                for look in r.values.values().take(MAX_END_VALUES) {
                    if look.arrow.as_deref().is_some_and(|a| Arrow::from_name(a).is_none()) {
                        out.push(format!("{ctx}: unknown arrow `{}`", short(look.arrow.as_deref().unwrap_or_default())));
                    }
                    if look.dash.as_deref().is_some_and(|d| Dash::from_name(d).is_none()) {
                        out.push(format!("{ctx}: unknown dash `{}`", short(look.dash.as_deref().unwrap_or_default())));
                    }
                    if look.label.as_ref().is_some_and(|l| l.len() > MAX_END_LABEL) {
                        out.push(format!("{ctx}: end labels are at most {MAX_END_LABEL} bytes"));
                    }
                }
            }
        }
    }
    let decls =
        pack.element_types.iter().flat_map(|t| t.compartments.iter()).chain(pack.variants.iter().flat_map(|v| v.compartments.values().flatten()));
    for d in decls {
        let Some(q) = &d.query else { continue };
        let ctx = format!("compartment `{}`", short(&d.id));
        if q.filter.as_ref().is_some_and(|f| !is_key(&f.property)) {
            out.push(format!("{ctx}: the filter property must be letters, digits and `_`"));
        }
        if q.values.len() > MAX_QUERY_COLUMNS || q.values.values().any(|m| m.len() > MAX_END_VALUES) {
            out.push(format!("{ctx}: too many shown values"));
        }
        for col in q.values.keys() {
            if !q.columns.contains(col) {
                out.push(format!("{ctx}: shown values for `{}`, which is not a column", short(col)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML_PACK: &str = r##"
schemaVersion = 1
id = "tiny"
version = "0.1.0"
name = "Tiny"
audience = ["general"]

[[glyphs]]
id = "tiny.box"
label = "Box"
category = "Shapes"
outline = "rectangle"
w = 100
h = 60
fill = "#ffffff"
line = "#000000"
elementType = "tiny.Box"

[[elementTypes]]
id = "tiny.Box"
label = "Box"
glyph = "tiny.box"

[[relationshipTypes]]
id = "tiny.Link"
label = "Link"

[[connections]]
source = ["*"]
relationship = "tiny.Link"
target = ["*"]

[[rules]]
id = "tiny.illegal"
severity = "error"
mode = "live"
check = { kind = "illegalRelationship" }
"##;

    #[test]
    fn toml_and_json_packs_load() {
        let p = load("tiny.TOML", TOML_PACK.as_bytes()).unwrap();
        assert_eq!((p.id.as_str(), p.glyphs.len(), p.element_types.len()), ("tiny", 1, 1));
        assert_eq!(p.relationship_types[0].end_arrow, "triangle");
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(load("tiny.json", json.as_bytes()).unwrap(), p);
        assert_eq!(load_toml(TOML_PACK).unwrap(), p);
        for (name, text) in crate::catalog::BUILTIN {
            assert!(load(name, text.as_bytes()).is_ok(), "{name}");
        }
    }

    #[test]
    fn text_place_accepts_inside_and_below_and_rejects_anything_else() {
        for ok in ["inside", "below"] {
            let text = TOML_PACK.replace("elementType = \"tiny.Box\"", &format!("elementType = \"tiny.Box\"\ntextPlace = \"{ok}\""));
            assert!(load("tiny.toml", text.as_bytes()).is_ok(), "{ok}");
        }
        let text = TOML_PACK.replace("elementType = \"tiny.Box\"", "elementType = \"tiny.Box\"\ntextPlace = \"left\"");
        let err = load("tiny.toml", text.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("textPlace"), "{err}");
    }

    #[test]
    fn oversized_text_is_refused_before_parsing() {
        let big = " ".repeat(MAX_PACK + 1);
        assert!(matches!(load_json(&big), Err(NotationError::TooLarge { .. })));
        assert!(matches!(load_toml(&big), Err(NotationError::TooLarge { .. })));
        assert!(matches!(load("big.json", big.as_bytes()), Err(NotationError::TooLarge { .. })));
    }

    #[test]
    fn too_new_schema_is_refused() {
        let text = TOML_PACK.replace("schemaVersion = 1", "schemaVersion = 99");
        assert_eq!(load_toml(&text), Err(NotationError::TooNew { found: 99, supported: SCHEMA_VERSION }));
    }

    /// [`TOML_PACK`] with a path glyph, a second relationship drawing and two variants.
    fn variant_pack() -> serde_json::Value {
        let mut v = serde_json::to_value(load_toml(TOML_PACK).unwrap()).unwrap();
        v["glyphs"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "tiny.ring", "label": "Ring", "category": "Shapes", "outline": "path", "w": 80, "h": 80,
            "fill": "#ffffff", "line": "#000000",
            "path": "M0 0 L1 0 L1 1 L0 1 Z M0.3 0.3 L0.7 0.3 L0.7 0.7 L0.3 0.7 Z", "detail": "M0 0.5 L0.3 0.5"
        }));
        v["connectorTypes"] = serde_json::json!([
            {"id": "tiny.link", "label": "Link", "relationshipType": "tiny.Link"},
            {"id": "tiny.dashed", "label": "Dashed", "dash": "dashed"}
        ]);
        v["variants"] = serde_json::json!([
            {"id": "tiny.rings", "label": "Rings", "glyphs": {"tiny.Box": "tiny.ring"}, "connectors": {"tiny.Link": "tiny.dashed"}}
        ]);
        v
    }

    #[test]
    fn path_glyphs_and_variants_load_and_bad_ones_are_reported() {
        let good = variant_pack();
        let p = load_json(&good.to_string()).unwrap();
        assert_eq!(p.glyph("tiny.ring").map(|g| g.outline.as_str()), Some("path"), "the outline reads back as path");
        assert_eq!(p.variant("tiny.rings").map(|v| v.glyphs.len()), Some(1));
        let problems = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            edit(&mut v);
            match load_json(&v.to_string()) {
                Err(NotationError::Invalid(p)) => p.join("; "),
                other => panic!("{other:?}"),
            }
        };
        assert!(problems(&|v| v["glyphs"][1]["path"] = "M0 0 L9 9".into()).contains("bad number"));
        assert!(
            problems(&|v| {
                v["glyphs"][1]["path"] = "".into();
                v["glyphs"][1]["detail"] = "".into();
            })
            .contains("draws nothing"),
            "a path glyph needs path data"
        );
        assert!(problems(&|v| v["glyphs"][0]["path"] = "M0 0 L1 1".into()).contains("only with outline `path`"));
        assert!(problems(&|v| v["glyphs"][1]["detail"] = "M0 0 L".into()).contains("needs more numbers"));
        assert!(problems(&|v| v["variants"][0]["id"] = "other.rings".into()).contains("must be `tiny.<name>`"));
        assert!(problems(&|v| v["variants"][0]["glyphs"]["tiny.Ghost"] = "tiny.box".into()).contains("not an element type"));
        assert!(problems(&|v| v["variants"][0]["glyphs"]["tiny.Box"] = "tiny.ghost".into()).contains("not a glyph"));
        assert!(problems(&|v| v["variants"][0]["connectors"]["tiny.Link"] = "tiny.ghost".into()).contains("not a connector type"));
        assert!(problems(&|v| v["variants"][0]["connectors"]["tiny.Ghost"] = "tiny.link".into()).contains("not a relationship type"));
        assert!(
            problems(&|v| {
                let first = v["variants"][0].clone();
                v["variants"].as_array_mut().unwrap().push(first);
            })
            .contains("declared twice")
        );
    }

    /// [`TOML_PACK`] with a builder glyph that has a source.
    fn builder_pack() -> serde_json::Value {
        let mut v = serde_json::to_value(load_toml(TOML_PACK).unwrap()).unwrap();
        v["glyphs"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "tiny.contact", "label": "Contact", "category": "Shapes", "outline": "builder", "w": 40, "h": 40,
            "fill": "#ffffff", "line": "#000000", "builder": "contact", "params": {"standard": "jic", "poles": 2},
            "source": "own design"
        }));
        v
    }

    #[test]
    fn builder_glyphs_load_and_bad_ones_are_reported() {
        let good = builder_pack();
        let p = load_json(&good.to_string()).unwrap();
        let g = p.glyph("tiny.contact").unwrap();
        assert_eq!((g.outline.as_str(), g.builder.as_str(), g.params.len(), g.source.as_str()), ("builder", "contact", 2, "own design"));
        // The new fields are left out of a glyph that does not use them, so old packs write as before.
        let plain = serde_json::to_value(p.glyph("tiny.box").unwrap()).unwrap();
        assert!(plain.get("builder").is_none() && plain.get("params").is_none() && plain.get("source").is_none(), "{plain}");
        let problems = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            edit(&mut v);
            match load_json(&v.to_string()) {
                Err(NotationError::Invalid(p)) => p.join("; "),
                other => panic!("{other:?}"),
            }
        };
        assert!(problems(&|v| v["glyphs"][0]["builder"] = "contact".into()).contains("only with outline `builder`"));
        assert!(problems(&|v| v["glyphs"][0]["params"] = serde_json::json!({"poles": 2})).contains("only with outline `builder`"));
        assert!(problems(&|v| v["glyphs"][1]["builder"] = "".into()).contains("needs a `builder` name"));
        assert!(
            problems(&|v| {
                let many: serde_json::Map<String, serde_json::Value> = (0..65).map(|i| (format!("k{i}"), serde_json::json!(i))).collect();
                v["glyphs"][1]["params"] = serde_json::Value::Object(many);
            })
            .contains("at most 64 params")
        );
        assert!(problems(&|v| v["glyphs"][1]["params"]["k".repeat(65)] = 1.into()).contains("must be 1 to 64 bytes"));
        assert!(problems(&|v| v["glyphs"][1]["params"]["note"] = "x".repeat(1025).into()).contains("longer than 1024 bytes"));
        assert!(problems(&|v| v["glyphs"][1]["source"] = "x".repeat(257).into()).contains("`source` is longer"));
    }

    #[test]
    fn shipped_packs_need_a_source_on_every_drawn_glyph_and_disk_packs_only_warn() {
        let mut v = builder_pack();
        v["glyphs"][1]["source"] = "".into();
        let text = v.to_string();
        let p = load_json(&text).expect("a disk pack without a source still loads");
        assert_eq!(provenance(&p).len(), 1, "with a warning");
        match load_shipped("tiny.json", &text) {
            Err(NotationError::Invalid(p)) => {
                assert!(p.join(";").contains("tiny.json: glyph `tiny.contact`: a drawn glyph needs a `source`"), "{p:?}")
            }
            other => panic!("{other:?}"),
        }
        // A path glyph needs one too.
        let mut paths = variant_pack();
        assert!(load_shipped("tiny.json", &paths.to_string()).is_err());
        paths["glyphs"][1]["source"] = "own design".into();
        assert!(load_shipped("tiny.json", &paths.to_string()).is_ok());
        assert!(load_shipped("tiny.json", &builder_pack().to_string()).is_ok());
        for (name, text) in crate::catalog::BUILTIN {
            assert!(load_shipped(name, text).is_ok(), "{name}");
        }
    }

    #[test]
    fn junk_is_an_error_not_a_panic() {
        for junk in ["", "null", "[1,2]", r#"{"glyphs": 3}"#, "{{{{", "\u{0}"] {
            assert!(load_json(junk).is_err(), "{junk:?}");
        }
        assert!(matches!(load("p.toml", b"= = ="), Err(NotationError::Parse(_))));
        assert!(matches!(load("p.json", &[0xff, 0xfe, 0x00]), Err(NotationError::Parse(_))));
        assert!(matches!(load("p.yaml", b"{}"), Err(NotationError::Parse(_))));
        assert!(matches!(load("noext", b"{}"), Err(NotationError::Parse(_))));
        // Parses, but has no id: reported as invalid with the reason.
        match load_json("{}") {
            Err(NotationError::Invalid(p)) => assert!(p.iter().any(|m| m.contains("no id")), "{p:?}"),
            other => panic!("{other:?}"),
        }
        let bad = TOML_PACK.replace("illegalRelationship", "fromTheFuture");
        assert!(matches!(load_toml(&bad), Err(NotationError::Invalid(p)) if p.iter().any(|m| m.contains("unknown check"))));
    }
}
