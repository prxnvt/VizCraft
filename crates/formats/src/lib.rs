//! VizCraft file formats (model layer, below the engine).
//!
//! - [`native`]: the `.vizcraft` file, a versioned JSON wrapper around the diagram
//!   [`Document`](vizcraft_diagram::Document); also reads the version 1 placeholder files.
//! - [`svg`]: export of one page as a standalone SVG 1.1 document.
//!
//! Bytes in, bytes out: readers take a byte slice and writers return a byte vector, with no file
//! system access, so the desktop app, the CLI, the MCP server and the web build share one code
//! path. Every input is untrusted: sizes are capped and errors are returned, never panics.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod native;
pub mod svg;

/// Why a file could not be read or written.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The input is larger than the reader accepts.
    #[error("file is too large ({len} bytes; the limit is {max})")]
    TooLarge {
        /// Size of the input in bytes.
        len: usize,
        /// The limit in bytes.
        max: usize,
    },
    /// The input is not valid JSON, or not shaped like a document.
    #[error("not a VizCraft document: {0}")]
    Invalid(String),
    /// A wrapper whose `format` tag names some other format.
    #[error("not a VizCraft document: the format tag is `{0}`")]
    WrongFormat(String),
    /// A wrapper written by a newer VizCraft than this one.
    #[error("this file was saved by a newer VizCraft (file version {found}; this version reads up to {supported})")]
    TooNew {
        /// The file's version.
        found: u64,
        /// The newest version this build reads.
        supported: u32,
    },
    /// The requested page does not exist.
    #[error("the document has no page {0}")]
    NoPage(usize),
    /// Encoding failed (should not happen for a valid document).
    #[error("could not encode the document: {0}")]
    Encode(String),
}

#[cfg(test)]
mod tests;
