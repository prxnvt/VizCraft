//! VizCraft stencil catalog (model layer, below the engine).
//!
//! A stencil set is a named group of masters; a master is a recipe for a new shape: an outline
//! kind from `vizcraft-diagram`, a default size and style, a label and search keywords. Every
//! master is original and drawn in code; the names and colours are our own.
//!
//! Geometry lives in `vizcraft-diagram` ([`ShapeKind`]); this crate only names and styles it, so
//! hit testing and routing stay in one place.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use vizcraft_diagram::{Color, Shape, ShapeKind, Style};

/// A shape recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Master {
    /// Stable id, `<set>.<name>`: `basic.rectangle`, `flowchart.decision`. Stored on shapes.
    pub id: &'static str,
    /// Name shown in the Shapes pane.
    pub label: &'static str,
    /// Category within the set (the pane groups by it).
    pub category: &'static str,
    /// The outline.
    pub kind: ShapeKind,
    /// Default width in world units.
    pub w: f64,
    /// Default height in world units.
    pub h: f64,
    /// Default style.
    pub style: Style,
    /// Extra search words (the label and id are searched too).
    pub keywords: &'static [&'static str],
}

/// A named group of masters.
#[derive(Clone, Debug, PartialEq)]
pub struct StencilSet {
    /// Stable id: `basic`, `flowchart`.
    pub id: &'static str,
    /// Name shown in the Shapes pane.
    pub name: &'static str,
    /// Categories in display order.
    pub categories: &'static [&'static str],
    /// The masters, in display order.
    pub masters: &'static [Master],
}

/// A style with our own fill, line and text colours over the model defaults.
const fn style(fill: Color, line: Color) -> Style {
    Style { fill, line, ..Style::DEFAULT }
}

// Basic set: a cool blue-grey pair.
const BASIC: Style = style(Color::rgb(0xe9, 0xf0, 0xf8), Color::rgb(0x34, 0x5b, 0x84));
// Flowchart roles, one quiet colour each so a chart reads at a glance.
const STEP: Style = style(Color::rgb(0xe4, 0xf2, 0xec), Color::rgb(0x2d, 0x6e, 0x58));
const CHOICE: Style = style(Color::rgb(0xfc, 0xf0, 0xd6), Color::rgb(0x95, 0x6a, 0x1a));
const ENDPOINT: Style = style(Color::rgb(0xec, 0xe7, 0xf6), Color::rgb(0x5a, 0x4b, 0x8c));
const DATA: Style = style(Color::rgb(0xe6, 0xf1, 0xfa), Color::rgb(0x2f, 0x62, 0x93));
const STORE: Style = style(Color::rgb(0xf3, 0xe9, 0xe4), Color::rgb(0x84, 0x52, 0x3c));
const LINK: Style = style(Color::rgb(0xf1, 0xf3, 0xf5), Color::rgb(0x55, 0x60, 0x6d));

const fn m(
    id: &'static str,
    label: &'static str,
    category: &'static str,
    kind: ShapeKind,
    w: f64,
    h: f64,
    style: Style,
    keywords: &'static [&'static str],
) -> Master {
    Master { id, label, category, kind, w, h, style, keywords }
}

const BASIC_MASTERS: &[Master] = &[
    m("basic.rectangle", "Rectangle", "Shapes", ShapeKind::Rectangle, 120.0, 70.0, BASIC, &["box", "square"]),
    m("basic.roundedRectangle", "Rounded Rectangle", "Shapes", ShapeKind::RoundedRectangle, 120.0, 70.0, BASIC, &["box", "rounded"]),
    m("basic.ellipse", "Ellipse", "Shapes", ShapeKind::Ellipse, 110.0, 80.0, BASIC, &["circle", "oval"]),
    m("basic.diamond", "Diamond", "Shapes", ShapeKind::Diamond, 100.0, 100.0, BASIC, &["rhombus"]),
    m("basic.triangle", "Triangle", "Shapes", ShapeKind::Triangle, 100.0, 90.0, BASIC, &["delta"]),
    m("basic.parallelogram", "Parallelogram", "Shapes", ShapeKind::Parallelogram, 130.0, 70.0, BASIC, &["slant", "skew"]),
    m("basic.hexagon", "Hexagon", "Shapes", ShapeKind::Hexagon, 120.0, 80.0, BASIC, &["six"]),
    m("basic.cylinder", "Cylinder", "Shapes", ShapeKind::Cylinder, 80.0, 100.0, BASIC, &["drum", "can", "database"]),
    m("basic.document", "Document", "Shapes", ShapeKind::Document, 120.0, 80.0, BASIC, &["page", "paper"]),
    m("basic.cloud", "Cloud", "Shapes", ShapeKind::Cloud, 140.0, 90.0, BASIC, &["internet", "network"]),
];

