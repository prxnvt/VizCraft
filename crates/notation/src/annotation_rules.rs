//! The checks of the `annotation` pack (geometric tolerancing, welding and surface texture
//! symbols) behind [`Check::Annotation`].
//!
//! A symbol's conventions come from its own `standard` property when set, else from the variant
//! of the page it is drawn on (`annotation.asme` is the ASME family), else ISO. Messages state only
//! the finding.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vizcraft_analysis::mech::text;
use vizcraft_diagram::Document;
use vizcraft_model::{Element, Issue};

use crate::Catalog;
use crate::infer::Facts;
use crate::mech_rules::{finding, label, views};
use crate::schema::{Pack, RuleDecl};

/// The checks of `Check::Annotation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnotationTest {
    /// A feature control frame naming a datum no datum feature symbol carries.
    DatumsResolve,
    /// A datum letter that is I, O or Q, not one or two capital letters, or used twice.
    DatumLetters,
    /// A tolerance that is not a number greater than zero.
    ToleranceValue,
    /// A form tolerance (straightness, flatness, circularity, cylindricity) with datums.
    FormNoDatum,
    /// An orientation, location or runout tolerance with no datum.
    DatumRequired,
    /// Concentricity or symmetry where ASME Y14.5-2018 is in effect.
    RetiredCharacteristic,
    /// A material condition modifier on a feature that is not a feature of size.
    ModifierOnSize,
    /// A fillet weld with no size.
    WeldSize,
    /// A welding process reference number that is not in the ISO 4063 table.
    ProcessNumber,
    /// A surface texture requirement with no value, a value of zero or less, or an unknown
    /// parameter.
    TextureValue,
    /// A test this build does not know (from a newer pack); skipped.
    #[default]
    #[serde(other)]
    Unknown,
}

impl AnnotationTest {
    /// Whether this build knows the test.
    pub fn known(self) -> bool {
        self != AnnotationTest::Unknown
    }
}

/// The element types of the pack.
pub const FCF: &str = "annotation.FeatureControlFrame";
/// The datum feature symbol.
pub const DATUM: &str = "annotation.DatumFeatureSymbol";
/// The welding symbol.
pub const WELD: &str = "annotation.WeldingSymbol";
/// The surface texture symbol.
pub const TEXTURE: &str = "annotation.SurfaceTexture";
/// The ASME variant id.
pub const ASME_VARIANT: &str = "annotation.asme";

/// Characteristics that control form only and take no datum.
const FORM: &[&str] = &["straightness", "flatness", "circularity", "cylindricity"];

/// Characteristics that need a datum: orientation, runout and the two ASME retired ones.
const NEEDS_DATUM: &[&str] = &["angularity", "perpendicularity", "parallelism", "circularRunout", "totalRunout", "concentricity", "symmetry"];

/// Welding process reference numbers from ISO 4063 this build knows (a partial table: an
/// unknown number is only a warning).
pub const ISO_4063: &[&str] = &[
    "1", "11", "111", "112", "114", "12", "121", "122", "123", "124", "125", "13", "131", "132", "133", "135", "136", "138", "14", "141", "142",
    "143", "145", "146", "147", "15", "151", "152", "153", "154", "155", "18", "185", "2", "21", "22", "221", "222", "225", "23", "24", "241", "242",
    "25", "29", "3", "31", "311", "312", "313", "4", "41", "42", "44", "441", "45", "47", "48", "5", "51", "511", "512", "52", "521", "522", "7",
    "71", "72", "73", "74", "75", "753", "78", "782", "783", "784", "785", "786", "787", "788", "8", "81", "82", "821", "822", "83", "84", "86",
    "87", "9", "91", "912", "913", "94", "942", "943", "944", "945", "946", "947", "948", "949", "95", "951", "952", "953",
];

/// Surface texture parameters (profile, waviness and primary) a requirement may name.
const PARAMETERS: &[&str] = &[
    "Ra", "Rz", "Rq", "Rt", "Rp", "Rv", "Rsk", "Rku", "Rsm", "Rmr", "Rdq", "Wa", "Wz", "Wq", "Wt", "Wp", "Wsm", "Pa", "Pz", "Pq", "Pt", "Pp", "Psm",
    "Sa", "Sq", "Sz",
];

