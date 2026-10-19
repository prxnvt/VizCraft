//! DBML (Database Markup Language), from its public documentation at dbml.dbdiagram.io: read into
//! the ER IR ([`crate::er::Schema`]) and drawn as ER shapes, and written back canonically.
//!
//! Read: `Project` (noted as a warning), `Table` with a schema, an alias and settings, columns
//! with their types (arguments and quoted types included) and settings (`pk`, `primary key`,
//! `null`, `not null`, `unique`, `increment`, `default:`, `note:` and inline `ref:`), table
//! `Note`, `indexes { }` (kept as written; a composite `[pk]` index marks its columns as the
//! primary key), short and long `Ref` with composite columns, the four relationship operators
//! and `[delete:, update:]`, `Enum`, `TableGroup`, `//` and `/* */` comments, `'...'` and
//! `'''...'''` strings with escapes. Sticky notes, `TablePartial` and anything else unknown are
//! skipped with a warning.
//!
//! Relationships run from the referenced table (the parent) to the table holding the foreign
//! key (the child). DBML says nothing about the child end's optionality or whether a relationship
//! identifies: the parent end is `one`, or `zeroOrOne` when a foreign key column may be null; the
//! child end is `zeroOrMore` (`zeroOrOne` for `-`, both `zeroOrMore` for `<>`); a relationship is
//! identifying when its foreign key columns are all in the child's primary key.
//!
//! Write is canonical: enums, tables, relationships, then groups, in model order, two-space
//! indent, column settings in the order `pk`, `increment`, `not null`, `unique`, `default`,
//! `note`. A single-column primary key is written on the column, a composite one as an index.
//! Every relationship is written as a short `Ref`; one with no foreign key columns cannot be and
//! is left out with a warning.

use std::fmt::Write as _;

use crate::er::{self, Cardinality, Column, Enum, Group, Ref, Schema, Table, cut};
use crate::{FormatError, Imported};

/// Largest DBML text read, in bytes.
pub const MAX_DBML: usize = 2 * 1024 * 1024;

/// Most tokens read (a 2 MB file holds far fewer).
const MAX_TOKENS: usize = 1_000_000;

/// A token.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A word or a quoted name (`users`, `"full name"`).
    Word(String, bool),
    /// A `'...'` or `'''...'''` string, unescaped.
    Str(String),
    /// A `` `...` `` expression, without the backticks.
    Expr(String),
    /// A number as written.
    Num(String),
    /// A `#rrggbb` colour.
    Color(String),
    /// Punctuation: `{ } [ ] ( ) : , . < > - <> ~`.
    P(&'static str),
    /// A line break (settings and statements end at them).
    Nl,
}

/// Split `text` into tokens; an unterminated string or comment ends the text.
fn lex(text: &str, warnings: &mut Vec<String>) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.char_indices().peekable();
    let rest = |i: usize| text.get(i..).unwrap_or_default();
    while let Some((i, c)) = it.next() {
        if out.len() >= MAX_TOKENS {
            warnings.push("the text holds too many tokens; the rest was not read".into());
            break;
        }
        match c {
            '\n' => out.push(Tok::Nl),
            c if c.is_whitespace() => {}
            '/' if rest(i).starts_with("//") => {
                while it.peek().is_some_and(|(_, c)| *c != '\n') {
                    it.next();
                }
            }
            '/' if rest(i).starts_with("/*") => {
                it.next();
                let mut prev = ' ';
                for (_, c) in it.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            '\'' => {
                let triple = rest(i).starts_with("'''");
                if triple {
                    it.next();
                    it.next();
                }
                let mut s = String::new();
                let mut closed = false;
                while let Some((j, c)) = it.next() {
                    match c {
                        '\\' => match it.next().map(|(_, c)| c) {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            // A backslash at a line end joins the lines.
                            Some('\n') => {}
                            Some(other) => s.push(other),
                            None => {}
                        },
                        '\'' if !triple => {
                            closed = true;
                            break;
                        }
                        '\'' if rest(j).starts_with("'''") => {
                            it.next();
                            it.next();
                            closed = true;
                            break;
                        }
                        c => s.push(c),
                    }
                }
                if !closed {
                    warnings.push("a string is not closed".into());
                }
                out.push(Tok::Str(if triple { dedent(&s) } else { s }));
            }
            '`' => {
                let mut s = String::new();
                for (_, c) in it.by_ref() {
                    if c == '`' {
                        break;
                    }
                    s.push(c);
                }
                out.push(Tok::Expr(s));
            }
            '"' => {
                let mut s = String::new();
                while let Some((_, c)) = it.next() {
                    match c {
                        '\\' => {
                            if let Some((_, c)) = it.next() {
                                s.push(c);
                            }
                        }
                        '"' => break,
                        c => s.push(c),
                    }
                }
                out.push(Tok::Word(s, true));
            }
            '#' => {
                let mut s = String::from("#");
                while let Some(&(_, c)) = it.peek().filter(|(_, c)| c.is_ascii_alphanumeric()) {
                    s.push(c);
                    it.next();
                }
                out.push(Tok::Color(s));
            }
            '<' if rest(i).starts_with("<>") => {
                it.next();
                out.push(Tok::P("<>"));
            }
            '{' | '}' | '[' | ']' | '(' | ')' | ':' | ',' | '.' | '<' | '>' | '~' => out.push(Tok::P(punct(c))),
            '-' if !it.peek().is_some_and(|(_, c)| c.is_ascii_digit()) => out.push(Tok::P("-")),
            c if c.is_ascii_digit() || c == '-' => {
                let mut s = String::from(c);
                while let Some(&(_, c)) = it.peek().filter(|(_, c)| c.is_ascii_digit() || *c == '.') {
                    s.push(c);
                    it.next();
                }
                out.push(Tok::Num(s));
            }
            c if c.is_alphanumeric() || c == '_' => {
                let mut s = String::from(c);
                while let Some(&(_, c)) = it.peek().filter(|(_, c)| c.is_alphanumeric() || *c == '_') {
                    s.push(c);
                    it.next();
                }
                out.push(Tok::Word(s, false));
            }
            _ => {}
        }
    }
    out
}

