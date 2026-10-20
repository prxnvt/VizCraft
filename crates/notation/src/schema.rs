//! The notation pack schema, version [`SCHEMA_VERSION`].
//!
//! A pack is one JSON or TOML document. Every id in it is pack-qualified: element types,
//! relationship types, viewpoints, identifier scopes and rules are `<pack>.<Name>`
//! (`flowchart.Decision`, `flowchart.Flow`); glyphs are `<pack>.<name>` and double as stencil
//! master ids (`flowchart.decision`). The prefix must be the pack's own id; a reference to
//! another pack's type is written with that pack's prefix. In lists of types, `*` means every
//! element type of this pack.
//!
//! Every field has a default so a pack lists only what it needs; the loader
//! ([`crate::loader`]) checks references and values and reports problems instead of panicking.

use serde::{Deserialize, Serialize};
use vizcraft_model::{IdScope, PropValue, Severity};

/// The pack schema version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// A notation pack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pack {
    /// Pack schema version ([`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Pack id: the type prefix (`flowchart`). Lowercase letters, digits and `_`.
    pub id: String,
    /// Pack version, `major.minor.patch`.
    pub version: String,
    /// Name shown in the palette and the Shapes pane (`Flowchart`).
    pub name: String,
    /// One-paragraph description for agents and the backstage.
    pub description: String,
    /// Audiences the pack serves (`general`, `embedded`, `enterprise`, `industrial`).
    pub audience: Vec<String>,
    /// Original glyphs; each is also a stencil master.
    pub glyphs: Vec<Glyph>,
    /// Connector types: named routes, line ends and line styles (`basic.arrow`).
    pub connector_types: Vec<ConnectorType>,
    /// Element types.
    pub element_types: Vec<ElementType>,
    /// Relationship types.
    pub relationship_types: Vec<RelationshipType>,
    /// The connection table: which relationships may join which element types.
    pub connections: Vec<Connection>,
    /// Style changes keyed on property values.
    pub conditional_styles: Vec<ConditionalStyle>,
    /// Palette grouping, in display order.
    pub palette: Vec<PaletteGroup>,
    /// Named filters of allowed types, one per kind of view.
    pub viewpoints: Vec<Viewpoint>,
    /// Identifier scopes for the identity registry.
    pub identifiers: Vec<IdScope>,
    /// Validation rules.
    pub rules: Vec<RuleDecl>,
    /// Named sets of rules a document can be held to (`flowchart.iso5807`); the first is the one
    /// a document gets when it starts using the pack.
    pub rulesets: Vec<Ruleset>,
    /// Document types the pack offers (see [`crate::template`]): empty documents set up for a kind
    /// of work. Old bare-string entries are dropped when the pack loads.
    #[serde(deserialize_with = "crate::template::entries")]
    pub templates: Vec<crate::template::Template>,
    /// Examples: filled-in sample documents (Help > Examples), the same shape as a document type
    /// plus the seed that draws them.
    #[serde(deserialize_with = "crate::template::entries", skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<crate::template::Template>,
    /// Pattern ids the pack supports (none yet).
    pub patterns: Vec<String>,
    /// Exporter ids the pack supports (none yet).
    pub exporters: Vec<String>,
    /// Rendering variants: other ways of drawing the same element and relationship types (Chen
    /// against crow's foot, Gane-Sarson against Yourdon). A page picks one in `View.variant`;
    /// none picked is the default drawing (each type's own glyph and connector type).
    pub variants: Vec<Variant>,
    /// The symbol grid module the pack's glyphs are drawn on (`iso2_5mm` for IEC engineering
    /// packs, `inch0_1` for ANSI ones); a page that takes the pack's viewpoint takes it as its
    /// `View.module`. None by default.
    #[serde(skip_serializing_if = "vizcraft_model::Module::is_none")]
    pub module: vizcraft_model::Module,
    /// Document properties the pack declares (the ISO 7200 title block fields): `doc.setProperty`
    /// accepts their keys, `doc.properties` lists them and title blocks show them through
    /// `{{doc:key}}` fields. `required` marks a mandatory field and `maxChars` its length limit.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub document_properties: Vec<PropertySchema>,
    /// Sections of a large pack (OMG UML's class, component, state machine and so on): each adds
    /// its lists to the pack's own when the pack loads (see [`crate::sections::flatten`]), so a
    /// loaded pack always has flat lists and no sections.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<PackSection>,
}

/// One section of a pack: lists appended to the pack's own when it loads. A ruleset whose id
/// the pack (or an earlier section) already declares extends that ruleset's rules.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PackSection {
    /// Section id: lowercase letters, digits and `_`, unique in the pack.
    pub id: String,
    /// Name of the section.
    pub label: String,
    /// Glyphs added.
    pub glyphs: Vec<Glyph>,
    /// Connector types added.
    pub connector_types: Vec<ConnectorType>,
    /// Element types added.
    pub element_types: Vec<ElementType>,
    /// Relationship types added.
    pub relationship_types: Vec<RelationshipType>,
    /// Connection-table rows added.
    pub connections: Vec<Connection>,
    /// Conditional styles added.
    pub conditional_styles: Vec<ConditionalStyle>,
    /// Palette groups added.
    pub palette: Vec<PaletteGroup>,
    /// Viewpoints added.
    pub viewpoints: Vec<Viewpoint>,
    /// Rules added.
    pub rules: Vec<RuleDecl>,
    /// Rulesets added, or extended when the id exists.
    pub rulesets: Vec<Ruleset>,
    /// Document types added.
    #[serde(deserialize_with = "crate::template::entries")]
    pub templates: Vec<crate::template::Template>,
    /// Examples added.
    #[serde(deserialize_with = "crate::template::entries")]
    pub examples: Vec<crate::template::Template>,
}

