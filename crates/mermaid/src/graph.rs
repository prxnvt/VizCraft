//! The uniform view of every diagram type: nodes, edges and groups with shape and arrow hints.
//!
//! Each diagram type keeps its own detailed IR (class members, ER attributes, sequence order);
//! this view flattens them so the format mapper and the auto-layout engine can treat every type
//! the same way. An edge endpoint may name a group as well as a node, because Mermaid lets links
//! and transitions attach to subgraphs and composite states.

use serde::{Deserialize, Serialize};

use crate::common::{ClassDef, Direction, StyleProp};

/// Which Mermaid diagram type a graph came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramKind {
    /// `flowchart` or `graph`.
    Flowchart,
    /// `stateDiagram` or `stateDiagram-v2`.
    State,
    /// `erDiagram`.
    Er,
    /// `classDiagram`.
    Class,
    /// `sequenceDiagram`.
    Sequence,
    /// An architecture diagram (`architecture-beta`).
    Architecture,
}

/// A node shape hint. Flowchart shapes keep their Mermaid meaning; the other types use the
/// notation-specific variants at the end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Shape {
    /// Plain rectangle, `A[text]`.
    #[default]
    Rectangle,
    /// Rectangle with rounded corners, `A(text)`.
    Rounded,
    /// Pill shape, `A([text])`.
    Stadium,
    /// Rectangle with doubled side bars, `A[[text]]`.
    Subroutine,
    /// Database cylinder, `A[(text)]`.
    Cylinder,
    /// Circle, `A((text))`.
    Circle,
    /// Two concentric circles, `A(((text)))`.
    DoubleCircle,
    /// Flag-like asymmetric shape, `A>text]`.
    Asymmetric,
    /// Rhombus, `A{text}`.
    Diamond,
    /// Hexagon, `A{{text}}`.
    Hexagon,
    /// Parallelogram leaning right, `A[/text/]`.
    Parallelogram,
    /// Parallelogram leaning left, `A[\text\]`.
    ParallelogramAlt,
    /// Trapezoid wide at the bottom, `A[/text\]`.
    Trapezoid,
    /// Trapezoid wide at the top, `A[\text/]`.
    TrapezoidAlt,
    /// Document with a wavy bottom edge.
    Document,
    /// Stacked documents.
    MultiDocument,
    /// Document with a lined left edge.
    LinedDocument,
    /// Document with a folded corner tag.
    TaggedDocument,
    /// Small circle (start).
    SmallCircle,
    /// Circle inside a ring (stop).
    FramedCircle,
    /// Small filled circle (junction).
    FilledCircle,
    /// Circle with a cross (summary).
    CrossCircle,
    /// Cloud.
    Cloud,
    /// Triangle pointing up (extract).
    Triangle,
    /// Triangle pointing down (manual file).
    FlippedTriangle,
    /// Text with no border.
    Text,
    /// Rectangle with a notched corner (card).
    NotchedRectangle,
    /// Rectangle with a lined left edge.
    LinedRectangle,
    /// Stacked rectangles (multiple processes).
    StackedRectangle,
    /// Rectangle with a tag corner.
    TaggedRectangle,
    /// Rectangle divided by a top band.
    DividedRectangle,
    /// Rectangle with a window-pane cross (internal storage).
    InternalStorage,
    /// Rectangle with a sloped top (manual input).
    ManualInput,
    /// Half-rounded rectangle (delay).
    Delay,
    /// Cylinder lying on its side (direct access storage).
    HorizontalCylinder,
    /// Cylinder with a band (disk storage).
    LinedCylinder,
    /// Curved trapezoid (display).
    Display,
    /// Bow-tie rectangle (stored data).
    StoredData,
    /// Pentagon with notches (loop limit).
    LoopLimit,
    /// Hourglass (collate).
    Hourglass,
    /// Wavy flag (paper tape).
    Flag,
    /// Lightning bolt (communication link).
    Bolt,
    /// Curly brace comment.
    Brace,
    /// Bang or explosion outline.
    Bang,
    /// Thick bar (fork or join).
    ForkBar,
    /// State or flow start: a filled dot.
    Start,
    /// State or flow end: a ringed dot.
    End,
    /// State choice point: a small diamond.
    Choice,
    /// A note.
    Note,
    /// An ER entity box with attribute rows.
    Entity,
    /// An OMG UML class box with member rows.
    Class,
    /// A sequence participant box.
    Participant,
    /// A sequence actor figure.
    Actor,
    /// A shape name this crate does not know, kept as written.
    Other(String),
}

/// How an edge's line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LineStyle {
    /// Solid line.
    #[default]
    Solid,
    /// Dotted line (flowchart `-.->`).
    Dotted,
    /// Dashed line (class `..`, ER non-identifying, sequence `-->`).
    Dashed,
    /// Thick line (flowchart `==>`).
    Thick,
    /// Not drawn but still laid out (flowchart `~~~`).
    Invisible,
}

