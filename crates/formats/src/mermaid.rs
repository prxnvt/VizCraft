//! Mermaid import: flowchart, sequence, class, state and ER text become shapes and connectors on
//! one page, placed with the auto-layout.

use crate::{FormatError, Imported};

/// Read Mermaid text as one laid-out page; skipped statements come back as warnings.
pub fn import(_text: &str) -> Result<Imported, FormatError> {
    Err(FormatError::Import("Mermaid import is not built yet".into()))
}
