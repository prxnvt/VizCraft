//! Layout primitives: fixed drawings that the patterns and the audience packs lay their shapes
//! out with, where the general algorithms would only guess.
//!
//! Each primitive is a pure function from a [`Graph`] plus the roles its nodes play to a
//! [`Layout`] with the same normalised coordinates, bounds and notes as [`crate::layout`]. Roles
//! name nodes by index; a primitive refuses roles that are out of range, repeated, missing or
//! contradictory with [`LayoutError::InvalidRoles`] rather than guessing. Edges are routed as
//! straight segments (or orthogonal ones with [`Options::orthogonal_edges`]) between the placed
//! rectangles, and groups get their padded boxes, as in [`crate::layout`].
//!
//! | Primitive | Module | Drawing |
//! |---|---|---|
//! | [`Primitive::Bands`] | `band` | horizontal or vertical bands (swimlanes): nodes laid out layered across the whole graph, each kept inside its band, bands stacked with a gap |
//! | [`Primitive::Pipeline`] | `chain` | a chain of steps along the flow, with an optional parallel band that fans out between two consecutive steps and joins again |
//! | [`Primitive::Chain`] | `chain` | a two-row chain: forward steps on the first row and each step's compensation beside it on the second |
//! | [`Primitive::Matrix`] | `grid` | rows by columns in the given order with equal cells |
//! | [`Primitive::Timeline`] | `grid` | events on one axis by order with equal spacing, labels beside them |
//! | [`Primitive::Nested`] | `container` | a tree of containers packed recursively, children laid out layered inside their container's padding, containers sized to their content |
//! | [`Primitive::HubSpoke`] | `radial` | one hub, spokes on a circle whose radius clears the hub and the spokes |
//! | [`Primitive::Rings`] | `radial` | concentric rings of nodes around a centre |
//! | [`Primitive::Voter`] | `radial` | N channel chains side by side feeding one voter node after them |
//! | [`Primitive::Sequence`] | `crate::sequence` | OMG UML sequence lifelines: heads in a row left to right, the edges in index order as calls down the page |
//! | [`Primitive::Er`] | `er` | entities layered parents first with orthogonal edges and end-marker gaps, satellites on a ring around their owner |

mod band;
mod chain;
mod container;
pub mod er;
mod grid;
mod radial;

use serde::{Deserialize, Serialize};

use crate::geom::{self, Bounds};
use crate::{Direction, Graph, Layout, LayoutError, Notes, Options, Point, Rect, Resize, Sub, SubEdge};
pub use er::Satellite;

/// A parallel band of a pipeline: `members` fan out after step `from` and join again at `to`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parallel<T = usize> {
    /// The step the band fans out from.
    pub from: T,
    /// The step the band joins at; it must follow `from` directly among the steps.
    pub to: T,
    /// The band's nodes, stacked across the flow in this order.
    pub members: Vec<T>,
}

/// One container of the nested-container primitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nest<T = usize> {
    /// The container's own node; the layout gives it a new size ([`Layout::resized`]).
    pub id: T,
    /// The nodes directly inside it, which may be containers themselves.
    #[serde(default)]
    pub members: Vec<T>,
    /// Space between the container's edge and its content (default 20).
    #[serde(default)]
    pub padding: Option<f64>,
    /// Extra space above the content for a heading (default 0).
    #[serde(default)]
    pub header: Option<f64>,
}

