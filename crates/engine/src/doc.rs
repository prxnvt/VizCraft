//! The document model lives in `vizcraft-diagram`, below the engine, so file formats, the UI and
//! the MCP server share it without depending on the engine. This module re-exports it so engine
//! callers keep writing `vizcraft_engine::doc::Document`.

pub use vizcraft_diagram::{Arrow, Color, Connector, CoreProps, Document, End, Page, Route, Shape, ShapeKind, Style};
