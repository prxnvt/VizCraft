//! SVG export of one page as a standalone SVG 1.1 document.
//!
//! Choices:
//! - **Size.** The canvas is the page (`0 0 width height`). When shapes or connectors reach past
//!   the page edge, the canvas grows to their bounds plus [`MARGIN`], so nothing is cut off; the
//!   page origin stays where it was, so coordinates in the file are world coordinates.
//! - **Background.** The whole canvas is filled white, like a printed page.
//! - **Background pages.** The page's background chain (`Document::background_chain`) is drawn
//!   first, deepest first, each in a `<g class="background">`; then the page's own items.
//! - **Shapes.** One `<path>` per shape from its world outline (rotation already applied, corners
//!   rounded by `cornerRadius`), with fill, stroke, stroke width, dash pattern (scaled by the
//!   line width), line cap, and fill and line transparency folded into the opacities; interior
//!   detail lines (cylinder rim, process bars) are a second unfilled path in the same group.
//!   Hidden shapes, and shapes whose layers are all hidden, are left out.
//! - **Effects.** Gradients and blur filters go in one `<defs>` with numbered ids (`vc-g1`,
//!   `vc-f2`). A gradient is drawn in the shape's bounding box (it does not turn with a rotated
//!   shape). A shadow is a copy of the outline offset by `dx, dy` and blurred. Glow and soft edges
//!   are approximations: glow is a wide blurred stroke under the shape, soft edges blur the shape.
//! - **Text.** `<text>` with one `<tspan>` per line in the shape's text block (the frame, or the
//!   stored text block with its offset and angle), inset by the margins and indent, aligned left,
//!   centre or right and top, middle or bottom (justify draws left aligned), rotated with the
//!   shape. Letter case and bullets are applied to the drawn lines; underline and strikethrough
//!   are `text-decoration`; a highlight is a box behind each line, its width estimated from the
//!   character count. The font is the style's family, then Inter, then sans-serif; named, not
//!   embedded. Text direction is not drawn yet. Fields (`{{pageName}}` and the rest, see
//!   `vizcraft_diagram::text_fields`) are expanded first; on a background page they name the page
//!   being exported.
//! - **Header and footer.** Drawn last, the header's or footer's margin in from the page edges (a
//!   quarter inch by default), one line per slot, at 10 points, with fields (`{page}`, `{pages}`,
//!   `{pageName}`, `{title}`, `{date}`) expanded by `vizcraft_diagram::print_fields`.
//! - **Printing.** With [`SvgOptions::print`] the shapes `vizcraft_diagram::layer::printable`
//!   rejects (non-printing shapes, shapes on no printing layer) are left out, on background pages
//!   too; by default the export matches the screen.
//! - **Connectors.** One `<path>` from the page-level geometry ([`page_geometry`]: the line stops
//!   where a filled head begins, and connectors with line jumps carry their hops as arcs), heads as filled or open paths, the label centred on a white box at the label
//!   position. The box width is estimated from the character count, since there is no font
//!   metrics source here.
//! - **Containers and callouts.** A container's heading band or separator and a lane set's phase
//!   band, separators and titles are drawn between the outline and the text, and a callout's leader
//!   just before the callout, from the same `container::Decor` items the canvas draws.
//! - **Numbers** have at most three decimals and never print as NaN or infinity.

use std::fmt::Write as _;

use vizcraft_diagram::jumps::page_geometry;
use vizcraft_diagram::print_fields::{FieldValues, expand};
use vizcraft_diagram::text_fields::{FieldCtx, expand_shape, system_now_secs};
use vizcraft_diagram::{
    Color, Connector, ConnectorGeometry, Document, Gradient, GradientKind, HAlign, LineCap, Page, Path, PathEl, Rect, Shape, ShapeKind, Style,
    VAlign, connector_geometry,
};

use crate::FormatError;

mod compartments;
mod data_graphics;
pub mod pictures;
mod ports;

/// Space added around content that reaches past the page edge.
pub const MARGIN: f64 = 20.0;

/// Font stack named in the file.
const FONT_FAMILY: &str = "Inter, sans-serif";

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f64 = 1.2;

/// Rough advance width of one character as a multiple of the font size, for label boxes.
const CHAR_WIDTH: f64 = 0.6;

/// How a page is exported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SvgOptions {
    /// Leave out what does not print (`vizcraft_diagram::layer::printable`).
    pub print: bool,
    /// The text `{date}` expands to in the header and footer; empty uses the date part of the
    /// document's last save (`core.modified`), so plain exports stay reproducible.
    pub date: String,
}

/// Export page `page_index` (0 based) of `doc` as SVG bytes, as the screen shows it.
pub fn export_page(doc: &Document, page_index: usize) -> Result<Vec<u8>, FormatError> {
    export_page_with(doc, page_index, &SvgOptions::default())
}

/// Export page `page_index` (0 based) of `doc` as SVG bytes with `opts`.
pub fn export_page_with(doc: &Document, page_index: usize, opts: &SvgOptions) -> Result<Vec<u8>, FormatError> {
    let page = doc.pages.get(page_index).ok_or(FormatError::NoPage(page_index))?;
    Ok(page_svg(doc, page_index, page, opts).into_bytes())
}

/// The canvas: the page frame (see `Page::frame`), grown to the content bounds plus a margin when
/// needed.
pub fn canvas_rect(page: &Page) -> Rect {
    canvas_rect_with(page, false)
}

/// [`canvas_rect`], counting only printable shapes when `print` is set.
pub fn canvas_rect_with(page: &Page, print: bool) -> Rect {
    let page_rect = page.frame();
    match content_bounds(page, print) {
        Some(c) if !page_rect.contains_rect(&c) => page_rect.union(&c.inflate(MARGIN)),
        _ => page_rect,
    }
}

/// Whether shape `s` of `page` is drawn: visible, and printable when printing.
fn drawn(page: &Page, s: &Shape, print: bool) -> bool {
    if print { vizcraft_diagram::layer::printable(page, s) } else { page.shape_visible(s) }
}

