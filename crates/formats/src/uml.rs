//! A page read as UML: the elements it draws with their members and owners, and the
//! relationships between them. The PlantUML and Mermaid class and state writers start here.
//!
//! A shape bound to a model element is read from the model. An unbound shape that carries
//! `import.type` (a page imported without merging) is read from its `ext` data instead, so a
//! freshly imported page writes back the same way a merged one does. Owners come from the model
//! (`Element::owner`) or, for unbound shapes, from the nearest ancestor shape (`Shape::parent`)
//! that is read too. Owners that are not drawn on the page but own something that is are read as
//! well (marked `drawn: false`), so a subject or state machine frame can be written around its
//! members.

use std::collections::{BTreeMap, HashMap, HashSet};

use vizcraft_diagram::{Document, Page};
use vizcraft_notation::uml_text::{self, Member};

use crate::FormatError;
use crate::csv::{PROP_PREFIX, TYPE_KEY};

/// Most elements read from one page.
pub const MAX_ITEMS: usize = 20_000;

/// Deepest owner chain followed.
const MAX_DEPTH: usize = 64;

/// One UML element of the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UmlElement {
    /// Element id (the shape id for an unbound shape).
    pub id: String,
    /// Element type (`uml.Class`).
    pub type_id: String,
    /// Name.
    pub name: String,
    /// Owner among the elements read, if any.
    pub owner: Option<String>,
    /// Properties as text.
    pub properties: BTreeMap<String, String>,
    /// Whether a shape on the page draws it.
    pub drawn: bool,
}

impl UmlElement {
    /// A property's text, or empty.
    pub fn prop(&self, key: &str) -> &str {
        self.properties.get(key).map(String::as_str).unwrap_or_default()
    }
    /// The member rows of property `key`.
    pub fn members(&self, key: &str) -> Vec<Member> {
        uml_text::rows(self.prop(key)).take(vizcraft_diagram::compartment::MAX_ROWS).map(uml_text::parse_member).collect()
    }
    /// The local type name (`Class` for `uml.Class`).
    pub fn local(&self) -> &str {
        self.type_id.rsplit_once('.').map(|(_, l)| l).unwrap_or(&self.type_id)
    }
}

/// One relationship between elements of the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UmlRelationship {
    /// Relationship id (the connector id for an unbound connector).
    pub id: String,
    /// Relationship type (`uml.Generalization`).
    pub type_id: String,
    /// Source element id.
    pub source: String,
    /// Target element id.
    pub target: String,
    /// Name.
    pub name: String,
    /// Properties as text.
    pub properties: BTreeMap<String, String>,
}

impl UmlRelationship {
    /// A property's text, or empty.
    pub fn prop(&self, key: &str) -> &str {
        self.properties.get(key).map(String::as_str).unwrap_or_default()
    }
    /// The local type name.
    pub fn local(&self) -> &str {
        self.type_id.rsplit_once('.').map(|(_, l)| l).unwrap_or(&self.type_id)
    }
}

/// A page read as UML.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UmlPage {
    /// The page's viewpoint id, empty for none.
    pub viewpoint: String,
    /// The page name.
    pub name: String,
    /// Elements in model order (unbound shapes after them, in page order).
    pub elements: Vec<UmlElement>,
    /// Relationships whose ends are both read.
    pub relationships: Vec<UmlRelationship>,
}

impl UmlPage {
    /// The element with this id.
    pub fn element(&self, id: &str) -> Option<&UmlElement> {
        self.elements.iter().find(|e| e.id == id)
    }
    /// The elements owned directly by `owner` (`None`: the top level), in order.
    pub fn children<'a>(&'a self, owner: Option<&'a str>) -> impl Iterator<Item = &'a UmlElement> + 'a {
        self.elements.iter().filter(move |e| e.owner.as_deref() == owner)
    }
    /// Whether any element has one of these local type names.
    pub fn has_any(&self, locals: &[&str]) -> bool {
        self.elements.iter().any(|e| e.type_id.starts_with("uml.") && locals.contains(&e.local()))
    }
}

/// A shape or connector text without a leading `«keyword»` line.
fn name_of(text: &str) -> String {
    vizcraft_notation::bind::strip_keyword(text).trim().to_string()
}

