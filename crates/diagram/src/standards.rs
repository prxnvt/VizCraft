//! The project's standard choices: which symbol family it draws with and which drawing sheet its
//! pages use. Templates write them when a document is made (`template.new`) and read them when a
//! page is added (`template.addPage`), so a project keeps the choice it was started with; the
//! sheet commands read the sheet as their default. Empty means the default: IEC symbols on an ISO
//! A3 landscape sheet. Every value is a choice the user makes; nothing here is fixed.

use serde::{Deserialize, Serialize};

/// The symbol family when none was chosen.
pub const DEFAULT_SYMBOLS: &str = "iec";

/// The sheet preset when none was chosen.
pub const DEFAULT_SHEET: &str = "iso.a3.landscape";

/// Longest value either field keeps, in bytes.
pub const MAX_STANDARD: usize = 64;

/// A project's standard choices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Standards {
    /// The symbol family (`iec`, `ansi`, `jic`); a pack variant named `<pack>.<family>` draws it.
    /// Empty: [`DEFAULT_SYMBOLS`].
    #[serde(skip_serializing_if = "String::is_empty")]
    pub symbols: String,
    /// The sheet preset, `<standard>.<size>.<orientation>` (`iso.a3.landscape`). Empty:
    /// [`DEFAULT_SHEET`].
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sheet: String,
}

impl Standards {
    /// The symbol family in effect.
    pub fn symbols(&self) -> &str {
        if self.symbols.is_empty() { DEFAULT_SYMBOLS } else { &self.symbols }
    }
    /// The sheet preset in effect.
    pub fn sheet(&self) -> &str {
        if self.sheet.is_empty() { DEFAULT_SHEET } else { &self.sheet }
    }
    /// Both fields trimmed and cut to [`MAX_STANDARD`] bytes at a character boundary; a value
    /// with control characters is cleared, so it reads as the default.
    pub fn sanitize(&mut self) {
        for v in [&mut self.symbols, &mut self.sheet] {
            let t = v.trim();
            let mut end = t.len().min(MAX_STANDARD);
            while !t.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            let cut = t.get(..end).unwrap_or_default();
            *v = if cut.chars().any(char::is_control) { String::new() } else { cut.to_string() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reads_as_the_defaults_and_sanitize_caps_and_clears() {
        let mut s = Standards::default();
        assert_eq!((s.symbols(), s.sheet()), ("iec", "iso.a3.landscape"));
        s.symbols = format!("  {}", "\u{e9}".repeat(40));
        s.sheet = "ansi\u{0}.b".into();
        s.sanitize();
        assert!(s.symbols.len() <= MAX_STANDARD && s.symbols.starts_with('\u{e9}'));
        assert_eq!(s.sheet, "");
        let json = serde_json::to_string(&Standards::default()).unwrap();
        assert_eq!(json, "{}");
    }
}
