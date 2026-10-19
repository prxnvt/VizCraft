//! VizCraft code generation (model layer, below the engine).
//!
//! Generators turn the semantic model into files under the code-scaffold generation contract in
//! `docs/architecture.md`: generation is a command, configured by a spec in the repository, writes
//! only files a manifest says it owns, is deterministic, never overwrites a hand edit silently and
//! says honestly what each target cannot express.
//!
//! - [`spec`]: the generator spec ([`Spec`]), read from `.vizcraft-gen.toml` or JSON, or given inline.
//! - [`target`]: the [`Target`] trait, the construct-support matrix, the built-in registry and
//!   [`ModelView`], the model cut to the spec's scope in a stable order.
//! - [`targets`]: the built-in targets; `markdown-catalog` is the reference target.
//! - [`names`]: identifiers from the rename map, made from tags or names on first generation.
//! - [`manifest`]: `.vizcraft-gen/manifest.json` and the `last/` copies, the base of every merge.
//! - [`writer`]: [`writer::plan`] works out what a run would do; [`writer::apply`] does it.
//! - [`merge`]: the three-way merge and the unified diffs the preview shows.
//! - [`regions`]: protected `vizcraft:begin` and `vizcraft:end` blocks of single-file targets.
//!
//! Nothing here touches egui or the engine, and nothing panics on hostile specs, models or files:
//! sizes are capped, every path is checked and errors are [`CodegenError`]. Planning reads the
//! disk through [`writer::Disk`], so the web build plans against nothing on disk.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod manifest;
pub mod merge;
pub mod names;
pub mod regions;
pub mod spec;
pub mod target;
pub mod targets;
pub mod writer;

#[cfg(test)]
mod tests;

pub use manifest::Manifest;
pub use names::{Names, Rename};
pub use spec::{Scope, Spec};
pub use target::{Construct, ModelView, Options, Output, Support, Target, target, targets};
pub use writer::{Action, Disk, FsDisk, NoDisk, Plan, PlanFile, Report};

/// The generator's name and version, recorded in every manifest.
pub const GENERATOR: &str = concat!("vizcraft-codegen ", env!("CARGO_PKG_VERSION"));

/// The folder inside the output directory that holds the manifest and the last generated copies.
pub const STATE_DIR: &str = ".vizcraft-gen";

/// Most files one generation writes.
pub const MAX_FILES: usize = 2_000;

/// Largest file a target emits or a merge reads, in bytes.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;

/// Most bytes one generation emits across all files.
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// Longest relative path a generated file may have, in bytes.
pub const MAX_PATH: usize = 512;

/// Why a generation could not be planned or applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenError {
    /// The spec is unreadable, too large or names something that does not exist.
    #[error("spec: {0}")]
    Spec(String),
    /// A path is absolute, climbs out of the output directory, goes through a link or is too long.
    #[error("path: {0}")]
    Path(String),
    /// Reading or writing a file failed.
    #[error("{0}")]
    Io(String),
    /// The target could not generate (output too large, too many files).
    #[error("target: {0}")]
    Target(String),
}

/// A string cut to at most `max` bytes at a char boundary, for messages that quote input.
pub(crate) fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default()
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Text with `\r\n` and lone `\r` turned into `\n` and exactly one newline at the end (none for
/// empty text), so a target's output is the same on every platform.
pub fn normalize_newlines(text: &str) -> String {
    let mut s = text.replace("\r\n", "\n").replace('\r', "\n");
    while s.ends_with('\n') {
        s.pop();
    }
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// An issue raised by generation, with the rule id `codegen.<rule>`.
pub(crate) fn issue(rule: &str, severity: vizcraft_model::Severity, message: String, subjects: Vec<String>) -> vizcraft_model::Issue {
    vizcraft_model::Issue { rule: format!("codegen.{rule}"), severity, message, subjects, ..Default::default() }
}
