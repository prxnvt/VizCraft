//! The application model. A craft replaces this module (and `io.rs`, `sample.rs`)
//! with its own domain model (such as a DAW project, diagram, DB schema, or animation scene).

use serde::{Deserialize, Serialize};

/// Document metadata, the part every craft's model carries in some form.
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

/// The application document/model. A real craft replaces this with its own data structures.
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
