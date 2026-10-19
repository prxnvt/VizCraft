//! The contextual Sheet tab, shown while the active page has a drawing sheet (set from Design >
//! Page Setup > Size, the Page Setup dialog or `sheet.set`): its standard, size and title block,
//! revisions, balloons and renumbering, and the symbol builders (Valve, Rebuild Symbols; both
//! also on the Edit menu).
//!
//! A contextual tab rather than a Design group: Design is full at 1280 points wide, and these
//! tools mean something only on a sheet, the way the Connector and Container tabs follow the
//! selection. The way onto a sheet stays on Design (Page Setup), so a plain page is one choice
//! away from one.

use super::Tab;
use crate::CraftApp;

fn has_sheet(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.sheet.is_some())
}

groups! {
    sheet,
    annotate,
    symbols,
}

/// The Sheet tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Sheet", has_sheet, groups())
}
