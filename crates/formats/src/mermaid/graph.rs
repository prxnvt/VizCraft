//! Flowchart, state, class and ER diagrams: the parser's uniform graph view to a page.
//!
//! Placement is a nested layered layout. Every group (subgraph, composite state, region,
//! namespace) is laid out on its own first, innermost groups first, and then stands in its
//! parent's layout as one node of the size it came out at. An edge takes part in the layout of
//! the innermost container holding both of its ends, between the two items of that container
//! that hold the ends. Groups therefore never overlap shapes outside them, which a single flat
//! layout with cluster boxes cannot promise.

use std::collections::HashMap;

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Shape, ShapeKind, Style};
use vizcraft_layout as layout;
use vizcraft_mermaid::{ArrowKind, ClassDef, DiagramKind, Direction, Graph, GraphGroup, GraphNode, GroupKind, LineStyle, Shape as MShape, StyleProp};

use super::common::{self, Ids, MARGIN, Target, Warnings};
use crate::FormatError;

/// Space between a group's rectangle and its members.
const GROUP_PAD: f64 = 20.0;
/// Size of an empty group's rectangle.
const EMPTY_GROUP: (f64, f64) = (120.0, 60.0);
/// Gap between neighbouring shapes in a layer.
const NODE_SPACING: f64 = 40.0;
/// Gap between layers when no edge label needs more.
const LAYER_SPACING: f64 = 60.0;
/// Most extra room an edge label may claim between layers.
const MAX_LAYER_SPACING: f64 = 400.0;

/// One thing laid out inside a container.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Node(usize),
    Group(usize),
}

/// A container: the root (index 0) or group `g` (index `g + 1`).
struct Container {
    items: Vec<Item>,
    edges: Vec<layout::Edge>,
    /// Label size of the widest and tallest edge label in this container, for the layer gap.
    label: (f64, f64),
    direction: Direction,
}