/// Whether `e`, drawn on the document's pages, follows the ASME family: its own `standard` is an
/// ASME one, or it has none and a page drawing it uses the ASME variant.
fn asme(doc: &Document, e: &Element) -> bool {
    match text(&e.properties, "standard") {
        "" => doc.pages.iter().any(|p| {
            p.view.as_ref().is_some_and(|v| v.variant == ASME_VARIANT) && p.shapes.iter().any(|s| s.element.as_deref() == Some(e.id.as_str()))
        }),
        s => s.starts_with("asme") || s == "aws",
    }
}

/// The datum letters a frame names: each `|`-separated reference, a composite `A-B` giving both
/// letters, modifiers in brackets or lower case dropped.
pub fn frame_datums(datums: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in datums.split('|').take(8) {
        for letter in part.split('-').take(4) {
            let core: String = letter.trim().chars().take_while(|c| *c != '(' && *c != '[').filter(char::is_ascii_uppercase).collect();
            if !core.is_empty() && !out.contains(&core) {
                out.push(core);
            }
        }
    }
    out
}

/// The datum letter a datum symbol carries: its `letter`, else its name.
fn letter(e: &Element) -> String {
    let l = text(&e.properties, "letter");
    if l.is_empty() { e.name.trim().to_string() } else { l.to_string() }
}

/// The number in a texture requirement (`Ra 1.6` gives 1.6, `63` gives 63).
fn texture_number(value: &str) -> Option<f64> {
    value.split_whitespace().filter_map(|w| w.trim_start_matches(['<', '>', '=', '≤', '≥']).parse::<f64>().ok()).next_back()
}

