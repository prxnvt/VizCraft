//! Shape data: typed key and value rows on a shape, and the page's data key definitions (a data
//! set) that several shapes share.
//!
//! A shape that draws a model element already has typed properties, defined by its notation pack;
//! shape data is for free-drawing shapes and for keys a pack does not define. Each row has a type
//! (`string`, `number`, `bool`, `date`, `enum`, `url`) and a value kept as canonical text, which
//! is either empty (not set) or valid for the type: [`normalize`] turns caller input into that
//! text or says why it cannot, and [`ShapeData::sanitize`] keeps the rule for loaded files by
//! turning a row whose value does not fit its type into a `string` row, so no text is lost.
//!
//! Page definitions live in the page's `ext` map under [`PAGE_KEYS_EXT`] as a JSON list, read
//! with [`page_keys`] and written with [`set_page_keys`]; a shape opts in to them with
//! [`ShapeData::page_keys`]. [`search`] finds shapes by key and value text for Find.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::model::{Document, Page, Shape, truncate};
use crate::serde_util::{lenient, lenient_vec, named_enum};

/// Most rows one shape keeps.
pub const MAX_ROWS: usize = 128;
/// Longest key, in bytes.
pub const MAX_KEY: usize = 64;
/// Longest label, in bytes.
pub const MAX_LABEL: usize = 128;
/// Longest unit, in bytes.
pub const MAX_UNIT: usize = 32;
/// Most enum values one key allows.
pub const MAX_VALUES: usize = 64;
/// Longest value (and enum value), in bytes.
pub const MAX_VALUE: usize = 4096;
/// Most data keys one page defines.
pub const MAX_PAGE_KEYS: usize = 128;
/// Most hits [`search`] returns.
pub const MAX_HITS: usize = 10_000;
/// The page `ext` key holding the page's data key definitions.
pub const PAGE_KEYS_EXT: &str = "vizcraft.dataKeys";

/// The type of a data value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DataType {
    /// Free text.
    #[default]
    String,
    /// A finite number, shown with the row's unit.
    Number,
    /// `true` or `false`.
    Bool,
    /// A calendar date, `YYYY-MM-DD`.
    Date,
    /// One of the row's `values`.
    Enum,
    /// A web or mail address (`http`, `https`, `mailto`, `ftp` or `file`).
    Url,
}

named_enum!(DataType, DataType::String, {
    DataType::String => "string",
    DataType::Number => "number",
    DataType::Bool => "bool",
    DataType::Date => "date",
    DataType::Enum => "enum",
    DataType::Url => "url",
});

impl DataType {
    /// Every type, in menu order.
    pub const ALL: [DataType; 6] = [DataType::String, DataType::Number, DataType::Bool, DataType::Date, DataType::Enum, DataType::Url];
}

/// A data key definition: what a row with this key holds. Pages keep a list of them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataKey {
    /// The key (`cost`): 1 to [`MAX_KEY`] letters, digits, `_`, `-`, `.` or spaces.
    #[serde(deserialize_with = "lenient")]
    pub key: String,
    /// Label shown instead of the key; empty shows the key.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Value type.
    #[serde(rename = "type", deserialize_with = "lenient")]
    pub kind: DataType,
    /// Unit shown beside numbers (`kg`, `USD`).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Allowed values of an enum.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// The value must be set; an empty one is reported as a warning.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub required: bool,
    /// Value a new row starts with (canonical text, valid for the type).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// One row of shape data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataRow {
    /// The key, unique within the shape.
    #[serde(deserialize_with = "lenient")]
    pub key: String,
    /// Label shown instead of the key; empty shows the key.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Value type.
    #[serde(rename = "type", deserialize_with = "lenient")]
    pub kind: DataType,
    /// Unit shown beside numbers.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Allowed values of an enum.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// The value must be set.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub required: bool,
    /// Value the row started with.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The value as canonical text; empty is not set.
    #[serde(deserialize_with = "value_text", skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// Left out of the Shape Data pane unless hidden rows are shown.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub hidden: bool,
}

/// A shape's data: its rows in order, and whether it takes the page's key definitions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShapeData {
    /// Rows in display order, unique keys, at most [`MAX_ROWS`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<DataRow>,
    /// Defining a key on the page adds a row for it to this shape.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub page_keys: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A value read from a file: text as it is, numbers and booleans as their text, anything else
/// empty.
fn value_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(fmt_number).unwrap_or_default(),
        _ => String::new(),
    })
}