/// Bounds of every drawn shape and connector on the page, or `None` for an empty page.
fn content_bounds(page: &Page, print: bool) -> Option<Rect> {
    let shapes = page.shapes.iter().filter(|s| drawn(page, s, print)).map(effect_bounds);
    let connectors = page.connectors.iter().filter_map(|c| {
        let g = connector_geometry(page, c);
        let heads = g.start_head.iter().chain(g.end_head.iter()).filter_map(|h| h.path.bounds());
        g.path.bounds().into_iter().chain(heads).reduce(|a, b| a.union(&b))
    });
    shapes.chain(connectors).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
}

/// A shape's bounds grown to take in its shadow and glow.
fn effect_bounds(s: &Shape) -> Rect {
    let b = s.bounds();
    let mut r = b;
    if let Some(sh) = s.style.shadow {
        let moved = Rect::new(b.x0 + sh.dx, b.y0 + sh.dy, b.x1 + sh.dx, b.y1 + sh.dy).inflate(sh.blur);
        r = r.union(&moved);
    }
    if let Some(g) = s.style.glow {
        r = r.union(&b.inflate(g.radius));
    }
    r
}

fn page_svg(doc: &Document, page_index: usize, page: &Page, opts: &SvgOptions) -> String {
    let r = canvas_rect_with(page, opts.print);
    let (w, h) = (r.width().max(1.0), r.height().max(1.0));
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
        num(w),
        num(h),
        num(r.x0),
        num(r.y0),
        num(w),
        num(h)
    );
    let title = if doc.title.is_empty() { &page.name } else { &doc.title };
    if !title.is_empty() {
        let _ = writeln!(out, "  <title>{}</title>", escape(title));
    }
    let _ = writeln!(out, "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>", num(r.x0), num(r.y0), num(w), num(h));
    let mut cx = Ctx::default();
    // Fields on background pages show this page's name and number.
    let fields = FieldCtx::new(doc, page_index, system_now_secs());
    let mut body = String::new();
    for bg in doc.background_chain(page_index) {
        let _ = writeln!(body, "  <g class=\"background\" data-page=\"{}\">", escape(&bg.id));
        page_items_svg(&mut body, &mut cx, bg, opts.print, &fields);
        body.push_str("  </g>\n");
    }
    page_items_svg(&mut body, &mut cx, page, opts.print, &fields);
    let date = if opts.date.is_empty() { doc.core.modified.get(..10).unwrap_or_default() } else { opts.date.as_str() };
    header_footer_svg(&mut body, page, &FieldValues::for_page(doc, page_index, date));
    if !cx.defs.is_empty() {
        out.push_str("  <defs>\n");
        out.push_str(&cx.defs);
        out.push_str("  </defs>\n");
    }
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

/// Gradients and filters collected while drawing, written once in `<defs>` with numbered ids.
#[derive(Default)]
struct Ctx {
    defs: String,
    next: usize,
}

impl Ctx {
    fn id(&mut self, prefix: &str) -> String {
        self.next = self.next.saturating_add(1);
        format!("vc-{prefix}{}", self.next)
    }
    /// A Gaussian blur filter of `std_dev`; returns its id.
    fn blur(&mut self, std_dev: f64) -> String {
        let id = self.id("f");
        let _ = writeln!(
            self.defs,
            "    <filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
            num(std_dev)
        );
        id
    }
    /// A gradient definition; returns its id.
    fn gradient(&mut self, g: &Gradient) -> String {
        let id = self.id("g");
        match g.kind {
            GradientKind::Linear => {
                let (sin, cos) = g.angle_deg.to_radians().sin_cos();
                let _ = write!(
                    self.defs,
                    "    <linearGradient id=\"{id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                    num(0.5 - cos / 2.0),
                    num(0.5 - sin / 2.0),
                    num(0.5 + cos / 2.0),
                    num(0.5 + sin / 2.0)
                );
            }
            GradientKind::Radial => {
                let _ = write!(self.defs, "    <radialGradient id=\"{id}\" cx=\"0.5\" cy=\"0.5\" r=\"0.5\">");
            }
        }
        for s in &g.stops {
            let c = s.color;
            let _ = write!(self.defs, "<stop offset=\"{}\" stop-color=\"#{:02x}{:02x}{:02x}\"", num(s.offset), c.r, c.g, c.b);
            if c.a != 255 {
                let _ = write!(self.defs, " stop-opacity=\"{}\"", num(f64::from(c.a) / 255.0));
            }
            self.defs.push_str("/>");
        }
        self.defs.push_str(match g.kind {
            GradientKind::Linear => "</linearGradient>\n",
            GradientKind::Radial => "</radialGradient>\n",
        });
        id
    }
}

/// The drawn shapes, then the connectors, of one page.
fn page_items_svg(out: &mut String, cx: &mut Ctx, page: &Page, print: bool, fields: &FieldCtx<'_>) {
    for s in page.shapes.iter().filter(|s| drawn(page, s, print)) {
        if vizcraft_diagram::container::is_callout(s) {
            decor_svg(out, &vizcraft_diagram::container::leader_decor(page, s));
        }
        pictures::picture_svg(out, cx, fields.doc, s);
        data_graphics::shape_svg(out, cx, fields.doc, page, &expand_shape(s, fields));
    }
    ports::ports_svg(out, fields.doc, page, print);
    for (c, g) in page.connectors.iter().zip(page_geometry(page)) {
        connector_svg(out, c, &g);
    }
}

/// A path split into its closed subpaths and its open ones, in order.
fn split_subpaths(p: &Path) -> (Path, Path) {
    let (mut closed, mut open) = (Path::new(), Path::new());
    let mut i = 0;
    while i < p.els.len() {
        let mut j = i + 1;
        while j < p.els.len() && !matches!(p.els.get(j), Some(PathEl::MoveTo(_))) {
            j += 1;
        }
        let sub = p.els.get(i..j).unwrap_or_default();
        let into = if sub.iter().any(|e| matches!(e, PathEl::Close)) { &mut closed } else { &mut open };
        into.els.extend_from_slice(sub);
        i = j;
    }
    (closed, open)
}

