//! Review > Ink: the pens, eraser and lasso, the pen's colour and thickness, and Convert to
//! Shapes (the Ink tab's controls, gathered in one group).

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Review tab's Ink group.
pub(in crate::ribbon) fn group() -> Group {
    let mut controls = super::super::ink::tools();
    controls.extend(super::super::ink::format());
    controls.push(Big { id: "ink.toShapes", icon: "inkToShapes" });
    Group::new("Ink", controls)
}
