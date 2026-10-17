//! Reading one newline-terminated line with a size cap.
//!
//! `BufRead::read_line` keeps allocating until it sees a newline, so a peer that never sends one
//! could exhaust memory. Both ends of the server (the client on stdin, the app on the control
//! socket) are read through this instead.

use std::io::{self, BufRead};

/// What one read produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// A complete line without its terminator (a final line without a newline counts too).
    Text(String),
    /// A line longer than the cap. It was read to its end and dropped.
    TooLong,
    /// A line that is not UTF-8.
    NotUtf8,
}

/// Read the next line, keeping at most `max` bytes. `Ok(None)` at end of input.
pub(crate) fn read_bounded(reader: &mut impl BufRead, max: usize) -> io::Result<Option<Line>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut seen_any = false;
    let mut too_long = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            if !seen_any {
                return Ok(None);
            }
            break;
        }
        seen_any = true;
        let newline = available.iter().position(|&b| b == b'\n');
        let take = newline.unwrap_or(available.len());
        let chunk = available.get(..take).unwrap_or_default();
        if !too_long {
            if buf.len().saturating_add(chunk.len()) > max {
                // Keep reading to the newline so the next line starts in the right place, but
                // stop storing: that is the point of the cap.
                too_long = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(chunk);
            }
        }
        let consumed = newline.map_or(take, |i| i.saturating_add(1));
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if too_long {
        return Ok(Some(Line::TooLong));
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(match String::from_utf8(buf) {
        Ok(s) => Line::Text(s),
        Err(_) => Line::NotUtf8,
    }))
}
