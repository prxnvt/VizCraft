//! Shared types and text helpers: errors and warnings, preprocessing (front matter, comments,
//! directives, accessibility text), statement splitting, a char-safe cursor, label cleanup and
//! CSS-like property lists.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::limits;

/// Layout direction requested by a diagram or a subgraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Direction {
    /// Top to bottom (`TB` or `TD`), the default.
    #[default]
    TopToBottom,
    /// Bottom to top (`BT`).
    BottomToTop,
    /// Left to right (`LR`).
    LeftToRight,
    /// Right to left (`RL`).
    RightToLeft,
}

impl Direction {
    /// Parse a Mermaid direction keyword (`TB`, `TD`, `BT`, `LR`, `RL`, or the older `v`, `^`,
    /// `>`, `<`).
    pub fn from_keyword(word: &str) -> Option<Direction> {
        match word {
            "v" => return Some(Direction::TopToBottom),
            "^" => return Some(Direction::BottomToTop),
            ">" => return Some(Direction::LeftToRight),
            "<" => return Some(Direction::RightToLeft),
            _ => {}
        }
        match word.to_ascii_uppercase().as_str() {
            "TB" | "TD" => Some(Direction::TopToBottom),
            "BT" => Some(Direction::BottomToTop),
            "LR" => Some(Direction::LeftToRight),
            "RL" => Some(Direction::RightToLeft),
            _ => None,
        }
    }

    /// The canonical Mermaid keyword for this direction.
    pub fn keyword(self) -> &'static str {
        match self {
            Direction::TopToBottom => "TB",
            Direction::BottomToTop => "BT",
            Direction::LeftToRight => "LR",
            Direction::RightToLeft => "RL",
        }
    }
}

/// Title and accessibility text, from front matter and `accTitle` / `accDescr` statements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Meta {
    /// `title:` from the front matter, or a `title` statement where the diagram type has one.
    pub title: Option<String>,
    /// `accTitle: ...`.
    pub acc_title: Option<String>,
    /// `accDescr: ...` or the `accDescr { ... }` block.
    pub acc_descr: Option<String>,
}

/// One CSS-like property from `style`, `classDef` or `linkStyle`, kept as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleProp {
    /// Property name, for example `fill` or `stroke-width`.
    pub key: String,
    /// Property value, for example `#f9f` or `4px`; empty when the property had no `:`.
    pub value: String,
}

impl StyleProp {
    /// A property from a key and a value.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> StyleProp {
        StyleProp { key: key.into(), value: value.into() }
    }
}

/// A named style class from `classDef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Class name; `default` applies to every node without a class in Mermaid.
    pub name: String,
    /// Properties in source order.
    pub props: Vec<StyleProp>,
}

/// An interaction hint from a `click`, `link` or `callback` statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Click {
    /// The node or class it belongs to.
    pub node: String,
    /// The link target URL, when the statement named one.
    pub url: Option<String>,
    /// The link window target such as `_blank`.
    pub target: Option<String>,
    /// Tooltip text.
    pub tooltip: Option<String>,
    /// A JavaScript callback name; kept for reference only, VizCraft never runs it.
    pub callback: Option<String>,
}

/// A statement the parser skipped, with its 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// 1-based line number in the input.
    pub line: usize,
    /// What was skipped and why.
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// A syntax error, an unsupported diagram type or a limit exceeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("line {line}, column {column}: {message}")]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column, counted in characters.
    pub column: usize,
    /// What went wrong.
    pub message: String,
}

impl ParseError {
    /// An error at a position.
    pub fn new(line: usize, column: usize, message: impl Into<String>) -> ParseError {
        ParseError { line, column, message: message.into() }
    }
}

/// Collects warnings and counts statements while one diagram is parsed.
#[derive(Debug, Default)]
pub(crate) struct Ctx {
    warnings: Vec<Warning>,
    statements: usize,
    dropped: usize,
}

