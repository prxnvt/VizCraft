//! Home > Arrange: quarter turns and delete.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size, selected_shape};

/// The new angle for a quarter turn of the selected shape; with nothing selected the engine
/// answers "select a shape first", which the status bar shows.
fn quarter_turn(app: &CraftApp, by: f64) -> Value {
    let angle = selected_shape(app).map(|s| s.angle_deg + by).unwrap_or(0.0);
    json!({ "angle": angle.rem_euclid(360.0) })
}

/// The Home tab's Arrange group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Arrange",
        vec![Stack(vec![
            Button { id: "shape.rotate", icon: "rotateLeft", label: "Rotate Left", size: Size::Small, params: |a| quarter_turn(a, -90.0), checked: None },
            Button { id: "shape.rotate", icon: "rotateRight", label: "Rotate Right", size: Size::Small, params: |a| quarter_turn(a, 90.0), checked: None },
            Small { id: "edit.delete", icon: "delete" },
        ])],
    )
}