fn shape_svg(out: &mut String, cx: &mut Ctx, s: &Shape) {
    let _ = writeln!(out, "  <g id=\"{}\">", escape(&s.id));
    let st = &s.style;
    let outline = s.outline_world().rounded_corners(st.corner_radius);
    // A freeform's closed rings fill even-odd (a ring inside another is a hole) and its open
    // subpaths are lines: SVG would fill an open subpath too, so they go in a path of their own.
    let (outline, lines, rule) = if s.kind == ShapeKind::Freeform {
        let (closed, open) = split_subpaths(&outline);
        // One ring fills the same either way; the rule is written only where it matters.
        let rings = closed.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        (closed, open, if rings > 1 { " fill-rule=\"evenodd\"" } else { "" })
    } else {
        (outline, Path::new(), "")
    };
    let d = path_data(&outline);
    let fill = st.effective_fill();
    let gradient = st.gradient.as_ref().filter(|g| g.is_drawable());
    if let Some(glow) = st.glow.filter(|g| g.radius > 0.0 && g.color.a > 0 && !outline.is_empty()) {
        // Approximation: a blurred, wide stroke of the outline in the glow colour.
        let f = cx.blur(glow.radius / 2.0);
        let _ = writeln!(
            out,
            "    <path d=\"{d}\" fill=\"none\"{} stroke-width=\"{}\" stroke-linejoin=\"round\" filter=\"url(#{f})\"/>",
            paint("stroke", glow.color),
            num(glow.radius * 2.0)
        );
    }
    if let Some(sh) = st.shadow.filter(|sh| sh.color.a > 0) {
        let filter = if sh.blur > 0.0 { format!(" filter=\"url(#{})\"", cx.blur(sh.blur / 2.0)) } else { String::new() };
        let filled = fill.a > 0 || gradient.is_some();
        let paint_attrs = if filled {
            paint("fill", sh.color)
        } else {
            format!(" fill=\"none\"{} stroke-width=\"{}\"", paint("stroke", sh.color), num(st.line_width.max(1.0)))
        };
        if !outline.is_empty() {
            let _ = writeln!(out, "    <path d=\"{d}\"{paint_attrs}{rule} transform=\"translate({} {})\"{filter}/>", num(sh.dx), num(sh.dy));
        }
    }
    let fill_attrs = match gradient {
        Some(g) => {
            let id = cx.gradient(g);
            let opacity = 1.0 - st.fill_transparency;
            if opacity < 1.0 { format!(" fill=\"url(#{id})\" fill-opacity=\"{}\"", num(opacity)) } else { format!(" fill=\"url(#{id})\"") }
        }
        None => paint("fill", fill),
    };
    // Approximation of soft edges: the shape itself blurred by a fraction of the distance.
    let soft = if st.soft_edges > 0.0 { format!(" filter=\"url(#{})\"", cx.blur(st.soft_edges / 4.0)) } else { String::new() };
    if !outline.is_empty() {
        let _ = writeln!(out, "    <path d=\"{d}\"{fill_attrs}{rule}{}{soft}/>", stroke(st));
    }
    if !lines.is_empty() {
        let _ = writeln!(out, "    <path d=\"{}\" fill=\"none\"{}/>", path_data(&lines), stroke(st));
    }
    if let Some(detail) = s.detail_world() {
        let _ = writeln!(out, "    <path d=\"{}\" fill=\"none\"{}/>", path_data(&detail), stroke(st));
    }
    if s.container.is_some() {
        decor_svg(out, &vizcraft_diagram::container::decor(s));
    }
    compartments::compartments_svg(out, s);
    if !s.text.trim().is_empty() {
        shape_text_svg(out, s);
    }
    out.push_str("  </g>\n");
}

/// Container and callout decorations (`container::Decor`): bands, separators, phase titles and
/// leaders.
fn decor_svg(out: &mut String, items: &[vizcraft_diagram::container::Decor]) {
    use vizcraft_diagram::container::Decor;
    for d in items {
        match d {
            Decor::Fill { points, color } if color.a > 0 && points.len() >= 3 => {
                let pts: Vec<String> = points.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect();
                let _ = writeln!(out, "    <polygon points=\"{}\"{}/>", pts.join(" "), paint("fill", *color));
            }
            Decor::Line { points, color, width, dashed } if color.a > 0 && points.len() >= 2 => {
                let pts: Vec<String> = points.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect();
                let dash = if *dashed { " stroke-dasharray=\"5 4\"" } else { "" };
                let _ = writeln!(
                    out,
                    "    <polyline points=\"{}\" fill=\"none\"{} stroke-width=\"{}\"{dash}/>",
                    pts.join(" "),
                    paint("stroke", *color),
                    num(*width)
                );
            }
            Decor::Text { center, angle, text, size, color, bold, .. } if !text.trim().is_empty() => {
                let st = Style { font_size: *size, font_color: *color, bold: *bold, ..Style::DEFAULT };
                let transform =
                    if *angle != 0.0 { format!(" transform=\"rotate({} {} {})\"", num(*angle), num(center.x), num(center.y)) } else { String::new() };
                text_svg(out, std::slice::from_ref(text), center.x, center.y + size * 0.35, Anchor::Middle, &st, &transform);
            }
            _ => {}
        }
    }
}

fn connector_svg(out: &mut String, c: &Connector, g: &ConnectorGeometry) {
    let st = &c.style;
    let line_color = st.effective_line();
    let _ = writeln!(out, "  <g id=\"{}\">", escape(&c.id));
    let line = g.line.rounded_corners(st.corner_radius);
    let _ = writeln!(out, "    <path d=\"{}\" fill=\"none\"{} stroke-linejoin=\"round\"/>", path_data(&line), stroke(st));
    for head in g.start_head.iter().chain(g.end_head.iter()) {
        let fill = if head.filled { paint("fill", line_color) } else { " fill=\"none\"".to_string() };
        // Heads keep a visible outline even on a zero-width line, so they never vanish.
        let width = st.line_width.max(1.0);
        let _ = writeln!(
            out,
            "    <path d=\"{}\"{}{} stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
            path_data(&head.path),
            fill,
            paint("stroke", line_color),
            num(width)
        );
    }
    if !c.text.trim().is_empty() {
        let lines = styled_lines(&c.text, st);
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
        let bw = longest * st.font_size * CHAR_WIDTH + st.font_size * 0.6;
        let bh = lines.len().max(1) as f64 * st.font_size * LINE_HEIGHT + st.font_size * 0.3;
        let p = g.label_pos;
        let _ = writeln!(
            out,
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
            num(p.x - bw / 2.0),
            num(p.y - bh / 2.0),
            num(bw),
            num(bh)
        );
        let lh = st.font_size * LINE_HEIGHT;
        let first = p.y - (lines.len().max(1) as f64 - 1.0) * lh / 2.0 + st.font_size * 0.35;
        text_svg(out, &lines, p.x, first, Anchor::Middle, st, "");
    }
    end_labels_svg(out, c, g);
    out.push_str("  </g>\n");
}

