//! The document file format: pretty JSON in a `.craft.json` file. Bytes in, bytes out, so the
//! same code serves the desktop (files), the CLI and the web build (downloads).

use crate::EngineError;
use crate::doc::Document;

/// The file extension documents are saved with.
pub const EXTENSION: &str = "craft.json";

/// The file wrapper: a format tag and version, so a later model can read old files.
#[derive(serde::Serialize, serde::Deserialize)]
struct File {
    format: String,
    version: u32,
    document: Document,
}

/// Encode `doc` for a file called `name` (the name picks the format; only JSON exists today).
pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError> {
    if !is_json(name) {
        return Err(EngineError::Failed(format!("{name}: only .{EXTENSION} files are supported")));
    }
    let file = File { format: "basecraft".into(), version: 1, document: doc.clone() };
    serde_json::to_vec_pretty(&file).map_err(|e| EngineError::Failed(format!("{name}: {e}")))
}

/// Decode a file called `name`. A bare `Document` (no wrapper) is accepted too.
pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError> {
    if !is_json(name) {
        return Err(EngineError::Failed(format!("{name}: only .{EXTENSION} files are supported")));
    }
    let bad = |e: serde_json::Error| EngineError::Failed(format!("{name}: {e}"));
    let mut doc = match serde_json::from_slice::<File>(bytes) {
        Ok(f) => f.document,
        Err(_) => serde_json::from_slice::<Document>(bytes).map_err(bad)?,
    };
    doc.ensure_nonempty();
    Ok(doc)
}

fn is_json(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".json")
}
