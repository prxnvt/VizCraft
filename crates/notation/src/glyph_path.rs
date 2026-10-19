//! Path glyph outlines: the small path language of `Glyph.path` and `Glyph.detail`.
//!
//! The data is a subset of SVG path data in a unit box (`0 0` to `1 1`): the absolute commands
//! `M` (move), `L` (line), `C` (cubic), `Q` (quadratic, turned into a cubic) and `Z` (close),
//! numbers separated by spaces or commas, and a command letter that may be left out when it
//! repeats (after `M` the repeats are lines, as in SVG). Text from a pack file is hostile: the
//! length and the command count are capped, every number must be finite and within a margin of
//! the box, and anything else is an error that names the place, never a panic.

use vizcraft_diagram::geom::{Path, PathEl, Point};

/// Longest path text accepted, in bytes.
pub const MAX_PATH_TEXT: usize = 16 * 1024;

/// Most path commands one glyph path may produce.
pub const MAX_PATH_ELS: usize = 1024;

/// How far outside the unit box a coordinate may reach (a curve's control points may overshoot
/// the box a little).
const MARGIN: f64 = 1.0;

/// Why path data could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The text is longer than [`MAX_PATH_TEXT`].
    #[error("path data is longer than {MAX_PATH_TEXT} bytes")]
    TooLong,
    /// More than [`MAX_PATH_ELS`] commands.
    #[error("path data has more than {MAX_PATH_ELS} commands")]
    TooMany,
    /// Something that is not a command or a number, at a byte offset.
    #[error("unexpected `{0}` at byte {1}")]
    Unexpected(char, usize),
    /// A number that does not parse, is not finite or lies too far outside the unit box.
    #[error("bad number `{0}`")]
    BadNumber(String),
    /// A command whose numbers run out, at a byte offset.
    #[error("`{0}` needs more numbers (byte {1})")]
    Short(char, usize),
    /// A line, curve or close before any `M`.
    #[error("path data must start with M")]
    NoStart,
    /// Nothing drawn.
    #[error("path data draws nothing")]
    Empty,
}

/// One token: a command letter or a number, with its byte offset.
enum Tok {
    Cmd(char, usize),
    Num(f64),
}

fn tokens(text: &str) -> Result<Vec<Tok>, PathError> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else if matches!(c, 'M' | 'L' | 'C' | 'Q' | 'Z' | 'z') {
            chars.next();
            out.push(Tok::Cmd(c.to_ascii_uppercase(), i));
        } else if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') {
            let start = i;
            let mut end = i;
            let mut seen_e = false;
            while let Some(&(j, d)) = chars.peek() {
                // A sign starts a new number unless it follows an exponent.
                let sign_ok = j == start || (seen_e && text.get(..j).is_some_and(|t| t.ends_with(['e', 'E'])));
                if d.is_ascii_digit() || d == '.' || (matches!(d, '-' | '+') && sign_ok) || (matches!(d, 'e' | 'E') && !seen_e) {
                    seen_e |= matches!(d, 'e' | 'E');
                    end = j + d.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = text.get(start..end).unwrap_or_default();
            let v: f64 = word.parse().map_err(|_| PathError::BadNumber(word.chars().take(32).collect()))?;
            if !v.is_finite() || !(-MARGIN..=1.0 + MARGIN).contains(&v) {
                return Err(PathError::BadNumber(word.chars().take(32).collect()));
            }
            out.push(Tok::Num(v));
        } else {
            return Err(PathError::Unexpected(c, i));
        }
        if out.len() > MAX_PATH_ELS * 8 {
            return Err(PathError::TooMany);
        }
    }
    Ok(out)
}

