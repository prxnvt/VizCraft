//! File formats by name: a thin dispatcher to `vizcraft-formats`. Bytes in, bytes out, so the
//! same code serves the desktop (files), the CLI and the web build (downloads).
//!
//! Native files are `.vizcraft`. Names ending in `.json` (the Cascade 0 `.craft.json` files, or
//! a bare document) are read and written as native files too, so old paths keep working. Names
//! ending in `.drawio` or `.drawio.xml` are read and written as draw.io files, every page.
//!
//! Imports (`file.import`, `page.set`) go through [`import_bytes`]: draw.io XML, Mermaid text or
//! draw.io-style CSV, picked by [`import_format`].

use crate::EngineError;
use crate::doc::Document;

/// The file extension documents are saved with.
pub const EXTENSION: &str = "vizcraft";

/// Extensions read and written as native files, longest first so stripping prefers `.craft.json`.
const NATIVE_EXTENSIONS: [&str; 3] = ["craft.json", "vizcraft", "json"];

/// Extensions read and written as draw.io files, longest first.
const DRAWIO_EXTENSIONS: [&str; 2] = ["drawio.xml", "drawio"];

/// Export formats `export_bytes` knows, by name.
pub const EXPORT_FORMATS: [&str; 6] = ["svg", "png", "pdf", "drawio", "mermaid", "plantuml"];

/// Formats `import_bytes` knows, by name.
pub const IMPORT_FORMATS: [&str; 4] = ["drawio", "mermaid", "csv", "dbml"];

/// Largest file or text an import reads, in bytes. The parsers cap their own input too; this
/// stops a huge file being read into memory at all.
pub const MAX_IMPORT_BYTES: u64 = 32 * 1024 * 1024;

/// Encode `doc` for a file called `name` (the extension picks the format).
pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError> {
    if is_drawio(name) {
        return vizcraft_formats::drawio::save(doc).map_err(|e| fail(name, e));
    }
    if !is_native(name) {
        return Err(unsupported(name));
    }
    vizcraft_formats::native::save(doc).map_err(|e| fail(name, e))
}

/// Decode a file called `name`: `.vizcraft`, `.craft.json` or `.json`, wrapped or bare; or
/// `.drawio` and `.drawio.xml`.
pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError> {
    if is_drawio(name) {
        return vizcraft_formats::drawio::load(bytes).map_err(|e| fail(name, e));
    }
    if !is_native(name) {
        return Err(unsupported(name));
    }
    vizcraft_formats::native::load(bytes).map_err(|e| fail(name, e))
}

/// Read a file from disk for an import or a page command, refusing anything over
/// [`MAX_IMPORT_BYTES`] before reading it.
pub fn read_limited(path: &str) -> Result<Vec<u8>, EngineError> {
    let meta = std::fs::metadata(path).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(EngineError::Failed(format!("{path}: larger than {} MiB", MAX_IMPORT_BYTES / (1024 * 1024))));
    }
    std::fs::read(path).map_err(|e| EngineError::Failed(format!("{path}: {e}")))
}

/// The import format for a file called `name`, from its extension: `.drawio`, `.xml` (draw.io),
/// `.mmd`, `.mermaid` (Mermaid), `.csv` or `.dbml`. `None` for anything else.
pub fn import_format_for_name(name: &str) -> Option<&'static str> {
    if is_drawio(name) {
        return Some("drawio");
    }
    match extension(name).to_ascii_lowercase().as_str() {
        "xml" => Some("drawio"),
        "mmd" | "mermaid" => Some("mermaid"),
        "csv" => Some("csv"),
        "dbml" => Some("dbml"),
        _ => None,
    }
}

/// Guess the format of import text: XML starts with `<`; draw.io CSV starts with `#` directive
/// lines; anything else is read as Mermaid.
pub fn sniff_format(text: &[u8]) -> &'static str {
    let first = text.iter().copied().find(|b| !b.is_ascii_whitespace() && *b != 0xef && *b != 0xbb && *b != 0xbf);
    match first {
        Some(b'<') => "drawio",
        Some(b'#') => "csv",
        _ => "mermaid",
    }
}

/// The import format to use: `format` when given (it must be one of [`IMPORT_FORMATS`]), else
/// the name's extension, else a guess from the content.
pub fn import_format(name: Option<&str>, format: Option<&str>, bytes: &[u8]) -> Result<&'static str, EngineError> {
    if let Some(f) = format {
        let wanted = f.trim().trim_start_matches('.').to_ascii_lowercase();
        return IMPORT_FORMATS
            .iter()
            .find(|k| **k == wanted)
            .copied()
            .ok_or_else(|| EngineError::Params(format!("cannot import `{}`; supported: {}", crate::cmd::short(f), IMPORT_FORMATS.join(", "))));
    }
    Ok(name.and_then(import_format_for_name).unwrap_or_else(|| sniff_format(bytes)))
}

