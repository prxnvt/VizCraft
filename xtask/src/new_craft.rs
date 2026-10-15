//! `cargo xtask new-craft <name> [--into <dir>]`: instantiate the template.
//!
//! Copies every git-tracked file of this repository to the destination, rewriting `protocraft`,
//! `ProtoCraft` and `PROTOCRAFT` in file contents and in file and directory names, then makes the
//! copy a fresh git repository with one commit. The copy is a complete craft: its own `xtask
//! layers` and `new-craft` keep working because the rules match on name suffixes, not on the
//! `protocraft-` prefix.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The three spellings of a craft name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    /// `notecraft`: crate and directory names, env var and bundle id parts.
    pub lower: String,
    /// `NoteCraft`: the display name.
    pub display: String,
    /// `NOTECRAFT`: environment variables.
    pub upper: String,
}

impl Names {
    /// The reverse-DNS bundle id, `ai.storyteller.<name>`.
    pub fn bundle_id(&self) -> String {
        format!("ai.storyteller.{}", self.lower)
    }
}

/// The template's own names, the ones the copy must no longer contain. (This literal is itself
/// rewritten by `new-craft`, so a craft made from the template can be a template in turn.)
fn base() -> Names {
    Names { lower: "protocraft".into(), display: "ProtoCraft".into(), upper: "PROTOCRAFT".into() }
}

/// A valid craft name is lowercase ASCII letters ending in `craft`, with something before it.
pub fn validate(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(format!("`{name}` is not a craft name: lowercase ASCII letters only (like `notecraft`)"));
    }
    let Some(stem) = name.strip_suffix("craft") else {
        return Err(format!("`{name}` is not a craft name: it must end in `craft` (like `notecraft`)"));
    };
    if stem.is_empty() {
        return Err("`craft` alone is not a craft name: put a word in front (like `notecraft`)".into());
    }
    if name == "protocraft" {
        return Err("`protocraft` is the template itself; pick a new name".into());
    }
    Ok(())
}

/// `notecraft` -> `notecraft` / `NoteCraft` / `NOTECRAFT`.
pub fn derive(name: &str) -> Result<Names, String> {
    validate(name)?;
    let stem = name.strip_suffix("craft").unwrap_or(name);
    let mut display = String::with_capacity(name.len());
    let mut chars = stem.chars();
    if let Some(first) = chars.next() {
        display.extend(first.to_uppercase());
        display.push_str(chars.as_str());
    }
    display.push_str("Craft");
    Ok(Names { lower: name.to_owned(), display, upper: name.to_ascii_uppercase() })
}

/// Replace every spelling of the baseline name in `text`.
pub fn rewrite(text: &str, names: &Names) -> String {
    let b = base();
    text.replace(&b.upper, &names.upper).replace(&b.display, &names.display).replace(&b.lower, &names.lower)
}

/// Copy `files` (paths relative to `src`) into `dest`, renaming paths and rewriting UTF-8 contents.
/// Binary files are copied byte for byte. Returns the number of files written.
pub fn copy_tree(src: &Path, files: &[String], dest: &Path, names: &Names) -> Result<usize, String> {
    let mut n = 0;
    for rel in files {
        let from = src.join(rel);
        let to = dest.join(rewrite(rel, names));
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let bytes = std::fs::read(&from).map_err(|e| format!("read {}: {e}", from.display()))?;
        let out = match std::str::from_utf8(&bytes) {
            Ok(text) => rewrite(text, names).into_bytes(),
            Err(_) => bytes,
        };
        std::fs::write(&to, out).map_err(|e| format!("write {}: {e}", to.display()))?;
        // Keep scripts executable.
        if let Ok(meta) = std::fs::metadata(&from) {
            let _ = std::fs::set_permissions(&to, meta.permissions());
        }
        n += 1;
    }
    Ok(n)
}

/// Tracked files of the repository at `root`, relative, forward slashes.
fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git").current_dir(root).args(["ls-files", "-z"]).output().map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!("git ls-files failed in {}:\n{}", root.display(), String::from_utf8_lossy(&out.stderr)));
    }
    let files: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .filter(|rel| root.join(rel).is_file())
        .collect();
    if files.is_empty() { Err(format!("git ls-files lists nothing in {}", root.display())) } else { Ok(files) }
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let what = format!("git {}", args.join(" "));
    let out = Command::new("git").current_dir(dir).args(args).output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("{what} (in {}): {}", dir.display(), String::from_utf8_lossy(&out.stderr).trim())) }
}

