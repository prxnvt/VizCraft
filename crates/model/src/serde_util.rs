//! Tolerant deserializers. Files and agent requests are hostile, so a wrong value in one field
//! falls back to that field's default instead of failing the whole model.

use serde::{Deserialize, Deserializer};

/// Either the wanted value or anything else (which is ignored).
#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient<T> {
    Value(T),
    Other(serde::de::IgnoredAny),
}

/// Any deserializable value, falling back to `T::default()` when the input has the wrong shape.
pub fn lenient<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(d: D) -> Result<T, D::Error> {
    Ok(match Lenient::<T>::deserialize(d)? {
        Lenient::Value(v) => v,
        Lenient::Other(_) => T::default(),
    })
}

/// A list that keeps the elements that parse and drops the rest; a non-list reads as empty.
pub fn lenient_vec<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match Lenient::<Vec<Lenient<T>>>::deserialize(d)? {
        Lenient::Value(items) => items
            .into_iter()
            .filter_map(|i| match i {
                Lenient::Value(v) => Some(v),
                Lenient::Other(_) => None,
            })
            .collect(),
        Lenient::Other(_) => Vec::new(),
    })
}

/// A string-keyed map that keeps the entries whose values parse and drops the rest; a non-map
/// reads as empty.
pub fn lenient_map<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<std::collections::BTreeMap<String, T>, D::Error> {
    Ok(match Lenient::<std::collections::BTreeMap<String, Lenient<T>>>::deserialize(d)? {
        Lenient::Value(items) => items
            .into_iter()
            .filter_map(|(k, v)| match v {
                Lenient::Value(v) => Some((k, v)),
                Lenient::Other(_) => None,
            })
            .collect(),
        Lenient::Other(_) => std::collections::BTreeMap::new(),
    })
}