/// Map a flowchart, state, class or ER graph to one laid-out page.
pub(super) fn build(g: &Graph, warnings: &mut Warnings) -> Result<Page, FormatError> {
    let group_of_id: HashMap<&str, usize> = g.groups.iter().enumerate().rev().map(|(i, grp)| (grp.id.as_str(), i)).collect();
    let node_of_id: HashMap<&str, usize> = g.nodes.iter().enumerate().rev().map(|(i, n)| (n.id.as_str(), i)).collect();
    // A parent must come earlier in the list (the parser opens parents first); anything else is
    // treated as top level, which also rules out cycles.
    let parent: Vec<Option<usize>> =
        g.groups.iter().enumerate().map(|(i, grp)| grp.parent.as_deref().and_then(|p| group_of_id.get(p).copied()).filter(|&p| p < i)).collect();
    let mut listed_in: HashMap<&str, usize> = HashMap::new();
    for (i, grp) in g.groups.iter().enumerate() {
        for m in &grp.members {
            listed_in.entry(m.as_str()).or_insert(i);
        }
    }
    let node_container: Vec<usize> = g
        .nodes
        .iter()
        .map(|n| {
            n.group.as_deref().and_then(|gid| group_of_id.get(gid)).or_else(|| listed_in.get(n.id.as_str())).map_or(0, |&gi| gi.saturating_add(1))
        })
        .collect();
    let group_container = |gi: usize| parent.get(gi).copied().flatten().map_or(0, |p| p.saturating_add(1));

    let mut containers: Vec<Container> = Vec::with_capacity(g.groups.len().saturating_add(1));
    containers.push(Container { items: Vec::new(), edges: Vec::new(), label: (0.0, 0.0), direction: g.direction });
    for (gi, grp) in g.groups.iter().enumerate() {
        let inherited = containers.get(group_container(gi)).map_or(g.direction, |c| c.direction);
        containers.push(Container { items: Vec::new(), edges: Vec::new(), label: (0.0, 0.0), direction: grp.direction.unwrap_or(inherited) });
    }

    // Slot of every item inside its container, so edges can name layout node indices.
    let mut group_slot = vec![0usize; g.groups.len()];
    for gi in 0..g.groups.len() {
        if let (Some(c), Some(slot)) = (containers.get_mut(group_container(gi)), group_slot.get_mut(gi)) {
            *slot = c.items.len();
            c.items.push(Item::Group(gi));
        }
    }
    let class_defs: HashMap<&str, &ClassDef> = g.class_defs.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut ids = Ids::default();
    for grp in &g.groups {
        ids.reserve(&grp.id);
    }
    let mut node_shapes: Vec<Shape> = Vec::with_capacity(g.nodes.len());
    let mut node_slot = vec![0usize; g.nodes.len()];
    let mut node_ids: Vec<String> = Vec::with_capacity(g.nodes.len());
    for (ni, n) in g.nodes.iter().enumerate() {
        let ci = node_container.get(ni).copied().unwrap_or(0);
        let dir = containers.get(ci).map_or(g.direction, |c| c.direction);
        let id = ids.claim(&n.id);
        if id != n.id {
            warnings.add(format!("node id `{}` is used twice; the second one became `{}`", common::short(&n.id), common::short(&id)));
        }
        node_shapes.push(node_shape(n, &id, g.kind, dir, &class_defs, warnings));
        node_ids.push(id);
        if let (Some(c), Some(slot)) = (containers.get_mut(ci), node_slot.get_mut(ni)) {
            *slot = c.items.len();
            c.items.push(Item::Node(ni));
        }
    }

    // Edges: resolve both ends, then hand each to the innermost container holding both.
    let resolve = |id: &str| node_of_id.get(id).map(|&n| Item::Node(n)).or_else(|| group_of_id.get(id).map(|&gi| Item::Group(gi)));
    let container_of = |item: Item| match item {
        Item::Node(n) => node_container.get(n).copied().unwrap_or(0),
        Item::Group(gi) => group_container(gi),
    };
    let mut resolved: Vec<Option<(Item, Item)>> = Vec::with_capacity(g.edges.len());
    for e in &g.edges {
        let ends = match (resolve(&e.source), resolve(&e.target)) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => {
                warnings.add(format!(
                    "an edge from `{}` to `{}` names something that is not in the diagram and was skipped",
                    common::short(&e.source),
                    common::short(&e.target)
                ));
                None
            }
        };
        resolved.push(ends);
        let Some((a, b)) = ends else { continue };
        let (path_a, path_b) = (ancestry(a, &container_of, &parent), ancestry(b, &container_of, &parent));
        let Some((ci, ia, ib)) = path_b.iter().find_map(|&(c, ib)| path_a.iter().find(|(ca, _)| *ca == c).map(|&(_, ia)| (c, ia, ib))) else {
            continue;
        };
        if ia == ib {
            continue;
        }
        let slot = |it: Item| match it {
            Item::Node(n) => node_slot.get(n).copied(),
            Item::Group(gi) => group_slot.get(gi).copied(),
        };
        let (Some(sa), Some(sb)) = (slot(ia), slot(ib)) else { continue };
        if let Some(c) = containers.get_mut(ci) {
            let mut le = layout::Edge::new(sa, sb);
            le.min_len = u32::from(e.length).clamp(1, layout::MAX_MIN_LEN);
            c.edges.push(le);
            if let Some(text) = edge_text(e.label.as_deref(), e.source_label.as_deref(), e.target_label.as_deref(), e.start, e.end) {
                let (w, h) = common::text_size(&text);
                c.label = (c.label.0.max(w), c.label.1.max(h));
            }
        }
    }

    // Lay out innermost groups first: a child group always has a larger index than its parent.
    let group_titles: Vec<(f64, f64)> =
        g.groups.iter().map(|grp| if grp.label.is_empty() { (0.0, 0.0) } else { common::text_size(&grp.label) }).collect();
    let mut group_size = vec![EMPTY_GROUP; g.groups.len()];
    let mut group_offset = vec![(GROUP_PAD, GROUP_PAD); g.groups.len()];
    let mut local: Vec<Vec<layout::Point>> = vec![Vec::new(); containers.len()];
    for ci in (0..containers.len()).rev() {
        let Some(c) = containers.get(ci) else { continue };
        let size_of = |it: &Item| match *it {
            Item::Node(n) => node_shapes.get(n).map_or((1.0, 1.0), |s| (s.w, s.h)),
            Item::Group(gi) => group_size.get(gi).copied().unwrap_or(EMPTY_GROUP),
        };
        let lg = layout::Graph {
            nodes: c.items.iter().map(size_of).map(|(w, h)| layout::Node::new(w.max(1.0), h.max(1.0))).collect(),
            edges: c.edges.clone(),
            groups: Vec::new(),
        };
        let direction = layout_direction(c.direction);
        let label_room = if direction.is_horizontal() { c.label.0 } else { c.label.1 };
        let opts = layout::Options {
            direction,
            node_spacing: NODE_SPACING,
            layer_spacing: (label_room + 30.0).clamp(LAYER_SPACING, MAX_LAYER_SPACING),
            ..layout::Options::default()
        };
        let out = layout::layout(&lg, &opts).map_err(|e| FormatError::Import(format!("the Mermaid diagram could not be laid out: {e}")))?;
        if let Some(gi) = ci.checked_sub(1) {
            let (tw, th) = group_titles.get(gi).copied().unwrap_or((0.0, 0.0));
            let (cw, ch) = if c.items.is_empty() { (0.0, 0.0) } else { (out.bounds.width, out.bounds.height) };
            let band = if th > 0.0 { th } else { 0.0 };
            let w = (cw + 2.0 * GROUP_PAD).max(tw + 2.0 * GROUP_PAD).max(EMPTY_GROUP.0);
            let h = (ch + 2.0 * GROUP_PAD + band).max(EMPTY_GROUP.1);
            if let (Some(size), Some(off)) = (group_size.get_mut(gi), group_offset.get_mut(gi)) {
                *size = (w, h);
                *off = ((w - cw) / 2.0, GROUP_PAD + band + (h - ch - 2.0 * GROUP_PAD - band).max(0.0) / 2.0);
            }
        }
        for note in out.notes {
            warnings.add(format!("layout: {note}"));
        }
        if let Some(slot) = local.get_mut(ci) {
            *slot = out.nodes;
        }
    }

    // Absolute positions, outermost first.
    let mut origin = vec![(MARGIN, MARGIN); containers.len()];
    let mut group_pos = vec![(0.0, 0.0); g.groups.len()];
    for gi in 0..g.groups.len() {
        let ci = group_container(gi);
        let base = origin.get(ci).copied().unwrap_or((MARGIN, MARGIN));
        let p = group_slot.get(gi).and_then(|&s| local.get(ci).and_then(|l| l.get(s))).copied().unwrap_or_default();
        let at = (base.0 + p.x, base.1 + p.y);
        let off = group_offset.get(gi).copied().unwrap_or((GROUP_PAD, GROUP_PAD));
        if let Some(slot) = group_pos.get_mut(gi) {
            *slot = at;
        }
        if let Some(o) = origin.get_mut(gi.saturating_add(1)) {
            *o = (at.0 + off.0, at.1 + off.1);
        }
    }
    for (ni, s) in node_shapes.iter_mut().enumerate() {
        let ci = node_container.get(ni).copied().unwrap_or(0);
        let base = origin.get(ci).copied().unwrap_or((MARGIN, MARGIN));
        let p = node_slot.get(ni).and_then(|&slot| local.get(ci).and_then(|l| l.get(slot))).copied().unwrap_or_default();
        s.x = base.0 + p.x;
        s.y = base.1 + p.y;
    }

    let mut page = Page::new("p1", g.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Mermaid".into()));
    let mut group_ids: Vec<String> = Vec::with_capacity(g.groups.len());
    for (gi, grp) in g.groups.iter().enumerate() {
        let (x, y) = group_pos.get(gi).copied().unwrap_or((MARGIN, MARGIN));
        let (w, h) = group_size.get(gi).copied().unwrap_or(EMPTY_GROUP);
        let s = group_shape(grp, x, y, w, h, &class_defs, warnings);
        group_ids.push(s.id.clone());
        page.shapes.push(s);
    }
    page.shapes.extend(node_shapes);

    let item_id = |it: Item| match it {
        Item::Node(n) => node_ids.get(n).cloned(),
        Item::Group(gi) => group_ids.get(gi).cloned(),
    };
    let mut edge_n: usize = 0;
    for (e, ends) in g.edges.iter().zip(&resolved) {
        let Some((a, b)) = *ends else { continue };
        if e.line == LineStyle::Invisible {
            continue;
        }
        let (Some(from), Some(to)) = (item_id(a), item_id(b)) else { continue };
        let id = match &e.id {
            Some(id) if !id.is_empty() => ids.claim(id),
            _ => {
                edge_n = edge_n.saturating_add(1);
                ids.claim(&format!("e{edge_n}"))
            }
        };
        let mut c = Connector::new(id, End::dynamic(from), End::dynamic(to));
        c.start_arrow = arrow(e.start);
        c.end_arrow = arrow(e.end);
        c.text = edge_text(e.label.as_deref(), e.source_label.as_deref(), e.target_label.as_deref(), e.start, e.end).unwrap_or_default();
        match e.line {
            LineStyle::Thick => c.style.line_width = 3.0,
            LineStyle::Dotted => {
                c.ext.insert("mermaid.line".into(), "dotted".into());
            }
            LineStyle::Dashed => {
                c.ext.insert("mermaid.line".into(), "dashed".into());
            }
            LineStyle::Solid | LineStyle::Invisible => {}
        }
        for (key, kind) in [("mermaid.startMarker", e.start), ("mermaid.endMarker", e.end)] {
            if kind != ArrowKind::None {
                c.ext.insert(key.into(), marker_name(kind).into());
            }
        }
        if common::apply_props(&mut c.style, &e.style, Target::Connector, warnings) {
            c.ext.insert("mermaid.line".into(), "dashed".into());
        }
        c.sanitize();
        page.connectors.push(c);
    }
    common::fit_page(&mut page);
    Ok(page)
}

