//! Spelling: the bundled en-US word list, the words of a text that are not in it, and suggestions.
//!
//! The list is `assets/dictionaries/en-US.txt`, derived from SCOWL (see `ATTRIBUTION.md`): one
//! lower-case ASCII word per line, with its possessives and contractions. It is compiled in with
//! `include_str!` and indexed by length on first use, so a lookup is one hash of a short string
//! and suggestions can ask for every word one edit away without scanning the list.
//!
//! What counts as a word to check is decided in [`misspellings`]: runs of letters and apostrophes
//! that hold no digit, are not all capitals, have no capital inside them (`iPhone`, `VizCraft`),
//! are not inside a `{{field}}`, and are not part of a URL, an address or a file path. Offsets are
//! in chars of the text given, so a caller can cut the word out without caring about UTF-8.

use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

/// The bundled list.
static EN_US: &str = include_str!("../../../assets/dictionaries/en-US.txt");

/// The proofing language of a document that sets none.
pub const DEFAULT_LANGUAGE: &str = "en-US";

/// Proofing languages offered in menus: `(tag, name)`. Only [`DEFAULT_LANGUAGE`] has a word list;
/// a shape in another language is skipped by spelling checks and the result says so.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("fr-FR", "French (France)"),
    ("de-DE", "German (Germany)"),
    ("es-ES", "Spanish (Spain)"),
];

/// Most suggestions returned for one word.
pub const MAX_SUGGESTIONS: usize = 8;

/// Longest word, in chars, that gets suggestions.
const MAX_SUGGEST_LEN: usize = 40;

/// Longest word, in chars, for which suggestions two edits away are searched.
const MAX_TWO_EDIT_LEN: usize = 12;

/// Most distinct one-edit candidates expanded to look two edits away.
const MAX_TWO_EDIT_CANDIDATES: usize = 4000;

/// Letters candidates are built from.
const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz'";

/// Whether `tag` names a language with a word list.
pub fn has_dictionary(tag: &str) -> bool {
    tag.eq_ignore_ascii_case(DEFAULT_LANGUAGE)
}

/// The language a text is checked in: the shape's own tag, else the document's, else
/// [`DEFAULT_LANGUAGE`].
pub fn effective_language<'a>(document: &'a str, shape: &'a str) -> &'a str {
    [shape, document].into_iter().find(|t| !t.is_empty()).unwrap_or(DEFAULT_LANGUAGE)
}

/// A set of known words, indexed by length.
pub struct Dictionary {
    by_len: Vec<HashSet<&'static str>>,
}

impl Dictionary {
    fn build(list: &'static str) -> Dictionary {
        let mut by_len: Vec<HashSet<&'static str>> = Vec::new();
        for w in list.lines().map(str::trim).filter(|w| !w.is_empty()) {
            let n = w.chars().count();
            if by_len.len() <= n {
                by_len.resize_with(n + 1, HashSet::new);
            }
            if let Some(set) = by_len.get_mut(n) {
                set.insert(w);
            }
        }
        Dictionary { by_len }
    }

    /// The bundled en-US dictionary, built on first use.
    pub fn en_us() -> &'static Dictionary {
        static DICT: OnceLock<Dictionary> = OnceLock::new();
        DICT.get_or_init(|| Dictionary::build(EN_US))
    }

    /// Whether the lower-case `word` is in the list as written.
    pub fn contains(&self, word: &str) -> bool {
        self.by_len.get(word.chars().count()).is_some_and(|s| s.contains(word))
    }

    /// Whether `word` is known: in the list, or a known word with `'s` or a trailing `'`.
    pub fn knows(&self, word: &str) -> bool {
        let lower = normalize(word);
        if self.contains(&lower) {
            return true;
        }
        let base = lower.strip_suffix("'s").or_else(|| lower.strip_suffix('\''));
        base.is_some_and(|b| !b.is_empty() && self.contains(b))
    }

    /// Up to [`MAX_SUGGESTIONS`] known words one edit away from `word` (a deletion, an insertion,
    /// a change of one letter or a swap of two neighbours), swaps first; when there are none, the
    /// words two edits away. Cased like `word`: capitalised if it is, upper case if it is.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let lower = normalize(word);
        let chars: Vec<char> = lower.chars().collect();
        if chars.is_empty() || chars.len() > MAX_SUGGEST_LEN {
            return Vec::new();
        }
        let mut found: BTreeMap<String, u8> = BTreeMap::new();
        edits(&chars, &mut |cand, rank| {
            if self.contains(&cand) && cand != lower {
                let e = found.entry(cand).or_insert(rank);
                *e = (*e).min(rank);
            }
        });
        if found.is_empty() && chars.len() <= MAX_TWO_EDIT_LEN {
            let mut firsts: HashSet<String> = HashSet::new();
            edits(&chars, &mut |cand, _| {
                if firsts.len() < MAX_TWO_EDIT_CANDIDATES {
                    firsts.insert(cand);
                }
            });
            let mut firsts: Vec<String> = firsts.into_iter().collect();
            firsts.sort();
            for f in firsts {
                let fc: Vec<char> = f.chars().collect();
                edits(&fc, &mut |cand, _| {
                    if self.contains(&cand) && cand != lower {
                        found.entry(cand).or_insert(3);
                    }
                });
            }
        }
        let mut ranked: Vec<(u8, String)> = found.into_iter().map(|(w, r)| (r, w)).collect();
        ranked.sort();
        ranked.into_iter().take(MAX_SUGGESTIONS).map(|(_, w)| recase(&w, word)).collect()
    }
}

