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
    if pack.schema_version > SCHEMA_VERSION {
        return Err(NotationError::TooNew { found: pack.schema_version, supported: SCHEMA_VERSION });
    }
    // A path glyph draws as a freeform shape; the general checks see it as one, and its path
    // data is read here.
    let path_glyphs: Vec<usize> = pack.glyphs.iter().enumerate().filter(|(_, g)| g.outline == PATH_OUTLINE).map(|(i, _)| i).collect();
    for &i in &path_glyphs {
        if let Some(g) = pack.glyphs.get_mut(i) {
            g.outline = FREEFORM.into();
        }
    }
    let mut problems = crate::validate::check(&pack);
    for &i in &path_glyphs {
        if let Some(g) = pack.glyphs.get_mut(i) {
            g.outline = PATH_OUTLINE.into();
        }
    }
    problems.extend(glyph_paths(&pack));
    problems.extend(variants(&pack));
    if !problems.is_empty() {
        return Err(NotationError::Invalid(problems));
    }
    Ok(pack)
}

/// The `outline` of a glyph drawn by its `path`.
pub const PATH_OUTLINE: &str = "path";

/// The shape kind a path glyph is drawn as.
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
