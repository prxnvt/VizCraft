//! VizCraft semantic model (model layer, below the diagram and the engine).
//!
//! A [`Model`] holds typed [`Element`]s, typed [`Relationship`]s between them and a containment
//! tree (each element has at most one owner). Diagrams are views of the model: a diagram shape
//! names the element it draws (`Shape.element` in `vizcraft-diagram`) and a page carries the
//! [`View`] it shows, so geometry never lives here. One element may appear in many views.
//!
//! - [`identity`]: the project-wide registry of tags: identifier formats, auto-numbering,
//!   uniqueness and paired references.
//! - [`issue`]: the structured [`Issue`] that rules and the registry report.
//!
//! Types are pack-qualified strings (`flowchart.Decision`, `pid.CentrifugalPump`): the prefix
//! before the first `.` is the notation pack that defines them (`vizcraft-notation`). This crate
//! knows nothing about packs beyond that convention.
//!
//! Every type is plain data with serde. Fields are camelCase on disk, every field has a default
//! and wrong-typed values fall back to defaults, so files from newer versions load. Nothing here
//! panics on hostile input: [`Model::sanitize`] clamps counts and lengths and repairs references.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod identity;
pub mod issue;
pub mod pattern;
pub mod query;
mod repair;
mod serde_util;
#[cfg(test)]
mod tests;
mod types;

pub use identity::{Duplicate, IdFormat, IdScope, Registry, Unmatched};
pub use issue::{Issue, Severity};
pub use pattern::PatternInstance;
pub use types::{Element, Model, ModelError, PackRef, PropValue, Relationship, Removed, View};

/// The model schema version this build writes, and the newest it reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Most elements a model holds; a file with more keeps the first ones.
pub const MAX_ELEMENTS: usize = 100_000;

/// Most relationships a model holds; a file with more keeps the first ones.
pub const MAX_RELATIONSHIPS: usize = 200_000;

/// Most properties one element or relationship holds.
pub const MAX_PROPERTIES: usize = 256;

/// Longest id of an element or relationship, in bytes (the same limit as diagram item ids).
pub const MAX_ID: usize = 64;

/// Longest type id or property key, in bytes.
pub const MAX_KEY: usize = 128;

/// Longest tag, in bytes.
pub const MAX_TAG: usize = 256;

/// Longest name or text property value, in bytes.
pub const MAX_TEXT: usize = 64 * 1024;

/// Deepest containment chain walked; deeper chains are cut at load time.
pub const MAX_DEPTH: usize = 256;

/// The pack prefix of a qualified type id: `flowchart` for `flowchart.Decision`, empty when the
/// id has no `.`.
pub fn pack_of(type_id: &str) -> &str {
    type_id.split_once('.').map_or("", |(p, _)| p)
}

/// The local part of a qualified type id: `Decision` for `flowchart.Decision`, the whole id when
/// it has no `.`.
pub fn local_of(type_id: &str) -> &str {
    type_id.split_once('.').map_or(type_id, |(_, l)| l)
}
