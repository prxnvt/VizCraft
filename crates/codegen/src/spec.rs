//! The generator spec: which target, with which options, into which folder, over which part of
//! the model.
//!
//! A spec lives in the repository next to the code it generates (`.vizcraft-gen.toml`, or any
//! `.toml` or `.json` file) or is given inline to a command:
//!
//! ```toml
//! name = "catalogue"
//! target = "markdown-catalog"
//! output = "docs/model"          # relative to the spec file; never absolute, never `..`
//!
//! [options]
//! file = "catalog.md"
//!
//! [scope]
//! packs = ["flowchart"]          # empty lists mean everything
//! types = []
//! elements = []
//! ```
//!
//! Reading never panics: the file is capped at [`MAX_SPEC_BYTES`], every list and string is
//! capped, and the output path is checked by [`crate::writer::relative_path`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{CodegenError, clip};

/// Largest spec file read, in bytes.
pub const MAX_SPEC_BYTES: u64 = 64 * 1024;

/// Most entries in one scope list.
pub const MAX_SCOPE: usize = 10_000;

/// Longest scope entry, spec name or target id, in bytes.
pub const MAX_ID: usize = 256;

/// Largest options table, in bytes of JSON.
pub const MAX_OPTIONS_BYTES: usize = 64 * 1024;

/// The part of the model a generator covers. An element is in scope when it passes every
/// non-empty list; a relationship when both its ends are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scope {
    /// Pack ids (`flowchart`): elements whose type comes from one of them.
    pub packs: Vec<String>,
    /// Pack-qualified type ids (`flowchart.Decision`).
    pub types: Vec<String>,
    /// Element ids.
    pub elements: Vec<String>,
}

impl Scope {
    /// Whether every list is empty, so the whole model is in scope.
    pub fn is_all(&self) -> bool {
        self.packs.is_empty() && self.types.is_empty() && self.elements.is_empty()
    }
}

/// One generator: target, options, output folder and scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spec {
    /// A name for people (the catalogue's title, the run log).
    pub name: String,
    /// The target id (`markdown-catalog`).
    pub target: String,
    /// Target options; each target documents its own.
    pub options: Map<String, Value>,
    /// The output folder, relative to the spec file (or, inline, to the document's folder).
    pub output: String,
    /// The part of the model covered.
    pub scope: Scope,
}

impl Spec {
    /// Parse TOML text.
    pub fn from_toml(text: &str) -> Result<Spec, CodegenError> {
        let spec: Spec = toml::from_str(text).map_err(|e| CodegenError::Spec(clip(&e.to_string(), 400).to_string()))?;
        spec.checked()
    }

    /// Parse JSON text.
    pub fn from_json(text: &str) -> Result<Spec, CodegenError> {
        let spec: Spec = serde_json::from_str(text).map_err(|e| CodegenError::Spec(clip(&e.to_string(), 400).to_string()))?;
        spec.checked()
    }

    /// A spec from a JSON value (inline in command params).
    pub fn from_value(v: &Value) -> Result<Spec, CodegenError> {
        let spec: Spec = Spec::deserialize(v).map_err(|e| CodegenError::Spec(clip(&e.to_string(), 400).to_string()))?;
        spec.checked()
    }

    /// Read a spec file: JSON when the name ends in `.json`, TOML otherwise. Returns the spec and
    /// the folder it is in, which its `output` is relative to.
    pub fn read(path: &Path) -> Result<(Spec, PathBuf), CodegenError> {
        let shown = clip(&path.to_string_lossy(), 300).to_string();
        let meta = std::fs::metadata(path).map_err(|e| CodegenError::Spec(format!("{shown}: {e}")))?;
        if !meta.is_file() {
            return Err(CodegenError::Spec(format!("{shown}: not a file")));
        }
        if meta.len() > MAX_SPEC_BYTES {
            return Err(CodegenError::Spec(format!("{shown}: larger than {MAX_SPEC_BYTES} bytes")));
        }
        let text = std::fs::read_to_string(path).map_err(|e| CodegenError::Spec(format!("{shown}: {e}")))?;
        let is_json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
        let spec = if is_json { Spec::from_json(&text) } else { Spec::from_toml(&text) }.map_err(|e| CodegenError::Spec(format!("{shown}: {e}")))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok((spec, base))
    }

    /// The output folder: `output` checked and joined to `base`.
    pub fn output_dir(&self, base: &Path) -> Result<PathBuf, CodegenError> {
        if self.output.trim() == "." {
            return Ok(base.to_path_buf());
        }
        let rel = crate::writer::relative_path(&self.output)?;
        Ok(base.join(rel))
    }

    /// SHA-256 of the spec's canonical JSON (options are a sorted map), recorded in the manifest
    /// so a changed spec shows as a change.
    pub fn checksum(&self) -> String {
        crate::sha256_hex(serde_json::to_string(self).unwrap_or_default().as_bytes())
    }

    /// The spec when every field is within its cap; otherwise why not.
    pub fn checked(self) -> Result<Spec, CodegenError> {
        if self.name.len() > MAX_ID {
            return Err(CodegenError::Spec(format!("`name` is longer than {MAX_ID} bytes")));
        }
        if self.target.len() > MAX_ID {
            return Err(CodegenError::Spec(format!("`target` is longer than {MAX_ID} bytes")));
        }
        if self.output.len() > crate::MAX_PATH {
            return Err(CodegenError::Spec(format!("`output` is longer than {} bytes", crate::MAX_PATH)));
        }
        for (key, list) in [("packs", &self.scope.packs), ("types", &self.scope.types), ("elements", &self.scope.elements)] {
            if list.len() > MAX_SCOPE {
                return Err(CodegenError::Spec(format!("`scope.{key}` has more than {MAX_SCOPE} entries")));
            }
            if let Some(long) = list.iter().find(|s| s.len() > MAX_ID) {
                return Err(CodegenError::Spec(format!("`scope.{key}` entry `{}` is longer than {MAX_ID} bytes", clip(long, 40))));
            }
        }
        if serde_json::to_string(&self.options).map(|s| s.len()).unwrap_or(usize::MAX) > MAX_OPTIONS_BYTES {
            return Err(CodegenError::Spec(format!("`options` is larger than {MAX_OPTIONS_BYTES} bytes")));
        }
        Ok(self)
    }
}
