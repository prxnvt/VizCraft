//! Title blocks: a grid of labelled slots whose text holds field placeholders, turned into
//! generated shapes on a decoration page.
//!
//! A [`Block`] is plain data in millimetres: its size and its [`Slot`]s, each a cell with a small
//! caption and a value. Values are stored text with fields (`{{doc:title}}`, `{{sheet}}`, see
//! `crate::text_fields`), expanded when the page is drawn or exported, so a title block never goes
//! stale and rebuilding it is idempotent. [`iso7200`] and [`asme`] are our own layouts of the ISO
//! 7200 and ASME Y14.1 field sets (no standard's figure or another tool's block is copied).
//!
//! The same API serves other generated tables drawn the same way (a legend or key generator
//! builds its own `Block` and calls [`shapes`]).

use crate::geom::Point;
use crate::model::{Color, Shape, Style};
use crate::sheet::{INK, TitleBlockKind, font_for_char_height};
use crate::style_fields::{HAlign, VAlign};

/// Most slots one block draws.
pub const MAX_SLOTS: usize = 128;

/// Height of a slot's caption line, in millimetres.
pub const CAPTION_MM: f64 = 3.0;

/// One cell of a block, in millimetres from the block's top left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// The caption drawn small in the top left corner (`Title`); empty for none.
    pub label: String,
    /// The value: stored text, usually a field placeholder.
    pub text: String,
    /// Character height of the value, in millimetres.
    pub size: f64,
    /// The value is bold.
    pub bold: bool,
}

impl Slot {
    /// A slot at `(x, y)` of `w` by `h` mm with a caption and a value of 2.5 mm characters.
    pub fn new(x: f64, y: f64, w: f64, h: f64, label: &str, text: &str) -> Slot {
        Slot { x, y, w, h, label: label.into(), text: text.into(), size: 2.5, bold: false }
    }
    /// The same slot with `size` mm characters, bold.
    pub fn strong(mut self, size: f64) -> Slot {
        self.size = size;
        self.bold = true;
        self
    }
}

/// A title block (or any captioned table): its size and slots, in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Width.
    pub w_mm: f64,
    /// Height.
    pub h_mm: f64,
    /// The cells.
    pub slots: Vec<Slot>,
}

/// The block for a title block kind; `None` for [`TitleBlockKind::None`].
pub fn for_kind(kind: TitleBlockKind) -> Option<Block> {
    match kind {
        TitleBlockKind::Iso7200 => Some(iso7200()),
        TitleBlockKind::Asme => Some(asme()),
        TitleBlockKind::None => None,
    }
}

/// The ISO 7200 field set in our own 180 by 40 mm layout: two rows of administrative fields over
/// the owner, the titles and the identification block.
pub fn iso7200() -> Block {
    let d = |k: &str| format!("{{{{doc:{k}}}}}");
    Block {
        w_mm: 180.0,
        h_mm: 40.0,
        slots: vec![
            Slot::new(0.0, 0.0, 26.0, 8.0, "Responsible dept.", &d("responsibleDepartment")),
            Slot::new(26.0, 0.0, 34.0, 8.0, "Technical reference", &d("technicalReference")),
            Slot::new(60.0, 0.0, 35.0, 8.0, "Created by", &d("creator")),
            Slot::new(95.0, 0.0, 35.0, 8.0, "Approved by", &d("approvalPerson")),
            Slot::new(130.0, 0.0, 50.0, 8.0, "Document type", &d("documentType")),
            Slot::new(0.0, 8.0, 26.0, 8.0, "Classification", &d("classification")),
            Slot::new(26.0, 8.0, 34.0, 8.0, "Document status", &d("documentStatus")),
            Slot::new(60.0, 8.0, 35.0, 8.0, "Date of issue", &d("dateOfIssue")),
            Slot::new(95.0, 8.0, 15.0, 8.0, "Lang.", &d("language")),
            Slot::new(110.0, 8.0, 20.0, 8.0, "Size", &d("paperSize")),
            Slot::new(130.0, 8.0, 25.0, 8.0, "Sheet", "{{sheet}}/{{sheets}}"),
            Slot::new(155.0, 8.0, 25.0, 8.0, "Rev.", &d("revisionIndex")).strong(3.5),
            Slot::new(0.0, 16.0, 60.0, 24.0, "Legal owner", &d("legalOwner")).strong(3.5),
            Slot::new(60.0, 16.0, 70.0, 14.0, "Title", &d("title")).strong(5.0),
            Slot::new(60.0, 30.0, 70.0, 10.0, "Supplementary title", &d("supplementaryTitle")),
            Slot::new(130.0, 16.0, 50.0, 24.0, "Identification number", &d("identificationNumber")).strong(3.5),
        ],
    }
}

