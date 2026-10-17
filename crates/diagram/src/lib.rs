//! VizCraft diagram model (model layer, below the engine).
//!
//! - [`geom`]: points, rectangles, affine transforms, paths of lines and cubics (`f64`, world
//!   units are pixels at 100% zoom, y down, angles in degrees clockwise on screen).
//! - [`kind`]: the built-in outlines ([`ShapeKind`]) and their default connection points.
//! - [`model`]: [`Document`], [`Page`], [`Shape`], [`Connector`] and their styles, all serde.
//! - [`hit`]: hit testing in world space.
//! - [`route`]: glue resolution, the three route styles and arrowhead geometry.
//! - [`autolayout`]: automatic placement of shapes with `vizcraft-layout`.
//! - [`autoroute`]: connector routes that avoid shapes, with `vizcraft-routing`.
//!
//! No egui and no engine: the engine, the UI, file formats and the MCP server all build on this.
//! Nothing here panics on hostile input; sizes are clamped and non-finite numbers replaced.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod autolayout;
pub mod autoroute;
pub mod geom;
pub mod hit;
pub mod kind;
pub mod model;
pub mod route;
mod serde_util;

pub use geom::{Path, PathEl, Point, Polyline, Rect, Transform, Vec2};
pub use hit::{Hit, connection_point_at, connector_at, connectors_in_rect, hit, shape_at, shapes_in_rect};
pub use kind::ShapeKind;
pub use model::{Arrow, Color, Connector, CoreProps, Document, End, Page, Route, Shape, Style};
pub use model::{MAX_WAYPOINTS, is_plain_id};
pub use route::{Arrowhead, ConnectorGeometry, EndPoint, Ends, arrowhead, connector_geometry, resolve_ends, route_path, route_polyline};

/// Largest width or height a shape or page may have, in world units.
pub const MAX_EXTENT: f64 = 1.0e6;

/// Longest shape or connector text kept, in bytes.
pub const MAX_TEXT: usize = 64 * 1024;

#[cfg(test)]
mod tests;
