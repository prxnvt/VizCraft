//! The native `.vizcraft` file: pretty-printed JSON
//! `{"format": "vizcraft", "version": 3, "model": {...}, "document": {...}}`.
//!
//! Version 3 holds the semantic model (`vizcraft_model::Model`: elements, relationships, packs,
//! with its own `schemaVersion`) and the diagram document (pages, shapes, connectors). Shapes and
//! connectors name the model items they draw; the model section is left out when the model is
//! empty, so free-drawing documents stay as small as before.
//!
//! Older files load as free drawing with an empty model: version 2 held the diagram document
//! only; version 1 was the placeholder of Cascade 0 (title and metadata only, saved as
//! `.craft.json`), which loads as a document with that title and metadata and one blank page,
//! because the loader fills missing fields with defaults. A bare document without the wrapper
//! loads too, also with an empty model.

use serde::Serialize;
use serde_json::Value;
use vizcraft_diagram::Document;
use vizcraft_diagram::vizcraft_model::{self, Model};

use crate::FormatError;

/// The `format` tag every native file carries.
pub const FORMAT_TAG: &str = "vizcraft";

/// The version this build writes, and the newest it reads.
pub const VERSION: u32 = 3;

/// Largest input [`load`] accepts. Far above any real diagram, low enough that a hostile file
/// cannot make the parser allocate without bound.
pub const MAX_INPUT: usize = 64 * 1024 * 1024;

/// The wrapper as written; borrows the document and model so saving does not clone them. Field
/// order is the key order on disk.
#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a Model>,
    document: &'a Document,
}

/// Encode `doc` as a version 3 native file. The model section is written only when the model
/// has packs, elements or relationships.
pub fn save(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let model = (!doc.model.is_empty()).then_some(&doc.model);
    let file = FileOut { format: FORMAT_TAG, version: VERSION, model, document: doc };
    let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| FormatError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decode a native file: a version 1, 2 or 3 wrapper, or a bare document.
///
/// A wrapper is any JSON object with a `format` key. Its tag must be `vizcraft`, its `version`
/// a number no newer than [`VERSION`] (a missing version reads as 1) and its `document` an
/// object. In a version 3 wrapper an object `model` section is read by the model's tolerant
/// loader, after checking that its `schemaVersion` is no newer than
/// [`vizcraft_model::SCHEMA_VERSION`]; a `model` that is not an object reads as an empty model.
/// Everything inside the document is checked by the diagram's tolerant loader, which clamps
/// numbers and drops unreadable items; view references to model items that do not exist are
/// cleared, so those shapes and connectors load as free drawing.
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
    let (doc_value, model) = match obj.get("format") {
        None => (Value::Object(obj), Model::new()),
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
            let doc_value = match obj.remove("document") {
                Some(d @ Value::Object(_)) => d,
                Some(_) => return Err(FormatError::Invalid("the document is not an object".into())),
                None => return Err(FormatError::Invalid("the file has no document".into())),
            };
            // Versions 1 and 2 had no model; a stray `model` key in them is not ours to read.
            let model = if version >= 3 { read_model(obj.remove("model"))? } else { Model::new() };
            (doc_value, model)
        }
    };
    // The document's own loader leaves view references alone; with the model set, one sanitize
    // repairs them against it.
    let mut doc: Document = serde_json::from_value(doc_value).map_err(|e| FormatError::Invalid(e.to_string()))?;
    doc.model = model;
    doc.sanitize();
    Ok(doc)
}

/// Read the version 3 `model` section. Missing or not an object: an empty model. A
/// `schemaVersion` newer than this build reads is refused, so a newer model is never silently
/// half-read and then overwritten on save.
fn read_model(section: Option<Value>) -> Result<Model, FormatError> {
    let Some(section @ Value::Object(_)) = section else {
        return Ok(Model::new());
    };
    if let Some(found) = section.get("schemaVersion").and_then(schema_number)
        && found > u64::from(vizcraft_model::SCHEMA_VERSION)
    {
        return Err(FormatError::TooNew { found, supported: vizcraft_model::SCHEMA_VERSION });
    }
    // The model's fields are all lenient, so this only fails on input serde_json cannot map at
    // all; that reads as an empty model, like the rest of the tolerant loading.
    let mut model: Model = serde_json::from_value(section).unwrap_or_default();
    // An older, missing or unreadable schema version is upgraded on load: this build writes
    // its own version from now on.
    model.schema_version = vizcraft_model::SCHEMA_VERSION;
    Ok(model)
}

/// A schema version as a whole number: integers as they are, and finite positive fractions
/// rounded up so `1.5` counts as newer than 1. Anything else is not a version.
fn schema_number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        let f = v.as_f64().filter(|f| f.is_finite() && *f > 0.0)?;
        // `as` saturates for huge values, which still reads as too new.
        Some(f.ceil() as u64)
    })
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
