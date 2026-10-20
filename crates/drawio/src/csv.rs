//! CSV import in the style of JGraph's draw.io.
//!
//! The text starts with a header of `#` lines. `## text` is a comment; `# key: value` is a
//! directive (label template, style, which column identifies a row, which columns link rows,
//! sizes, spacing, layout). The first other line starts an ordinary RFC 4180 CSV table whose first
//! row names the columns. Each data row becomes one node; `connect` directives turn references
//! between rows into edges. A directive value of `-` means "not set".
//!
//! The result is a [`CsvDiagram`]: nodes with their resolved label and style, edges, parent links
//! and the layout hints. Placing the nodes is left to the caller's layout engine.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::attrs::parse_f64;
use crate::{DrawioError, Result, limits};

/// A parsed CSV diagram.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CsvDiagram {
    /// Column names from the header row, in order.
    pub columns: Vec<String>,
    /// One node per data row, in row order.
    pub nodes: Vec<CsvNode>,
    /// Edges produced by the `connect` directives.
    pub edges: Vec<CsvEdge>,
    /// The directives as read.
    pub directives: CsvDirectives,
    /// `layout`, when given.
    pub layout: Option<CsvLayout>,
    /// Node, level and edge spacing.
    pub spacing: Spacing,
    /// Problems that did not stop the import: unknown directives, bad values, dangling
    /// references.
    pub warnings: Vec<String>,
}

/// One node built from a data row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CsvNode {
    /// Zero-based data row (the header row is not counted).
    pub row: usize,
    /// Cell id: the namespace followed by the identity column's value, or by the row number when
    /// there is no identity column or the value is empty.
    pub id: String,
    /// Column/value pairs kept as node data: every column except those in `ignore`.
    pub attributes: Vec<(String, String)>,
    /// The label with `%column%` placeholders resolved.
    pub label: String,
    /// The style with placeholders resolved. A row other rows name as their parent gets
    /// `parentstyle` when that is set.
    pub style: String,
    /// Width from the `width` directive.
    pub width: Option<NodeSize>,
    /// Height from the `height` directive.
    pub height: Option<NodeSize>,
    /// The link column's value, when non-empty.
    pub link: Option<String>,
    /// X from the `left` column.
    pub left: Option<f64>,
    /// Y from the `top` column.
    pub top: Option<f64>,
    /// Id of the parent node (container), from the `parent` column.
    pub parent: Option<String>,
    /// Whether another row names this one as its parent.
    pub is_container: bool,
}

/// A node dimension.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NodeSize {
    /// Fit the label.
    Auto,
    /// A fixed size in pixels.
    Fixed(f64),
}

/// An edge between two nodes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CsvEdge {
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Label from the directive, with placeholders resolved against the row holding the
    /// reference.
    pub label: Option<String>,
    /// Style from the directive.
    pub style: Option<String>,
    /// Index of the `connect` directive that produced this edge.
    pub connect: usize,
}

/// Spacing hints for the layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Spacing {
    /// `nodespacing`: between siblings.
    pub node: Option<f64>,
    /// `levelspacing`: between tree levels.
    pub level: Option<f64>,
    /// `edgespacing`: between parallel edges.
    pub edge: Option<f64>,
}

/// The `layout` directive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvLayout {
    /// `auto`: let the importer choose.
    Auto,
    /// `none`: use `left`/`top` or leave nodes where they fall.
    None,
    /// `verticaltree`.
    VerticalTree,
    /// `horizontaltree`.
    HorizontalTree,
    /// `verticalflow`.
    VerticalFlow,
    /// `horizontalflow`.
    HorizontalFlow,
    /// `organic`.
    Organic,
    /// `circle`.
    Circle,
    /// `orgchart`.
    OrgChart,
    /// Anything else, such as a JSON list of custom layouts, kept verbatim.
    Other(String),
}

/// A `width`/`height` directive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SizeSpec {
    /// `auto`.
    Auto,
    /// A number.
    Fixed(f64),
    /// `@column`: read the size from a column.
    Column(String),
}

