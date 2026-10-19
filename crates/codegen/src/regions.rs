//! Protected regions for single-file targets.
//!
//! A target that writes one file cannot keep hand-written code in a separate file, so it emits an
//! empty block per element, keyed by element id (never by position or name):
//!
//! ```text
//! <!-- vizcraft:begin e7 -->
//! hand-written text survives every regeneration
//! <!-- vizcraft:end e7 -->
//! ```
//!
//! [`carry`] copies each block's body from the file on disk into the new text. A block whose id
//! has no place in the new text (the element was deleted or left the scope) is kept at the end of
//! the file with an issue, so nothing hand-written is ever dropped.

use std::collections::HashMap;

use vizcraft_model::{Issue, Severity};

/// Longest region id read, in bytes.
const MAX_REGION_ID: usize = 256;

/// One region: its element id and the lines between its markers (each ending in `\n`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The element id it is keyed by.
    pub id: String,
    /// The text between the markers.
    pub body: String,
}

/// The begin marker line for `id` in a comment syntax.
pub fn begin(id: &str, open: &str, close: &str) -> String {
    marker_line("begin", id, open, close)
}

/// The end marker line for `id` in a comment syntax.
pub fn end(id: &str, open: &str, close: &str) -> String {
    marker_line("end", id, open, close)
}

fn marker_line(word: &str, id: &str, open: &str, close: &str) -> String {
    if close.is_empty() { format!("{open} vizcraft:{word} {id}") } else { format!("{open} vizcraft:{word} {id} {close}") }
}

/// The id of a marker line of kind `word` (`begin` or `end`), if `line` is one.
fn marker<'a>(line: &'a str, word: &str, open: &str, close: &str) -> Option<&'a str> {
    let t = line.trim();
    let t = t.strip_prefix(open)?;
    let t = if close.is_empty() { t } else { t.strip_suffix(close)? };
    let t = t.trim().strip_prefix("vizcraft:")?.strip_prefix(word)?;
    if !t.starts_with(char::is_whitespace) {
        return None;
    }
    let id = t.trim();
    (!id.is_empty() && id.len() <= MAX_REGION_ID && !id.contains(char::is_whitespace)).then_some(id)
}

/// The regions of `text`, in order. A begin with no matching end is not a region; a second
/// begin before the end starts over.
pub fn blocks(text: &str, open: &str, close: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in text.split_inclusive('\n') {
        if let Some(id) = marker(line, "begin", open, close) {
            cur = Some((id.to_string(), String::new()));
            continue;
        }
        if let Some(id) = marker(line, "end", open, close) {
            if let Some((open_id, body)) = cur.take()
                && open_id == id
            {
                out.push(Block { id: open_id, body });
            }
            continue;
        }
        if let Some((_, body)) = cur.as_mut() {
            body.push_str(line);
        }
    }
    out
}

/// `new` with the body of every region of `disk` copied in. Regions with no place in `new` are
/// appended at the end, each with an issue naming `file`.
pub fn carry(file: &str, new: &str, disk: &str, open: &str, close: &str) -> (String, Vec<Issue>) {
    let old = blocks(disk, open, close);
    if old.is_empty() {
        return (new.to_string(), Vec::new());
    }
    let mut bodies: HashMap<&str, &str> = HashMap::new();
    for b in &old {
        bodies.entry(b.id.as_str()).or_insert(b.body.as_str());
    }
    let mut out = String::with_capacity(new.len() + disk.len() / 4);
    let mut placed: std::collections::HashSet<String> = std::collections::HashSet::new();
    // While inside a region of `new` whose body comes from disk: its id.
    let mut skipping: Option<String> = None;
    for line in new.split_inclusive('\n') {
        if let Some(id) = skipping.as_deref() {
            if marker(line, "end", open, close) == Some(id) {
                out.push_str(line);
                skipping = None;
            }
            continue;
        }
        out.push_str(line);
        if let Some(id) = marker(line, "begin", open, close)
            && let Some(body) = bodies.get(id)
            && !placed.contains(id)
        {
            out.push_str(body);
            placed.insert(id.to_string());
            skipping = Some(id.to_string());
        }
    }
    let mut issues = Vec::new();
    let mut appended: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for b in &old {
        if placed.contains(&b.id) || !appended.insert(b.id.as_str()) {
            continue;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&begin(&b.id, open, close));
        out.push('\n');
        out.push_str(&b.body);
        if !b.body.is_empty() && !b.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&end(&b.id, open, close));
        out.push('\n');
        issues.push(crate::issue(
            "orphanRegion",
            Severity::Warning,
            format!("the hand-written region for `{}` in {file} has no place in the new output (its element is gone or out of scope); it is kept at the end of the file", crate::clip(&b.id, 60)),
            vec![b.id.clone()],
        ));
    }
    (out, issues)
}

/// `text` with the body of every matched region removed and the markers kept, so a three-way
/// merge sees only what generation owns and the hand-written bodies come back through [`carry`].
/// Regions are matched as [`blocks`] matches them: a begin, lines with no marker, the end with
/// the same id.
pub fn strip(text: &str, open: &str, close: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let is_marker = |l: &str| marker(l, "begin", open, close).is_some() || marker(l, "end", open, close).is_some();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        out.push_str(line);
        i += 1;
        let Some(id) = marker(line, "begin", open, close) else { continue };
        // The first marker after the begin closes the region only when it is its end.
        let mut j = i;
        while lines.get(j).is_some_and(|l| !is_marker(l)) {
            j += 1;
        }
        if let Some(end_line) = lines.get(j)
            && marker(end_line, "end", open, close) == Some(id)
        {
            out.push_str(end_line);
            i = j + 1;
        }
    }
    out
}
