//! Pack test cases: a document, an optional conformance profile and the issues expected.
//!
//! A pack may ship cases under `tests/<pack id>/<case>.json` next to its file (the built-in
//! ones are in `crates/notation/packs/tests/`). Each holds
//! `{"document": <native file JSON>, "profile"?: <conformance profile>, "expect": [...]}`, where
//! each expected entry is an issue key (`Issue::key`) or `{"rule", "items"?}` (items in any
//! order; left out, any items). A case passes when the live and batch rules over the whole
//! document report exactly the expected issues, each matched once. This is how a rule is added
//! and tested one at a time: write the case, then the rule.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use vizcraft_diagram::{Conformance, Document};
use vizcraft_model::{Issue, Model};

use crate::Catalog;
use crate::rules::{self, Scope};

/// Largest case file read, in bytes.
pub const MAX_CASE: usize = 16 * 1024 * 1024;

/// Most cases run from one folder.
pub const MAX_CASES: usize = 1_000;

/// One case file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Case {
    /// The document: a native file (`{"format": "vizcraft", ...}`) or a bare document.
    pub document: Value,
    /// The profile to run with instead of the document's own (it counts as set by the user).
    pub profile: Option<Conformance>,
    /// The issues expected.
    pub expect: Vec<Expect>,
}

/// One expected issue.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Expect {
    /// An exact issue key.
    Key(String),
    /// A rule id and, optionally, the items in any order.
    Match {
        /// Rule id.
        rule: String,
        /// Items; `None` matches any.
        #[serde(default)]
        items: Option<Vec<String>>,
    },
}

impl Expect {
    fn matches(&self, i: &Issue) -> bool {
        match self {
            Expect::Key(k) => i.key() == *k,
            Expect::Match { rule, items } => {
                rule == &i.rule
                    && items.as_ref().is_none_or(|want| {
                        let (mut a, mut b) = (want.clone(), i.items.clone());
                        a.sort();
                        b.sort();
                        a == b
                    })
            }
        }
    }

    fn describe(&self) -> String {
        match self {
            Expect::Key(k) => format!("issue {k}"),
            Expect::Match { rule, items: Some(items) } => format!("{rule} on {}", items.join(", ")),
            Expect::Match { rule, items: None } => rule.clone(),
        }
    }
}

/// What running one case found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// The case's file name (or the name it was run under).
    pub name: String,
    /// Every issue the rules reported.
    pub issues: Vec<Issue>,
    /// Why the case failed; empty when it passed.
    pub problems: Vec<String>,
}

