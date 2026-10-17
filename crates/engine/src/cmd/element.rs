//! Element commands: create elements (optionally with a view), rename, set properties, retag through the identity registry, show in another view, delete from view.

use crate::CommandSpec;

/// Return all element commands: `element.create`, `element.get`, `element.rename`, `element.setProperty`, `element.retag`, `element.show`, `element.deleteFromView` (filled in by the c4a-commands piece).
pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}
