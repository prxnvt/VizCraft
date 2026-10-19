//! VizCraft stencil catalog (model layer, below the engine).
//!
//! A stencil set is a named group of masters; a master is a recipe for a new shape: an outline
//! kind from `vizcraft-diagram`, a default size and style, a label and search keywords. Every
//! master is original and drawn in code; the names and colours are our own.
//!
//! The sets are not written here: each is built once from a notation pack's glyphs
//! (`vizcraft-notation`), so the pack is the one place a master's outline, size and colours live.
//! A set's id and name are the pack's; a pack with no glyphs gives no set.
//!
//! Geometry lives in `vizcraft-diagram` ([`ShapeKind`]); this crate only names and styles it, so
//! hit testing and routing stay in one place.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod builders;
mod connectors;
pub mod ports;
mod search;

use std::sync::{Mutex, PoisonError};

use vizcraft_diagram::geom::{Path, Point, Transform};
use vizcraft_diagram::glyph_state::GlyphState;
use vizcraft_diagram::{Color, Freeform, Shape, ShapeKind, Style};
use vizcraft_notation::{Catalog, Glyph, Pack};

pub use connectors::{ConnectorKind, connector_kind, connector_kind_for_relationship, connector_kinds, search_connector_kinds};
pub use ports::{PortMark, port_marks, port_marks_in, port_points, port_points_in};

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
    /// A path glyph's drawing in the unit box (`kind` is then [`ShapeKind::Freeform`]): closed
    /// subpaths fill, open ones are lines. `None` for a glyph drawn by its kind.
    pub path: Option<Path>,
    /// Connection points, normalized, from the ports of the glyph's element type (port `i` at
    /// point `i`), or from the builder for a builder glyph; empty to use the kind's own.
    pub ports: Vec<Point>,
    /// For a builder glyph: the builder, its port ids and slots as drawn with the glyph's
    /// defaults (no shape overrides). Copied onto every shape drawn from this master.
    pub glyph: Option<GlyphState>,
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

/// The sets built from one catalog, with the address of that catalog (identity, never read through).
type Cached = (usize, &'static [StencilSet]);

/// The stencil sets built from the catalog's glyphs. They are rebuilt when the catalog is not the
/// one they were built from (a pack was loaded), so a pack added at run time shows up in the
/// palette and in the catalog listing. The catalog replaces itself at most
/// a fixed number of times, so the strings leaked by the rebuilds are bounded.
fn built() -> &'static [StencilSet] {
    static SETS: Mutex<Option<Cached>> = Mutex::new(None);
    let cat = vizcraft_notation::catalog();
    let key = std::ptr::from_ref(cat) as usize;
    let mut slot = SETS.lock().unwrap_or_else(PoisonError::into_inner);
    match *slot {
        Some((k, sets)) if k == key => sets,
        _ => {
            let sets = build(cat);
            *slot = Some((key, sets));
            sets
        }
    }
}

/// An identity for the catalog the sets were built from: changes whenever a pack is loaded.
pub fn catalog_stamp() -> usize {
    std::ptr::from_ref(vizcraft_notation::catalog()) as usize
}

/// One stencil set per pack that has glyphs, in catalog order. The strings are leaked once so
/// the public API can keep handing out `&'static` data; this runs once per catalog (see
/// [`built`]) and the packs are small, so the leak is bounded.
fn build(cat: &Catalog) -> &'static [StencilSet] {
    let sets: Vec<StencilSet> = cat.packs().iter().filter(|p| !p.glyphs.is_empty()).map(set_from_pack).collect();
    sets.leak()
}

fn leak(s: &str) -> &'static str {
    s.to_string().leak()
}

fn set_from_pack(pack: &Pack) -> StencilSet {
    let mut categories: Vec<&'static str> = Vec::new();
    let masters: Vec<Master> = pack
        .glyphs
        .iter()
        .map(|g| {
            let mut m = master_from_glyph(g);
            // A builder places its own ports; the type's ports name them.
            if m.glyph.is_none() {
                m.ports = pack.element_type(&g.element_type).map(|t| ports::points_of(&t.ports)).unwrap_or_default();
            }
            if !categories.contains(&m.category) {
                categories.push(m.category);
            }
            m
        })
        .collect();
    StencilSet { id: leak(&pack.id), name: leak(&pack.name), categories: categories.leak(), masters: masters.leak() }
}

