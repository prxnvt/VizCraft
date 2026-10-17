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