/// The ASME Y14.1 field set in our own 178 by 50 mm layout: the owner and sign-offs on top, the
/// title, then the size, CAGE code, drawing number and revision, then scale, weight, material,
/// finish and sheet.
pub fn asme() -> Block {
    let d = |k: &str| format!("{{{{doc:{k}}}}}");
    Block {
        w_mm: 178.0,
        h_mm: 50.0,
        slots: vec![
            Slot::new(0.0, 0.0, 100.0, 10.0, "Company", &d("legalOwner")).strong(3.5),
            Slot::new(100.0, 0.0, 40.0, 10.0, "Drawn", &d("creator")),
            Slot::new(140.0, 0.0, 38.0, 10.0, "Date", &d("drawnDate")),
            Slot::new(0.0, 10.0, 100.0, 20.0, "Title", &d("title")).strong(5.0),
            Slot::new(100.0, 10.0, 40.0, 10.0, "Checked", &d("checkedBy")),
            Slot::new(140.0, 10.0, 38.0, 10.0, "Date", &d("checkedDate")),
            Slot::new(100.0, 20.0, 40.0, 10.0, "Approved", &d("approvalPerson")),
            Slot::new(140.0, 20.0, 38.0, 10.0, "Date", &d("approvedDate")),
            Slot::new(0.0, 30.0, 16.0, 12.0, "Size", &d("paperSize")).strong(3.5),
            Slot::new(16.0, 30.0, 30.0, 12.0, "CAGE code", &d("cageCode")),
            Slot::new(46.0, 30.0, 104.0, 12.0, "Dwg. no.", &d("drawingNumber")).strong(5.0),
            Slot::new(150.0, 30.0, 28.0, 12.0, "Rev.", &d("revisionIndex")).strong(3.5),
            Slot::new(0.0, 42.0, 30.0, 8.0, "Scale", &d("scale")),
            Slot::new(30.0, 42.0, 30.0, 8.0, "Weight", &d("weight")),
            Slot::new(60.0, 42.0, 40.0, 8.0, "Material", &d("material")),
            Slot::new(100.0, 42.0, 40.0, 8.0, "Finish", &d("finish")),
            Slot::new(140.0, 42.0, 38.0, 8.0, "Sheet", "{{sheet}} of {{sheets}}"),
        ],
    }
}

/// Clamp a millimetre value read from a block into a sane range.
fn mm(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, 10_000.0) } else { 0.0 }
}

/// Gap between a cell's rules and its text, in millimetres. The text shapes have no margins of
/// their own (the default four world units would shift text by a different amount at every
/// sheet scale), so this is the only inset.
pub const PAD_MM: f64 = 1.0;

/// Gap between a caption's top and its cell's top rule, in millimetres.
const CAPTION_TOP_MM: f64 = 0.4;

