//! Topology commands (`topology.*`): the Cloud Topology page as Mermaid `architecture-beta`
//! text. Reading that text needs no command of its own: `file.import {format: "mermaid"}` reads
//! it. The command takes no path, so it never writes files; a later code pane wraps it.

use serde_json::{Value, json};

use super::model::page_param;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the topology commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("topology.toMermaid", "Copy as Mermaid Architecture", "Notation > Code", to_mermaid)
            .params(r#"{"page"?: page} -> {"text", "warnings"}; the page's Cloud Topology shapes as architecture-beta text; labels and steps the grammar cannot hold come back as warnings"#),
    ]
}

fn to_mermaid(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let (text, warnings) = vizcraft_formats::mermaid::architecture::write(&s.doc, i).map_err(|e| EngineError::Failed(e.to_string()))?;
    Ok(json!({"text": text, "warnings": warnings}))
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;