/// Whether `key` is a usable data key: 1 to [`MAX_KEY`] bytes of letters, digits, `_`, `-`, `.`
/// or inner spaces.
pub fn is_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY && key.trim() == key && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

/// A number as canonical text: the shortest form that reads back as the same number.
pub fn fmt_number(n: f64) -> String {
    if n == 0.0 { "0".into() } else { format!("{n}") }
}

/// Whether `y-m-d` is a real calendar date.
fn valid_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| text.get(r).filter(|s| s.bytes().all(|c| c.is_ascii_digit())).and_then(|s| s.parse::<u32>().ok());
    let (Some(y), Some(m), Some(d)) = (digits(0..4), digits(5..7), digits(8..10)) else { return false };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// Whether `text` is an address of an allowed scheme with no spaces or control characters.
fn valid_url(text: &str) -> bool {
    if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    ["http://", "https://", "ftp://", "file://", "mailto:"].iter().any(|p| lower.starts_with(p) && lower.len() > p.len())
}

/// Turn caller text into the canonical value of a row of `kind` (with enum `values` and `unit`),
/// or say why it does not fit. Empty (after trimming) is always allowed: the value is not set.
pub fn normalize_text(kind: DataType, values: &[String], unit: &str, text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    if text.len() > MAX_VALUE {
        return Err(format!("a value is at most {MAX_VALUE} bytes"));
    }
    match kind {
        DataType::String => Ok(text.to_string()),
        DataType::Number => {
            // A trailing unit the row already shows (`12 kg`) is accepted and dropped.
            let bare = match (unit.is_empty(), t.len().checked_sub(unit.len())) {
                (false, Some(cut)) if t.is_char_boundary(cut) && t.get(cut..).is_some_and(|u| u.eq_ignore_ascii_case(unit)) => {
                    t.get(..cut).unwrap_or(t).trim_end()
                }
                _ => t,
            };
            bare.parse::<f64>().ok().filter(|n| n.is_finite()).map(fmt_number).ok_or_else(|| format!("`{}` is not a number", short(t)))
        }
        DataType::Bool => match t.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Ok("true".into()),
            "false" | "no" | "off" | "0" => Ok("false".into()),
            _ => Err(format!("`{}` is not true or false", short(t))),
        },
        DataType::Date => {
            if valid_date(t) {
                Ok(t.to_string())
            } else {
                Err(format!("`{}` is not a date (YYYY-MM-DD)", short(t)))
            }
        }
        DataType::Enum => values
            .iter()
            .find(|v| v.as_str() == t)
            .or_else(|| values.iter().find(|v| v.eq_ignore_ascii_case(t)))
            .cloned()
            .ok_or_else(|| format!("`{}` is not one of {}", short(t), values.iter().map(|v| short(v)).collect::<Vec<_>>().join(", "))),
        DataType::Url => {
            if valid_url(t) {
                Ok(t.to_string())
            } else {
                Err(format!("`{}` is not an http, https, ftp, file or mailto address", short(t)))
            }
        }
    }
}

/// [`normalize_text`] for a JSON value: a string, a number, a boolean or null (not set).
pub fn normalize(kind: DataType, values: &[String], unit: &str, v: &Value) -> Result<String, String> {
    match v {
        Value::Null => Ok(String::new()),
        Value::String(s) => normalize_text(kind, values, unit, s),
        Value::Bool(b) if matches!(kind, DataType::Bool | DataType::String) => Ok(b.to_string()),
        Value::Number(n) if matches!(kind, DataType::Number | DataType::String | DataType::Enum) => {
            let f = n.as_f64().filter(|f| f.is_finite()).ok_or_else(|| "the number is out of range".to_string())?;
            normalize_text(kind, values, unit, &fmt_number(f))
        }
        _ => Err(format!("a {} value cannot be {}", kind.name(), json_kind(v))),
    }
}

/// The JSON type of a value, for messages.
fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// The type a new row takes from its first value when the caller names none.
pub fn infer_type(v: &Value) -> DataType {
    match v {
        Value::Bool(_) => DataType::Bool,
        Value::Number(_) => DataType::Number,
        _ => DataType::String,
    }
}

/// A canonical value as JSON: numbers and booleans typed, everything else text, empty as null.
pub fn typed_value(kind: DataType, text: &str) -> Value {
    if text.is_empty() {
        return Value::Null;
    }
    match kind {
        DataType::Number => text.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or_else(|| text.into()),
        DataType::Bool => Value::Bool(text == "true"),
        _ => text.into(),
    }
}

/// At most 40 chars of `s`, for messages.
fn short(s: &str) -> String {
    let mut out: String = s.chars().take(40).collect();
    if s.chars().nth(40).is_some() {
        out.push_str("...");
    }
    out
}