/// Read path data in the unit box into a [`Path`]. See the module docs for the language.
pub fn parse(text: &str) -> Result<Path, PathError> {
    if text.len() > MAX_PATH_TEXT {
        return Err(PathError::TooLong);
    }
    let toks = tokens(text)?;
    let mut path = Path::new();
    let mut i = 0usize;
    let mut cmd: Option<(char, usize)> = None;
    let mut pen: Option<Point> = None;
    let mut start = Point::ZERO;
    while i < toks.len() {
        // A number where a command is due repeats the last command.
        let (c, at) = match toks.get(i) {
            Some(Tok::Cmd(c, at)) => {
                i += 1;
                (*c, *at)
            }
            Some(Tok::Num(_)) => match cmd {
                Some(('M', at)) => ('L', at),
                Some((c, at)) if c != 'Z' => (c, at),
                _ => return Err(PathError::NoStart),
            },
            None => break,
        };
        let need = match c {
            'M' | 'L' => 2,
            'C' => 6,
            'Q' => 4,
            _ => 0,
        };
        let mut nums = [0.0f64; 6];
        for slot in nums.iter_mut().take(need) {
            match toks.get(i) {
                Some(Tok::Num(v)) => *slot = *v,
                _ => return Err(PathError::Short(c, at)),
            }
            i += 1;
        }
        let pt = |k: usize| Point::new(nums.get(k).copied().unwrap_or(0.0), nums.get(k + 1).copied().unwrap_or(0.0));
        match c {
            'M' => {
                path.els.push(PathEl::MoveTo(pt(0)));
                start = pt(0);
                pen = Some(pt(0));
            }
            'L' => {
                pen.ok_or(PathError::NoStart)?;
                path.els.push(PathEl::LineTo(pt(0)));
                pen = Some(pt(0));
            }
            'C' => {
                pen.ok_or(PathError::NoStart)?;
                path.els.push(PathEl::CubicTo(pt(0), pt(2), pt(4)));
                pen = Some(pt(4));
            }
            'Q' => {
                let p0 = pen.ok_or(PathError::NoStart)?;
                let (q, p) = (pt(0), pt(2));
                // The cubic with the same curve: control points two thirds of the way to `q`.
                path.els.push(PathEl::CubicTo(p0 + (q - p0) * (2.0 / 3.0), p + (q - p) * (2.0 / 3.0), p));
                pen = Some(p);
            }
            _ => {
                pen.ok_or(PathError::NoStart)?;
                path.els.push(PathEl::Close);
                pen = Some(start);
            }
        }
        cmd = Some((c, at));
        if path.els.len() > MAX_PATH_ELS {
            return Err(PathError::TooMany);
        }
    }
    if !path.els.iter().any(|e| matches!(e, PathEl::LineTo(_) | PathEl::CubicTo(..))) {
        return Err(PathError::Empty);
    }
    Ok(path)
}

/// A path glyph's whole drawing: the outline subpaths of `path` as written, then the `detail`
/// lines with any `Z` dropped so they are never filled. Either part may be empty text; both
/// empty, or either malformed, is an error.
pub fn glyph_outline(path: &str, detail: &str) -> Result<Path, PathError> {
    let mut out = if path.trim().is_empty() { Path::new() } else { parse(path)? };
    if !detail.trim().is_empty() {
        out.els.extend(parse(detail)?.els.into_iter().filter(|e| !matches!(e, PathEl::Close)));
    }
    if out.els.len() > MAX_PATH_ELS {
        return Err(PathError::TooMany);
    }
    if out.is_empty() {
        return Err(PathError::Empty);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_parse_into_absolute_path_elements() {
        let p = parse("M0 0 L1 0 L1,1 Z M .25 .25 Q 0.5 0 0.75 0.25 C 1 0.5 0.5 1 0.25 0.75").unwrap();
        assert_eq!(p.els.first(), Some(&PathEl::MoveTo(Point::ZERO)));
        assert_eq!(p.els.iter().filter(|e| matches!(e, PathEl::Close)).count(), 1);
        assert_eq!(p.els.iter().filter(|e| matches!(e, PathEl::CubicTo(..))).count(), 2, "Q becomes a cubic");
        // Repeated numbers repeat the command; after M they are lines.
        let r = parse("M0 0 1 0 1 1z").unwrap();
        assert_eq!(r.els.len(), 4);
        assert!(matches!(r.els[1], PathEl::LineTo(_)) && r.els[3] == PathEl::Close);
        assert_eq!(parse("M0,0L1e-1,-0.5").unwrap().els[1], PathEl::LineTo(Point::new(0.1, -0.5)));
    }

    #[test]
    fn detail_lines_are_never_closed() {
        let g = glyph_outline("M0 0 L1 0 L1 1 Z", "M0 0.5 L1 0.5 Z").unwrap();
        assert_eq!(g.els.iter().filter(|e| matches!(e, PathEl::Close)).count(), 1);
        assert!(glyph_outline("", "M0 0 L1 1").is_ok(), "lines alone are a drawing too");
        assert_eq!(glyph_outline("", " "), Err(PathError::Empty));
    }

    #[test]
    fn junk_is_an_error_never_a_panic() {
        for junk in [
            "",
            "Z",
            "L1 1",
            "M",
            "M0",
            "M0 0",
            "M0 0 L",
            "M0 0 C1 1 1",
            "M0 0 X1 1",
            "M0 0 L1e400 0",
            "M0 0 L9 0",
            "M0 0 LNaN 0",
            "M0 0 L1-",
            "--",
            "M0 0 L1 1 ü",
            "\u{0}",
        ] {
            assert!(parse(junk).is_err(), "{junk:?}");
        }
        assert_eq!(parse(&"M0 0 L1 1 ".repeat(5000)), Err(PathError::TooLong));
        assert_eq!(parse(&"L1 1 ".repeat(1500).replacen("L", "M0 0 L", 1)), Err(PathError::TooMany));
    }
}
