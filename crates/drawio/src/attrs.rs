//! An ordered attribute map that keeps document order, so a round trip writes attributes back in
//! the order it found them.

use serde::{Deserialize, Serialize};

/// Ordered key/value pairs. Lookups are linear because JGraph's draw.io elements carry a handful of
/// attributes; keys are unique once read through [`Attributes::push`], which replaces an earlier
/// value with the same key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes(Vec<(String, String)>);

impl Attributes {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// The value for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Whether `key` is present.
    pub fn contains(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    /// Sets `key`, keeping its position when it already exists and appending otherwise.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.0.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.0.push((key, value));
        }
    }

    /// Same as [`Attributes::set`]; named for the reader, where duplicates in hostile XML collapse
    /// to the last value instead of producing two entries.
    pub fn push(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.set(key, value);
    }

    /// Removes `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let idx = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(idx).1)
    }

    /// Iterates pairs in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of pairs.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no pairs.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value for `key` parsed as a finite number.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(parse_f64)
    }

    /// The value for `key` read as a draw.io flag (`1`/`true` or `0`/`false`).
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(parse_bool)
    }
}

impl FromIterator<(String, String)> for Attributes {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        let mut a = Attributes::new();
        for (k, v) in iter {
            a.push(k, v);
        }
        a
    }
}

/// Parses a finite number, tolerating surrounding whitespace. Infinity and NaN are rejected
/// because they cannot be written back as draw.io numbers.
pub fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Parses a draw.io flag: `1` or `true` is true, `0` or `false` is false, anything else is unknown.
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// Formats a number the way draw.io files write them: integers without a fraction, everything
/// else in Rust's shortest round-tripping form.
pub fn format_f64(v: f64) -> String {
    if v == 0.0 {
        // Avoid writing "-0".
        "0".to_string()
    } else {
        format!("{v}")
    }
}
