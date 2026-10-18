//! Table reading for data links: a CSV file (the `csv` crate) or an XLSX workbook (`calamine`,
//! not in the web build) as named columns and rows of text, with each column's type inferred.
//!
//! The first row names the columns: blank names become `Column N` and repeats get a number. Every
//! input is hostile, so the table is capped at [`MAX_ROWS`] rows, [`MAX_COLUMNS`] columns and
//! [`MAX_CELL`] bytes a cell; anything cut is reported in `warnings`. A workbook's ZIP directory
//! is checked before it is opened, so a small file that would inflate to gigabytes is refused.

use vizcraft_diagram::data::DataType;
use vizcraft_diagram::data_graphic::{TableFormat, infer_text_type};

use crate::FormatError;

/// Most data rows read.
pub const MAX_ROWS: usize = 10_000;
/// Most columns read.
pub const MAX_COLUMNS: usize = 256;
/// Longest cell kept, in bytes.
pub const MAX_CELL: usize = 4096;
/// Largest file read, in bytes.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// Largest total a workbook's parts may inflate to, in bytes.
pub const MAX_INFLATED: u64 = 256 * 1024 * 1024;

/// A named column and the type its values share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// The header text, unique within the table.
    pub name: String,
    /// `number`, `bool` or `date` when every set value is one; else `string`.
    pub kind: DataType,
}

/// A table: columns and rows of cell text, every row as long as the column list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// The columns, left to right.
    pub columns: Vec<Column>,
    /// The data rows, top to bottom.
    pub rows: Vec<Vec<String>>,
    /// What was cut or skipped, in plain words.
    pub warnings: Vec<String>,
}

impl Table {
    /// The index of the column called `name` (exact, then ignoring case and outer spaces).
    pub fn column(&self, name: &str) -> Option<usize> {
        let t = name.trim();
        self.columns.iter().position(|c| c.name == name).or_else(|| self.columns.iter().position(|c| c.name.trim().eq_ignore_ascii_case(t)))
    }
    /// The text of row `row`, column `col`; empty outside the table.
    pub fn cell(&self, row: usize, col: usize) -> &str {
        self.rows.get(row).and_then(|r| r.get(col)).map_or("", String::as_str)
    }
}

/// Read `bytes` as a table in `format`; `sheet` names a workbook sheet (empty is the first).
pub fn read(bytes: &[u8], format: TableFormat, sheet: &str) -> Result<Table, FormatError> {
    if bytes.len() > MAX_BYTES {
        return Err(FormatError::TooLarge { len: bytes.len(), max: MAX_BYTES });
    }
    let grid = match format {
        TableFormat::Csv => csv_grid(bytes)?,
        TableFormat::Xlsx => xlsx_grid(bytes, sheet)?,
    };
    Ok(from_grid(grid))
}

/// Rows of cells as read, before the header is taken off, plus warnings so far.
struct Grid {
    rows: Vec<Vec<String>>,
    warnings: Vec<String>,
}

/// Cut a cell to [`MAX_CELL`] bytes at a char boundary; returns whether it was cut.
fn clamp_cell(s: &mut String) -> bool {
    if s.len() <= MAX_CELL {
        return false;
    }
    let mut cut = MAX_CELL;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    true
}

/// The header, unique names, rows padded or cut to the header, types inferred.
fn from_grid(g: Grid) -> Table {
    let mut warnings = g.warnings;
    let mut rows = g.rows.into_iter();
    let header = rows.next().unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for (i, h) in header.iter().enumerate() {
        let base = if h.trim().is_empty() { format!("Column {}", i + 1) } else { h.trim().to_string() };
        let mut name = base.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        names.push(name);
    }
    let width = names.len();
    let mut data: Vec<Vec<String>> = Vec::new();
    for mut r in rows {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        r.resize(width, String::new());
        data.push(r);
    }
    let columns = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let mut kinds = data.iter().filter_map(|r| r.get(i)).filter(|c| !c.trim().is_empty()).map(|c| infer_text_type(c));
            let first = kinds.next();
            let kind = match first {
                Some(k) if kinds.all(|o| o == k) => k,
                _ => DataType::String,
            };
            Column { name, kind }
        })
        .collect();
    if width == 0 {
        warnings.push("the table has no header row".into());
    }
    Table { columns, rows: data, warnings }
}

