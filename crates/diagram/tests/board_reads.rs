//! No code reads a page's stored sheet size (`width`, `height`) where it should ask
//! `Page::frame`. The sheet size means something only on a fixed board, so a consumer that reads
//! it directly draws, exports or fits the wrong rectangle on an infinite one. This test walks the
//! source tree and fails on a direct read outside the files that own the sheet size.
//!
//! It lives here, and not in `xtask`, because it is a plain `cargo test` away for anyone who
//! edits the model. It scans the non-test part of each file (everything before the first
//! `#[cfg(test)]`) under `crates/*/src` and `apps/*/src`, and skips the `layout` and `routing`
//! crates, whose `width` and `height` belong to nodes, not pages.

use std::fs;
use std::path::{Path, PathBuf};

/// Files that own or deliberately use the stored sheet size, relative to the workspace root.
const ALLOWED: &[(&str, &str)] = &[
    ("crates/diagram/src/model.rs", "the field itself, `Page::rect` and `Page::sanitize`"),
    ("crates/diagram/src/board.rs", "`Page::frame`"),
    ("crates/diagram/src/autolayout.rs", "`grow_page` grows the sheet of a fixed board"),
    ("crates/diagram/src/decor.rs", "generated background pages copy the sheet they decorate"),
    ("crates/engine/src/cmd/page.rs", "`page.setup`, `page.list`, auto size and `page.fitToDrawing` set and report the sheet"),
    ("crates/engine/src/cmd/file.rs", "an import carries its sheet size over"),
    ("crates/formats/src/drawio.rs", "a draw.io page carries its sheet size over"),
    ("crates/formats/src/drawio/read.rs", "a draw.io page brings its sheet size"),
    ("crates/formats/src/mermaid/common.rs", "an imported diagram grows the sheet to fit"),
    ("crates/ui-egui/src/page_setup.rs", "the Page Setup dialog edits the sheet size"),
    ("crates/ui-egui/src/ribbon/design/page_setup.rs", "the Orientation and Size menus show the sheet size"),
];

/// Names a `Page` goes by in the code that reads one.
const RECEIVERS: &[&str] = &["page", "pg", "p", "fg", "bp", "first", "from", "target", "deco_page", "src"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

fn is_test_file(rel: &str) -> bool {
    rel.contains("/tests/") || rel.ends_with("/tests.rs") || rel.ends_with("_tests.rs")
}

fn ident_before(s: &str) -> &str {
    let start = s.rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |i| i + 1);
    s.get(start..).unwrap_or_default()
}

/// Whether `line` reads `.width` or `.height` off something that is a page.
fn reads_sheet_size(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or_default();
    for field in [".width", ".height"] {
        for (at, _) in code.match_indices(field) {
            let after = code.get(at + field.len()..).unwrap_or_default();
            if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = after.trim_start();
            // A call (`r.width()`), or an assignment (`page.width = w`, which writes the sheet and
            // reads nothing), is not a read.
            if rest.starts_with('(') || (rest.starts_with('=') && !rest.starts_with("==")) {
                continue;
            }
            let before = code.get(..at).unwrap_or_default();
            let named = RECEIVERS.contains(&ident_before(before));
            let chained = ["page().unwrap()", "page_mut().unwrap()", "page().expect(", "page_mut()?"].iter().any(|s| before.ends_with(s));
            let indexed = before.ends_with(']') && before.contains("pages[");
            if named || chained || indexed {
                return true;
            }
        }
    }
    false
}

#[test]
fn nothing_reads_the_sheet_size_where_the_frame_belongs() {
    let root = root();
    let mut files = Vec::new();
    for top in ["crates", "apps"] {
        let Ok(members) = fs::read_dir(root.join(top)) else { continue };
        for m in members.flatten() {
            let name = m.file_name().to_string_lossy().to_string();
            if top == "crates" && (name == "layout" || name == "routing") {
                continue;
            }
            rust_files(&m.path().join("src"), &mut files);
        }
    }
    assert!(files.len() > 50, "the scan found only {} files; the workspace layout changed", files.len());
    let mut stray = Vec::new();
    for path in files {
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if is_test_file(&rel) || ALLOWED.iter().any(|(f, _)| *f == rel) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        for (n, line) in text.lines().enumerate() {
            if line.contains("#[cfg(test)]") {
                break;
            }
            if reads_sheet_size(line) {
                stray.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "read `Page::frame()` instead of the stored sheet size, or add the file to ALLOWED with a reason:\n{}",
        stray.join("\n")
    );
}

#[test]
fn the_scan_recognises_reads_and_ignores_writes_and_calls() {
    assert!(reads_sheet_size("let w = page.width;"));
    assert!(reads_sheet_size("if p.height > p.width {"));
    assert!(reads_sheet_size("let h = s.doc.page().unwrap().height;"));
    assert!(reads_sheet_size("let w = doc.pages[0].width;"));
    assert!(!reads_sheet_size("page.width = w;"));
    assert!(!reads_sheet_size("let w = r.width();"));
    assert!(!reads_sheet_size("let w = frame.width();"));
    assert!(!reads_sheet_size("let w = stroke.width;"));
    assert!(!reads_sheet_size("// page.width is the sheet"));
}