/// Lower case with the typographic apostrophe made plain.
fn normalize(word: &str) -> String {
    word.replace('\u{2019}', "'").to_lowercase()
}

/// `suggestion` in the case style of `original`.
fn recase(suggestion: &str, original: &str) -> String {
    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return suggestion.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut cs = suggestion.chars();
        return cs.next().map(|f| f.to_uppercase().collect::<String>() + cs.as_str()).unwrap_or_default();
    }
    suggestion.to_string()
}

/// Call `f(candidate, rank)` for every string one edit from `word`; rank 0 is a swap of two
/// neighbours, 1 a changed letter, 2 an insertion or a deletion.
fn edits(word: &[char], f: &mut impl FnMut(String, u8)) {
    for i in 0..word.len().saturating_sub(1) {
        let mut w = word.to_vec();
        w.swap(i, i + 1);
        f(w.iter().collect(), 0);
    }
    for i in 0..word.len() {
        for a in ALPHABET.chars() {
            if word.get(i) != Some(&a) {
                let mut w = word.to_vec();
                if let Some(slot) = w.get_mut(i) {
                    *slot = a;
                }
                f(w.iter().collect(), 1);
            }
        }
    }
    for i in 0..word.len() {
        let mut w = word.to_vec();
        w.remove(i);
        f(w.iter().collect(), 2);
    }
    for i in 0..=word.len() {
        for a in ALPHABET.chars() {
            let mut w = word.to_vec();
            w.insert(i, a);
            f(w.iter().collect(), 2);
        }
    }
}

/// A word the dictionary does not know, with where it is in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Miss {
    /// Offset of the first char, in chars.
    pub start: usize,
    /// Offset after the last char, in chars.
    pub end: usize,
    /// The word as written.
    pub word: String,
}

/// Whether a whitespace-free chunk is a URL, an e-mail address or a file path, none of which are
/// prose.
fn opaque(chunk: &str) -> bool {
    let lower = chunk.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") || lower.starts_with("mailto:") || chunk.contains('@') {
        return true;
    }
    let seps = chunk.chars().filter(|c| matches!(c, '/' | '\\')).count();
    let drive = chunk.chars().nth(1) == Some(':') && chunk.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    seps > 0 && (seps > 1 || chunk.starts_with(['/', '\\', '~', '.']) || chunk.contains('.') || drive)
}

/// The chars of `chars` that are checked: `false` inside `{{...}}` and in opaque chunks.
fn checked_mask(chars: &[char]) -> Vec<bool> {
    let mut mask = vec![true; chars.len()];
    let mut i = 0;
    while i < chars.len() {
        if chars.get(i) == Some(&'{') && chars.get(i + 1) == Some(&'{') {
            let close = (i + 2..chars.len().saturating_sub(1)).find(|&j| chars.get(j) == Some(&'}') && chars.get(j + 1) == Some(&'}'));
            let end = close.map_or(chars.len(), |j| j + 2);
            mask.iter_mut().take(end).skip(i).for_each(|m| *m = false);
            i = end;
        } else {
            i += 1;
        }
    }
    let mut i = 0;
    while i < chars.len() {
        if chars.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
            continue;
        }
        let end = (i..chars.len()).find(|&j| chars.get(j).is_some_and(|c| c.is_whitespace())).unwrap_or(chars.len());
        if let Some(chunk) = chars.get(i..end)
            && opaque(&chunk.iter().collect::<String>())
        {
            mask.iter_mut().take(end).skip(i).for_each(|m| *m = false);
        }
        i = end;
    }
    mask
}

/// Whether a candidate word is worth checking: ASCII letters and apostrophes only (the list has
/// no other), at least two letters, not all capitals and no capital after a lower-case letter.
fn checkable(word: &str) -> bool {
    let letters: Vec<char> = word.chars().filter(|c| *c != '\'' && *c != '\u{2019}').collect();
    if letters.len() < 2 || !word.chars().all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '\u{2019}') {
        return false;
    }
    if letters.iter().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    !letters.windows(2).any(|w| matches!(w, [a, b] if a.is_ascii_lowercase() && b.is_ascii_uppercase()))
}