/// Read page `index` of `doc` as UML.
pub fn read(doc: &Document, index: usize) -> Result<UmlPage, FormatError> {
    let page = doc.pages.get(index).ok_or(FormatError::NoPage(index))?;
    Ok(read_page(doc, page))
}

fn read_page(doc: &Document, page: &Page) -> UmlPage {
    let model = &doc.model;
    let mut out =
        UmlPage { viewpoint: page.view.as_ref().map(|v| v.viewpoint.clone()).unwrap_or_default(), name: page.name.clone(), ..UmlPage::default() };
    // Bound shapes: the elements they draw, in model order.
    let drawn: HashSet<&str> = page.shapes.iter().filter_map(|s| s.element.as_deref()).collect();
    let mut wanted: HashSet<&str> = drawn.clone();
    for id in &drawn {
        let mut at = model.element(id).and_then(|e| e.owner.as_deref());
        for _ in 0..MAX_DEPTH {
            let Some(o) = at else { break };
            if !wanted.insert(o) {
                break;
            }
            at = model.element(o).and_then(|e| e.owner.as_deref());
        }
    }
    for e in model.elements.iter().filter(|e| wanted.contains(e.id.as_str())).take(MAX_ITEMS) {
        out.elements.push(UmlElement {
            id: e.id.clone(),
            type_id: e.type_id.clone(),
            name: e.name.clone(),
            owner: e.owner.clone().filter(|o| wanted.contains(o.as_str())),
            properties: e.properties.iter().map(|(k, v)| (k.clone(), v.display())).collect(),
            drawn: drawn.contains(e.id.as_str()),
        });
    }
    // Unbound shapes with import data.
    let unbound: HashMap<&str, &vizcraft_diagram::Shape> =
        page.shapes.iter().filter(|s| s.element.is_none() && s.ext.contains_key(TYPE_KEY)).map(|s| (s.id.as_str(), s)).collect();
    let parents: HashMap<&str, Option<&str>> = page.shapes.iter().map(|s| (s.id.as_str(), s.parent.as_deref())).collect();
    for s in page.shapes.iter().filter(|s| unbound.contains_key(s.id.as_str())) {
        if out.elements.len() >= MAX_ITEMS {
            break;
        }
        let mut owner = None;
        let mut at = s.parent.as_deref();
        for _ in 0..MAX_DEPTH {
            let Some(p) = at else { break };
            if p == s.id {
                break;
            }
            if unbound.contains_key(p) {
                owner = Some(p.to_string());
                break;
            }
            at = parents.get(p).copied().flatten();
        }
        out.elements.push(UmlElement {
            id: s.id.clone(),
            type_id: s.ext.get(TYPE_KEY).cloned().unwrap_or_default(),
            name: name_of(&s.text),
            owner,
            properties: s.ext.iter().filter_map(|(k, v)| k.strip_prefix(PROP_PREFIX).map(|k| (k.to_string(), v.clone()))).collect(),
            drawn: true,
        });
    }
    let read: HashSet<String> = out.elements.iter().map(|e| e.id.clone()).collect();
    // Relationships between elements read: from the model, then unbound typed connectors.
    for r in model.relationships.iter().filter(|r| read.contains(&r.source) && read.contains(&r.target)).take(MAX_ITEMS) {
        out.relationships.push(UmlRelationship {
            id: r.id.clone(),
            type_id: r.type_id.clone(),
            source: r.source.clone(),
            target: r.target.clone(),
            name: r.name.clone(),
            properties: r.properties.iter().map(|(k, v)| (k.clone(), v.display())).collect(),
        });
    }
    for c in page.connectors.iter().filter(|c| c.relationship.is_none() && c.ext.contains_key(TYPE_KEY)) {
        let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
        if !unbound.contains_key(a) || !unbound.contains_key(b) || out.relationships.len() >= MAX_ITEMS {
            continue;
        }
        out.relationships.push(UmlRelationship {
            id: c.id.clone(),
            type_id: c.ext.get(TYPE_KEY).cloned().unwrap_or_default(),
            source: a.to_string(),
            target: b.to_string(),
            name: name_of(&c.text),
            properties: c.ext.iter().filter_map(|(k, v)| k.strip_prefix(PROP_PREFIX).map(|k| (k.to_string(), v.clone()))).collect(),
        });
    }
    out
}

