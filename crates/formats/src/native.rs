//! The native `.vizcraft` file: pretty-printed JSON
//! `{"format": "vizcraft", "version": 2, "document": {...}}`.
//!
//! Version 2 holds the diagram document (pages, shapes, connectors). Version 1 was the placeholder
//! model of Cascade 0 (title and metadata only, saved as `.craft.json`); it loads as a document
//! with that title and metadata and one blank page, because the model's loader fills missing
//! fields with defaults. A bare document without the wrapper loads too.

use serde::Serialize;
use serde_json::Value;
use vizcraft_diagram::Document;

use crate::FormatError;

/// The `format` tag every native file carries.
pub const FORMAT_TAG: &str = "vizcraft";

/// The version this build writes, and the newest it reads.
pub const VERSION: u32 = 2;

/// Largest input [`load`] accepts. Far above any real diagram, low enough that a hostile file
/// cannot make the parser allocate without bound.
pub const MAX_INPUT: usize = 64 * 1024 * 1024;

/// The wrapper as written; borrows the document so saving does not clone it.
#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u32,
    document: &'a Document,
}

/// Encode `doc` as a version 2 native file.
pub fn save(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let file = FileOut { format: FORMAT_TAG, version: VERSION, document: doc };
    let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| FormatError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decode a native file: a version 1 or 2 wrapper, or a bare document.
///
/// A wrapper is any JSON object with a `format` key. Its tag must be `vizcraft`, its `version`
/// a number no newer than [`VERSION`] (a missing version reads as 1) and its `document` an
/// object. Everything inside the document is checked by the model's tolerant loader, which
/// clamps numbers and drops unreadable items.
pub fn load(bytes: &[u8]) -> Result<Document, FormatError> {
    if bytes.len() > MAX_INPUT {
        return Err(FormatError::TooLarge { len: bytes.len(), max: MAX_INPUT });
    }
    // Parse to a value first so the wrapper can be inspected before the document is read; the
    // size cap above bounds the cost, and serde_json bounds the nesting depth.
    let value: Value = serde_json::from_slice(bytes).map_err(|e| FormatError::Invalid(e.to_string()))?;
    let Value::Object(mut obj) = value else {
        return Err(FormatError::Invalid("expected a JSON object".into()));
    };
    let doc_value = match obj.get("format") {
        None => Value::Object(obj),
        Some(tag) => {
            let tag = tag.as_str().unwrap_or_default();
            if tag != FORMAT_TAG {
                return Err(FormatError::WrongFormat(tag.chars().take(64).collect()));
            }
            let version = match obj.get("version") {
                None => 1,
                Some(v) => v.as_u64().ok_or_else(|| FormatError::Invalid("the version is not a whole number".into()))?,
            };
            if version > u64::from(VERSION) {
                return Err(FormatError::TooNew { found: version, supported: VERSION });
            }
            match obj.remove("document") {
                Some(d @ Value::Object(_)) => d,
                Some(_) => return Err(FormatError::Invalid("the document is not an object".into())),
                None => return Err(FormatError::Invalid("the file has no document".into())),
            }
        }
    };
    let mut doc: Document = serde_json::from_value(doc_value).map_err(|e| FormatError::Invalid(e.to_string()))?;
    // Deserialization already sanitizes; this is cheap and keeps the guarantee local.
    doc.sanitize();
    Ok(doc)
}