/// A rendering variant of a pack: which glyph draws each element type and which connector type
/// draws each relationship type, for the types it changes. A type it does not list keeps its
/// default drawing. Ids are pack-qualified like the types (`er.chen`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Variant {
    /// Variant id, `<pack>.<name>`.
    pub id: String,
    /// Name shown in the notation tab's Variant menu.
    pub label: String,
    /// One sentence on what changes.
    pub description: String,
    /// Element type id to glyph id.
    pub glyphs: std::collections::BTreeMap<String, String>,
    /// Relationship type id to connector type id.
    pub connectors: std::collections::BTreeMap<String, String>,
    /// The symbol grid module this variant draws on (an ANSI variant of an IEC pack says
    /// `inch0_1`); `variant.set` copies it onto the page. None keeps the page's module.
    #[serde(skip_serializing_if = "vizcraft_model::Module::is_none")]
    pub module: vizcraft_model::Module,
    /// Drawing options the sync pass reads on pages drawn with this variant, at most 16 short
    /// entries: `memberVisibility` (`symbols`, `keywords` or `hidden`) and `members`
    /// (`suppressed` leaves member rows empty).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub options: std::collections::BTreeMap<String, String>,
    /// Element types drawn as shapes of their own around their owner's view on pages with this
    /// variant (Chen draws an entity's attributes as ellipses around it); see
    /// `crate::satellites`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub satellites: Vec<String>,
    /// Compartments per element type that replace the type's own on pages with this variant; an
    /// empty list removes them (see [`crate::Catalog::compartments_in`]).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub compartments: std::collections::BTreeMap<String, Vec<CompartmentDecl>>,
    /// Other ids this variant answers to, `<pack>.<name>` each (`fluidpower.jic` for
    /// `fluidpower.ansi`: a project that picks JIC symbols draws fluid power in the ANSI Y32.10
    /// style). [`Pack::variant`] finds the variant by any of them, so a symbol family resolves
    /// through `crate::template::variant_for`; the Variant menu lists the variant once.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

impl Pack {
    /// The variant with this id, or the one that lists it among its [`Variant::aliases`].
    pub fn variant(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|v| v.id == id).or_else(|| self.variants.iter().find(|v| v.aliases.iter().any(|a| a == id)))
    }
    /// The element type with this qualified id.
    pub fn element_type(&self, id: &str) -> Option<&ElementType> {
        self.element_types.iter().find(|t| t.id == id)
    }
    /// The relationship type with this qualified id.
    pub fn relationship_type(&self, id: &str) -> Option<&RelationshipType> {
        self.relationship_types.iter().find(|t| t.id == id)
    }
    /// The glyph with this id (a stencil master id).
    pub fn glyph(&self, id: &str) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.id == id)
    }
    /// The connector type with this qualified id.
    pub fn connector_type(&self, id: &str) -> Option<&ConnectorType> {
        self.connector_types.iter().find(|t| t.id == id)
    }
    /// The rule with this qualified id.
    pub fn rule(&self, id: &str) -> Option<&RuleDecl> {
        self.rules.iter().find(|r| r.id == id)
    }
    /// The ruleset with this qualified id.
    pub fn ruleset(&self, id: &str) -> Option<&Ruleset> {
        self.rulesets.iter().find(|r| r.id == id)
    }
    /// The viewpoint with this qualified id.
    pub fn viewpoint(&self, id: &str) -> Option<&Viewpoint> {
        self.viewpoints.iter().find(|v| v.id == id)
    }
    /// Whether `type_id` is listed by `list`, where `*` stands for every element type of this pack.
    pub fn lists(&self, list: &[String], type_id: &str) -> bool {
        list.iter().any(|t| t == type_id || (t == "*" && self.element_type(type_id).is_some()))
    }
}

