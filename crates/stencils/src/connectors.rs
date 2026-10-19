//! The connector catalog: every pack's connector types as kinds a connector can be drawn as.
//!
//! Like the shape masters, the kinds are not written here; each is built once from a notation
//! pack's `connectorTypes`, so the pack is the one place a kind's route, ends and line style live.
//! A kind is a recipe: [`ConnectorKind::apply`] copies its values onto a connector and records
//! its id in `Connector.kind`; the geometry of the ends lives in `vizcraft-diagram`.

use std::sync::{Mutex, PoisonError};

use vizcraft_diagram::vizcraft_model::NetKind;
use vizcraft_diagram::{Arrow, Color, Connector, Dash, LineMarker, Route, Style};
use vizcraft_notation::{Catalog, ConnectorType};

use crate::search::words;

/// Longest query looked at, in chars; real queries are a few words.
const MAX_QUERY_CHARS: usize = 256;
/// Most query words looked at; the rest are ignored.
const MAX_WORDS: usize = 16;
/// Longest query word looked at, in chars.
const MAX_WORD_CHARS: usize = 64;

/// A connector recipe: a named route, pair of line ends and line style.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorKind {
    /// Stable id, `<pack>.<name>`: `basic.arrow`, `flowchart.flow`. Stored on connectors as `kind`.
    pub id: &'static str,
    /// Name shown in the Kind gallery.
    pub label: &'static str,
    /// Category within the catalog (the gallery groups by it).
    pub category: &'static str,
    /// How the connector runs.
    pub route: Route,
    /// Line end at the start.
    pub start_arrow: Arrow,
    /// Line end at the end.
    pub end_arrow: Arrow,
    /// Dash pattern.
    pub dash: Dash,
    /// Line width.
    pub line_width: f64,
    /// Line colour.
    pub line: Color,
    /// Whether the connector hops over lines drawn before it.
    pub jumps: bool,
    /// Extra search words (the label and id are searched too).
    pub keywords: &'static [&'static str],
    /// The relationship type a connector of this kind becomes between bound shapes, if the kind
    /// names one.
    pub relationship_type: Option<&'static str>,
    /// The net kind a connector of this kind is a segment of, if any: the sync step's net pass
    /// gives such a connector a net of this kind.
    pub net: Option<NetKind>,
    /// Marks repeated along the line.
    pub marker: LineMarker,
}

impl ConnectorKind {
    /// Draw `c` as this kind: its route, ends, dash, width, line colour, jumps and line marker
    /// are replaced and `kind` is set. Glue, text, bends, avoidance, net membership and every
    /// other style field stay.
    pub fn apply(&self, c: &mut Connector) {
        c.kind = self.id.to_string();
        c.route = self.route;
        c.start_arrow = self.start_arrow;
        c.end_arrow = self.end_arrow;
        c.jumps = self.jumps;
        c.style.dash = self.dash;
        c.style.line_width = self.line_width;
        c.style.line = self.line;
        c.marker = self.marker;
        c.sanitize();
    }
}

fn leak(s: &str) -> &'static str {
    s.to_string().leak()
}

/// A kind from a pack's connector type. The loader has already checked names and colours, so the
/// fallbacks only guard against a pack built without the loader.
fn kind_from(t: &ConnectorType) -> ConnectorKind {
    let keywords: Vec<&'static str> = t.keywords.iter().map(|k| leak(k)).collect();
    ConnectorKind {
        id: leak(&t.id),
        label: leak(&t.label),
        category: leak(&t.category),
        route: Route::from_name(&t.route).unwrap_or_default(),
        start_arrow: Arrow::from_name(&t.start_arrow).unwrap_or_default(),
        end_arrow: Arrow::from_name(&t.end_arrow).unwrap_or_default(),
        dash: Dash::from_name(&t.dash).unwrap_or_default(),
        line_width: t.line_width,
        line: Color::parse(&t.line).unwrap_or(Style::DEFAULT.line),
        jumps: t.jumps,
        keywords: keywords.leak(),
        relationship_type: t.relationship_type.as_deref().map(leak),
        net: t.net.as_deref().and_then(NetKind::from_name),
        marker: LineMarker::from_name(&t.marker).unwrap_or_default(),
    }
}

/// One kind per connector type of every pack, in catalog order. The strings are leaked once so
/// the API can hand out `&'static` data; this runs once per catalog (see [`built`]), and the
/// catalog is replaced a bounded number of times, so the leak is bounded.
fn build(cat: &Catalog) -> &'static [ConnectorKind] {
    let kinds: Vec<ConnectorKind> = cat.packs().iter().flat_map(|p| p.connector_types.iter()).map(kind_from).collect();
    kinds.leak()
}