/// Parse `<name> [--into <dir>]`.
fn parse_args<'a>(args: &[&'a str]) -> Result<(&'a str, Option<&'a str>), String> {
    let usage = "usage: cargo xtask new-craft <name> [--into <dir>]";
    let mut name = None;
    let mut into = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--into" => into = Some(*it.next().ok_or_else(|| format!("--into needs a directory\n{usage}"))?),
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`\n{usage}")),
            s if name.is_none() => name = Some(s),
            s => return Err(format!("unexpected argument `{s}`\n{usage}")),
        }
    }
    Ok((name.ok_or_else(|| usage.to_string())?, into))
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let (name, into) = parse_args(args)?;
    let names = derive(name)?;
    let dest: PathBuf = match into {
        Some(d) => PathBuf::from(d),
        None => root.parent().unwrap_or(root).join(&names.lower),
    };
    if dest.exists() {
        return Err(format!("{} already exists; choose another --into directory or remove it", dest.display()));
    }
    let files = tracked_files(root)?;
    println!("new-craft: {} -> {} ({}, {}, {})", names.lower, dest.display(), names.display, names.upper, names.bundle_id());
    let n = copy_tree(root, &files, &dest, &names)?;
    git(&dest, &["init", "-q"])?;
    git(&dest, &["add", "-A"])?;
    git(&dest, &["commit", "-q", "-m", "Created from craft-baseline"])
        .map_err(|e| format!("{e}\nThe copy is in place and staged; set git user.name/user.email and commit it yourself"))?;
    println!("new-craft: {n} files copied, committed as \"Created from craft-baseline\"");
    println!("next: cd {} && cargo xtask ci", dest.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether any spelling of the baseline name survives in `text`.
    fn mentions_base(text: &str) -> bool {
        let b = base();
        text.contains(&b.lower) || text.contains(&b.display) || text.contains(&b.upper)
    }

    #[test]
    fn derives_all_spellings() {
        let n = derive("quuxcraft").unwrap();
        assert_eq!(n, Names { lower: "quuxcraft".into(), display: "QuuxCraft".into(), upper: "QUUXCRAFT".into() });
        assert_eq!(n.bundle_id(), "ai.storyteller.quuxcraft");
        assert_eq!(derive("vectorcraft").unwrap().display, "VectorCraft");
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["", "craft", "Quuxcraft", "quux-craft", "quux_craft", "quuxcraft2", "notes", "protocraft", "craftnote", "QUUXCRAFT"] {
            assert!(validate(bad).is_err(), "{bad}");
        }
        assert!(validate("quuxcraft").is_ok());
    }

    #[test]
    fn rewrites_every_spelling() {
        let n = derive("quuxcraft").unwrap();
        let text = "ProtoCraft (protocraft-engine, PROTOCRAFT_NO_PREFS, ai.storyteller.protocraft) at apps/protocraft; crafts/ProtoCraft.app";
        assert_eq!(
            rewrite(text, &n),
            "QuuxCraft (quuxcraft-engine, QUUXCRAFT_NO_PREFS, ai.storyteller.quuxcraft) at apps/quuxcraft; crafts/QuuxCraft.app"
        );
        assert_eq!(rewrite("nothing here", &n), "nothing here");
        assert!(mentions_base("x protocraft y") && mentions_base("ProtoCraft") && mentions_base("PROTOCRAFT_X"));
        assert!(!mentions_base(&rewrite(text, &n)));
    }

    #[test]
    fn parses_args() {
        assert_eq!(parse_args(&["quuxcraft"]).unwrap(), ("quuxcraft", None));
        assert_eq!(parse_args(&["quuxcraft", "--into", "/tmp/x"]).unwrap(), ("quuxcraft", Some("/tmp/x")));
        assert_eq!(parse_args(&["--into", "/tmp/x", "quuxcraft"]).unwrap(), ("quuxcraft", Some("/tmp/x")));
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["quuxcraft", "--into"]).is_err());
        assert!(parse_args(&["a", "b"]).is_err());
        assert!(parse_args(&["--bogus"]).is_err());
    }

    #[test]
    fn copies_and_renames_a_tree() {
        let base = std::env::temp_dir().join(format!("xtask-new-craft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        let dest = base.join("quuxcraft");
        std::fs::create_dir_all(src.join("apps/protocraft/src")).unwrap();
        std::fs::create_dir_all(src.join("packaging")).unwrap();
        std::fs::write(src.join("Cargo.toml"), "[workspace]\nmembers = [\"apps/protocraft\"]\nprotocraft-engine = { path = \"crates/engine\" }\n")
            .unwrap();
        std::fs::write(src.join("apps/protocraft/src/main.rs"), "// ProtoCraft desktop app\nconst ENV: &str = \"PROTOCRAFT_CONTROL_PORT\";\n")
            .unwrap();
        std::fs::write(src.join("packaging/ProtoCraft.icns"), [0u8, 159, 146, 150, b'b', b'a', b's', b'e']).unwrap();
        std::fs::write(src.join("packaging/run.sh"), "#!/bin/sh\nopen ProtoCraft.app\n").unwrap();
        let files =
            vec!["Cargo.toml".to_string(), "apps/protocraft/src/main.rs".into(), "packaging/ProtoCraft.icns".into(), "packaging/run.sh".into()];

        let n = copy_tree(&src, &files, &dest, &derive("quuxcraft").unwrap()).unwrap();
        assert_eq!(n, 4);
        assert_eq!(
            std::fs::read_to_string(dest.join("Cargo.toml")).unwrap(),
            "[workspace]\nmembers = [\"apps/quuxcraft\"]\nquuxcraft-engine = { path = \"crates/engine\" }\n"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("apps/quuxcraft/src/main.rs")).unwrap(),
            "// QuuxCraft desktop app\nconst ENV: &str = \"QUUXCRAFT_CONTROL_PORT\";\n"
        );
        // Binary content is untouched, the name is still renamed.
        assert_eq!(std::fs::read(dest.join("packaging/QuuxCraft.icns")).unwrap(), [0u8, 159, 146, 150, b'b', b'a', b's', b'e']);
        assert_eq!(std::fs::read_to_string(dest.join("packaging/run.sh")).unwrap(), "#!/bin/sh\nopen QuuxCraft.app\n");
        assert!(!dest.join("apps/protocraft").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
