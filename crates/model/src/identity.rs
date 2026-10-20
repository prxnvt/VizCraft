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
//!
//! Composed parts read the element's context: `{owner.tag}` is the direct owner's tag and
//! `{owner:TYPE.tag}` the tag of the nearest ancestor of type `TYPE` (both through the model the
//! [`Registry`] reads); `{page}` (or `{page:W}`, zero-padded) is the sheet number of the element's
//! first view, `{zone}` the zone of that view and `{rung}` (or `{rung:W}`) its rung, all from a
//! [`ComposeCtx`] the caller supplies. So `{owner.tag}-K{n}` under an owner tagged `=F1+A2` gives
//! `=F1+A2-K3`, and `{page}{rung:3}` on sheet 2, rung 3 gives `2003`. Context parts change only
//! when tags are recomposed on request (`sheet.renumber`); an [`IdScope::per_owner`] scope numbers
//! and checks uniqueness within each owner, and an [`IdScope::number_by`] scope within each value
//! of one property (`numberBy: "code"` numbers K1, K2 and F1 separately).

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
    /// Numbers and uniqueness count within each owner (the element's direct owner in the
    /// containment tree), so every owner has its own K1.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub per_owner: bool,
    /// A property key: numbers and uniqueness count among members with the same value of it
    /// (`code`, so K1, K2 and F1 each count separately); empty for one count over the scope.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub number_by: String,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for IdScope {
    fn default() -> Self {
        IdScope {
            id: String::new(),
            types: Vec::new(),
            format: "{n}".into(),
            unique: true,
            paired: false,
            auto_number: false,
            per_owner: false,
            number_by: String::new(),
        }
    }
}

/// What the context parts of a format read for one element; empty text composes as nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposeCtx {
    /// The sheet number of the element's first view (`{page}`).
    pub page: String,
    /// The zone of that view (`{zone}`, `B4`).
    pub zone: String,
    /// The element's rung (`{rung}`).
    pub rung: String,
}

/// Most matching steps one tag is given when a format has parts that match any text, so a
/// hostile format cannot make reading a number slow.
const MATCH_BUDGET: u32 = 20_000;

/// One piece of a parsed format.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    /// Literal text.
    Lit(String),
    /// The auto number, zero-padded to this width (0: no padding).
    Number(usize),
    /// An element property, by key.
    Prop(String),
    /// The owner's tag: the direct owner (`None`) or the nearest ancestor of this type.
    Owner(Option<String>),
    /// The sheet number, zero-padded to this width.
    Page(usize),
    /// The zone.
    Zone,
    /// The rung, zero-padded to this width.
    Rung(usize),
}

/// A format part resolved for matching a tag.
enum Tok {
    /// Exact text.
    Fixed(String),
    /// The auto number: at least this many digits.
    Num(usize),
    /// Any text (an owner tag not resolved).
    Any,
    /// A run of digits (a page or rung not resolved).
    Digits,
    /// One or two capitals then digits (a zone not resolved).
    Zone,
}

/// Match `toks` against `s` from `pos`: `Some(number)` when the whole rest matches, with the auto
/// number when the format has one.
fn match_toks(toks: &[Tok], s: &[u8], pos: usize, budget: &mut u32) -> Option<Option<u64>> {
    *budget = budget.checked_sub(1)?;
    let Some((tok, rest)) = toks.split_first() else { return (pos == s.len()).then_some(None) };
    let tail = s.get(pos..)?;
    let digit_run = tail.iter().take_while(|b| b.is_ascii_digit()).count();
    match tok {
        Tok::Fixed(t) => {
            if tail.starts_with(t.as_bytes()) {
                match_toks(rest, s, pos + t.len(), budget)
            } else {
                None
            }
        }
        Tok::Num(w) => {
            for l in ((*w).max(1)..=digit_run).rev() {
                if let Some(found) = match_toks(rest, s, pos + l, budget) {
                    let n = tail.get(..l)?.iter().fold(0u64, |n, b| n.saturating_mul(10).saturating_add(u64::from(b - b'0')));
                    return Some(found.or(Some(n)));
                }
            }
            None
        }
        Tok::Digits => (1..=digit_run).rev().find_map(|l| match_toks(rest, s, pos + l, budget)),
        Tok::Any => (0..=tail.len()).find_map(|l| match_toks(rest, s, pos + l, budget)),
        Tok::Zone => {
            let letters = tail.iter().take(2).take_while(|b| b.is_ascii_uppercase()).count();
            (1..=letters).find_map(|la| {
                let digits = tail.get(la..)?.iter().take_while(|b| b.is_ascii_digit()).count();
                (1..=digits).rev().find_map(|ld| match_toks(rest, s, pos + la + ld, budget))
            })
        }
    }
}

