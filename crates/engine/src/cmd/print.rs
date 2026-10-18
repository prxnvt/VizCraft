//! Printing, headers and footers: `file.print` and `page.setHeaderFooter`, plus the page list
//! that `file.export` and `file.print` share ([`resolve_pages`]).
//!
//! `file.print` lays the pages out on paper (`vizcraft_formats::pdf::plan`), writes a print-ready
//! PDF with the non-printing shapes and layers left out, and asks the front end to hand it to the
//! operating system by pushing `{"print": path}` to `ui_requests`. It never opens a dialog: the
//! desktop app sends the file to the printer (see `apps/vizcraft`), and a session without a front
//! end (CLI, tests) only writes the file. Print Preview is the UI's `ui.print`.

use serde_json::{Value, json};
use vizcraft_diagram::HeaderFooter;
pub use vizcraft_formats::pdf::Plan;
use vizcraft_formats::pdf::{self, MAX_TILES, PrintLayout};
use vizcraft_formats::svg::SvgOptions;

use super::{opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Most page references in a `pages` list.
pub const MAX_PAGE_REFS: usize = 1024;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("file.print", "Print", "File", print).params(
            r#"{"pages"?: "active"|"all"|[id|name|index], "fit"?: bool, "tiles"?: [cols, rows], "paper"?: "page"|"letter"|"legal"|"tabloid"|"a4"|"a3"|{"width", "height"}, "path"?: string, "send"?: bool} -> {path, sheets, pages: [{page, cols, rows, scale}], sent} (a print-ready PDF without non-printing shapes; a drawing larger than the paper is tiled unless `fit`; `tiles` spreads each page over that grid; paper defaults to the page size and turns to match the page; the PDF goes to `path` or a temporary file; `send` (default false; the Print button sets it) hands it to the operating system from the desktop app; pages default to every foreground page)"#,
        ),
        CommandSpec::new("page.setHeaderFooter", "Header and Footer", "Design > Page Setup", set_header_footer).params(
            r#"{"page"?: id|name|index, "all"?: bool, "header"?: {"left"?, "center"?, "right"?}, "footer"?: {...}, "margin"?: number|null} -> {pages, header, footer} (slots not given keep their text; fields {page}, {pages}, {pageName}, {title}, {date} are expanded on export and print; `margin` is the distance from the page edges in page units, 0 to 500, null for the default quarter inch, applied to both)"#,
        ),
    ]
}

/// Today's date, `YYYY-MM-DD`, for the `{date}` field.
pub fn today() -> String {
    super::now_iso().get(..10).unwrap_or_default().to_string()
}

/// The foreground pages (not background pages), or every page when all are background pages.
fn foreground(s: &Session) -> Vec<usize> {
    let fg: Vec<usize> = s.doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background).map(|(i, _)| i).collect();
    if fg.is_empty() { (0..s.doc.pages.len()).collect() } else { fg }
}

/// One page reference (id, name or index as a string or number) as an index.
fn page_ref(s: &Session, v: &Value) -> Result<usize, EngineError> {
    match v {
        Value::String(r) => s.doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r)))),
        Value::Number(n) => n
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| i < s.doc.pages.len())
            .ok_or_else(|| EngineError::Params(format!("no page {n}"))),
        _ => Err(EngineError::Params("a page must be a page id, name or index".into())),
    }
}

/// The pages the `pages` param names (`"active"`, `"all"` for every foreground page, or a list of
/// references, kept in order without repeats), else the `page` param, else `default`. Naming both
/// is an error.
pub fn resolve_pages(s: &Session, v: &Value, default: &str) -> Result<Vec<usize>, EngineError> {
    let page = v.get("page").filter(|x| !x.is_null());
    let pages = v.get("pages").filter(|x| !x.is_null());
    if page.is_some() && pages.is_some() {
        return Err(EngineError::Params("give `page` or `pages`, not both".into()));
    }
    if let Some(p) = page {
        return Ok(vec![page_ref(s, p)?]);
    }
    let named = |what: &str| -> Result<Vec<usize>, EngineError> {
        match what.trim().to_ascii_lowercase().as_str() {
            "active" | "current" => Ok(vec![s.doc.active_index()]),
            "all" => Ok(foreground(s)),
            _ => s.doc.find_page(what).map(|i| vec![i]).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(what)))),
        }
    };
    let out = match pages {
        None => named(default)?,
        Some(Value::String(t)) => named(t)?,
        Some(Value::Array(a)) => {
            if a.is_empty() || a.len() > MAX_PAGE_REFS {
                return Err(EngineError::Params(format!("`pages` must list 1 to {MAX_PAGE_REFS} pages")));
            }
            let mut out = Vec::new();
            for r in a {
                let i = page_ref(s, r)?;
                if !out.contains(&i) {
                    out.push(i);
                }
            }
            out
        }
        Some(_) => return Err(EngineError::Params("`pages` must be \"active\", \"all\" or a list of pages".into())),
    };
    if out.is_empty() { Err(EngineError::Failed("the document has no page".into())) } else { Ok(out) }
}

