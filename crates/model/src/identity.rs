//! The identity registry: tags and reference designations across the whole model.
//!
//! A notation pack declares identifier scopes ([`IdScope`]): which element types share a tag
//! space, the format their tags follow, whether a tag must be unique, whether tags come in pairs
//! (an off-page reference and its partner), and whether new elements are numbered automatically.
//! The [`Registry`] reads the model through those scopes. It never changes the model; commands
//! write the tags it hands out and the rules engine turns its findings into issues.
//!
//! Format grammar ([`IdFormat`]): literal text, `{n}` for the auto number, `{n:3}` for the auto
//! number zero-padded to three digits, `{key}` for the element's property `key` (letters, digits
//! and `_`), and `{{` or `}}` for a literal brace. So `S{n}` gives `S1`, `S2`; `{letters}-{n:3}`
//! gives `FIC-101` for an element whose `letters` property is `FIC`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::serde_util::{lenient, lenient_vec};
use crate::{Element, Model, ModelError};

/// Longest format string accepted.
pub const MAX_FORMAT: usize = 256;

/// Widest zero padding a `{n:W}` part may ask for.
pub const MAX_PAD: usize = 12;

/// An identifier scope as a pack declares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IdScope {
    /// Pack-qualified scope id (`flowchart.step`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Pack-qualified element types whose tags live in this scope.
    #[serde(deserialize_with = "lenient_vec")]
    pub types: Vec<String>,
    /// The tag format (see the module docs).
    #[serde(deserialize_with = "lenient")]
    pub format: String,
    /// Two elements of the scope may not share a tag.
    #[serde(deserialize_with = "lenient")]
    pub unique: bool,
    /// Tags come in pairs: each tag should be used by exactly two elements (off-page references).
    #[serde(deserialize_with = "lenient")]
    pub paired: bool,
    /// New elements of these types get the next number in the format.
    #[serde(deserialize_with = "lenient")]
    pub auto_number: bool,
}

impl Default for IdScope {
    fn default() -> Self {
        IdScope { id: String::new(), types: Vec::new(), format: "{n}".into(), unique: true, paired: false, auto_number: false }
    }
}

/// One piece of a parsed format.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    /// Literal text.
    Lit(String),
    /// The auto number, zero-padded to this width (0: no padding).
    Number(usize),
    /// An element property, by key.
    Prop(String),
}

/// A parsed identifier format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdFormat {
    source: String,
    parts: Vec<Part>,
}

