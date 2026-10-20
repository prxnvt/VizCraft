//! Ranked master search over a keyword index, rebuilt when the catalog changes.
//!
//! The index reads the catalog only through [`crate::sets`] and the public [`Master`] fields, so
//! it works whatever builds the masters. Each master gets its lowercased id, label, the words of
//! its id, label, category and set name, and its keywords. A query matches a master when every
//! query word matches it somewhere; matches rank in tiers (see [`Tier`]) and ties keep catalog
//! order, so the Shapes pane and `shape.search` show the most likely master first.

use std::sync::{Mutex, PoisonError};

use crate::Master;

/// Longest query looked at, in chars; real queries are a few words.
const MAX_QUERY_CHARS: usize = 256;
/// Most query words looked at; the rest are ignored.
const MAX_WORDS: usize = 16;
/// Longest query word looked at, in chars.
const MAX_WORD_CHARS: usize = 64;

/// How well a master matches, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    /// The whole query is the label, the id, or the id's name part (`decision`).
    Exact,
    /// Each query word starts a word of the id, label, category or set name.
    WordPrefix,
    /// Each query word starts a keyword word (or a better tier above).
    Keyword,
    /// Some query word only appears inside the text.
    Substring,
}

/// One master's searchable text, all lowercased.
struct Entry {
    master: &'static Master,
    /// The id, label and id name part (after the set prefix).
    exact: [String; 3],
    /// Words of the id, label, category and set name.
    names: Vec<String>,
    /// Words of the keywords.
    keywords: Vec<String>,
    /// Everything above in one string, for substring matches.
    hay: String,
}

/// Split text into lowercase words at anything that is not a letter or digit, and at camelCase
/// humps, so `onPageReference` and `On-page Reference` both give `on`, `page`, `reference`.
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_lowercase() || c.is_numeric();
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn entry(set_name: &str, m: &'static Master) -> Entry {
    let id = m.id.to_lowercase();
    let name = id.split_once('.').map_or(id.as_str(), |(_, n)| n).to_string();
    let label = m.label.to_lowercase();
    let mut names = words(m.id);
    names.extend(words(m.label));
    names.extend(words(m.category));
    names.extend(words(set_name));
    let keywords: Vec<String> = m.keywords.iter().flat_map(|k| words(k)).collect();
    let hay = format!("{id} {label} {} {} {}", m.category.to_lowercase(), set_name.to_lowercase(), m.keywords.join(" ").to_lowercase());
    Entry { master: m, exact: [id, label, name], names, keywords, hay }
}

/// The index, rebuilt when the stencil sets are (a pack was loaded).
fn index() -> &'static [Entry] {
    static INDEX: Mutex<Option<(usize, &'static [Entry])>> = Mutex::new(None);
    let stamp = crate::catalog_stamp();
    let mut slot = INDEX.lock().unwrap_or_else(PoisonError::into_inner);
    match *slot {
        Some((k, idx)) if k == stamp => idx,
        _ => {
            let idx: Vec<Entry> = crate::sets().iter().flat_map(|s| s.masters.iter().map(move |m| entry(s.name, m))).collect();
            let idx: &'static [Entry] = idx.leak();
            *slot = Some((stamp, idx));
            idx
        }
    }
}

/// How one query word matches an entry, or `None` when it does not.
fn word_tier(e: &Entry, w: &str) -> Option<Tier> {
    if e.names.iter().any(|n| n.starts_with(w)) {
        Some(Tier::WordPrefix)
    } else if e.keywords.iter().any(|k| k.starts_with(w)) {
        Some(Tier::Keyword)
    } else if e.hay.contains(w) {
        Some(Tier::Substring)
    } else {
        None
    }
}

