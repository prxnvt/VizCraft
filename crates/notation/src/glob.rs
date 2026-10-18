//! A small glob matcher for the `nameFormat` check.
//!
//! `*` matches any run of characters, `?` one character, `[abc]`, `[a-z]` and `[!0-9]` one
//! character of (or not of) a class, and `\` makes the next character literal. Matching is
//! linear in the name for each star (no backtracking beyond the last star), so a hostile pattern
//! cannot make it slow; patterns and names are also capped in length.

/// Longest pattern accepted, in characters.
pub const MAX_PATTERN: usize = 256;

/// Longest name matched, in characters; longer names never match.
pub const MAX_NAME: usize = 4_096;

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Any,
    One,
    Lit(char),
    Class { negated: bool, ranges: Vec<(char, char)> },
}

impl Token {
    fn accepts(&self, c: char) -> bool {
        match self {
            Token::Any | Token::One => true,
            Token::Lit(l) => *l == c,
            Token::Class { negated, ranges } => ranges.iter().any(|(a, b)| *a <= c && c <= *b) != *negated,
        }
    }
}

/// A parsed glob.
#[derive(Clone, Debug, PartialEq)]
pub struct Glob {
    tokens: Vec<Token>,
}

impl Glob {
    /// Parse `pattern`; `None` when it is empty, too long, has an unclosed class or ends in a
    /// lone `\`.
    pub fn parse(pattern: &str) -> Option<Glob> {
        if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN {
            return None;
        }
        let mut tokens = Vec::new();
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            tokens.push(match c {
                '*' => {
                    // Runs of stars are one star.
                    if tokens.last() == Some(&Token::Any) {
                        continue;
                    }
                    Token::Any
                }
                '?' => Token::One,
                '\\' => Token::Lit(chars.next()?),
                '[' => {
                    let mut negated = false;
                    let mut ranges = Vec::new();
                    let mut first = true;
                    loop {
                        let c = chars.next()?;
                        match c {
                            '!' if first => negated = true,
                            ']' if !first || !ranges.is_empty() => break,
                            _ => {
                                let lo = if c == '\\' { chars.next()? } else { c };
                                // A `-` between two characters makes a range; peeking needs a clone.
                                let mut ahead = chars.clone();
                                if ahead.next() == Some('-')
                                    && let Some(hi) = ahead.next()
                                    && hi != ']'
                                {
                                    chars = ahead;
                                    ranges.push((lo.min(hi), lo.max(hi)));
                                } else {
                                    ranges.push((lo, lo));
                                }
                            }
                        }
                        first = false;
                    }
                    Token::Class { negated, ranges }
                }
                c => Token::Lit(c),
            });
        }
        Some(Glob { tokens })
    }

    /// Whether the whole of `name` matches.
    pub fn matches(&self, name: &str) -> bool {
        let text: Vec<char> = name.chars().take(MAX_NAME.saturating_add(1)).collect();
        if text.len() > MAX_NAME {
            return false;
        }
        let (mut t, mut p) = (0usize, 0usize);
        // Where the last star was and the text position it is matching up to.
        let mut star: Option<(usize, usize)> = None;
        while t < text.len() {
            match (self.tokens.get(p), text.get(t)) {
                (Some(Token::Any), _) => {
                    star = Some((p, t));
                    p += 1;
                }
                (Some(tok), Some(&c)) if tok.accepts(c) => {
                    p += 1;
                    t += 1;
                }
                _ => match star {
                    Some((sp, st)) => {
                        p = sp + 1;
                        t = st + 1;
                        star = Some((sp, st + 1));
                    }
                    None => return false,
                },
            }
        }
        self.tokens.get(p..).is_some_and(|rest| rest.iter().all(|tok| *tok == Token::Any))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, n: &str) -> bool {
        Glob::parse(p).unwrap().matches(n)
    }

    #[test]
    fn globs_match_whole_names() {
        assert!(m("S*", "S12") && m("S*", "S") && !m("S*", "xS1"));
        assert!(m("P-???", "P-101") && !m("P-???", "P-1010"));
        assert!(m("[A-Z][a-z]*", "Pump") && !m("[A-Z][a-z]*", "pump"));
        assert!(m("[!0-9]*", "a1") && !m("[!0-9]*", "1a"));
        assert!(m("a\\*b", "a*b") && !m("a\\*b", "axb"));
        assert!(m("*.*", "a.b") && !m("*.*", "ab"));
        assert!(m("[]a]", "]") && m("[a-]", "-"));
        assert!(m("**x**", "abxcd"));
    }

    #[test]
    fn bad_patterns_are_refused_and_matching_is_bounded() {
        for bad in ["", "[abc", "a\\", "[!", &"x".repeat(MAX_PATTERN + 1)] {
            assert!(Glob::parse(bad).is_none(), "{bad}");
        }
        let g = Glob::parse(&"*a".repeat(100)).unwrap();
        let name = "a".repeat(MAX_NAME - 1) + "b";
        let start = std::time::Instant::now();
        assert!(!g.matches(&name));
        assert!(start.elapsed().as_secs() < 5, "{:?}", start.elapsed());
        assert!(!Glob::parse("*").unwrap().matches(&"a".repeat(MAX_NAME + 1)));
    }
}
