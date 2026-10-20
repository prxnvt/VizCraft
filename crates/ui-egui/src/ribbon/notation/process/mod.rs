//! The process pack's groups on its notation tab (Process Plant), after the generic notation
//! groups: Lines (the Pipe tool, its line kind, Assign Line, Place and Pair Off-page), Plant
//! (the Loops, Tags and Add Nozzle menus) and Lists (the process lists in the Tables dock). They
//! are menus in stacks so the tab fits 1280 points wide.
//!
//! Every control runs a command by id: `ui.tool {tool: "pipe", line}`, `line.assign`,
//! `offpage.place`, `offpage.pair`, `plant.numbering`, `identity.setFormat` (no params opens the
//! Tag Formats dialog), `sheet.renumber`, `loop.create`, `ui.loopTrace`, `loop.diagram`,
//! `plant.addNozzle` and `ui.tables {pack}`.

use serde_json::{Value, json};

use crate::ribbon::Control::*;
use crate::ribbon::{Control, Group, MenuEntry, Size};
use crate::widgets::{menu_button, menu_item};
use crate::{CraftApp, Tool};

/// The process pack's groups.
pub(in crate::ribbon) fn groups() -> Vec<Group> {
    vec![plant()]
}

/// Whether the Pipe tool is armed.
fn pipe_armed(app: &CraftApp) -> bool {
    app.tool == Tool::Wire { pipe: true }
}

/// The Pipe button: arms the Pipe tool with the chosen line kind, or returns to the pointer.
fn pipe_params(app: &CraftApp) -> Value {
    if pipe_armed(app) {
        json!({"tool": "pointer"})
    } else {
        match &app.line_type {
            None => json!({"tool": "pipe"}),
            Some(l) => json!({"tool": "pipe", "line": l}),
        }
    }
}

/// The page's pipe kinds (pipes, ducts and water lines the page's viewpoint draws), the chosen
/// one checked; choosing one arms the Pipe tool with it.
fn line_kinds(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "lineKind", "Kind", "Line Kind: what the Pipe tool draws", false, |ui, app| {
        let now = crate::canvas::interact::wire::line_type(app, true).unwrap_or_default();
        for k in crate::canvas::interact::wire::line_kinds(app, true) {
            menu_item(ui, app, &k.label, "ui.tool", json!({"tool": "pipe", "line": k.id}), Some(now == k.id));
        }
    });
}

fn lines() -> Control {
    Stack(vec![
        Button { id: "ui.tool", icon: "processPipe", label: "Pipe", size: Size::Small, params: pipe_params, checked: Some(pipe_armed) },
        Custom(line_kinds),
        Menu {
            label: "Lines",
            icon: "assignLine",
            big: false,
            menu: vec![
                MenuEntry::new("Assign Line", "line.assign", json!({})),
                MenuEntry::new("New Line from Selection", "line.assign", json!({"line": "new"})),
                MenuEntry::new("Place Off-page at the Free End", "offpage.place", json!({})),
                MenuEntry::new("Pair the Two Selected Off-page", "offpage.pair", json!({})),
            ],
        },
    ])
}

fn parallel(app: &CraftApp) -> bool {
    !serial(app)
}

fn serial(app: &CraftApp) -> bool {
    app.session.doc.model.id_formats.get("process.loop").and_then(|o| o.number_by.as_deref()).is_some_and(str::is_empty)
}

/// Loops, tags and nozzles, each a menu of its commands, in one stack so the tab fits.
fn plant() -> Group {
    Group::new(
        "Plant",
        vec![
            lines(),
            Stack(vec![
                Menu {
                    label: "Loops",
                    icon: "processLoop",
                    big: false,
                    menu: vec![
                        MenuEntry::new("New Loop", "loop.create", json!({})),
                        MenuEntry::new("Trace Loop", "ui.loopTrace", json!({})),
                        MenuEntry::new("Loop Diagram", "loop.diagram", json!({})),
                    ],
                },
                Menu {
                    label: "Tags",
                    icon: "tagFormats",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Parallel Numbering (per variable)", "plant.numbering", json!({"scheme": "parallel"})).checked(parallel),
                        MenuEntry::new("Serial Numbering (one sequence)", "plant.numbering", json!({"scheme": "serial"})).checked(serial),
                        MenuEntry::new("Tag Formats...", "identity.setFormat", json!({})),
                        MenuEntry::new("Renumber Tags", "sheet.renumber", json!({})),
                    ],
                },
                Menu {
                    label: "Nozzles",
                    icon: "nozzle",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Left", "plant.addNozzle", json!({"side": "left"})),
                        MenuEntry::new("Right", "plant.addNozzle", json!({"side": "right"})),
                        MenuEntry::new("Top", "plant.addNozzle", json!({"side": "top"})),
                        MenuEntry::new("Bottom", "plant.addNozzle", json!({"side": "bottom"})),
                    ],
                },
            ]),
            lists(),
        ],
    )
}

fn lists() -> Control {
    let entry = |label: &'static str, list: &'static str| MenuEntry::new(label, "ui.tables", json!({"pack": format!("process.{list}")}));
    Menu {
        label: "Lists",
        icon: "plantList",
        big: true,
        menu: vec![
            entry("Line List", "line"),
            entry("Equipment List", "equipment"),
            entry("Instrument Index", "instrument"),
            entry("Valve List", "valve"),
            entry("I/O List", "io"),
            entry("Stream Table", "stream"),
            entry("Nozzle List", "nozzle"),
            entry("Fixture Units", "fixtureUnits"),
            entry("Bill of Materials", "bom"),
        ],
    }
}

/// Generic notation groups a process page leaves out to make room (Steps numbers request flows,
/// which a plant diagram does not have).
pub(in crate::ribbon) const HIDES: &[&str] = &["Steps"];
