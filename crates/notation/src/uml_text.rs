//! Member rows and multiplicities as UML 2.5.1 writes them (OMG formal/17-12-05, 9.5.4 for
//! properties, 9.6.4 for operations, 7.5.4 for multiplicities).
//!
//! An attribute row is `[visibility] name [: type] [[multiplicity]] [= default] [{modifiers}]`;
//! an operation row is `[visibility] name(params) [: return] [{modifiers}]`. Static and abstract
//! members carry a leading `{static}` or `{abstract}` until rows can carry markup (underline and
//! italics). Reading never fails: anything unexpected stays in the name, so a row written by
//! hand comes back as it was.

/// Longest row read, in bytes; the rest is dropped.
pub const MAX_ROW: usize = 1024;

/// Longest multiplicity read, in bytes.
pub const MAX_MULTIPLICITY: usize = 64;

/// A member's visibility (UML 2.5.1, 7.4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// `+`, `public`.
    Public,
    /// `-`, `private`.
    Private,
    /// `#`, `protected`.
    Protected,
    /// `~`, `package`.
    Package,
}

impl Visibility {
    /// The symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Visibility::Public => "+",
            Visibility::Private => "-",
            Visibility::Protected => "#",
            Visibility::Package => "~",
        }
    }
    /// The keyword.
    pub fn word(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Package => "package",
        }
    }
    fn from_symbol(c: char) -> Option<Self> {
        match c {
            '+' => Some(Visibility::Public),
            '-' => Some(Visibility::Private),
            '#' => Some(Visibility::Protected),
            '~' => Some(Visibility::Package),
            _ => None,
        }
    }
    fn from_word(w: &str) -> Option<Self> {
        [Visibility::Public, Visibility::Private, Visibility::Protected, Visibility::Package].into_iter().find(|v| v.word() == w)
    }
}

/// How rows show visibility: the page variant's `memberVisibility` option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shown {
    /// `+name` (the default).
    #[default]
    Symbols,
    /// `public name`.
    Keywords,
    /// No visibility.
    Hidden,
}

impl Shown {
    /// The style an option value names; anything else is [`Shown::Symbols`].
    pub fn from_option(v: &str) -> Self {
        match v {
            "keywords" => Shown::Keywords,
            "hidden" => Shown::Hidden,
            _ => Shown::Symbols,
        }
    }
}

/// Attribute or operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemberKind {
    /// A property row.
    #[default]
    Attribute,
    /// An operation row (it has a parameter list).
    Operation,
}

/// One member row, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    /// Visibility, when the row gives one.
    pub visibility: Option<Visibility>,
    /// The name (everything unexpected stays here).
    pub name: String,
    /// The type of an attribute; empty for none.
    pub ty: String,
    /// The multiplicity text between the brackets; empty for none.
    pub multiplicity: String,
    /// The default value of an attribute; empty for none.
    pub default: String,
    /// The parameter list of an operation, as written.
    pub params: String,
    /// The return type of an operation; empty for none.
    pub ret: String,
    /// Modifiers: leading `static` and `abstract`, then trailing ones (`readOnly`).
    pub modifiers: Vec<String>,
    /// Attribute or operation.
    pub kind: MemberKind,
}

impl Member {
    /// Whether the row is static.
    pub fn is_static(&self) -> bool {
        self.modifiers.iter().any(|m| m == "static")
    }
    /// Whether the row is abstract.
    pub fn is_abstract(&self) -> bool {
        self.modifiers.iter().any(|m| m == "abstract")
    }
}

/// `t` cut to at most `max` bytes at a character boundary.
fn capped(t: &str, max: usize) -> &str {
    if t.len() <= max {
        return t;
    }
    let mut end = max;
    while end > 0 && !t.is_char_boundary(end) {
        end -= 1;
    }
    t.get(..end).unwrap_or_default()
}

/// Leading modifiers: `static` and `abstract` are written first.
fn is_leading(m: &str) -> bool {
    m == "static" || m == "abstract"
}

/// The byte index of the `)` closing the `(` at `open`, or `None`.
fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Trailing `{a, b}` modifiers off the end of `rest`.
fn trailing_modifiers(rest: &str, out: &mut Vec<String>) -> String {
    let t = rest.trim_end();
    if t.ends_with('}')
        && let Some(open) = t.rfind('{')
    {
        let inner = t.get(open + 1..t.len() - 1).unwrap_or_default();
        out.extend(inner.split(',').map(str::trim).filter(|m| !m.is_empty()).map(str::to_string));
        return t.get(..open).unwrap_or_default().trim_end().to_string();
    }
    t.to_string()
}