impl Ctx {
    /// Record a warning, or count it once the cap is reached.
    pub(crate) fn warn(&mut self, line: usize, message: impl Into<String>) {
        if self.warnings.len() < limits::MAX_WARNINGS {
            self.warnings.push(Warning { line, message: message.into() });
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Count one statement, failing once the statement cap is passed.
    pub(crate) fn statement(&mut self, line: usize) -> Result<(), ParseError> {
        self.statements = self.statements.saturating_add(1);
        if self.statements > limits::MAX_STATEMENTS {
            return Err(ParseError::new(line, 1, format!("more than {} statements", limits::MAX_STATEMENTS)));
        }
        Ok(())
    }

    /// The warnings, with a summary for any that did not fit.
    pub(crate) fn finish(mut self) -> Vec<Warning> {
        if self.dropped > 0 {
            let line = self.warnings.last().map_or(1, |w| w.line);
            self.warnings.push(Warning { line, message: format!("{} more warnings not shown", self.dropped) });
        }
        self.warnings
    }
}

/// Fail when a node count would pass the cap.
pub(crate) fn check_nodes(count: usize, line: usize) -> Result<(), ParseError> {
    if count >= limits::MAX_NODES {
        return Err(ParseError::new(line, 1, format!("more than {} nodes", limits::MAX_NODES)));
    }
    Ok(())
}

/// Fail when an edge count would pass the cap.
pub(crate) fn check_edges(count: usize, line: usize) -> Result<(), ParseError> {
    if count >= limits::MAX_EDGES {
        return Err(ParseError::new(line, 1, format!("more than {} edges", limits::MAX_EDGES)));
    }
    Ok(())
}

/// Fail when opening one more block would pass the nesting cap.
pub(crate) fn check_depth(depth: usize, line: usize, col: usize) -> Result<(), ParseError> {
    if depth >= limits::MAX_DEPTH {
        return Err(ParseError::new(line, col, format!("blocks nested deeper than {}", limits::MAX_DEPTH)));
    }
    Ok(())
}

/// One trimmed, non-empty source line that is not a comment or directive.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SrcLine<'a> {
    /// 1-based line number.
    pub no: usize,
    /// 1-based column where `text` starts.
    pub col: usize,
    /// The trimmed text.
    pub text: &'a str,
}

/// Input after preprocessing: metadata, the header keyword and the body lines.
#[derive(Debug)]
pub(crate) struct Prepared<'a> {
    pub meta: Meta,
    /// First word of the header line, for example `flowchart`.
    pub keyword: &'a str,
    /// Column of the keyword.
    pub header_col: usize,
    /// Rest of the header line after the keyword (may be empty), with its line and column.
    pub header: SrcLine<'a>,
    /// Remaining lines in order.
    pub body: Vec<SrcLine<'a>>,
}

impl<'a> Prepared<'a> {
    /// The header remainder (when not empty) followed by the body lines.
    pub(crate) fn all_lines(&self) -> Vec<SrcLine<'a>> {
        let mut out = Vec::with_capacity(self.body.len().saturating_add(1));
        if !self.header.text.is_empty() {
            out.push(self.header);
        }
        out.extend(self.body.iter().copied());
        out
    }
}

/// A trimmed line with the 1-based column of its first non-blank character.
fn trimmed(raw: &str) -> (usize, &str) {
    let start = raw.trim_start();
    let lead = raw.len().saturating_sub(start.len());
    let col = raw.get(..lead).map_or(0, |s| s.chars().count()).saturating_add(1);
    (col, start.trim_end())
}

/// Strip one pair of surrounding double quotes.
pub(crate) fn unquote(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return t.get(1..t.len().saturating_sub(1)).unwrap_or(t);
    }
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        return t.get(1..t.len().saturating_sub(1)).unwrap_or(t);
    }
    t
}