/// Collect rows with the caps, from any source of rows of cells.
fn capped(rows: impl Iterator<Item = Vec<String>>, mut warnings: Vec<String>) -> Grid {
    let mut out = Vec::new();
    let (mut wide, mut long) = (false, false);
    for mut r in rows {
        // The header row plus the data rows.
        if out.len() > MAX_ROWS {
            warnings.push(format!("only the first {MAX_ROWS} rows are read"));
            break;
        }
        if r.len() > MAX_COLUMNS {
            r.truncate(MAX_COLUMNS);
            wide = true;
        }
        for c in &mut r {
            long |= clamp_cell(c);
        }
        out.push(r);
    }
    if wide {
        warnings.push(format!("only the first {MAX_COLUMNS} columns are read"));
    }
    if long {
        warnings.push(format!("cells longer than {MAX_CELL} bytes are cut"));
    }
    Grid { rows: out, warnings }
}

/// The delimiter of a CSV file: a comma, unless the first line has none and has semicolons or
/// tabs.
fn delimiter(text: &[u8]) -> u8 {
    let first = text.split(|b| *b == b'\n').next().unwrap_or_default();
    let count = |d: u8| first.iter().filter(|b| **b == d).count();
    if count(b',') > 0 {
        b','
    } else if count(b';') > 0 {
        b';'
    } else if count(b'\t') > 0 {
        b'\t'
    } else {
        b','
    }
}

fn csv_grid(bytes: &[u8]) -> Result<Grid, FormatError> {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF".as_slice()).unwrap_or(bytes);
    let mut rd = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delimiter(body)).from_reader(body);
    let mut bad = 0usize;
    let rows = rd.byte_records().filter_map(|r| match r {
        Ok(rec) => Some(rec.iter().map(|f| String::from_utf8_lossy(f).into_owned()).collect::<Vec<_>>()),
        Err(_) => {
            bad = bad.saturating_add(1);
            None
        }
    });
    let mut grid = capped(rows, Vec::new());
    if bad > 0 {
        grid.warnings.push(format!("{bad} unreadable CSV records were skipped"));
    }
    if grid.rows.is_empty() {
        return Err(FormatError::Import("CSV: the file is empty".into()));
    }
    Ok(grid)
}

/// Sum the uncompressed sizes in a ZIP file's central directory (APPNOTE.TXT, sections 4.3.12
/// and 4.3.16); `None` when the directory cannot be found or uses ZIP64 sizes.
fn zip_inflated_size(b: &[u8]) -> Option<u64> {
    let u16_at = |i: usize| b.get(i..i.checked_add(2)?).and_then(|s| s.try_into().ok()).map(u16::from_le_bytes);
    let u32_at = |i: usize| b.get(i..i.checked_add(4)?).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes);
    // The end record is 22 bytes plus a comment of at most 65535 bytes.
    let lowest = b.len().saturating_sub(22 + 65_535);
    let eocd = (lowest..=b.len().saturating_sub(22)).rev().find(|&i| u32_at(i) == Some(0x0605_4b50))?;
    let count = usize::from(u16_at(eocd + 10)?);
    let mut at = usize::try_from(u32_at(eocd + 16)?).ok()?;
    let mut total: u64 = 0;
    for _ in 0..count {
        if u32_at(at)? != 0x0201_4b50 {
            return None;
        }
        let size = u32_at(at + 24)?;
        if size == u32::MAX {
            return None;
        }
        total = total.saturating_add(u64::from(size));
        let skip = 46usize + usize::from(u16_at(at + 28)?) + usize::from(u16_at(at + 30)?) + usize::from(u16_at(at + 32)?);
        at = at.checked_add(skip)?;
    }
    Some(total)
}

#[cfg(target_arch = "wasm32")]
fn xlsx_grid(_bytes: &[u8], _sheet: &str) -> Result<Grid, FormatError> {
    Err(FormatError::Import("XLSX: workbooks cannot be read in the web build; save the sheet as CSV".into()))
}