/// An original glyph: a built-in outline with a default size and our own colours. Glyphs become
/// the stencil masters of the Shapes pane (`vizcraft-stencils` builds its sets from them).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Glyph {
    /// Master id (`flowchart.decision`).
    pub id: String,
    /// Name shown in the Shapes pane.
    pub label: String,
    /// Category in the Shapes pane; categories show in order of first appearance.
    pub category: String,
    /// Outline: a `ShapeKind` name from `vizcraft-diagram` (`diamond`, `terminator`), `path`
    /// for an outline drawn by [`Glyph::path`], or `builder` for a symbol drawn by
    /// [`Glyph::builder`].
    pub outline: String,
    /// With outline `path`: the outline as SVG-style path data in a unit box (`0 0` to `1 1`):
    /// `M`, `L`, `C`, `Q` and `Z`, absolute coordinates. Closed subpaths fill even-odd, open ones
    /// are lines. Read by [`crate::glyph_path`]; the shape stretches it onto its frame.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// With outline `path`: extra lines over the fill in the same unit box (an inner rim, a
    /// cross), never filled. Optional.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Default width in world units.
    pub w: f64,
    /// Default height in world units.
    pub h: f64,
    /// Fill colour, `#rrggbb`.
    pub fill: String,
    /// Line colour, `#rrggbb`.
    pub line: String,
    /// Extra search words (the label and id are searched too).
    pub keywords: Vec<String>,
    /// Element type a shape placed from this glyph becomes; empty for free drawing.
    pub element_type: String,
    /// With outline `builder`: the name of the Rust builder that draws the symbol from the
    /// element's properties and the page's standard (`contact`, `valve`). See
    /// `vizcraft_stencils::builders`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub builder: String,
    /// With outline `builder`: the builder's parameter defaults for this glyph (a JIC contact
    /// is the `contact` builder with `standard` `jic`). The element's properties and the shape's
    /// own overrides win over these.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<String, PropValue>,
    /// Where the drawing came from, at most 256 bytes: `own design`, or `drawn from ...` naming
    /// the written description of the public notation it follows. Required on every path and
    /// builder glyph of a shipped pack.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The border a container type's views draw (dash, corner, heading); a glyph with one is a
    /// container master. See [`crate::nesting::ContainerLook`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<crate::nesting::ContainerLook>,
    /// The outline: `single` (empty is the same) or `double` (a second outline inset inside the
    /// first, an ER weak entity).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub compound: String,
    /// Where a shape placed from this glyph draws its text: `inside` (empty is the same) or
    /// `below` (under the frame, for a figure such as an actor).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text_place: String,
}

/// A connector type: a named route, pair of line ends and line style, applied to a connector as
/// its `kind`. Like a glyph it is a recipe: applying it copies the values onto the connector, and
/// later hand styling wins. Ids are `<pack>.<name>` (`basic.arrow`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnectorType {
    /// Qualified id (`basic.dependency`); stored on connectors as their `kind`.
    pub id: String,
    /// Name shown in the Kind gallery.
    pub label: String,
    /// Category in the gallery; categories show in order of first appearance.
    pub category: String,
    /// Route: a `Route` name from `vizcraft-diagram` (`straight`, `orthogonal`, `curved`).
    pub route: String,
    /// Line end at the start: an `Arrow` name from `vizcraft-diagram`.
    pub start_arrow: String,
    /// Line end at the end: an `Arrow` name from `vizcraft-diagram`.
    pub end_arrow: String,
    /// Dash pattern: a `Dash` name from `vizcraft-diagram` (`solid`, `dashed`).
    pub dash: String,
    /// Line width.
    pub line_width: f64,
    /// Line colour, `#rrggbb`.
    pub line: String,
    /// Draw a hop wherever the connector crosses one drawn before it.
    pub jumps: bool,
    /// Extra search words (the label and id are searched too).
    pub keywords: Vec<String>,
    /// Relationship type a connector of this kind becomes when it joins two bound shapes; `None`
    /// lets the pack's connection table choose.
    pub relationship_type: Option<String>,
    /// The net kind a connector of this kind is a segment of (`wire`, `potential`, `pipe`,
    /// `fluid`, `signal`); the net pass gives such a connector a net. `None`: an ordinary
    /// connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    /// Marks repeated along the line: a `LineMarker` name from `vizcraft-diagram` (`none`,
    /// `doubleSlash`, `cross`, `l`, `circle`, `wave`).
    #[serde(skip_serializing_if = "is_none_marker")]
    pub marker: String,
    /// The frame around the label: a `LabelFrame` name from `vizcraft-diagram` (`box`, `diamond`,
    /// `doubleDiamond`); empty is the box.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label_frame: String,
    /// Ends read from the relationship's properties (ER cardinality): the end pass
    /// (`crate::end_pass`) gives each end of a connector of this kind the look its rules pick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends: Option<EndRules>,
}

/// The rules that pick a connector's end looks from its relationship's properties: for each end,
/// rules applied in order over the kind's own look, a later rule's fields winning.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndRules {
    /// Rules for the start (source) end and the half of the line nearest it.
    pub start: Vec<EndRule>,
    /// Rules for the end (target) end and the half of the line nearest it.
    pub end: Vec<EndRule>,
}

/// One end rule: the look for each value of a relationship property (its schema default when
/// unset). A value the rule does not list changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndRule {
    /// Property key read.
    pub property: String,
    /// Property value (as text) to the look.
    pub values: std::collections::BTreeMap<String, EndLook>,
    /// Apply the rule only when another property has one of some values (Chen's `M` at the
    /// source of a many-to-many relationship).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<EndCondition>,
}

/// The condition of an [`EndRule`]: `property` (or its schema default) is one of `any`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndCondition {
    /// Property key read.
    pub property: String,
    /// Values that let the rule apply.
    pub any: Vec<String>,
}

/// What an end rule sets; `None` fields are left as earlier rules (or the kind) set them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndLook {
    /// The line end: an `Arrow` name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow: Option<String>,
    /// The text beside the end (`1`, `N`, `P`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The dash of this end's half: a `Dash` name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    /// Draw this end's half as a double line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double: Option<bool>,
}

fn is_none_marker(m: &String) -> bool {
    m.is_empty() || m == "none"
}