impl Outcome {
    /// Whether the rules reported exactly what was expected.
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// A document from native file JSON or a bare document object. The model section is read when
/// the wrapper has one; view references are then repaired against it.
pub fn document(v: &Value) -> Result<Document, String> {
    let Value::Object(obj) = v else { return Err("the document is not a JSON object".into()) };
    let (doc_value, model_value) = match obj.get("format") {
        None => (v.clone(), None),
        Some(tag) if tag.as_str() == Some("vizcraft") => match obj.get("document") {
            Some(d @ Value::Object(_)) => (d.clone(), obj.get("model").cloned()),
            _ => return Err("the native file has no document object".into()),
        },
        Some(_) => return Err("the document is not a VizCraft native file".into()),
    };
    let mut doc: Document = serde_json::from_value(doc_value).map_err(|e| format!("the document does not read: {e}"))?;
    doc.model = match model_value {
        Some(m @ Value::Object(_)) => serde_json::from_value::<Model>(m).map_err(|e| format!("the model does not read: {e}"))?,
        _ => Model::new(),
    };
    doc.sanitize();
    Ok(doc)
}

/// Every issue for `doc`: the live rules and the batch rules over the whole project, sorted.
pub fn all_issues(doc: &Document, cat: &Catalog) -> Vec<Issue> {
    let mut issues = rules::live(doc, cat);
    issues.extend(rules::batch(doc, cat, &Scope::All));
    rules::sort(&mut issues);
    issues
}

/// Run `case` under `cat`.
pub fn run(name: &str, case: &Case, cat: &Catalog) -> Outcome {
    let mut out = Outcome { name: name.to_string(), ..Outcome::default() };
    let mut doc = match document(&case.document) {
        Ok(d) => d,
        Err(e) => {
            out.problems.push(e);
            return out;
        }
    };
    if let Some(p) = &case.profile {
        doc.conformance = Conformance { user_set: true, ..p.clone() };
    }
    out.issues = all_issues(&doc, cat);
    let mut used = vec![false; out.issues.len()];
    for e in &case.expect {
        let hit = out.issues.iter().enumerate().position(|(i, is)| !used.get(i).copied().unwrap_or(true) && e.matches(is));
        match hit.and_then(|i| used.get_mut(i)) {
            Some(slot) => *slot = true,
            None => out.problems.push(format!("expected {} but it was not reported", e.describe())),
        }
    }
    for (is, u) in out.issues.iter().zip(&used) {
        if !u {
            out.problems.push(format!("unexpected {} {}: {}", is.severity.name(), is.key(), is.message));
        }
    }
    out
}

/// Run the case in `text` (a case file's contents).
pub fn run_text(name: &str, text: &str, cat: &Catalog) -> Outcome {
    if text.len() > MAX_CASE {
        return Outcome { name: name.into(), problems: vec![format!("the case is larger than {MAX_CASE} bytes")], ..Outcome::default() };
    }
    match serde_json::from_str::<Case>(text) {
        Ok(case) => run(name, &case, cat),
        Err(e) => Outcome { name: name.into(), problems: vec![format!("not a test case: {e}")], ..Outcome::default() },
    }
}

/// Whether `text` looks like a test case (an object with `document` and `expect`) rather than
/// a document.
pub fn is_case(v: &Value) -> bool {
    v.get("document").is_some() && v.get("expect").is_some()
}

/// The case folder of pack `pack` in a folder of packs: `<dir>/tests/<pack>`.
pub fn cases_dir(packs_dir: &Path, pack: &str) -> PathBuf {
    packs_dir.join("tests").join(pack)
}

/// Run every `*.json` case directly in `dir`, in file name order, at most [`MAX_CASES`]. A
/// missing folder has no cases.
pub fn run_dir(dir: &Path, cat: &Catalog) -> Vec<Outcome> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("json")))
        .collect();
    files.sort();
    files
        .into_iter()
        .take(MAX_CASES)
        .map(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match read_capped(&path) {
                Ok(text) => run_text(&name, &text, cat),
                Err(e) => Outcome { name, problems: vec![e], ..Outcome::default() },
            }
        })
        .collect()
}

/// A text file of at most [`MAX_CASE`] bytes.
pub fn read_capped(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_CASE).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() > MAX_CASE {
        return Err(format!("{} is larger than {MAX_CASE} bytes", path.display()));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8 text", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in cases, `crates/notation/packs/tests/<pack>/*.json`.
    fn builtin_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packs")
    }

    #[test]
    fn every_builtin_case_passes() {
        let cat = Catalog::builtin();
        let mut ran = 0;
        for pack in cat.packs() {
            for o in run_dir(&cases_dir(&builtin_dir(), &pack.id), &cat) {
                assert!(o.passed(), "{}/{}: {:#?}", pack.id, o.name, o.problems);
                ran += 1;
            }
        }
        assert!(ran >= 5, "only {ran} cases ran");
    }

    #[test]
    fn a_wrong_expectation_fails_with_reasons() {
        let cat = Catalog::builtin();
        let text = serde_json::json!({
            "document": {"pages": [{"id": "p1", "name": "P", "width": 800, "height": 600, "shapes": [], "connectors": [
                {"id": "c1", "start": {"x": 10, "y": 10}, "end": {"x": 100, "y": 10}}
            ]}]},
            "profile": {"enabled": true, "rulesets": ["drawing.tidy"]},
            "expect": [{"rule": "drawing.overlap"}]
        })
        .to_string();
        let o = run_text("t.json", &text, &cat);
        assert!(!o.passed());
        assert!(o.problems.iter().any(|p| p.contains("expected drawing.overlap")), "{:?}", o.problems);
        assert!(o.problems.iter().any(|p| p.contains("unexpected warning drawing.ungluedEnd")), "{:?}", o.problems);
        for junk in ["", "[]", "{\"document\": 5, \"expect\": []}", "{\"document\": {\"format\": \"other\"}, \"expect\": []}"] {
            assert!(!run_text("j.json", junk, &cat).passed(), "{junk}");
        }
        assert!(run_dir(Path::new("/nonexistent/vizcraft/cases"), &cat).is_empty());
    }
}
