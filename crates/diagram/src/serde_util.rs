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

/// A `bool` whose wrong-typed values read as `true`, for flags that are on by default.
pub fn lenient_true<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Lenient::<bool>::deserialize(d)? {
        Lenient::Value(v) => v,
        Lenient::Other(_) => true,
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

/// Whether a value equals its type's default; used to leave default fields out of files.
pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Bytes as standard base64 (with padding) on disk; unreadable text loads as no bytes.
pub mod base64 {
    use serde::{Deserializer, Serializer};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Standard base64 with `=` padding.
    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
        for chunk in bytes.chunks(3) {
            let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            let sym = |shift: u32| char::from(ALPHABET.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'A'));
            out.push(sym(18));
            out.push(sym(12));
            out.push(if chunk.len() > 1 { sym(6) } else { '=' });
            out.push(if chunk.len() > 2 { sym(0) } else { '=' });
        }
        out
    }

    /// Decode standard base64, ignoring ASCII whitespace; `None` for any other bad character.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let mut acc: u32 = 0;
        let mut bits = 0u32;
        for c in text.bytes() {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => break,
                c if c.is_ascii_whitespace() => continue,
                _ => return None,
            };
            acc = (acc << 6) | u32::from(v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((acc >> bits) & 0xff) as u8);
            }
            acc &= (1 << bits) - 1;
        }
        Some(out)
    }

    /// Serde serializer for `Vec<u8>` as base64 text.
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(bytes))
    }

    /// Serde deserializer for base64 text; anything unreadable is no bytes.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text: String = super::lenient(d)?;
        Ok(decode(&text).unwrap_or_default())
    }
}