fn text_style(size_mm: f64, bold: bool, align: HAlign, v: VAlign) -> Style {
    Style {
        text_margins: crate::style_fields::Margins { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
        fill: Color::TRANSPARENT,
        line: Color::TRANSPARENT,
        line_width: 0.0,
        font_size: font_for_char_height(mm(size_mm).max(0.5)),
        font_color: INK,
        bold,
        h_align: align,
        v_align: v,
        ..Style::DEFAULT
    }
}

/// The shapes that draw `block` with its top left corner at `origin` (world units), at
/// `units_per_mm` world units per millimetre: the outer border, one bordered cell per slot, its
/// caption and its value. `id(role)` hands out each shape's id (`border`, `cell3`, `label3`,
/// `text3`); a role it refuses is left out. At most [`MAX_SLOTS`] slots are drawn.
pub fn shapes(block: &Block, origin: Point, units_per_mm: f64, id: impl Fn(&str) -> Option<String>) -> Vec<Shape> {
    let k = if units_per_mm.is_finite() && units_per_mm > 0.0 { units_per_mm.min(1000.0) } else { crate::sheet::WORLD_PER_MM };
    let at = |x: f64, y: f64, w: f64, h: f64| (origin.x + mm(x) * k, origin.y + mm(y) * k, (mm(w) * k).max(1.0), (mm(h) * k).max(1.0));
    let mut out = Vec::new();
    let mut push = |role: &str, (x, y, w, h): (f64, f64, f64, f64), style: Style, text: &str| {
        if let Some(id) = id(role) {
            let mut s = Shape::new(id, crate::ShapeKind::Rectangle, x, y, w, h);
            s.style = style;
            s.text = text.to_string();
            s.sanitize();
            out.push(s);
        }
    };
    let cell = Style { fill: Color::TRANSPARENT, line: INK, line_width: 0.35 * k, ..Style::DEFAULT };
    push("border", at(0.0, 0.0, block.w_mm, block.h_mm), Style { fill: Color::WHITE, line: INK, line_width: 0.7 * k, ..Style::DEFAULT }, "");
    for (i, s) in block.slots.iter().take(MAX_SLOTS).enumerate() {
        push(&format!("cell{i}"), at(s.x, s.y, s.w, s.h), cell.clone(), "");
        let pad = PAD_MM;
        if !s.label.is_empty() {
            push(
                &format!("label{i}"),
                at(s.x + pad, s.y + CAPTION_TOP_MM, (s.w - 2.0 * pad).max(1.0), CAPTION_MM),
                text_style(1.8, false, HAlign::Left, VAlign::Top),
                &s.label,
            );
        }
        // The value sits in the band under the caption, centred in it, so a short 8 mm cell
        // (Size, Sheet) keeps a clear gap between its caption and its value.
        let top = if s.label.is_empty() { 0.0 } else { CAPTION_TOP_MM + CAPTION_MM };
        push(
            &format!("text{i}"),
            at(s.x + pad, s.y + top, (s.w - 2.0 * pad).max(1.0), (s.h - top - CAPTION_TOP_MM).max(1.0)),
            text_style(s.size, s.bold, HAlign::Left, VAlign::Middle),
            &s.text,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_stay_inside_their_blocks_and_every_slot_is_a_field() {
        for b in [iso7200(), asme()] {
            assert!(b.w_mm <= 180.0, "ISO 7200 caps the width at 180 mm");
            for s in &b.slots {
                assert!(s.x + s.w <= b.w_mm + 1e-9 && s.y + s.h <= b.h_mm + 1e-9, "{s:?}");
                assert!(s.text.contains("{{"), "{s:?} shows a field");
            }
        }
        let area: f64 = iso7200().slots.iter().map(|s| s.w * s.h).sum();
        assert!((area - 180.0 * 40.0).abs() < 1e-6, "the ISO slots tile the block");
        let area: f64 = asme().slots.iter().map(|s| s.w * s.h).sum();
        assert!((area - 178.0 * 50.0).abs() < 1e-6, "the ASME slots tile the block");
    }

    #[test]
    fn shapes_take_ids_from_the_caller_and_scale_to_world_units() {
        let b = iso7200();
        let all = shapes(&b, Point::new(10.0, 20.0), 2.0, |r| Some(format!("x-{r}")));
        assert_eq!(all.len(), 1 + 3 * b.slots.len());
        assert_eq!((all[0].x, all[0].y, all[0].w, all[0].h), (10.0, 20.0, 360.0, 80.0));
        assert!(all.iter().any(|s| s.id == "x-text13" && s.text == "{{doc:title}}"));
        let some = shapes(&b, Point::new(0.0, 0.0), f64::NAN, |r| r.starts_with("cell").then(|| r.to_string()));
        assert_eq!(some.len(), b.slots.len());
    }
}