/// A master from a glyph. The loader has already checked outline names and colours, so the
/// fallbacks only guard against a pack built without the loader. A builder glyph is built once
/// with its defaults at its standard's module: the drawing normalized to the unit box, the size
/// its true size, the ports from the builder.
fn master_from_glyph(g: &Glyph) -> Master {
    let d = Style::DEFAULT;
    let style = Style { fill: Color::parse(&g.fill).unwrap_or(d.fill), line: Color::parse(&g.line).unwrap_or(d.line), ..d };
    let keywords: Vec<&'static str> = g.keywords.iter().map(|k| leak(k)).collect();
    let mut m = Master {
        id: leak(&g.id),
        label: leak(&g.label),
        category: leak(&g.category),
        kind: ShapeKind::from_name(&g.outline).unwrap_or_default(),
        w: g.w,
        h: g.h,
        style,
        keywords: keywords.leak(),
        path: None,
        ports: Vec::new(),
        glyph: None,
    };
    if g.outline == vizcraft_notation::loader::PATH_OUTLINE {
        m.kind = ShapeKind::Freeform;
        // A path that does not read (a pack built without the loader) draws as the frame.
        m.path = vizcraft_notation::glyph_path::glyph_outline(&g.path, &g.detail).ok();
    } else if g.outline == vizcraft_notation::loader::BUILDER_OUTLINE {
        // An unknown builder (a pack built without the engine's check) draws as the frame.
        if let Ok(b) = builders::build(&g.builder, &g.params, None) {
            m.kind = ShapeKind::Freeform;
            m.w = b.width();
            m.h = b.height();
            m.path = Some(b.path().transformed(&Transform::scale(1.0 / m.w.max(1e-9), 1.0 / m.h.max(1e-9))));
            m.ports = builders::apply::points_of(&b);
            m.glyph = Some(GlyphState {
                builder: g.builder.clone(),
                params: Default::default(),
                ports: b.ports.iter().map(|p| p.id.clone()).collect(),
                labels: builders::apply::labels_of(&b),
            });
        }
    }
    m
}

/// Every built-in stencil set, in display order.
pub fn sets() -> &'static [StencilSet] {
    built()
}

/// The set with this id.
pub fn set(id: &str) -> Option<&'static StencilSet> {
    built().iter().find(|s| s.id == id)
}

/// The master with this id (`flowchart.decision`).
pub fn master(id: &str) -> Option<&'static Master> {
    built().iter().flat_map(|s| s.masters.iter()).find(|m| m.id == id)
}

/// The master that draws `master_id`'s element type under rendering variant `variant` (empty
/// for the default): the variant's glyph when it maps the type, else `master_id`'s own master.
/// A master with no element type is returned as it is.
pub fn variant_master(master_id: &str, variant: &str) -> Option<&'static Master> {
    let cat = vizcraft_notation::catalog();
    let swapped = cat.type_for_master(master_id).and_then(|t| cat.master_for_type_in(t, variant)).and_then(master);
    swapped.or_else(|| master(master_id))
}

/// Masters matching every whitespace-separated word of `query` (case-insensitive) in the id,
/// label, category, set name or keywords, best match first: the exact label or id, then word
/// prefixes, then keywords, then substrings; ties keep catalog order. An empty query gives the
/// whole catalog in order.
pub fn search(query: &str) -> Vec<&'static Master> {
    search::search(query)
}

impl Master {
    /// A new shape from this master with its top-left corner at `(x, y)`. `w` and `h` override
    /// the default size; the model clamps hostile values.
    pub fn instantiate(&self, id: impl Into<String>, x: f64, y: f64, w: Option<f64>, h: Option<f64>) -> Shape {
        let mut s = Shape::new(id, self.kind, x, y, w.unwrap_or(self.w), h.unwrap_or(self.h));
        s.stencil = self.id.to_string();
        s.style = self.style.clone();
        s.sanitize();
        self.redraw(&mut s);
        s
    }