impl DataKey {
    /// Clamp text lengths, drop junk enum values and make the default fit the type. Returns
    /// false when the key itself is unusable and the definition should be dropped.
    pub fn sanitize(&mut self) -> bool {
        sanitize_meta(&mut self.label, &mut self.kind, &mut self.unit, &mut self.values, &mut self.default);
        is_key(&self.key)
    }
    /// A new row for this key, holding the default.
    pub fn row(&self) -> DataRow {
        DataRow {
            key: self.key.clone(),
            label: self.label.clone(),
            kind: self.kind,
            unit: self.unit.clone(),
            values: self.values.clone(),
            required: self.required,
            default: self.default.clone(),
            value: self.default.clone().unwrap_or_default(),
            hidden: false,
        }
    }
    /// The label, or the key when there is none.
    pub fn title(&self) -> &str {
        if self.label.is_empty() { &self.key } else { &self.label }
    }
}

/// The shared part of key and row sanitising.
fn sanitize_meta(label: &mut String, kind: &mut DataType, unit: &mut String, values: &mut Vec<String>, default: &mut Option<String>) {
    truncate(label, MAX_LABEL);
    truncate(unit, MAX_UNIT);
    values.truncate(MAX_VALUES);
    values.iter_mut().for_each(|v| truncate(v, MAX_VALUE));
    let mut seen = std::collections::HashSet::new();
    values.retain(|v| !v.trim().is_empty() && seen.insert(v.clone()));
    if *kind == DataType::Enum && values.is_empty() {
        *kind = DataType::String;
    }
    if let Some(d) = default {
        truncate(d, MAX_VALUE);
        match normalize_text(*kind, values, unit, d) {
            Ok(t) if !t.is_empty() => *d = t,
            _ => *default = None,
        }
    }
}

impl DataRow {
    /// Clamp text lengths and keep the value valid for the type: a value that does not fit makes
    /// the row a `string` row, so loaded text is never thrown away.
    pub fn sanitize(&mut self) {
        sanitize_meta(&mut self.label, &mut self.kind, &mut self.unit, &mut self.values, &mut self.default);
        truncate(&mut self.value, MAX_VALUE);
        match normalize_text(self.kind, &self.values, &self.unit, &self.value) {
            Ok(t) => self.value = t,
            Err(_) => self.kind = DataType::String,
        }
    }
    /// The label, or the key when there is none.
    pub fn title(&self) -> &str {
        if self.label.is_empty() { &self.key } else { &self.label }
    }
    /// Whether the row is required and has no value.
    pub fn missing(&self) -> bool {
        self.required && self.value.is_empty()
    }
    /// Take the definition's label, type, unit, values, required flag and default, keeping the
    /// value; refused (no change) when the value does not fit the new type.
    pub fn apply(&mut self, def: &DataKey) -> Result<(), String> {
        let value = normalize_text(def.kind, &def.values, &def.unit, &self.value)?;
        self.label = def.label.clone();
        self.kind = def.kind;
        self.unit = def.unit.clone();
        self.values = def.values.clone();
        self.required = def.required;
        self.default = def.default.clone();
        self.value = value;
        Ok(())
    }
}

impl ShapeData {
    /// Drop rows with unusable or repeated keys, keep at most [`MAX_ROWS`], sanitise each.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.rows.retain(|r| is_key(&r.key) && seen.insert(r.key.clone()));
        self.rows.truncate(MAX_ROWS);
        self.rows.iter_mut().for_each(DataRow::sanitize);
    }
    /// The row with this key.
    pub fn row(&self, key: &str) -> Option<&DataRow> {
        self.rows.iter().find(|r| r.key == key)
    }
    /// The row with this key, mutably.
    pub fn row_mut(&mut self, key: &str) -> Option<&mut DataRow> {
        self.rows.iter_mut().find(|r| r.key == key)
    }
    /// No rows and not opted in to page keys.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && !self.page_keys
    }
}

