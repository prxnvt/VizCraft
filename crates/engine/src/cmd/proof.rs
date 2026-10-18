//! Proofing commands: spelling check over shape texts, compartments and connector texts, word
//! suggestions, the document's ignore list and the proofing language of shapes.
//!
//! Checking never changes the document. Which language a text is checked in comes from
//! `Shape.language`, else `Document.language`, else en-US (see `crate::proof`); only en-US has a
//! word list, so texts in other languages are skipped and the result says which.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use vizcraft_diagram::doc_fields::{LANGUAGE_NONE, MAX_IGNORED_WORD, MAX_IGNORED_WORDS, normalize_language};
use vizcraft_diagram::{Connector, Document, Page, Shape};

use super::{ids_param, opt_bool, short, with_sel};
use crate::proof::{self, Dictionary};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Most misspellings one `proof.check` reports; past it the result says `truncated`.
pub const MAX_MISSES: usize = 2000;

/// Return the proofing commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("proof.check", "Spelling", "Review > Proofing", check)
            .key("F7")
            .params(r#"{"ids"?: [string], "page"?: id|name|index, "all"?: bool} -> {"checked": n, "misspellings": [{"page", "id", "kind": "shape"|"connector", "field", "word", "start", "end", "language", "suggestions": [string]}], "skipped": [{"id", "language"}], "notes": [string], "truncated": bool}; default is the active page; field is text, title:<compartment>, body:<compartment> or row:<compartment>:<n>; offsets are in chars of that field; read-only"#),
        CommandSpec::new("proof.suggest", "Suggest Spelling", "Review > Proofing", suggest)
            .params(r#"{"word": string} -> {"word", "known": bool, "suggestions": [string]}; at most eight words one edit away, or two when none; read-only"#),
        CommandSpec::new("proof.ignore", "Ignore All", "Review > Proofing", ignore)
            .params(r#"{"word": string} -> {"word", "added": bool, "ignored": [string]}; the document's spelling checks accept the word from now on (case-insensitive)"#),
        CommandSpec::new("proof.setLanguage", "Proofing Language", "Review > Language", set_language).params(
            r#"{"ids"?: [string], "language": "en-US"|"fr-FR"|any BCP 47 tag|"none"|"default", "document"?: bool} -> {"language", "ids": [string], "document": bool}; sets the shapes (default: the selected ones); none skips them in spelling checks, default follows the document; with no shapes selected, or document true, sets the document's default language"#,
        ),
    ]
}

/// The `page` param as text: a string, or a number meaning an index.
pub(super) fn page_ref(v: &Value) -> Result<Option<String>, EngineError> {
    match v.get("page") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(EngineError::Params("`page` must be a page id, name or index".into())),
    }
}

/// One text of a shape or connector that spelling checks look at.
struct Field {
    id: String,
    kind: &'static str,
    field: String,
    text: String,
}

/// Every checked text of `shape`: its text, then each compartment's title, text and rows.
fn shape_fields(shape: &Shape) -> Vec<Field> {
    let f = |field: String, text: &str| Field { id: shape.id.clone(), kind: "shape", field, text: text.to_string() };
    let mut out = vec![f("text".into(), &shape.text)];
    for c in &shape.compartments {
        if let Some(t) = &c.title {
            out.push(f(format!("title:{}", c.id), t));
        }
        out.push(f(format!("body:{}", c.id), &c.text));
        out.extend(c.rows.iter().enumerate().map(|(i, r)| f(format!("row:{}:{i}", c.id), r)));
    }
    out.retain(|x| !x.text.trim().is_empty());
    out
}

fn connector_fields(c: &Connector) -> Vec<Field> {
    if c.text.trim().is_empty() {
        return Vec::new();
    }
    vec![Field { id: c.id.clone(), kind: "connector", field: "text".into(), text: c.text.clone() }]
}

