//! Consistency checks on a loaded pack.

use crate::schema::Pack;

/// Problems with `pack`, in plain words; empty when it is consistent. Checks that every id is
/// qualified with the pack's own prefix (or another pack's, for references), that ids are unique,
/// that every reference (glyph, type, relationship, scope, viewpoint) resolves, that colours parse,
/// outlines and arrows are known names, sizes are positive and finite, formats parse, and that
/// rules use known checks.
pub fn check(pack: &Pack) -> Vec<String> {
    // Contract stub: the pack id only. The c4a-notation piece adds the checks listed above.
    let mut out = Vec::new();
    if pack.id.is_empty() {
        out.push("the pack has no id".to_string());
    }
    out
}
