//! draw.io-style CSV import: rows become shapes, references between rows become connectors, and
//! the auto-layout places them as the CSV's `layout` directive asks.

use crate::{FormatError, Imported};

/// Read draw.io-style CSV text as one laid-out page.
pub fn import(_text: &str) -> Result<Imported, FormatError> {
    Err(FormatError::Import("CSV import is not built yet".into()))
}