/// Split off the input's front matter, comments, directives and accessibility statements, and
/// find the header line.
pub(crate) fn prepare(text: &str) -> Result<Prepared<'_>, ParseError> {
    if text.len() > limits::MAX_INPUT_BYTES {
        return Err(ParseError::new(1, 1, format!("input is {} bytes; the limit is {} bytes", text.len(), limits::MAX_INPUT_BYTES)));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut meta = Meta::default();
    let mut lines = text.lines().enumerate().map(|(i, l)| (i.saturating_add(1), l)).peekable();

    // Front matter must come first, so only blank lines may precede it.
    while lines.peek().is_some_and(|(_, l)| l.trim().is_empty()) {
        lines.next();
    }
    if let Some(&(open_no, l)) = lines.peek()
        && l.trim() == "---"
    {
        lines.next();
        let mut closed = false;
        for (_, l) in lines.by_ref() {
            if l.trim() == "---" {
                closed = true;
                break;
            }
            // Only the top-level `title:` key matters; the rest is renderer configuration.
            if let Some(v) = l.strip_prefix("title:") {
                meta.title = Some(unquote(v).to_string());
            }
        }
        if !closed {
            return Err(ParseError::new(open_no, 1, "front matter starting here is never closed with `---`"));
        }
    }

    let mut header = None;
    while let Some((no, raw)) = lines.next() {
        let (col, t) = trimmed(raw);
        if t.is_empty() {
            continue;
        }
        if t.starts_with("%%") {
            skip_directive(t, &mut lines);
            continue;
        }
        header = Some((no, col, t));
        break;
    }
    let Some((header_no, header_col, header_text)) = header else {
        return Err(ParseError::new(1, 1, "no diagram found: the text is empty or holds only comments"));
    };
    let kw_end = header_text.find(|c: char| c.is_whitespace() || c == ';').unwrap_or(header_text.len());
    let keyword = header_text.get(..kw_end).unwrap_or("");
    let after = header_text.get(kw_end..).unwrap_or("");
    let rest = after.trim_start();
    let rest_col =
        header_text.get(..header_text.len().saturating_sub(rest.len())).map_or(header_col, |s| header_col.saturating_add(s.chars().count()));

    let mut body = Vec::new();
    while let Some((no, raw)) = lines.next() {
        let (col, t) = trimmed(raw);
        if t.is_empty() {
            continue;
        }
        if t.starts_with("%%") {
            skip_directive(t, &mut lines);
            continue;
        }
        if let Some(rest) = keyword_rest(t, "accTitle")
            && let Some(v) = rest.strip_prefix(':')
        {
            meta.acc_title = Some(v.trim().to_string());
            continue;
        }
        if let Some(rest) = keyword_rest(t, "accDescr") {
            if let Some(v) = rest.strip_prefix(':') {
                meta.acc_descr = Some(v.trim().to_string());
                continue;
            }
            if let Some(v) = rest.strip_prefix('{') {
                let mut descr = String::new();
                if let Some(end) = v.find('}') {
                    descr.push_str(v.get(..end).unwrap_or("").trim());
                } else {
                    descr.push_str(v.trim());
                    for (_, l) in lines.by_ref() {
                        let (_, lt) = trimmed(l);
                        let (part, done) = match lt.find('}') {
                            Some(end) => (lt.get(..end).unwrap_or(""), true),
                            None => (lt, false),
                        };
                        if !part.trim().is_empty() {
                            if !descr.is_empty() {
                                descr.push('\n');
                            }
                            descr.push_str(part.trim());
                        }
                        if done {
                            break;
                        }
                    }
                }
                meta.acc_descr = Some(descr);
                continue;
            }
        }
        body.push(SrcLine { no, col, text: t });
    }
    Ok(Prepared { meta, keyword, header_col, header: SrcLine { no: header_no, col: rest_col, text: rest.trim_end() }, body })
}