impl IdFormat {
    /// Parse a format. Refused when it is longer than [`MAX_FORMAT`], has an unclosed or empty
    /// brace, a pad wider than [`MAX_PAD`], or more than one `{n}`.
    pub fn parse(source: &str) -> Result<IdFormat, ModelError> {
        if source.len() > MAX_FORMAT {
            return Err(ModelError::Format(format!("longer than {MAX_FORMAT} bytes")));
        }
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    lit.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    lit.push('}');
                }
                '}' => return Err(ModelError::Format(format!("unmatched `}}` in `{source}`"))),
                '{' => {
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(ch) => name.push(ch),
                            None => return Err(ModelError::Format(format!("unclosed `{{` in `{source}`"))),
                        }
                    }
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(Self::placeholder(&name, source)?);
                }
                _ => lit.push(c),
            }
        }
        if !lit.is_empty() {
            parts.push(Part::Lit(lit));
        }
        if parts.iter().filter(|p| matches!(p, Part::Number(_))).count() > 1 {
            return Err(ModelError::Format(format!("more than one `{{n}}` in `{source}`")));
        }
        Ok(IdFormat { source: source.to_string(), parts })
    }
    /// One `{...}` part: `n`, `n:W` or a property key.
    fn placeholder(name: &str, source: &str) -> Result<Part, ModelError> {
        if name == "n" {
            return Ok(Part::Number(0));
        }
        if let Some(w) = name.strip_prefix("n:") {
            let w: usize = w.parse().map_err(|_| ModelError::Format(format!("bad pad `{name}` in `{source}`")))?;
            if w > MAX_PAD {
                return Err(ModelError::Format(format!("pad wider than {MAX_PAD} in `{source}`")));
            }
            return Ok(Part::Number(w));
        }
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(ModelError::Format(format!("bad part `{{{name}}}` in `{source}`")));
        }
        Ok(Part::Prop(name.to_string()))
    }
    /// The format as written.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Whether the format has an `{n}` part.
    pub fn has_number(&self) -> bool {
        self.parts.iter().any(|p| matches!(p, Part::Number(_)))
    }
    /// The tag for `element` with auto number `n`; a missing property composes as empty text.
    pub fn compose(&self, element: &Element, n: u64) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                Part::Lit(t) => t.clone(),
                Part::Number(w) => format!("{n:0w$}", w = *w),
                Part::Prop(k) => element.prop(k).map(|v| v.display()).unwrap_or_default(),
            })
            .collect()
    }
    /// The auto number inside `tag` when `tag` matches this format for `element` (literal and
    /// property parts equal, a run of digits where `{n}` is), else `None`. A `{n:W}` pad is a
    /// minimum: a run of at least W digits matches, so `S{n:3}` reads `S1000` as 1000 but not
    /// `S7`. A format without `{n}` never yields a number. Numbers too large for `u64` saturate.
    pub fn number_in(&self, tag: &str, element: &Element) -> Option<u64> {
        let at = self.parts.iter().position(|p| matches!(p, Part::Number(_)))?;
        let width = match self.parts.get(at) {
            Some(Part::Number(w)) => *w,
            _ => return None,
        };
        // Only one `{n}` is allowed, so everything around it composes to fixed text and the
        // match is a prefix, a suffix and digits in between: no backtracking.
        let fixed = |parts: &[Part]| -> String {
            parts
                .iter()
                .map(|p| match p {
                    Part::Lit(t) => t.clone(),
                    Part::Prop(k) => element.prop(k).map(|v| v.display()).unwrap_or_default(),
                    Part::Number(_) => String::new(),
                })
                .collect()
        };
        let prefix = fixed(self.parts.get(..at).unwrap_or_default());
        let suffix = fixed(self.parts.get(at.saturating_add(1)..).unwrap_or_default());
        let digits = tag.strip_prefix(prefix.as_str())?.strip_suffix(suffix.as_str())?;
        if digits.is_empty() || digits.len() < width || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(digits.bytes().fold(0u64, |n, b| n.saturating_mul(10).saturating_add(u64::from(b - b'0'))))
    }
}

/// A tag used by more elements than its scope allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    /// Scope id.
    pub scope: String,
    /// The tag.
    pub tag: String,
    /// Element ids using it, in model order.
    pub elements: Vec<String>,
}

/// A paired tag that only one element uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmatched {
    /// Scope id.
    pub scope: String,
    /// The tag.
    pub tag: String,
    /// The element with no partner.
    pub element: String,
}

/// Read-only view of the model's tags through a set of scopes.
pub struct Registry<'a> {
    model: &'a Model,
    scopes: &'a [IdScope],
}

