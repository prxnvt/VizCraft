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
    /// Template ids the pack supports (none yet).
    pub templates: Vec<String>,
    /// Pattern ids the pack supports (none yet).
    pub patterns: Vec<String>,
    /// Exporter ids the pack supports (none yet).
    pub exporters: Vec<String>,
}

impl Pack {
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
    /// Outline: a `ShapeKind` name from `vizcraft-diagram` (`diamond`, `terminator`).
    pub outline: String,
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
}

impl Default for RelationshipType {
    fn default() -> Self {
        RelationshipType { id: String::new(), label: String::new(), description: String::new(), properties: Vec::new(), end_arrow: "triangle".into() }
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

/// A style change for the views of elements whose property has a value. For bound shapes the
/// named style fields are controlled by the pack: when any conditional style of the element's
/// type matches, its value wins; otherwise the glyph's value applies. Fields left `None` are not
/// controlled.
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
}

impl Default for RuleDecl {
    fn default() -> Self {
        RuleDecl { id: String::new(), description: String::new(), severity: Severity::Warning, mode: RuleMode::Live, check: Check::Unknown }
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
        /// Relationship type followed.
        #[serde(default)]
        relationship: String,
    },
    /// A check this build does not know (from a newer pack); skipped, and reported by the loader.
    #[default]
    #[serde(other)]
    Unknown,
}