/// One `connect` directive.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectSpec {
    /// Column holding references (several separated by commas).
    pub from: String,
    /// Column the references are matched against.
    pub to: String,
    /// Reverse the edge direction: from the referenced row to the referencing one.
    pub invert: bool,
    /// Edge label.
    pub label: Option<String>,
    /// Edge style.
    pub style: Option<String>,
}

/// The header directives. Unset ones (absent or `-`) are `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CsvDirectives {
    /// `label`: label template.
    pub label: Option<String>,
    /// `style`: style template.
    pub style: Option<String>,
    /// `stylename`: column whose value picks a style from `styles`.
    pub stylename: Option<String>,
    /// `styles`: named style templates.
    pub styles: Vec<(String, String)>,
    /// `labelname`: column whose value picks a label from `labels`.
    pub labelname: Option<String>,
    /// `labels`: named label templates.
    pub labels: Vec<(String, String)>,
    /// `vars`: columns exposed as placeholders.
    pub vars: Vec<String>,
    /// `identity`: column with the row's identifier.
    pub identity: Option<String>,
    /// `parent`: column with the parent row's identifier.
    pub parent: Option<String>,
    /// `parentstyle`: style for rows used as parents.
    pub parentstyle: Option<String>,
    /// `namespace`: prefix for generated ids.
    pub namespace: Option<String>,
    /// `connect` directives, in order.
    pub connect: Vec<ConnectSpec>,
    /// `width`.
    pub width: Option<SizeSpec>,
    /// `height`.
    pub height: Option<SizeSpec>,
    /// `padding` around auto-sized labels.
    pub padding: Option<f64>,
    /// `ignore`: columns not kept as node data.
    pub ignore: Vec<String>,
    /// `link`: column holding a link.
    pub link: Option<String>,
    /// `left`: column holding x.
    pub left: Option<String>,
    /// `top`: column holding y.
    pub top: Option<String>,
}

struct Warnings(Vec<String>);

impl Warnings {
    fn push(&mut self, w: String) {
        // Hostile input could otherwise produce one warning per row.
        if self.0.len() < limits::MAX_WARNINGS {
            self.0.push(w);
        } else if self.0.len() == limits::MAX_WARNINGS {
            self.0.push("further warnings suppressed".into());
        }
    }
}

