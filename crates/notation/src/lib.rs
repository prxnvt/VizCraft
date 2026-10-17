//! VizCraft notation packs (model layer, below the engine).
//!
//! A notation pack is data: element and relationship types with property schemas and ports,
//! original glyphs, a connection table with multiplicity, conditional styles, palette groups,
//! viewpoints, identifier formats and validation rules. One pack drives the stencil catalog, the
//! palette, connection suggestions, the validator and agent schema discovery.
//!
//! - [`schema`]: the pack types ([`Pack`] and its parts), version [`schema::SCHEMA_VERSION`].
//! - [`loader`]: JSON and TOML packs to [`Pack`], with size caps and errors, never panics.
//! - [`validate`]: consistency checks on a loaded pack (ids, references, colours, outlines).
//! - [`catalog`]: the built-in packs (Basic and Flowchart) and lookups across packs.
//! - [`legal`]: connection-table queries: which relationships may join which types.
//! - [`rules`]: the rules engine: live rules after every command, batch rules on request,
//!   producing structured [`vizcraft_model::Issue`]s.
//! - [`bind`]: the sync pass that keeps model and views in step after every command.
//!
//! No egui and no engine; wasm safe.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod bind;
pub mod catalog;
pub mod legal;
pub mod loader;
pub mod rules;
pub mod schema;
pub mod validate;

pub use catalog::{Catalog, catalog};
pub use loader::NotationError;
pub use schema::{
    Check, ConditionalStyle, Connection, ElementType, Glyph, Pack, PaletteGroup, Port, PropKind, PropertySchema, RelationshipType, RuleDecl,
    RuleMode, Viewpoint,
};