pub(crate) fn check(doc: &Document, cat: &Catalog, _pack: &Pack, rule: &RuleDecl, test: AnnotationTest, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let views = views(doc);
    // Only symbols that are drawn are checked; a symbol in no view is not on any drawing.
    let drawn = |e: &&Element| views.contains_key(e.id.as_str());
    let of = |t: &str| -> Vec<&Element> { doc.model.elements.iter().filter(|e| facts.is_a(&e.type_id, t)).filter(drawn).collect() };
    let frames = of(FCF);
    let mut push = |what: String, subjects: Vec<String>| out.push(finding(doc, &views, rule, &what, subjects, None));
    match test {
        AnnotationTest::DatumsResolve => {
            let known: HashSet<String> = of(DATUM).iter().map(|d| letter(d)).collect();
            for f in &frames {
                for d in frame_datums(text(&f.properties, "datums")) {
                    if !known.contains(&d) {
                        push(format!("{} refers to datum {d}, which no datum feature symbol carries.", label(cat, f)), vec![f.id.clone()]);
                    }
                }
            }
        }
        AnnotationTest::DatumLetters => {
            let mut used: HashMap<String, Vec<&Element>> = HashMap::new();
            for d in of(DATUM) {
                let l = letter(d);
                let ok = (1..=2).contains(&l.chars().count()) && l.chars().all(|c| c.is_ascii_uppercase()) && !l.contains(['I', 'O', 'Q']);
                if !ok {
                    push(
                        format!(
                            "{} has datum letter \"{}\"; use one or two capital letters other than I, O and Q.",
                            label(cat, d),
                            crate::rules::cut(&l, 20)
                        ),
                        vec![d.id.clone()],
                    );
                }
                used.entry(l).or_default().push(d);
            }
            let mut twice: Vec<(&String, &Vec<&Element>)> = used.iter().filter(|(l, v)| v.len() > 1 && !l.is_empty()).collect();
            twice.sort_by_key(|(l, _)| (*l).clone());
            for (l, v) in twice {
                push(
                    format!("Datum letter {l} is used by {} datum feature symbols; each datum has its own letter.", v.len()),
                    v.iter().map(|d| d.id.clone()).collect(),
                );
            }
        }
        AnnotationTest::ToleranceValue => {
            for f in &frames {
                let t = text(&f.properties, "tolerance");
                let n = t.trim_start_matches(['Ø', '⌀', 'S']).trim().parse::<f64>().ok().filter(|n| n.is_finite());
                if n.is_none_or(|n| n <= 0.0) {
                    let shown = if t.is_empty() { "empty".to_string() } else { format!("\"{}\"", crate::rules::cut(t, 20)) };
                    push(format!("{} has tolerance {shown}; give a number greater than zero.", label(cat, f)), vec![f.id.clone()]);
                }
            }
        }
        AnnotationTest::FormNoDatum => {
            for f in &frames {
                let ch = text(&f.properties, "characteristic");
                if FORM.contains(&ch) && !frame_datums(text(&f.properties, "datums")).is_empty() {
                    push(format!("{} controls {ch}, a form tolerance, which takes no datum.", label(cat, f)), vec![f.id.clone()]);
                }
            }
        }
        AnnotationTest::DatumRequired => {
            for f in &frames {
                let ch = text(&f.properties, "characteristic");
                if NEEDS_DATUM.contains(&ch) && frame_datums(text(&f.properties, "datums")).is_empty() {
                    push(format!("{} controls {ch}, which needs at least one datum.", label(cat, f)), vec![f.id.clone()]);
                }
            }
        }
        AnnotationTest::RetiredCharacteristic => {
            for f in &frames {
                let ch = text(&f.properties, "characteristic");
                if matches!(ch, "concentricity" | "symmetry") && asme(doc, f) {
                    push(format!("{} uses {ch}, which ASME Y14.5-2018 no longer has; use position or profile.", label(cat, f)), vec![f.id.clone()]);
                }
            }
        }
        AnnotationTest::ModifierOnSize => {
            for f in &frames {
                let modifier = text(&f.properties, "modifier");
                let size = vizcraft_analysis::mech::flag(&f.properties, "featureOfSize").unwrap_or(false);
                if matches!(modifier, "mmc" | "lmc") && !size {
                    let which = if modifier == "mmc" { "maximum" } else { "least" };
                    push(
                        format!("{} has a {which} material condition modifier on a feature that is not a feature of size.", label(cat, f)),
                        vec![f.id.clone()],
                    );
                }
            }
        }
        AnnotationTest::WeldSize => {
            for w in of(WELD) {
                let fillet = ["arrowSide", "otherSide"].iter().any(|k| text(&w.properties, k) == "fillet");
                if fillet && text(&w.properties, "size").is_empty() {
                    push(format!("{} is a fillet weld with no size.", label(cat, w)), vec![w.id.clone()]);
                }
            }
        }
        AnnotationTest::ProcessNumber => {
            for w in of(WELD) {
                let tail = text(&w.properties, "tail");
                let first = tail.split([' ', '/', ',']).next().unwrap_or_default();
                if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) && !ISO_4063.contains(&first) {
                    push(
                        format!("{} names welding process {first}, which is not an ISO 4063 reference number this build knows.", label(cat, w)),
                        vec![w.id.clone()],
                    );
                }
            }
        }
        AnnotationTest::TextureValue => {
            for t in of(TEXTURE) {
                let value = text(&t.properties, "value");
                let param = value.split_whitespace().next().filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_alphabetic()));
                let what = if value.is_empty() {
                    Some(format!("{} has no requirement value.", label(cat, t)))
                } else if param.is_some_and(|p| !PARAMETERS.contains(&p)) {
                    Some(format!(
                        "{} names parameter {}, which is not a surface texture parameter.",
                        label(cat, t),
                        crate::rules::cut(param.unwrap_or_default(), 12)
                    ))
                } else if texture_number(value).is_none_or(|n| n <= 0.0) {
                    Some(format!("{} has requirement \"{}\" with no value greater than zero.", label(cat, t), crate::rules::cut(value, 24)))
                } else {
                    None
                };
                if let Some(what) = what {
                    push(what, vec![t.id.clone()]);
                }
            }
        }
        AnnotationTest::Unknown => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datum_references_read_composites_and_modifiers() {
        assert_eq!(frame_datums("A|B|C"), ["A", "B", "C"]);
        assert_eq!(frame_datums("A-B | C(M)"), ["A", "B", "C"]);
        assert_eq!(frame_datums(""), Vec::<String>::new());
        assert_eq!(texture_number("Ra 1.6"), Some(1.6));
        assert_eq!(texture_number("63"), Some(63.0));
        assert_eq!(texture_number("Ra"), None);
        assert!(ISO_4063.contains(&"135") && !ISO_4063.contains(&"999"));
    }
}