impl<'a> Registry<'a> {
    /// A registry over `model` with these scopes (from every pack the model uses).
    pub fn new(model: &'a Model, scopes: &'a [IdScope]) -> Self {
        Registry { model, scopes }
    }
    /// The first scope that lists `type_id`.
    pub fn scope_for(&self, type_id: &str) -> Option<&'a IdScope> {
        self.scopes.iter().find(|s| s.types.iter().any(|t| t == type_id))
    }
    /// Elements whose type is in `scope`, in model order.
    pub fn members(&self, scope: &IdScope) -> Vec<&'a Element> {
        self.model.elements.iter().filter(|e| scope.types.contains(&e.type_id)).collect()
    }
    /// The next tag for `element` when its scope numbers automatically: one more than the highest
    /// number found in the scope's existing tags ([`IdFormat::number_in`]), composed with the
    /// element's properties, skipping any composed tag already taken. `Ok(None)` when the type
    /// has no auto-numbered scope; an error when the scope's format does not parse.
    ///
    /// Each member's tag is read with that member's own properties, so a format such as
    /// `{letters}-{n:3}` shares one counter across every letter code in the scope. A member with
    /// the same id as `element` is ignored, so retagging an element does not count its own tag.
    /// An auto-numbered format without `{n}` is a format error, and a scope whose numbers have
    /// reached `u64::MAX` reports [`ModelError::TooMany`].
    pub fn next_tag(&self, element: &Element) -> Result<Option<String>, ModelError> {
        let Some(scope) = self.scope_for(&element.type_id) else { return Ok(None) };
        if !scope.auto_number {
            return Ok(None);
        }
        let format = IdFormat::parse(&scope.format)?;
        if !format.has_number() {
            return Err(ModelError::Format(format!("auto-numbered scope `{}` has no `{{n}}` in `{}`", scope.id, scope.format)));
        }
        let others: Vec<&Element> = self.members(scope).into_iter().filter(|m| m.id != element.id || element.id.is_empty()).collect();
        let highest = others.iter().filter_map(|m| format.number_in(&m.tag, m)).max().unwrap_or(0);
        let taken: HashSet<&str> = others.iter().map(|m| m.tag.as_str()).filter(|t| !t.is_empty()).collect();
        // At most `taken.len()` candidates can be taken, so this many tries always finds a free
        // one unless the numbers run out.
        let mut n = highest;
        for _ in 0..=taken.len() {
            n = n.checked_add(1).ok_or(ModelError::TooMany("tag numbers"))?;
            let tag = format.compose(element, n);
            if !taken.contains(tag.as_str()) {
                return Ok(Some(tag));
            }
        }
        Err(ModelError::TooMany("tag numbers"))
    }
    /// Whether `tag` may be given to an element of `type_id` (`except`: the element being
    /// retagged, ignored when counting): in a unique scope no other member has it, in a paired
    /// scope fewer than two others have it. Always true for a type with no scope or an empty tag.
    /// A scope that is both paired and unique counts as paired; one that is neither allows any tag.
    pub fn is_free(&self, type_id: &str, tag: &str, except: Option<&str>) -> bool {
        let Some(scope) = self.scope_for(type_id) else { return true };
        if tag.is_empty() {
            return true;
        }
        let Some(limit) = Self::limit(scope) else { return true };
        let users = self.members(scope).into_iter().filter(|m| m.tag == tag && Some(m.id.as_str()) != except).count();
        users < limit
    }
    /// How many members of `scope` may share one tag, or `None` when there is no limit.
    fn limit(scope: &IdScope) -> Option<usize> {
        if scope.paired {
            Some(2)
        } else if scope.unique {
            Some(1)
        } else {
            None
        }
    }
    /// Member element ids of `scope` grouped by non-empty tag, tags sorted, ids in model order.
    fn by_tag(&self, scope: &IdScope) -> BTreeMap<&'a str, Vec<String>> {
        let mut map: BTreeMap<&'a str, Vec<String>> = BTreeMap::new();
        for m in self.members(scope) {
            if !m.tag.is_empty() {
                map.entry(m.tag.as_str()).or_default().push(m.id.clone());
            }
        }
        map
    }
    /// The scopes sorted by id (stable, so scopes declared twice keep their declared order).
    fn sorted_scopes(&self) -> Vec<&'a IdScope> {
        let mut scopes: Vec<&'a IdScope> = self.scopes.iter().collect();
        scopes.sort_by(|a, b| a.id.cmp(&b.id));
        scopes
    }
    /// Tags used too often: by more than one member of a unique scope, or more than two of a
    /// paired scope. Empty tags are ignored. Sorted by scope, then tag.
    pub fn duplicates(&self) -> Vec<Duplicate> {
        let mut out = Vec::new();
        for scope in self.sorted_scopes() {
            let Some(limit) = Self::limit(scope) else { continue };
            for (tag, elements) in self.by_tag(scope) {
                if elements.len() > limit {
                    out.push(Duplicate { scope: scope.id.clone(), tag: tag.to_string(), elements });
                }
            }
        }
        out
    }
    /// Tags of paired scopes used by exactly one member. Sorted by scope, then tag.
    pub fn unmatched(&self) -> Vec<Unmatched> {
        let mut out = Vec::new();
        for scope in self.sorted_scopes().into_iter().filter(|s| s.paired) {
            for (tag, mut elements) in self.by_tag(scope) {
                if elements.len() == 1
                    && let Some(element) = elements.pop()
                {
                    out.push(Unmatched { scope: scope.id.clone(), tag: tag.to_string(), element });
                }
            }
        }
        out
    }
}
