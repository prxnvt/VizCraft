//! Keys (legends): a captioned table that explains the symbols, line kinds and step numbers a
//! page uses, drawn with the title block machinery ([`crate::title_block`]).
//!
//! [`block`] lays a key out in millimetres: a heading, then one row per [`KeyRow`], a sample cell
//! on the left and the label on the right. The engine fills the sample cells with a small shape
//! from the master, a short line of the connector kind or a step badge, groups everything and
//! marks each item with [`KEY_EXT`], so the drawing checks and the layout leave the key alone.

use crate::model::{Connector, Shape};
use crate::title_block::{Block, Slot};

/// The `ext` key every shape and connector of a generated key carries (its value is the key
/// group's id).
pub const KEY_EXT: &str = "vizcraft.key";

/// Most rows a key lists before an "and N more" row.
pub const MAX_ROWS: usize = 40;

/// Width of a sample cell, in millimetres.
pub const SAMPLE_MM: f64 = 20.0;

/// Width of a label cell, in millimetres.
pub const LABEL_MM: f64 = 50.0;

/// Height of one row, in millimetres.
pub const ROW_MM: f64 = 9.0;

/// Height of the heading, in millimetres.
pub const HEADING_MM: f64 = 8.0;

/// What a key row's sample shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Sample {
    /// A small shape drawn from this master.
    Glyph(String),
    /// A short line of this connector kind.
    Line(String),
    /// A step badge with this number.
    Step(u16),
}

/// One row of a key.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRow {
    /// The text beside the sample.
    pub label: String,
    /// The sample.
    pub sample: Sample,
}

/// Where one row's sample is drawn, in millimetres from the block's top left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleCell {
    /// The row's index in the rows given.
    pub row: usize,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// What to draw.
    pub sample: Sample,
}

/// The key's table: a heading slot across the top, then per row (at most [`MAX_ROWS`]) an empty
/// sample cell and a label cell, then an "and N more" row when rows were left out. The sample
/// cells come back separately for the caller to fill.
pub fn block(heading: &str, rows: &[KeyRow]) -> (Block, Vec<SampleCell>) {
    let w = SAMPLE_MM + LABEL_MM;
    let mut slots = vec![Slot::new(0.0, 0.0, w, HEADING_MM, "", heading).strong(3.5)];
    let mut cells = Vec::new();
    let mut y = HEADING_MM;
    for (i, r) in rows.iter().take(MAX_ROWS).enumerate() {
        slots.push(Slot::new(0.0, y, SAMPLE_MM, ROW_MM, "", ""));
        slots.push(Slot::new(SAMPLE_MM, y, LABEL_MM, ROW_MM, "", &r.label));
        cells.push(SampleCell { row: i, x: 0.0, y, w: SAMPLE_MM, h: ROW_MM, sample: r.sample.clone() });
        y += ROW_MM;
    }
    if rows.len() > MAX_ROWS {
        slots.push(Slot::new(0.0, y, w, ROW_MM, "", &format!("and {} more", rows.len() - MAX_ROWS)));
        y += ROW_MM;
    }
    (Block { w_mm: w, h_mm: y, slots }, cells)
}

/// Whether `s` belongs to a generated key.
pub fn is_key(s: &Shape) -> bool {
    s.ext.contains_key(KEY_EXT)
}

/// Whether `c` belongs to a generated key (a line sample).
pub fn is_key_connector(c: &Connector) -> bool {
    c.ext.contains_key(KEY_EXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_become_slots_and_sample_cells_with_a_cap() {
        let rows: Vec<KeyRow> = (0..3).map(|i| KeyRow { label: format!("R{i}"), sample: Sample::Step(i) }).collect();
        let (b, cells) = block("Key", &rows);
        assert_eq!(b.slots.len(), 1 + 2 * 3);
        assert_eq!(cells.len(), 3);
        assert_eq!(b.h_mm, HEADING_MM + 3.0 * ROW_MM);
        assert!(b.slots.iter().any(|s| s.text == "R2"));
        let many: Vec<KeyRow> = (0..50).map(|i| KeyRow { label: format!("R{i}"), sample: Sample::Line("basic.arrow".into()) }).collect();
        let (b, cells) = block("Key", &many);
        assert_eq!(cells.len(), MAX_ROWS);
        assert!(b.slots.last().is_some_and(|s| s.text == "and 10 more"));
        let mut s = Shape::default();
        assert!(!is_key(&s));
        s.ext.insert(KEY_EXT.into(), "g".into());
        assert!(is_key(&s));
    }
}