#[cfg(not(target_arch = "wasm32"))]
fn xlsx_grid(bytes: &[u8], sheet: &str) -> Result<Grid, FormatError> {
    use calamine::{Data, Reader, Xlsx};
    match zip_inflated_size(bytes) {
        None => return Err(FormatError::Import("XLSX: not a workbook (no ZIP directory)".into())),
        Some(n) if n > MAX_INFLATED => {
            return Err(FormatError::Import(format!("XLSX: the workbook inflates to more than {} MiB", MAX_INFLATED / (1024 * 1024))));
        }
        Some(_) => {}
    }
    let mut wb: Xlsx<_> = Xlsx::new(std::io::Cursor::new(bytes)).map_err(|e| FormatError::Import(format!("XLSX: {e}")))?;
    let names = wb.sheet_names();
    let name = if sheet.trim().is_empty() {
        names.first().cloned().ok_or_else(|| FormatError::Import("XLSX: the workbook has no sheets".into()))?
    } else {
        names
            .iter()
            .find(|n| n.as_str() == sheet || n.eq_ignore_ascii_case(sheet.trim()))
            .cloned()
            .ok_or_else(|| FormatError::Import(format!("XLSX: no sheet `{}` (sheets: {})", short(sheet), names.join(", "))))?
    };
    let range = wb.worksheet_range(&name).map_err(|e| FormatError::Import(format!("XLSX: {e}")))?;
    let text = |d: &Data| -> String {
        match d {
            Data::Empty => String::new(),
            Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
            Data::Int(i) => i.to_string(),
            Data::Float(f) => vizcraft_diagram::data::fmt_number(*f),
            Data::Bool(b) => b.to_string(),
            Data::DateTime(dt) => serial_date(dt.as_f64()),
            Data::Error(e) => format!("#{e:?}"),
        }
    };
    let rows = range.rows().map(|r| r.iter().take(MAX_COLUMNS + 1).map(text).collect::<Vec<_>>());
    Ok(capped(rows, Vec::new()))
}