/// Paper sizes by name, portrait, in page units (hundredths of an inch).
const PAPERS: [(&str, f64, f64); 5] =
    [("letter", 850.0, 1100.0), ("legal", 850.0, 1400.0), ("tabloid", 1100.0, 1700.0), ("a4", 826.8, 1169.3), ("a3", 1169.3, 1653.5)];

/// The layout params of `file.print`: `fit`, `tiles` and `paper` (turned to match `page_wide`).
pub fn layout_params(v: &Value, page_wide: bool) -> Result<PrintLayout, EngineError> {
    let fit = opt_bool(v, "fit")?.unwrap_or(false);
    let tiles = match v.get("tiles") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) if a.len() == 2 => {
            let (Some(c), Some(r)) = (a.first(), a.get(1)) else { return Err(EngineError::Params("`tiles` must be [cols, rows]".into())) };
            let n = |x: &Value| {
                x.as_u64()
                    .filter(|&n| (1..=u64::from(MAX_TILES)).contains(&n))
                    .map(|n| n as u32)
                    .ok_or_else(|| EngineError::Params(format!("`tiles` must be two whole numbers from 1 to {MAX_TILES}")))
            };
            Some((n(c)?, n(r)?))
        }
        Some(_) => return Err(EngineError::Params(format!("`tiles` must be [cols, rows], each 1 to {MAX_TILES}"))),
    };
    let paper = match v.get("paper") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("page") => None,
        Some(Value::String(t)) => {
            let name = t.trim().to_ascii_lowercase();
            let (_, w, h) = PAPERS.iter().find(|(n, _, _)| *n == name).ok_or_else(|| {
                EngineError::Params(format!("`paper` must be page, letter, legal, tabloid, a4, a3 or {{width, height}}, not `{}`", short(t)))
            })?;
            Some(if page_wide { (*h, *w) } else { (*w, *h) })
        }
        Some(paper @ Value::Object(_)) => {
            let side = |k: &str| {
                super::num(paper, k)?
                    .filter(|x| (10.0..=vizcraft_diagram::MAX_EXTENT).contains(x))
                    .ok_or_else(|| EngineError::Params(format!("`paper.{k}` must be a number from 10 to {}", vizcraft_diagram::MAX_EXTENT)))
            };
            Some((side("width")?, side("height")?))
        }
        Some(_) => return Err(EngineError::Params("`paper` must be a paper name or {width, height}".into())),
    };
    Ok(PrintLayout { paper, fit, tiles })
}

/// Whether the first printed page's frame is at least as wide as it is tall, which picks the
/// default paper orientation.
fn first_is_wide(s: &Session, pages: &[usize]) -> bool {
    s.doc.pages.get(pages.first().copied().unwrap_or(0)).is_some_and(|p| {
        let f = p.frame();
        f.width() >= f.height()
    })
}

/// How `file.print` with params `v` would lay each page out, as (page id, page name, plan), without
/// writing anything; Print Preview draws its tiles from this.
pub fn plans(s: &Session, v: &Value) -> Result<Vec<(String, String, Plan)>, EngineError> {
    let pages = resolve_pages(s, v, "all")?;
    let wide = first_is_wide(s, &pages);
    let layout = layout_params(v, wide)?;
    pages
        .iter()
        .map(|&i| {
            let plan = pdf::plan(&s.doc, i, &layout, true).map_err(|e| EngineError::Failed(format!("print: {e}")))?;
            let page = s.doc.pages.get(i);
            Ok((page.map(|p| p.id.clone()).unwrap_or_default(), page.map(|p| p.name.clone()).unwrap_or_default(), plan))
        })
        .collect()
}

/// Where a print goes when no `path` is given: one file per process in the temporary directory,
/// replaced by the next print.
fn temp_print_path() -> String {
    std::env::temp_dir().join(format!("vizcraft-print-{}.pdf", std::process::id())).to_string_lossy().to_string()
}

/// The print-ready PDF `file.print` with params `v` makes, without writing it (the web build
/// downloads it instead).
pub fn print_pdf(s: &Session, v: &Value) -> Result<pdf::Printed, EngineError> {
    let pages = resolve_pages(s, v, "all")?;
    let wide = first_is_wide(s, &pages);
    let layout = layout_params(v, wide)?;
    let opts = SvgOptions { print: true, date: today() };
    pdf::print_pages(&s.doc, &pages, &opts, &layout).map_err(|e| EngineError::Failed(format!("print: {e}")))
}