    /// Draw `s` as this master: its kind, path (scaled from the unit box onto the shape's
    /// size, keeping the box, so a glyph with a margin keeps it), connection points, style and
    /// builder state. The id, frame, text, data and glue are kept, and so are the shape's own
    /// builder parameters when the master is drawn by the same builder.
    pub fn redraw(&self, s: &mut Shape) {
        s.kind = self.kind;
        s.stencil = self.id.to_string();
        s.style = self.style.clone();
        s.path = self.path.as_ref().map(|p| {
            let (w, h) = (s.w.max(1e-6), s.h.max(1e-6));
            let local = p.transformed(&Transform::scale(w, h));
            let closed = local.els.iter().any(|e| matches!(e, vizcraft_diagram::geom::PathEl::Close));
            Freeform::in_frame(&local, closed, w, h)
        });
        s.connection_points = if self.ports.is_empty() { self.kind.connection_points() } else { self.ports.clone() };
        let kept = s.glyph.take().filter(|old| self.glyph.as_ref().is_some_and(|g| g.builder == old.builder)).map(|old| old.params);
        s.glyph = self.glyph.clone().map(|mut g| {
            g.params = kept.unwrap_or_default();
            g
        });
        s.sanitize();
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
        assert_eq!(search("").iter().filter(|m| m.id.starts_with("basic.") || m.id.starts_with("flowchart.")).count(), 22);
        assert!(search("nothing-like-this").is_empty());
    }