impl Default for ConnectorType {
    fn default() -> Self {
        ConnectorType {
            id: String::new(),
            label: String::new(),
            category: String::new(),
            route: "orthogonal".into(),
            start_arrow: "none".into(),
            end_arrow: "none".into(),
            dash: "solid".into(),
            line_width: 1.5,
            line: "#3a4a5c".into(),
            jumps: false,
            keywords: Vec::new(),
            relationship_type: None,
            net: None,
            marker: "none".into(),
            label_frame: String::new(),
            ends: None,
        }
    }
}

/// The value type of a property.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropKind {
    /// Free text.
    #[default]
    Text,
    /// Any finite number.
    Number,
    /// A whole number.
    Integer,
    /// `true` or `false`.
    Bool,
    /// One of `values`.
    Enum,
}

/// One property of an element or relationship type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertySchema {
    /// Property key (`reference`), letters, digits and `_`.
    pub key: String,
    /// Label in the Inspector.
    pub label: String,
    /// Value type.
    pub kind: PropKind,
    /// Unit shown beside numbers (`min`, `bar`), empty for none.
    pub unit: String,
    /// Allowed values of an enum.
    pub values: Vec<String>,
    /// The property must be set (a missing or empty value raises the `requiredProperty` check).
    pub required: bool,
    /// Value a new element starts with.
    pub default: Option<PropValue>,
    /// What the property means.
    pub description: String,
    /// Most characters the value may have (a title block cell's room); `None` for no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,
    /// Group the Inspector shows the property under (`Identification`); empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
}

/// Direction of a port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortDirection {
    /// Relationships end here.
    In,
    /// Relationships start here.
    Out,
    /// Either.
    #[default]
    InOut,
}

/// A named connection place on an element type. Port `i` is drawn at the shape's connection
/// point `i`, so a connector glued to point `i` attaches its relationship to this port.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Port {
    /// Port id, unique within the type (`in1`).
    pub id: String,
    /// Label.
    pub label: String,
    /// Direction.
    pub direction: PortDirection,
    /// Position across the frame, `0..1`.
    pub x: f64,
    /// Position down the frame, `0..1`.
    pub y: f64,
}

/// An element type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ElementType {
    /// Qualified id (`flowchart.Decision`).
    pub id: String,
    /// Display name.
    pub label: String,
    /// What it means, for agents and the Inspector.
    pub description: String,
    /// Glyph id its views are drawn with (`flowchart.decision`).
    pub glyph: String,
    /// Property schema.
    pub properties: Vec<PropertySchema>,
    /// Ports; empty means the shape's connection points carry no port meaning.
    pub ports: Vec<Port>,
    /// Elements of this type may own other elements in the containment tree.
    pub container: bool,
    /// The supertype (`process.Equipment` for `process.Pump`), possibly in another pack. A type
    /// list in a rule or the connection table that names the supertype covers this type too, and
    /// properties and ports the type does not declare come from the supertype.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Compartments every view of this type carries, filled from the element's properties by
    /// the sync pass (see `crate::bind`); empty for none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub compartments: Vec<CompartmentDecl>,
    /// The types an element of this type may sit in, by drawing or in the model: qualified
    /// container type ids (supertypes count), `*` for any, `top` for no owner. Empty: the type
    /// does not nest by drawing. See [`crate::nesting`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_parents: Vec<String>,
    /// A keyword the views show as `«keyword»` on a line above the name (`interface`), at most
    /// 64 bytes; an element's own `stereotype` property wins over it. Empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub keyword: String,
}

/// A compartment an element type declares for its views (`vizcraft_diagram::compartment`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompartmentDecl {
    /// The compartment id on the shape (`attrs`); ports name it.
    pub id: String,
    /// Paragraph, list or slot.
    pub kind: vizcraft_diagram::CompartmentKind,
    /// Heading drawn at the top of the band.
    pub title: Option<String>,
    /// Property keys whose values fill the compartment, one `name: value` row (or line) each,
    /// in this order; a key the element has no value for (and no schema default) is left out.
    pub from_properties: Vec<String>,
    /// A table filled from the model instead (a revision table, a parts list): one row per
    /// element the query selects, cells separated by tabs (see `crate::sheet_pass`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<TableQuery>,
    /// Typed rows: the text of this property, one row per line, each read as a member row and
    /// written in the page's member style (see `crate::uml_text`). Empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rows_from: String,
}

/// A table a list compartment shows: elements of `types`, one row each, with `columns` read as
/// in `vizcraft_model::query::list` (`tag`, `name` or a property key).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TableQuery {
    /// Element types listed (pack-qualified). An element of the compartment's own type may
    /// override them with its `types` property (comma-separated).
    pub types: Vec<String>,
    /// Column keys, in order.
    pub columns: Vec<String>,
    /// Header titles, one per column (the column key when missing).
    pub headers: Vec<String>,
    /// Column widths in world units, one per column (an even share when missing).
    pub widths: Vec<f64>,
    /// The column key rows are sorted by (numbers numerically, revisions in order); empty keeps
    /// model order.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort_by: String,
    /// List only elements the compartment's element owns (an entity's columns).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub owned: bool,
    /// List only elements whose property (or its schema default) equals a value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<RowFilter>,
    /// Per column key, cell text to the text shown (`pk` `true` shows `PK`); a value the map
    /// does not list shows empty.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub values: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

