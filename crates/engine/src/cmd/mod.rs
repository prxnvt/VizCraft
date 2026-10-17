//! Command implementations, one module per menu: each exposes `specs()`.

pub mod edit;
pub mod file;
pub mod help;
pub mod insert;
pub mod view;

use serde_json::json;

use crate::{CmdResult, Registry, Session};

/// The full registry, in menu order.
pub fn registry() -> Registry {
    let mut v = Vec::new();
    v.extend(file::specs());
    v.extend(edit::specs());
    v.extend(view::specs());
    v.extend(insert::specs());
    v.extend(help::specs());
    Registry::new(v)
}

/// The standard result: the selection after the command.
pub fn sel_result(s: &Session) -> CmdResult {
    Ok(json!({
        "primary": s.sel.primary,
        "items": s.sel.items,
        "range": s.sel.range,
    }))
}

/// ISO-8601 timestamp (UTC, seconds), without a date-time dependency.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 to (year, month, day): Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
