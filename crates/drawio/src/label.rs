//! Label text. With `html=1` a cell's value is an HTML fragment; these helpers turn it into plain
//! text with line breaks and back. This is a deliberately small tag stripper, not an HTML
//! parser: it keeps the words and line structure, which is what a diagram label needs.

/// Turns an HTML label into plain text: tags are removed, `<br>` and block elements (`div`, `p`,
/// `li`, headings, table rows) become line breaks, named and numeric character references are
/// decoded, and runs of source whitespace collapse to one space as a browser would show them.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    // Text inside script and style elements is never shown.
    let mut skip_until: Option<&'static str> = None;
    // Once no `>` remains, later `<` cannot open a tag; remembering that keeps hostile input
    // such as thousands of `<` linear instead of quadratic.
    let mut no_close = false;
    while !rest.is_empty() {
        if let Some(end_tag) = skip_until {
            match find_ci(rest, end_tag) {
                Some(i) => {
                    rest = rest.get(i..).unwrap_or("");
                    skip_until = None;
                }
                None => break,
            }
            continue;
        }
        let Some(c) = rest.chars().next() else { break };
        if c == '<' && !no_close {
            if let Some(after) = rest.strip_prefix("<!--") {
                rest = after.find("-->").and_then(|i| after.get(i + 3..)).unwrap_or("");
                continue;
            }
            let Some(close) = rest.find('>') else {
                no_close = true;
                // A lone `<` with no closing bracket is text, not a tag.
                push_space_aware(&mut out, '<');
                rest = rest.get(1..).unwrap_or("");
                continue;
            };
            let tag = rest.get(1..close).unwrap_or("");
            rest = rest.get(close + 1..).unwrap_or("");
            let closing = tag.starts_with('/');
            let name: String =
                tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect();
            match name.as_str() {
                "br" => out.push('\n'),
                "div" | "p" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "table" | "blockquote" | "pre" => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                "script" if !closing => skip_until = Some("</script"),
                "style" if !closing => skip_until = Some("</style"),
                _ => {}
            }
            continue;
        }
        if c == '&' {
            let (decoded, used) = decode_entity(rest);
            match decoded {
                Some(ch) => {
                    out.push(ch);
                    rest = rest.get(used..).unwrap_or("");
                }
                None => {
                    out.push('&');
                    rest = rest.get(1..).unwrap_or("");
                }
            }
            continue;
        }
        push_space_aware(&mut out, c);
        rest = rest.get(c.len_utf8()..).unwrap_or("");
    }
    // Spaces next to line breaks and trailing breaks come from markup, not from the text.
    let lines: Vec<&str> = out.split('\n').map(|l| l.trim_matches(' ')).collect();
    let mut text = lines.join("\n");
    while text.ends_with('\n') {
        text.pop();
    }
    text
}

/// Pushes `c`, collapsing ASCII whitespace runs to one space.
fn push_space_aware(out: &mut String, c: char) {
    if c.is_ascii_whitespace() {
        if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
            out.push(' ');
        }
    } else {
        out.push(c);
    }
}

/// Case-insensitive ASCII search.
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h.get(i..i + n.len()).is_some_and(|w| w.eq_ignore_ascii_case(n)))
}

/// Decodes a character reference at the start of `s` (which begins with `&`). Returns the
/// character and the bytes consumed, or `None` when it is not a reference we know.
fn decode_entity(s: &str) -> (Option<char>, usize) {
    // References are short; looking further would let a stray `&` scan a whole label.
    let window = s.get(1..).unwrap_or("");
    let Some(semi) = window.char_indices().take(12).find(|(_, c)| *c == ';').map(|(i, _)| i) else {
        return (None, 0);
    };
    let body = window.get(..semi).unwrap_or("");
    let used = semi + 2;
    if let Some(num) = body.strip_prefix('#') {
        let value = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            num.parse::<u32>().ok()
        };
        return match value {
            Some(v) => (Some(char::from_u32(v).unwrap_or('\u{fffd}')), used),
            None => (None, 0),
        };
    }
    let ch = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        // A non-breaking space reads as a space in plain text.
        "nbsp" => ' ',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "trade" => '\u{2122}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "bull" => '\u{2022}',
        "middot" => '\u{b7}',
        "deg" => '\u{b0}',
        "times" => '\u{d7}',
        "divide" => '\u{f7}',
        "plusmn" => '\u{b1}',
        "euro" => '\u{20ac}',
        "pound" => '\u{a3}',
        "yen" => '\u{a5}',
        "cent" => '\u{a2}',
        "sect" => '\u{a7}',
        "para" => '\u{b6}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "larr" => '\u{2190}',
        "rarr" => '\u{2192}',
        "uarr" => '\u{2191}',
        "darr" => '\u{2193}',
        "le" => '\u{2264}',
        "ge" => '\u{2265}',
        "ne" => '\u{2260}',
        _ => return (None, 0),
    };
    (Some(ch), used)
}

/// Escapes plain text for an HTML label: `&`, `<`, `>` and `"` become references and line breaks
/// become `<br>`, the reverse of [`html_to_text`] for text without markup.
pub fn text_to_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("<br>");
            }
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
    out
}