/// The containers holding `item`, innermost first, each with the item of that container that
/// holds it (the item itself, then its enclosing groups).
fn ancestry(item: Item, container_of: &impl Fn(Item) -> usize, parent: &[Option<usize>]) -> Vec<(usize, Item)> {
    let mut out = Vec::new();
    let mut cur = item;
    // Parents have strictly smaller indices, so this walks at most one step per group.
    for _ in 0..=parent.len() {
        let c = container_of(cur);
        out.push((c, cur));
        match c.checked_sub(1) {
            Some(gi) => cur = Item::Group(gi),
            None => break,
        }
    }
    out
}

fn layout_direction(d: Direction) -> layout::Direction {
    match d {
        Direction::TopToBottom => layout::Direction::TopBottom,
        Direction::BottomToTop => layout::Direction::BottomTop,
        Direction::LeftToRight => layout::Direction::LeftRight,
        Direction::RightToLeft => layout::Direction::RightLeft,
    }
}

/// The master a Mermaid shape becomes, and whether that is a known mapping.
fn master_for(shape: &MShape, kind: DiagramKind) -> (&'static str, bool) {
    let m = match shape {
        MShape::Rectangle if kind == DiagramKind::Flowchart => "flowchart.process",
        MShape::Rectangle | MShape::Class | MShape::Entity | MShape::Participant | MShape::Actor => "basic.rectangle",
        MShape::Rounded | MShape::Delay => "basic.roundedRectangle",
        MShape::Stadium => "flowchart.terminator",
        MShape::Subroutine | MShape::StackedRectangle => "flowchart.predefinedProcess",
        MShape::Cylinder | MShape::LinedCylinder | MShape::HorizontalCylinder | MShape::StoredData => "flowchart.database",
        MShape::Circle | MShape::DoubleCircle => "basic.ellipse",
        MShape::SmallCircle | MShape::FramedCircle | MShape::FilledCircle | MShape::CrossCircle | MShape::Start | MShape::End => {
            "flowchart.onPageReference"
        }
        MShape::Diamond | MShape::Choice => "flowchart.decision",
        MShape::Hexagon | MShape::LoopLimit => "flowchart.preparation",
        MShape::Parallelogram | MShape::ParallelogramAlt => "flowchart.data",
        MShape::Trapezoid | MShape::TrapezoidAlt => "flowchart.manualOperation",
        MShape::Document | MShape::MultiDocument | MShape::LinedDocument | MShape::TaggedDocument | MShape::Flag => "flowchart.document",
        MShape::ManualInput => "flowchart.manualInput",
        MShape::Cloud | MShape::Bang => "basic.cloud",
        MShape::Triangle | MShape::FlippedTriangle => "basic.triangle",
        MShape::Asymmetric
        | MShape::NotchedRectangle
        | MShape::LinedRectangle
        | MShape::TaggedRectangle
        | MShape::DividedRectangle
        | MShape::InternalStorage
        | MShape::Display
        | MShape::Hourglass
        | MShape::Bolt => "flowchart.process",
        MShape::Text | MShape::Brace | MShape::ForkBar | MShape::Note => "basic.rectangle",
        MShape::Other(_) => return ("basic.rectangle", false),
    };
    (m, true)
}

