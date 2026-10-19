//! The pattern schema: a [`Pattern`] names its typed [`Param`]s, and [`Pattern::check`] turns a
//! caller's JSON arguments into checked [`Args`] with every parameter filled.
//!
//! Arguments are hostile (agents, the control channel, files), so the check refuses rather than
//! guesses: an unknown key, a value of the wrong kind, a count outside its range, a choice that is
//! not offered, a text or list beyond its cap are each an [`PatternError::Args`] that says which
//! parameter and why.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PatternError;

/// Longest text argument, in characters.
pub const MAX_TEXT: usize = 256;

/// Most entries in a label list.
pub const MAX_LABELS: usize = 64;

/// Most rows in a table argument.
pub const MAX_ROWS: usize = 256;

/// Most columns a table parameter declares.
pub const MAX_COLUMNS: usize = 32;

/// Longest tag prefix, in characters.
pub const MAX_PREFIX: usize = 16;

/// Longest key or value quoted back in an error message.
const QUOTE: usize = 40;

/// A pattern: a generator with typed parameters over one notation pack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    /// Stable id (`workflow.parallelSplit`).
    pub id: String,
    /// Name shown in the Patterns pane and the palette.
    pub name: String,
    /// One or two sentences for people and agents.
    pub description: String,
    /// The notation pack whose types it emits (`flowchart`).
    pub pack: String,
    /// Audiences it serves (`general`, `enterprise`), as packs list theirs.
    pub audience: Vec<String>,
    /// Its parameters, in form order.
    pub params: Vec<Param>,
}

/// One parameter of a pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// Argument key (`branches`).
    pub key: String,
    /// Label shown in the form.
    pub label: String,
    /// What values it takes; flattened, so the JSON reads `{"key", "label", "kind": "count", "min", "max", "default"}`.
    #[serde(flatten)]
    pub kind: ParamKind,
    /// The value used when the caller gives none.
    pub default: Value,
}

/// The kind of value a parameter takes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParamKind {
    /// A whole number from `min` to `max`.
    Count {
        /// Smallest value.
        min: u32,
        /// Largest value.
        max: u32,
    },
    /// One line of text, at most [`MAX_TEXT`] characters.
    Text,
    /// A list of texts (at most [`MAX_LABELS`]); missing entries are generated.
    Labels,
    /// One of `values`.
    Choice {
        /// The values offered, in menu order.
        values: Vec<String>,
    },
    /// A prefix for the tags of the elements a pattern numbers: letters, digits, `-` and `_`.
    TagPrefix,
    /// A table: rows of texts under `columns`. A row is an array (by position) or an object (by
    /// column name).
    Table {
        /// The column names.
        columns: Vec<String>,
    },
}

impl ParamKind {
    /// The kind's name as it appears in JSON.
    pub fn name(&self) -> &'static str {
        match self {
            ParamKind::Count { .. } => "count",
            ParamKind::Text => "text",
            ParamKind::Labels => "labels",
            ParamKind::Choice { .. } => "choice",
            ParamKind::TagPrefix => "tagPrefix",
            ParamKind::Table { .. } => "table",
        }
    }
}

impl Param {
    /// A count parameter.
    pub fn count(key: &str, label: &str, min: u32, max: u32, default: u32) -> Param {
        Param { key: key.into(), label: label.into(), kind: ParamKind::Count { min, max }, default: json!(default) }
    }
    /// A text parameter.
    pub fn text(key: &str, label: &str, default: &str) -> Param {
        Param { key: key.into(), label: label.into(), kind: ParamKind::Text, default: json!(default) }
    }
    /// A label list parameter.
    pub fn labels(key: &str, label: &str, default: &[&str]) -> Param {
        Param { key: key.into(), label: label.into(), kind: ParamKind::Labels, default: json!(default) }
    }
    /// A choice parameter; the default is the first value.
    pub fn choice(key: &str, label: &str, values: &[&str]) -> Param {
        let default = json!(values.first().copied().unwrap_or_default());
        Param { key: key.into(), label: label.into(), kind: ParamKind::Choice { values: values.iter().map(|v| v.to_string()).collect() }, default }
    }
    /// A tag prefix parameter, empty by default.
    pub fn tag_prefix(key: &str, label: &str) -> Param {
        Param { key: key.into(), label: label.into(), kind: ParamKind::TagPrefix, default: json!("") }
    }
    /// A table parameter, empty by default.
    pub fn table(key: &str, label: &str, columns: &[&str]) -> Param {
        Param {
            key: key.into(),
            label: label.into(),
            kind: ParamKind::Table { columns: columns.iter().map(|c| c.to_string()).collect() },
            default: json!([]),
        }
    }