/// A layout primitive and the roles of its nodes. As JSON it is adjacently tagged:
/// `{"primitive": "hubSpoke", "roles": {"hub": 0}}`. `T` is how roles name nodes: indices for
/// [`layout_primitive`], anything else (diagram ids) for a caller to map with [`Primitive::try_map`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "primitive", content = "roles", rename_all = "camelCase")]
pub enum Primitive<T = usize> {
    /// Steps in node order along the flow, with an optional parallel band.
    Pipeline {
        /// The parallel band; its members are not steps.
        #[serde(default)]
        parallel: Option<Parallel<T>>,
    },
    /// Bands stacked across the flow; every node belongs to exactly one band.
    Bands {
        /// The nodes of each band, the bands in stacking order.
        bands: Vec<Vec<T>>,
    },
    /// A two-row chain: forward steps and, below each, its compensation.
    Chain {
        /// The forward steps in order.
        forward: Vec<T>,
        /// Per forward step (by position), its compensation step or `null` for none.
        #[serde(default)]
        compensation: Vec<Option<T>>,
    },
    /// A matrix with equal cells, given by rows or by columns (exactly one of the two).
    Matrix {
        /// The cells row by row, each row left to right; `null` leaves a cell empty.
        #[serde(default)]
        rows: Option<Vec<Vec<Option<T>>>>,
        /// The cells column by column, each column top to bottom; `null` leaves a cell empty.
        #[serde(default)]
        cols: Option<Vec<Vec<Option<T>>>>,
    },
    /// A tree of containers, packed recursively.
    Nested {
        /// The containers; a node that is no container's member is on the top level.
        containers: Vec<Nest<T>>,
    },
    /// One hub with every other node on a circle around it.
    HubSpoke {
        /// The hub.
        hub: T,
    },
    /// Concentric rings around a centre; every node is the centre or on one ring.
    Rings {
        /// The centre.
        center: T,
        /// The rings from the inside out, each in angular order from the top, clockwise.
        rings: Vec<Vec<T>>,
    },
    /// Events on one axis with equal spacing, each optionally with a label beside it.
    Timeline {
        /// The events in order (default: every node that is not a label, in node order).
        #[serde(default)]
        events: Option<Vec<T>>,
        /// Per event (by position), its label or `null` for none.
        #[serde(default)]
        labels: Vec<Option<T>>,
    },
    /// N channels side by side, each a chain, all feeding one voter after them.
    Voter {
        /// The channels, each a chain of steps in order.
        channels: Vec<Vec<T>>,
        /// The node the channels feed.
        voter: T,
    },
    /// Lifeline heads in a row, the edges in index order as synchronous calls.
    Sequence {
        /// The lifelines left to right (default: node order); nodes left out follow.
        #[serde(default)]
        lifelines: Option<Vec<T>>,
    },
    /// An entity-relationship drawing: entities layered parents first, satellites around their
    /// owners.
    Er {
        /// The entities with nodes drawn around them (Chen attributes); empty for none.
        #[serde(default)]
        satellites: Vec<Satellite<T>>,
    },
}

