//! Connector > Arrows: the start and end arrowheads, each a menu of all thirteen ends with the
//! connector's current one checked. Every row runs `connector.setArrows`.

use serde_json::json;
use vizcraft_diagram::Arrow;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, selected_connector};

/// One row of an end menu: sets `param` ("start" or "end") to `arrow`, checked while the selected
/// connector's `end(c)` is `arrow`. The check function cannot capture, so each end gets its own
/// closure from [`menu`]'s macro.
macro_rules! entries {
    ($param:literal, $field:ident) => {
        vec![
            entries!(@one $param, $field, Arrow::None, "None"),
            entries!(@one $param, $field, Arrow::Triangle, "Triangle"),
            entries!(@one $param, $field, Arrow::Open, "Open arrow"),
            entries!(@one $param, $field, Arrow::Diamond, "Diamond"),
            entries!(@one $param, $field, Arrow::Circle, "Circle"),
            entries!(@one $param, $field, Arrow::HollowTriangle, "Hollow triangle"),
            entries!(@one $param, $field, Arrow::HollowDiamond, "Hollow diamond"),
            entries!(@one $param, $field, Arrow::Bar, "Bar"),
            entries!(@one $param, $field, Arrow::OpenCircle, "Hollow circle"),
            entries!(@one $param, $field, Arrow::CrowsFoot, "Crow's foot"),
            entries!(@one $param, $field, Arrow::CrowsFootBar, "Crow's foot with bar"),
            entries!(@one $param, $field, Arrow::CrowsFootCircle, "Crow's foot with circle"),
            entries!(@one $param, $field, Arrow::BarCircle, "Bar with circle"),
        ]
    };
    (@one $param:literal, $field:ident, $arrow:expr, $label:literal) => {
        MenuEntry::new($label, "connector.setArrows", json!({ $param: $arrow.name() }))
            .checked(|a| selected_connector(a).is_some_and(|c| c.$field == $arrow))
    };
}

/// The start end's rows.
fn start_menu() -> Vec<MenuEntry> {
    entries!("start", start_arrow)
}

/// The end end's rows.
fn end_menu() -> Vec<MenuEntry> {
    entries!("end", end_arrow)
}

/// The Connector tab's Arrows group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Arrows",
        vec![Stack(vec![
            Menu { label: "Start Arrow", icon: "arrowStart", big: false, menu: start_menu() },
            Menu { label: "End Arrow", icon: "arrowEnd", big: false, menu: end_menu() },
        ])],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every end in the order of [`Arrow::ALL`], with the words the menus show.
    const ARROW_LABELS: [(Arrow, &str); 13] = [
        (Arrow::None, "None"),
        (Arrow::Triangle, "Triangle"),
        (Arrow::Open, "Open arrow"),
        (Arrow::Diamond, "Diamond"),
        (Arrow::Circle, "Circle"),
        (Arrow::HollowTriangle, "Hollow triangle"),
        (Arrow::HollowDiamond, "Hollow diamond"),
        (Arrow::Bar, "Bar"),
        (Arrow::OpenCircle, "Hollow circle"),
        (Arrow::CrowsFoot, "Crow's foot"),
        (Arrow::CrowsFootBar, "Crow's foot with bar"),
        (Arrow::CrowsFootCircle, "Crow's foot with circle"),
        (Arrow::BarCircle, "Bar with circle"),
    ];

    #[test]
    fn the_label_table_and_both_menus_list_every_end_in_order() {
        let all: Vec<Arrow> = ARROW_LABELS.iter().map(|(a, _)| *a).collect();
        assert_eq!(all, Arrow::ALL.to_vec());
        for menu in [start_menu(), end_menu()] {
            let labels: Vec<&str> = menu.iter().map(|e| e.label).collect();
            assert_eq!(labels, ARROW_LABELS.iter().map(|(_, l)| *l).collect::<Vec<_>>());
        }
        let params: Vec<String> = start_menu().iter().map(|e| e.params["start"].as_str().unwrap_or_default().to_string()).collect();
        assert_eq!(params, Arrow::ALL.iter().map(|a| a.name().to_string()).collect::<Vec<_>>());
    }
}