/// The writer family a page belongs to: its viewpoint, else the kinds of element it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Class, component, package, deployment and use case diagrams: declarations and links.
    Structure,
    /// Activity diagrams.
    Activity,
    /// State machine diagrams.
    State,
}

/// The local names of the state machine types.
pub const STATE_TYPES: &[&str] =
    &["State", "Initial", "Final", "Choice", "Junction", "Fork", "Join", "ShallowHistory", "DeepHistory", "Terminate", "Region"];

/// The local names of the activity types.
pub const ACTIVITY_TYPES: &[&str] = &[
    "InitialNode",
    "ActivityFinal",
    "FlowFinal",
    "Action",
    "ObjectNode",
    "Decision",
    "ForkNode",
    "JoinNode",
    "SendSignal",
    "AcceptEvent",
    "TimeEvent",
];

impl UmlPage {
    /// Which writer family the page belongs to.
    pub fn family(&self) -> Family {
        match self.viewpoint.as_str() {
            "uml.StateMachineDiagram" => Family::State,
            "uml.ActivityDiagram" => Family::Activity,
            v if v.starts_with("uml.") => Family::Structure,
            _ if self.has_any(STATE_TYPES) => Family::State,
            _ if self.has_any(ACTIVITY_TYPES) => Family::Activity,
            _ => Family::Structure,
        }
    }
}

