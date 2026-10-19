//! VizCraft diagram model (model layer, below the engine).
//!
//! - [`geom`]: points, rectangles, affine transforms, paths of lines and cubics (`f64`, world
//!   units are pixels at 100% zoom, y down, angles in degrees clockwise on screen).
//! - [`kind`]: the built-in outlines ([`ShapeKind`]) and their default connection points.
//! - [`model`]: [`Document`], [`Page`], [`Shape`], [`Connector`] and their styles, all serde.
//! - [`hit`]: hit testing in world space.
//! - [`layer`]: what a shape's layers allow (drawing, selecting, printing, snapping, glue).
//! - [`arrows`]: the line ends ([`Arrow`]) and the one geometry table every renderer draws from.
//! - [`route`]: glue resolution and the three route styles.
//! - [`autolayout`]: automatic placement of shapes with `vizcraft-layout`.
//! - [`autoroute`]: connector routes that avoid shapes, with `vizcraft-routing`.
//! - [`net`]: wire segments, junctions, the net index and line labels; [`line_marks`]: the
//!   marks repeated along a line that say what it carries.
//! - [`jumps`]: line jumps, and the page-level connector geometry that draws them.
//! - [`views`]: shapes and connectors as views of `vizcraft-model` elements and relationships.
//! - [`data`]: typed shape data rows and the page's data key definitions.
//! - [`data_graphic`]: data graphics drawn around shapes, and links to external tables.
//! - [`container`]: containers, swimlanes (lane sets of lanes, with phases) and callouts.
//! - [`decor`]: page backgrounds and borders with title blocks, on generated background pages.
//! - [`theme`]: the built-in document themes, their variants and quick styles.
//! - [`shapeop`]: boolean operations, offset, join and trim, making freeform shapes.
//! - [`master`]: the document's own masters and the stencil file they export to.
//! - [`style_fields`], [`shape_fields`], [`page_fields`], [`doc_fields`]: the richer style, shape,
//!   page and document data (text layout, effects, links, locks, layers, comments, images).
//!
//! No egui and no engine: the engine, the UI, file formats and the MCP server all build on this.
//! Nothing here panics on hostile input; sizes are clamped and non-finite numbers replaced.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod arrange;
pub mod arrows;
pub mod autolayout;
pub mod autoroute;
pub mod board;
pub mod compartment;
pub mod conformance;
pub mod container;
pub mod data;
pub mod data_graphic;
pub mod decor;
pub mod doc_fields;
pub mod geom;
pub mod hit;
pub mod import_record;
pub mod ink;
pub mod jumps;
pub mod kind;
pub mod layer;
pub mod line_marks;
pub mod master;
pub mod model;
pub mod net;
pub mod page_fields;
pub mod print_fields;
pub mod route;
pub mod routing_policy;
mod serde_util;
pub mod shape_fields;
pub mod shapeop;
pub mod sheet;
pub mod snap;
pub mod style_fields;
pub mod text_fields;
pub mod theme;
pub mod title_block;
pub mod views;

pub use arrows::{ArrowGeometry, outline};
pub use board::Board;
pub use compartment::{Band, Compartment, CompartmentKind, Height};
pub use conformance::{Conformance, RuleOverride, Suppression};
pub use doc_fields::{Comment, CommentAnchor, DocProperties, ImageAsset, Protection, Reply};
pub use geom::{Path, PathEl, Point, Polyline, Rect, Transform, Vec2};
pub use hit::{BandHit, Hit, band_at, connection_point_at, connector_at, connectors_in_rect, glue_shape_at, hit, shape_at, shapes_in_rect};
pub use kind::ShapeKind;
pub use line_marks::LineMarker;
pub use master::CustomMaster;
pub use model::{Arrow, Color, Connector, CoreProps, Document, End, Page, Route, Shape, Style};
pub use model::{MAX_WAYPOINTS, is_plain_id};
pub use page_fields::{DrawingScale, Guide, GuideOrientation, HeaderFooter, Layer, Orientation, Units};
pub use route::{Arrowhead, ConnectorGeometry, EndPoint, Ends, arrowhead, connector_geometry, resolve_ends, route_path, route_polyline};
pub use routing_policy::{Avoid, RoutingPolicy, StraightFallback};
pub use serde_util::base64;
pub use shape_fields::{Behavior, CropRect, Freeform, Hyperlink, ImageRef, LinkKind, Locks, Orient, TextBlock};
pub use style_fields::{
    Bullets, Dash, Glow, Gradient, GradientKind, GradientStop, HAlign, LineCap, Margins, Shadow, TextCase, TextDirection, VAlign,
};
pub use views::ViewItems;
pub use vizcraft_model;

/// Largest width or height a shape or page may have, in world units.
pub const MAX_EXTENT: f64 = 1.0e6;

/// Longest shape or connector text kept, in bytes.
pub const MAX_TEXT: usize = 64 * 1024;

#[cfg(test)]
mod tests;
