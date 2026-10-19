//! The Process tab: diagram validation (on pages without a notation tab, which shows the same
//! controls in its own Check group) and subprocesses.

use super::Tab;
use crate::CraftApp;

mod diagram_validation;
mod subprocess;

pub(in crate::ribbon) use diagram_validation::controls as validation_controls;

/// The Process tab. While the page has a notation tab, that tab holds the validation controls and
/// the Process tab keeps Subprocess only; free pages keep Diagram Validation here, so validation is
/// always one tab away.
pub(super) fn tab(app: &CraftApp) -> Tab {
    let mut groups = Vec::new();
    if !super::notation_tab_shown(app) {
        groups.push(diagram_validation::group());
    }
    groups.push(subprocess::group());
    Tab::new("Process", groups)
}