/// The pages to check and, for each, the ids to limit it to (`None` is everything on the page).
fn scope(s: &Session, v: &Value) -> Result<Vec<(usize, Option<Vec<String>>)>, EngineError> {
    let doc = &s.doc;
    if let Some(ids) = ids_param(v)? {
        let page = doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        if let Some(bad) = ids.iter().find(|i| !page.contains_id(i)) {
            return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(bad))));
        }
        return Ok(vec![(doc.active_index(), Some(ids))]);
    }
    if opt_bool(v, "all")?.unwrap_or(false) {
        return Ok((0..doc.pages.len()).map(|i| (i, None)).collect());
    }
    match page_ref(v)? {
        Some(r) => {
            let i = doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))?;
            Ok(vec![(i, None)])
        }
        None => Ok(vec![(doc.active_index(), None)]),
    }
}

fn check(s: &mut Session, v: &Value) -> CmdResult {
    let targets = scope(s, v)?;
    let doc = &s.doc;
    let dict = Dictionary::en_us();
    let mut checked = 0usize;
    let mut misspellings = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut languages = BTreeSet::new();
    let mut truncated = false;
    'pages: for (pi, only) in targets {
        let Some(page) = doc.pages.get(pi) else { continue };
        let wanted = |id: &str| only.as_ref().is_none_or(|ids| ids.iter().any(|i| i == id));
        let items = page
            .shapes
            .iter()
            .filter(|sh| wanted(&sh.id))
            .map(|sh| (proof::effective_language(&doc.language, &sh.language).to_string(), shape_fields(sh)))
            .chain(page.connectors.iter().filter(|c| wanted(&c.id)).map(|c| (proof::effective_language(&doc.language, "").to_string(), connector_fields(c))));
        for (language, fields) in items {
            if fields.is_empty() {
                continue;
            }
            if language == LANGUAGE_NONE || !proof::has_dictionary(&language) {
                if let Some(first) = fields.first() {
                    skipped.push(json!({"page": page.id, "id": first.id, "language": language}));
                }
                if language != LANGUAGE_NONE {
                    languages.insert(language);
                }
                continue;
            }
            for f in fields {
                checked += 1;
                for m in proof::misspellings(dict, &f.text, &doc.ignored_words) {
                    if misspellings.len() >= MAX_MISSES {
                        truncated = true;
                        break 'pages;
                    }
                    misspellings.push(json!({
                        "page": page.id, "id": f.id, "kind": f.kind, "field": f.field,
                        "word": m.word, "start": m.start, "end": m.end, "language": language,
                        "suggestions": dict.suggest(&m.word),
                    }));
                }
            }
        }
    }
    let notes: Vec<String> = languages.into_iter().map(|l| format!("No dictionary for {l}; its texts were not checked")).collect();
    Ok(json!({"checked": checked, "misspellings": misspellings, "skipped": skipped, "notes": notes, "truncated": truncated}))
}

fn suggest(_: &mut Session, v: &Value) -> CmdResult {
    let word = p::req_str(v, "word")?;
    let dict = Dictionary::en_us();
    Ok(json!({"word": word, "known": dict.knows(word), "suggestions": dict.suggest(word)}))
}

fn ignore(s: &mut Session, v: &Value) -> CmdResult {
    let word = p::req_str(v, "word")?.trim().replace('\u{2019}', "'").to_lowercase();
    if word.chars().count() > MAX_IGNORED_WORD || !word.chars().any(char::is_alphabetic) {
        return Err(EngineError::Params(format!("`word` must hold a letter and be at most {MAX_IGNORED_WORD} characters")));
    }
    let added = !s.doc.ignored_words.contains(&word);
    if added {
        if s.doc.ignored_words.len() >= MAX_IGNORED_WORDS {
            return Err(EngineError::Failed(format!("the document already ignores {MAX_IGNORED_WORDS} words")));
        }
        s.doc.ignored_words.push(word.clone());
    }
    Ok(json!({"word": word, "added": added, "ignored": s.doc.ignored_words}))
}

/// The language a shape is checked in, for menus: its own tag, else the document's.
pub fn language_of(doc: &Document, page: &Page, id: &str) -> String {
    let own = page.shape(id).map(|sh| sh.language.as_str()).unwrap_or_default();
    proof::effective_language(&doc.language, own).to_string()
}

