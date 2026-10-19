//! Three-way merges of hand-edited generated files, and the unified diffs the preview shows.
//!
//! When an owned file on disk no longer matches the checksum in the manifest, someone edited it.
//! The new text is merged with [`diffy::merge`]: the base is the last generated copy, "ours" is
//! the file on disk and "theirs" is the new generation, so a hand edit to lines generation did not
//! touch survives and an edit to the same lines is a conflict that stops the run.

/// Largest diff kept for the preview, in bytes; a longer one is cut with a note.
pub const MAX_DIFF_BYTES: usize = 256 * 1024;

/// Merge `ours` (on disk) and `theirs` (new) from `base` (last generated). `Err` holds the text
/// with conflict markers, for the preview.
pub fn three_way(base: &str, ours: &str, theirs: &str) -> Result<String, String> {
    if ours == base {
        return Ok(theirs.to_string());
    }
    if theirs == base || ours == theirs {
        return Ok(ours.to_string());
    }
    diffy::merge(base, ours, theirs)
}

/// A unified diff from `old` to `new` for `path`, with `a/` and `b/` headers; empty when they are
/// the same. Cut at [`MAX_DIFF_BYTES`].
pub fn unified(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let mut opts = diffy::DiffOptions::new();
    opts.set_original_filename(format!("a/{path}")).set_modified_filename(format!("b/{path}"));
    let text = opts.create_patch(old, new).to_string();
    if text.len() <= MAX_DIFF_BYTES {
        return text;
    }
    let mut cut = crate::clip(&text, MAX_DIFF_BYTES).to_string();
    if let Some(nl) = cut.rfind('\n') {
        cut.truncate(nl + 1);
    }
    cut.push_str("... diff cut at 256 KiB\n");
    cut
}
