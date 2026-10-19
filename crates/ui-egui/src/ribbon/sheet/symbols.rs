//! Sheet > Symbols: the Valve builder dialog (`ui.valve`, over `glyph.builders` and
//! `glyph.build`) and Rebuild Symbols (`glyph.rebuild`), which redraws the page's builder
//! symbols from their parameters at their true size.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Sheet tab's Symbols group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Symbols", vec![Big { id: "ui.valve", icon: "symbol" }, Big { id: "glyph.rebuild", icon: "refresh" }])
}