/// The words of `text` that `dict` does not know and `ignored` (lower case) does not list, in
/// text order.
pub fn misspellings(dict: &Dictionary, text: &str, ignored: &[String]) -> Vec<Miss> {
    let chars: Vec<char> = text.chars().collect();
    let mask = checked_mask(&chars);
    let word_char = |i: usize| chars.get(i).is_some_and(|c| c.is_alphanumeric() || *c == '\'' || *c == '\u{2019}') && mask.get(i) == Some(&true);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !word_char(i) {
            i += 1;
            continue;
        }
        let mut end = i;
        while word_char(end) {
            end += 1;
        }
        // Apostrophes at the edges are quotes, not part of the word.
        let (mut s, mut e) = (i, end);
        while chars.get(s).is_some_and(|c| *c == '\'' || *c == '\u{2019}') && s < e {
            s += 1;
        }
        while e > s && chars.get(e - 1).is_some_and(|c| *c == '\'' || *c == '\u{2019}') {
            e -= 1;
        }
        let word: String = chars.get(s..e).unwrap_or_default().iter().collect();
        if checkable(&word) && !dict.knows(&word) {
            let lower = normalize(&word);
            if !ignored.contains(&lower) {
                out.push(Miss { start: s, end: e, word });
            }
        }
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<String> {
        misspellings(Dictionary::en_us(), text, &[]).into_iter().map(|m| m.word).collect()
    }

    #[test]
    fn the_list_knows_common_words_and_forms() {
        let d = Dictionary::en_us();
        for w in ["the", "diagram", "Color", "pump's", "don't", "dogs'", "Monday"] {
            assert!(d.knows(w), "{w}");
        }
        assert!(!d.knows("teh") && !d.knows("colour"), "american spelling");
    }

    #[test]
    fn a_misspelling_is_found_with_its_offsets_and_the_right_suggestion() {
        let m = misspellings(Dictionary::en_us(), "Send teh data", &[]);
        assert_eq!(m, vec![Miss { start: 5, end: 8, word: "teh".into() }]);
        assert!(Dictionary::en_us().suggest("teh").contains(&"the".to_string()));
        assert_eq!(Dictionary::en_us().suggest("teh").first().map(String::as_str), Some("the"), "a swap ranks first");
    }

    #[test]
    fn offsets_count_chars_not_bytes() {
        let m = misspellings(Dictionary::en_us(), "caf\u{e9} \u{1F600}\u{2014} teh", &[]);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].start, m[0].end), (8, 11));
    }

    #[test]
    fn fields_urls_paths_capitals_numbers_and_identifiers_are_skipped() {
        let skipped = "{{pagename}} {{ teh }} https://exmple.com/teh www.teh.com bob@teh.org /usr/teh/file.txt C:\\teh\\x ~/teh/a.b NASA ab12 4th iPhone VizCraft";
        assert_eq!(found(skipped), Vec::<String>::new());
        assert_eq!(found("and/or teh"), vec!["teh"], "a lone slash is prose");
        assert_eq!(found("{{unterminated teh"), Vec::<String>::new());
        assert_eq!(found("{{a}} teh"), vec!["teh"]);
    }

    #[test]
    fn quotes_and_possessives_are_not_part_of_the_word() {
        assert_eq!(found("'teh' and \u{2018}teh\u{2019} teh's"), vec!["teh", "teh", "teh's"]);
        assert!(found("the pump's tank, the pumps' tank, don\u{2019}t").is_empty());
    }

    #[test]
    fn ignored_words_are_skipped_in_any_case() {
        let m = misspellings(Dictionary::en_us(), "Teh teh", &["teh".to_string()]);
        assert!(m.is_empty());
    }

    #[test]
    fn suggestions_are_capped_cased_and_fall_back_to_two_edits() {
        let d = Dictionary::en_us();
        assert!(d.suggest("a").len() <= MAX_SUGGESTIONS);
        assert_eq!(d.suggest("Teh").first().map(String::as_str), Some("The"));
        assert_eq!(d.suggest("TEH").first().map(String::as_str), Some("THE"));
        assert!(d.suggest("recieeve").contains(&"receive".to_string()), "two edits");
        assert!(d.suggest("").is_empty() && d.suggest(&"x".repeat(100)).is_empty());
        assert!(d.suggest("zzzzzzzzzzzzzzzzzz").is_empty());
    }

    #[test]
    fn language_choice_prefers_the_shape() {
        assert_eq!(effective_language("", ""), "en-US");
        assert_eq!(effective_language("fr-FR", ""), "fr-FR");
        assert_eq!(effective_language("fr-FR", "none"), "none");
        assert!(has_dictionary("en-us") && !has_dictionary("fr-FR"));
    }
}