/// The kinds and their search index, with the catalog stamp they were built from.
struct Built {
    stamp: usize,
    kinds: &'static [ConnectorKind],
    index: &'static [Entry],
}

/// The kinds of the current catalog, rebuilt when the catalog changes (a pack was loaded) the
/// way the stencil sets are, so a pack loaded at run time draws its kinds.
fn built() -> (&'static [ConnectorKind], &'static [Entry]) {
    static BUILT: Mutex<Option<Built>> = Mutex::new(None);
    let stamp = crate::catalog_stamp();
    let mut slot = BUILT.lock().unwrap_or_else(PoisonError::into_inner);
    match &*slot {
        Some(b) if b.stamp == stamp => (b.kinds, b.index),
        _ => {
            let kinds = build(vizcraft_notation::catalog());
            let index: &'static [Entry] = kinds.iter().map(entry).collect::<Vec<_>>().leak();
            *slot = Some(Built { stamp, kinds, index });
            (kinds, index)
        }
    }
}

/// Every connector kind, in display order.
pub fn connector_kinds() -> &'static [ConnectorKind] {
    built().0
}

/// The kind with this id (`basic.arrow`).
pub fn connector_kind(id: &str) -> Option<&'static ConnectorKind> {
    connector_kinds().iter().find(|k| k.id == id)
}

/// The kind a relationship type is drawn with under rendering variant `variant` (empty for the
/// default; see `Catalog::connector_type_for_relationship_in`).
pub fn connector_kind_for_relationship(rel_type: &str, variant: &str) -> Option<&'static ConnectorKind> {
    vizcraft_notation::catalog().connector_type_for_relationship_in(rel_type, variant).and_then(connector_kind)
}

/// How well a kind matches, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    /// The whole query is the label, the id, or the id's name part.
    Exact,
    /// Each query word starts a word of the id, label or category.
    WordPrefix,
    /// Each query word starts a keyword word (or a better tier above).
    Keyword,
    /// Some query word only appears inside the text.
    Substring,
}

/// The kind's searchable text, lowercased.
struct Entry {
    kind: &'static ConnectorKind,
    exact: [String; 3],
    names: Vec<String>,
    keywords: Vec<String>,
    hay: String,
}

fn entry(k: &'static ConnectorKind) -> Entry {
    let id = k.id.to_lowercase();
    let name = id.split_once('.').map_or(id.as_str(), |(_, n)| n).to_string();
    let label = k.label.to_lowercase();
    let mut names = words(k.id);
    names.extend(words(k.label));
    names.extend(words(k.category));
    let keywords: Vec<String> = k.keywords.iter().flat_map(|w| words(w)).collect();
    let hay = format!("{id} {label} {} {}", k.category.to_lowercase(), k.keywords.join(" ").to_lowercase());
    Entry { kind: k, exact: [id, label, name], names, keywords, hay }
}

fn index() -> &'static [Entry] {
    built().1
}

fn word_tier(e: &Entry, w: &str) -> Option<Tier> {
    if e.names.iter().any(|n| n.starts_with(w)) {
        Some(Tier::WordPrefix)
    } else if e.keywords.iter().any(|k| k.starts_with(w)) {
        Some(Tier::Keyword)
    } else if e.hay.contains(w) {
        Some(Tier::Substring)
    } else {
        None
    }
}