const FLOWCHART_MASTERS: &[Master] = &[
    m("flowchart.process", "Process", "Steps", ShapeKind::Rectangle, 140.0, 60.0, STEP, &["step", "action", "task"]),
    m("flowchart.decision", "Decision", "Steps", ShapeKind::Diamond, 140.0, 90.0, CHOICE, &["choice", "branch", "if", "condition"]),
    m("flowchart.terminator", "Terminator", "Steps", ShapeKind::Terminator, 140.0, 50.0, ENDPOINT, &["start", "end", "begin", "stop"]),
    m("flowchart.predefinedProcess", "Predefined Process", "Steps", ShapeKind::PredefinedProcess, 140.0, 60.0, STEP, &["subroutine", "call"]),
    m("flowchart.preparation", "Preparation", "Steps", ShapeKind::Preparation, 140.0, 60.0, STEP, &["setup", "initialize"]),
    m("flowchart.manualOperation", "Manual Operation", "Steps", ShapeKind::Trapezoid, 140.0, 60.0, STEP, &["manual", "by hand"]),
    m("flowchart.data", "Data", "Data", ShapeKind::Parallelogram, 140.0, 60.0, DATA, &["input", "output", "io"]),
    m("flowchart.manualInput", "Manual Input", "Data", ShapeKind::ManualInput, 140.0, 60.0, DATA, &["keyboard", "entry", "type"]),
    m("flowchart.document", "Document", "Data", ShapeKind::Document, 140.0, 70.0, DATA, &["report", "paper", "print"]),
    m("flowchart.database", "Database", "Data", ShapeKind::Cylinder, 90.0, 100.0, STORE, &["store", "storage", "disk", "cylinder"]),
    m("flowchart.onPageReference", "On-page Reference", "Links", ShapeKind::Ellipse, 50.0, 50.0, LINK, &["connector", "jump", "circle"]),
    m("flowchart.offPageReference", "Off-page Reference", "Links", ShapeKind::OffPageReference, 60.0, 60.0, LINK, &["jump", "page", "continue"]),
];

const SETS: &[StencilSet] = &[
    StencilSet { id: "basic", name: "Basic Shapes", categories: &["Shapes"], masters: BASIC_MASTERS },
    StencilSet { id: "flowchart", name: "Flowchart", categories: &["Steps", "Data", "Links"], masters: FLOWCHART_MASTERS },
];

/// Every built-in stencil set, in display order.
pub fn sets() -> &'static [StencilSet] {
    SETS
}

/// The set with this id.
pub fn set(id: &str) -> Option<&'static StencilSet> {
    SETS.iter().find(|s| s.id == id)
}

/// The master with this id (`flowchart.decision`).
pub fn master(id: &str) -> Option<&'static Master> {
    SETS.iter().flat_map(|s| s.masters.iter()).find(|m| m.id == id)
}

/// Masters matching every whitespace-separated word of `query` (case-insensitive) in the id,
/// label or keywords. An empty query matches everything.
pub fn search(query: &str) -> Vec<&'static Master> {
    let words: Vec<String> = query.split_whitespace().take(16).map(str::to_lowercase).collect();
    SETS.iter()
        .flat_map(|s| s.masters.iter())
        .filter(|m| {
            let hay = format!("{} {} {}", m.id, m.label, m.keywords.join(" ")).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect()
}

impl Master {
    /// A new shape from this master with its top-left corner at `(x, y)`. `w` and `h` override
    /// the default size; the model clamps hostile values.
    pub fn instantiate(&self, id: impl Into<String>, x: f64, y: f64, w: Option<f64>, h: Option<f64>) -> Shape {
        let mut s = Shape::new(id, self.kind, x, y, w.unwrap_or(self.w), h.unwrap_or(self.h));
        s.stencil = self.id.to_string();
        s.style = self.style.clone();
        s.sanitize();
        s
    }
}

/// A new shape from the master `master_id`, or `None` for an unknown master.
pub fn new_shape(master_id: &str, id: impl Into<String>, x: f64, y: f64, w: Option<f64>, h: Option<f64>) -> Option<Shape> {
    master(master_id).map(|m| m.instantiate(id, x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_prefixed_by_set() {
        let mut seen = std::collections::HashSet::new();
        for s in sets() {
            for m in s.masters {
                assert!(seen.insert(m.id), "duplicate {}", m.id);
                assert!(m.id.starts_with(&format!("{}.", s.id)), "{}", m.id);
                assert!(s.categories.contains(&m.category), "{} category {}", m.id, m.category);
                assert!(m.w > 0.0 && m.h > 0.0);
            }
        }
        assert_eq!(set("basic").unwrap().masters.len(), 10);
    }

    #[test]
    fn basic_set_covers_the_ten_basic_kinds() {
        let kinds: Vec<_> = set("basic").unwrap().masters.iter().map(|m| m.kind).collect();
        for k in [
            ShapeKind::Rectangle,
            ShapeKind::RoundedRectangle,
            ShapeKind::Ellipse,
            ShapeKind::Diamond,
            ShapeKind::Triangle,
            ShapeKind::Parallelogram,
            ShapeKind::Hexagon,
            ShapeKind::Cylinder,
            ShapeKind::Document,
            ShapeKind::Cloud,
        ] {
            assert!(kinds.contains(&k), "{k:?}");
        }
    }

    #[test]
    fn new_shape_uses_master_defaults_and_overrides() {
        let s = new_shape("flowchart.decision", "s1", 10.0, 20.0, None, None).unwrap();
        assert_eq!((s.id.as_str(), s.stencil.as_str(), s.kind), ("s1", "flowchart.decision", ShapeKind::Diamond));
        assert_eq!((s.x, s.y, s.w, s.h), (10.0, 20.0, 140.0, 90.0));
        assert_eq!(s.connection_points.len(), 4);
        let s = new_shape("basic.cloud", "s2", 0.0, 0.0, Some(f64::NAN), Some(1e300)).unwrap();
        assert_eq!((s.w, s.h), (0.0, vizcraft_diagram::MAX_EXTENT));
        assert!(new_shape("vendor.logo", "s3", 0.0, 0.0, None, None).is_none());
    }

    #[test]
    fn search_matches_keywords() {
        let ids: Vec<_> = search("start").iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["flowchart.terminator"]);
        assert!(search("DATABASE").iter().any(|m| m.id == "flowchart.database"));
        assert_eq!(search("").len(), 22);
        assert!(search("nothing-like-this").is_empty());
    }
}