/// The labels beside each end (`start_label`, `end_label`): a little outside the line, past the
/// head, on a white box like the main label, one line each.
fn end_labels_svg(out: &mut String, c: &Connector, g: &ConnectorGeometry) {
    let st = &c.style;
    for (text, at_start) in [(&c.start_label, true), (&c.end_label, false)] {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Some(p) = vizcraft_diagram::arrows::end_label_anchor(&g.polyline, at_start, st.font_size, st.line_width) else { continue };
        let line = text.lines().next().unwrap_or_default().to_string();
        let bw = line.chars().count() as f64 * st.font_size * CHAR_WIDTH + st.font_size * 0.6;
        let bh = st.font_size * LINE_HEIGHT + st.font_size * 0.3;
        let _ = writeln!(
            out,
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
            num(p.x - bw / 2.0),
            num(p.y - bh / 2.0),
            num(bw),
            num(bh)
        );
        text_svg(out, &[line], p.x, p.y + st.font_size * 0.35, Anchor::Middle, st, "");
    }
}

/// Where text lines hang from their x position.
#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn name(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        }
    }
}

/// The text's lines with the style's letter case and bullets applied.
fn styled_lines(text: &str, st: &Style) -> Vec<String> {
    text.lines().enumerate().map(|(i, l)| format!("{}{}", st.bullets.marker(i), st.text_case.apply(l))).collect()
}

/// A shape's text laid out in its text block: margins, indent and alignment applied, rotated
/// with the shape (and with the text block's own angle).
fn shape_text_svg(out: &mut String, s: &Shape) {
    let st = &s.style;
    let (local, block_angle) = s.text_rect();
    // The block in unrotated world coordinates; the shape's rotation is a transform on the text.
    let block = Rect::from_xywh(local.x0 + s.x, local.y0 + s.y, local.width(), local.height());
    let m = st.text_margins;
    let inner = Rect::new(block.x0 + m.left + st.indent, block.y0 + m.top, block.x1 - m.right, block.y1 - m.bottom);
    let (x, anchor) = match st.h_align {
        HAlign::Left | HAlign::Justify => (inner.x0, Anchor::Start),
        HAlign::Center => ((inner.x0 + inner.x1) / 2.0, Anchor::Middle),
        HAlign::Right => (inner.x1, Anchor::End),
    };
    let lines = styled_lines(&s.text, st);
    let n = lines.len().max(1) as f64;
    let lh = st.font_size * LINE_HEIGHT;
    // Centre of the first line; baselines sit about 0.35 em below each line's centre, which
    // centres Latin text optically without relying on dominant-baseline (unevenly supported by
    // SVG 1.1 renderers).
    let first_centre = match st.v_align {
        VAlign::Top => inner.y0 + lh / 2.0,
        VAlign::Middle => (inner.y0 + inner.y1) / 2.0 - (n - 1.0) * lh / 2.0,
        VAlign::Bottom => inner.y1 - lh / 2.0 - (n - 1.0) * lh,
    };
    let c = s.center();
    let mut transform = String::new();
    if s.angle_deg != 0.0 {
        let _ = write!(transform, "rotate({} {} {})", num(s.angle_deg), num(c.x), num(c.y));
    }
    if block_angle != 0.0 {
        let bc = block.center();
        if !transform.is_empty() {
            transform.push(' ');
        }
        let _ = write!(transform, "rotate({} {} {})", num(block_angle), num(bc.x), num(bc.y));
    }
    let transform = if transform.is_empty() { transform } else { format!(" transform=\"{transform}\"") };
    text_svg(out, &lines, x, first_centre + st.font_size * 0.35, anchor, st, &transform);
}

/// Lines of text from the first baseline down, hung from `x` by `anchor`, with highlight boxes
/// behind them when the style has a highlight colour.
fn text_svg(out: &mut String, lines: &[String], x: f64, first_baseline: f64, anchor: Anchor, st: &Style, transform: &str) {
    let lh = st.font_size * LINE_HEIGHT;
    if st.highlight.a > 0 {
        for (i, line) in lines.iter().enumerate() {
            // Widths are estimated from the character count; there is no font metrics source here.
            let w = line.chars().count() as f64 * st.font_size * CHAR_WIDTH;
            let x0 = match anchor {
                Anchor::Start => x,
                Anchor::Middle => x - w / 2.0,
                Anchor::End => x - w,
            };
            let top = first_baseline + i as f64 * lh - st.font_size * 0.35 - lh / 2.0;
            let _ = writeln!(
                out,
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}{transform}/>",
                num(x0),
                num(top),
                num(w),
                num(lh),
                paint("fill", st.highlight)
            );
        }
    }
    let family = if st.font_family.trim().is_empty() { FONT_FAMILY.to_string() } else { format!("{}, {FONT_FAMILY}", escape(st.font_family.trim())) };
    let weight = if st.bold { " font-weight=\"bold\"" } else { "" };
    let style = if st.italic { " font-style=\"italic\"" } else { "" };
    let decoration = match (st.underline, st.strikethrough) {
        (true, true) => " text-decoration=\"underline line-through\"",
        (true, false) => " text-decoration=\"underline\"",
        (false, true) => " text-decoration=\"line-through\"",
        (false, false) => "",
    };
    // No whitespace between the tags: under xml:space="preserve" it would render as text and
    // shift the centring.
    let _ = write!(
        out,
        "    <text text-anchor=\"{}\" font-family=\"{family}\" font-size=\"{}\"{weight}{style}{decoration}{}{transform} xml:space=\"preserve\">",
        anchor.name(),
        num(st.font_size),
        paint("fill", st.font_color),
    );
    for (i, line) in lines.iter().enumerate() {
        let y = first_baseline + i as f64 * lh;
        let _ = write!(out, "<tspan x=\"{}\" y=\"{}\">{}</tspan>", num(x), num(y), escape(line));
    }
    out.push_str("</text>\n");
}

