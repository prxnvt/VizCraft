//! Picture > Adjust: brightness, contrast, recolour and transparency steps (the current one
//! checked), compress, change and reset.

use serde_json::json;

use super::{adjust_of, recolor_is};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};

/// A `picture.adjust` step for one setting, checked while the selected picture has that value.
macro_rules! step {
    ($label:expr, $param:literal, $field:ident, $v:expr) => {
        MenuEntry::new($label, "picture.adjust", json!({ $param: $v })).checked(|a| adjust_of(a).is_some_and(|x| (x.$field - $v).abs() < 1e-6))
    };
}

/// A `picture.adjust` recolour, checked while the selected picture has it.
macro_rules! recolor {
    ($label:expr, $v:literal) => {
        MenuEntry::new($label, "picture.adjust", json!({ "recolor": $v })).checked(|a| recolor_is(a, $v))
    };
}

/// The Picture tab's Adjust group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Adjust",
        vec![
            Stack(vec![
                Menu {
                    label: "Brightness",
                    icon: "brightness",
                    big: false,
                    menu: vec![
                        step!("-40%", "brightness", brightness, -0.4),
                        step!("-20%", "brightness", brightness, -0.2),
                        step!("0% (Normal)", "brightness", brightness, 0.0),
                        step!("+20%", "brightness", brightness, 0.2),
                        step!("+40%", "brightness", brightness, 0.4),
                    ],
                },
                Menu {
                    label: "Contrast",
                    icon: "contrast",
                    big: false,
                    menu: vec![
                        step!("-40%", "contrast", contrast, -0.4),
                        step!("-20%", "contrast", contrast, -0.2),
                        step!("0% (Normal)", "contrast", contrast, 0.0),
                        step!("+20%", "contrast", contrast, 0.2),
                        step!("+40%", "contrast", contrast, 0.4),
                    ],
                },
                Menu {
                    label: "Recolor",
                    icon: "recolor",
                    big: false,
                    menu: vec![
                        recolor!("No Recolor", "none"),
                        recolor!("Grayscale", "grayscale"),
                        recolor!("Sepia", "sepia"),
                        recolor!("Blue Tint", "#2f6fd0"),
                        recolor!("Green Tint", "#2e8b57"),
                        recolor!("Orange Tint", "#d9822b"),
                    ],
                },
            ]),
            Stack(vec![
                Menu {
                    label: "Transparency",
                    icon: "pictureTransparency",
                    big: false,
                    menu: vec![
                        step!("0% (Opaque)", "transparency", transparency, 0.0),
                        step!("25%", "transparency", transparency, 0.25),
                        step!("50%", "transparency", transparency, 0.5),
                        step!("75%", "transparency", transparency, 0.75),
                    ],
                },
                Small { id: "picture.compress", icon: "compress" },
                Small { id: "ui.changePicture", icon: "changePicture" },
            ]),
            Big { id: "picture.reset", icon: "resetPicture" },
        ],
    )
}