/// The page's data key definitions, sanitised; a missing or unreadable entry is none.
pub fn page_keys(page: &Page) -> Vec<DataKey> {
    #[derive(Deserialize)]
    struct Keys(#[serde(deserialize_with = "lenient_vec")] Vec<DataKey>);
    let Some(text) = page.ext.get(PAGE_KEYS_EXT) else { return Vec::new() };
    let mut keys = serde_json::from_str::<Keys>(text).map(|k| k.0).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    keys.retain_mut(|k| k.sanitize() && seen.insert(k.key.clone()));
    keys.truncate(MAX_PAGE_KEYS);
    keys
}

/// Store the page's data key definitions; an empty list removes the entry.
pub fn set_page_keys(page: &mut Page, keys: &[DataKey]) {
    if keys.is_empty() {
        page.ext.remove(PAGE_KEYS_EXT);
    } else if let Ok(text) = serde_json::to_string(keys) {
        page.ext.insert(PAGE_KEYS_EXT.into(), text);
    }
}

/// Add every page key `shape` lacks, as rows holding the key's default; returns whether any
/// row was added.
pub fn add_page_rows(shape: &mut Shape, keys: &[DataKey]) -> bool {
    let mut added = false;
    for k in keys {
        if shape.data.rows.len() >= MAX_ROWS {
            break;
        }
        if shape.data.row(&k.key).is_none() {
            shape.data.rows.push(k.row());
            added = true;
        }
    }
    added
}

/// One match of [`search`]: the shape, the key, and the matching value text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DataHit {
    /// The shape id.
    pub shape: String,
    /// The data key (or model property key) that matched.
    pub key: String,
    /// The value text.
    pub value: String,
    /// The value belongs to the shape's model element, not to the shape.
    pub model: bool,
}

