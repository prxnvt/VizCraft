//! A workbook writer: one sheet from the smallest parts SpreadsheetML needs, in a ZIP archive
//! whose entries are stored uncompressed. Written from ECMA-376 (Office Open XML, parts 1 and 2:
//! the package, the workbook and worksheet parts) and the ZIP appnote's record layouts; nothing
//! here reads another application's output or source, and it needs no dependency, so the web
//! build writes workbooks too. Reading a workbook is `table::read`.
//!
//! The parts are `[Content_Types].xml`, `_rels/.rels`, `xl/workbook.xml`,
//! `xl/_rels/workbook.xml.rels` and `xl/worksheets/sheet1.xml`. Text is written as inline strings
//! (no shared string table) and numbers and flags as typed cells.

use vizcraft_diagram::data::DataType;

use crate::FormatError;
use crate::table::Table;

/// Longest sheet name a workbook may carry.
const MAX_SHEET_NAME: usize = 31;

/// The CRC-32 table (polynomial 0xEDB88320), built at compile time.
const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// The CRC-32 of `data`, as a ZIP entry records it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        let idx = usize::from((c as u8) ^ b);
        c = CRC_TABLE.get(idx).copied().unwrap_or(0) ^ (c >> 8);
    }
    !c
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn too_big() -> FormatError {
    FormatError::Encode("the workbook is too large for a plain ZIP archive".into())
}

/// A ZIP archive of `entries` (name, bytes), each stored without compression: a local header and
/// the data per entry, then the central directory and the end record. The timestamp is fixed
/// (1980-01-01), so the same entries always give the same bytes.
pub fn zip_stored(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, FormatError> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let count = u16::try_from(entries.len()).map_err(|_| too_big())?;
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        // Local file header (signature, version needed 2.0, flags with the UTF-8 name bit,
        // method 0 stored, time, date, crc, sizes, name length, extra length).
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20);
        put16(&mut out, 0x0800);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0x0021);
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, name_len);
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Central directory header: the same fields plus version made by, comment length, disk
        // number, internal and external attributes and the local header's offset.
        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 20);
        put16(&mut central, 0x0800);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0x0021);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, name_len);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let dir_offset = u32::try_from(out.len()).map_err(|_| too_big())?;
    let dir_size = u32::try_from(central.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, dir_size);
    put32(&mut out, dir_offset);
    put16(&mut out, 0);
    Ok(out)
}

/// `text` for XML character data or an attribute: the five markup characters escaped and the
/// characters XML 1.0 forbids (most control characters) dropped.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// The column letters of the zero-based column `i` (`A`, `B`, .. `Z`, `AA`).
fn column_letters(mut i: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    letters.iter().rev().map(|b| char::from(*b)).collect()
}

/// A sheet name a workbook accepts: no `[ ] : * ? / \`, at most 31 characters, not empty.
fn sheet_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') && !c.is_control()).take(MAX_SHEET_NAME).collect();
    let n = n.trim().to_string();
    if n.is_empty() { "Sheet1".to_string() } else { n }
}

/// One cell: a number or flag when the column's kind says so and the text fits it, else text.
fn cell(out: &mut String, at: &str, kind: DataType, text: &str) {
    if text.is_empty() {
        return;
    }
    let t = text.trim();
    match kind {
        // Only a number that prints back as it was typed is stored as one, so `007` stays text.
        DataType::Number if t.parse::<f64>().is_ok_and(|f| f.is_finite() && vizcraft_diagram::data::fmt_number(f) == t) => {
            out.push_str(&format!("<c r=\"{at}\"><v>{t}</v></c>"));
        }
        DataType::Bool if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") => {
            out.push_str(&format!("<c r=\"{at}\" t=\"b\"><v>{}</v></c>", u8::from(t.eq_ignore_ascii_case("true"))));
        }
        _ => out.push_str(&format!("<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", xml_escape(text))),
    }
}

/// A workbook with one sheet called `sheet` holding `table`: the column names as the first row,
/// then the rows, in a stored-entry ZIP. A number column writes numbers, a Bool column flags,
/// everything else inline text; a cell that does not fit its column's kind is written as text.
pub fn write(table: &Table, sheet: &str) -> Result<Vec<u8>, FormatError> {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
    const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
    let content_types = format!(
        "{HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
</Types>"
    );
    let root_rels = format!(
        "{HEAD}<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
    );
    let workbook = format!(
        "{HEAD}<workbook xmlns=\"{MAIN}\" xmlns:r=\"{REL}\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        xml_escape(&sheet_name(sheet))
    );
    let workbook_rels = format!(
        "{HEAD}<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>"
    );

    let mut data = String::new();
    data.push_str(&format!("{HEAD}<worksheet xmlns=\"{MAIN}\"><sheetData>"));
    data.push_str("<row r=\"1\">");
    for (j, c) in table.columns.iter().enumerate() {
        cell(&mut data, &format!("{}1", column_letters(j)), DataType::String, &c.name);
    }
    data.push_str("</row>");
    for (i, row) in table.rows.iter().enumerate() {
        let r = i.saturating_add(2);
        data.push_str(&format!("<row r=\"{r}\">"));
        for (j, text) in row.iter().enumerate() {
            let kind = table.columns.get(j).map_or(DataType::String, |c| c.kind);
            cell(&mut data, &format!("{}{r}", column_letters(j)), kind, text);
        }
        data.push_str("</row>");
    }
    data.push_str("</sheetData></worksheet>");
    zip_stored(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", data.as_bytes()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_known_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn column_letters_roll_over() {
        let l: Vec<String> = [0, 1, 25, 26, 27, 51, 52, 701, 702].iter().map(|i| column_letters(*i)).collect();
        assert_eq!(l, ["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
    }

    #[test]
    fn the_archive_has_a_local_header_per_entry_and_an_end_record() {
        let z = zip_stored(&[("a.txt", b"hello"), ("dir/b.txt", b"")]).unwrap();
        assert_eq!(&z[..4], b"PK\x03\x04");
        assert_eq!(z.windows(4).filter(|w| *w == b"PK\x03\x04").count(), 2);
        assert_eq!(z.windows(4).filter(|w| *w == b"PK\x01\x02").count(), 2);
        let end = z.len() - 22;
        assert_eq!(&z[end..end + 4], b"PK\x05\x06");
        assert_eq!(u16::from_le_bytes([z[end + 10], z[end + 11]]), 2);
        assert_eq!(zip_stored(&[("a.txt", b"hello"), ("dir/b.txt", b"")]).unwrap(), z, "deterministic");
    }

    #[test]
    fn xml_text_is_escaped_and_forbidden_characters_dropped() {
        assert_eq!(xml_escape("a<b>&\"'\u{1}c\n"), "a&lt;b&gt;&amp;&quot;&apos;c\n");
        assert_eq!(sheet_name("A/B:[C]?"), "ABC");
        assert_eq!(sheet_name(""), "Sheet1");
        assert_eq!(sheet_name(&"x".repeat(60)).len(), 31);
    }
}
