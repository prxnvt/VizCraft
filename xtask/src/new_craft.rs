//! `cargo xtask new-craft <name> [--into <dir>] [--display-name "<Display Name>"]`: instantiate
//! the template.
//!
//! Copies every git-tracked file of this repository to the destination, rewriting `protocraft`,
//! `ProtoCraft` and `PROTOCRAFT` in file contents and in file and directory names, formats the
//! copy with `cargo fmt` (a name of a different length moves lines across the rustfmt width), then
//! makes the copy a fresh git repository with one commit. The copy is a complete craft: its own
//! `xtask layers` and `new-craft` keep working because the rules match on name suffixes, not on
//! the `protocraft-` prefix.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The spellings of a craft name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    /// `notecraft`: crate and directory names, env var and bundle id parts, the process name.
    pub lower: String,
    /// `NoteCraft` (or an override such as `Note Craft`): the display name in titles, menus,
    /// the preferences folder and the bundle.
    pub display: String,
    /// `NoteCraft`: the display name without spaces, used where it is glued to an identifier
    /// (`NoteCraftOpenDocumentsHandler`, `NoteCraftVersion`). Equal to `display` unless the
    /// display name contains spaces.
    pub ident: String,
    /// `NOTECRAFT`: environment variables.
    pub upper: String,
}

impl Names {
    /// Build the set from the three spellings; `ident` is derived from `display`.
    pub fn new(lower: &str, display: &str, upper: &str) -> Names {
        Names { lower: lower.into(), display: display.into(), ident: display.chars().filter(|c| !c.is_whitespace()).collect(), upper: upper.into() }
    }

    /// The reverse-DNS bundle id, `ai.storyteller.<name>`.
    pub fn bundle_id(&self) -> String {
        format!("ai.storyteller.{}", self.lower)
    }
}

/// The template's own names, the ones the copy must no longer contain. (This literal is itself
/// rewritten by `new-craft`, so a craft made from the template can be a template in turn.)
fn base() -> Names {
    Names::new("protocraft", "ProtoCraft", "PROTOCRAFT")
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
    if name == base().lower {
        return Err(format!("`{name}` is the template itself; pick a new name"));
    }
    Ok(())
}

/// A valid display name is ASCII letters and digits, words separated by single spaces, starting
/// with a letter, and not the template's own name.
pub fn validate_display(display: &str) -> Result<(), String> {
    let hint = "like `Note Craft`: ASCII letters and digits, words separated by single spaces";
    if display.is_empty() {
        return Err(format!("the display name is empty ({hint})"));
    }
    if display != display.trim() || display.contains("  ") {
        return Err(format!("`{display}` is not a display name: no leading, trailing or doubled spaces ({hint})"));
    }
    if !display.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ') {
        return Err(format!("`{display}` is not a display name ({hint})"));
    }
    if !display.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
        return Err(format!("`{display}` is not a display name: it must start with a letter ({hint})"));
    }
    let b = base();
    if display == b.display || display.contains(&b.display) || display.contains(&b.lower) || display.contains(&b.upper) {
        return Err(format!("`{display}` names the template itself; pick a new display name"));
    }
    Ok(())
}