impl<T> Primitive<T> {
    /// The primitive's name as it appears in JSON.
    pub fn name(&self) -> &'static str {
        match self {
            Primitive::Pipeline { .. } => "pipeline",
            Primitive::Bands { .. } => "bands",
            Primitive::Chain { .. } => "chain",
            Primitive::Matrix { .. } => "matrix",
            Primitive::Nested { .. } => "nested",
            Primitive::HubSpoke { .. } => "hubSpoke",
            Primitive::Rings { .. } => "rings",
            Primitive::Timeline { .. } => "timeline",
            Primitive::Voter { .. } => "voter",
            Primitive::Sequence { .. } => "sequence",
            Primitive::Er { .. } => "er",
        }
    }

    /// The same roles with every node reference passed through `f` (for example diagram ids
    /// mapped to node indices); the first error stops the mapping.
    pub fn try_map<U, E>(self, f: &mut impl FnMut(T) -> Result<U, E>) -> Result<Primitive<U>, E> {
        let list = |v: Vec<T>, f: &mut dyn FnMut(T) -> Result<U, E>| v.into_iter().map(f).collect::<Result<Vec<U>, E>>();
        let opt = |v: Vec<Option<T>>, f: &mut dyn FnMut(T) -> Result<U, E>| {
            v.into_iter().map(|x| x.map(&mut *f).transpose()).collect::<Result<Vec<_>, E>>()
        };
        let mut g = |t: T| f(t);
        Ok(match self {
            Primitive::Pipeline { parallel } => Primitive::Pipeline {
                parallel: match parallel {
                    Some(p) => Some(Parallel { from: g(p.from)?, to: g(p.to)?, members: list(p.members, &mut g)? }),
                    None => None,
                },
            },
            Primitive::Bands { bands } => Primitive::Bands { bands: bands.into_iter().map(|b| list(b, &mut g)).collect::<Result<_, _>>()? },
            Primitive::Chain { forward, compensation } => {
                Primitive::Chain { forward: list(forward, &mut g)?, compensation: opt(compensation, &mut g)? }
            }
            Primitive::Matrix { rows, cols } => {
                let mut grid =
                    |v: Option<Vec<Vec<Option<T>>>>| v.map(|v| v.into_iter().map(|r| opt(r, &mut g)).collect::<Result<Vec<_>, E>>()).transpose();
                let rows = grid(rows)?;
                let cols = grid(cols)?;
                Primitive::Matrix { rows, cols }
            }
            Primitive::Nested { containers } => Primitive::Nested {
                containers: containers
                    .into_iter()
                    .map(|c| Ok(Nest { id: g(c.id)?, members: list(c.members, &mut g)?, padding: c.padding, header: c.header }))
                    .collect::<Result<_, E>>()?,
            },
            Primitive::HubSpoke { hub } => Primitive::HubSpoke { hub: g(hub)? },
            Primitive::Rings { center, rings } => {
                Primitive::Rings { center: g(center)?, rings: rings.into_iter().map(|r| list(r, &mut g)).collect::<Result<_, _>>()? }
            }
            Primitive::Timeline { events, labels } => {
                Primitive::Timeline { events: events.map(|e| list(e, &mut g)).transpose()?, labels: opt(labels, &mut g)? }
            }
            Primitive::Voter { channels, voter } => {
                Primitive::Voter { channels: channels.into_iter().map(|c| list(c, &mut g)).collect::<Result<_, _>>()?, voter: g(voter)? }
            }
            Primitive::Sequence { lifelines } => Primitive::Sequence { lifelines: lifelines.map(|l| list(l, &mut g)).transpose()? },
            Primitive::Er { satellites } => Primitive::Er {
                satellites: satellites
                    .into_iter()
                    .map(|s| Ok(Satellite { owner: g(s.owner)?, members: list(s.members, &mut g)? }))
                    .collect::<Result<_, E>>()?,
            },
        })
    }

    /// Every node reference in the roles, in the order they appear.
    pub fn references(&self) -> Vec<&T> {
        let mut out: Vec<&T> = Vec::new();
        match self {
            Primitive::Pipeline { parallel } => {
                if let Some(p) = parallel {
                    out.push(&p.from);
                    out.push(&p.to);
                    out.extend(&p.members);
                }
            }
            Primitive::Bands { bands } => out.extend(bands.iter().flatten()),
            Primitive::Chain { forward, compensation } => {
                out.extend(forward);
                out.extend(compensation.iter().flatten());
            }
            Primitive::Matrix { rows, cols } => {
                for grid in [rows, cols].into_iter().flatten() {
                    out.extend(grid.iter().flatten().flatten());
                }
            }
            Primitive::Nested { containers } => {
                for c in containers {
                    out.push(&c.id);
                    out.extend(&c.members);
                }
            }
            Primitive::HubSpoke { hub } => out.push(hub),
            Primitive::Rings { center, rings } => {
                out.push(center);
                out.extend(rings.iter().flatten());
            }
            Primitive::Timeline { events, labels } => {
                out.extend(events.iter().flatten());
                out.extend(labels.iter().flatten());
            }
            Primitive::Voter { channels, voter } => {
                out.extend(channels.iter().flatten());
                out.push(voter);
            }
            Primitive::Sequence { lifelines } => out.extend(lifelines.iter().flatten()),
            Primitive::Er { satellites } => {
                for s in satellites {
                    out.push(&s.owner);
                    out.extend(&s.members);
                }
            }
        }
        out
    }
}

