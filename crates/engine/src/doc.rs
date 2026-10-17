//! The document model: a placeholder with metadata only. The diagram model (pages, layers,
//! shapes, connectors) replaces it in Cascade 1, together with `io.rs` and `sample.rs`.

use serde::{Deserialize, Serialize};

/// Document metadata: author, timestamps and revision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreProps {
    /// Who made the document.
    pub author: String,
    /// Who saved it last.
    pub last_modified_by: String,
    /// ISO-8601 timestamps, empty until the first save.
    pub created: String,
    pub modified: String,
    /// Save count.
    pub revision: u32,
}

/// The document. A placeholder until the diagram model lands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Document {
    /// Shown in the title bar when the document has no file yet.
    pub title: String,
    pub core: CoreProps,
}

impl Document {
    /// An empty document.
    pub fn new() -> Self {
        Document { title: String::new(), core: CoreProps::default() }
    }
}