/// The filter of a [`TableQuery`]: `property` (or its schema default) equals `equals`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RowFilter {
    /// Property key compared.
    pub property: String,
    /// The value it must have; `None`: any value.
    pub equals: Option<PropValue>,
}

impl ElementType {
    /// The property schema for `key`.
    pub fn property(&self, key: &str) -> Option<&PropertySchema> {
        self.properties.iter().find(|p| p.key == key)
    }
}

/// A relationship type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RelationshipType {
    /// Qualified id (`flowchart.Flow`).
    pub id: String,
    /// Display name.
    pub label: String,
    /// What it means.
    pub description: String,
    /// Property schema.
    pub properties: Vec<PropertySchema>,
    /// Arrowhead drawn at the target end: an `Arrow` name from `vizcraft-diagram`.
    pub end_arrow: String,
    /// The relationship type this one is the inverse of (`partOf` for `hasPart`): a relationship
    /// of this type from A to B also counts as one of that type from B to A wherever rules follow
    /// relationships (reachability, paths, cycles, islands).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inverse_of: Option<String>,
    /// What the relationship means to drawings: a cross reference labels both ends with each
    /// other's sheet and zone (see `crate::sheet_pass`).
    #[serde(skip_serializing_if = "is_plain")]
    pub kind: RelationshipKind,
    /// A keyword the connector shows as `«keyword»` above its name (`use`, `include`), at most
    /// 64 bytes. Empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub keyword: String,
    /// Properties shown beside the ends of its connectors (multiplicities, roles).
    #[serde(skip_serializing_if = "EndLabels::is_empty")]
    pub end_labels: EndLabels,
}

/// The properties a connector shows beside its ends: the values of the listed keys, joined by a
/// space (role first, then multiplicity). Editing an end label writes it back: the last word to
/// the last key, and so on, the first key taking what is left.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndLabels {
    /// Keys shown at the start (source) end.
    pub start: Vec<String>,
    /// Keys shown at the end (target) end.
    pub end: Vec<String>,
}

impl EndLabels {
    /// Whether no key is shown at either end.
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.end.is_empty()
    }
}

/// What a relationship type means to drawings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipKind {
    /// An ordinary relationship.
    #[default]
    Plain,
    /// A cross reference: each end shows where the other is.
    CrossReference,
}

fn is_plain(k: &RelationshipKind) -> bool {
    *k == RelationshipKind::Plain
}

impl Default for RelationshipType {
    fn default() -> Self {
        RelationshipType {
            id: String::new(),
            label: String::new(),
            description: String::new(),
            properties: Vec::new(),
            end_arrow: "triangle".into(),
            inverse_of: None,
            kind: RelationshipKind::Plain,
            keyword: String::new(),
            end_labels: EndLabels::default(),
        }
    }
}

impl RelationshipType {
    /// The property schema for `key`.
    pub fn property(&self, key: &str) -> Option<&PropertySchema> {
        self.properties.iter().find(|p| p.key == key)
    }
}

/// One row of the connection table: a relationship of type `relationship` may go from an element
/// whose type is in `source` to one whose type is in `target`. A relationship no row allows is
/// illegal (the `illegalRelationship` check).
///
/// Multiplicity, checked by the `multiplicity` check: for each source element, the number of its
/// outgoing relationships of this type whose target type this row lists is at most `outMax`;
/// for each target element, the number of its incoming relationships of this type whose source
/// type this row lists is at most `inMax`. `None` means no limit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Connection {
    /// Source element types (`*`: every type of this pack).
    pub source: Vec<String>,
    /// Relationship type.
    pub relationship: String,
    /// Target element types (`*`: every type of this pack).
    pub target: Vec<String>,
    /// Most matching outgoing relationships per source element.
    pub out_max: Option<u32>,
    /// Most matching incoming relationships per target element.
    pub in_max: Option<u32>,
}

/// A style change for the views of elements whose property has a value. The named style fields
/// of bound shapes change only when the match changes: when this style starts matching (it is
/// the first matching style naming the field), its value is set; when no style matches any more,
/// the glyph's value comes back. A new element or view gets the matching values once. Otherwise
/// the fields are left alone, so hand styling wins. Fields left `None` are not touched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConditionalStyle {
    /// Element types it applies to.
    pub types: Vec<String>,
    /// Property key compared.
    pub property: String,
    /// Value that triggers the style.
    pub equals: Option<PropValue>,
    /// Fill colour, `#rrggbb`.
    pub fill: Option<String>,
    /// Line colour, `#rrggbb`.
    pub line: Option<String>,
    /// Text colour, `#rrggbb`.
    pub font_color: Option<String>,
    /// Line width.
    pub line_width: Option<f64>,
    /// Italic text (an abstract class name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
}

/// A palette group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PaletteGroup {
    /// Qualified id (`flowchart.steps`).
    pub id: String,
    /// Group heading.
    pub label: String,
    /// Element types offered, in order.
    pub types: Vec<String>,
    /// Glyphs offered as free drawing, in order (packs without element types).
    pub glyphs: Vec<String>,
}

