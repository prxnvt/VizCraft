//! `cargo xtask version`: the workspace version and the git commit.
//!
//! The version lives in `[workspace.package] version` of the root `Cargo.toml`; every crate
//! inherits it with `version.workspace = true`.

use std::path::Path;
use std::process::Command;

/// The `version = "..."` value inside `[workspace.package]`.
pub fn read(manifest: &str) -> Result<String, String> {
    let mut in_section = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[workspace.package]";
            continue;
        }
        if in_section
            && let Some(rest) = t.strip_prefix("version")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            let v = value.split('#').next().unwrap_or_default().trim().trim_matches('"');
            if v.is_empty() {
                return Err("empty [workspace.package] version".into());
            }
            return Ok(v.to_owned());
        }
    }
    Err("no `version = \"...\"` in [workspace.package] of the root Cargo.toml".into())
}

/// Short commit hash, with `-dirty` when the tree has uncommitted changes; `unknown` outside git.
pub fn git_sha(root: &Path) -> String {
    let git = |args: &[&str]| Command::new("git").current_dir(root).args(args).output().ok().filter(|o| o.status.success());
    let Some(rev) = git(&["rev-parse", "--short", "HEAD"]) else { return "unknown".into() };
    let sha = String::from_utf8_lossy(&rev.stdout).trim().to_owned();
    let dirty = git(&["status", "--porcelain"]).is_some_and(|o| !o.stdout.is_empty());
    if dirty { format!("{sha}-dirty") } else { sha }
}

pub fn run(root: &Path) -> Result<(), String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    println!("{} ({})", read(&text)?, git_sha(root));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.1.0\" # comment\nedition = \"2024\"\n\n[workspace.dependencies]\nfoo = { version = \"1\" }\n";

    #[test]
    fn reads_the_workspace_version() {
        assert_eq!(read(MANIFEST).unwrap(), "0.1.0");
        assert!(read("[package]\nversion = \"1.0.0\"\n").is_err());
        assert!(read("[workspace.package]\nversion = \"\"\n").is_err());
    }

    #[test]
    fn the_real_manifest_has_a_version() {
        let text = std::fs::read_to_string(crate::root().join("Cargo.toml")).unwrap();
        let v = read(&text).unwrap();
        assert_eq!(v.split('.').count(), 3, "{v}");
    }
}