/// Read import content in `format` (one of [`IMPORT_FORMATS`]) as pages for `Document::adopt`.
pub fn import_bytes(format: &str, bytes: &[u8]) -> Result<vizcraft_formats::Imported, EngineError> {
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(EngineError::Failed(format!("the {format} source is larger than {} MiB", MAX_IMPORT_BYTES / (1024 * 1024))));
    }
    let text = || std::str::from_utf8(bytes).map_err(|_| EngineError::Failed(format!("the {format} source is not UTF-8 text")));
    let r = match format {
        "drawio" => vizcraft_formats::drawio::import(bytes),
        "mermaid" => vizcraft_formats::mermaid::import(text()?.trim_start_matches('\u{feff}')),
        "csv" => vizcraft_formats::csv::import(text()?.trim_start_matches('\u{feff}')),
        "dbml" => vizcraft_formats::dbml::import(text()?.trim_start_matches('\u{feff}')),
        other => return Err(EngineError::Params(format!("cannot import `{}`", crate::cmd::short(other)))),
    };
    r.map_err(|e| EngineError::Failed(e.to_string()))
}

/// The export format for a file called `name`: `format` when given (case-insensitive), else
/// the extension. Errors for anything not in [`EXPORT_FORMATS`].
pub fn export_format(name: &str, format: Option<&str>) -> Result<&'static str, EngineError> {
    let wanted = match format {
        Some(f) => f.trim().trim_start_matches('.').to_ascii_lowercase(),
        None => extension(name).to_ascii_lowercase(),
    };
    // Text forms answer to their usual extensions too.
    let wanted = match wanted.as_str() {
        "mmd" => "mermaid".to_string(),
        "puml" => "plantuml".to_string(),
        _ => wanted,
    };
    EXPORT_FORMATS
        .iter()
        .find(|f| **f == wanted)
        .copied()
        .ok_or_else(|| EngineError::Failed(format!("{name}: cannot export as `{wanted}`; supported: {}", EXPORT_FORMATS.join(", "))))
}

/// Export the first page of `doc` for a file called `name`, in `format` or the one its extension
/// names (PNG at scale 1).
pub fn export_bytes(name: &str, format: Option<&str>, doc: &Document) -> Result<Vec<u8>, EngineError> {
    export_page_bytes(name, format, doc, 0, 1.0)
}

/// Export page `page_index` of `doc` for a file called `name`, in `format` or the one its
/// extension names. `scale` is pixels per page unit for PNG (clamped by the renderer) and is
/// ignored otherwise; draw.io files always hold every page; PDF holds this one page.
pub fn export_page_bytes(name: &str, format: Option<&str>, doc: &Document, page_index: usize, scale: f64) -> Result<Vec<u8>, EngineError> {
    match export_format(name, format)? {
        "svg" => vizcraft_formats::svg::export_page(doc, page_index).map_err(|e| fail(name, e)),
        "png" => vizcraft_formats::png::render_page(doc, page_index, scale).map(|(png, _, _)| png).map_err(|e| fail(name, e)),
        "pdf" => vizcraft_formats::pdf::export_pages(doc, &[page_index], &Default::default()).map_err(|e| fail(name, e)),
        "drawio" => vizcraft_formats::drawio::save(doc).map_err(|e| fail(name, e)),
        f @ ("mermaid" | "plantuml") => export_text_page(name, Some(f), doc, page_index).map(|(b, _)| b),
        other => Err(EngineError::Failed(format!("{name}: cannot export as `{other}`"))),
    }
}

/// Export page `page_index` of `doc` as Mermaid or PlantUML text (the format from `format` or
/// the extension of `name`): the bytes and the warnings about what the text cannot hold.
pub fn export_text_page(name: &str, format: Option<&str>, doc: &Document, page_index: usize) -> Result<(Vec<u8>, Vec<String>), EngineError> {
    let (text, warnings) = match export_format(name, format)? {
        "mermaid" => vizcraft_formats::mermaid::export(doc, page_index),
        "plantuml" => vizcraft_formats::plantuml::write(doc, page_index),
        other => return Err(EngineError::Failed(format!("{name}: `{other}` is not a text format"))),
    }
    .map_err(|e| fail(name, e))?;
    Ok((text.into_bytes(), warnings))
}

