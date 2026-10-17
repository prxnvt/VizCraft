//! Tolerant deserializers. Files and agent requests are hostile, so a wrong value in one field
//! falls back to that field's default instead of failing the whole document.

use serde::{Deserialize, Deserializer};

/// Either the wanted value or anything else (which is ignored).
#[derive(Deserialize)]
#[serde(untagged)]
enum Lenient<T> {
    Value(T),
    Other(serde::de::IgnoredAny),
}

/// An `f64` that reads `null`, strings and other junk as `0.0`, and non-finite values as `0.0`.
pub fn num<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match Lenient::<f64>::deserialize(d)? {
        Lenient::Value(v) if v.is_finite() => v,
        _ => 0.0,
    })
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

/// Give a fieldless enum a stable camelCase name on disk, with `name()` and `from_name()`, and an
/// unknown or wrong-typed name loading as `$fallback` so newer files still open.
macro_rules! named_enum {
    ($t:ty, $fallback:expr, { $($v:path => $n:literal),* $(,)? }) => {
        impl $t {
            /// The name used on disk and in command params.
            pub fn name(self) -> &'static str {
                match self { $($v => $n),* }
            }
            /// The value for a name, or `None` for an unknown name.
            pub fn from_name(s: &str) -> Option<Self> {
                match s { $($n => Some($v),)* _ => None }
            }
        }
        impl serde::Serialize for $t {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.name())
            }
        }
        impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s: String = $crate::serde_util::lenient(d)?;
                Ok(Self::from_name(&s).unwrap_or($fallback))
            }
        }
    };
}
pub(crate) use named_enum;