    /// Check one value against the kind and return it normalised (a table as rows of arrays).
    pub fn check(&self, v: &Value) -> Result<Value, PatternError> {
        let bad = |why: String| PatternError::Args(format!("`{}`: {why}", quote(&self.key)));
        match &self.kind {
            ParamKind::Count { min, max } => {
                let n = whole(v).ok_or_else(|| bad("must be a whole number".into()))?;
                if n < i64::from(*min) || n > i64::from(*max) {
                    return Err(bad(format!("must be from {min} to {max}, not {n}")));
                }
                Ok(json!(n))
            }
            ParamKind::Text => Ok(json!(text(v).map_err(bad)?)),
            ParamKind::Labels => {
                let a = v.as_array().ok_or_else(|| bad("must be a list of texts".into()))?;
                if a.len() > MAX_LABELS {
                    return Err(bad(format!("holds at most {MAX_LABELS} labels")));
                }
                let out: Vec<String> = a.iter().map(text).collect::<Result<_, _>>().map_err(bad)?;
                Ok(json!(out))
            }
            ParamKind::Choice { values } => {
                let s = v.as_str().ok_or_else(|| bad(format!("must be one of {}", values.join(", "))))?;
                if values.iter().any(|x| x == s) { Ok(json!(s)) } else { Err(bad(format!("`{}` is not one of {}", quote(s), values.join(", ")))) }
            }
            ParamKind::TagPrefix => {
                let s = v.as_str().ok_or_else(|| bad("must be text".into()))?;
                if s.chars().count() > MAX_PREFIX {
                    return Err(bad(format!("is at most {MAX_PREFIX} characters")));
                }
                if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    return Err(bad("holds only letters, digits, `-` and `_`".into()));
                }
                Ok(json!(s))
            }
            ParamKind::Table { columns } => {
                let rows = v.as_array().ok_or_else(|| bad("must be a list of rows".into()))?;
                if rows.len() > MAX_ROWS {
                    return Err(bad(format!("holds at most {MAX_ROWS} rows")));
                }
                let cols = columns.len().min(MAX_COLUMNS);
                let mut out = Vec::with_capacity(rows.len());
                for (i, row) in rows.iter().enumerate() {
                    let cells: Vec<String> = match row {
                        Value::Array(a) => {
                            if a.len() > cols {
                                return Err(bad(format!("row {} has more than {cols} cells", i.saturating_add(1))));
                            }
                            let mut c: Vec<String> = a.iter().map(text).collect::<Result<_, _>>().map_err(bad)?;
                            c.resize(cols, String::new());
                            c
                        }
                        Value::Object(o) => {
                            if let Some(k) = o.keys().find(|k| !columns.iter().take(cols).any(|c| c == *k)) {
                                return Err(bad(format!("row {} names no column `{}`", i.saturating_add(1), quote(k))));
                            }
                            columns.iter().take(cols).map(|c| o.get(c).map_or(Ok(String::new()), text)).collect::<Result<_, _>>().map_err(bad)?
                        }
                        _ => return Err(bad(format!("row {} must be a list or an object", i.saturating_add(1)))),
                    };
                    out.push(cells);
                }
                Ok(json!(out))
            }
        }
    }
}

/// A whole number from a JSON number (`3` or `3.0`); `None` for anything else.
fn whole(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    let f = v.as_f64()?;
    // Out-of-range floats are refused rather than saturated, so the range check sees the truth.
    (f.is_finite() && f.fract() == 0.0 && f.abs() < 1.0e15).then_some(f as i64)
}

/// A text value of at most [`MAX_TEXT`] characters.
fn text(v: &Value) -> Result<String, String> {
    let s = v.as_str().ok_or_else(|| "must be text".to_string())?;
    if s.chars().count() > MAX_TEXT {
        return Err(format!("a text is at most {MAX_TEXT} characters"));
    }
    Ok(s.to_string())
}

/// `s` cut to a readable length for a message.
pub(crate) fn quote(s: &str) -> String {
    if s.chars().count() <= QUOTE { s.to_string() } else { format!("{}...", s.chars().take(QUOTE).collect::<String>()) }
}

/// Checked arguments: every parameter of the pattern, with its value normalised.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Args(pub BTreeMap<String, Value>);