/// The text after `keyword` and optional blanks, when `t` starts with that word.
fn keyword_rest<'a>(t: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = t.strip_prefix(keyword)?;
    let trimmed = rest.trim_start();
    if trimmed.starts_with(':') || trimmed.starts_with('{') { Some(trimmed) } else { None }
}

/// Skip a `%%{ ... }%%` directive that spans several lines; one-line comments need nothing.
fn skip_directive<'a, I: Iterator<Item = (usize, &'a str)>>(t: &str, lines: &mut I) {
    if t.starts_with("%%{") && !t.contains("}%%") {
        for (_, l) in lines.by_ref() {
            if l.contains("}%%") {
                break;
            }
        }
    }
}

/// One statement, with the line and column where it starts.
#[derive(Debug, Clone)]
pub(crate) struct Stmt {
    pub line: usize,
    pub col: usize,
    pub text: String,
}

impl Stmt {
    /// A statement from one source line.
    pub(crate) fn from_line(l: &SrcLine<'_>) -> Stmt {
        Stmt { line: l.line_no(), col: l.col, text: l.text.to_string() }
    }

    /// The 1-based character column of a byte offset into `text`.
    pub(crate) fn column(&self, byte: usize) -> usize {
        let prefix = self.text.get(..byte.min(self.text.len())).unwrap_or("");
        // Columns past a joined line break count from that line's start.
        let after_break = prefix.rsplit('\n').next().unwrap_or(prefix);
        if after_break.len() == prefix.len() {
            self.col.saturating_add(prefix.chars().count())
        } else {
            after_break.chars().count().saturating_add(1)
        }
    }

    /// The 1-based line of a byte offset into `text`, counting joined line breaks.
    pub(crate) fn line_at(&self, byte: usize) -> usize {
        let prefix = self.text.get(..byte.min(self.text.len())).unwrap_or("");
        self.line.saturating_add(prefix.matches('\n').count())
    }
}

impl SrcLine<'_> {
    fn line_no(&self) -> usize {
        self.no
    }
}

/// Whether `buf` ends in an entity code such as `#35` or `#quot`, so a following `;` closes the
/// code rather than the statement.
fn ends_with_entity(buf: &str) -> bool {
    for (count, c) in buf.chars().rev().enumerate() {
        if c == '#' {
            return count > 0;
        }
        if !c.is_ascii_alphanumeric() || count > 16 {
            return false;
        }
    }
    false
}

/// Push the trimmed buffer as a statement when it is not empty.
fn push_stmt(out: &mut Vec<Stmt>, line: usize, col: usize, buf: &mut String) {
    let lead = buf.len().saturating_sub(buf.trim_start().len());
    let lead_chars = buf.get(..lead).map_or(0, |s| s.chars().count());
    let text = buf.trim();
    if !text.is_empty() {
        out.push(Stmt { line, col: col.saturating_add(lead_chars), text: text.to_string() });
    }
    buf.clear();
}

