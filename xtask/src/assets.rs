//! `cargo xtask assets`: every asset file has a row in `ATTRIBUTION.md`.
//!
//! Every file under `assets/` and `packaging/` that is not source (`.md`, `.sh`, `.plist`, `.py`)
//! needs a table row whose first column is the backticked repository-relative path. The check
//! also fails when a row names a file that no longer exists, so the table cannot rot.

use std::path::{Path, PathBuf};

/// Directories whose contents are assets.
pub const ASSET_DIRS: &[&str] = &["assets", "packaging"];
/// Extensions that are source, not assets, even inside the asset directories.
pub const SOURCE_EXT: &[&str] = &["md", "sh", "plist", "py", "in"];

/// Whether a repository-relative path (forward slashes) must be attributed.
pub fn is_asset(rel: &str) -> bool {
    let in_dir = ASSET_DIRS.iter().any(|d| rel.starts_with(&format!("{d}/")));
    let ext = Path::new(rel).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    in_dir && !SOURCE_EXT.contains(&ext.as_str())
}

/// Every file below `root/assets` and `root/packaging`, as sorted repository-relative paths.
pub fn asset_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for dir in ASSET_DIRS {
        let top = root.join(dir);
        if top.is_dir() {
            walk(&top, &mut out)?;
        }
    }
    let mut rel: Vec<String> = out
        .iter()
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
        .filter(|r| is_asset(r))
        .collect();
    rel.sort();
    Ok(rel)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("read {}: {e}", dir.display()))?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// The paths named in the first column of ATTRIBUTION.md table rows (`| \`path\` | ... |`).
pub fn attributed(md: &str) -> Vec<String> {
    md.lines()
        .filter_map(|line| {
            let first = line.trim().strip_prefix('|')?.split('|').next()?.trim();
            let inner = first.strip_prefix('`')?.strip_suffix('`')?;
            if inner.is_empty() { None } else { Some(inner.to_owned()) }
        })
        .collect()
}

/// (missing from the table, listed but not on disk).
pub fn compare(files: &[String], rows: &[String], root: &Path) -> (Vec<String>, Vec<String>) {
    let missing = files.iter().filter(|f| !rows.contains(f)).cloned().collect();
    let stale = rows.iter().filter(|r| !root.join(r).is_file()).cloned().collect();
    (missing, stale)
}

/// Run the check against a repository root.
pub fn check(root: &Path) -> Result<usize, String> {
    let md_path = root.join("ATTRIBUTION.md");
    let md = std::fs::read_to_string(&md_path)
        .map_err(|e| format!("{} is missing or unreadable ({e}); every asset needs a row there", md_path.display()))?;
    let files = asset_files(root)?;
    let rows = attributed(&md);
    let (missing, stale) = compare(&files, &rows, root);
    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!(
            "{} asset file(s) lack an ATTRIBUTION.md row (path, author, source, licence):\n  {}",
            missing.len(),
            missing.join("\n  ")
        ));
    }
    if !stale.is_empty() {
        problems.push(format!("{} ATTRIBUTION.md row(s) name a file that does not exist:\n  {}", stale.len(), stale.join("\n  ")));
    }
    if problems.is_empty() { Ok(files.len()) } else { Err(problems.join("\n")) }
}

pub fn run(root: &Path) -> Result<(), String> {
    let n = check(root)?;
    println!("assets: all {n} asset files attributed in ATTRIBUTION.md");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-assets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets/fonts")).unwrap();
        std::fs::create_dir_all(dir.join("packaging/icons")).unwrap();
        std::fs::write(dir.join("assets/fonts/Inter.ttf"), b"x").unwrap();
        std::fs::write(dir.join("assets/fonts/OFL.txt"), b"x").unwrap();
        std::fs::write(dir.join("assets/README.md"), b"x").unwrap();
        std::fs::write(dir.join("packaging/icons/app.png"), b"x").unwrap();
        std::fs::write(dir.join("packaging/build.sh"), b"x").unwrap();
        std::fs::write(dir.join("packaging/Info.plist"), b"x").unwrap();
        std::fs::write(dir.join("packaging/make_icons.py"), b"x").unwrap();
        dir
    }

    #[test]
    fn source_files_are_not_assets() {
        assert!(is_asset("assets/fonts/Inter.ttf"));
        assert!(is_asset("assets/fonts/OFL.txt"));
        assert!(is_asset("packaging/icons/app.png"));
        assert!(!is_asset("assets/README.md"));
        assert!(!is_asset("packaging/build.sh"));
        assert!(!is_asset("packaging/Info.plist"));
        assert!(!is_asset("packaging/make_icons.py"));
        assert!(!is_asset("crates/engine/src/lib.rs"));
        assert!(!is_asset("assetsx/a.png"));
    }

    #[test]
    fn rows_are_parsed_from_the_first_column() {
        let md = "# Attribution\n\n| File | Author | Source | Licence |\n|---|---|---|---|\n| `assets/fonts/Inter.ttf` | Rasmus | https://x | OFL |\n|`packaging/icons/app.png`| us | here | MIT |\nNot a row `x`\n| plain | a | b | c |\n";
        assert_eq!(attributed(md), vec!["assets/fonts/Inter.ttf".to_string(), "packaging/icons/app.png".into()]);
    }

    #[test]
    fn missing_file_fails() {
        let root = temp_root("missing");
        assert!(check(&root).unwrap_err().contains("ATTRIBUTION.md is missing"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_rows_and_stale_rows_are_reported() {
        let root = temp_root("rows");
        std::fs::write(root.join("ATTRIBUTION.md"), "| `assets/fonts/Inter.ttf` | a | b | c |\n| `assets/gone.png` | a | b | c |\n").unwrap();
        let err = check(&root).unwrap_err();
        assert!(err.contains("assets/fonts/OFL.txt"), "{err}");
        assert!(err.contains("packaging/icons/app.png"), "{err}");
        assert!(!err.contains("Inter.ttf"), "{err}");
        assert!(err.contains("does not exist") && err.contains("assets/gone.png"), "{err}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn complete_table_passes() {
        let root = temp_root("ok");
        std::fs::write(
            root.join("ATTRIBUTION.md"),
            "| `assets/fonts/Inter.ttf` | a | b | c |\n| `assets/fonts/OFL.txt` | a | b | c |\n| `packaging/icons/app.png` | a | b | c |\n",
        )
        .unwrap();
        assert_eq!(check(&root).unwrap(), 3);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_asset_dirs_is_fine_with_an_empty_table() {
        let dir = std::env::temp_dir().join(format!("xtask-assets-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ATTRIBUTION.md"), "# Attribution\n").unwrap();
        assert_eq!(check(&dir).unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
