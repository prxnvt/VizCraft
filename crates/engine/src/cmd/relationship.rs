//! Relationship commands: create (with a connector view when both ends are drawn on one page), retype, reconnect, set properties.

use crate::CommandSpec;

/// Return all relationship commands: `relationship.create`, `relationship.retype`, `relationship.reconnect`, `relationship.setProperty` (filled in by the c4a-commands piece).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