/// Parses draw.io-style CSV text into a [`CsvDiagram`].
pub fn parse_csv(text: &str) -> Result<CsvDiagram> {
    if text.len() > limits::MAX_FILE_BYTES {
        return Err(DrawioError::Limit(format!("CSV larger than {} bytes", limits::MAX_FILE_BYTES)));
    }
    let text = text.trim_start_matches('\u{feff}');
    let mut warnings = Warnings(Vec::new());
    let mut d = CsvDirectives::default();
    let mut layout = None;
    let mut spacing = Spacing::default();

    // Header: every leading line that starts with `#` or is blank.
    let mut table_start = text.len();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset += line.len();
            continue;
        }
        if !trimmed.starts_with('#') {
            table_start = offset;
            break;
        }
        offset += line.len();
        if trimmed.starts_with("##") {
            continue;
        }
        let body = trimmed.trim_start_matches('#').trim();
        if body.is_empty() {
            continue;
        }
        let Some((key, value)) = body.split_once(':') else {
            warnings.push(format!("header line without a colon: {}", clip(body)));
            continue;
        };
        apply_directive(&mut d, &mut layout, &mut spacing, &key.trim().to_ascii_lowercase(), value.trim(), &mut warnings);
    }

    let table = text.get(table_start..).unwrap_or("");
    let mut rows = parse_table(table, &mut warnings)?;
    if rows.is_empty() {
        return Ok(CsvDiagram { directives: d, layout, spacing, warnings: warnings.0, ..CsvDiagram::default() });
    }
    let columns: Vec<String> = rows.remove(0).into_iter().map(|c| c.trim().to_string()).collect();
    let col_index: HashMap<&str, usize> = columns.iter().enumerate().rev().map(|(i, c)| (c.as_str(), i)).collect();
    let check_col = |name: &Option<String>, what: &str, warnings: &mut Warnings| {
        if let Some(n) = name
            && !col_index.contains_key(n.as_str())
        {
            warnings.push(format!("{what} column '{}' is not in the table", clip(n)));
        }
    };
    check_col(&d.identity, "identity", &mut warnings);
    check_col(&d.parent, "parent", &mut warnings);
    check_col(&d.link, "link", &mut warnings);
    check_col(&d.left, "left", &mut warnings);
    check_col(&d.top, "top", &mut warnings);
    check_col(&d.stylename, "stylename", &mut warnings);
    check_col(&d.labelname, "labelname", &mut warnings);

    let get = |row: &[String], col: &Option<String>| -> Option<String> {
        let c = col.as_deref()?;
        let i = *col_index.get(c)?;
        row.get(i).cloned()
    };
    let namespace = d.namespace.clone().unwrap_or_default();

    // First pass: ids and identities, so parents and edges can refer to any row.
    let mut nodes: Vec<CsvNode> = Vec::with_capacity(rows.len());
    let mut by_identity: HashMap<String, usize> = HashMap::new();
    for (row_no, row) in rows.iter().enumerate() {
        let identity = get(row, &d.identity).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let id = format!("{namespace}{}", identity.clone().unwrap_or_else(|| row_no.to_string()));
        if let Some(ident) = identity {
            match by_identity.entry(ident) {
                std::collections::hash_map::Entry::Occupied(e) => {
                    warnings.push(format!("row {row_no}: duplicate identity '{}'", clip(e.key())));
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(row_no);
                }
            }
        }
        nodes.push(CsvNode { row: row_no, id, ..CsvNode::default() });
    }

    // Parents.
    for (row_no, row) in rows.iter().enumerate() {
        let Some(p) = get(row, &d.parent).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
            continue;
        };
        match by_identity.get(&p).copied() {
            Some(pi) if pi != row_no => {
                let pid = nodes.get(pi).map(|n| n.id.clone());
                if let Some(n) = nodes.get_mut(pi) {
                    n.is_container = true;
                }
                if let Some(n) = nodes.get_mut(row_no) {
                    n.parent = pid;
                }
            }
            Some(_) => warnings.push(format!("row {row_no}: a row cannot be its own parent")),
            None => warnings.push(format!("row {row_no}: parent '{}' not found", clip(&p))),
        }
    }

    // Labels, styles, sizes and data.
    let styles: HashMap<&str, &str> = d.styles.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let labels: HashMap<&str, &str> = d.labels.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let ignored: std::collections::HashSet<&str> = d.ignore.iter().map(String::as_str).collect();
    for (row, node) in rows.iter().zip(nodes.iter_mut()) {
        let label_tpl =
            get(row, &d.labelname).and_then(|k| labels.get(k.as_str()).map(|s| s.to_string())).or_else(|| d.label.clone()).unwrap_or_default();
        node.label = resolve_placeholders(&label_tpl, &col_index, row);
        let style_tpl = if node.is_container && d.parentstyle.is_some() {
            d.parentstyle.clone().unwrap_or_default()
        } else {
            get(row, &d.stylename).and_then(|k| styles.get(k.as_str()).map(|s| s.to_string())).or_else(|| d.style.clone()).unwrap_or_default()
        };
        node.style = resolve_placeholders(&style_tpl, &col_index, row);
        node.width = size_for(&d.width, row, &col_index, node.row, "width", &mut warnings);
        node.height = size_for(&d.height, row, &col_index, node.row, "height", &mut warnings);
        node.link = get(row, &d.link).filter(|s| !s.trim().is_empty());
        node.left = get(row, &d.left).and_then(|s| parse_f64(&s));
        node.top = get(row, &d.top).and_then(|s| parse_f64(&s));
        node.attributes =
            columns.iter().zip(row.iter()).filter(|(c, _)| !ignored.contains(c.as_str())).map(|(c, v)| (c.clone(), v.clone())).collect();
    }

    // Edges.
    let mut edges = Vec::new();
    'connect: for (ci, spec) in d.connect.iter().enumerate() {
        let (Some(&from_i), Some(&to_i)) = (col_index.get(spec.from.as_str()), col_index.get(spec.to.as_str())) else {
            warnings.push(format!("connect {ci}: column '{}' or '{}' is not in the table", clip(&spec.from), clip(&spec.to)));
            continue;
        };
        let mut targets: HashMap<&str, usize> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            if let Some(v) = row.get(to_i).map(|s| s.trim())
                && !v.is_empty()
            {
                targets.entry(v).or_insert(i);
            }
        }
        for (i, row) in rows.iter().enumerate() {
            let refs = row.get(from_i).map(String::as_str).unwrap_or("");
            for r in refs.split(',').map(str::trim).filter(|r| !r.is_empty()) {
                let Some(&j) = targets.get(r) else {
                    warnings.push(format!("row {i}: reference '{}' in column '{}' not found", clip(r), clip(&spec.from)));
                    continue;
                };
                if edges.len() >= limits::MAX_CSV_EDGES {
                    warnings.push(format!("more than {} edges; the rest were dropped", limits::MAX_CSV_EDGES));
                    break 'connect;
                }
                let (Some(a), Some(b)) = (nodes.get(i), nodes.get(j)) else { continue };
                let (source, target) = if spec.invert { (b.id.clone(), a.id.clone()) } else { (a.id.clone(), b.id.clone()) };
                edges.push(CsvEdge {
                    source,
                    target,
                    label: spec.label.as_ref().map(|l| resolve_placeholders(l, &col_index, row)),
                    style: spec.style.clone(),
                    connect: ci,
                });
            }
        }
    }

    Ok(CsvDiagram { columns, nodes, edges, directives: d, layout, spacing, warnings: warnings.0 })
}

