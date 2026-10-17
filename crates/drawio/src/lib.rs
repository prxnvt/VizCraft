#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Reads and writes draw.io files and draw.io-style CSV into a faithful intermediate
//! representation.
//!
//! The IR stays close to the file: an [`DrawioFile`] holds pages, each page holds its
//! `<mxGraphModel>` XML, and [`GraphModel`] is that XML as cells with parsed [`Style`]s and
//! geometry. Reading, writing and reading again gives an equal IR, and untouched pages are
//! written back byte for byte. `crates/formats` will later map this IR onto VizCraft documents
//! (Cascade 3); this crate knows nothing about them.
//!
//! Implemented from draw.io's public documentation of its file format and CSV import, not from
//! any other implementation. Bytes and strings in, bytes and strings out: no file system, threads
//! or clocks, so it builds for wasm. Every input is treated as hostile and bounded by [`limits`].

mod attrs;
mod csv;
mod encoding;
mod file;
mod label;
mod model;
mod style;
mod xml;

pub use attrs::{Attributes, format_f64, parse_bool, parse_f64};
pub use csv::{ConnectSpec, CsvDiagram, CsvDirectives, CsvEdge, CsvLayout, CsvNode, NodeSize, SizeSpec, Spacing, parse_csv};
pub use encoding::{compress_page, decompress_page, percent_decode, percent_encode};
pub use file::{Container, DrawioFile, Page, PageInfo, PageRef, WriteOptions, read_file, write_file, write_file_with};
pub use label::{html_to_text, text_to_html};
pub use model::{Cell, Geometry, GraphModel, Point, Rect, Wrapper, WrapperKind, read_model, write_model};
pub use style::{Rgb, Style, StyleItem, parse_color};

/// Size limits applied to every input. They are far above what real diagrams need and exist to
/// turn hostile input into an error instead of an unbounded allocation.
pub mod limits {
    /// Largest accepted file or CSV text, in bytes.
    pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
    /// Largest decompressed page (and largest model XML), in bytes.
    pub const MAX_PAGE_BYTES: usize = 64 * 1024 * 1024;
    /// Largest total of decompressed pages in one file, in bytes.
    pub const MAX_TOTAL_DECODED_BYTES: usize = 256 * 1024 * 1024;
    /// Most cells in one page.
    pub const MAX_CELLS: usize = 200_000;
    /// Most pages in one file.
    pub const MAX_PAGES: usize = 1_000;
    /// Deepest XML element nesting.
    pub const MAX_XML_DEPTH: usize = 64;
    /// Most XML elements in one document.
    pub const MAX_XML_ELEMENTS: usize = 2_000_000;
    /// Most CSV data rows.
    pub const MAX_CSV_ROWS: usize = 100_000;
    /// Most CSV columns.
    pub const MAX_CSV_COLUMNS: usize = 1_000;
    /// Most CSV fields in total, header included.
    pub const MAX_CSV_CELLS: usize = 2_000_000;
    /// Longest CSV field, in bytes.
    pub const MAX_CSV_FIELD: usize = 1024 * 1024;
    /// Most edges a CSV import produces.
    pub const MAX_CSV_EDGES: usize = 500_000;
    /// Most `connect` directives.
    pub const MAX_CSV_CONNECTS: usize = 64;
    /// Most warnings kept from one CSV import.
    pub const MAX_WARNINGS: usize = 1_000;
}

/// Everything that can go wrong reading draw.io input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DrawioError {
    /// The XML is malformed.
    #[error("malformed XML: {0}")]
    Xml(String),
    /// The XML is well formed but is not a draw.io document.
    #[error("not a draw.io document: {0}")]
    NotDrawio(String),
    /// Text is not valid UTF-8 or a percent encoding is broken.
    #[error("encoding error: {0}")]
    Encoding(String),
    /// A compressed page is not valid base64 or deflate data.
    #[error("compressed page error: {0}")]
    Compression(String),
    /// No page matches the selector.
    #[error("page not found: {0}")]
    PageNotFound(String),
    /// A size limit was exceeded.
    #[error("limit exceeded: {0}")]
    Limit(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, DrawioError>;