fn punct(c: char) -> &'static str {
    match c {
        '{' => "{",
        '}' => "}",
        '[' => "[",
        ']' => "]",
        '(' => "(",
        ')' => ")",
        ':' => ":",
        ',' => ",",
        '.' => ".",
        '<' => "<",
        '>' => ">",
        _ => "~",
    }
}

/// A `'''` string without its first and last blank lines and the indent its lines share.
fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(start, |e| e + 1);
    let body = lines.get(start..end).unwrap_or_default();
    let indent = body.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    body.iter().map(|l| l.get(indent..).unwrap_or(l.trim_start())).collect::<Vec<_>>().join("\n")
}

/// The parser: a cursor over the tokens.
struct P<'a> {
    toks: &'a [Tok],
    at: usize,
    warnings: Vec<String>,
}

/// A column named in a relationship: `[schema.]table.column` or `table.(a, b)`.
struct Endpoint {
    table: String,
    columns: Vec<String>,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.at)
    }
    fn next(&mut self) -> Option<&'a Tok> {
        let t = self.toks.get(self.at);
        self.at = self.at.saturating_add(1);
        t
    }
    fn skip_nl(&mut self) {
        while self.peek() == Some(&Tok::Nl) {
            self.at += 1;
        }
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::P(x)) if *x == p)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    /// The next word (bare or quoted), skipping nothing.
    fn word(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w, _)) => {
                self.at += 1;
                Some(cut(w))
            }
            _ => None,
        }
    }
    /// Skip to the end of the line or a closing `}` on it, which is left for the block.
    fn skip_rest(&mut self) {
        while let Some(t) = self.peek() {
            if *t == Tok::Nl || *t == Tok::P("}") {
                break;
            }
            self.at += 1;
        }
    }
    /// Skip to the end of the line.
    fn skip_line(&mut self) {
        while let Some(t) = self.peek() {
            if *t == Tok::Nl {
                break;
            }
            self.at += 1;
        }
    }
    /// Skip a `{ ... }` block (nested), the cursor on or before its `{`.
    fn skip_block(&mut self) {
        while let Some(t) = self.peek() {
            if *t == Tok::P("{") {
                break;
            }
            if *t == Tok::Nl {
                self.at += 1;
                if !self.is("{") {
                    return;
                }
                break;
            }
            self.at += 1;
        }
        let mut depth = 0usize;
        while let Some(t) = self.next() {
            match t {
                Tok::P("{") => depth += 1,
                Tok::P("}") => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
    /// A dotted name `a.b.c` as its parts (a trailing `(` list is left for the caller).
    fn dotted(&mut self) -> Vec<String> {
        let mut parts = Vec::new();
        while let Some(w) = self.word() {
            parts.push(w);
            if !(self.is(".") && matches!(self.toks.get(self.at + 1), Some(Tok::Word(..)))) {
                break;
            }
            self.at += 1;
        }
        parts
    }
    /// `[schema.]table.column` or `[schema.]table.(a, b)`.
    fn endpoint(&mut self) -> Option<Endpoint> {
        let mut parts = self.dotted();
        if self.eat(".") && self.eat("(") {
            let mut columns = Vec::new();
            while let Some(w) = self.word() {
                columns.push(w);
                if !self.eat(",") {
                    break;
                }
            }
            if !self.eat(")") {
                return None;
            }
            let table = parts.pop()?;
            return Some(Endpoint { table, columns });
        }
        let column = parts.pop()?;
        let table = parts.pop()?;
        Some(Endpoint { table, columns: vec![column] })
    }
    /// A setting value: text up to `,` or `]`, strings unquoted (`set null`, `'note'`).
    fn value(&mut self) -> (String, bool) {
        let mut out = String::new();
        let mut quoted = false;
        while let Some(t) = self.peek() {
            match t {
                Tok::P(",") | Tok::P("]") | Tok::Nl => break,
                Tok::Str(s) => {
                    quoted = true;
                    out.push_str(s);
                }
                Tok::Word(w, _) | Tok::Num(w) | Tok::Color(w) => {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(w);
                }
                Tok::Expr(e) => out.push_str(&format!("`{e}`")),
                Tok::P(p) => out.push_str(p),
            }
            self.at += 1;
        }
        (cut(&out), quoted)
    }
    /// A `[ ... ]` settings list as (key, value, raw token text of the value) entries; the cursor
    /// on its `[`.
    fn settings(&mut self) -> Vec<(String, Option<(String, bool)>)> {
        let mut out = Vec::new();
        if !self.eat("[") {
            return out;
        }
        loop {
            while self.peek() == Some(&Tok::Nl) {
                self.at += 1;
            }
            if self.eat("]") || self.peek().is_none() {
                break;
            }
            // A key is one or more words (`primary key`, `not null`).
            let mut key = String::new();
            while let Some(Tok::Word(w, _)) = self.peek() {
                if !key.is_empty() {
                    key.push(' ');
                }
                key.push_str(&w.to_ascii_lowercase());
                self.at += 1;
            }
            let value = if self.eat(":") { Some(self.default_or_value(&key)) } else { None };
            if key.is_empty() && value.is_none() {
                // Something unreadable: skip one token.
                self.at += 1;
            } else {
                out.push((key, value));
            }
            if !self.eat(",") && !self.is("]") && self.peek().is_some() {
                self.at += 1;
            }
        }
        out
    }
    /// The value of a setting; `default:` and `ref:` keep their tokens as written.
    fn default_or_value(&mut self, key: &str) -> (String, bool) {
        match key {
            "default" => {
                let v = match self.next() {
                    Some(Tok::Str(s)) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
                    Some(Tok::Expr(e)) => format!("`{e}`"),
                    Some(Tok::Num(n)) => n.clone(),
                    Some(Tok::Word(w, _)) => w.clone(),
                    _ => String::new(),
                };
                (cut(&v), false)
            }
            "ref" => {
                let start = self.at;
                while !matches!(self.peek(), None | Some(Tok::P(",")) | Some(Tok::P("]")) | Some(Tok::Nl)) {
                    self.at += 1;
                }
                (format!("@{start}:{}", self.at), false)
            }
            _ => self.value(),
        }
    }
}

/// The relationship a DBML operator draws between `left` and `right`: (parent, child,
/// many-to-many).
fn sides(op: &str, left: Endpoint, right: Endpoint) -> Option<(Endpoint, Endpoint, bool)> {
    match op {
        ">" | "-" => Some((right, left, false)),
        "<" => Some((left, right, false)),
        "<>" => Some((left, right, true)),
        _ => None,
    }
}

/// A relationship before table names are resolved.
struct RawRef {
    name: Option<String>,
    op: &'static str,
    parent: Endpoint,
    child: Endpoint,
    many: bool,
    on_delete: Option<String>,
    on_update: Option<String>,
}

/// Read DBML text into a schema and warnings.
pub fn read(text: &str) -> Result<(Schema, Vec<String>), FormatError> {
    if text.len() > MAX_DBML {
        return Err(FormatError::TooLarge { len: text.len(), max: MAX_DBML });
    }
    let mut warnings = Vec::new();
    let toks = lex(text.trim_start_matches('\u{feff}'), &mut warnings);
    let mut p = P { toks: &toks, at: 0, warnings };
    let mut schema = Schema::default();
    let mut raw: Vec<RawRef> = Vec::new();
    let mut composite_pk: Vec<(usize, Vec<String>)> = Vec::new();
    loop {
        p.skip_nl();
        let Some(t) = p.next() else { break };
        let Tok::Word(kw, false) = t else {
            p.warnings.push("a statement does not start with a keyword; it was skipped".into());
            p.skip_line();
            continue;
        };
        match kw.to_ascii_lowercase().as_str() {
            "table" => table(&mut p, &mut schema, &mut raw, &mut composite_pk),
            "ref" => {
                if let Some(r) = reference(&mut p) {
                    raw.extend(r);
                }
            }
            "enum" => enumeration(&mut p, &mut schema),
            "tablegroup" => group(&mut p, &mut schema),
            "project" => {
                p.warnings.push("the Project block is not drawn".into());
                p.skip_block();
            }
            "note" => {
                p.warnings.push("sticky notes are not drawn".into());
                p.skip_block();
            }
            "tablepartial" => {
                p.warnings.push("TablePartial is not supported; it was skipped".into());
                p.skip_block();
            }
            other => {
                p.warnings.push(format!(
                    "`{}` is not a DBML statement this reader knows; it was skipped",
                    er::cut(other).chars().take(40).collect::<String>()
                ));
                p.skip_block();
            }
        }
    }
    let mut warnings = p.warnings;
    for (ti, cols) in composite_pk {
        if let Some(t) = schema.tables.get_mut(ti) {
            for c in &mut t.columns {
                if cols.contains(&c.name) {
                    c.pk = true;
                    c.nullable = false;
                }
            }
        }
    }
    resolve(&mut schema, raw, &mut warnings);
    Ok((schema, warnings))
}

/// Read a table; the cursor after `Table`.
fn table(p: &mut P<'_>, schema: &mut Schema, raw: &mut Vec<RawRef>, composite_pk: &mut Vec<(usize, Vec<String>)>) {
    let mut name = p.dotted();
    let Some(n) = name.pop() else {
        p.warnings.push("a table has no name; it was skipped".into());
        p.skip_block();
        return;
    };
    let mut t = Table { name: n, schema: name.pop(), ..Table::default() };
    if matches!(p.peek(), Some(Tok::Word(w, false)) if w.eq_ignore_ascii_case("as")) {
        p.at += 1;
        t.alias = p.word();
    }
    for (k, v) in p.settings() {
        match (k.as_str(), v) {
            ("note", Some((v, _))) => t.note = Some(v),
            ("headercolor", _) => {}
            (k, _) => p.warnings.push(format!("table {}: setting `{k}` is not kept", t.name)),
        }
    }
    p.skip_nl();
    if !p.eat("{") {
        p.warnings.push(format!("table {} has no body; it was skipped", t.name));
        return;
    }
    let ti = schema.tables.len();
    loop {
        p.skip_nl();
        match p.peek() {
            None => break,
            Some(Tok::P("}")) => {
                p.at += 1;
                break;
            }
            Some(Tok::P("~")) => {
                p.warnings.push(format!("table {}: partial injections are not supported", t.name));
                p.at += 1;
                p.skip_rest();
            }
            Some(Tok::Word(w, false)) if w.eq_ignore_ascii_case("note") && matches!(p.toks.get(p.at + 1), Some(Tok::P(":") | Tok::P("{"))) => {
                p.at += 1;
                if p.eat(":") {
                    if let Some(Tok::Str(s)) = p.next() {
                        t.note = Some(cut(s));
                    }
                } else {
                    p.at += 1;
                    p.skip_nl();
                    if let Some(Tok::Str(s)) = p.peek() {
                        t.note = Some(cut(s));
                        p.at += 1;
                    }
                    p.skip_nl();
                    p.eat("}");
                }
            }
            Some(Tok::Word(w, false)) if w.eq_ignore_ascii_case("indexes") && matches!(p.toks.get(p.at + 1), Some(Tok::P("{")) | Some(Tok::Nl)) => {
                p.at += 1;
                indexes(p, &mut t, ti, composite_pk);
            }
            Some(Tok::Word(..)) => {
                if let Some(c) = column(p, &t.name, raw) {
                    if t.columns.len() < er::MAX_COLUMNS {
                        t.columns.push(c);
                    } else if t.columns.len() == er::MAX_COLUMNS {
                        p.warnings.push(format!("table {} has more than {} columns; the rest were left out", t.name, er::MAX_COLUMNS));
                    }
                }
            }
            Some(_) => {
                p.at += 1;
                p.skip_rest();
            }
        }
    }
    if schema.tables.len() >= er::MAX_TABLES {
        if schema.tables.len() == er::MAX_TABLES {
            p.warnings.push(format!("more than {} tables; the rest were left out", er::MAX_TABLES));
        }
        return;
    }
    schema.tables.push(t);
}

/// Read one column line; the cursor on its name.
fn column(p: &mut P<'_>, table: &str, raw: &mut Vec<RawRef>) -> Option<Column> {
    let name = p.word()?;
    let mut c = Column { name, ..Column::default() };
    // The type: a word (possibly quoted or schema-qualified) and an optional argument list.
    let mut ty = String::new();
    if let Some(Tok::Word(w, quoted)) = p.peek() {
        ty.push_str(w);
        let quoted = *quoted;
        p.at += 1;
        while p.is(".") && matches!(p.toks.get(p.at + 1), Some(Tok::Word(..))) {
            p.at += 1;
            if let Some(w) = p.word() {
                ty.push('.');
                ty.push_str(&w);
            }
        }
        if quoted && ty.contains(char::is_whitespace) {
            ty = format!("\"{ty}\"");
        }
    }
    if p.eat("(") {
        ty.push('(');
        let mut first = true;
        while let Some(t) = p.peek() {
            match t {
                Tok::P(")") => {
                    p.at += 1;
                    break;
                }
                Tok::P(",") => ty.push(','),
                Tok::Num(n) | Tok::Word(n, _) => {
                    if !first && !ty.ends_with(['(', ',']) {
                        ty.push(' ');
                    }
                    ty.push_str(n);
                }
                Tok::Str(s) => ty.push_str(&format!("'{s}'")),
                Tok::Nl => break,
                _ => {}
            }
            first = false;
            p.at += 1;
        }
        ty.push(')');
    }
    if p.is("[") && p.toks.get(p.at + 1) == Some(&Tok::P("]")) {
        p.at += 2;
        ty.push_str("[]");
    }
    c.data_type = cut(&ty);
    for (k, v) in p.settings() {
        match (k.as_str(), v) {
            ("pk" | "primary key", _) => c.pk = true,
            ("null", _) => c.nullable = true,
            ("not null", _) => c.nullable = false,
            ("unique", _) => c.unique = true,
            ("increment", _) => c.increment = true,
            ("default", Some((v, _))) => c.default = Some(v),
            ("note", Some((v, _))) => c.note = Some(v),
            ("ref", Some((span, _))) => {
                if let Some(r) = inline_ref(p, &span, table, &c.name) {
                    raw.push(r);
                }
            }
            (k, _) => p.warnings.push(format!("column {table}.{}: setting `{k}` is not kept", c.name)),
        }
    }
    // A primary key is never null.
    if c.pk {
        c.nullable = false;
    }
    p.skip_rest();
    Some(c)
}

/// An inline `ref: > table.column` of column `column` of `table`, from the token span the
/// settings reader kept (`@start:end`).
fn inline_ref(p: &mut P<'_>, span: &str, table: &str, column: &str) -> Option<RawRef> {
    let (a, b) = span.strip_prefix('@')?.split_once(':')?;
    let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
    let toks = p.toks.get(a..b)?;
    let mut sub = P { toks, at: 0, warnings: Vec::new() };
    let op = match sub.next() {
        Some(Tok::P(op @ ("<" | ">" | "-" | "<>"))) => *op,
        _ => {
            p.warnings.push(format!("column {table}.{column}: the inline ref does not read"));
            return None;
        }
    };
    let right = sub.endpoint()?;
    let left = Endpoint { table: table.to_string(), columns: vec![column.to_string()] };
    let (parent, child, many) = sides(op, left, right)?;
    Some(RawRef { name: None, op, parent, child, many, on_delete: None, on_update: None })
}

/// Read `indexes { ... }`; the cursor after `indexes`.
fn indexes(p: &mut P<'_>, t: &mut Table, ti: usize, composite_pk: &mut Vec<(usize, Vec<String>)>) {
    p.skip_nl();
    if !p.eat("{") {
        return;
    }
    loop {
        p.skip_nl();
        match p.peek() {
            None => break,
            Some(Tok::P("}")) => {
                p.at += 1;
                break;
            }
            Some(_) => {
                let start = p.at;
                p.skip_line();
                let line = render(p.toks.get(start..p.at).unwrap_or_default());
                // A composite primary key marks its columns; the writer makes the line again.
                let toks = p.toks.get(start..p.at).unwrap_or_default();
                let is_pk = toks.windows(3).any(|w| {
                    w.first() == Some(&Tok::P("["))
                        && matches!(w.get(1), Some(Tok::Word(k, false)) if k.eq_ignore_ascii_case("pk"))
                        && w.get(2) == Some(&Tok::P("]"))
                });
                if is_pk && toks.first() == Some(&Tok::P("(")) {
                    let cols: Vec<String> = toks
                        .iter()
                        .take_while(|t| **t != Tok::P(")"))
                        .filter_map(|t| if let Tok::Word(w, _) = t { Some(w.clone()) } else { None })
                        .collect();
                    composite_pk.push((ti, cols));
                } else if !line.is_empty() && t.indexes.len() < 256 {
                    t.indexes.push(cut(&line));
                }
            }
        }
    }
}

/// Tokens written back as DBML text with canonical spacing.
fn render(toks: &[Tok]) -> String {
    let mut s = String::new();
    for t in toks {
        let piece = match t {
            Tok::Word(w, true) => format!("\"{w}\""),
            Tok::Word(w, false) | Tok::Num(w) | Tok::Color(w) => w.clone(),
            Tok::Str(x) => quote(x),
            Tok::Expr(e) => format!("`{e}`"),
            Tok::P(p) => (*p).to_string(),
            Tok::Nl => continue,
        };
        let glue_left = matches!(t, Tok::P(")" | "]" | "," | ":" | "."));
        let after_open = s.ends_with(['(', '[', '.']);
        if !s.is_empty() && !glue_left && !after_open {
            s.push(' ');
        }
        s.push_str(&piece);
    }
    s
}

/// Read a `Ref` (short or long form); the cursor after `Ref`.
fn reference(p: &mut P<'_>) -> Option<Vec<RawRef>> {
    let name = match p.peek() {
        Some(Tok::Word(..)) if matches!(p.toks.get(p.at + 1), Some(Tok::P(":") | Tok::P("{")) | Some(Tok::Nl)) => p.word(),
        _ => None,
    };
    let mut out = Vec::new();
    if p.eat(":") {
        out.extend(one_ref(p, name));
    } else {
        p.skip_nl();
        if !p.eat("{") {
            p.warnings.push("a Ref does not read; it was skipped".into());
            p.skip_line();
            return None;
        }
        loop {
            p.skip_nl();
            match p.peek() {
                None => break,
                Some(Tok::P("}")) => {
                    p.at += 1;
                    break;
                }
                Some(_) => {
                    out.extend(one_ref(p, name.clone()));
                    p.skip_rest();
                }
            }
        }
    }
    Some(out)
}

/// One relationship line `a.x op b.y [settings]`.
fn one_ref(p: &mut P<'_>, name: Option<String>) -> Option<RawRef> {
    let left = p.endpoint();
    let op = match p.next() {
        Some(Tok::P(op @ ("<" | ">" | "-" | "<>"))) => *op,
        _ => {
            p.warnings.push("a relationship has no operator (<, >, - or <>); it was skipped".into());
            p.skip_line();
            return None;
        }
    };
    let right = p.endpoint();
    let (Some(left), Some(right)) = (left, right) else {
        p.warnings.push("a relationship names no columns; it was skipped".into());
        p.skip_line();
        return None;
    };
    let (parent, child, many) = sides(op, left, right)?;
    let mut r = RawRef { name, op, parent, child, many, on_delete: None, on_update: None };
    for (k, v) in p.settings() {
        match (k.as_str(), v) {
            ("delete", Some((v, _))) => r.on_delete = Some(v.to_ascii_lowercase()),
            ("update", Some((v, _))) => r.on_update = Some(v.to_ascii_lowercase()),
            (k, _) => p.warnings.push(format!("relationship setting `{k}` is not kept")),
        }
    }
    if r.parent.columns.len() != r.child.columns.len() {
        p.warnings.push(format!("a relationship from {} to {} pairs unequal column lists; it was skipped", r.child.table, r.parent.table));
        return None;
    }
    Some(r)
}

/// Read an enum; the cursor after `Enum`.
fn enumeration(p: &mut P<'_>, schema: &mut Schema) {
    let mut name = p.dotted();
    let Some(n) = name.pop() else {
        p.skip_block();
        return;
    };
    let mut e = Enum { name: n, schema: name.pop(), values: Vec::new() };
    p.skip_nl();
    if !p.eat("{") {
        return;
    }
    loop {
        p.skip_nl();
        match p.peek() {
            None => break,
            Some(Tok::P("}")) => {
                p.at += 1;
                break;
            }
            Some(Tok::Word(..)) => {
                let Some(v) = p.word() else { break };
                let mut note = None;
                for (k, val) in p.settings() {
                    if let ("note", Some((val, _))) = (k.as_str(), val) {
                        note = Some(val);
                    }
                }
                if e.values.len() < er::MAX_VALUES {
                    e.values.push((v, note));
                }
                p.skip_rest();
            }
            Some(_) => {
                p.at += 1;
                p.skip_rest();
            }
        }
    }
    if schema.enums.len() < er::MAX_ENUMS {
        schema.enums.push(e);
    }
}

/// Read a table group; the cursor after `TableGroup`.
fn group(p: &mut P<'_>, schema: &mut Schema) {
    let Some(name) = p.word() else {
        p.skip_block();
        return;
    };
    let mut g = Group { name, tables: Vec::new() };
    let _ = p.settings();
    p.skip_nl();
    if !p.eat("{") {
        return;
    }
    loop {
        p.skip_nl();
        match p.peek() {
            None => break,
            Some(Tok::P("}")) => {
                p.at += 1;
                break;
            }
            Some(Tok::Word(w, false)) if w.eq_ignore_ascii_case("note") => p.skip_rest(),
            Some(Tok::Word(..)) => {
                let mut parts = p.dotted();
                if let Some(t) = parts.pop()
                    && g.tables.len() < er::MAX_TABLES
                {
                    g.tables.push(t);
                }
                p.skip_rest();
            }
            Some(_) => {
                p.at += 1;
                p.skip_rest();
            }
        }
    }
    schema.groups.push(g);
}

/// Turn raw relationships into schema relationships: table aliases resolved, foreign key columns
/// marked, cardinalities and the identifying flag derived.
fn resolve(schema: &mut Schema, raw: Vec<RawRef>, warnings: &mut Vec<String>) {
    let find = |schema: &Schema, name: &str| -> Option<usize> {
        schema.tables.iter().position(|t| t.name == name).or_else(|| schema.tables.iter().position(|t| t.alias.as_deref() == Some(name)))
    };
    for r in raw.into_iter().take(er::MAX_REFS) {
        let (Some(pi), Some(ci)) = (find(schema, &r.parent.table), find(schema, &r.child.table)) else {
            warnings.push(format!("a relationship names {} or {}, which is not a table; it was skipped", r.parent.table, r.child.table));
            continue;
        };
        let parent_name = schema.tables.get(pi).map(|t| t.name.clone()).unwrap_or_default();
        let child_name = schema.tables.get(ci).map(|t| t.name.clone()).unwrap_or_default();
        let mut nullable = false;
        if !r.many
            && let Some(child) = schema.tables.get_mut(ci)
        {
            for (cc, pc) in r.child.columns.iter().zip(&r.parent.columns) {
                match child.columns.iter_mut().find(|c| &c.name == cc) {
                    Some(c) => {
                        c.fk = true;
                        c.references = Some(format!("{parent_name}.{pc}"));
                        nullable |= c.nullable;
                    }
                    None => warnings.push(format!("table {child_name} has no column {cc}")),
                }
            }
        }
        let identifying = !r.many && schema.tables.get(ci).is_some_and(|c| er::all_in_pk(c, &r.child.columns));
        let (source, target) = match (r.many, r.op) {
            (true, _) => (Cardinality::ZeroOrMore, Cardinality::ZeroOrMore),
            (false, "-") => (if nullable { Cardinality::ZeroOrOne } else { Cardinality::One }, Cardinality::ZeroOrOne),
            _ => (if nullable { Cardinality::ZeroOrOne } else { Cardinality::One }, Cardinality::ZeroOrMore),
        };
        schema.refs.push(Ref {
            name: r.name,
            parent: parent_name,
            parent_columns: r.parent.columns,
            child: child_name,
            child_columns: r.child.columns,
            source,
            target,
            identifying,
            on_delete: r.on_delete,
            on_update: r.on_update,
        });
    }
}

/// Read DBML text as one page of ER shapes ready for a merging import.
pub fn import(text: &str) -> Result<Imported, FormatError> {
    let (schema, mut warnings) = read(text)?;
    if schema.tables.is_empty() && schema.enums.is_empty() {
        return Err(FormatError::Import("the DBML text holds no table or enum".into()));
    }
    let page = er::to_page(&schema, &mut warnings);
    Ok(Imported { pages: vec![page], warnings })
}

// ------------------------------------------------------------------------------------ write

/// Whether `w` can be written bare.
fn bare(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || c == '_') && !w.starts_with(|c: char| c.is_ascii_digit())
}

/// A name, double-quoted when it is not a bare word.
fn name(w: &str) -> String {
    if bare(w) { w.to_string() } else { format!("\"{}\"", w.replace('\\', "\\\\").replace('"', "\\\"")) }
}

/// A string literal: `'...'`, or `'''...'''` for text with line breaks.
fn quote(s: &str) -> String {
    if s.contains('\n') {
        format!("'''\n{}\n'''", s.replace('\\', "\\\\").replace("'''", "\\'''"))
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// `schema.name`, or the name.
fn qualified(schema: &Option<String>, n: &str) -> String {
    match schema {
        Some(s) if !s.is_empty() => format!("{}.{}", name(s), name(n)),
        _ => name(n),
    }
}

/// Write `schema` as canonical DBML. The warnings say what was left out.
pub fn write(schema: &Schema) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut warnings = Vec::new();
    for e in &schema.enums {
        let _ = writeln!(out, "Enum {} {{", qualified(&e.schema, &e.name));
        for (v, note) in &e.values {
            match note {
                Some(n) => {
                    let _ = writeln!(out, "  {} [note: {}]", name(v), quote(n));
                }
                None => {
                    let _ = writeln!(out, "  {}", name(v));
                }
            }
        }
        out.push_str("}\n\n");
    }
    for t in &schema.tables {
        let mut head = format!("Table {}", qualified(&t.schema, &t.name));
        if let Some(a) = &t.alias {
            let _ = write!(head, " as {}", name(a));
        }
        let _ = writeln!(out, "{head} {{");
        let pks = t.columns.iter().filter(|c| c.pk).count();
        for c in &t.columns {
            let mut settings: Vec<String> = Vec::new();
            if c.pk && pks == 1 {
                settings.push("pk".into());
            }
            if c.increment {
                settings.push("increment".into());
            }
            if !c.nullable && !(c.pk && pks == 1) {
                settings.push("not null".into());
            }
            if c.unique {
                settings.push("unique".into());
            }
            if let Some(d) = &c.default {
                settings.push(format!("default: {d}"));
            }
            if let Some(n) = &c.note {
                settings.push(format!("note: {}", quote(n)));
            }
            let ty = if c.data_type.is_empty() { "text".to_string() } else { c.data_type.clone() };
            let s = if settings.is_empty() { String::new() } else { format!(" [{}]", settings.join(", ")) };
            let _ = writeln!(out, "  {} {ty}{s}", name(&c.name));
        }
        let mut idx: Vec<String> = Vec::new();
        if pks > 1 {
            let cols: Vec<String> = t.columns.iter().filter(|c| c.pk).map(|c| name(&c.name)).collect();
            idx.push(format!("({}) [pk]", cols.join(", ")));
        }
        idx.extend(t.indexes.iter().cloned());
        if !idx.is_empty() {
            out.push_str("\n  indexes {\n");
            for i in &idx {
                let _ = writeln!(out, "    {i}");
            }
            out.push_str("  }\n");
        }
        if let Some(n) = &t.note {
            let _ = writeln!(out, "\n  Note: {}", quote(n));
        }
        out.push_str("}\n\n");
    }
    for r in &schema.refs {
        if r.child_columns.is_empty() || r.child_columns.len() != r.parent_columns.len() {
            warnings.push(format!("the relationship from {} to {} has no foreign key columns, so DBML cannot write it", r.parent, r.child));
            continue;
        }
        let cols = |t: &str, cs: &[String]| {
            let tn = schema.table(t).map(|x| qualified(&x.schema, &x.name)).unwrap_or_else(|| name(t));
            if cs.len() == 1 {
                format!("{tn}.{}", cs.first().map(|c| name(c)).unwrap_or_default())
            } else {
                format!("{tn}.({})", cs.iter().map(|c| name(c)).collect::<Vec<_>>().join(", "))
            }
        };
        let op = if r.source.many() && r.target.many() {
            "<>"
        } else if !r.target.many() {
            "-"
        } else {
            ">"
        };
        let mut settings = Vec::new();
        if let Some(d) = &r.on_delete {
            settings.push(format!("delete: {d}"));
        }
        if let Some(u) = &r.on_update {
            settings.push(format!("update: {u}"));
        }
        let s = if settings.is_empty() { String::new() } else { format!(" [{}]", settings.join(", ")) };
        let label = r.name.as_deref().filter(|n| bare(n)).map(|n| format!(" {n}")).unwrap_or_default();
        if r.name.as_deref().is_some_and(|n| !bare(n)) {
            warnings.push(format!("the relationship name {:?} is not a DBML name; it was left out", r.name.as_deref().unwrap_or_default()));
        }
        // A many-to-many relationship reads left to right as parent to child; the others put the
        // foreign key holder on the left.
        let (left, right) = if op == "<>" {
            (cols(&r.parent, &r.parent_columns), cols(&r.child, &r.child_columns))
        } else {
            (cols(&r.child, &r.child_columns), cols(&r.parent, &r.parent_columns))
        };
        let _ = writeln!(out, "Ref{label}: {left} {op} {right}{s}");
    }
    if !schema.refs.is_empty() {
        out.push('\n');
    }
    for g in &schema.groups {
        let _ = writeln!(out, "TableGroup {} {{", name(&g.name));
        for t in &g.tables {
            let tn = schema.table(t).map(|x| qualified(&x.schema, &x.name)).unwrap_or_else(|| name(t));
            let _ = writeln!(out, "  {tn}");
        }
        out.push_str("}\n\n");
    }
    let trimmed = out.trim_end().to_string();
    (if trimmed.is_empty() { String::new() } else { trimmed + "\n" }, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/dbml");
        let src = std::fs::read_to_string(dir.join(format!("{name}.dbml"))).unwrap();
        let (schema, _) = read(&src).unwrap();
        let (out, _) = write(&schema);
        let path = dir.join(format!("{name}.golden.dbml"));
        if std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, &out).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap();
        assert_eq!(out, want, "{name}: write(read(x)) differs from the golden");
        let (again, _) = write(&read(&want).unwrap().0);
        assert_eq!(again, want, "{name}: the golden is not a fixpoint");
    }

    #[test]
    fn shop_round_trips_to_its_golden() {
        golden("shop");
    }

    #[test]
    fn composite_keys_round_trip_to_their_golden() {
        golden("composite_keys");
    }

    #[test]
    fn enums_and_groups_round_trip_to_their_golden() {
        golden("enums_and_groups");
    }

    #[test]
    fn relationships_read_with_cardinality_identifying_and_actions() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int [not null, ref: > users.id]\n}\nTable likes {\n  user_id int [pk]\n  post_id int [pk]\n}\nRef: likes.user_id > users.id [delete: cascade]\nRef { posts.id < likes.post_id }\n";
        let (s, w) = read(src).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(s.refs.len(), 3);
        let posts = &s.refs[0];
        assert_eq!(
            (posts.parent.as_str(), posts.child.as_str(), posts.source, posts.target),
            ("users", "posts", Cardinality::One, Cardinality::ZeroOrMore)
        );
        assert!(!posts.identifying);
        let likes = &s.refs[1];
        assert!(likes.identifying, "the foreign key is in the child's primary key");
        assert_eq!(likes.on_delete.as_deref(), Some("cascade"));
        assert_eq!((s.refs[2].parent.as_str(), s.refs[2].child.as_str()), ("posts", "likes"));
        let col = s.tables[1].columns.iter().find(|c| c.name == "user_id").unwrap();
        assert!(col.fk && !col.nullable && col.references.as_deref() == Some("users.id"));
    }

    #[test]
    fn unsupported_parts_become_warnings() {
        let src = "Project shop { database_type: 'PostgreSQL' }\nNote sticky { 'hello' }\nTablePartial base { id int }\nTable a { id int [pk] }\nwhatever x\n";
        let (s, w) = read(src).unwrap();
        assert_eq!(s.tables.len(), 1);
        assert!(w.len() >= 4, "{w:?}");
    }

    #[test]
    fn the_reader_never_panics_on_truncated_or_garbage_text() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/dbml");
        let src = std::fs::read_to_string(dir.join("shop.dbml")).unwrap();
        for end in (0..src.len()).filter(|i| src.is_char_boundary(*i)) {
            let _ = read(src.get(..end).unwrap_or_default());
        }
        for junk in [
            "'''",
            "\"",
            "`",
            "/*",
            "Table",
            "Table {",
            "Ref:",
            "Ref: a.(",
            "Ref: a.b >",
            "Enum {",
            "[[[",
            "Table t { c int [ref: > ] }",
            "\u{0}\u{ffff}é",
            "Table t { c x(((( }",
        ] {
            let _ = read(junk);
            let _ = import(junk);
        }
        let mut seed = 7u64;
        for _ in 0..300 {
            let s: String = (0..200)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    b"Table Ref Enum {}[]():,.<>-'\"`/*\n abc 123 pk"[(seed >> 33) as usize % 44] as char
                })
                .collect();
            let _ = read(&s);
        }
        assert!(matches!(read(&"x".repeat(MAX_DBML + 1)), Err(FormatError::TooLarge { .. })));
    }
}
