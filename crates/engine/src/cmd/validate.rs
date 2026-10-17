//! Validation commands: batch rules over a scope and the live issue list.

use crate::CommandSpec;

/// Return all validate commands: `validate.run`, `validate.list` (filled in by the c4a-rules piece).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