/// `notecraft` -> `notecraft` / `NoteCraft` / `NOTECRAFT`; the display name can be overridden.
pub fn derive(name: &str, display: Option<&str>) -> Result<Names, String> {
    validate(name)?;
    let display = match display {
        Some(d) => {
            validate_display(d)?;
            d.to_owned()
        }
        None => {
            let stem = name.strip_suffix("craft").unwrap_or(name);
            let mut display = String::with_capacity(name.len());
            let mut chars = stem.chars();
            if let Some(first) = chars.next() {
                display.extend(first.to_uppercase());
                display.push_str(chars.as_str());
            }
            display.push_str("Craft");
            display
        }
    };
    Ok(Names::new(name, &display, &name.to_ascii_uppercase()))
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Replace every spelling of the baseline name in `text`. The display spelling becomes
/// `names.ident` where it touches an identifier character on either side (so an override with a
/// space never lands inside a type or key name) and `names.display` everywhere else.
pub fn rewrite(text: &str, names: &Names) -> String {
    let b = base();
    // Longest and most specific first: the upper and display forms are never prefixes of each
    // other, but every spelling must be tried before falling through to a plain character.
    let mut patterns: Vec<(&str, Spelling)> = vec![(&b.upper, Spelling::Upper), (&b.display, Spelling::Display)];
    if b.ident != b.display {
        patterns.push((&b.ident, Spelling::Display));
    }
    patterns.push((&b.lower, Spelling::Lower));

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'scan: while i < bytes.len() {
        for (pat, spelling) in &patterns {
            if bytes[i..].starts_with(pat.as_bytes()) {
                let end = i + pat.len();
                let glued = (i > 0 && is_ident_byte(bytes[i - 1])) || bytes.get(end).is_some_and(|b| is_ident_byte(*b));
                out.push_str(match spelling {
                    Spelling::Upper => &names.upper,
                    Spelling::Lower => &names.lower,
                    Spelling::Display if glued => &names.ident,
                    Spelling::Display => &names.display,
                });
                i = end;
                continue 'scan;
            }
        }
        // Not at a pattern: copy one whole character.
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    out
}

#[derive(Clone, Copy)]
enum Spelling {
    Upper,
    Display,
    Lower,
}

/// Tracked files that belong to the template only and must not travel into a craft. The root
/// marker `.craft-template` tells `crafts/rebuild.sh` to skip the baseline, so it is never
/// installed to /Applications; a craft without the marker is picked up by the installer.
pub fn skipped(rel: &str) -> bool {
    rel == ".craft-template"
}

/// Copy `files` (paths relative to `src`) into `dest`, renaming paths and rewriting UTF-8 contents.
/// Binary files are copied byte for byte; template-only files ([`skipped`]) are left out. Returns
/// the number of files written.
pub fn copy_tree(src: &Path, files: &[String], dest: &Path, names: &Names) -> Result<usize, String> {
    let mut n = 0;
    for rel in files.iter().filter(|rel| !skipped(rel)) {
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

/// `cargo fmt --all` in the copy: a name of a different length moves lines across the rustfmt
/// width, and `cargo xtask ci` in the copy starts with `fmt --check`.
fn format_copy(dest: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo).current_dir(dest).args(["fmt", "--all"]).output().map_err(|e| format!("cargo fmt in {}: {e}", dest.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo fmt --all failed in {} (is rustfmt installed? `rustup component add rustfmt`):\n{}",
            dest.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The message for a `git commit` that failed in the fresh copy. A missing identity is the
/// common case (a fresh machine or a worktree without `user.name`/`user.email`), so it gets the
/// exact commands to run; anything else is passed through with the copy's location.
pub fn commit_failure_message(dest: &Path, git_error: &str) -> String {
    let no_identity = ["Please tell me who you are", "Author identity unknown", "user.name", "user.email", "empty ident name"]
        .iter()
        .any(|needle| git_error.contains(needle));
    if no_identity {
        format!(
            "{git_error}\n\
             The copy is in place at {dest} and staged, but git has no identity to commit with.\n\
             Set one and commit:\n\
             \x20 git config --global user.name \"Your Name\"\n\
             \x20 git config --global user.email \"you@example.com\"\n\
             \x20 cd {dest} && git commit -m \"Created from craft-baseline\"",
            dest = dest.display()
        )
    } else {
        format!("{git_error}\nThe copy is in place at {} and staged; commit it yourself", dest.display())
    }
}

/// The directory entry that `dest` would collide with, if any: `dest` itself, or a sibling whose
/// name differs only in case (the file systems macOS and Windows use would treat them as one).
pub fn collision(dest: &Path) -> Option<PathBuf> {
    if dest.exists() {
        return Some(dest.to_path_buf());
    }
    let parent = dest.parent()?;
    let name = dest.file_name()?.to_str()?;
    let entries = std::fs::read_dir(parent).ok()?;
    entries.flatten().map(|e| e.path()).find(|p| p.file_name().and_then(|f| f.to_str()).is_some_and(|f| f.eq_ignore_ascii_case(name)))
}

/// Parsed command line.
#[derive(Debug, PartialEq, Eq)]
struct Args<'a> {
    name: &'a str,
    into: Option<&'a str>,
    display: Option<&'a str>,
}

const USAGE: &str = "usage: cargo xtask new-craft <name> [--into <dir>] [--display-name \"<Display Name>\"]";

/// Parse `<name> [--into <dir>] [--display-name <display>]`.
fn parse_args<'a>(args: &[&'a str]) -> Result<Args<'a>, String> {
    let mut name = None;
    let mut into = None;
    let mut display = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--into" => into = Some(*it.next().ok_or_else(|| format!("--into needs a directory\n{USAGE}"))?),
            "--display-name" => display = Some(*it.next().ok_or_else(|| format!("--display-name needs a name\n{USAGE}"))?),
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`\n{USAGE}")),
            s if name.is_none() => name = Some(s),
            s => return Err(format!("unexpected argument `{s}`\n{USAGE}")),
        }
    }
    Ok(Args { name: name.ok_or_else(|| USAGE.to_string())?, into, display })
}

/// What to do once the copy exists.
pub fn next_steps(dest: &Path, names: &Names) -> String {
    format!(
        "next steps:\n\
         \x20 cd {dest}\n\
         \x20 cargo xtask ci                     # proves the copy builds and passes every gate\n\
         \x20 cargo run -p {lower} -- --sample   # the app under its new name\n\
         then follow docs/new-craft.md (in the copy) from step 2: replace the sample model.",
        dest = dest.display(),
        lower = names.lower
    )
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let Args { name, into, display } = parse_args(args)?;
    let names = derive(name, display)?;
    let dest: PathBuf = match into {
        Some(d) => PathBuf::from(d),
        None => root.parent().unwrap_or(root).join(&names.lower),
    };
    if let Some(existing) = collision(&dest) {
        return Err(format!("{} already exists; choose another name or --into directory, or remove it first", existing.display()));
    }
    let files = tracked_files(root)?;
    println!("new-craft: {} -> {} ({}, {}, {})", names.lower, dest.display(), names.display, names.upper, names.bundle_id());
    let n = copy_tree(root, &files, &dest, &names)?;
    format_copy(&dest)?;
    git(&dest, &["init", "-q"])?;
    git(&dest, &["add", "-A"])?;
    git(&dest, &["commit", "-q", "-m", "Created from craft-baseline"]).map_err(|e| commit_failure_message(&dest, &e))?;
    println!("new-craft: {n} files copied, formatted, committed as \"Created from craft-baseline\"");
    println!("{}", next_steps(&dest, &names));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether any spelling of the baseline name survives in `text`.
    fn mentions_base(text: &str) -> bool {
        let b = base();
        text.contains(&b.lower) || text.contains(&b.display) || text.contains(&b.ident) || text.contains(&b.upper)
    }

    /// A scratch directory unique to this test.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-new-craft-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn derives_all_spellings() {
        let n = derive("quuxcraft", None).unwrap();
        assert_eq!(n, Names::new("quuxcraft", "QuuxCraft", "QUUXCRAFT"));
        assert_eq!(n.ident, "QuuxCraft");
        assert_eq!(n.bundle_id(), "ai.storyteller.quuxcraft");
        assert_eq!(derive("vectorcraft", None).unwrap().display, "VectorCraft");
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["", "craft", "Quuxcraft", "quux-craft", "quux_craft", "quuxcraft2", "notes", "protocraft", "craftnote", "QUUXCRAFT"] {
            assert!(validate(bad).is_err(), "{bad}");
        }
        assert!(validate("quuxcraft").is_ok());
    }

    #[test]
    fn display_name_override() {
        let n = derive("notecraft", Some("Note Craft")).unwrap();
        assert_eq!(n, Names { lower: "notecraft".into(), display: "Note Craft".into(), ident: "NoteCraft".into(), upper: "NOTECRAFT".into() });
        // The override need not relate to the name at all.
        assert_eq!(derive("quuxcraft", Some("Quux Pro 2")).unwrap().ident, "QuuxPro2");
        for bad in [
            "",
            " Note Craft",
            "Note Craft ",
            "Note  Craft",
            "Note-Craft",
            "Note/Craft",
            "2Craft",
            "Note\nCraft",
            "ProtoCraft",
            "My ProtoCraft",
            "protocraft",
        ] {
            assert!(validate_display(bad).is_err(), "{bad:?}");
            assert!(derive("notecraft", Some(bad)).is_err(), "{bad:?}");
        }
        assert!(validate_display("NoteCraft").is_ok());
        assert!(validate_display("Note Craft").is_ok());
    }

    #[test]
    fn rewrites_every_spelling() {
        let n = derive("quuxcraft", None).unwrap();
        let text = "ProtoCraft (protocraft-engine, PROTOCRAFT_NO_PREFS, ai.storyteller.protocraft) at apps/protocraft; crafts/ProtoCraft.app";
        assert_eq!(
            rewrite(text, &n),
            "QuuxCraft (quuxcraft-engine, QUUXCRAFT_NO_PREFS, ai.storyteller.quuxcraft) at apps/quuxcraft; crafts/QuuxCraft.app"
        );
        assert_eq!(rewrite("nothing here", &n), "nothing here");
        assert_eq!(rewrite("caf\u{e9} ProtoCraft \u{1F600}protocraft", &n), "caf\u{e9} QuuxCraft \u{1F600}quuxcraft");
        assert!(mentions_base("x protocraft y") && mentions_base("ProtoCraft") && mentions_base("PROTOCRAFT_X"));
        assert!(!mentions_base(&rewrite(text, &n)));
    }

    #[test]
    fn display_override_keeps_identifiers_whole() {
        let n = derive("notecraft", Some("Note Craft")).unwrap();
        assert_eq!(rewrite("About ProtoCraft", &n), "About Note Craft");
        assert_eq!(rewrite("Welcome to ProtoCraft - ProtoCraft.app", &n), "Welcome to Note Craft - Note Craft.app");
        assert_eq!(rewrite("#[name = \"ProtoCraftOpenDocumentsHandler\"]", &n), "#[name = \"NoteCraftOpenDocumentsHandler\"]");
        assert_eq!(rewrite("<key>ProtoCraftVersion</key>", &n), "<key>NoteCraftVersion</key>");
        assert_eq!(rewrite("struct MyProtoCraft;", &n), "struct MyNoteCraft;");
        assert_eq!(rewrite("Application Support/ProtoCraft\"", &n), "Application Support/Note Craft\"");
        // The other spellings are unaffected by the override.
        assert_eq!(rewrite("PROTOCRAFT_NO_PREFS protocraft-cli", &n), "NOTECRAFT_NO_PREFS notecraft-cli");
    }

    #[test]
    fn parses_args() {
        assert_eq!(parse_args(&["quuxcraft"]).unwrap(), Args { name: "quuxcraft", into: None, display: None });
        assert_eq!(parse_args(&["quuxcraft", "--into", "/tmp/x"]).unwrap(), Args { name: "quuxcraft", into: Some("/tmp/x"), display: None });
        assert_eq!(parse_args(&["--into", "/tmp/x", "quuxcraft"]).unwrap(), Args { name: "quuxcraft", into: Some("/tmp/x"), display: None });
        assert_eq!(
            parse_args(&["quuxcraft", "--display-name", "Quux Craft"]).unwrap(),
            Args { name: "quuxcraft", into: None, display: Some("Quux Craft") }
        );
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["quuxcraft", "--into"]).is_err());
        assert!(parse_args(&["quuxcraft", "--display-name"]).is_err());
        assert!(parse_args(&["a", "b"]).is_err());
        assert!(parse_args(&["--bogus"]).is_err());
    }

    #[test]
    fn refuses_colliding_destinations() {
        let dir = scratch("collide");
        std::fs::create_dir(dir.join("gridcraft")).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        // The directory itself, in any case the file system might fold together.
        // (A case-insensitive file system reports the differently cased path as existing; a
        // case-sensitive one finds the sibling. Either way the hit is the same entry.)
        for spelling in ["gridcraft", "GridCraft", "GRIDCRAFT"] {
            let hit = collision(&dir.join(spelling)).unwrap_or_else(|| panic!("{spelling} should collide"));
            assert!(hit.file_name().unwrap().to_str().unwrap().eq_ignore_ascii_case("gridcraft"), "{spelling}: {hit:?}");
        }
        // A file with the name is a collision too.
        assert!(collision(&dir.join("notes.txt")).is_some());
        // A free name is not, and a parent that does not exist is not either.
        assert_eq!(collision(&dir.join("quuxcraft")), None);
        assert_eq!(collision(&dir.join("nowhere/quuxcraft")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn git_identity_error_gets_instructions() {
        let dest = Path::new("/tmp/quuxcraft");
        let git_err = "git commit -q -m x (in /tmp/quuxcraft): Author identity unknown\n\n*** Please tell me who you are.";
        let msg = commit_failure_message(dest, git_err);
        assert!(msg.contains("Author identity unknown"), "{msg}");
        assert!(msg.contains("git config --global user.name"), "{msg}");
        assert!(msg.contains("git config --global user.email"), "{msg}");
        assert!(msg.contains("cd /tmp/quuxcraft && git commit"), "{msg}");
        // Any other failure is passed through without the identity instructions.
        let other = commit_failure_message(dest, "git commit (in /tmp/quuxcraft): fatal: disk full");
        assert!(other.contains("disk full") && other.contains("/tmp/quuxcraft"), "{other}");
        assert!(!other.contains("user.name"), "{other}");
    }

    #[test]
    fn next_steps_point_at_the_doc() {
        let msg = next_steps(Path::new("/tmp/quuxcraft"), &derive("quuxcraft", None).unwrap());
        assert!(msg.contains("cd /tmp/quuxcraft") && msg.contains("cargo xtask ci"), "{msg}");
        assert!(msg.contains("cargo run -p quuxcraft -- --sample") && msg.contains("docs/new-craft.md"), "{msg}");
    }

    #[test]
    fn copies_and_renames_a_tree() {
        let base_dir = scratch("tree");
        let src = base_dir.join("src");
        let dest = base_dir.join("quuxcraft");
        let b = base();
        std::fs::create_dir_all(src.join(format!("apps/{}/src", b.lower))).unwrap();
        std::fs::create_dir_all(src.join("packaging")).unwrap();
        std::fs::write(
            src.join("Cargo.toml"),
            format!("[workspace]\nmembers = [\"apps/{l}\"]\n{l}-engine = {{ path = \"crates/engine\" }}\n", l = b.lower),
        )
        .unwrap();
        std::fs::write(
            src.join(format!("apps/{}/src/main.rs", b.lower)),
            format!("// {} desktop app\nconst ENV: &str = \"{}_CONTROL_PORT\";\n", b.display, b.upper),
        )
        .unwrap();
        std::fs::write(src.join(format!("packaging/{}.icns", b.display)), [0u8, 159, 146, 150, b'b', b'a', b's', b'e']).unwrap();
        std::fs::write(src.join("packaging/run.sh"), format!("#!/bin/sh\nopen {}.app\n", b.display)).unwrap();
        std::fs::write(src.join(".craft-template"), "").unwrap();
        let files: Vec<String> = vec![
            "Cargo.toml".into(),
            format!("apps/{}/src/main.rs", b.lower),
            format!("packaging/{}.icns", b.display),
            "packaging/run.sh".into(),
            ".craft-template".into(),
        ];

        let n = copy_tree(&src, &files, &dest, &derive("quuxcraft", None).unwrap()).unwrap();
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
        assert!(!dest.join(format!("apps/{}", b.lower)).exists());
        // The template marker stays behind: the copy is a craft, not a template.
        assert!(!dest.join(".craft-template").exists());
        std::fs::remove_dir_all(&base_dir).unwrap();
    }

    #[test]
    fn template_marker_is_skipped() {
        assert!(skipped(".craft-template"));
        for kept in ["Cargo.toml", ".gitignore", ".cargo/config.toml", "docs/.craft-template", "craft-template"] {
            assert!(!skipped(kept), "{kept}");
        }
    }
}
