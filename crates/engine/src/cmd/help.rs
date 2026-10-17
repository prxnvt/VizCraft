//! Help menu: About. The engine has no dialogs, so it asks the UI to show one.

use serde_json::json;

use crate::CommandSpec;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("help.about", "About VizCraft", "Help", |s, _| {
        s.ui_requests.push(json!({"open": "about"}));
        Ok(json!({"version": env!("CARGO_PKG_VERSION")}))
    })]
}