/// Split lines into statements. A double-quoted string may run over line breaks (they are kept
/// as `\n`). With `semicolons`, a `;` outside quotes and brackets also ends a statement, unless
/// it closes an entity code like `#35;`.
pub(crate) fn split_statements(lines: &[SrcLine<'_>], semicolons: bool) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut buf_line = 0usize;
    let mut buf_col = 1usize;
    let mut in_quote = false;
    let mut depth = 0usize;
    for l in lines {
        if in_quote {
            buf.push('\n');
        } else {
            buf_line = l.no;
            buf_col = l.col;
        }
        let mut char_idx = 0usize;
        for c in l.text.chars() {
            char_idx = char_idx.saturating_add(1);
            if c == '"' {
                in_quote = !in_quote;
            }
            if !in_quote {
                match c {
                    '[' | '(' | '{' => depth = depth.saturating_add(1),
                    ']' | ')' | '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if semicolons && c == ';' && !in_quote && depth == 0 && !ends_with_entity(&buf) {
                push_stmt(&mut out, buf_line, buf_col, &mut buf);
                buf_line = l.no;
                buf_col = l.col.saturating_add(char_idx);
                continue;
            }
            buf.push(c);
        }
        if !in_quote {
            push_stmt(&mut out, buf_line, buf_col, &mut buf);
            depth = 0;
        }
    }
    push_stmt(&mut out, buf_line, buf_col, &mut buf);
    out
}

/// How a statement failed: not recognised (a warning) or recognised but broken (an error).
#[derive(Debug)]
pub(crate) enum StmtError {
    /// Not a statement this parser knows; skipped with a warning. Carries the byte offset.
    Unknown(usize, String),
    /// A known construct with a syntax error; aborts the parse. Carries the byte offset.
    Malformed(usize, String),
    /// A hard error raised while applying the statement (limits).
    Fatal(ParseError),
}

impl From<ParseError> for StmtError {
    fn from(e: ParseError) -> StmtError {
        StmtError::Fatal(e)
    }
}

/// Result of parsing or applying one statement.
pub(crate) type StmtResult<T> = Result<T, StmtError>;

/// Turn a statement result into a warning or an error.
pub(crate) fn settle(ctx: &mut Ctx, stmt: &Stmt, r: StmtResult<()>) -> Result<(), ParseError> {
    match r {
        Ok(()) => Ok(()),
        Err(StmtError::Unknown(at, msg)) => {
            ctx.warn(stmt.line_at(at), format!("{msg}; skipped `{}`", snippet(&stmt.text)));
            Ok(())
        }
        Err(StmtError::Malformed(at, msg)) => Err(ParseError::new(stmt.line_at(at), stmt.column(at), msg)),
        Err(StmtError::Fatal(e)) => Err(e),
    }
}

/// A short, single-line excerpt of a statement for messages.
pub(crate) fn snippet(s: &str) -> String {
    let one_line: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).take(60).collect();
    if s.chars().count() > 60 { format!("{one_line}...") } else { one_line }
}

/// A byte cursor over a statement that only ever stops on char boundaries.
#[derive(Debug, Clone)]
pub(crate) struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(src: &'a str) -> Cursor<'a> {
        Cursor { src, pos: 0 }
    }

    /// Current byte offset.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Move back to an offset previously returned by [`Cursor::pos`].
    pub(crate) fn set_pos(&mut self, pos: usize) {
        if self.src.is_char_boundary(pos) {
            self.pos = pos;
        }
    }

    pub(crate) fn rest(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    pub(crate) fn slice(&self, from: usize, to: usize) -> &'a str {
        self.src.get(from..to).unwrap_or("")
    }

    pub(crate) fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub(crate) fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos = self.pos.saturating_add(c.len_utf8());
        Some(c)
    }

    /// Consume `s` when the rest starts with it.
    pub(crate) fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.pos = self.pos.saturating_add(s.len());
            true
        } else {
            false
        }
    }

    pub(crate) fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    /// Advance by `n` bytes of the rest, clamped to a char boundary at or before the end.
    pub(crate) fn advance(&mut self, n: usize) {
        let mut target = self.pos.saturating_add(n).min(self.src.len());
        while target > self.pos && !self.src.is_char_boundary(target) {
            target -= 1;
        }
        self.pos = target;
    }

    pub(crate) fn take_while(&mut self, mut f: impl FnMut(char) -> bool) -> &'a str {
        let start = self.pos;
        while self.peek().is_some_and(&mut f) {
            self.bump();
        }
        self.slice(start, self.pos)
    }

    /// Count and consume a run of `c`.
    pub(crate) fn count_run(&mut self, c: char) -> usize {
        let mut n = 0usize;
        while self.peek() == Some(c) {
            self.bump();
            n = n.saturating_add(1);
        }
        n
    }

    /// Consume a double-quoted string, returning its inner text, or `None` (cursor unchanged)
    /// when no closing quote follows.
    pub(crate) fn quoted(&mut self) -> Option<&'a str> {
        if self.peek() != Some('"') {
            return None;
        }
        let rest = self.rest();
        let inner = rest.get(1..)?;
        let end = inner.find('"')?;
        let text = inner.get(..end)?;
        self.pos = self.pos.saturating_add(end).saturating_add(2);
        Some(text)
    }

    /// Consume a word made of identifier characters. A `-` or `.` joins the word only when a
    /// word character follows it, so `A-->B` stops at `A` while `my-node` stays whole.
    pub(crate) fn word(&mut self, dash: bool, dot: bool) -> &'a str {
        let start = self.pos;
        loop {
            match self.peek() {
                Some(c) if is_word_char(c) => {
                    self.bump();
                }
                Some('-') if dash && self.peek_nth(1).is_some_and(is_word_char) => {
                    self.bump();
                }
                Some('.') if dot && self.peek_nth(1).is_some_and(is_word_char) => {
                    self.bump();
                }
                _ => break,
            }
        }
        self.slice(start, self.pos)
    }
}

