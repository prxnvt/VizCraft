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

fn finish(pack: Pack) -> Result<Pack, NotationError> {
    if pack.schema_version > SCHEMA_VERSION {
        return Err(NotationError::TooNew { found: pack.schema_version, supported: SCHEMA_VERSION });
    }
    let problems = crate::validate::check(&pack);
    if !problems.is_empty() {
        return Err(NotationError::Invalid(problems));
    }
    Ok(pack)
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