/// A viewpoint: the element and relationship types a view may show.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Viewpoint {
    /// Qualified id (`flowchart.Flowchart`).
    pub id: String,
    /// Display name.
    pub label: String,
    /// What the view is for.
    pub description: String,
    /// Allowed element types (`*`: every type of this pack; empty: any type).
    pub types: Vec<String>,
    /// Allowed relationship types (empty: any).
    pub relationships: Vec<String>,
}

/// When a rule runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleMode {
    /// After every command that changes the document. Must be cheap.
    #[default]
    Live,
    /// On request (`validate.run`) over a selection, page or the whole project.
    Batch,
}

/// A validation rule: a check from the fixed vocabulary below, with its parameters, a severity
/// and a mode. Rules are data so packs can add them without code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleDecl {
    /// Qualified id (`flowchart.illegalFlow`); issues carry it.
    pub id: String,
    /// What the rule enforces, in plain words; issue messages start from it.
    pub description: String,
    /// How bad a finding is.
    pub severity: Severity,
    /// When it runs.
    pub mode: RuleMode,
    /// The check and its parameters.
    pub check: Check,
    /// The standard and clause the rule comes from (`ISO 5807:1985, 9.2`), empty when it is a
    /// house rule; `validate.explain` returns it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reference: String,
}

impl Default for RuleDecl {
    fn default() -> Self {
        RuleDecl {
            id: String::new(),
            description: String::new(),
            severity: Severity::Warning,
            mode: RuleMode::Live,
            check: Check::Unknown,
            reference: String::new(),
        }
    }
}

/// A named set of rules a document can be held to. A document's conformance profile lists
/// ruleset ids; only the rules they list run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Ruleset {
    /// Qualified id (`flowchart.iso5807`).
    pub id: String,
    /// Display name (`ISO 5807 flowchart`).
    pub label: String,
    /// What holding a document to it means.
    pub description: String,
    /// Rule ids, pack-qualified; a rule of another pack may be listed.
    pub rules: Vec<String>,
}

/// The shape [`Check::FlowBalance`] expects of an element's flows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowExpect {
    /// One in, several out (a fork): more incoming than outgoing is reported.
    #[default]
    FanOut,
    /// Several in, one out (a join): more outgoing than incoming is reported.
    FanIn,
}

/// How [`Check::PropertyCompare`] compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareOp {
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Equal.
    #[default]
    Eq,
    /// Greater than or equal.
    Ge,
    /// Greater than.
    Gt,
    /// Not equal.
    Ne,
}

impl CompareOp {
    /// The operator as a message shows it.
    pub fn symbol(self) -> &'static str {
        match self {
            CompareOp::Lt => "less than",
            CompareOp::Le => "at most",
            CompareOp::Eq => "equal to",
            CompareOp::Ge => "at least",
            CompareOp::Gt => "greater than",
            CompareOp::Ne => "different from",
        }
    }
}