/// `text` on one line, cut to `max` bytes at a character boundary.
pub(crate) fn one_line(text: &str, max: usize) -> String {
    let mut out: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if out.len() > max {
        let mut end = max;
        while end > 0 && !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
    out
}

/// An identifier for text formats: letters, digits and `_`, unique among `used`.
pub(crate) fn alias(id: &str, used: &mut HashSet<String>) -> String {
    let mut base: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).take(64).collect();
    if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
        base.insert(0, 'n');
    }
    let mut out = base.clone();
    let mut n = 1u32;
    while !used.insert(out.clone()) {
        n = n.saturating_add(1);
        out = format!("{base}_{n}");
        if n == u32::MAX {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::{Document, Page};

    use super::*;

    /// Compare `text` with golden file `name` under `testdata/uml/`; `VIZCRAFT_UPDATE_GOLDEN=1`
    /// writes it first.
    fn golden(name: &str, text: &str) {
        let path = format!("{}/testdata/uml/{name}", env!("CARGO_MANIFEST_DIR"));
        if std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, text).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} exists (run with VIZCRAFT_UPDATE_GOLDEN=1 to create it)"));
        assert_eq!(text, want, "{name} changed; refresh the golden file if that was intended");
    }

    fn source(name: &str) -> String {
        std::fs::read_to_string(format!("{}/testdata/uml/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn doc_of(page: Page) -> Document {
        let mut d = Document::new();
        d.pages = vec![page];
        d
    }

    fn imported(text: &str) -> (Document, UmlPage) {
        let imp = crate::mermaid::import(text).unwrap();
        let doc = doc_of(imp.pages.into_iter().next().unwrap());
        let p = read(&doc, 0).unwrap();
        (doc, p)
    }

    /// Read, write, read: the golden text, and the same model both times.
    fn round_trip(name: &str) -> UmlPage {
        let (doc, first) = imported(&source(&format!("{name}.mmd")));
        let (out, warnings) = crate::mermaid::export(&doc, 0).unwrap();
        golden(&format!("{name}.out.mmd"), &out);
        let (_, second) = imported(&out);
        // The same model: order may differ (the writer groups members under their owners), and
        // a relationship's id is the connector's.
        let model = |p: &UmlPage| {
            let mut e = p.elements.clone();
            e.sort_by(|a, b| a.id.cmp(&b.id));
            let mut r: Vec<UmlRelationship> = p.relationships.iter().cloned().map(|r| UmlRelationship { id: String::new(), ..r }).collect();
            r.sort_by(|a, b| (&a.source, &a.target, &a.type_id, &a.name).cmp(&(&b.source, &b.target, &b.type_id, &b.name)));
            (e, r)
        };
        assert_eq!(model(&first), model(&second), "{warnings:?}");
        first
    }

    #[test]
    fn mermaid_class_reads_as_uml_and_round_trips() {
        let p = round_trip("class");
        let get = |id: &str| p.element(id).unwrap();
        assert_eq!(get("Item").type_id, "uml.Class");
        assert_eq!(get("Item").prop("isAbstract"), "true");
        assert_eq!(get("Item").prop("attributes"), "+title: String\n#copies: Integer");
        assert_eq!(get("Item").prop("operations"), "+isAvailable(): Boolean");
        assert_eq!(get("Lendable").type_id, "uml.Interface");
        assert_eq!(get("Status").type_id, "uml.Enumeration");
        assert_eq!(get("Status").prop("literals"), "ACTIVE\nOVERDUE\nRETURNED");
        assert_eq!(get("Loan").prop("operations"), "+renew(days: Integer): Date\n{static} +count(): Integer");
        assert_eq!(get("Member").prop("attributes"), "+name: String\n-email: String [0..1]");
        assert_eq!((get("people").type_id.as_str(), get("Member").owner.as_deref()), ("uml.Package", Some("people")));
        let rel = |t: &str| p.relationships.iter().find(|r| r.type_id == t).unwrap();
        let g = rel("uml.Generalization");
        assert_eq!((g.source.as_str(), g.target.as_str()), ("Book", "Item"), "`A <|-- B` is B specializing A");
        assert_eq!((rel("uml.Realization").source.as_str(), rel("uml.Realization").target.as_str()), ("Book", "Lendable"));
        let c = rel("uml.Composition");
        assert_eq!((c.source.as_str(), c.target.as_str()), ("Item", "Status"), "the diamond end is the whole");
        let borrows = p.relationships.iter().find(|r| r.name == "borrows").unwrap();
        assert_eq!((borrows.prop("sourceMultiplicity"), borrows.prop("targetMultiplicity"), borrows.prop("navigability")), ("1", "0..*", "target"));
    }

    #[test]
    fn mermaid_state_reads_as_uml_and_round_trips() {
        let p = round_trip("state");
        let get = |id: &str| p.element(id).unwrap();
        assert_eq!(get("initial").type_id, "uml.Initial");
        assert_eq!(get("final").type_id, "uml.Final");
        assert_eq!((get("moving_initial").type_id.as_str(), get("moving_initial").owner.as_deref()), ("uml.Initial", Some("moving")));
        assert_eq!(get("opening").owner.as_deref(), Some("moving"));
        assert_eq!(get("check").type_id, "uml.Choice");
        assert_eq!(get("closed").prop("entry"), "lock");
        assert_eq!(get("open").prop("do"), "beep");
        let t = p.relationships.iter().find(|r| r.name.starts_with("button")).unwrap();
        assert_eq!((t.prop("trigger"), t.prop("guard")), ("button", "authorized"));
        let t = p.relationships.iter().find(|r| r.name.contains("log")).unwrap();
        assert_eq!((t.prop("guard"), t.prop("effect")), ("fully closed", "log"));
    }

    #[test]
    fn hostile_pages_write_without_panicking() {
        let mut page = Page::new("p1", "x");
        for (i, t) in ["uml.Class", "uml.State", "uml.Initial", "uml.Action", "uml.Package", "nope", ""].iter().enumerate() {
            let mut s = vizcraft_diagram::Shape::new(format!("s{i}"), vizcraft_diagram::ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
            s.ext.insert(TYPE_KEY.into(), (*t).into());
            s.ext.insert(format!("{PROP_PREFIX}attributes"), "\"]{(\n[[[\n+x: [5..2]".into());
            s.text = "\u{ab}\n\"\u{0}".repeat(50);
            s.parent = Some(format!("s{}", (i + 1) % 7));
            page.shapes.push(s);
        }
        page.connectors.push(vizcraft_diagram::Connector::new("c", vizcraft_diagram::End::dynamic("s0"), vizcraft_diagram::End::dynamic("s0")));
        let doc = doc_of(page);
        assert!(read(&doc, 0).is_ok());
        assert!(read(&doc, 5).is_err());
        let _ = crate::mermaid::export(&doc, 0);
        assert!(crate::plantuml::write(&doc, 0).is_ok());
    }
}
