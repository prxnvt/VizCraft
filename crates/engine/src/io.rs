//! File formats by name: a thin dispatcher to `vizcraft-formats`. Bytes in, bytes out, so the
//! same code serves the desktop (files), the CLI and the web build (downloads).
//!
//! Native files are `.vizcraft`. Names ending in `.json` (the Cascade 0 `.craft.json` files, or
//! a bare document) are read and written as native files too, so old paths keep working.

use crate::EngineError;
use crate::doc::Document;

/// The file extension documents are saved with.
pub const EXTENSION: &str = "vizcraft";

/// Extensions read and written as native files, longest first so stripping prefers `.craft.json`.
const NATIVE_EXTENSIONS: [&str; 3] = ["craft.json", "vizcraft", "json"];

/// Export formats `export_bytes` knows, by name.
pub const EXPORT_FORMATS: [&str; 1] = ["svg"];

/// Encode `doc` for a file called `name` (the extension picks the format).
pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError> {
    if !is_native(name) {
        return Err(unsupported(name));
    }
    vizcraft_formats::native::save(doc).map_err(|e| fail(name, e))
}

/// Decode a file called `name`: `.vizcraft`, `.craft.json` or `.json`, wrapped or bare.
pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError> {
    if !is_native(name) {
        return Err(unsupported(name));
    }
    vizcraft_formats::native::load(bytes).map_err(|e| fail(name, e))
}

/// The export format for a file called `name`: `format` when given (case-insensitive), else
/// the extension. Errors for anything not in [`EXPORT_FORMATS`].
pub fn export_format(name: &str, format: Option<&str>) -> Result<&'static str, EngineError> {
    let wanted = match format {
        Some(f) => f.trim().trim_start_matches('.').to_ascii_lowercase(),
        None => extension(name).to_ascii_lowercase(),
    };
    EXPORT_FORMATS
        .iter()
        .find(|f| **f == wanted)
        .copied()
        .ok_or_else(|| EngineError::Failed(format!("{name}: cannot export as `{wanted}`; supported: {}", EXPORT_FORMATS.join(", "))))
}

/// Export the first page of `doc` for a file called `name`, in `format` or the one its extension
/// names.
pub fn export_bytes(name: &str, format: Option<&str>, doc: &Document) -> Result<Vec<u8>, EngineError> {
    match export_format(name, format)? {
        "svg" => vizcraft_formats::svg::export_page(doc, 0).map_err(|e| fail(name, e)),
        other => Err(EngineError::Failed(format!("{name}: cannot export as `{other}`"))),
    }
}

/// `name` without a native extension (`Plans.vizcraft` and `Plans.craft.json` both give
/// `Plans`); other names are returned unchanged.
pub fn strip_extension(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in NATIVE_EXTENSIONS {
        if lower.ends_with(&format!(".{ext}")) {
            // ASCII lowercasing keeps byte offsets, and the suffix is ASCII, so this is a char
            // boundary.
            return name.get(..name.len() - ext.len() - 1).unwrap_or(name);
        }
    }
    name
}

/// Whether a file called `name` is read and written as a native file.
pub fn is_native(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NATIVE_EXTENSIONS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// The text after the last dot of the file name, or empty.
fn extension(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

fn unsupported(name: &str) -> EngineError {
    EngineError::Failed(format!("{name}: only .{EXTENSION} (and older .craft.json) files can be opened and saved"))
}

fn fail(name: &str, e: vizcraft_formats::FormatError) -> EngineError {
    EngineError::Failed(format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_pick_the_format() {
        let doc = crate::sample::sample_document();
        for name in ["a.vizcraft", "A.VIZCRAFT", "old.craft.json", "bare.json"] {
            let bytes = save_bytes(name, &doc).unwrap();
            assert_eq!(load_bytes(name, &bytes).unwrap(), doc, "{name}");
        }
        assert!(save_bytes("x.txt", &doc).is_err());
        assert!(load_bytes("x.svg", b"{}").is_err());
        assert!(load_bytes("x.vizcraft", b"{not json").is_err());

        assert_eq!(export_format("out.SVG", None).unwrap(), "svg");
        assert_eq!(export_format("out", Some("svg")).unwrap(), "svg");
        assert_eq!(export_format("out.png", Some(".SVG")).unwrap(), "svg");
        assert!(export_format("out.png", None).is_err());
        assert!(export_format("out.svg", Some("pdf")).is_err());
        assert!(export_format("dir.svg/out", None).is_err());
        assert!(export_bytes("out.svg", None, &doc).unwrap().starts_with(b"<?xml"));
        assert!(export_bytes("out.pdf", None, &doc).is_err());

        assert_eq!(strip_extension("/tmp/Plans.vizcraft"), "/tmp/Plans");
        assert_eq!(strip_extension("Plans.craft.json"), "Plans");
        assert_eq!(strip_extension("Plans.json"), "Plans");
        assert_eq!(strip_extension("Plans.svg"), "Plans.svg");
        assert_eq!(strip_extension("\u{e9}.VizCraft"), "\u{e9}");
    }
}