/// The tag of `element`'s owner for an owner part: the direct owner, or the nearest ancestor of
/// type `ty`. Empty when there is none.
fn owner_tag(model: &Model, element: &Element, ty: Option<&str>) -> String {
    match ty {
        None => element.owner.as_deref().and_then(|o| model.element(o)).map(|o| o.tag.clone()).unwrap_or_default(),
        Some(t) => {
            let own = element.owner.as_deref().and_then(|o| model.element(o));
            if let Some(o) = own.filter(|o| o.type_id == t) {
                return o.tag.clone();
            }
            let Some(start) = own else { return String::new() };
            model.ancestors(&start.id).iter().filter_map(|a| model.element(a)).find(|a| a.type_id == t).map(|a| a.tag.clone()).unwrap_or_default()
        }
    }
}

/// The rung a composed tag shows: the context's, else the element's `rung` property.
fn rung_of(element: &Element, ctx: &ComposeCtx) -> String {
    if ctx.rung.is_empty() { element.prop("rung").map(|v| v.display()).unwrap_or_default() } else { ctx.rung.clone() }
}

/// `v` zero-padded to `w` when it is all digits; other text as it is.
fn pad(v: &str, w: usize) -> String {
    if w > 0 && !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) { format!("{v:0>w$}") } else { v.to_string() }
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
    /// One `{...}` part: `n`, `n:W`, a context part or a property key.
    fn placeholder(name: &str, source: &str) -> Result<Part, ModelError> {
        let width = |w: &str| -> Result<usize, ModelError> {
            let w: usize = w.parse().map_err(|_| ModelError::Format(format!("bad pad `{name}` in `{source}`")))?;
            if w > MAX_PAD {
                return Err(ModelError::Format(format!("pad wider than {MAX_PAD} in `{source}`")));
            }
            Ok(w)
        };
        match name {
            "n" => return Ok(Part::Number(0)),
            "page" => return Ok(Part::Page(0)),
            "zone" => return Ok(Part::Zone),
            "rung" => return Ok(Part::Rung(0)),
            "owner.tag" => return Ok(Part::Owner(None)),
            _ => {}
        }
        if let Some(w) = name.strip_prefix("n:") {
            return Ok(Part::Number(width(w)?));
        }
        if let Some(w) = name.strip_prefix("page:") {
            return Ok(Part::Page(width(w)?));
        }
        if let Some(w) = name.strip_prefix("rung:") {
            return Ok(Part::Rung(width(w)?));
        }
        if let Some(ty) = name.strip_prefix("owner:").and_then(|t| t.strip_suffix(".tag")) {
            if ty.is_empty() || !ty.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.')) {
                return Err(ModelError::Format(format!("bad owner type in `{{{name}}}` in `{source}`")));
            }
            return Ok(Part::Owner(Some(ty.to_string())));
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
    /// Whether the format reads the element's context (owner, page, zone or rung), so its tags
    /// can go stale and are recomposed on request.
    pub fn has_context(&self) -> bool {
        self.parts.iter().any(|p| matches!(p, Part::Owner(_) | Part::Page(_) | Part::Zone | Part::Rung(_)))
    }
    /// The tag for `element` with auto number `n`; a missing property, and every context part,
    /// composes as empty text (see [`IdFormat::compose_in`]).
    pub fn compose(&self, element: &Element, n: u64) -> String {
        self.compose_with(None, element, n, &ComposeCtx::default())
    }
    /// The tag for `element` with auto number `n`, owner parts read from `model` and the page,
    /// zone and rung from `ctx`.
    pub fn compose_in(&self, model: &Model, element: &Element, n: u64, ctx: &ComposeCtx) -> String {
        self.compose_with(Some(model), element, n, ctx)
    }
    fn compose_with(&self, model: Option<&Model>, element: &Element, n: u64, ctx: &ComposeCtx) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                Part::Lit(t) => t.clone(),
                Part::Number(w) => format!("{n:0w$}", w = *w),
                Part::Prop(k) => element.prop(k).map(|v| v.display()).unwrap_or_default(),
                Part::Owner(ty) => model.map(|m| owner_tag(m, element, ty.as_deref())).unwrap_or_default(),
                Part::Page(w) => pad(&ctx.page, *w),
                Part::Zone => ctx.zone.clone(),
                Part::Rung(w) => pad(&rung_of(element, ctx), *w),
            })
            .collect()
    }
    /// The auto number inside `tag` when it matches this format for `element` with its owner
    /// parts read from `model` and its context from `ctx` exactly.
    pub fn number_in_ctx(&self, tag: &str, model: &Model, element: &Element, ctx: &ComposeCtx) -> Option<u64> {
        if !self.has_number() {
            return None;
        }
        let toks = self.toks(Some(model), element, Some(ctx));
        let mut budget = MATCH_BUDGET;
        match_toks(&toks, tag.as_bytes(), 0, &mut budget).flatten()
    }
    /// The parts as matching tokens; context parts not given match loosely.
    fn toks(&self, model: Option<&Model>, element: &Element, ctx: Option<&ComposeCtx>) -> Vec<Tok> {
        self.parts
            .iter()
            .map(|p| match (p, ctx) {
                (Part::Lit(t), _) => Tok::Fixed(t.clone()),
                (Part::Number(w), _) => Tok::Num(*w),
                (Part::Prop(k), _) => Tok::Fixed(element.prop(k).map(|v| v.display()).unwrap_or_default()),
                (Part::Owner(ty), _) => match model {
                    Some(m) => Tok::Fixed(owner_tag(m, element, ty.as_deref())),
                    None => Tok::Any,
                },
                (Part::Page(w), Some(c)) => Tok::Fixed(pad(&c.page, *w)),
                (Part::Zone, Some(c)) => Tok::Fixed(c.zone.clone()),
                (Part::Rung(w), Some(c)) => Tok::Fixed(pad(&rung_of(element, c), *w)),
                (Part::Page(_) | Part::Rung(_), None) => Tok::Digits,
                (Part::Zone, None) => Tok::Zone,
            })
            .collect()
    }
    /// The auto number inside `tag` when `tag` matches this format for `element` (literal and
    /// property parts equal, a run of digits where `{n}` is), else `None`. A `{n:W}` pad is a
    /// minimum: a run of at least W digits matches, so `S{n:3}` reads `S1000` as 1000 but not
    /// `S7`. A format without `{n}` never yields a number. Numbers too large for `u64` saturate.
    ///
    /// Context parts match loosely here (an owner tag any text, a page or rung digits, a zone
    /// capitals then digits), so the number survives a move to another sheet or zone.
    pub fn number_in(&self, tag: &str, element: &Element) -> Option<u64> {
        if self.has_context() {
            if !self.has_number() {
                return None;
            }
            let toks = self.toks(None, element, None);
            let mut budget = MATCH_BUDGET;
            return match_toks(&toks, tag.as_bytes(), 0, &mut budget).flatten();
        }
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
                    Part::Number(_) | Part::Owner(_) | Part::Page(_) | Part::Zone | Part::Rung(_) => String::new(),
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
    /// The context new tags compose with; `None` composes context parts as empty text.
    ctx: Option<&'a dyn Fn(&Element) -> ComposeCtx>,
}

