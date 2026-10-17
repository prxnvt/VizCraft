//! Notation commands: list packs, describe a pack or type, legal relationships from an element or type. They read the catalog only.

use crate::CommandSpec;

/// Return all notation commands: `notation.list`, `notation.describe`, `notation.legal` (filled in by the c4a-notation piece).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