/// Shortens text quoted in warnings so a hostile field cannot make them huge.
fn clip(s: &str) -> String {
    const MAX: usize = 60;
    if s.chars().count() <= MAX { s.to_string() } else { format!("{}...", s.chars().take(MAX).collect::<String>()) }
}

fn unset(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty() && v != "-").then(|| v.to_string())
}

fn apply_directive(d: &mut CsvDirectives, layout: &mut Option<CsvLayout>, spacing: &mut Spacing, key: &str, value: &str, w: &mut Warnings) {
    let num = |v: &str, w: &mut Warnings| -> Option<f64> {
        let v = unset(v)?;
        let n = parse_f64(&v);
        if n.is_none() {
            w.push(format!("{key}: '{}' is not a number", clip(&v)));
        }
        n
    };
    match key {
        "label" => d.label = unset(value),
        "style" => d.style = unset(value),
        "stylename" => d.stylename = unset(value),
        "styles" => d.styles = unset(value).map(|v| json_string_map(key, &v, w)).unwrap_or_default(),
        "labelname" => d.labelname = unset(value),
        "labels" => d.labels = unset(value).map(|v| json_string_map(key, &v, w)).unwrap_or_default(),
        "vars" => {
            d.vars = match unset(value).map(|v| serde_json::from_str::<Vec<String>>(&v)) {
                None => Vec::new(),
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    w.push(format!("vars: not a JSON list of strings: {e}"));
                    Vec::new()
                }
            }
        }
        "identity" => d.identity = unset(value),
        "parent" => d.parent = unset(value),
        "parentstyle" => d.parentstyle = unset(value),
        "namespace" => d.namespace = unset(value),
        "connect" => {
            if let Some(v) = unset(value) {
                match parse_connect(&v) {
                    Ok(c) => {
                        if d.connect.len() < limits::MAX_CSV_CONNECTS {
                            d.connect.push(c);
                        } else {
                            w.push("too many connect directives".into());
                        }
                    }
                    Err(e) => w.push(format!("connect: {e}")),
                }
            }
        }
        "width" => d.width = size_spec(key, value, w),
        "height" => d.height = size_spec(key, value, w),
        "padding" => d.padding = num(value, w),
        "ignore" => {
            d.ignore = unset(value).map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
        }
        "link" => d.link = unset(value),
        "left" => d.left = unset(value),
        "top" => d.top = unset(value),
        "nodespacing" => spacing.node = num(value, w),
        "levelspacing" => spacing.level = num(value, w),
        "edgespacing" => spacing.edge = num(value, w),
        "layout" => *layout = unset(value).map(|v| parse_layout(&v)),
        _ => w.push(format!("unknown directive '{}'", clip(key))),
    }
}