/// Letters, digits and underscore in any script.
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// First whitespace-separated word and the trimmed rest.
pub(crate) fn first_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (s.get(..i).unwrap_or(s), s.get(i..).unwrap_or("").trim_start()),
        None => (s, ""),
    }
}

/// Clean a label as Mermaid displays it: drop surrounding quotes and markdown backticks,
/// decode entity codes and turn `<br>` into a line break.
pub(crate) fn clean_label(raw: &str) -> String {
    let t = raw.trim();
    let quoted = t.len() >= 2 && t.starts_with('"') && t.ends_with('"');
    let t = if quoted { t.get(1..t.len().saturating_sub(1)).unwrap_or(t) } else { t };
    let t = if t.len() >= 2 && t.starts_with('`') && t.ends_with('`') { t.get(1..t.len().saturating_sub(1)).unwrap_or(t) } else { t };
    replace_br(&decode_entities(t))
}

/// Replace `<br>`, `<br/>` and `<br />` (any case) with a line break.
fn replace_br(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut c = Cursor::new(s);
    while let Some(ch) = c.peek() {
        if ch == '<' {
            let save = c.pos();
            c.bump();
            let is_br = matches!(c.peek(), Some('b' | 'B')) && matches!(c.peek_nth(1), Some('r' | 'R'));
            if is_br {
                c.bump();
                c.bump();
                c.skip_ws();
                c.eat("/");
                if c.eat(">") {
                    out.push('\n');
                    continue;
                }
            }
            c.set_pos(save);
        }
        out.push(ch);
        c.bump();
    }
    out
}