    /// The masters as they were hard-coded before packs drove them: id, label, category, kind,
    /// w, h, fill, line, keywords. Every built master must match exactly.
    #[test]
    fn masters_match_the_old_hard_coded_table() {
        const BASIC: (Color, Color) = (Color::rgb(0xe9, 0xf0, 0xf8), Color::rgb(0x34, 0x5b, 0x84));
        const STEP: (Color, Color) = (Color::rgb(0xe4, 0xf2, 0xec), Color::rgb(0x2d, 0x6e, 0x58));
        const CHOICE: (Color, Color) = (Color::rgb(0xfc, 0xf0, 0xd6), Color::rgb(0x95, 0x6a, 0x1a));
        const ENDPOINT: (Color, Color) = (Color::rgb(0xec, 0xe7, 0xf6), Color::rgb(0x5a, 0x4b, 0x8c));
        const DATA: (Color, Color) = (Color::rgb(0xe6, 0xf1, 0xfa), Color::rgb(0x2f, 0x62, 0x93));
        const STORE: (Color, Color) = (Color::rgb(0xf3, 0xe9, 0xe4), Color::rgb(0x84, 0x52, 0x3c));
        const LINK: (Color, Color) = (Color::rgb(0xf1, 0xf3, 0xf5), Color::rgb(0x55, 0x60, 0x6d));
        type Row = (&'static str, &'static str, &'static str, ShapeKind, f64, f64, (Color, Color), &'static [&'static str]);
        let old: &[Row] = &[
            ("basic.rectangle", "Rectangle", "Shapes", ShapeKind::Rectangle, 120.0, 70.0, BASIC, &["box", "square"]),
            ("basic.roundedRectangle", "Rounded Rectangle", "Shapes", ShapeKind::RoundedRectangle, 120.0, 70.0, BASIC, &["box", "rounded"]),
            ("basic.ellipse", "Ellipse", "Shapes", ShapeKind::Ellipse, 110.0, 80.0, BASIC, &["circle", "oval"]),
            ("basic.diamond", "Diamond", "Shapes", ShapeKind::Diamond, 100.0, 100.0, BASIC, &["rhombus"]),
            ("basic.triangle", "Triangle", "Shapes", ShapeKind::Triangle, 100.0, 90.0, BASIC, &["delta"]),
            ("basic.parallelogram", "Parallelogram", "Shapes", ShapeKind::Parallelogram, 130.0, 70.0, BASIC, &["slant", "skew"]),
            ("basic.hexagon", "Hexagon", "Shapes", ShapeKind::Hexagon, 120.0, 80.0, BASIC, &["six"]),
            ("basic.cylinder", "Cylinder", "Shapes", ShapeKind::Cylinder, 80.0, 100.0, BASIC, &["drum", "can", "database"]),
            ("basic.document", "Document", "Shapes", ShapeKind::Document, 120.0, 80.0, BASIC, &["page", "paper"]),
            ("basic.cloud", "Cloud", "Shapes", ShapeKind::Cloud, 140.0, 90.0, BASIC, &["internet", "network"]),
            ("flowchart.process", "Process", "Steps", ShapeKind::Rectangle, 140.0, 60.0, STEP, &["step", "action", "task"]),
            ("flowchart.decision", "Decision", "Steps", ShapeKind::Diamond, 140.0, 90.0, CHOICE, &["choice", "branch", "if", "condition"]),
            ("flowchart.terminator", "Terminator", "Steps", ShapeKind::Terminator, 140.0, 50.0, ENDPOINT, &["start", "end", "begin", "stop"]),
            ("flowchart.predefinedProcess", "Predefined Process", "Steps", ShapeKind::PredefinedProcess, 140.0, 60.0, STEP, &["subroutine", "call"]),
            ("flowchart.preparation", "Preparation", "Steps", ShapeKind::Preparation, 140.0, 60.0, STEP, &["setup", "initialize"]),
            ("flowchart.manualOperation", "Manual Operation", "Steps", ShapeKind::Trapezoid, 140.0, 60.0, STEP, &["manual", "by hand"]),
            ("flowchart.data", "Data", "Data", ShapeKind::Parallelogram, 140.0, 60.0, DATA, &["input", "output", "io"]),
            ("flowchart.manualInput", "Manual Input", "Data", ShapeKind::ManualInput, 140.0, 60.0, DATA, &["keyboard", "entry", "type"]),
            ("flowchart.document", "Document", "Data", ShapeKind::Document, 140.0, 70.0, DATA, &["report", "paper", "print"]),
            ("flowchart.database", "Database", "Data", ShapeKind::Cylinder, 90.0, 100.0, STORE, &["store", "storage", "disk", "cylinder"]),
            ("flowchart.onPageReference", "On-page Reference", "Links", ShapeKind::Ellipse, 50.0, 50.0, LINK, &["connector", "jump", "circle"]),
            (
                "flowchart.offPageReference",
                "Off-page Reference",
                "Links",
                ShapeKind::OffPageReference,
                60.0,
                60.0,
                LINK,
                &["jump", "page", "continue"],
            ),
        ];
        let built: Vec<&Master> = sets().iter().filter(|s| s.id == "basic" || s.id == "flowchart").flat_map(|s| s.masters.iter()).collect();
        assert_eq!(built.len(), old.len());
        for (m, (id, label, category, kind, w, h, (fill, line), keywords)) in built.iter().zip(old) {
            let want = Master {
                id,
                label,
                category,
                kind: *kind,
                w: *w,
                h: *h,
                style: Style { fill: *fill, line: *line, ..Style::DEFAULT },
                keywords,
                path: None,
                ports: Vec::new(),
                glyph: None,
            };
            assert_eq!(**m, want, "{id}");
        }
        let heads: Vec<_> = sets().iter().filter(|s| s.id == "basic" || s.id == "flowchart").map(|s| (s.id, s.name, s.categories)).collect();
        assert_eq!(heads, vec![("basic", "Basic Shapes", &["Shapes"][..]), ("flowchart", "Flowchart", &["Steps", "Data", "Links"][..])]);
    }

    #[test]
    fn packs_without_glyphs_give_no_set() {
        let empty = Pack { id: "empty".into(), name: "Empty".into(), ..Pack::default() };
        let mut packs = vizcraft_notation::catalog().packs().to_vec();
        packs.push(empty);
        let sets = build(&Catalog::new(packs));
        let ids: Vec<&str> = sets.iter().map(|s| s.id).collect();
        assert_eq!(ids.get(..3), Some(&["basic", "flowchart", "sheet"][..]));
        assert!(!ids.contains(&"empty"));
    }
}
