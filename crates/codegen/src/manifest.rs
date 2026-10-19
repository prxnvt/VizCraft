//! The manifest: which files a generator owns and what it last wrote into them.
//!
//! It lives at `.vizcraft-gen/manifest.json` inside the output folder, next to
//! `.vizcraft-gen/last/<path>`, a copy of each file as last generated. A file's checksum is the
//! SHA-256 of that generated text, so a file on disk whose checksum differs was edited by hand
//! and is merged three ways (last generated, on disk, new) rather than overwritten.
//!
//! The JSON is written with sorted keys and rows sorted by path, so an unchanged generation
//! writes the same manifest byte for byte.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{CodegenError, STATE_DIR, clip};

/// Version of the manifest layout this build writes and reads.
pub const MANIFEST_VERSION: u32 = 1;

/// Largest manifest read, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

/// One owned file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileEntry {
    /// Path relative to the output folder, `/`-separated.
    pub path: String,
    /// SHA-256 (lowercase hex) of the text last generated for it.
    pub sha256: String,
}

/// What a generator owns in an output folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manifest {
    /// Layout version ([`MANIFEST_VERSION`]).
    pub version: u32,
    /// The generator that wrote it ([`crate::GENERATOR`]).
    pub generator: String,
    /// The target id.
    pub target: String,
    /// SHA-256 of the spec it was generated with.
    pub spec_checksum: String,
    /// Owned files, sorted by path.
    pub files: Vec<FileEntry>,
    /// Element id to identifier.
    pub renames: BTreeMap<String, String>,
}

impl Manifest {
    /// Path of the manifest relative to the output folder.
    pub fn path() -> String {
        format!("{STATE_DIR}/manifest.json")
    }

    /// Path of the last generated copy of `file`, relative to the output folder.
    pub fn last_path(file: &str) -> String {
        format!("{STATE_DIR}/last/{file}")
    }

    /// Parse a manifest. Rows with unsafe paths are dropped, so a hand-edited manifest can never
    /// make the writer touch a file outside the output folder.
    pub fn parse(text: &str) -> Result<Manifest, CodegenError> {
        if text.len() > MAX_MANIFEST_BYTES {
            return Err(CodegenError::Spec(format!("manifest is larger than {MAX_MANIFEST_BYTES} bytes")));
        }
        let mut m: Manifest = serde_json::from_str(text).map_err(|e| CodegenError::Spec(format!("manifest: {}", clip(&e.to_string(), 300))))?;
        m.files.retain(|f| crate::writer::relative_path(&f.path).is_ok() && !f.path.starts_with(STATE_DIR));
        m.files.truncate(crate::MAX_FILES);
        m.files.sort_by(|a, b| a.path.cmp(&b.path));
        m.files.dedup_by(|a, b| a.path == b.path);
        Ok(m)
    }

    /// The manifest as written: pretty JSON with a final newline.
    pub fn to_text(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).unwrap_or_default();
        s.push('\n');
        s
    }

    /// The row for `path`.
    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path == path)
    }
}