fn set_language(s: &mut Session, v: &Value) -> CmdResult {
    let raw = p::req_str(v, "language")?;
    let tag = if raw.trim().is_empty() || raw.trim().eq_ignore_ascii_case("default") {
        String::new()
    } else {
        normalize_language(raw).ok_or_else(|| EngineError::Params(format!("`language` must be a BCP 47 tag such as en-US, none or default, not `{}`", short(raw))))?
    };
    let ids = match ids_param(v)? {
        Some(ids) => ids,
        None => s.sel.items.clone(),
    };
    let page_has = |id: &String| s.doc.page().is_some_and(|pg| pg.shape(id).is_some());
    let shapes: Vec<String> = ids.into_iter().filter(page_has).collect();
    let document = opt_bool(v, "document")?.unwrap_or(false) || shapes.is_empty();
    if document {
        s.doc.language = tag.clone();
        return with_sel(s, json!({"language": tag, "ids": Vec::<String>::new(), "document": true}));
    }
    if let Some(page) = s.doc.page_mut() {
        for id in &shapes {
            if let Some(sh) = page.shape_mut(id) {
                sh.language = tag.clone();
            }
        }
    }
    with_sel(s, json!({"language": tag, "ids": shapes, "document": false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::compartment::{Compartment, CompartmentKind};
    use vizcraft_diagram::{End, ShapeKind};

    fn run(s: &mut Session, id: &str, params: Value) -> Value {
        s.run(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// Page one holds a misspelling in a shape, a field, a URL, capitals and a connector label;
    /// page two holds one more.
    fn planted() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        a.text = "Send teh data to {{pagename}} at https://exmple.com/teh via NASA".into();
        let mut b = Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 60.0);
        b.text = "All fine here".into();
        let mut class = Compartment::new("attrs", CompartmentKind::List);
        class.title = Some("Atributes".into());
        class.rows = vec!["name".into(), "adress".into()];
        b.compartments.push(class);
        page.shapes.extend([a, b]);
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.text = "recieve".into();
        page.connectors.push(c);
        doc.pages.push(vizcraft_diagram::Page::new("p2", "Second"));
        doc.pages[1].shapes.push({
            let mut x = Shape::new("x", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
            x.text = "wrnog".into();
            x
        });
        Session::new(doc)
    }

    fn words(r: &Value) -> Vec<String> {
        r["misspellings"].as_array().unwrap().iter().map(|m| format!("{}:{}", m["id"].as_str().unwrap(), m["word"].as_str().unwrap())).collect()
    }

    #[test]
    fn check_flags_teh_with_the_among_suggestions_and_skips_fields_urls_and_capitals() {
        let mut s = planted();
        let r = run(&mut s, "proof.check", json!({}));
        assert_eq!(words(&r), ["a:teh", "b:Atributes", "b:adress", "c:recieve"]);
        let m = &r["misspellings"][0];
        assert_eq!((m["start"].as_u64(), m["end"].as_u64(), m["field"].as_str()), (Some(5), Some(8), Some("text")));
        assert!(m["suggestions"].as_array().unwrap().iter().any(|x| x == "the"));
        assert_eq!(r["misspellings"][2]["field"], "row:attrs:1");
        assert_eq!(r["misspellings"][1]["field"], "title:attrs");
        assert!(r["misspellings"][3]["suggestions"].as_array().unwrap().iter().any(|x| x == "receive"));
    }

    #[test]
    fn check_scopes_to_ids_a_page_or_the_document() {
        let mut s = planted();
        assert_eq!(words(&run(&mut s, "proof.check", json!({"ids": ["a"]}))), ["a:teh"]);
        assert_eq!(words(&run(&mut s, "proof.check", json!({"page": "Second"}))), ["x:wrnog"]);
        assert_eq!(words(&run(&mut s, "proof.check", json!({"page": 1}))), ["x:wrnog"]);
        assert_eq!(run(&mut s, "proof.check", json!({"all": true}))["misspellings"].as_array().unwrap().len(), 5);
        assert!(s.run("proof.check", &json!({"ids": ["nope"]})).is_err());
        assert!(s.run("proof.check", &json!({"page": "nope"})).is_err());
    }

    #[test]
    fn check_changes_nothing_and_makes_no_undo_step() {
        let mut s = planted();
        let before = s.doc.clone();
        run(&mut s, "proof.check", json!({"all": true}));
        run(&mut s, "proof.suggest", json!({"word": "teh"}));
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }

    #[test]
    fn language_none_skips_a_shape_and_other_languages_are_skipped_with_a_note() {
        let mut s = planted();
        run(&mut s, "proof.setLanguage", json!({"ids": ["a"], "language": "none"}));
        assert_eq!(s.doc.pages[0].shapes[0].language, "none");
        let r = run(&mut s, "proof.check", json!({}));
        assert_eq!(words(&r), ["b:Atributes", "b:adress", "c:recieve"]);
        assert_eq!(r["skipped"][0]["id"], "a");
        run(&mut s, "proof.setLanguage", json!({"ids": ["b"], "language": "fr-fr"}));
        assert_eq!(s.doc.pages[0].shapes[1].language, "fr-FR");
        let r = run(&mut s, "proof.check", json!({}));
        assert_eq!(words(&r), ["c:recieve"]);
        assert_eq!(r["notes"][0], "No dictionary for fr-FR; its texts were not checked");
        // The document default applies to shapes with no language of their own and to connectors.
        run(&mut s, "proof.setLanguage", json!({"language": "de-DE", "document": true}));
        assert_eq!(s.doc.language, "de-DE");
        assert!(words(&run(&mut s, "proof.check", json!({}))).is_empty());
        run(&mut s, "proof.setLanguage", json!({"ids": ["b"], "language": "default"}));
        assert_eq!(s.doc.pages[0].shapes[1].language, "");
        run(&mut s, "proof.setLanguage", json!({"ids": ["a", "b"], "language": "en-US"}));
        run(&mut s, "proof.setLanguage", json!({"language": "en-US", "document": true}));
        assert_eq!(words(&run(&mut s, "proof.check", json!({"ids": ["a", "b", "c"]}))).len(), 4);
        assert!(s.run("proof.setLanguage", &json!({"language": "not a tag"})).is_err());
    }

    #[test]
    fn set_language_defaults_to_the_selection_then_the_document() {
        let mut s = planted();
        run(&mut s, "edit.select", json!({"ids": ["b"]}));
        run(&mut s, "proof.setLanguage", json!({"language": "es-ES"}));
        assert_eq!(s.doc.pages[0].shapes[1].language, "es-ES");
        assert_eq!(language_of(&s.doc, &s.doc.pages[0], "b"), "es-ES");
        run(&mut s, "edit.select", json!({"ids": []}));
        run(&mut s, "proof.setLanguage", json!({"language": "fr-FR"}));
        assert_eq!(s.doc.language, "fr-FR");
        assert_eq!(language_of(&s.doc, &s.doc.pages[0], "a"), "fr-FR");
    }

    #[test]
    fn ignore_hides_the_word_everywhere_in_one_undo_step() {
        let mut s = planted();
        let r = run(&mut s, "proof.ignore", json!({"word": "Teh"}));
        assert_eq!((r["added"].as_bool(), r["word"].as_str()), (Some(true), Some("teh")));
        assert_eq!(run(&mut s, "proof.ignore", json!({"word": "teh"}))["added"], false);
        assert_eq!(s.doc.ignored_words, ["teh"]);
        assert!(!words(&run(&mut s, "proof.check", json!({}))).contains(&"a:teh".to_string()));
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.ignored_words.is_empty());
        for bad in [json!({"word": ""}), json!({"word": "123"}), json!({"word": "x".repeat(200)}), json!({})] {
            assert!(s.run("proof.ignore", &bad).is_err());
        }
    }

    #[test]
    fn suggest_reports_known_words_and_suggestions() {
        let mut s = planted();
        let r = run(&mut s, "proof.suggest", json!({"word": "teh"}));
        assert_eq!(r["known"], false);
        assert_eq!(r["suggestions"][0], "the");
        assert_eq!(run(&mut s, "proof.suggest", json!({"word": "the"}))["known"], true);
    }
}
