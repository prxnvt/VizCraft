//! `.drawio` files: the mapping between the `vizcraft-drawio` cell IR and the diagram model.
//!
//! The style mapping is fixed in the Cascade 3 contract (see docs/formats.md). Style keys
//! VizCraft does not model are kept in the items' `ext` maps so a round trip loses nothing.

use vizcraft_diagram::{Document, Page};
use vizcraft_stencils::Master;

use crate::{FormatError, Imported};

fn not_built() -> FormatError {
    FormatError::Import("draw.io files are not built yet".into())
}

/// Read a `.drawio` file (an `mxfile` with one or more pages, or a bare `mxGraphModel`) as a
/// document with one page per diagram.
pub fn load(_bytes: &[u8]) -> Result<Document, FormatError> {
    Err(not_built())
}

/// Write a document as a `.drawio` file, one uncompressed diagram per page.
pub fn save(_doc: &Document) -> Result<Vec<u8>, FormatError> {
    Err(not_built())
}

/// Read draw.io XML (an `mxfile` or a bare `mxGraphModel`) for import into an existing document.
pub fn import(_bytes: &[u8]) -> Result<Imported, FormatError> {
    Err(not_built())
}

/// One page as a bare `mxGraphModel` XML string.
pub fn page_xml(_page: &Page) -> Result<String, FormatError> {
    Err(not_built())
}

/// The draw.io style string for a stencil master, so an agent can write XML that opens as that
/// master.
pub fn master_style(master: &Master) -> String {
    format!("vizcraft={}", master.id)
}
