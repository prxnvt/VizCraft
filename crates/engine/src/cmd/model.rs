//! Model commands: project-level operations on the semantic model (containment, show in all views, delete from model, page viewpoint).

use crate::CommandSpec;

/// Return all model commands: `model.get`, `model.tree`, `model.move`, `model.views`, `model.delete`, `model.setViewpoint` (filled in by the c4a-commands piece).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
