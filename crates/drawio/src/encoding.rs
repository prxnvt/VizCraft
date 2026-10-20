//! The compressed page encoding: the model XML is URI-component encoded, deflated without a zlib
//! header, then base64 encoded. Also the percent coding on its own, which JGraph's draw.io uses
//! for that first step.

use std::io::{Read, Write};

use base64::Engine;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};
use flate2::Compression;
use flate2::read::{DeflateDecoder, ZlibDecoder};
use flate2::write::DeflateEncoder;

use crate::{DrawioError, Result, limits};

/// Percent-encodes `s` like a URI component: ASCII letters, digits and `-_.!~*'()` stay as they
/// are, every other byte of the UTF-8 encoding becomes `%XX` with upper-case hex.
pub fn percent_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0')));
            out.push(char::from(HEX.get(usize::from(b & 0x0f)).copied().unwrap_or(b'0')));
        }
    }
    out
}

/// Decodes `%XX` sequences. A `%` not followed by two hex digits is kept literally, so a stray
/// percent sign never fails a page. Errors only when the decoded bytes are not UTF-8.
pub fn percent_decode(s: &str) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let (Some(h), Some(l)) = (bytes.get(i + 1).and_then(|c| hex_val(*c)), bytes.get(i + 2).and_then(|c| hex_val(*c)))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).map_err(|_| DrawioError::Encoding("percent-decoded page is not UTF-8".into()))
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Encodes model XML as a compressed page body.
pub fn compress_page(xml: &str) -> String {
    let encoded = percent_encode(xml);
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    // Writing into a Vec cannot fail; if it ever did, an empty body would be detected on read.
    let deflated = enc.write_all(encoded.as_bytes()).and_then(|()| enc.finish()).unwrap_or_default();
    STANDARD.encode(deflated)
}

/// Decodes a compressed page body back into model XML, capping the inflated size so a deflate
/// bomb fails instead of exhausting memory.
pub fn decompress_page(body: &str) -> Result<String> {
    let cleaned: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let lenient =
        GeneralPurpose::new(&base64::alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent));
    let raw = lenient.decode(cleaned.as_bytes()).map_err(|e| DrawioError::Compression(format!("page is not base64: {e}")))?;
    let inflated = match inflate_capped(DeflateDecoder::new(raw.as_slice())) {
        Ok(v) => v,
        // Some writers emit a zlib header; accept that too, but report the raw-deflate error for a
        // body that is neither.
        Err(DrawioError::Limit(m)) => return Err(DrawioError::Limit(m)),
        Err(first) => inflate_capped(ZlibDecoder::new(raw.as_slice())).map_err(|_| first)?,
    };
    let text = String::from_utf8(inflated).map_err(|_| DrawioError::Encoding("inflated page is not UTF-8".into()))?;
    // Older files deflate the XML directly; newer ones URI-encode it first.
    if text.trim_start().starts_with('<') { Ok(text) } else { percent_decode(&text) }
}

fn inflate_capped<R: Read>(reader: R) -> Result<Vec<u8>> {
    let cap = u64::try_from(limits::MAX_PAGE_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    let mut out = Vec::new();
    reader.take(cap).read_to_end(&mut out).map_err(|e| DrawioError::Compression(format!("page does not inflate: {e}")))?;
    if out.len() > limits::MAX_PAGE_BYTES {
        return Err(DrawioError::Limit(format!("decompressed page larger than {} bytes", limits::MAX_PAGE_BYTES)));
    }
    Ok(out)
}