impl<'a> Registry<'a> {
    /// A registry over `model` with these scopes (from every pack the model uses).
    pub fn new(model: &'a Model, scopes: &'a [IdScope]) -> Self {
        Registry { model, scopes, ctx: None }
    }
    /// The same registry composing context parts (`{page}`, `{zone}`, `{rung}`) with `ctx`.
    pub fn with_context(mut self, ctx: &'a dyn Fn(&Element) -> ComposeCtx) -> Self {
        self.ctx = Some(ctx);
        self
    }
    /// The model the registry reads.
    pub fn model(&self) -> &'a Model {
        self.model
    }
    /// The members of `scope` that share a numbering and uniqueness space with `element`: every
    /// member, or with [`IdScope::per_owner`] those with the same owner, and with
    /// [`IdScope::number_by`] those with the same value of that property.
    fn peers(&self, scope: &IdScope, element: &Element) -> Vec<&'a Element> {
        let key = Self::group_key(scope, element);
        self.members(scope).into_iter().filter(|m| Self::group_key(scope, m) == key).collect()
    }
    /// The numbering group of `element` in `scope`: its owner (per-owner scopes) and its value of
    /// the `numberBy` property, empty parts when the scope does not group by them.
    fn group_key(scope: &IdScope, element: &Element) -> (String, String) {
        let owner = if scope.per_owner { element.owner.clone().unwrap_or_default() } else { String::new() };
        let by = if scope.number_by.is_empty() { String::new() } else { element.prop(&scope.number_by).map(|v| v.display()).unwrap_or_default() };
        (owner, by)
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
        let others: Vec<&Element> = self.peers(scope, element).into_iter().filter(|m| m.id != element.id || element.id.is_empty()).collect();
        let ctx = self.ctx.map(|f| f(element)).unwrap_or_default();
        let highest = others.iter().filter_map(|m| format.number_in(&m.tag, m)).max().unwrap_or(0);
        let taken: HashSet<&str> = others.iter().map(|m| m.tag.as_str()).filter(|t| !t.is_empty()).collect();
        // At most `taken.len()` candidates can be taken, so this many tries always finds a free
        // one unless the numbers run out.
        let mut n = highest;
        for _ in 0..=taken.len() {
            n = n.checked_add(1).ok_or(ModelError::TooMany("tag numbers"))?;
            let tag = format.compose_in(self.model, element, n, &ctx);
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
        let peers = match except.and_then(|e| self.model.element(e)) {
            Some(e) => self.peers(scope, e),
            None if scope.per_owner || !scope.number_by.is_empty() => {
                self.members(scope).into_iter().filter(|m| Self::group_key(scope, m) == (String::new(), String::new())).collect()
            }
            None => self.members(scope),
        };
        let users = peers.into_iter().filter(|m| m.tag == tag && Some(m.id.as_str()) != except).count();
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
    /// Member element ids of `scope` grouped by non-empty tag (and by owner in a per-owner
    /// scope, by the `numberBy` value when it is set), tags sorted, ids in model order.
    fn by_tag(&self, scope: &IdScope) -> BTreeMap<(&'a str, (String, String)), Vec<String>> {
        let mut map: BTreeMap<(&'a str, (String, String)), Vec<String>> = BTreeMap::new();
        for m in self.members(scope) {
            if !m.tag.is_empty() {
                map.entry((m.tag.as_str(), Self::group_key(scope, m))).or_default().push(m.id.clone());
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
            for ((tag, _), elements) in self.by_tag(scope) {
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
            for ((tag, _), mut elements) in self.by_tag(scope) {
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