impl Args {
    /// A count argument (0 when absent).
    pub fn count(&self, key: &str) -> usize {
        self.0.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0)
    }
    /// A text, choice or tag prefix argument (empty when absent).
    pub fn text(&self, key: &str) -> &str {
        self.0.get(key).and_then(Value::as_str).unwrap_or_default()
    }
    /// A label list argument (empty when absent).
    pub fn labels(&self, key: &str) -> Vec<String> {
        self.0.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    }
    /// Label `i` (from 0) of a label list, or `fallback` when the list has none there or it is blank.
    pub fn label_or(&self, key: &str, i: usize, fallback: impl FnOnce() -> String) -> String {
        let l = self.0.get(key).and_then(Value::as_array).and_then(|a| a.get(i)).and_then(Value::as_str).map(str::trim).unwrap_or_default();
        if l.is_empty() { fallback() } else { l.to_string() }
    }
    /// A table argument as rows of cells (empty when absent).
    pub fn table(&self, key: &str) -> Vec<Vec<String>> {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect()).collect())
            .unwrap_or_default()
    }
    /// The arguments as a JSON object.
    pub fn to_value(&self) -> Value {
        Value::Object(self.0.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

impl Pattern {
    /// The parameter with this key.
    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.key == key)
    }

    /// Every parameter at its default.
    pub fn defaults(&self) -> Args {
        Args(self.params.iter().map(|p| (p.key.clone(), p.default.clone())).collect())
    }

    /// Check `args` (a JSON object, or null for every default) against the parameters: unknown
    /// keys, values of the wrong kind and counts outside their range are refused with a message;
    /// a parameter not given takes its default.
    pub fn check(&self, args: &Value) -> Result<Args, PatternError> {
        let given = match args {
            Value::Null => serde_json::Map::new(),
            Value::Object(o) => o.clone(),
            _ => return Err(PatternError::Args("`args` must be an object".into())),
        };
        if let Some(k) = given.keys().find(|k| self.param(k).is_none()) {
            let known: Vec<&str> = self.params.iter().map(|p| p.key.as_str()).collect();
            return Err(PatternError::Args(format!("`{}` is not a parameter of {} (it takes {})", quote(k), self.id, known.join(", "))));
        }
        let mut out = BTreeMap::new();
        for p in &self.params {
            let v = given.get(&p.key).filter(|v| !v.is_null()).unwrap_or(&p.default);
            out.insert(p.key.clone(), p.check(v)?);
        }
        Ok(Args(out))
    }

    /// The pattern as `pattern.list` reports it: its fields with the parameter schemas.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Pattern {
        Pattern {
            id: "t.p".into(),
            name: "P".into(),
            description: String::new(),
            pack: "t".into(),
            audience: vec![],
            params: vec![
                Param::count("n", "N", 2, 8, 3),
                Param::text("name", "Name", "x"),
                Param::labels("labels", "Labels", &[]),
                Param::choice("mode", "Mode", &["a", "b"]),
                Param::tag_prefix("prefix", "Prefix"),
                Param::table("rows", "Rows", &["Name", "Owner"]),
            ],
        }
    }

    #[test]
    fn defaults_fill_and_values_normalise() {
        let a = p().check(&Value::Null).unwrap();
        assert_eq!((a.count("n"), a.text("name"), a.text("mode")), (3, "x", "a"));
        let a = p().check(&json!({"n": 5.0, "rows": [["one"], {"Owner": "me"}]})).unwrap();
        assert_eq!(a.count("n"), 5);
        assert_eq!(a.table("rows"), vec![vec!["one".to_string(), String::new()], vec![String::new(), "me".to_string()]]);
    }

    #[test]
    fn bad_args_are_refused_with_a_message() {
        for (bad, says) in [
            (json!({"nope": 1}), "not a parameter"),
            (json!({"n": 9}), "from 2 to 8"),
            (json!({"n": 1}), "from 2 to 8"),
            (json!({"n": 2.5}), "whole number"),
            (json!({"n": "3"}), "whole number"),
            (json!({"name": 3}), "text"),
            (json!({"name": "x".repeat(MAX_TEXT + 1)}), "at most"),
            (json!({"labels": "a"}), "list"),
            (json!({"labels": vec!["a"; MAX_LABELS + 1]}), "at most"),
            (json!({"mode": "c"}), "not one of"),
            (json!({"prefix": "a b"}), "letters"),
            (json!({"rows": [["a", "b", "c"]]}), "more than"),
            (json!({"rows": [{"Nope": "a"}]}), "no column"),
            (json!({"rows": [3]}), "list or an object"),
            (json!([1]), "object"),
        ] {
            let e = p().check(&bad).unwrap_err().to_string();
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn schema_serialises_flat() {
        let v = p().to_json();
        assert_eq!(v["params"][0], json!({"key": "n", "label": "N", "kind": "count", "min": 2, "max": 8, "default": 3}));
        assert_eq!(v["params"][3]["values"], json!(["a", "b"]));
        let back: Pattern = serde_json::from_value(v).unwrap();
        assert_eq!(back, p());
    }
}