/// Kinds matching every whitespace-separated word of `query` (case-insensitive) in the id, label,
/// category or keywords, best match first (the exact label or id, then word prefixes, keywords,
/// substrings; ties keep catalog order), at most `limit`. An empty query gives the catalog in order.
pub fn search_connector_kinds(query: &str, limit: usize) -> Vec<&'static ConnectorKind> {
    let query: String = query.chars().take(MAX_QUERY_CHARS).collect::<String>().to_lowercase();
    let qwords: Vec<String> = query.split_whitespace().take(MAX_WORDS).map(|w| w.chars().take(MAX_WORD_CHARS).collect()).collect();
    let whole = qwords.join(" ");
    let mut hits: Vec<(Tier, usize, &'static ConnectorKind)> = index()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if !whole.is_empty() && e.exact.contains(&whole) {
                return Some((Tier::Exact, i, e.kind));
            }
            let mut tier = Tier::WordPrefix;
            for w in &qwords {
                tier = tier.max(word_tier(e, w)?);
            }
            Some((tier, i, e.kind))
        })
        .collect();
    hits.sort_by_key(|(t, i, _)| (*t, *i));
    hits.into_iter().take(limit).map(|(_, _, k)| k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::End;

    fn ids(q: &str) -> Vec<&'static str> {
        search_connector_kinds(q, usize::MAX).iter().map(|k| k.id).collect()
    }

    #[test]
    fn the_catalog_lists_the_basic_and_flowchart_kinds_in_order() {
        // Other packs add their kinds after these.
        let all: Vec<&str> = connector_kinds().iter().map(|k| k.id).filter(|id| id.starts_with("basic.") || id.starts_with("flowchart.")).collect();
        assert_eq!(all, ["basic.line", "basic.arrow", "basic.twoWay", "basic.dependency", "basic.bus", "basic.signal", "flowchart.flow"]);
        let dep = connector_kind("basic.dependency").unwrap();
        assert_eq!((dep.dash, dep.end_arrow, dep.start_arrow), (Dash::Dashed, Arrow::Open, Arrow::None));
        let bus = connector_kind("basic.bus").unwrap();
        assert_eq!((bus.line_width, bus.start_arrow, bus.end_arrow), (3.0, Arrow::None, Arrow::None));
        let sig = connector_kind("basic.signal").unwrap();
        assert_eq!((sig.line_width, sig.start_arrow, sig.end_arrow), (1.0, Arrow::Circle, Arrow::Open));
        let two = connector_kind("basic.twoWay").unwrap();
        assert_eq!((two.start_arrow, two.end_arrow), (Arrow::Triangle, Arrow::Triangle));
        let flow = connector_kind("flowchart.flow").unwrap();
        assert_eq!((flow.end_arrow, flow.relationship_type), (Arrow::Triangle, Some("flowchart.Flow")));
        assert!(connector_kind("vendor.nothing").is_none());
    }

    #[test]
    fn apply_sets_the_look_and_keeps_glue_text_and_bends() {
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.text = "label".into();
        c.waypoints = vec![vizcraft_diagram::Point::new(5.0, 6.0)];
        c.avoid = true;
        c.style.font_size = 20.0;
        connector_kind("basic.bus").unwrap().apply(&mut c);
        assert_eq!((c.kind.as_str(), c.style.line_width, c.end_arrow), ("basic.bus", 3.0, Arrow::None));
        assert_eq!((c.start.shape(), c.end.shape()), (Some("a"), Some("b")));
        assert_eq!((c.text.as_str(), c.waypoints.len(), c.avoid, c.style.font_size), ("label", 1, true, 20.0));
    }

    #[test]
    fn search_ranks_and_limits() {
        assert_eq!(ids("").len(), connector_kinds().len());
        assert_eq!(search_connector_kinds("", 3).len(), 3);
        assert_eq!(ids("arrow").first(), Some(&"basic.arrow"));
        assert_eq!(ids("dependency"), ["basic.dependency"]);
        assert_eq!(ids("thick"), ["basic.bus"]);
        assert_eq!(ids("flow").first(), Some(&"flowchart.flow"));
        assert!(ids("nothing-like-this").is_empty());
        assert!(search_connector_kinds("line", 0).is_empty());
    }

    #[test]
    fn hostile_queries_are_bounded() {
        assert!(ids(&"a ".repeat(100_000)).len() <= connector_kinds().len());
        assert!(ids(&"x".repeat(1_000_000)).is_empty());
        assert!(ids("\u{0}\u{FFFF}\u{1F600}").is_empty());
        let long = format!("{}{}", "é".repeat(MAX_QUERY_CHARS - 1), "ü".repeat(10));
        assert!(ids(&long).is_empty());
    }

    #[test]
    fn a_connector_type_with_a_net_kind_and_marker_carries_them() {
        let t = ConnectorType { id: "t.pneumatic".into(), net: Some("signal".into()), marker: "doubleSlash".into(), ..ConnectorType::default() };
        let k = kind_from(&t);
        assert_eq!((k.net, k.marker), (Some(NetKind::Signal), LineMarker::DoubleSlash));
        let mut c = Connector { net: Some("n1".into()), ..Connector::default() };
        k.apply(&mut c);
        assert_eq!((c.marker, c.net.as_deref()), (LineMarker::DoubleSlash, Some("n1")), "the net is kept");
        let junk = kind_from(&ConnectorType { net: Some("plasma".into()), marker: "zigzag".into(), ..ConnectorType::default() });
        assert_eq!((junk.net, junk.marker), (None, LineMarker::None));
        assert_eq!(connector_kind("basic.line").map(|k| (k.net, k.marker)), Some((None, LineMarker::None)));
    }
}
