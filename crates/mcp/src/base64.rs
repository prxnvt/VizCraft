//! Standard base64 (RFC 4648, with padding), for MCP image content.
//!
//! The server encodes one PNG per screenshot and never decodes, so a dozen lines here are cheaper
//! than another dependency.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The character for the low six bits of `v`.
fn sym(v: u32) -> char {
    // `& 63` keeps the index inside the 64-entry table, so the fallback is never taken.
    char::from(ALPHABET.get((v & 63) as usize).copied().unwrap_or(b'A'))
}

/// Encode `bytes` as standard padded base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(sym(n >> 18));
        out.push(sym(n >> 12));
        out.push(if chunk.len() > 1 { sym(n >> 6) } else { '=' });
        out.push(if chunk.len() > 2 { sym(n) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn rfc4648_vectors() {
        let cases = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")];
        for (plain, coded) in cases {
            assert_eq!(encode(plain.as_bytes()), coded, "{plain}");
        }
        assert_eq!(encode(&[0xff, 0xfe, 0xfd]), "//79");
    }
}