/// The marker at one end of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArrowKind {
    /// No marker.
    #[default]
    None,
    /// Filled arrowhead (flowchart, state, sequence sync) or open arrowhead (class association).
    Arrow,
    /// Open, stick arrowhead (sequence async `-)`).
    OpenArrow,
    /// Circle (flowchart `--o`).
    Circle,
    /// Cross (flowchart `--x`, sequence `-x`).
    Cross,
    /// Hollow triangle (class inheritance and realization).
    Triangle,
    /// Filled diamond (class composition).
    DiamondFilled,
    /// Hollow diamond (class aggregation).
    DiamondHollow,
    /// Lollipop interface (class `()--`).
    Lollipop,
    /// Crow's foot: exactly one.
    ExactlyOne,
    /// Crow's foot: zero or one.
    ZeroOrOne,
    /// Crow's foot: one or more.
    OneOrMore,
    /// Crow's foot: zero or more.
    ZeroOrMore,
}

/// What kind of container a group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupKind {
    /// Flowchart subgraph.
    Subgraph,
    /// Composite state.
    CompositeState,
    /// One concurrent region of a composite state.
    Region,
    /// Class diagram namespace.
    Namespace,
    /// Sequence diagram participant box.
    Box,
}

/// A node in the uniform view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique id within the diagram.
    pub id: String,
    /// Display text; may contain `\n`.
    pub label: String,
    /// Shape hint.
    pub shape: Shape,
    /// Stereotypes such as `interface` (class) or `choice` (state), without angle brackets.
    pub annotations: Vec<String>,
    /// Extra rows: class members, ER attributes, state descriptions.
    pub details: Vec<String>,
    /// Style class names applied to the node.
    pub classes: Vec<String>,
    /// Inline style properties in source order.
    pub style: Vec<StyleProp>,
    /// Innermost group holding the node.
    pub group: Option<String>,
    /// Click-through URL.
    pub link: Option<String>,
    /// Tooltip text.
    pub tooltip: Option<String>,
}

impl GraphNode {
    /// A node with only an id, label and shape.
    pub fn new(id: impl Into<String>, label: impl Into<String>, shape: Shape) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: label.into(),
            shape,
            annotations: Vec::new(),
            details: Vec::new(),
            classes: Vec::new(),
            style: Vec::new(),
            group: None,
            link: None,
            tooltip: None,
        }
    }
}

/// An edge in the uniform view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Optional edge id (flowchart `e1@-->`).
    pub id: Option<String>,
    /// Source node or group id.
    pub source: String,
    /// Target node or group id.
    pub target: String,
    /// Text on the edge.
    pub label: Option<String>,
    /// Text at the source end (class cardinality).
    pub source_label: Option<String>,
    /// Text at the target end (class cardinality).
    pub target_label: Option<String>,
    /// Line style.
    pub line: LineStyle,
    /// Marker at the source end.
    pub start: ArrowKind,
    /// Marker at the target end.
    pub end: ArrowKind,
    /// Requested length in ranks: 1 is normal, each extra dash in Mermaid adds one.
    pub length: u8,
    /// Inline style properties in source order (flowchart `linkStyle`).
    pub style: Vec<StyleProp>,
}

impl GraphEdge {
    /// A plain solid edge with no markers.
    pub fn new(source: impl Into<String>, target: impl Into<String>) -> GraphEdge {
        GraphEdge {
            id: None,
            source: source.into(),
            target: target.into(),
            label: None,
            source_label: None,
            target_label: None,
            line: LineStyle::Solid,
            start: ArrowKind::None,
            end: ArrowKind::None,
            length: 1,
            style: Vec::new(),
        }
    }
}

/// A container in the uniform view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphGroup {
    /// Unique id; never equal to a node id.
    pub id: String,
    /// Title text.
    pub label: String,
    /// What kind of container it is.
    pub kind: GroupKind,
    /// Enclosing group.
    pub parent: Option<String>,
    /// Ids of the nodes directly inside (child groups name this group as their parent).
    pub members: Vec<String>,
    /// Direction requested for the group's contents.
    pub direction: Option<Direction>,
    /// Style class names applied to the group.
    pub classes: Vec<String>,
    /// Inline style properties in source order.
    pub style: Vec<StyleProp>,
}

impl GraphGroup {
    /// An empty group.
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: GroupKind) -> GraphGroup {
        GraphGroup {
            id: id.into(),
            label: label.into(),
            kind,
            parent: None,
            members: Vec::new(),
            direction: None,
            classes: Vec::new(),
            style: Vec::new(),
        }
    }
}

/// The uniform view of one diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    /// Source diagram type.
    pub kind: DiagramKind,
    /// Requested overall direction.
    pub direction: Direction,
    /// Diagram title, when given.
    pub title: Option<String>,
    /// Nodes in source order.
    pub nodes: Vec<GraphNode>,
    /// Edges in source order.
    pub edges: Vec<GraphEdge>,
    /// Groups in the order they were opened (parents before children).
    pub groups: Vec<GraphGroup>,
    /// Style classes the nodes and groups refer to.
    pub class_defs: Vec<ClassDef>,
}

impl Graph {
    /// An empty graph.
    pub fn new(kind: DiagramKind, direction: Direction, title: Option<String>) -> Graph {
        Graph { kind, direction, title, nodes: Vec::new(), edges: Vec::new(), groups: Vec::new(), class_defs: Vec::new() }
    }

    /// The node with this id.
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The group with this id.
    pub fn group(&self, id: &str) -> Option<&GraphGroup> {
        self.groups.iter().find(|g| g.id == id)
    }
}