/// Decode Mermaid entity codes: `#35;` (decimal) and a small set of named ones like `#quot;`.
/// Unknown codes stay as written.
pub(crate) fn decode_entities(s: &str) -> String {
    if !s.contains('#') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('#') {
        out.push_str(rest.get(..i).unwrap_or(""));
        let after = rest.get(i.saturating_add(1)..).unwrap_or("");
        let name_len = after.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(after.len());
        let name = after.get(..name_len).unwrap_or("");
        let decoded = if name_len > 0 && name_len <= 16 && after.get(name_len..).is_some_and(|r| r.starts_with(';')) { entity(name) } else { None };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = after.get(name_len.saturating_add(1)..).unwrap_or("");
            }
            None => {
                out.push('#');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One entity code without `#` and `;`.
fn entity(name: &str) -> Option<char> {
    if name.bytes().all(|b| b.is_ascii_digit()) {
        return name.parse::<u32>().ok().and_then(char::from_u32);
    }
    let c = match name {
        "quot" => '"',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "num" => '#',
        "semi" => ';',
        "colon" => ':',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "deg" => '\u{b0}',
        "hellip" => '\u{2026}',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "larr" => '\u{2190}',
        "uarr" => '\u{2191}',
        "rarr" => '\u{2192}',
        "darr" => '\u{2193}',
        "harr" => '\u{2194}',
        "hearts" => '\u{2665}',
        "infin" => '\u{221e}',
        "ne" => '\u{2260}',
        "le" => '\u{2264}',
        "ge" => '\u{2265}',
        "times" => '\u{d7}',
        "divide" => '\u{f7}',
        _ => return None,
    };
    Some(c)
}

/// Parse `fill:#f9f,stroke:#333,stroke-width:4px` into ordered properties. `\,` keeps a comma
/// inside a value; a trailing `;` is dropped.
pub(crate) fn parse_style_props(s: &str) -> Vec<StyleProp> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<StyleProp>| {
        let part = cur.trim().trim_end_matches(';').trim();
        if !part.is_empty() {
            match part.split_once(':') {
                Some((k, v)) => out.push(StyleProp::new(k.trim(), v.trim())),
                None => out.push(StyleProp::new(part, "")),
            }
        }
        cur.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&',') => {
                cur.push(',');
                chars.next();
            }
            ',' => flush(&mut cur, &mut out),
            _ => cur.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// A token from [`tokens`]: its text and whether it was double-quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub text: String,
    pub quoted: bool,
}

/// Split on whitespace, keeping double-quoted strings (without their quotes) as one token.
pub(crate) fn tokens(s: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut c = Cursor::new(s);
    loop {
        c.skip_ws();
        if c.is_eof() {
            break;
        }
        if let Some(q) = c.quoted() {
            out.push(Token { text: q.to_string(), quoted: true });
            continue;
        }
        let w = c.take_while(|ch| !ch.is_whitespace());
        if w.is_empty() {
            // A lone quote with no closing partner: take it as a plain token.
            if let Some(ch) = c.bump() {
                out.push(Token { text: ch.to_string(), quoted: false });
            }
            continue;
        }
        out.push(Token { text: w.to_string(), quoted: false });
    }
    out
}

/// Add `name` to `list` unless it is already there.
pub(crate) fn push_unique(list: &mut Vec<String>, name: &str) {
    if !name.is_empty() && !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

/// Merge a `classDef` into a list, replacing the properties of an earlier one with that name.
pub(crate) fn add_class_def(defs: &mut Vec<ClassDef>, names: &str, props: &str) {
    let props = parse_style_props(props);
    for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match defs.iter_mut().find(|d| d.name == name) {
            Some(d) => d.props.clone_from(&props),
            None => defs.push(ClassDef { name: name.to_string(), props: props.clone() }),
        }
    }
}

/// Parse a `click` statement (flowchart and class diagrams) after the keyword. Forms:
/// `id "url" ["tooltip"] [target]`, `id href "url" ["tooltip"] [target]`,
/// `id callback ["tooltip"]` and `id call callback(args) ["tooltip"]`.
pub(crate) fn parse_click(rest: &str) -> Option<Click> {
    let toks = tokens(rest);
    let mut it = toks.into_iter();
    let node = it.next()?.text;
    let mut click = Click { node, url: None, target: None, tooltip: None, callback: None };
    let next = it.next()?;
    let mut remaining: Vec<Token> = it.collect();
    match (next.quoted, next.text.as_str()) {
        (false, "href") => {
            if remaining.is_empty() {
                return None;
            }
            click.url = Some(remaining.remove(0).text);
        }
        (false, "call") => {
            if remaining.is_empty() {
                return None;
            }
            click.callback = Some(remaining.remove(0).text);
        }
        (true, url) => click.url = Some(url.to_string()),
        (false, name) => click.callback = Some(name.to_string()),
    }
    for t in remaining {
        if t.quoted && click.tooltip.is_none() {
            click.tooltip = Some(t.text);
        } else if !t.quoted && t.text.starts_with('_') {
            click.target = Some(t.text);
        }
    }
    Some(click)
}