fn parse_layout(v: &str) -> CsvLayout {
    match v.to_ascii_lowercase().as_str() {
        "auto" => CsvLayout::Auto,
        "none" => CsvLayout::None,
        "verticaltree" => CsvLayout::VerticalTree,
        "horizontaltree" => CsvLayout::HorizontalTree,
        "verticalflow" => CsvLayout::VerticalFlow,
        "horizontalflow" => CsvLayout::HorizontalFlow,
        "organic" => CsvLayout::Organic,
        "circle" => CsvLayout::Circle,
        "orgchart" => CsvLayout::OrgChart,
        _ => CsvLayout::Other(v.to_string()),
    }
}

fn size_spec(key: &str, value: &str, w: &mut Warnings) -> Option<SizeSpec> {
    let v = unset(value)?;
    if v.eq_ignore_ascii_case("auto") {
        Some(SizeSpec::Auto)
    } else if let Some(col) = v.strip_prefix('@') {
        Some(SizeSpec::Column(col.trim().to_string()))
    } else if let Some(n) = parse_f64(&v) {
        Some(SizeSpec::Fixed(n))
    } else {
        w.push(format!("{key}: '{}' is not a number, auto or @column", clip(&v)));
        None
    }
}

fn size_for(spec: &Option<SizeSpec>, row: &[String], cols: &HashMap<&str, usize>, row_no: usize, what: &str, w: &mut Warnings) -> Option<NodeSize> {
    match spec.as_ref()? {
        SizeSpec::Auto => Some(NodeSize::Auto),
        SizeSpec::Fixed(n) => Some(NodeSize::Fixed(*n)),
        SizeSpec::Column(c) => {
            let raw = cols.get(c.as_str()).and_then(|&i| row.get(i))?;
            let raw = raw.trim();
            if raw.eq_ignore_ascii_case("auto") {
                return Some(NodeSize::Auto);
            }
            let n = parse_f64(raw);
            if n.is_none() && !raw.is_empty() {
                w.push(format!("row {row_no}: {what} '{}' is not a number", clip(raw)));
            }
            n.map(NodeSize::Fixed)
        }
    }
}

fn json_string_map(key: &str, v: &str, w: &mut Warnings) -> Vec<(String, String)> {
    match serde_json::from_str::<serde_json::Value>(v) {
        Ok(serde_json::Value::Object(map)) => map
            .into_iter()
            .filter_map(|(k, val)| match val {
                serde_json::Value::String(s) => Some((k, s)),
                _ => {
                    w.push(format!("{key}: value for '{}' is not a string", clip(&k)));
                    None
                }
            })
            .collect(),
        Ok(_) => {
            w.push(format!("{key}: expected a JSON object"));
            Vec::new()
        }
        Err(e) => {
            w.push(format!("{key}: invalid JSON: {e}"));
            Vec::new()
        }
    }
}

fn parse_connect(v: &str) -> std::result::Result<ConnectSpec, String> {
    let value: serde_json::Value = serde_json::from_str(v).map_err(|e| format!("invalid JSON: {e}"))?;
    let obj = value.as_object().ok_or("expected a JSON object")?;
    let s = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let from = s("from").ok_or("missing \"from\"")?;
    let to = s("to").ok_or("missing \"to\"")?;
    let invert = match obj.get("invert") {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(t)) => t == "true" || t == "1",
        Some(serde_json::Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        _ => false,
    };
    Ok(ConnectSpec { from, to, invert, label: s("label"), style: s("style") })
}