/// Header and footer text, each its margin in from the frame edges, in the default text style at
/// a small size, with fields expanded from `values`.
fn header_footer_svg(out: &mut String, page: &Page, values: &FieldValues) {
    let st = Style { font_size: 10.0, ..Style::DEFAULT };
    let frame = page.frame();
    for (hf, top) in [(&page.header, true), (&page.footer, false)] {
        if hf.is_empty() {
            continue;
        }
        let inset = hf.margin();
        // The header's cap height sits at the margin; the footer's baseline sits at the margin.
        let baseline = if top { frame.y0 + inset + 7.0 } else { frame.y1 - inset };
        let slots =
            [(&hf.left, frame.x0 + inset, Anchor::Start), (&hf.center, frame.center().x, Anchor::Middle), (&hf.right, frame.x1 - inset, Anchor::End)];
        for (text, x, anchor) in slots {
            if !text.trim().is_empty() {
                // One line only: a header slot is a single line of text.
                let line = expand(text.lines().next().unwrap_or_default(), values);
                text_svg(out, &[line], x, baseline, anchor, &st, "");
            }
        }
    }
}

/// ` name="#rrggbb"`, plus an opacity attribute when the colour is translucent, or `none`.
fn paint(name: &str, c: Color) -> String {
    if c.a == 0 {
        return format!(" {name}=\"none\"");
    }
    let hex = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    if c.a == 255 { format!(" {name}=\"{hex}\"") } else { format!(" {name}=\"{hex}\" {name}-opacity=\"{}\"", num(f64::from(c.a) / 255.0)) }
}

/// The stroke attributes for a style (colour with line transparency, width, dash pattern, cap);
/// no stroke when the width is zero.
fn stroke(st: &Style) -> String {
    let line = st.effective_line();
    if st.line_width <= 0.0 || !st.line_width.is_finite() || line.a == 0 {
        return " stroke=\"none\"".into();
    }
    let mut s = format!("{} stroke-width=\"{}\"", paint("stroke", line), num(st.line_width));
    let pattern = st.dash.pattern();
    if !pattern.is_empty() {
        let unit = st.line_width.max(1.0);
        let dashes: Vec<String> = pattern.iter().map(|v| num(v * unit)).collect();
        let _ = write!(s, " stroke-dasharray=\"{}\"", dashes.join(" "));
    }
    if st.line_cap != LineCap::Butt {
        let _ = write!(s, " stroke-linecap=\"{}\"", st.line_cap.name());
    }
    s
}

/// SVG path data for a path.
pub fn path_data(p: &Path) -> String {
    let mut d = String::new();
    for el in &p.els {
        if !d.is_empty() {
            d.push(' ');
        }
        match el {
            PathEl::MoveTo(a) => {
                let _ = write!(d, "M{} {}", num(a.x), num(a.y));
            }
            PathEl::LineTo(a) => {
                let _ = write!(d, "L{} {}", num(a.x), num(a.y));
            }
            PathEl::CubicTo(a, b, c) => {
                let _ = write!(d, "C{} {} {} {} {} {}", num(a.x), num(a.y), num(b.x), num(b.y), num(c.x), num(c.y));
            }
            PathEl::Close => d.push('Z'),
        }
    }
    d
}

/// A compact number: at most three decimals, no trailing zeros, `0` for NaN, infinity and `-0`.
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 || !r.is_finite() { "0".into() } else { format!("{r}") }
}

/// Escape text for XML content and attribute values. Characters XML 1.0 forbids (most control
/// characters) are dropped so the file always parses.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod jump_tests {
    use vizcraft_diagram::{End, Point, Route};

    use super::*;

    fn crossing(jumps: bool) -> String {
        let mut doc = Document::new();
        if let Some(page) = doc.pages.first_mut() {
            for (id, a, b) in [("h", Point::new(0.0, 100.0), Point::new(200.0, 100.0)), ("v", Point::new(100.0, 0.0), Point::new(100.0, 200.0))] {
                let mut c = Connector::new(id, End::free(a), End::free(b));
                c.route = Route::Straight;
                c.jumps = jumps;
                page.connectors.push(c);
            }
        }
        export_page(&doc, 0).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// The `d` of the line path in connector group `id`.
    fn line_of(svg: &str, id: &str) -> String {
        let group = svg.split(&format!("<g id=\"{id}\">")).nth(1).unwrap_or_default();
        group.split("d=\"").nth(1).and_then(|d| d.split('"').next()).unwrap_or_default().to_string()
    }

    #[test]
    fn the_svg_holds_the_hops_of_connectors_with_jumps() {
        let svg = crossing(true);
        assert!(!line_of(&svg, "h").contains('C'), "the first connector does not hop");
        let v = line_of(&svg, "v");
        // A semicircle of radius 5 around (100, 100), bulging left: two cubics ending at the top
        // of the hop and past the crossing.
        assert_eq!(v.matches('C').count(), 2, "{v}");
        assert!(v.contains(" 95 100") || v.contains("95 100"), "{v}");
        assert!(v.ends_with("100 105") || v.contains("100 105"), "{v}");
        let plain = crossing(false);
        assert!(!line_of(&plain, "v").contains('C'));
    }
}

#[cfg(test)]
mod style_tests {
    use vizcraft_diagram::{
        Bullets, Dash, Gradient, GradientKind, GradientStop, HeaderFooter, LineCap, Margins, Point, Shadow, ShapeKind, TextBlock, VAlign,
    };

    use super::*;

