//! SVG export of one page as a standalone SVG 1.1 document.
//!
//! Choices:
//! - **Size.** The canvas is the page (`0 0 width height`). When shapes or connectors reach past
//!   the page edge, the canvas grows to their bounds plus [`MARGIN`], so nothing is cut off; the
//!   page origin stays where it was, so coordinates in the file are world coordinates.
//! - **Background.** The whole canvas is filled white, like a printed page.
//! - **Shapes.** One `<path>` per shape from its world outline (rotation already applied), with
//!   fill, stroke and stroke width; interior detail lines (cylinder rim, process bars) are a
//!   second unfilled path in the same group.
//! - **Text.** Centred `<text>` with one `<tspan>` per line, rotated with the shape, in Inter
//!   with a sans-serif fallback; the font is named, not embedded.
//! - **Connectors.** One `<path>` from [`connector_geometry`] (the line stops where a filled head
//!   begins), heads as filled or open paths, the label centred on a white box at the label
//!   position. The box width is estimated from the character count, since there is no font
//!   metrics source here.
//! - **Numbers** have at most three decimals and never print as NaN or infinity.

use std::fmt::Write as _;

use vizcraft_diagram::{Color, Connector, Document, Page, Path, PathEl, Point, Rect, Shape, Style, connector_geometry};

use crate::FormatError;

/// Space added around content that reaches past the page edge.
pub const MARGIN: f64 = 20.0;

/// Font stack named in the file.
const FONT_FAMILY: &str = "Inter, sans-serif";

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f64 = 1.2;

/// Rough advance width of one character as a multiple of the font size, for label boxes.
const CHAR_WIDTH: f64 = 0.6;

/// Export page `page_index` (0 based) of `doc` as SVG bytes.
pub fn export_page(doc: &Document, page_index: usize) -> Result<Vec<u8>, FormatError> {
    let page = doc.pages.get(page_index).ok_or(FormatError::NoPage(page_index))?;
    Ok(page_svg(doc, page).into_bytes())
}

/// The canvas: the page rectangle, grown to the content bounds plus a margin when needed.
pub fn canvas_rect(page: &Page) -> Rect {
    let page_rect = page.rect();
    match content_bounds(page) {
        Some(c) if !page_rect.contains_rect(&c) => page_rect.union(&c.inflate(MARGIN)),
        _ => page_rect,
    }
}

/// Bounds of every shape and connector on the page, or `None` for an empty page.
fn content_bounds(page: &Page) -> Option<Rect> {
    let shapes = page.shapes.iter().map(Shape::bounds);
    let connectors = page.connectors.iter().filter_map(|c| {
        let g = connector_geometry(page, c);
        let heads = g.start_head.iter().chain(g.end_head.iter()).filter_map(|h| h.path.bounds());
        g.path.bounds().into_iter().chain(heads).reduce(|a, b| a.union(&b))
    });
    shapes.chain(connectors).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).reduce(|a, b| a.union(&b))
}

fn page_svg(doc: &Document, page: &Page) -> String {
    let r = canvas_rect(page);
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
    for s in &page.shapes {
        shape_svg(&mut out, s);
    }
    for c in &page.connectors {
        connector_svg(&mut out, page, c);
    }
    out.push_str("</svg>\n");
    out
}

fn shape_svg(out: &mut String, s: &Shape) {
    let _ = writeln!(out, "  <g id=\"{}\">", escape(&s.id));
    let st = &s.style;
    let _ = writeln!(out, "    <path d=\"{}\"{}{}/>", path_data(&s.outline_world()), paint("fill", st.fill), stroke(st));
    if let Some(detail) = s.detail_world() {
        let _ = writeln!(out, "    <path d=\"{}\" fill=\"none\"{}/>", path_data(&detail), stroke(st));
    }
    if !s.text.trim().is_empty() {
        let c = s.center();
        let rotate =
            if s.angle_deg != 0.0 { format!(" transform=\"rotate({} {} {})\"", num(s.angle_deg), num(c.x), num(c.y)) } else { String::new() };
        text_svg(out, &s.text, c, st, &rotate);
    }
    out.push_str("  </g>\n");
}

fn connector_svg(out: &mut String, page: &Page, c: &Connector) {
    let g = connector_geometry(page, c);
    let st = &c.style;
    let _ = writeln!(out, "  <g id=\"{}\">", escape(&c.id));
    let _ = writeln!(out, "    <path d=\"{}\" fill=\"none\"{} stroke-linejoin=\"round\"/>", path_data(&g.line), stroke(st));
    for head in g.start_head.iter().chain(g.end_head.iter()) {
        let fill = if head.filled { paint("fill", st.line) } else { " fill=\"none\"".to_string() };
        // Heads keep a visible outline even on a zero-width line, so they never vanish.
        let width = st.line_width.max(1.0);
        let _ = writeln!(
            out,
            "    <path d=\"{}\"{}{} stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
            path_data(&head.path),
            fill,
            paint("stroke", st.line),
            num(width)
        );
    }
    if !c.text.trim().is_empty() {
        let lines: Vec<&str> = c.text.lines().collect();
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
        text_svg(out, &c.text, p, st, "");
    }
    out.push_str("  </g>\n");
}

/// Centred, possibly multi-line text around `center`.
fn text_svg(out: &mut String, text: &str, center: Point, st: &Style, transform: &str) {
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len().max(1) as f64;
    let lh = st.font_size * LINE_HEIGHT;
    // Baselines sit about 0.35 em below each line's centre, which centres Latin text optically
    // without relying on dominant-baseline (unevenly supported by SVG 1.1 renderers).
    let first = center.y - (n - 1.0) * lh / 2.0 + st.font_size * 0.35;
    let weight = if st.bold { " font-weight=\"bold\"" } else { "" };
    let style = if st.italic { " font-style=\"italic\"" } else { "" };
    // No whitespace between the tags: under xml:space="preserve" it would render as text and
    // shift the centring.
    let _ = write!(
        out,
        "    <text text-anchor=\"middle\" font-family=\"{}\" font-size=\"{}\"{}{}{}{} xml:space=\"preserve\">",
        FONT_FAMILY,
        num(st.font_size),
        weight,
        style,
        paint("fill", st.font_color),
        transform
    );
    for (i, line) in lines.iter().enumerate() {
        let y = first + i as f64 * lh;
        let _ = write!(out, "<tspan x=\"{}\" y=\"{}\">{}</tspan>", num(center.x), num(y), escape(line));
    }
    out.push_str("</text>\n");
}

/// ` name="#rrggbb"`, plus an opacity attribute when the colour is translucent, or `none`.
fn paint(name: &str, c: Color) -> String {
    if c.a == 0 {
        return format!(" {name}=\"none\"");
    }
    let hex = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    if c.a == 255 { format!(" {name}=\"{hex}\"") } else { format!(" {name}=\"{hex}\" {name}-opacity=\"{}\"", num(f64::from(c.a) / 255.0)) }
}

/// The stroke attributes for a style; no stroke when the width is zero.
fn stroke(st: &Style) -> String {
    if st.line_width <= 0.0 || !st.line_width.is_finite() || st.line.a == 0 {
        return " stroke=\"none\"".into();
    }
    format!("{} stroke-width=\"{}\"", paint("stroke", st.line), num(st.line_width))
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
