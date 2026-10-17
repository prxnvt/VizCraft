#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Mermaid text to a typed diagram IR.
//!
//! This crate parses the text form of Mermaid diagrams into plain Rust data. It knows nothing
//! about VizCraft pages, shapes or connectors: in Cascade 3, `crates/formats` maps the IR onto
//! the diagram model and the auto-layout crate places the result. Keeping the parser a leaf
//! library means the desktop app, the CLI, the MCP server and the web build share one parser
//! with no file system, threads or clocks.
//!
//! The syntax is implemented from the public Mermaid documentation (mermaid.js.org) only.
//!
//! Entry point: [`parse`]. It detects the diagram type from the header line (after front matter,
//! `%%` comments and `%%{init}%%` directives), parses one of
//!
//! - flowcharts (`flowchart`, `graph`): [`flowchart::Flowchart`]
//! - state diagrams (`stateDiagram`, `stateDiagram-v2`): [`state::StateDiagram`]
//! - entity-relationship diagrams (`erDiagram`): [`er::ErDiagram`]
//! - class diagrams (`classDiagram`): [`class::ClassDiagram`]
//! - sequence diagrams (`sequenceDiagram`): [`sequence::SequenceDiagram`]
//!
//! and returns it with warnings for statements it recognised but does not support, or did not
//! recognise at all. Real syntax errors (an unclosed bracket, a link with no target, a block that
//! never ends) are a [`ParseError`] with a line and column. [`Diagram::to_graph`] gives one
//! uniform node, edge and group view of every type for the mapper and the layout engine.
//!
//! Input is untrusted: the text length, the number of statements, nodes and edges, and the block
//! nesting depth are capped ([`limits`]), and the parser never recurses, so hostile input ends in
//! an error rather than a crash or an unbounded allocation.

pub mod class;
mod common;
pub mod er;
pub mod flowchart;
pub mod graph;
pub mod sequence;
pub mod state;

use serde::{Deserialize, Serialize};

pub use common::{ClassDef, Click, Direction, Meta, ParseError, StyleProp, Warning};
pub use graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, LineStyle, Shape};

/// Hard limits that keep hostile input bounded.
pub mod limits {
    /// Largest accepted input, in bytes.
    pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
    /// Most statements one diagram may contain.
    pub const MAX_STATEMENTS: usize = 50_000;
    /// Most nodes (states, entities, classes, participants, notes) one diagram may contain.
    pub const MAX_NODES: usize = 50_000;
    /// Most edges (links, transitions, relationships, messages) one diagram may contain.
    pub const MAX_EDGES: usize = 50_000;
    /// Deepest nesting of subgraphs, composite states, namespaces or sequence blocks.
    pub const MAX_DEPTH: usize = 64;
    /// Most warnings kept; further ones are counted in a final summary warning.
    pub const MAX_WARNINGS: usize = 1_000;
}

/// One parsed diagram of any supported type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Diagram {
    /// A flowchart (`flowchart` or `graph`).
    Flowchart(flowchart::Flowchart),
    /// A state diagram (`stateDiagram` or `stateDiagram-v2`).
    State(state::StateDiagram),
    /// An entity-relationship diagram (`erDiagram`).
    Er(er::ErDiagram),
    /// A class diagram (`classDiagram`).
    Class(class::ClassDiagram),
    /// A sequence diagram (`sequenceDiagram`).
    Sequence(sequence::SequenceDiagram),
}

impl Diagram {
    /// Which kind of diagram this is.
    pub fn kind(&self) -> DiagramKind {
        match self {
            Diagram::Flowchart(_) => DiagramKind::Flowchart,
            Diagram::State(_) => DiagramKind::State,
            Diagram::Er(_) => DiagramKind::Er,
            Diagram::Class(_) => DiagramKind::Class,
            Diagram::Sequence(_) => DiagramKind::Sequence,
        }
    }

    /// Title and accessibility text shared by every diagram type.
    pub fn meta(&self) -> &Meta {
        match self {
            Diagram::Flowchart(d) => &d.meta,
            Diagram::State(d) => &d.meta,
            Diagram::Er(d) => &d.meta,
            Diagram::Class(d) => &d.meta,
            Diagram::Sequence(d) => &d.meta,
        }
    }

    /// The uniform node, edge and group view of this diagram.
    pub fn to_graph(&self) -> Graph {
        match self {
            Diagram::Flowchart(d) => d.to_graph(),
            Diagram::State(d) => d.to_graph(),
            Diagram::Er(d) => d.to_graph(),
            Diagram::Class(d) => d.to_graph(),
            Diagram::Sequence(d) => d.to_graph(),
        }
    }
}

/// A successfully parsed diagram plus everything the parser skipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parsed {
    /// The diagram.
    pub diagram: Diagram,
    /// Statements that were skipped, with their line numbers, in source order.
    pub warnings: Vec<Warning>,
}

/// Diagram headers Mermaid documents that this crate does not parse, so the error can say
/// "not supported" instead of "unknown".
const OTHER_MERMAID_TYPES: &[&str] = &[
    "gantt",
    "pie",
    "journey",
    "gitGraph",
    "mindmap",
    "timeline",
    "quadrantChart",
    "requirementDiagram",
    "C4Context",
    "C4Container",
    "C4Component",
    "C4Dynamic",
    "C4Deployment",
    "sankey-beta",
    "sankey",
    "xychart-beta",
    "xychart",
    "block-beta",
    "block",
    "packet-beta",
    "packet",
    "architecture-beta",
    "kanban",
    "radar-beta",
    "treemap-beta",
    "treemap",
    "zenuml",
];

/// Parse Mermaid text into a typed diagram.
///
/// The diagram type comes from the first line that is not front matter, a comment or a
/// directive. Statements that are not understood become [`Warning`]s; malformed constructs,
/// unsupported diagram types and inputs over the [`limits`] are a [`ParseError`].
pub fn parse(text: &str) -> Result<Parsed, ParseError> {
    let prepared = common::prepare(text)?;
    let mut ctx = common::Ctx::default();
    let header_line = prepared.header.no;
    let keyword = prepared.keyword;
    let diagram = match keyword {
        "flowchart" | "graph" | "flowchart-elk" => Diagram::Flowchart(flowchart::parse(prepared, &mut ctx)?),
        "stateDiagram" | "stateDiagram-v2" => Diagram::State(state::parse(prepared, &mut ctx)?),
        "erDiagram" => Diagram::Er(er::parse(prepared, &mut ctx)?),
        "classDiagram" | "classDiagram-v2" => Diagram::Class(class::parse(prepared, &mut ctx)?),
        "sequenceDiagram" => Diagram::Sequence(sequence::parse(prepared, &mut ctx)?),
        other if OTHER_MERMAID_TYPES.contains(&other) => {
            return Err(ParseError::new(
                header_line,
                prepared.header_col,
                format!(
                    "Mermaid `{other}` diagrams are not supported; supported types are flowchart, graph, stateDiagram, erDiagram, classDiagram and sequenceDiagram"
                ),
            ));
        }
        other => {
            return Err(ParseError::new(
                header_line,
                prepared.header_col,
                format!("unknown diagram type `{}`; expected a header such as `flowchart TD`", common::snippet(other)),
            ));
        }
    };
    Ok(Parsed { diagram, warnings: ctx.finish() })
}