    fn one_shape(edit: impl FnOnce(&mut Shape)) -> String {
        let mut doc = Document::new();
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
        edit(&mut s);
        if let Some(p) = doc.page_mut() {
            p.board = vizcraft_diagram::Board::Fixed;
            p.width = 200.0;
            p.height = 120.0;
            p.shapes.push(s);
        }
        export_page(&doc, 0).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// The lines after the declaration, root, title and white canvas rectangle: defs and drawing.
    fn body(svg: &str) -> String {
        svg.lines().skip(4).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn golden_gradient_fill() {
        let svg = one_shape(|s| {
            let stops =
                vec![GradientStop { offset: 0.0, color: Color::WHITE }, GradientStop { offset: 1.0, color: Color { r: 0, g: 0, b: 255, a: 128 } }];
            s.style.gradient = Some(Gradient { kind: GradientKind::Linear, angle_deg: 90.0, stops });
            s.style.fill_transparency = 0.5;
        });
        let want = r##"  <defs>
    <linearGradient id="vc-g1" x1="0.5" y1="0" x2="0.5" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#0000ff" stop-opacity="0.502"/></linearGradient>
  </defs>
  <g id="s1">
    <path d="M10 20 L110 20 L110 80 L10 80 Z" fill="url(#vc-g1)" fill-opacity="0.5" stroke="#3a4a5c" stroke-width="1.5"/>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
        let radial = one_shape(|s| {
            let stops = vec![GradientStop { offset: 0.0, color: Color::WHITE }, GradientStop { offset: 1.0, color: Color::BLACK }];
            s.style.gradient = Some(Gradient { kind: GradientKind::Radial, angle_deg: 0.0, stops });
        });
        assert!(radial.contains(r##"<radialGradient id="vc-g1" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#000000"/></radialGradient>"##), "{radial}");
        // One stop is not a gradient: the solid fill is drawn.
        let single = one_shape(|s| s.style.gradient = Some(Gradient { stops: vec![GradientStop::default()], ..Gradient::default() }));
        assert!(!single.contains("<defs>") && single.contains("fill=\"#ffffff\" stroke"), "{single}");
    }

    #[test]
    fn golden_dash_cap_rounding_and_line_transparency() {
        let svg = one_shape(|s| {
            s.style.dash = Dash::Dashed;
            s.style.line_width = 2.0;
            s.style.line_cap = LineCap::Round;
            s.style.corner_radius = 10.0;
            s.style.line_transparency = 0.5;
        });
        let want = r##"  <g id="s1">
    <path d="M10 30 C10 24.477 14.477 20 20 20 L100 20 C105.523 20 110 24.477 110 30 L110 70 C110 75.523 105.523 80 100 80 L20 80 C14.477 80 10 75.523 10 70 Z" fill="#ffffff" stroke="#3a4a5c" stroke-opacity="0.502" stroke-width="2" stroke-dasharray="8 6" stroke-linecap="round"/>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
        for (dash, array) in [(Dash::Dotted, "1.5 3"), (Dash::DashDot, "6 3 1.5 3"), (Dash::LongDash, "12 4.5")] {
            let svg = one_shape(|s| s.style.dash = dash);
            assert!(svg.contains(&format!("stroke-dasharray=\"{array}\"")), "{dash:?}: {svg}");
        }
    }

    #[test]
    fn golden_shadow_glow_and_soft_edges() {
        let svg = one_shape(|s| {
            s.style.shadow = Some(Shadow::default());
            s.style.glow = Some(vizcraft_diagram::Glow::default());
            s.style.soft_edges = 4.0;
        });
        let want = r##"  <defs>
    <filter id="vc-f1" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="3"/></filter>
    <filter id="vc-f2" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="1"/></filter>
    <filter id="vc-f3" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="1"/></filter>
  </defs>
  <g id="s1">
    <path d="M10 20 L110 20 L110 80 L10 80 Z" fill="none" stroke="#ffc000" stroke-opacity="0.6" stroke-width="12" stroke-linejoin="round" filter="url(#vc-f1)"/>
    <path d="M10 20 L110 20 L110 80 L10 80 Z" fill="#000000" fill-opacity="0.333" transform="translate(3 3)" filter="url(#vc-f2)"/>
    <path d="M10 20 L110 20 L110 80 L10 80 Z" fill="#ffffff" stroke="#3a4a5c" stroke-width="1.5" filter="url(#vc-f3)"/>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
        // A hard shadow needs no filter.
        let hard = one_shape(|s| s.style.shadow = Some(Shadow { blur: 0.0, ..Shadow::default() }));
        assert!(!hard.contains("<filter") && hard.contains("transform=\"translate(3 3)\"/>"), "{hard}");
    }

    #[test]
    fn golden_text_alignment_bullets_decoration_and_highlight() {
        let svg = one_shape(|s| {
            s.text = "one\ntwo".into();
            s.style.h_align = HAlign::Left;
            s.style.v_align = VAlign::Top;
            s.style.text_margins = Margins { left: 5.0, top: 5.0, right: 5.0, bottom: 5.0 };
            s.style.bullets = Bullets::Disc;
            s.style.underline = true;
            s.style.strikethrough = true;
            s.style.highlight = Color::rgb(255, 255, 0);
        });
        let want = "  <g id=\"s1\">
    <path d=\"M10 20 L110 20 L110 80 L10 80 Z\" fill=\"#ffffff\" stroke=\"#3a4a5c\" stroke-width=\"1.5\"/>
    <rect x=\"15\" y=\"25\" width=\"36\" height=\"14.4\" fill=\"#ffff00\"/>
    <rect x=\"15\" y=\"39.4\" width=\"36\" height=\"14.4\" fill=\"#ffff00\"/>
    <text text-anchor=\"start\" font-family=\"Inter, sans-serif\" font-size=\"12\" text-decoration=\"underline line-through\" fill=\"#1e2630\" xml:space=\"preserve\"><tspan x=\"15\" y=\"36.4\">\u{2022} one</tspan><tspan x=\"15\" y=\"50.8\">\u{2022} two</tspan></text>
  </g>
</svg>";
        assert_eq!(body(&svg), want);
    }

    #[test]
    fn golden_text_block_right_bottom_and_rotation() {
        let svg = one_shape(|s| {
            s.text = "r".into();
            s.angle_deg = 90.0;
            s.style.h_align = HAlign::Right;
            s.style.v_align = VAlign::Bottom;
            s.text_block = Some(TextBlock { dx: 0.0, dy: 0.0, w: 50.0, h: 40.0, angle_deg: 45.0 });
        });
        let want = r##"  <g id="s1">
    <path d="M90 0 L90 100 L30 100 L30 0 Z" fill="#ffffff" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="end" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" transform="rotate(90 60 50) rotate(45 60 50)" xml:space="preserve"><tspan x="81" y="63">r</tspan></text>
  </g>
</svg>"##;
        assert_eq!(body(&svg), want);
        // Defaults keep the centred layout of earlier versions.
        let centred = one_shape(|s| s.text = "c".into());
        assert!(centred.contains(r##"<text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="60" y="54.2">c</tspan></text>"##), "{centred}");
    }

    #[test]
    fn text_case_family_and_connector_dash() {
        let svg = one_shape(|s| {
            s.text = "mixed Case".into();
            s.style.text_case = vizcraft_diagram::TextCase::Upper;
            s.style.font_family = "Mono \"x\"".into();
        });
        assert!(svg.contains(">MIXED CASE</tspan>"), "{svg}");
        assert!(svg.contains("font-family=\"Mono &quot;x&quot;, Inter, sans-serif\""), "{svg}");
        let mut doc = Document::new();
        let mut c = Connector::new("c1", vizcraft_diagram::End::free(Point::new(0.0, 0.0)), vizcraft_diagram::End::free(Point::new(100.0, 50.0)));
        c.style.dash = Dash::Dotted;
        c.style.line_cap = LineCap::Square;
        if let Some(p) = doc.page_mut() {
            p.connectors.push(c);
        }
        let svg = String::from_utf8(export_page(&doc, 0).unwrap_or_default()).unwrap_or_default();
        assert!(svg.contains("stroke-dasharray=\"1.5 3\" stroke-linecap=\"square\" stroke-linejoin=\"round\""), "{svg}");
    }

    #[test]
    fn background_pages_draw_under_and_header_footer_on_top() {
        let mut doc = Document::new();
        let mut bg = Page::new("bg", "Background");
        bg.is_background = true;
        bg.shapes.push(Shape::new("logo", ShapeKind::Ellipse, 0.0, 0.0, 50.0, 20.0));
        doc.pages.push(bg);
        if let Some(p) = doc.page_mut() {
            p.board = vizcraft_diagram::Board::Fixed;
            p.background = Some("bg".into());
            p.header = HeaderFooter { left: "Plant".into(), center: String::new(), right: "Rev A\nignored".into(), margin: None };
            p.footer = HeaderFooter { left: String::new(), center: "Page 1".into(), right: String::new(), margin: None };
            p.shapes.push(Shape::new("s1", ShapeKind::Rectangle, 100.0, 100.0, 50.0, 50.0));
        }
        doc.sanitize();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap_or_default()).unwrap_or_default();
        let at = |needle: &str| svg.find(needle).unwrap_or_else(|| panic!("no {needle} in {svg}"));
        assert!(at("<g class=\"background\" data-page=\"bg\">") < at("<g id=\"logo\">"));
        assert!(at("<g id=\"logo\">") < at("<g id=\"s1\">"), "the background is drawn first");
        assert!(at("<g id=\"s1\">") < at(">Plant</tspan>"), "the header is drawn last");
        assert!(svg.contains(r##"<text text-anchor="start" font-family="Inter, sans-serif" font-size="10" fill="#1e2630" xml:space="preserve"><tspan x="25" y="32">Plant</tspan></text>"##), "{svg}");
        assert!(svg.contains(r##"<tspan x="1075" y="32">Rev A</tspan>"##) && !svg.contains("ignored"), "{svg}");
        assert!(svg.contains(r##"<text text-anchor="middle" font-family="Inter, sans-serif" font-size="10" fill="#1e2630" xml:space="preserve"><tspan x="550" y="825">Page 1</tspan></text>"##), "{svg}");
        // The background page itself exports without anything under it.
        let own = String::from_utf8(export_page(&doc, 1).unwrap_or_default()).unwrap_or_default();
        assert!(!own.contains("class=\"background\""));
    }

    #[test]
    fn fields_expand_on_the_page_and_on_its_background_page() {
        let mut doc = Document::new();
        let mut bg = vizcraft_diagram::Page::new("bg", "Back");
        bg.is_background = true;
        let mut s = Shape::new("f", vizcraft_diagram::ShapeKind::Rectangle, 0.0, 0.0, 100.0, 40.0);
        s.text = "{{pageName}} {{pageNumber}}/{{pageCount}}".into();
        bg.shapes.push(s.clone());
        doc.pages.push(bg);
        doc.pages[0].background = Some("bg".into());
        s.id = "g".into();
        s.text = "[{{data:missing}}] {{title}}".into();
        doc.pages[0].shapes.push(s);
        doc.pages[0].name = "Floor".into();
        doc.title = "Plan".into();
        let svg = String::from_utf8(export_page(&doc, 0).unwrap()).unwrap();
        assert!(svg.contains(">Floor 1/1</tspan>"), "{svg}");
        assert!(svg.contains(">[] Plan</tspan>"), "{svg}");
        assert!(!svg.contains("{{"), "{svg}");
    }

    #[test]
    fn hidden_shapes_and_hidden_layers_are_not_drawn() {
        let mut doc = Document::new();
        if let Some(p) = doc.page_mut() {
            p.layers = vec![vizcraft_diagram::Layer { id: "off".into(), visible: false, ..vizcraft_diagram::Layer::default() }];
            let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
            a.layers = vec!["off".into()];
            let mut b = Shape::new("b", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
            b.behavior.hidden = true;
            p.shapes.extend([a, b, Shape::new("c", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0)]);
        }
        let svg = String::from_utf8(export_page(&doc, 0).unwrap_or_default()).unwrap_or_default();
        assert!(!svg.contains("<g id=\"a\">") && !svg.contains("<g id=\"b\">") && svg.contains("<g id=\"c\">"), "{svg}");
    }

    #[test]
    fn shadows_grow_the_canvas() {
        let svg = one_shape(|s| {
            s.x = 150.0;
            s.style.shadow = Some(Shadow { dx: 30.0, dy: 0.0, blur: 0.0, ..Shadow::default() });
        });
        // Shape right edge 250 plus a 30 unit shadow plus the margin.
        assert!(svg.contains("width=\"300\""), "{}", &svg[..200]);
    }
    /// The same drawing and header on a fixed sheet and on an infinite board.
    fn on_board(board: vizcraft_diagram::Board) -> String {
        let mut doc = Document::new();
        if let Some(p) = doc.page_mut() {
            p.board = board;
            p.width = 200.0;
            p.height = 120.0;
            p.header = HeaderFooter { left: "Top".into(), center: String::new(), right: String::new(), margin: Some(10.0) };
            p.footer = HeaderFooter { right: "Bottom".into(), margin: Some(10.0), ..HeaderFooter::default() };
            p.shapes.push(Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0));
        }
        export_page(&doc, 0).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    #[test]
    fn golden_fixed_board_is_the_sheet() {
        let svg = on_board(vizcraft_diagram::Board::Fixed);
        let head: Vec<&str> = svg.lines().take(4).collect();
        let want = [
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="200" height="120" viewBox="0 0 200 120">"#,
            "  <title>Page-1</title>",
            r##"  <rect x="0" y="0" width="200" height="120" fill="#ffffff"/>"##,
        ];
        assert_eq!(head, want);
        assert!(svg.contains(r#"<tspan x="10" y="17">Top</tspan>"#), "{svg}");
        assert!(svg.contains(r#"<tspan x="190" y="110">Bottom</tspan>"#), "{svg}");
    }

    #[test]
    fn golden_infinite_board_is_the_drawing_plus_the_fit_margin() {
        let svg = on_board(vizcraft_diagram::Board::Infinite);
        let head: Vec<&str> = svg.lines().take(4).collect();
        // The shape spans 10,20 to 110,80; the frame adds 25 on every side.
        let want = [
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="150" height="110" viewBox="-15 -5 150 110">"#,
            "  <title>Page-1</title>",
            r##"  <rect x="-15" y="-5" width="150" height="110" fill="#ffffff"/>"##,
        ];
        assert_eq!(head, want);
        // Header and footer sit their margin inside the frame edges, not the stored sheet's.
        assert!(svg.contains(r#"<tspan x="-5" y="12">Top</tspan>"#), "{svg}");
        assert!(svg.contains(r#"<tspan x="125" y="95">Bottom</tspan>"#), "{svg}");
    }

    #[test]
    fn an_empty_infinite_page_exports_a_letter_frame() {
        let svg = String::from_utf8_lossy(&export_page(&Document::new(), 0).unwrap_or_default()).into_owned();
        assert!(svg.contains(r#"width="1100" height="850" viewBox="0 0 1100 850""#), "{svg}");
    }
}

#[cfg(test)]
mod glyph_tests {
    use vizcraft_diagram::{End, Freeform, Point, ShapeKind};

    use super::*;

    fn page_svg_of(doc: &Document) -> String {
        let svg = export_page(doc, 0).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let start = svg.find("  <g id=").unwrap_or(0);
        svg.get(start..).unwrap_or_default().trim_end().to_string()
    }

    fn fixed() -> Document {
        let mut doc = Document::new();
        if let Some(p) = doc.page_mut() {
            p.board = vizcraft_diagram::Board::Fixed;
            p.width = 300.0;
            p.height = 200.0;
        }
        doc
    }

    const HOLE_GOLDEN: &str = r##"<g id="ring">
    <path d="M20 20 L120 20 L120 100 L20 100 Z M50 40 L90 40 L90 80 L50 80 Z" fill="#ffffff" fill-rule="evenodd" stroke="#3a4a5c" stroke-width="1.5"/>
    <path d="M20 30 L120 30" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
  </g>
  <g id="line">
    <path d="M150 20 L210 60 L150 60" fill="none" stroke="#3a4a5c" stroke-width="1.5"/>
  </g>
</svg>"##;

    #[test]
    fn golden_freeform_with_a_hole_fills_even_odd_and_strokes_its_lines_unfilled() {
        let mut doc = fixed();
        let p = |x: f64, y: f64| Point::new(x, y);
        let mut path = Path::polygon(&[p(0.0, 0.0), p(100.0, 0.0), p(100.0, 80.0), p(0.0, 80.0)]);
        path.els.extend(Path::polygon(&[p(30.0, 20.0), p(70.0, 20.0), p(70.0, 60.0), p(30.0, 60.0)]).els);
        path.els.extend(Path::polyline(&[p(0.0, 10.0), p(100.0, 10.0)]).els);
        let mut s = Shape::new("ring", ShapeKind::Freeform, 20.0, 20.0, 100.0, 80.0);
        s.path = Some(Freeform::new(&path, true));
        s.sanitize();
        // An open freeform: lines only, no fill.
        let mut line = Shape::new("line", ShapeKind::Freeform, 150.0, 20.0, 60.0, 40.0);
        line.path = Some(Freeform::new(&Path::polyline(&[p(0.0, 0.0), p(60.0, 40.0), p(0.0, 40.0)]), false));
        line.sanitize();
        if let Some(pg) = doc.page_mut() {
            pg.shapes.extend([s, line]);
        }
        let body = page_svg_of(&doc);
        assert_eq!(body.trim_start(), HOLE_GOLDEN, "{body}");
        assert!(body.contains("fill-rule=\"evenodd\""));
    }

    #[test]
    fn end_labels_are_written_beside_each_end_on_a_white_box() {
        let mut doc = fixed();
        let mut c = Connector::new("c", End::free(Point::new(20.0, 100.0)), End::free(Point::new(260.0, 100.0)));
        c.route = vizcraft_diagram::Route::Straight;
        c.start_label = "1".into();
        c.end_label = "N & M".into();
        if let Some(pg) = doc.page_mut() {
            pg.connectors.push(c);
        }
        let body = page_svg_of(&doc);
        assert!(body.contains(">1</tspan>") && body.contains(">N &amp; M</tspan>"), "{body}");
        assert_eq!(body.matches("fill=\"#ffffff\"/>").count(), 2, "one white box per end label: {body}");
        // Beside the line, not on it: both labels sit above or below y = 100.
        assert!(!body.contains("y=\"104.2\""), "{body}");
    }
}