fn print(s: &mut Session, v: &Value) -> CmdResult {
    let send = opt_bool(v, "send")?.unwrap_or(false);
    let path = match opt_str(v, "path")? {
        Some(p) if p.trim().is_empty() => return Err(EngineError::Params("`path` must not be empty".into())),
        Some(p) => p.to_string(),
        None => temp_print_path(),
    };
    let printed = print_pdf(s, v)?;
    std::fs::write(&path, &printed.pdf).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    let list: Vec<Value> = printed
        .plans
        .iter()
        .map(|(i, p)| json!({"page": s.doc.pages.get(*i).map(|pg| pg.id.clone()), "cols": p.cols, "rows": p.rows, "scale": p.scale}))
        .collect();
    if send {
        s.ui_requests.push(json!({"print": path}));
    }
    s.status = format!("Printed {} sheet(s) to {path}", printed.sheets());
    Ok(json!({"path": path, "sheets": printed.sheets(), "pages": list, "sent": send}))
}

/// Merge a `{left, center, right}` object into `hf`; slots not given keep their text.
fn merge_slots(hf: &mut HeaderFooter, v: &Value, what: &str) -> Result<(), EngineError> {
    let Value::Object(map) = v else {
        return Err(EngineError::Params(format!("`{what}` must be an object with left, center and right")));
    };
    for key in map.keys() {
        if !matches!(key.as_str(), "left" | "center" | "centre" | "right") {
            return Err(EngineError::Params(format!("`{what}` has no slot `{}` (left, center, right)", short(key))));
        }
    }
    for (keys, slot) in [(&["left"][..], &mut hf.left), (&["center", "centre"][..], &mut hf.center), (&["right"][..], &mut hf.right)] {
        for k in keys {
            if let Some(x) = map.get(*k).filter(|x| !x.is_null()) {
                let t = x.as_str().ok_or_else(|| EngineError::Params(format!("`{what}.{k}` must be a string")))?;
                if t.len() > HeaderFooter::MAX {
                    return Err(EngineError::Params(format!("`{what}.{k}` is longer than {} bytes", HeaderFooter::MAX)));
                }
                *slot = t.to_string();
            }
        }
    }
    Ok(())
}

fn set_header_footer(s: &mut Session, v: &Value) -> CmdResult {
    let all = opt_bool(v, "all")?.unwrap_or(false);
    if all && v.get("page").is_some_and(|p| !p.is_null()) {
        return Err(EngineError::Params("give `page` or `all`, not both".into()));
    }
    let targets: Vec<usize> = if all { (0..s.doc.pages.len()).collect() } else { resolve_pages(s, v, "active")? };
    // `margin`: absent keeps it, null resets it, a number sets it.
    let margin = match v.get("margin") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(x) => Some(Some(
            x.as_f64()
                .filter(|m| m.is_finite() && (0.0..=HeaderFooter::MAX_MARGIN).contains(m))
                .ok_or_else(|| EngineError::Params(format!("`margin` must be a number from 0 to {}", HeaderFooter::MAX_MARGIN)))?,
        )),
    };
    let header = v.get("header").filter(|x| !x.is_null());
    let footer = v.get("footer").filter(|x| !x.is_null());
    // Check every param on a scratch copy first, so a bad footer does not leave a changed header.
    let mut probe = HeaderFooter::default();
    if let Some(h) = header {
        merge_slots(&mut probe, h, "header")?;
    }
    if let Some(f) = footer {
        merge_slots(&mut probe, f, "footer")?;
    }
    for &i in &targets {
        let Some(page) = s.doc.pages.get_mut(i) else { continue };
        if let Some(h) = header {
            merge_slots(&mut page.header, h, "header")?;
        }
        if let Some(f) = footer {
            merge_slots(&mut page.footer, f, "footer")?;
        }
        if let Some(m) = margin {
            page.header.margin = m;
            page.footer.margin = m;
        }
    }
    let first = targets.first().and_then(|&i| s.doc.pages.get(i));
    let ids: Vec<String> = targets.iter().filter_map(|&i| s.doc.pages.get(i).map(|p| p.id.clone())).collect();
    Ok(json!({
        "pages": ids,
        "header": first.map(|p| serde_json::to_value(&p.header).unwrap_or(Value::Null)),
        "footer": first.map(|p| serde_json::to_value(&p.footer).unwrap_or(Value::Null)),
    }))
}

#[cfg(test)]
mod tests;