/// Read one member row (see the module doc). Never fails and never panics.
pub fn parse_member(row: &str) -> Member {
    let mut m = Member::default();
    let mut rest = capped(row, MAX_ROW).trim();
    // Leading {static} and {abstract}.
    while let Some(after) = rest.strip_prefix('{') {
        let Some(close) = after.find('}') else { break };
        let word = after.get(..close).unwrap_or_default().trim();
        if !is_leading(word) {
            break;
        }
        m.modifiers.push(word.to_string());
        rest = after.get(close + 1..).unwrap_or_default().trim_start();
    }
    if let Some(v) = rest.chars().next().and_then(Visibility::from_symbol) {
        m.visibility = Some(v);
        rest = rest.get(1..).unwrap_or_default().trim_start();
    } else if let Some((w, after)) = rest.split_once(' ')
        && let Some(v) = Visibility::from_word(w)
    {
        m.visibility = Some(v);
        rest = after.trim_start();
    }
    let paren = rest.find('(');
    let stop = rest.find([':', '=', '[', '{']);
    if let Some(open) = paren.filter(|p| stop.is_none_or(|s| *p < s))
        && let Some(close) = closing(rest, open)
    {
        m.kind = MemberKind::Operation;
        m.name = rest.get(..open).unwrap_or_default().trim().to_string();
        m.params = rest.get(open + 1..close).unwrap_or_default().trim().to_string();
        let mut mods = Vec::new();
        let after = trailing_modifiers(rest.get(close + 1..).unwrap_or_default(), &mut mods);
        m.modifiers.extend(mods);
        if let Some(r) = after.trim().strip_prefix(':') {
            m.ret = r.trim().to_string();
        } else if !after.trim().is_empty() {
            // Not a return type: keep it in the name so nothing is lost.
            m.name = format!("{}({}){}", m.name, m.params, after);
            m.params.clear();
            m.kind = MemberKind::Attribute;
        }
        return m;
    }
    let mut mods = Vec::new();
    let body = trailing_modifiers(rest, &mut mods);
    m.modifiers.extend(mods);
    let (head, default) = match body.split_once('=') {
        Some((h, d)) => (h.trim_end().to_string(), d.trim().to_string()),
        None => (body.clone(), String::new()),
    };
    m.default = default;
    let (head, mult) = match (head.rfind('['), head.trim_end().ends_with(']')) {
        (Some(open), true) => {
            let t = head.trim_end();
            (t.get(..open).unwrap_or_default().trim_end().to_string(), t.get(open + 1..t.len() - 1).unwrap_or_default().trim().to_string())
        }
        _ => (head, String::new()),
    };
    m.multiplicity = mult;
    match head.split_once(':') {
        Some((n, t)) => {
            m.name = n.trim().to_string();
            m.ty = t.trim().to_string();
        }
        None => m.name = head.trim().to_string(),
    }
    m
}

/// Write a member row in `shown` style; [`parse_member`] reads it back as the same member
/// (with `shown` other than hidden).
pub fn format_member(m: &Member, shown: Shown) -> String {
    let mut out = String::new();
    for lead in m.modifiers.iter().filter(|x| is_leading(x)) {
        out.push('{');
        out.push_str(lead);
        out.push_str("} ");
    }
    if let Some(v) = m.visibility {
        match shown {
            Shown::Symbols => out.push_str(v.symbol()),
            Shown::Keywords => {
                out.push_str(v.word());
                out.push(' ');
            }
            Shown::Hidden => {}
        }
    }
    out.push_str(&m.name);
    match m.kind {
        MemberKind::Operation => {
            out.push('(');
            out.push_str(&m.params);
            out.push(')');
            if !m.ret.is_empty() {
                out.push_str(": ");
                out.push_str(&m.ret);
            }
        }
        MemberKind::Attribute => {
            if !m.ty.is_empty() {
                out.push_str(": ");
                out.push_str(&m.ty);
            }
            if !m.multiplicity.is_empty() {
                out.push_str(" [");
                out.push_str(&m.multiplicity);
                out.push(']');
            }
            if !m.default.is_empty() {
                out.push_str(" = ");
                out.push_str(&m.default);
            }
        }
    }
    let trailing: Vec<&str> = m.modifiers.iter().filter(|x| !is_leading(x)).map(String::as_str).collect();
    if !trailing.is_empty() {
        out.push_str(" {");
        out.push_str(&trailing.join(", "));
        out.push('}');
    }
    out
}