/// Lay out `graph` with `primitive`, whose roles name nodes by index, using the spacings,
/// direction and edge style of `options` (the algorithm is not used; pins are ignored).
///
/// Returns an error for invalid or oversized input and for roles the primitive cannot draw;
/// never panics.
pub fn layout_primitive(graph: &Graph, primitive: &Primitive, options: &Options) -> Result<Layout, LayoutError> {
    crate::validate(graph, options)?;
    let mut opt = *options;
    opt.max_iterations = opt.max_iterations.clamp(1, crate::MAX_ITERATIONS);
    let mut notes = Notes::default();
    if graph.nodes.iter().any(|n| n.pin.is_some()) {
        notes.push("layout primitives ignore pins");
    }
    let (pos, resized) = match primitive {
        Primitive::Pipeline { parallel } => (chain::pipeline(graph, parallel.as_ref(), &opt)?, Vec::new()),
        Primitive::Bands { bands } => (band::bands(graph, bands, &opt)?, Vec::new()),
        Primitive::Chain { forward, compensation } => (chain::two_rows(graph, forward, compensation, &opt)?, Vec::new()),
        Primitive::Matrix { rows, cols } => (grid::matrix(graph, rows.as_deref(), cols.as_deref(), &opt)?, Vec::new()),
        Primitive::Nested { containers } => container::nested(graph, containers, &opt)?,
        Primitive::HubSpoke { hub } => (radial::hub_spoke(graph, *hub, &opt)?, Vec::new()),
        Primitive::Rings { center, rings } => (radial::rings(graph, *center, rings, &opt)?, Vec::new()),
        Primitive::Timeline { events, labels } => (grid::timeline(graph, events.as_deref(), labels, &opt)?, Vec::new()),
        Primitive::Voter { channels, voter } => (radial::voter(graph, channels, *voter, &opt)?, Vec::new()),
        Primitive::Sequence { lifelines } => (crate::sequence::graph_columns(graph, lifelines.as_deref(), &opt)?, Vec::new()),
        Primitive::Er { satellites } => {
            // The entity edges are routed orthogonally, as the drawing laid them out.
            opt.orthogonal_edges = true;
            (er::er(graph, satellites, &opt)?, Vec::new())
        }
    };
    Ok(finish(graph, pos, resized, &opt, notes))
}

/// The error for roles a primitive cannot draw.
fn bad(primitive: &'static str, reason: impl Into<String>) -> LayoutError {
    LayoutError::InvalidRoles { primitive, reason: reason.into() }
}

/// Tracks which nodes the roles have named, refusing unknown and repeated ones.
struct Claims {
    primitive: &'static str,
    taken: Vec<bool>,
}

impl Claims {
    fn new(primitive: &'static str, n: usize) -> Self {
        Self { primitive, taken: vec![false; n] }
    }

    /// Claims node `v` for a role described by `what`.
    fn claim(&mut self, v: usize, what: &str) -> Result<usize, LayoutError> {
        match self.taken.get_mut(v) {
            None => Err(bad(self.primitive, format!("{what} names node {v}, which does not exist"))),
            Some(true) => Err(bad(self.primitive, format!("{what} names node {v}, which already has a role"))),
            Some(slot) => {
                *slot = true;
                Ok(v)
            }
        }
    }

    /// Nodes no role named, in index order.
    fn unclaimed(&self) -> Vec<usize> {
        self.taken.iter().enumerate().filter(|(_, t)| !**t).map(|(v, _)| v).collect()
    }

    /// Refuses the roles when some node has none.
    fn require_all(&self, what: &str) -> Result<(), LayoutError> {
        match self.unclaimed().first() {
            Some(v) => Err(bad(self.primitive, format!("node {v} is not {what}"))),
            None => Ok(()),
        }
    }
}

/// Node `v`'s width and height.
fn size(graph: &Graph, v: usize) -> (f64, f64) {
    graph.nodes.get(v).map_or((0.0, 0.0), |n| (n.width, n.height))
}

/// Maps a drawing made along the flow (the "along" axis grows with the flow, "across" is
/// perpendicular to it) to diagram coordinates for a direction.
#[derive(Debug, Clone, Copy)]
struct Frame {
    dir: Direction,
}

