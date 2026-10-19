//! The scoped palette's content, without any drawing: which groups of shapes and connector kinds
//! the Palette pane and the notation tab offer for the active page.
//!
//! A page with a view (a viewpoint of some pack) shows that pack's palette groups first, with the
//! types filtered by the viewpoint unless the filter is switched off, then a Connectors group
//! (the pack's connector kinds, then the Basic ones) and a Drawing group of the Basic primitives.
//! A page with no view lists every pack's groups, each pack closed under its name, and Drawing
//! open. Everything is read from the notation catalog each time, so a pack loaded at run time
//! appears at once.
//!
//! Until the electrical, process and fluid power packs ship glyphs drawn by the parametric
//! builders, a closed "Engineering symbols (preview)" section offers one row per builder
//! (`vizcraft_stencils::builders`), each placing a free symbol with its defaults through
//! `glyph.insert`. Those packs replace it.

use serde_json::{Value, json};
use vizcraft_notation::{Catalog, ConnectorType, Pack, Viewpoint};
use vizcraft_stencils::Master;
use vizcraft_stencils::builders::{self, BuilderSpec};

use crate::CraftApp;

/// What a palette row inserts or selects.
#[derive(Clone, Copy)]
pub enum What {
    /// A stencil master: inserted on the page.
    Master(&'static Master),
    /// A connector kind: sets the connector tool's kind.
    Kind(&'static ConnectorType),
    /// A parametric glyph builder: places a free symbol it draws (`glyph.insert`).
    Builder(&'static BuilderSpec),
}

/// Id of the preview section of builder symbols.
pub const PREVIEW_SECTION: &str = "engineeringPreview";

/// The drag-and-drop payload prefix that names a builder instead of a master (`glyph:contact`).
pub const BUILDER_DRAG: &str = "glyph:";

/// A builder's symbol drawn with its defaults: the outline in page units (fills, then lines) and
/// its size, built once for the palette's previews and the insert position.
pub struct BuilderPreview {
    /// The outline, origin top left, in page units.
    pub path: vizcraft_diagram::Path,
    /// Width in page units.
    pub w: f64,
    /// Height in page units.
    pub h: f64,
}

/// The default drawing of builder `name`, or `None` for an unknown builder or one that cannot
/// build with its defaults.
pub fn builder_preview(name: &str) -> Option<&'static BuilderPreview> {
    static CACHE: std::sync::OnceLock<Vec<(&'static str, BuilderPreview)>> = std::sync::OnceLock::new();
    let all = CACHE.get_or_init(|| {
        builders::specs()
            .iter()
            .filter_map(|s| {
                let b = builders::build(s.name, &Default::default(), None).ok()?;
                Some((s.name, BuilderPreview { path: b.path(), w: b.width(), h: b.height() }))
            })
            .collect()
    });
    all.iter().find(|(n, _)| *n == name).map(|(_, p)| p)
}

/// One row of the palette.
#[derive(Clone)]
pub struct Item {
    /// The master id or connector kind id.
    pub id: &'static str,
    /// The element type the master stands for in the pack, when it has one.
    pub type_id: Option<&'static str>,
    /// Shown text.
    pub label: String,
    /// What the row does.
    pub what: What,
    /// The name of the pack the row comes from, shown beside a search hit whose label another hit
    /// shares; empty when it has none.
    pub pack: String,
    /// Lowercased words searched besides the label and id.
    keywords: Vec<String>,
}

/// A titled group of rows.
#[derive(Clone)]
pub struct Group {
    /// Stable id: the pack's palette group id (`flowchart.steps`), `connectors` or `drawing`.
    pub id: String,
    /// Heading.
    pub label: String,
    /// The rows, in order.
    pub items: Vec<Item>,
    /// Whether the rows are shown as a tile grid (the Drawing group) rather than a list.
    pub tiles: bool,
}

/// Groups shown together: a pack under its name when the page has no view, or the unnamed whole
/// scope when it has one.
#[derive(Clone)]
pub struct Section {
    /// Stable id: the pack id, `scope` or `drawing`.
    pub id: String,
    /// The pack's name; `None` for a section whose groups stand on their own.
    pub label: Option<String>,
    /// The groups, in order.
    pub groups: Vec<Group>,
    /// Whether a named section starts open.
    pub open: bool,
}

/// The palette for one page.
#[derive(Clone)]
pub struct Palette {
    /// The viewpoint the page shows, when it shows one the catalog knows.
    pub viewpoint: Option<String>,
    /// The sections, top to bottom.
    pub sections: Vec<Section>,
}

/// The viewpoint id of the active page, if its view names one.
pub fn active_viewpoint(app: &CraftApp) -> Option<&str> {
    app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.viewpoint.as_str()).filter(|v| !v.is_empty())
}

fn words_of(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_numeric();
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn master_item(cat: &'static Catalog, m: &'static Master, label: Option<&str>) -> Item {
    let type_id = cat.type_for_master(m.id);
    let mut keywords: Vec<String> = m.keywords.iter().flat_map(|k| words_of(k)).collect();
    keywords.extend(words_of(m.category));
    let pack = pack_name(cat, m.id);
    Item { id: m.id, type_id, label: label.filter(|l| !l.is_empty()).unwrap_or(m.label).to_string(), what: What::Master(m), pack, keywords }
}

/// The display name of the pack that owns id `id` (`<pack>.<name>`), or empty.
fn pack_name(cat: &Catalog, id: &str) -> String {
    id.split_once('.').and_then(|(p, _)| cat.pack(p)).map(|p| p.name.clone()).unwrap_or_default()
}

fn kind_item(t: &'static ConnectorType) -> Item {
    let mut keywords: Vec<String> = t.keywords.iter().flat_map(|k| words_of(k)).collect();
    keywords.extend(words_of(&t.category));
    let pack = pack_name(vizcraft_notation::catalog(), &t.id);
    Item { id: t.id.as_str(), type_id: None, label: t.label.clone(), what: What::Kind(t), pack, keywords }
}

fn builder_item(spec: &'static BuilderSpec) -> Item {
    let mut keywords = words_of(spec.description);
    keywords.extend(spec.sources.iter().flat_map(|(standard, _)| words_of(standard)));
    keywords.extend(["engineering", "symbol", "glyph"].map(str::to_string));
    Item { id: spec.name, type_id: None, label: spec.label.to_string(), what: What::Builder(spec), pack: String::new(), keywords }
}

/// The closed preview section: one row per glyph builder.
fn preview_section() -> Section {
    let items: Vec<Item> = builders::specs().iter().map(builder_item).collect();
    Section {
        id: PREVIEW_SECTION.into(),
        label: Some("Engineering symbols (preview)".into()),
        groups: vec![Group { id: "engineering.symbols".into(), label: "Symbols".into(), items, tiles: false }],
        open: false,
    }
}

/// The pack's palette groups. With a viewpoint, a type it does not list is left out (`*` lists
/// every type of the pack, an empty list any type), and a group left empty is dropped.
fn pack_groups(cat: &'static Catalog, pack: &'static Pack, vp: Option<&Viewpoint>) -> Vec<Group> {
    let mut out = Vec::new();
    for g in &pack.palette {
        let mut items = Vec::new();
        for t in &g.types {
            if vp.is_some_and(|v| !v.types.is_empty() && !pack.lists(&v.types, t)) {
                continue;
            }
            let Some(m) = cat.master_for_type(t).and_then(vizcraft_stencils::master) else { continue };
            let label = cat.element_type(t).map(|(_, e)| e.label.as_str());
            items.push(master_item(cat, m, label));
        }
        for gl in &g.glyphs {
            if let Some(m) = vizcraft_stencils::master(gl) {
                items.push(master_item(cat, m, None));
            }
        }
        if !items.is_empty() {
            out.push(Group { id: g.id.clone(), label: g.label.clone(), items, tiles: false });
        }
    }
    out
}

/// The pack's connector kinds, then the Basic ones, each once.
fn connector_group(cat: &'static Catalog, pack: Option<&'static Pack>) -> Option<Group> {
    let mut items: Vec<Item> = Vec::new();
    for p in pack.into_iter().chain(cat.pack("basic")) {
        for t in &p.connector_types {
            if !items.iter().any(|i| i.id == t.id) {
                items.push(kind_item(t));
            }
        }
    }
    (!items.is_empty()).then(|| Group { id: "connectors".into(), label: "Connectors".into(), items, tiles: false })
}

/// The Basic primitives as a tile grid.
fn drawing_group(cat: &'static Catalog) -> Option<Group> {
    let pack = cat.pack("basic")?;
    let items: Vec<Item> = pack.glyphs.iter().filter_map(|g| vizcraft_stencils::master(&g.id)).map(|m| master_item(cat, m, None)).collect();
    (!items.is_empty()).then(|| Group { id: "drawing".into(), label: "Drawing".into(), items, tiles: true })
}

/// The palette for a page showing `viewpoint` (or no view), with the viewpoint filter on or off.
pub fn build(viewpoint: Option<&str>, filter: bool) -> Palette {
    let cat = vizcraft_notation::catalog();
    let known = viewpoint.and_then(|v| cat.pack(v.split_once('.').map_or("", |(p, _)| p)).and_then(|p| p.viewpoint(v).map(|vp| (p, vp))));
    let mut sections = Vec::new();
    if let Some((pack, vp)) = known {
        let mut groups = if pack.id == "basic" { Vec::new() } else { pack_groups(cat, pack, filter.then_some(vp)) };
        groups.extend(connector_group(cat, Some(pack)));
        groups.extend(drawing_group(cat));
        sections.push(Section { id: "scope".into(), label: None, groups, open: true });
        sections.push(preview_section());
    } else {
        for pack in cat.packs().iter().filter(|p| p.id != "basic") {
            let mut groups = pack_groups(cat, pack, None);
            let kinds = pack.connector_types.iter().map(kind_item).collect::<Vec<_>>();
            if !kinds.is_empty() {
                groups.push(Group { id: format!("{}.connectors", pack.id), label: "Connectors".into(), items: kinds, tiles: false });
            }
            if !groups.is_empty() {
                sections.push(Section { id: pack.id.clone(), label: Some(pack.name.clone()), groups, open: false });
            }
        }
        sections.push(preview_section());
        if let Some(d) = drawing_group(cat) {
            sections.push(Section { id: "drawing".into(), label: None, groups: vec![d], open: true });
        }
    }
    Palette { viewpoint: known.map(|(_, vp)| vp.id.clone()), sections }
}

/// The palette for the active page of `app`.
pub fn for_app(app: &CraftApp) -> Palette {
    build(active_viewpoint(app), app.palette_filter)
}

impl Palette {
    /// Every group, in order.
    pub fn groups(&self) -> impl Iterator<Item = &Group> {
        self.sections.iter().flat_map(|s| s.groups.iter())
    }

    /// The rows matching `query` (every word must match), best first: the whole label or id, then
    /// word prefixes of the label or id, then keywords, then substrings; ties keep palette order.
    /// A row offered twice appears once.
    pub fn search(&self, query: &str) -> Vec<&Item> {
        let q: Vec<String> = words_of(&query.chars().take(256).collect::<String>()).into_iter().take(16).collect();
        if q.is_empty() {
            return Vec::new();
        }
        let whole = q.join(" ");
        let mut hits: Vec<(u8, usize, &Item)> = Vec::new();
        for (n, item) in self.groups().flat_map(|g| g.items.iter()).enumerate() {
            if hits.iter().any(|(_, _, h)| h.id == item.id) {
                continue;
            }
            let names: Vec<String> = words_of(&item.label).into_iter().chain(words_of(item.id)).collect();
            let hay = format!("{} {} {}", item.label.to_lowercase(), item.id.to_lowercase(), item.keywords.join(" "));
            let mut worst = 0u8;
            let mut all = true;
            for w in &q {
                let tier = if names.iter().any(|x| x.starts_with(w.as_str())) {
                    1
                } else if item.keywords.iter().any(|x| x.starts_with(w.as_str())) {
                    2
                } else if hay.contains(w.as_str()) {
                    3
                } else {
                    all = false;
                    break;
                };
                worst = worst.max(tier);
            }
            if !all {
                continue;
            }
            if words_of(&item.label).join(" ") == whole || words_of(item.id).join(" ") == whole {
                worst = 0;
            }
            hits.push((worst, n, item));
        }
        hits.sort_by_key(|(t, n, _)| (*t, *n));
        hits.into_iter().map(|(_, _, i)| i).collect()
    }

    /// The `palette` part of `ui.inspect`: the viewpoint, the filter, the search text and every
    /// group with the ids of its rows.
    pub fn report(&self, app: &CraftApp) -> Value {
        let groups: Vec<Value> = self
            .sections
            .iter()
            .flat_map(|s| {
                s.groups.iter().map(move |g| {
                    json!({"id": g.id, "label": g.label, "section": s.id, "pack": s.label,
                        "items": g.items.iter().map(|i| i.id).collect::<Vec<_>>()})
                })
            })
            .collect();
        json!({
            "viewpoint": self.viewpoint,
            "viewpointFilter": app.palette_filter,
            "query": app.shapes_query,
            "connectorKind": app.connector_kind,
            "groups": groups,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(p: &Palette) -> Vec<&str> {
        p.groups().map(|g| g.label.as_str()).collect()
    }

    #[test]
    fn a_flowchart_view_lists_steps_data_links_connectors_drawing() {
        let p = build(Some("flowchart.Flowchart"), true);
        assert_eq!(labels(&p), ["Steps", "Data", "Links", "Connectors", "Drawing", "Symbols"]);
        let kinds: Vec<&str> = p.groups().find(|g| g.id == "connectors").unwrap().items.iter().map(|i| i.id).collect();
        assert_eq!(kinds[0], "flowchart.flow");
        assert!(kinds.contains(&"basic.arrow"));
        assert!(p.groups().find(|g| g.id == "drawing").unwrap().tiles);
    }

    #[test]
    fn no_view_lists_every_pack_closed_and_drawing_open() {
        let p = build(None, true);
        let flow = p.sections.iter().find(|s| s.id == "flowchart").unwrap();
        assert!(!flow.open && flow.label.as_deref() == Some("Flowchart"));
        let last = p.sections.last().unwrap();
        assert!(last.open && last.label.is_none() && last.groups[0].id == "drawing");
    }

    #[test]
    fn the_preview_section_lists_every_builder_with_a_drawing() {
        for p in [build(None, true), build(Some("flowchart.Flowchart"), true)] {
            let sec = p.sections.iter().find(|s| s.id == PREVIEW_SECTION).unwrap();
            assert!(!sec.open && sec.label.as_deref() == Some("Engineering symbols (preview)"));
            let ids: Vec<&str> = sec.groups[0].items.iter().map(|i| i.id).collect();
            assert_eq!(ids, ["contact", "coil", "bubble", "valve", "cylinder", "pump", "gate", "fcf", "weld", "texture"]);
            for id in ids {
                let b = builder_preview(id).unwrap_or_else(|| panic!("{id} builds with its defaults"));
                assert!(b.w > 0.0 && b.h > 0.0 && !b.path.els.is_empty(), "{id}");
            }
            assert!(p.search("coil").iter().any(|i| i.id == "coil"));
        }
    }

    #[test]
    fn search_ranks_the_exact_label_first_and_finds_kinds() {
        let p = build(Some("flowchart.Flowchart"), true);
        let ids: Vec<&str> = p.search("decision").iter().map(|i| i.id).collect();
        assert_eq!(ids.first(), Some(&"flowchart.decision"));
        assert!(p.search("flow").iter().any(|i| i.id == "flowchart.flow"));
        assert!(p.search("cloud").iter().any(|i| i.id == "basic.cloud"), "Drawing masters are searched");
        assert!(p.search("").is_empty() && p.search("zzzzqq").is_empty());
    }
}
