//! Writers for a table of text: CSV, a Markdown pipe table and an XLSX workbook. The table is the
//! one `table::read` returns, so what is written reads back through it.

use vizcraft_diagram::data::DataType;
use vizcraft_diagram::data_graphic::infer_text_type;

use crate::FormatError;
use crate::table::{Column, Table};

/// A table from column names and rows of text, each column typed by what its set cells share
/// (number, flag, date, else text) as `table::read` types a column.
pub fn from_text(columns: &[String], rows: Vec<Vec<String>>) -> Table {
    let columns = columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut kinds = rows.iter().filter_map(|r| r.get(i)).filter(|c| !c.trim().is_empty()).map(|c| infer_text_type(c));
            let kind = match kinds.next() {
                Some(k) if kinds.all(|o| o == k) => k,
                _ => DataType::String,
            };
            Column { name: name.clone(), kind }
        })
        .collect();
    Table { columns, rows, warnings: Vec::new() }
}

/// RFC 4180 quoting: a field with a comma, a quote or a line break is quoted, quotes doubled.
fn csv_field(f: &str) -> String {
    if f.contains([',', '"', '\n', '\r']) { format!("\"{}\"", f.replace('"', "\"\"")) } else { f.to_string() }
}

/// The table as CSV text: a header line, then one line per row, each ending in a line feed.
pub fn write_csv(t: &Table) -> String {
    let mut out = String::new();
    let line = |out: &mut String, cells: &mut dyn Iterator<Item = &str>| {
        out.push_str(&cells.map(csv_field).collect::<Vec<_>>().join(","));
        out.push('\n');
    };
    line(&mut out, &mut t.columns.iter().map(|c| c.name.as_str()));
    for row in &t.rows {
        line(&mut out, &mut row.iter().map(String::as_str));
    }
    out
}

/// A cell of a pipe table: backslashes and pipes escaped, line breaks turned into spaces.
fn md_cell(f: &str) -> String {
    f.chars()
        .map(|c| match c {
            '|' => "\\|".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' | '\r' | '\t' => " ".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// The table as a Markdown pipe table: a header, a rule line and one line per row.
pub fn write_markdown(t: &Table) -> String {
    let row = |cells: Vec<String>| format!("| {} |\n", cells.join(" | "));
    let mut out = row(t.columns.iter().map(|c| md_cell(&c.name)).collect());
    out.push_str(&row(t.columns.iter().map(|_| "---".to_string()).collect()));
    for r in &t.rows {
        out.push_str(&row((0..t.columns.len()).map(|j| md_cell(r.get(j).map_or("", String::as_str))).collect()));
    }
    out
}

/// The table as an XLSX workbook with one sheet called `sheet`.
pub fn write_xlsx(t: &Table, sheet: &str) -> Result<Vec<u8>, FormatError> {
    crate::xlsx::write(t, sheet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::data_graphic::TableFormat;

    fn sample() -> Table {
        let cols = ["id", "name", "cost", "done"].map(String::from);
        let rows = vec![
            vec!["a".into(), "Pump, \"main\"".into(), "12.5".into(), "true".into()],
            vec!["b".into(), "Tank | two\nlines <&>".into(), "3".into(), "false".into()],
            vec!["c".into(), " padded ".into(), "".into(), "".into()],
        ];
        from_text(&cols, rows)
    }

    #[test]
    fn csv_matches_its_golden() {
        let golden = "id,name,cost,done\na,\"Pump, \"\"main\"\"\",12.5,true\nb,\"Tank | two\nlines <&>\",3,false\nc, padded ,,\n";
        assert_eq!(write_csv(&sample()), golden);
    }

    #[test]
    fn markdown_matches_its_golden() {
        let golden = "| id | name | cost | done |\n| --- | --- | --- | --- |\n| a | Pump, \"main\" | 12.5 | true |\n| b | Tank \\| two lines <&> | 3 | false |\n| c |  padded  |  |  |\n";
        assert_eq!(write_markdown(&sample()), golden);
    }

    #[test]
    fn columns_are_typed_from_their_cells() {
        let kinds: Vec<DataType> = sample().columns.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [DataType::String, DataType::String, DataType::Number, DataType::Bool]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_xlsx_reads_back_equal() {
        let t = sample();
        let bytes = write_xlsx(&t, "Elements").unwrap();
        let back = crate::table::read(&bytes, TableFormat::Xlsx, "Elements").unwrap();
        assert_eq!(back.columns, t.columns);
        let want: Vec<Vec<String>> = t.rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect();
        assert_eq!(back.rows, want);
        let first = crate::table::read(&bytes, TableFormat::Xlsx, "").unwrap();
        assert_eq!(first.rows, want, "the first sheet when none is named");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn leading_zero_numbers_stay_text_and_hostile_text_is_safe() {
        let t = Table {
            columns: vec![Column { name: "tag".into(), kind: DataType::Number }, Column { name: "note".into(), kind: DataType::String }],
            rows: vec![vec!["007".into(), "a\u{1}b </t>".into()]],
            warnings: vec![],
        };
        let back = crate::table::read(&write_xlsx(&t, "x").unwrap(), TableFormat::Xlsx, "").unwrap();
        assert_eq!(back.rows, [["007".to_string(), "ab </t>".to_string()]]);
    }
}