/// The name of a Mermaid shape as kept in `ext` (`stadium`, `doubleCircle`, or the unknown name).
fn shape_name(shape: &MShape) -> String {
    if let MShape::Other(name) = shape {
        return common::short(name);
    }
    let debug = format!("{shape:?}");
    let mut chars = debug.chars();
    chars.next().map(|c| c.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// The shape for one graph node, at the origin; the layout moves it.
fn node_shape(n: &GraphNode, id: &str, kind: DiagramKind, dir: Direction, class_defs: &HashMap<&str, &ClassDef>, warnings: &mut Warnings) -> Shape {
    let (master, known) = master_for(&n.shape, kind);
    if !known {
        warnings.add(format!("node shape `{}` is not known and was drawn as a rectangle", shape_name(&n.shape)));
    }
    let mut s = common::from_master(master, id);
    let pseudo = matches!(n.shape, MShape::Start | MShape::End | MShape::ForkBar | MShape::Choice);
    let mut lines: Vec<&str> = Vec::new();
    let annotations: Vec<String> = if pseudo { Vec::new() } else { n.annotations.iter().map(|a| format!("<<{a}>>")).collect() };
    lines.extend(annotations.iter().map(String::as_str));
    if !n.label.is_empty() {
        lines.push(&n.label);
    }
    lines.extend(n.details.iter().map(String::as_str));
    let text = lines.join("\n");
    let dark = Color::rgb(0x2b, 0x33, 0x3d);
    match n.shape {
        MShape::Start | MShape::FilledCircle => {
            (s.w, s.h) = (20.0, 20.0);
            s.style.fill = dark;
            s.style.line = dark;
        }
        MShape::End => {
            // The model has no ringed dot yet: a filled dot with a thick light ring reads as one.
            (s.w, s.h) = (24.0, 24.0);
            s.style.fill = dark;
            s.style.line = Color::rgb(0x9a, 0xa5, 0xb1);
            s.style.line_width = 3.0;
        }
        MShape::ForkBar => {
            // A bar across the flow: wide when the flow runs down, tall when it runs across.
            let across = matches!(dir, Direction::LeftToRight | Direction::RightToLeft);
            (s.w, s.h) = if across { (8.0, 80.0) } else { (80.0, 8.0) };
            s.style.fill = dark;
            s.style.line = dark;
        }
        MShape::Choice => {
            (s.w, s.h) = (36.0, 36.0);
        }
        MShape::SmallCircle | MShape::FramedCircle | MShape::CrossCircle => {
            (s.w, s.h) = common::fit(ShapeKind::Ellipse, &text, (30.0, 30.0));
        }
        _ => {
            (s.w, s.h) = common::fit(s.kind, &text, (s.w, s.h));
        }
    }
    if matches!(n.shape, MShape::Circle | MShape::DoubleCircle | MShape::SmallCircle | MShape::FramedCircle | MShape::CrossCircle) {
        let d = s.w.max(s.h);
        (s.w, s.h) = (d, d);
    }
    if matches!(n.shape, MShape::DoubleCircle | MShape::FramedCircle) {
        s.style.line_width = 3.0;
    }
    match n.shape {
        MShape::Text | MShape::Brace => {
            s.style.fill = Color::TRANSPARENT;
            s.style.line_width = 0.0;
        }
        MShape::Note => {
            s.style.fill = Color::rgb(0xfd, 0xf6, 0xc8);
            s.style.line = Color::rgb(0xb5, 0x9f, 0x3b);
        }
        _ => {}
    }
    s.text = text;
    apply_classes(&mut s, &n.classes, &n.style, class_defs, warnings);
    s.ext.insert("mermaid.shape".into(), shape_name(&n.shape));
    if let Some(link) = &n.link {
        s.ext.insert("mermaid.link".into(), link.clone());
    }
    if let Some(tip) = &n.tooltip {
        s.ext.insert("mermaid.tooltip".into(), tip.clone());
    }
    s.sanitize();
    s
}

/// Apply the `default` class, then the named classes in order, then the inline style.
fn apply_classes(s: &mut Shape, classes: &[String], inline: &[StyleProp], defs: &HashMap<&str, &ClassDef>, warnings: &mut Warnings) {
    let mut dashed = false;
    let named = std::iter::once("default").chain(classes.iter().map(String::as_str));
    for name in named {
        if let Some(def) = defs.get(name) {
            dashed |= common::apply_props(&mut s.style, &def.props, Target::Shape, warnings);
        } else if name != "default" {
            warnings.add(format!("style class `{}` is not defined", common::short(name)));
        }
    }
    dashed |= common::apply_props(&mut s.style, inline, Target::Shape, warnings);
    if dashed {
        s.ext.insert("mermaid.line".into(), "dashed".into());
    }
}

/// The background rectangle of a group, with its title as text.
fn group_shape(grp: &GraphGroup, x: f64, y: f64, w: f64, h: f64, defs: &HashMap<&str, &ClassDef>, warnings: &mut Warnings) -> Shape {
    let mut s = common::from_master("basic.rectangle", &grp.id);
    (s.x, s.y, s.w, s.h) = (x, y, w, h);
    s.text = grp.label.clone();
    s.style = Style { fill: Color::rgb(0xf4, 0xf6, 0xf9), line: Color::rgb(0x8a, 0x97, 0xa6), line_width: 1.0, bold: true, ..s.style };
    if grp.kind == GroupKind::Region {
        s.style.fill = Color::TRANSPARENT;
        s.ext.insert("mermaid.line".into(), "dashed".into());
    }
    apply_classes(&mut s, &grp.classes, &grp.style, defs, warnings);
    let kind = match grp.kind {
        GroupKind::Subgraph => "subgraph",
        GroupKind::CompositeState => "compositeState",
        GroupKind::Region => "region",
        GroupKind::Namespace => "namespace",
        GroupKind::Box => "box",
    };
    s.ext.insert("mermaid.group".into(), kind.into());
    s.sanitize();
    s
}

/// The VizCraft arrowhead closest to a Mermaid marker. Crow's feet have no arrowhead; their
/// meaning goes into the connector text and `ext`.
fn arrow(k: ArrowKind) -> Arrow {
    match k {
        ArrowKind::Arrow | ArrowKind::Triangle => Arrow::Triangle,
        ArrowKind::OpenArrow | ArrowKind::Cross => Arrow::Open,
        ArrowKind::Circle | ArrowKind::Lollipop => Arrow::Circle,
        ArrowKind::DiamondFilled | ArrowKind::DiamondHollow => Arrow::Diamond,
        ArrowKind::None | ArrowKind::ExactlyOne | ArrowKind::ZeroOrOne | ArrowKind::OneOrMore | ArrowKind::ZeroOrMore => Arrow::None,
    }
}

/// The Mermaid marker name kept in `ext`.
fn marker_name(k: ArrowKind) -> &'static str {
    match k {
        ArrowKind::None => "none",
        ArrowKind::Arrow => "arrow",
        ArrowKind::OpenArrow => "openArrow",
        ArrowKind::Circle => "circle",
        ArrowKind::Cross => "cross",
        ArrowKind::Triangle => "triangle",
        ArrowKind::DiamondFilled => "diamondFilled",
        ArrowKind::DiamondHollow => "diamondHollow",
        ArrowKind::Lollipop => "lollipop",
        ArrowKind::ExactlyOne => "exactlyOne",
        ArrowKind::ZeroOrOne => "zeroOrOne",
        ArrowKind::OneOrMore => "oneOrMore",
        ArrowKind::ZeroOrMore => "zeroOrMore",
    }
}

/// A crow's foot marker as a cardinality.
fn cardinality(k: ArrowKind) -> Option<&'static str> {
    match k {
        ArrowKind::ExactlyOne => Some("1"),
        ArrowKind::ZeroOrOne => Some("0..1"),
        ArrowKind::OneOrMore => Some("1..*"),
        ArrowKind::ZeroOrMore => Some("0..*"),
        _ => None,
    }
}

/// The connector text: the label between the two end cardinalities, so `CUSTOMER ||--o{ ORDER :
/// places` reads `1 places 0..*` and `A "1" --> "*" B` reads `1 to *`.
fn edge_text(label: Option<&str>, source: Option<&str>, target: Option<&str>, start: ArrowKind, end: ArrowKind) -> Option<String> {
    let src = source.filter(|s| !s.is_empty()).or(cardinality(start));
    let tgt = target.filter(|s| !s.is_empty()).or(cardinality(end));
    let label = label.filter(|s| !s.is_empty());
    if src.is_none() && tgt.is_none() {
        return label.map(str::to_string);
    }
    let middle = label.unwrap_or("to");
    let parts: Vec<&str> = [src, Some(middle), tgt].into_iter().flatten().collect();
    Some(parts.join(" "))
}