/// `name` without a document extension (`Plans.vizcraft`, `Plans.craft.json` and `Plans.drawio`
/// all give `Plans`); other names are returned unchanged.
pub fn strip_extension(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in NATIVE_EXTENSIONS.iter().chain(DRAWIO_EXTENSIONS.iter()) {
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

/// Whether a file called `name` is read and written as a draw.io file.
pub fn is_drawio(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    DRAWIO_EXTENSIONS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Whether a file called `name` opens as a document (native or draw.io) rather than an import.
pub fn is_document(name: &str) -> bool {
    is_native(name) || is_drawio(name)
}

/// The text after the last dot of the file name, or empty.
fn extension(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

fn unsupported(name: &str) -> EngineError {
    EngineError::Failed(format!("{name}: only .{EXTENSION}, .drawio (and older .craft.json) files can be opened and saved"))
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
        for name in ["a.drawio", "B.DRAWIO", "c.drawio.xml"] {
            let bytes = save_bytes(name, &doc).unwrap();
            assert!(bytes.starts_with(b"<mxfile"), "{name}");
            let back = load_bytes(name, &bytes).unwrap();
            assert_eq!(back.page().unwrap().shapes.len(), doc.page().unwrap().shapes.len());
            assert_eq!(back.page().unwrap().connectors.len(), doc.page().unwrap().connectors.len());
        }
        assert!(load_bytes("x.drawio", b"<mxfile><diagram").is_err());
        assert_eq!(export_format("out.drawio", None).unwrap(), "drawio");
        assert!(export_bytes("out.drawio", None, &doc).unwrap().starts_with(b"<mxfile"));
        assert_eq!(strip_extension("Plans.drawio"), "Plans");
        assert_eq!(strip_extension("Plans.drawio.xml"), "Plans");
        assert!(load_bytes("x.svg", b"{}").is_err());
        assert!(load_bytes("x.vizcraft", b"{not json").is_err());

        assert_eq!(export_format("out.SVG", None).unwrap(), "svg");
        assert_eq!(export_format("out", Some("svg")).unwrap(), "svg");
        assert_eq!(export_format("out.png", Some(".SVG")).unwrap(), "svg");
        assert_eq!(export_format("out.PNG", None).unwrap(), "png");
        assert!(export_format("out.jpg", None).is_err());
        assert!(export_bytes("out.png", None, &doc).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(export_page_bytes("out.png", None, &doc, 7, 1.0).is_err());
        assert!(export_page_bytes("out.svg", None, &doc, 7, 1.0).is_err());
        assert_eq!(export_format("out.svg", Some("pdf")).unwrap(), "pdf");
        assert!(export_format("out.svg", Some("jpeg")).is_err());
        assert!(export_format("dir.svg/out", None).is_err());
        assert!(export_bytes("out.svg", None, &doc).unwrap().starts_with(b"<?xml"));
        assert!(export_bytes("out.pdf", None, &doc).unwrap().starts_with(b"%PDF-"));
        assert!(export_page_bytes("out.pdf", None, &doc, 7, 1.0).is_err());

        assert_eq!(strip_extension("/tmp/Plans.vizcraft"), "/tmp/Plans");
        assert_eq!(strip_extension("Plans.craft.json"), "Plans");
        assert_eq!(strip_extension("Plans.json"), "Plans");
        assert_eq!(strip_extension("Plans.svg"), "Plans.svg");
        assert_eq!(strip_extension("\u{e9}.VizCraft"), "\u{e9}");
    }

    #[test]
    fn import_formats_come_from_the_name_the_param_or_the_text() {
        assert_eq!(import_format(Some("a.drawio"), None, b"").unwrap(), "drawio");
        assert_eq!(import_format(Some("a.XML"), None, b"").unwrap(), "drawio");
        assert_eq!(import_format(Some("a.mmd"), None, b"").unwrap(), "mermaid");
        assert_eq!(import_format(Some("a.mermaid"), None, b"").unwrap(), "mermaid");
        assert_eq!(import_format(Some("a.csv"), None, b"").unwrap(), "csv");
        assert_eq!(import_format(Some("a.csv"), Some("drawio"), b"").unwrap(), "drawio");
        assert!(import_format(None, Some("pdf"), b"").is_err());
        assert_eq!(import_format(None, None, b"  \n<mxfile/>").unwrap(), "drawio");
        assert_eq!(import_format(None, None, b"# label: %name%").unwrap(), "csv");
        assert_eq!(import_format(Some("notes.txt"), None, b"flowchart LR\nA-->B").unwrap(), "mermaid");
        assert!(import_bytes("csv", b"\xff\xfe").is_err());
        assert!(import_bytes("nope", b"").is_err());
    }
}
