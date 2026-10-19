//! Identifiers for generated constructs, kept stable by a rename map.
//!
//! The manifest stores a map from element id to identifier. On first generation an element's
//! identifier is made from its tag (or its name, or its id), sanitised to the target's
//! [`NameRules`] and made unique. Later runs reuse the stored identifier, so renaming an element
//! in the model changes no generated name, and no code that refers to it, until a run asks for
//! `renames: true`. That run makes every identifier afresh and lists the ones that change.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::target::ModelView;

/// Letter case and word joining of an identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// `gather-input`
    Kebab,
    /// `gather_input`
    Snake,
    /// `GATHER_INPUT`
    ScreamingSnake,
    /// `gatherInput`
    Camel,
    /// `GatherInput`
    Pascal,
}

/// How a target's language spells identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameRules {
    /// Case and joining.
    pub case: Case,
    /// Longest identifier, in bytes (ASCII).
    pub max_len: usize,
    /// Words the language reserves; an identifier equal to one gets a trailing `_`.
    pub reserved: &'static [&'static str],
    /// Put in front of an identifier that would start with a digit, when the language forbids
    /// that; `None` lets digits lead.
    pub digit_prefix: Option<&'static str>,
    /// Used when nothing of the source survives sanitising.
    pub fallback: &'static str,
}

impl NameRules {
    /// The separator before a uniqueness counter.
    fn counter_sep(&self) -> &'static str {
        match self.case {
            Case::Kebab => "-",
            Case::Snake | Case::ScreamingSnake => "_",
            Case::Camel | Case::Pascal => "",
        }
    }
}

/// Most uniqueness counters tried before falling back to the element id.
const MAX_COUNTER: usize = 10_000;

/// `raw` as an identifier under `rules`: ASCII letters and digits only, split into words at
/// anything else and at lower-to-upper case changes, joined in the rules' case.
pub fn sanitize(raw: &str, rules: &NameRules) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in raw.chars().take(4 * rules.max_len.max(1)) {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            cur.push(ch.to_ascii_lowercase());
        } else {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    let cap = |w: &str| {
        let mut c = w.chars();
        c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
    };
    let mut out = match rules.case {
        Case::Kebab => words.join("-"),
        Case::Snake => words.join("_"),
        Case::ScreamingSnake => words.join("_").to_ascii_uppercase(),
        Case::Camel => words.iter().enumerate().map(|(i, w)| if i == 0 { w.clone() } else { cap(w) }).collect(),
        Case::Pascal => words.iter().map(|w| cap(w)).collect(),
    };
    if out.is_empty() {
        out = rules.fallback.to_string();
    }
    if let Some(prefix) = rules.digit_prefix
        && out.starts_with(|c: char| c.is_ascii_digit())
    {
        out.insert_str(0, prefix);
    }
    out.truncate(rules.max_len.max(1));
    if rules.reserved.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// The identifier each element in a view gets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    map: BTreeMap<String, String>,
}

/// An identifier that a `renames: true` run changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    /// The element id.
    pub id: String,
    /// The identifier until now.
    pub from: String,
    /// The identifier from this run on.
    pub to: String,
}

impl Names {
    /// Names from a ready map (tests and targets that are given names).
    pub fn from_map(map: BTreeMap<String, String>) -> Self {
        Names { map }
    }

    /// The identifier of element `id`.
    pub fn get(&self, id: &str) -> Option<&str> {
        self.map.get(id).map(String::as_str)
    }

    /// The identifier of element `id`, or the id itself when it has none.
    pub fn ident<'a>(&'a self, id: &'a str) -> &'a str {
        self.get(id).unwrap_or(id)
    }

    /// The whole map, element id to identifier, as the manifest stores it.
    pub fn map(&self) -> &BTreeMap<String, String> {
        &self.map
    }

    /// Names for every element of `view`. Without `renames`, an element keeps the identifier in
    /// `previous` when that is still a valid, unclaimed identifier; every other element gets one
    /// from its tag, name or id. With `renames`, every identifier is made afresh and the ones
    /// that differ from `previous` are returned.
    pub fn resolve(view: &ModelView, rules: &NameRules, previous: &BTreeMap<String, String>, renames: bool) -> (Names, Vec<Rename>) {
        let mut map = BTreeMap::new();
        let mut used: HashSet<String> = HashSet::new();
        // The next counter to try per base, so many elements with one name stay linear.
        let mut next: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        if !renames {
            for e in &view.elements {
                if let Some(prev) = previous.get(&e.id)
                    && !prev.is_empty()
                    && sanitize(prev, rules) == *prev
                    && used.insert(prev.clone())
                {
                    map.insert(e.id.clone(), prev.clone());
                }
            }
        }
        for e in &view.elements {
            if map.contains_key(&e.id) {
                continue;
            }
            let source = if !e.tag.trim().is_empty() {
                e.tag.as_str()
            } else if !e.name.trim().is_empty() {
                e.name.as_str()
            } else {
                e.id.as_str()
            };
            let base = sanitize(source, rules);
            let mut ident = base.clone();
            let mut n = next.get(&base).copied().unwrap_or(2);
            while used.contains(&ident) && n < MAX_COUNTER {
                ident = format!("{base}{}{n}", rules.counter_sep());
                n += 1;
            }
            next.insert(base.clone(), n);
            if used.contains(&ident) {
                ident = sanitize(&format!("{base} {}", e.id), rules);
            }
            used.insert(ident.clone());
            map.insert(e.id.clone(), ident);
        }
        let mut changed = Vec::new();
        if renames {
            for (id, to) in &map {
                if let Some(from) = previous.get(id)
                    && from != to
                {
                    changed.push(Rename { id: id.clone(), from: from.clone(), to: to.clone() });
                }
            }
        }
        (Names { map }, changed)
    }
}