/// The checks a rule can run. `types` lists use pack-qualified ids, `*` for every type of the
/// pack, and an empty list for every element type of the pack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Check {
    /// A relationship of this pack that no connection-table row allows.
    IllegalRelationship,
    /// A connection-table `outMax` or `inMax` exceeded.
    Multiplicity,
    /// A required property of an element (or relationship) of these types missing or empty.
    RequiredProperty {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
    },
    /// A property value that does not fit its schema (wrong kind, enum value not allowed).
    PropertyType {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
    },
    /// A tag used too often in an identifier scope (identity registry `duplicates`).
    DuplicateIdentifier {
        /// Scope id.
        #[serde(default)]
        scope: String,
    },
    /// A paired tag with no partner (identity registry `unmatched`).
    UnmatchedPair {
        /// Scope id.
        #[serde(default)]
        scope: String,
    },
    /// An element drawn on a page whose viewpoint does not allow its type.
    ViewpointType,
    /// A connector with a free end on a page that shows elements of this pack.
    DanglingConnector,
    /// An element of these types with fewer than `min` outgoing relationships of `relationship`.
    MinOutgoing {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Relationship type counted.
        #[serde(default)]
        relationship: String,
        /// Fewest allowed.
        #[serde(default)]
        min: u32,
    },
    /// An element of these types with fewer than `min` incoming relationships of `relationship`.
    MinIncoming {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Relationship type counted.
        #[serde(default)]
        relationship: String,
        /// Fewest allowed.
        #[serde(default)]
        min: u32,
    },
    /// An element of these types that has the wrong share of incoming and outgoing relationships
    /// of `relationship` for the way it is meant to be used: with `expect` `fanOut` (a fork)
    /// more incoming than outgoing is reported, with `fanIn` (a join) more outgoing than incoming.
    /// Elements with none of either are skipped.
    FlowBalance {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Relationship type counted.
        #[serde(default)]
        relationship: String,
        /// The shape the element is expected to have.
        #[serde(default)]
        expect: FlowExpect,
    },
    /// An element of these types with both incoming and outgoing relationships of `relationship`.
    ExclusiveDirections {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Relationship type counted.
        #[serde(default)]
        relationship: String,
    },
    /// An element of this pack not reachable along `relationship` from any element whose type is
    /// in `from` that has no incoming relationship of that type (a start).
    Unreachable {
        /// Start types.
        #[serde(default)]
        from: Vec<String>,
        /// Relationship type followed (empty: every relationship).
        #[serde(default)]
        relationship: String,
        /// Elements checked (empty: every element of the pack).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        types: Vec<String>,
    },
    /// A cycle along `relationship` (one issue per cycle, with every element and relationship in
    /// it). Inverse relationship types count, reversed.
    ForbiddenCycle {
        /// Relationship type followed (empty: every relationship).
        #[serde(default)]
        relationship: String,
    },
    /// An element of a `from` type from which no walk along `relationship` reaches an element of
    /// a `to` type.
    RequiredPath {
        /// Types the path starts at.
        #[serde(default)]
        from: Vec<String>,
        /// Types the path must reach.
        #[serde(default)]
        to: Vec<String>,
        /// Relationship type followed (empty: every relationship).
        #[serde(default)]
        relationship: String,
    },
    /// Elements of these types fall apart into islands along `relationship` (either direction):
    /// one issue per island other than the largest.
    Connected {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Relationship type followed (empty: every relationship).
        #[serde(default)]
        relationship: String,
    },
    /// An element owned (in the containment tree) by an element of an `owners` type whose own
    /// type is not in `children`.
    Containment {
        /// Owner types the rule is about.
        #[serde(default)]
        owners: Vec<String>,
        /// Types those owners may own.
        #[serde(default)]
        children: Vec<String>,
    },
    /// A relationship of this type that starts at a port that is not an `out` port or ends at
    /// one that is not an `in` port (`inOut` ports allow both).
    PortDirection {
        /// Relationship type checked (empty: every relationship).
        #[serde(default)]
        relationship: String,
    },
    /// A port that more than `max` relationships of this type end at (at most n drivers).
    PortFanIn {
        /// Relationship type counted (empty: every relationship).
        #[serde(default)]
        relationship: String,
        /// Most allowed.
        #[serde(default)]
        max: u32,
    },
    /// Two elements of these types with the same value of `property` under one owner: the
    /// nearest ancestor whose type is in `within`, or the direct owner when `within` is empty.
    UniqueProperty {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Property key compared.
        #[serde(default)]
        property: String,
        /// Owner types that bound the uniqueness (empty: the direct owner).
        #[serde(default)]
        within: Vec<String>,
    },
    /// An element of these types whose `property` does not compare as `op` against `other`:
    /// text naming a property the element's type declares compares the two properties
    /// (design pressure at least operating pressure); anything else is a constant. Elements
    /// missing either value are skipped (`requiredProperty` reports those).
    PropertyCompare {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Property key on the left.
        #[serde(default)]
        property: String,
        /// The comparison.
        #[serde(default)]
        op: CompareOp,
        /// A property key or a constant on the right.
        #[serde(default)]
        other: Option<PropValue>,
    },
    /// An element of these types whose name does not match `pattern`, a glob: `*` any run, `?`
    /// one character, `[a-z]` or `[!0-9]` a class, `\` escapes. Unnamed elements are skipped.
    NameFormat {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// The glob.
        #[serde(default)]
        pattern: String,
    },
    /// Drawing: a connector whose drawn route runs through a shape it is not glued to.
    CrossesShape,
    /// Drawing: a connector with an end that is not glued to a shape.
    UngluedEnd,
    /// Drawing: two solid shapes whose frames overlap, neither inside the other as its group,
    /// container or lane.
    Overlap,
    /// Drawing: a shape not wholly on the page.
    OffBoard,
    /// Drawing: text taller than its block at the shape's font size.
    TextOverflow,
    /// Sheets: a document property the pack declares `required` left empty, or with `length`, a
    /// value longer than its `maxChars`. Runs when a page has a sheet.
    DocumentFields {
        /// Check lengths instead of mandatory fields.
        #[serde(default)]
        length: bool,
    },
    /// Sheets: revision records (`sheet.Revision`) whose letters skip or repeat a step of the
    /// revision scheme.
    RevisionSequence,
    /// Sheets: a balloon that calls out no part (or several), or a listed part with no balloon.
    BalloonRows,
    /// Sheets: a cross-reference relationship with an end that is drawn nowhere, so its label
    /// cannot say where the partner is.
    CrossReferences,
    /// Sheets: a tag composed from its sheet, zone, rung or owner that no longer matches (the
    /// element moved); `sheet.renumber` recomposes it.
    StaleTag,
    /// An element of these types whose owner's type its `allowedParents` does not list (or with
    /// no owner where `top` is not listed). With `drawn`, also a view inside a container shape
    /// whose element's type is not allowed. See [`crate::nesting`].
    NestingAllowed {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Also check the drawing: the nearest container shape around each view.
        #[serde(default)]
        drawn: bool,
    },
    /// An element or relationship of these types with an empty name.
    Labelled {
        /// Types checked (element or relationship types; empty: every type of the pack).
        #[serde(default)]
        types: Vec<String>,
    },
    /// A relationship of this type with an end on no port, where that end's type declares ports.
    EndsOnPorts {
        /// Relationship type checked (empty: every relationship of the pack).
        #[serde(default)]
        relationship: String,
    },
    /// A relationship of this type whose connector ends do not show what its `property` says:
    /// the first marker whose `value` equals the property (or its default) names the arrow each
    /// end should have. The fix runs `connector.setArrows`.
    EndMarkerMatchesProperty {
        /// Relationship type checked.
        #[serde(default)]
        relationship: String,
        /// Property key read.
        #[serde(default)]
        property: String,
        /// Value to ends.
        #[serde(default)]
        markers: Vec<EndMarker>,
    },
    /// A multiplicity that does not read (`n`, `*`, `n..m`, `n..*` with n at most m): in the
    /// `properties` of elements and relationships of these types, and in the member rows of
    /// their `members` properties.
    MultiplicitySyntax {
        /// Element or relationship types checked (empty: every type of the pack).
        #[serde(default)]
        types: Vec<String>,
        /// Property keys holding one multiplicity each.
        #[serde(default)]
        properties: Vec<String>,
        /// Property keys holding member rows, one per line.
        #[serde(default)]
        members: Vec<String>,
    },
    /// More than `max` elements of these types under one owner: the nearest ancestor whose type
    /// is in `within`; elements with none are grouped by the page they are first drawn on.
    MaxPerOwner {
        /// Types counted.
        #[serde(default)]
        types: Vec<String>,
        /// Owner types.
        #[serde(default)]
        within: Vec<String>,
        /// Most allowed.
        #[serde(default)]
        max: u32,
    },
    /// Two member rows of one element's `property` naming the same attribute (operations may
    /// overload, so rows with a parameter list are skipped).
    UniqueMembers {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Property key holding member rows.
        #[serde(default)]
        property: String,
    },
    /// On a sequence page: a message drawn above an earlier one or on its row, a message to a
    /// created lifeline before its create message or after a lifeline's destroy, an unbalanced
    /// fragment or a second operand where the operator takes one, a message drawn outside its
    /// operand (see `vizcraft_diagram::sequence::check_order`).
    MessageOrder,
    /// On a sequence page, operands taken as paths: a reply with no open synchronous call from
    /// its receiver to its sender, a call answered twice, and with `requireReplies` a call never
    /// answered (see `vizcraft_diagram::sequence::check_replies`).
    RepliesMatched {
        /// Report calls never answered too.
        #[serde(default)]
        require_replies: bool,
    },
    /// An element of these types (on pages whose viewpoint is in `viewpoints`, all pages when
    /// empty) that owns no element marked `pk`: a table without a primary key.
    PrimaryKey {
        /// Types checked.
        #[serde(default)]
        types: Vec<String>,
        /// Viewpoints whose pages are checked (empty: every page).
        #[serde(default)]
        viewpoints: Vec<String>,
    },
    /// A column of these types whose `references` (`<entity>.<column>`) names no column, or a
    /// column whose `dataType` differs from the one it references (a warning-level finding).
    ForeignKey {
        /// Column types checked.
        #[serde(default)]
        types: Vec<String>,
    },
    /// A test over the nets of the drawing (`vizcraft_diagram::net::NetIndex`): `test` names it
    /// and the other fields are its parameters. Fluid power adds `openPort` (a fluid port of a
    /// component of `types` on no line and not listed in its `plugged` property; exhaust ports of
    /// pneumatic valves may stay open), run by [`crate::fluid::rules`].
    NetCheck {
        /// The test.
        #[serde(default)]
        test: String,
        /// Element types checked (empty: every type of the pack).
        #[serde(default)]
        types: Vec<String>,
        /// Net kinds looked at (empty: every kind).
        #[serde(default)]
        kinds: Vec<String>,
        /// Connector type looked at (empty: every type).
        #[serde(default)]
        connector_type: String,
        /// Property read by the test.
        #[serde(default)]
        property: String,
        /// Element types allowed on the other end.
        #[serde(default)]
        peers: Vec<String>,
        /// Most allowed (0: no limit).
        #[serde(default)]
        max: u32,
    },
    /// Fluid power: an actuator that no pressure source (pump, compressor, pressure source or
    /// accumulator) reaches through the lines and any valve position (see [`crate::fluid`]).
    FluidPressurePath,
    /// Fluid power: a fixed displacement pump whose pressure port reaches no relief valve through
    /// lines and pass-through parts (never through a directional valve).
    FluidRelief,
    /// Fluid power: a hydraulic pump whose suction port reaches no reservoir.
    FluidSupply,
    /// Fluid power: a directional valve not drawn at rest (a spring returned two-position valve
    /// rests in the square on the spring side, a three-position valve in the middle).
    FluidRestPosition,
    /// Fluid power: a valve whose port labels are not its medium's set (ISO 9461 letters for
    /// hydraulic, ISO 5599 numbers for pneumatic).
    FluidPortSet,
    /// Fluid power: a pilot line with no end on a pilot port, or a drain line with no end on a
    /// drain port or a reservoir.
    FluidLineEnds,
    /// Fluid power: a component code that does not read as the page edition's code form
    /// (`{installation}{circuit}{letter}{n}` or the numeric form) for its circuit and class.
    FluidCodes,
    /// Fluid power: a displacement-step chart row naming no actuator, or a step no valve position
    /// can drive; with `idle`, a step in which no actuator moves.
    FluidSteps {
        /// Report idle steps instead.
        #[serde(default)]
        idle: bool,
    },
    /// A check this build does not know (from a newer pack); skipped, and reported by the loader.
    #[default]
    #[serde(other)]
    Unknown,
}

/// One row of [`Check::EndMarkerMatchesProperty`]: for this property value, the arrows the
/// connector's ends should have (`None`: either end is fine).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EndMarker {
    /// The property value.
    pub value: String,
    /// Arrow name at the start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// Arrow name at the end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}