/// Shapes on `page` whose data holds `text` (ignoring case) under `key` (a key or label, ignoring
/// case; `None` is any key). Shape rows and the typed properties of the element a shape draws are
/// both searched; empty `text` matches every set value. At most [`MAX_HITS`] hits, in page order.
pub fn search(doc: &Document, page: &Page, key: Option<&str>, text: &str) -> Vec<DataHit> {
    let needle = text.to_lowercase();
    let key_ok = |k: &str, label: &str| key.is_none_or(|q| q.eq_ignore_ascii_case(k) || (!label.is_empty() && q.eq_ignore_ascii_case(label)));
    let value_ok = |v: &str| !v.is_empty() && (needle.is_empty() || v.to_lowercase().contains(&needle));
    let mut hits = Vec::new();
    for sh in &page.shapes {
        for r in &sh.data.rows {
            if key_ok(&r.key, &r.label) && value_ok(&r.value) {
                hits.push(DataHit { shape: sh.id.clone(), key: r.key.clone(), value: r.value.clone(), model: false });
            }
        }
        if let Some(e) = sh.element.as_deref().and_then(|e| doc.model.element(e)) {
            for (k, v) in &e.properties {
                let v = match v {
                    vizcraft_model::PropValue::Text(t) => t.clone(),
                    vizcraft_model::PropValue::Number(n) => fmt_number(*n),
                    vizcraft_model::PropValue::Bool(b) => b.to_string(),
                };
                if key_ok(k, "") && value_ok(&v) {
                    hits.push(DataHit { shape: sh.id.clone(), key: k.clone(), value: v, model: true });
                }
            }
        }
        if hits.len() >= MAX_HITS {
            hits.truncate(MAX_HITS);
            break;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(key: &str, kind: DataType, value: &str) -> DataRow {
        DataRow { key: key.into(), kind, value: value.into(), ..DataRow::default() }
    }

    #[test]
    fn values_normalize_by_type() {
        let none: Vec<String> = Vec::new();
        assert_eq!(normalize_text(DataType::Number, &none, "kg", " 12.50 kg").unwrap(), "12.5");
        assert_eq!(normalize_text(DataType::Number, &none, "", "-0").unwrap(), "0");
        assert!(normalize_text(DataType::Number, &none, "", "NaN").is_err());
        assert!(normalize_text(DataType::Number, &none, "", "inf").is_err());
        assert!(normalize_text(DataType::Number, &none, "", "12 kg").is_err());
        assert_eq!(normalize_text(DataType::Bool, &none, "", "Yes").unwrap(), "true");
        assert!(normalize_text(DataType::Bool, &none, "", "maybe").is_err());
        assert_eq!(normalize_text(DataType::Date, &none, "", "2024-02-29").unwrap(), "2024-02-29");
        for bad in ["2023-02-29", "2024-13-01", "2024-1-01", "24-01-01", "2024-01-00", "２０２４-01-01"] {
            assert!(normalize_text(DataType::Date, &none, "", bad).is_err(), "{bad}");
        }
        let values = vec!["Low".to_string(), "High".to_string()];
        assert_eq!(normalize_text(DataType::Enum, &values, "", "high").unwrap(), "High");
        assert!(normalize_text(DataType::Enum, &values, "", "Medium").is_err());
        assert!(normalize_text(DataType::Url, &none, "", "https://example.com/a?b=1").is_ok());
        for bad in ["javascript:alert(1)", "https://", "http://a b", "example.com"] {
            assert!(normalize_text(DataType::Url, &none, "", bad).is_err(), "{bad}");
        }
        assert_eq!(normalize_text(DataType::Date, &none, "", "   ").unwrap(), "", "blank is not set for every type");
        assert!(normalize_text(DataType::String, &none, "", &"x".repeat(MAX_VALUE + 1)).is_err());
        // Units with multi-byte characters never split a char.
        assert!(normalize_text(DataType::Number, &none, "µm", "é").is_err());
        assert_eq!(normalize_text(DataType::Number, &none, "µm", "3µm").unwrap(), "3");
    }

    #[test]
    fn json_values_normalize_and_type_back() {
        let none: Vec<String> = Vec::new();
        assert_eq!(normalize(DataType::Number, &none, "", &json!(3)).unwrap(), "3");
        assert_eq!(normalize(DataType::Bool, &none, "", &json!(false)).unwrap(), "false");
        assert_eq!(normalize(DataType::String, &none, "", &json!(true)).unwrap(), "true");
        assert_eq!(normalize(DataType::Date, &none, "", &json!(null)).unwrap(), "");
        assert!(normalize(DataType::Number, &none, "", &json!([1])).is_err());
        assert!(normalize(DataType::Bool, &none, "", &json!(1)).is_err());
        assert_eq!(typed_value(DataType::Number, "2.5"), json!(2.5));
        assert_eq!(typed_value(DataType::Bool, "true"), json!(true));
        assert_eq!(typed_value(DataType::Date, ""), Value::Null);
        assert_eq!(infer_type(&json!(1)), DataType::Number);
    }

    #[test]
    fn sanitize_keeps_rows_valid_and_bounded() {
        let mut d = ShapeData {
            rows: vec![
                row("cost", DataType::Number, "abc"),
                row("cost", DataType::Number, "1"),
                row("", DataType::String, "x"),
                row(" bad", DataType::String, "x"),
                DataRow { key: "level".into(), kind: DataType::Enum, value: "x".into(), ..DataRow::default() },
                DataRow { key: "due".into(), kind: DataType::Date, default: Some("never".into()), ..DataRow::default() },
            ],
            page_keys: false,
        };
        for i in 0..300 {
            d.rows.push(row(&format!("k{i}"), DataType::String, "v"));
        }
        d.sanitize();
        assert_eq!(d.rows.len(), MAX_ROWS);
        let cost = d.row("cost").unwrap();
        assert_eq!((cost.kind, cost.value.as_str()), (DataType::String, "abc"), "a value that does not fit keeps its text as a string");
        assert_eq!(d.row("level").unwrap().kind, DataType::String, "an enum with no values is a string");
        assert_eq!(d.row("due").unwrap().default, None);
        assert!(d.row(" bad").is_none());
    }

    #[test]
    fn page_keys_round_trip_through_ext() {
        let mut page = Page::new("p1", "Page-1");
        assert!(page_keys(&page).is_empty());
        let keys = vec![DataKey { key: "cost".into(), kind: DataType::Number, unit: "USD".into(), ..DataKey::default() }];
        set_page_keys(&mut page, &keys);
        assert_eq!(page_keys(&page), keys);
        page.ext.insert(PAGE_KEYS_EXT.into(), "{not json".into());
        assert!(page_keys(&page).is_empty());
        page.ext.insert(PAGE_KEYS_EXT.into(), r#"[{"key": "a"}, {"key": "a"}, 7, {"key": ""}, {"key": "b", "type": "wat"}]"#.into());
        let k = page_keys(&page);
        assert_eq!(k.iter().map(|k| k.key.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(k[1].kind, DataType::String);
        set_page_keys(&mut page, &[]);
        assert!(!page.ext.contains_key(PAGE_KEYS_EXT));
    }

    #[test]
    fn search_finds_shape_rows_by_key_and_text() {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        let mut a = Shape::new("a", crate::ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        a.data.rows.push(DataRow { key: "owner".into(), label: "Owner".into(), value: "Ada Lovelace".into(), ..DataRow::default() });
        a.data.rows.push(row("cost", DataType::Number, "12"));
        page.shapes.push(a);
        let page = doc.page().unwrap();
        assert_eq!(search(&doc, page, Some("OWNER"), "ada").len(), 1);
        assert_eq!(search(&doc, page, None, "12")[0].key, "cost");
        assert!(search(&doc, page, Some("cost"), "ada").is_empty());
        assert_eq!(search(&doc, page, None, "").len(), 2);
    }
}