/// Replaces `%column%` with the row's value. A `%name%` that is not a column is left as written.
pub fn resolve_placeholders(template: &str, cols: &HashMap<&str, usize>, row: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(rest.get(..start).unwrap_or(""));
        let after = rest.get(start + 1..).unwrap_or("");
        match after.find('%') {
            Some(end) => {
                let name = after.get(..end).unwrap_or("");
                match cols.get(name).and_then(|&i| row.get(i)) {
                    Some(v) => {
                        out.push_str(v);
                        rest = after.get(end + 1..).unwrap_or("");
                    }
                    None => {
                        // Not a placeholder: keep this `%` and look again from the next one,
                        // which may open a real placeholder.
                        out.push('%');
                        rest = after;
                    }
                }
            }
            None => {
                out.push('%');
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parses an RFC 4180 table: comma separated, fields optionally in double quotes, `""` inside
/// quotes is a quote, and quoted fields may hold commas and line breaks. Blank lines are skipped.
fn parse_table(text: &str, w: &mut Warnings) -> Result<Vec<Vec<String>>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    // Whether the current field began with a quote, so `a"b` outside quotes stays literal.
    let mut field_quoted = false;
    let mut chars = text.chars().peekable();

    let mut total_fields: usize = 0;
    let mut finish_field = |row: &mut Vec<String>, field: &mut String| -> Result<()> {
        if row.len() >= limits::MAX_CSV_COLUMNS {
            return Err(DrawioError::Limit(format!("more than {} CSV columns", limits::MAX_CSV_COLUMNS)));
        }
        total_fields = total_fields.saturating_add(1);
        if total_fields > limits::MAX_CSV_CELLS {
            return Err(DrawioError::Limit(format!("more than {} CSV cells", limits::MAX_CSV_CELLS)));
        }
        row.push(std::mem::take(field));
        Ok(())
    };
    let finish_row = |rows: &mut Vec<Vec<String>>, row: &mut Vec<String>| -> Result<()> {
        let r = std::mem::take(row);
        // A blank line is one empty field.
        if r.len() == 1 && r.first().is_some_and(|f| f.trim().is_empty()) {
            return Ok(());
        }
        if rows.len() > limits::MAX_CSV_ROWS {
            return Err(DrawioError::Limit(format!("more than {} CSV rows", limits::MAX_CSV_ROWS)));
        }
        rows.push(r);
        Ok(())
    };

    while let Some(c) = chars.next() {
        if field.len() > limits::MAX_CSV_FIELD {
            return Err(DrawioError::Limit(format!("CSV field longer than {} bytes", limits::MAX_CSV_FIELD)));
        }
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !field_quoted => {
                in_quotes = true;
                field_quoted = true;
            }
            ',' => {
                finish_field(&mut row, &mut field)?;
                field_quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                finish_field(&mut row, &mut field)?;
                field_quoted = false;
                finish_row(&mut rows, &mut row)?;
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        w.push("unterminated quoted field at end of input".into());
    }
    if !field.is_empty() || !row.is_empty() || field_quoted {
        finish_field(&mut row, &mut field)?;
        finish_row(&mut rows, &mut row)?;
    }
    if let Some(header) = rows.first() {
        let n = header.len();
        // Padding short rows to the header width must not multiply a small file into a huge
        // table, so bound the padded size up front.
        if n.saturating_mul(rows.len()) > limits::MAX_CSV_CELLS {
            return Err(DrawioError::Limit(format!("more than {} CSV cells", limits::MAX_CSV_CELLS)));
        }
        for (i, r) in rows.iter_mut().enumerate().skip(1) {
            if r.len() > n {
                w.push(format!("row {}: {} fields, header has {n}; extra fields ignored", i - 1, r.len()));
                r.truncate(n);
            }
            r.resize(n, String::new());
        }
    }
    Ok(rows)
}