/// Read a multiplicity: `n`, `*`, `n..m` or `n..*`, with n at most m. Returns the lower bound
/// and the upper (`None` for unlimited), or why it does not read.
pub fn parse_multiplicity(text: &str) -> Result<(u32, Option<u32>), String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("it is empty".into());
    }
    if t.len() > MAX_MULTIPLICITY {
        return Err(format!("it is longer than {MAX_MULTIPLICITY} bytes"));
    }
    let bound = |b: &str| -> Result<u32, String> {
        let b = b.trim();
        if b.is_empty() || !b.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("`{b}` is not a whole number of zero or more"));
        }
        b.parse::<u32>().map_err(|_| format!("`{b}` is too large"))
    };
    if t == "*" {
        return Ok((0, None));
    }
    match t.split_once("..") {
        None => bound(t).map(|n| (n, Some(n))),
        Some((lo, hi)) => {
            let lo = bound(lo)?;
            if hi.trim() == "*" {
                return Ok((lo, None));
            }
            let hi = bound(hi)?;
            if lo > hi {
                return Err(format!("the lower bound {lo} is more than the upper bound {hi}"));
            }
            Ok((lo, Some(hi)))
        }
    }
}

/// The non-empty member rows of a property's text.
pub fn rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [&str; 20] = [
        "+name: String",
        "-id: Integer",
        "#items: Item [0..*]",
        "~count: Integer = 0",
        "+total: Money [1] = 0 {readOnly}",
        "{static} +instances: Integer = 0",
        "+getTotal(): Money",
        "-recalc()",
        "+add(item: Item, qty: Integer): Boolean",
        "{abstract} +area(): Real",
        "{static} {abstract} +make(kind: String): Shape",
        "name",
        "+tags: String [*] {ordered, unique}",
        "#parent: Node [0..1]",
        "+lookup(key: String): Value [0..1]",
        "-cache: Map<String, Value>",
        "+size(): Integer {query}",
        "~owner: Party [1..*]",
        "+RED",
        "+pair(a: (Integer, Integer)): Integer",
    ];

    #[test]
    fn twenty_rows_round_trip() {
        for row in ROWS {
            let m = parse_member(row);
            assert_eq!(format_member(&m, Shown::Symbols), row, "{m:?}");
            let words = format_member(&m, Shown::Keywords);
            assert_eq!(parse_member(&words), m, "{words}");
        }
        let m = parse_member("{static} +add(item: Item): Boolean");
        assert_eq!(
            (m.kind, m.name.as_str(), m.params.as_str(), m.ret.as_str(), m.is_static()),
            (MemberKind::Operation, "add", "item: Item", "Boolean", true)
        );
        let a = parse_member("public items: Item [0..*] = none");
        assert_eq!((a.visibility, a.ty.as_str(), a.multiplicity.as_str(), a.default.as_str()), (Some(Visibility::Public), "Item", "0..*", "none"));
        assert_eq!(format_member(&a, Shown::Keywords), "public items: Item [0..*] = none");
        assert_eq!(format_member(&a, Shown::Hidden), "items: Item [0..*] = none");
    }

    #[test]
    fn multiplicities_good_and_bad() {
        for (t, want) in
            [("1", (1, Some(1))), ("*", (0, None)), ("0..1", (0, Some(1))), ("1..*", (1, None)), (" 2..5 ", (2, Some(5))), ("0..0", (0, Some(0)))]
        {
            assert_eq!(parse_multiplicity(t), Ok(want), "{t}");
        }
        for t in ["5..2", "..", "-1", "", "a", "1..", "..3", "1...2", "*..1", "99999999999", "1..-1", "١"] {
            assert!(parse_multiplicity(t).is_err(), "{t}");
        }
        assert!(parse_multiplicity(&"1".repeat(65)).is_err());
    }

    #[test]
    fn hostile_rows_never_panic() {
        let long = "é".repeat(2000);
        for row in [
            "",
            "(",
            ")",
            "((((",
            "+(",
            "{",
            "{static",
            "{static}",
            "}{",
            ":",
            "=",
            "[",
            "]",
            "[]",
            "a:[",
            "x(y",
            "+é(ü)",
            "{abstract}{static}",
            "a]b[c",
            long.as_str(),
            "\u{0}\u{1}",
            "+ +",
            "public",
            "public  ",
            "x)(",
            "=[]{}()",
        ] {
            let m = parse_member(row);
            let _ = format_member(&m, Shown::Keywords);
            assert!(m.name.len() <= MAX_ROW);
        }
    }
}