/// A spreadsheet date serial (days since 1899-12-30) as `YYYY-MM-DD`; times of day are dropped.
#[cfg(not(target_arch = "wasm32"))]
fn serial_date(serial: f64) -> String {
    if !serial.is_finite() || !(0.0..3_000_000.0).contains(&serial) {
        return vizcraft_diagram::data::fmt_number(serial);
    }
    // Days from 1970-01-01, then the civil date (Howard Hinnant's days_from_civil inverse).
    let z = serial.floor() as i64 - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn short(s: &str) -> String {
    s.chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_header_types_and_padding() {
        let t = read(b"\xEF\xBB\xBFTag,Cost,Done,Due,,Tag\nA,12.5,true,2026-01-02,x\nB,3,false,2026-02-03\n\n", TableFormat::Csv, "").unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Tag", "Cost", "Done", "Due", "Column 5", "Tag 2"]);
        let kinds: Vec<DataType> = t.columns.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [DataType::String, DataType::Number, DataType::Bool, DataType::Date, DataType::String, DataType::String]);
        assert_eq!(t.rows.len(), 2, "the blank line is skipped");
        assert_eq!(t.rows[1].len(), 6, "short rows are padded");
        assert_eq!(t.column("cost"), Some(1));
        assert_eq!(t.cell(0, 1), "12.5");
        assert_eq!(t.cell(99, 99), "");
    }

    #[test]
    fn csv_caps_rows_columns_and_cells() {
        let mut text = String::new();
        text.push_str(&(0..300).map(|i| format!("c{i}")).collect::<Vec<_>>().join(","));
        text.push('\n');
        text.push_str(&"x".repeat(5000));
        text.push('\n');
        for i in 0..10_050 {
            text.push_str(&format!("{i}\n"));
        }
        let t = read(text.as_bytes(), TableFormat::Csv, "").unwrap();
        assert_eq!(t.columns.len(), MAX_COLUMNS);
        assert_eq!(t.rows.len(), MAX_ROWS);
        assert_eq!(t.rows[0][0].len(), MAX_CELL);
        assert_eq!(t.warnings.len(), 3, "{:?}", t.warnings);
        assert_eq!(read(b"a;b\n1;2\n", TableFormat::Csv, "").unwrap().columns.len(), 2, "semicolons are sniffed");
    }

    #[test]
    fn junk_never_panics() {
        for bytes in [&b""[..], b"\xff\xfe\x00", b"\"unterminated\n", b"PK\x05\x06", &[0u8; 64]] {
            let _ = read(bytes, TableFormat::Csv, "");
            assert!(read(bytes, TableFormat::Xlsx, "").is_err());
        }
        assert!(matches!(read(&vec![b'a'; MAX_BYTES + 1], TableFormat::Csv, ""), Err(FormatError::TooLarge { .. })));
    }

    /// CRC-32 (IEEE), for the stored ZIP entries the test writes.
    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for b in data {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
            }
        }
        !c
    }

    /// A ZIP archive of `files`, stored without compression (APPNOTE.TXT, sections 4.3.7, 4.3.12
    /// and 4.3.16).
    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut dir = Vec::new();
        for (name, body) in files {
            let (n, b) = (name.as_bytes(), body.as_bytes());
            let crc = crc32(b);
            let offset = out.len() as u32;
            let common = |v: &mut Vec<u8>| {
                v.extend_from_slice(&20u16.to_le_bytes()); // version needed
                v.extend_from_slice(&0u16.to_le_bytes()); // flags
                v.extend_from_slice(&0u16.to_le_bytes()); // stored
                v.extend_from_slice(&0u32.to_le_bytes()); // time and date
                v.extend_from_slice(&crc.to_le_bytes());
                v.extend_from_slice(&(b.len() as u32).to_le_bytes());
                v.extend_from_slice(&(b.len() as u32).to_le_bytes());
                v.extend_from_slice(&(n.len() as u16).to_le_bytes());
                v.extend_from_slice(&0u16.to_le_bytes()); // extra
            };
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            common(&mut out);
            out.extend_from_slice(n);
            out.extend_from_slice(b);
            dir.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
            common(&mut dir);
            dir.extend_from_slice(&[0u8; 6]); // comment length, disk, internal attributes
            dir.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            dir.extend_from_slice(&offset.to_le_bytes());
            dir.extend_from_slice(n);
        }
        let start = out.len() as u32;
        out.extend_from_slice(&dir);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// The smallest workbook: content types, the package and workbook relationships, the
    /// workbook with one sheet called Data, shared strings and the sheet. Written by hand from
    /// the Office Open XML part layout (ECMA-376 Part 1, SpreadsheetML).
    fn workbook() -> Vec<u8> {
        const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#;
        let rels = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
        );
        let book = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let book_rels = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#
        );
        let strings = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="{MAIN}" count="4" uniqueCount="4"><si><t>Tag</t></si><si><t>Cost</t></si><si><t>A</t></si><si><t>B</t></si></sst>"#
        );
        let sheet = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>12.5</v></c></row><row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3"><v>7</v></c></row></sheetData></worksheet>"#
        );
        zip(&[
            ("[Content_Types].xml", types),
            ("_rels/.rels", &rels),
            ("xl/workbook.xml", &book),
            ("xl/_rels/workbook.xml.rels", &book_rels),
            ("xl/sharedStrings.xml", &strings),
            ("xl/worksheets/sheet1.xml", &sheet),
        ])
    }

    #[test]
    fn xlsx_reads_a_hand_written_workbook() {
        let bytes = workbook();
        assert!(zip_inflated_size(&bytes).is_some_and(|n| n > 0));
        let t = read(&bytes, TableFormat::Xlsx, "").unwrap();
        assert_eq!(t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Tag", "Cost"]);
        assert_eq!(t.columns[1].kind, DataType::Number);
        assert_eq!(t.rows, [vec!["A".to_string(), "12.5".to_string()], vec!["B".to_string(), "7".to_string()]]);
        assert_eq!(read(&bytes, TableFormat::Xlsx, "data").unwrap().rows.len(), 2, "sheet names ignore case");
        assert!(read(&bytes, TableFormat::Xlsx, "Nope").is_err());
    }

    #[test]
    fn spreadsheet_serials_are_dates() {
        assert_eq!(serial_date(45_658.0), "2025-01-01");
        assert_eq!(serial_date(25_569.75), "1970-01-01");
    }
}