impl Frame {
    /// A node's (along, across) extents.
    fn extents(self, (w, h): (f64, f64)) -> (f64, f64) {
        if self.dir.is_horizontal() { (w, h) } else { (h, w) }
    }

    /// The diagram top-left of a node of size `(w, h)` whose frame top-left is `(along, across)`.
    /// Reversed directions mirror the along axis; the result is normalised later.
    fn place(self, along: f64, across: f64, (w, h): (f64, f64)) -> Point {
        match self.dir {
            Direction::LeftRight => Point::new(along, across),
            Direction::RightLeft => Point::new(-along - w, across),
            Direction::TopBottom => Point::new(across, along),
            Direction::BottomTop => Point::new(across, -along - h),
        }
    }
}

/// Routes, group boxes, bounds and normalisation for a primitive's positions.
fn finish(graph: &Graph, pos: Vec<Point>, resized: Vec<Resize>, opt: &Options, notes: Notes) -> Layout {
    let mut sizes: Vec<(f64, f64)> = graph.nodes.iter().map(|n| (n.width, n.height)).collect();
    for r in &resized {
        if let Some(slot) = sizes.get_mut(r.node) {
            *slot = (r.width, r.height);
        }
    }
    let rect = |v: usize| {
        let p = pos.get(v).copied().unwrap_or_default();
        let (w, h) = sizes.get(v).copied().unwrap_or((0.0, 0.0));
        Rect::new(p.x, p.y, w, h)
    };
    let mut sub = Sub { sizes: sizes.clone(), edges: Vec::new(), order: Vec::new() };
    let mut ids: Vec<usize> = Vec::new();
    let mut routes: Vec<Vec<Point>> = vec![Vec::new(); graph.edges.len()];
    let mut loops_on: Vec<usize> = vec![0; graph.nodes.len()];
    for (i, e) in graph.edges.iter().enumerate() {
        if e.source == e.target {
            let k = loops_on.get(e.source).copied().unwrap_or(0);
            if let Some(slot) = loops_on.get_mut(e.source) {
                *slot += 1;
            }
            if let Some(slot) = routes.get_mut(i) {
                *slot = geom::self_loop(&rect(e.source), k, k + 1, opt.edge_spacing.max(4.0), false);
            }
        } else {
            sub.edges.push(SubEdge { s: e.source, t: e.target, weight: 1.0, min_len: 1 });
            ids.push(i);
        }
    }
    let fresh = geom::route_free(&sub, &pos, opt.edge_spacing, opt.orthogonal_edges);
    for (i, route) in ids.into_iter().zip(fresh) {
        if let Some(slot) = routes.get_mut(i) {
            *slot = route;
        }
    }
    let groups: Vec<Option<Rect>> = graph
        .groups
        .iter()
        .map(|g| {
            let mut acc = Bounds::default();
            for &m in &g.members {
                acc.add_rect(&rect(m));
            }
            acc.get().map(|r| Rect::new(r.x - g.padding, r.y - g.padding, r.width + 2.0 * g.padding, r.height + 2.0 * g.padding))
        })
        .collect();
    let mut acc = Bounds::default();
    for v in 0..graph.nodes.len() {
        acc.add_rect(&rect(v));
    }
    for p in routes.iter().flatten() {
        acc.add_point(*p);
    }
    for r in groups.iter().flatten() {
        acc.add_rect(r);
    }
    let b = acc.rect();
    let (dx, dy) = (-b.x, -b.y);
    let shift = |p: Point| Point::new(p.x + dx, p.y + dy);
    let mut nodes: Vec<Point> = pos.into_iter().map(shift).collect();
    nodes.resize(graph.nodes.len(), Point::default());
    Layout {
        nodes,
        edges: routes.into_iter().map(|r| r.into_iter().map(shift).collect()).collect(),
        groups: groups.into_iter().map(|g| g.map_or(Rect::default(), |r| Rect::new(r.x + dx, r.y + dy, r.width, r.height))).collect(),
        bounds: Rect::new(0.0, 0.0, b.width, b.height),
        notes: notes.0,
        resized,
    }
}

#[cfg(test)]
mod tests;