/// Masters matching every word of `query`, best first; an empty query gives the whole catalog in
/// order.
pub(crate) fn search(query: &str) -> Vec<&'static Master> {
    let query: String = query.chars().take(MAX_QUERY_CHARS).collect::<String>().to_lowercase();
    let qwords: Vec<String> = query.split_whitespace().take(MAX_WORDS).map(|w| w.chars().take(MAX_WORD_CHARS).collect()).collect();
    let whole = qwords.join(" ");
    let mut hits: Vec<(Tier, usize, &'static Master)> = index()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if !whole.is_empty() && e.exact.contains(&whole) {
                return Some((Tier::Exact, i, e.master));
            }
            // The weakest word decides: a master is only as good a match as its worst word.
            let mut tier = Tier::WordPrefix;
            for w in &qwords {
                tier = tier.max(word_tier(e, w)?);
            }
            Some((tier, i, e.master))
        })
        .collect();
    hits.sort_by_key(|(t, i, _)| (*t, *i));
    hits.into_iter().map(|(_, _, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sets;

    fn ids(q: &str) -> Vec<&'static str> {
        search(q).iter().map(|m| m.id).collect()
    }

    #[test]
    fn words_split_ids_labels_and_humps() {
        assert_eq!(words("flowchart.onPageReference"), ["flowchart", "on", "page", "reference"]);
        assert_eq!(words("On-page Reference"), ["on", "page", "reference"]);
        assert_eq!(words("  "), Vec::<String>::new());
        assert_eq!(words("Étape ÜBER"), ["étape", "über"]);
    }

    #[test]
    fn every_master_has_keywords_and_is_found_by_its_label_and_id() {
        for s in sets() {
            for m in s.masters {
                assert!(!m.keywords.is_empty(), "{} has no keywords", m.id);
                // Masters sharing a label ("Document") all come before anything else.
                let same = sets().iter().flat_map(|s| s.masters.iter()).filter(|x| x.label == m.label).count();
                for q in [m.label.to_string(), m.label.to_uppercase()] {
                    assert!(ids(&q)[..same].contains(&m.id), "label {q:?}");
                }
                assert_eq!(ids(m.id).first(), Some(&m.id), "id {}", m.id);
                for k in m.keywords {
                    assert!(search(k).iter().any(|x| x.id == m.id), "{} keyword {k:?}", m.id);
                }
            }
        }
    }

    #[test]
    fn ranking_is_exact_then_word_prefix_then_keyword_then_substring() {
        // Exact label first, then word prefix (the Data category and the Database label) in
        // catalog order, then the keyword match on the basic cylinder.
        let data = ids("data");
        assert_eq!(data.first(), Some(&"flowchart.data"));
        let pos = |id: &str| data.iter().position(|x| *x == id).unwrap();
        assert!(pos("flowchart.manualInput") < pos("flowchart.database"), "ties keep catalog order");
        assert!(pos("flowchart.database") < pos("basic.cylinder"), "word prefix beats keyword");
        // Two exact labels ("Document") in catalog order.
        assert_eq!(ids("document")[..2], ["basic.document", "flowchart.document"]);
        // Word prefix ahead of substring: "rect" starts words; "angle" sits inside them.
        // Engineering packs add their own matches (a directional valve) after these.
        let core = |v: Vec<&'static str>| v.into_iter().filter(|i| i.starts_with("basic.") || i.starts_with("flowchart.")).collect::<Vec<_>>();
        assert_eq!(core(ids("rect")), ["basic.rectangle", "basic.roundedRectangle"]);
        assert_eq!(ids("angle"), ["basic.rectangle", "basic.roundedRectangle", "basic.triangle"]);
        assert_eq!(ids("circle"), ["basic.ellipse", "flowchart.onPageReference"]);
        assert_eq!(ids("decision").first(), Some(&"flowchart.decision"));
        assert_eq!(ids("manual input").first(), Some(&"flowchart.manualInput"));
    }

    #[test]
    fn every_query_word_must_match() {
        assert_eq!(ids("manual keyboard"), ["flowchart.manualInput"]);
        assert!(ids("decision cloud").is_empty());
        assert_eq!(ids("  Flowchart   DECISION "), ["flowchart.decision"]);
    }

    #[test]
    fn hostile_queries_are_bounded() {
        let all: usize = sets().iter().map(|s| s.masters.len()).sum();
        assert_eq!(search("").len(), all);
        assert_eq!(search(" \t\n ").len(), all);
        assert!(search(&"a ".repeat(100_000)).len() <= all);
        assert!(search(&"x".repeat(1_000_000)).is_empty());
        assert!(search("\u{0}\u{FFFF}\u{1F600}").is_empty());
        assert!(search("ß İ ǅ 中文 مرحبا").is_empty());
        // A query past the char cap still splits at a char boundary.
        let long = format!("{}{}", "é".repeat(MAX_QUERY_CHARS - 1), "ü".repeat(10));
        assert!(search(&long).is_empty());
    }
}
