//! Planning and applying a generation.
//!
//! [`plan`] runs the target and compares every file it emits with the disk and the manifest,
//! giving each an [`Action`]:
//!
//! | Action | When |
//! |---|---|
//! | `create` | the file is not on disk |
//! | `unchanged` | the file on disk is what generation would write (or a hand edit generation has nothing to add to) |
//! | `update` | the file is owned, untouched since the last run, and the new text differs |
//! | `merge` | the file is owned, edited by hand, and the new text merges cleanly with the edit |
//! | `conflict` | a hand edit overlaps the regenerated lines, or the file exists but is not owned |
//! | `delete` | the manifest lists a file the target no longer emits, untouched since |
//!
//! A conflict stops the run: [`apply`] then writes nothing. Otherwise it writes only owned paths
//! inside the output folder (relative, no `..`, never through a symbolic link), each atomically
//! (a temporary file renamed over the old one), in path order, and skips any file whose bytes
//! would not change, so an unchanged model regenerates byte for byte.
//!
//! Planning reads through [`Disk`], so the same plan serves a real folder ([`FsDisk`]) and the
//! web build, which has none ([`NoDisk`]).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use vizcraft_model::{Issue, Model, Severity};
use vizcraft_notation::Catalog;

use crate::manifest::{FileEntry, MANIFEST_VERSION, Manifest};
use crate::names::{Names, Rename};
use crate::spec::Spec;
use crate::target::{ModelView, Options};
use crate::{CodegenError, GENERATOR, MAX_FILE_BYTES, MAX_FILES, MAX_PATH, MAX_TOTAL_BYTES, STATE_DIR, clip, merge, regions, sha256_hex};

/// What a run does to one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Written for the first time.
    Create,
    /// Left as it is.
    Unchanged,
    /// Overwritten with the new text (it was not edited by hand).
    Update,
    /// A hand edit merged with the new text.
    Merge,
    /// Stops the run: overlapping edits, or a file the generator does not own.
    Conflict,
    /// Removed: the target no longer emits it.
    Delete,
}

impl Action {
    /// The name in results.
    pub fn name(self) -> &'static str {
        match self {
            Action::Create => "create",
            Action::Unchanged => "unchanged",
            Action::Update => "update",
            Action::Merge => "merge",
            Action::Conflict => "conflict",
            Action::Delete => "delete",
        }
    }
}

/// One file of a plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanFile {
    /// Path relative to the output folder.
    pub path: String,
    /// What the run does.
    pub action: Action,
    /// Unified diff from the file on disk to what the run leaves there (for a conflict: to the
    /// text with conflict markers).
    pub diff: String,
    /// The bytes to put on disk, when they change.
    pub write: Option<String>,
    /// The text as generated (protected regions filled in), stored as the last generated copy.
    pub generated: Option<String>,
    /// The file on disk differs from what the manifest says was generated.
    pub hand_edited: bool,
}

/// What a run would do.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The target id.
    pub target: String,
    /// One entry per file, in path order.
    pub files: Vec<PlanFile>,
    /// Issues from the support matrix, regions and conflicts.
    pub issues: Vec<Issue>,
    /// Identifiers a `renames: true` run changes.
    pub renames: Vec<Rename>,
    /// The manifest the run writes.
    pub manifest: Manifest,
    /// The manifest on disk differs from [`Plan::manifest`].
    pub manifest_changed: bool,
    /// A conflict stops the run.
    pub stopped: bool,
}

impl Plan {
    /// Whether the output on disk is what generation would write: no file to create, change,
    /// merge or delete and an up-to-date manifest.
    pub fn is_current(&self) -> bool {
        !self.manifest_changed && self.files.iter().all(|f| f.action == Action::Unchanged && f.write.is_none())
    }
}

/// What a run did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Files created or updated.
    pub written: Vec<String>,
    /// Hand-edited files merged with the new text.
    pub merged: Vec<String>,
    /// Files removed.
    pub deleted: Vec<String>,
    /// Files left as they were.
    pub unchanged: Vec<String>,
    /// A conflict stopped the run before anything was written.
    pub stopped: bool,
    /// The plan's issues.
    pub issues: Vec<Issue>,
}

/// Read access to an output folder, for planning.
pub trait Disk {
    /// The text of `rel` (relative to the output folder), `None` when it does not exist.
    fn read(&self, rel: &str) -> Result<Option<String>, CodegenError>;
}

/// No folder at all: every file is missing (the web build's preview).
pub struct NoDisk;

impl Disk for NoDisk {
    fn read(&self, _rel: &str) -> Result<Option<String>, CodegenError> {
        Ok(None)
    }
}

/// A folder on disk.
pub struct FsDisk {
    root: PathBuf,
}

impl FsDisk {
    /// The output folder at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FsDisk { root: root.into() }
    }
}

impl Disk for FsDisk {
    fn read(&self, rel: &str) -> Result<Option<String>, CodegenError> {
        let rel = relative_path(rel)?;
        let mut p = self.root.clone();
        let parts: Vec<&str> = rel.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            p.push(part);
            let meta = match std::fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(io(&p, e)),
            };
            if meta.file_type().is_symlink() {
                return Err(CodegenError::Path(format!("{rel}: goes through a symbolic link")));
            }
            let last = i + 1 == parts.len();
            if !last && !meta.is_dir() {
                return Ok(None);
            }
            if last {
                if !meta.is_file() {
                    return Err(CodegenError::Path(format!("{rel}: not a file")));
                }
                if meta.len() > MAX_FILE_BYTES as u64 {
                    return Err(CodegenError::Io(format!("{rel}: larger than {MAX_FILE_BYTES} bytes")));
                }
            }
        }
        let bytes = std::fs::read(&p).map_err(|e| io(&p, e))?;
        String::from_utf8(bytes).map(Some).map_err(|_| CodegenError::Io(format!("{rel}: not UTF-8 text")))
    }
}

fn io(p: &Path, e: std::io::Error) -> CodegenError {
    CodegenError::Io(format!("{}: {e}", clip(&p.to_string_lossy(), 300)))
}

/// `path` when it is a safe relative path: not empty, at most [`MAX_PATH`] bytes, `/`-separated,
/// no empty, `.` or `..` part, no leading `/`, no backslash, colon or control character.
pub fn relative_path(path: &str) -> Result<String, CodegenError> {
    let shown = clip(path, 80);
    if path.is_empty() {
        return Err(CodegenError::Path("empty path".into()));
    }
    if path.len() > MAX_PATH {
        return Err(CodegenError::Path(format!("`{shown}...` is longer than {MAX_PATH} bytes")));
    }
    if path.chars().any(|c| c.is_control() || c == '\\' || c == ':') {
        return Err(CodegenError::Path(format!("`{}` has a backslash, colon or control character", shown.escape_debug())));
    }
    if path.starts_with('/') || Path::new(path).is_absolute() {
        return Err(CodegenError::Path(format!("`{shown}` is absolute; paths are relative to the output folder")));
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(CodegenError::Path(format!("`{shown}` has an empty, `.` or `..` part")));
        }
    }
    if Path::new(path).components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(CodegenError::Path(format!("`{shown}` is not a plain relative path")));
    }
    Ok(path.to_string())
}

/// Work out what generating `spec` over `model` would do to the folder `disk` reads.
pub fn plan(spec: &Spec, model: &Model, catalog: &Catalog, renames: bool, disk: &dyn Disk) -> Result<Plan, CodegenError> {
    let target = crate::target(&spec.target).ok_or_else(|| {
        let known: Vec<&str> = crate::targets().iter().map(|t| t.id()).collect();
        CodegenError::Spec(format!("unknown target `{}` (known: {})", clip(&spec.target, 60), known.join(", ")))
    })?;
    let options = Options::new(spec.options.clone());
    let view = ModelView::new(model, catalog, &spec.scope);
    let mut issues = Vec::new();

    let old_text = disk.read(&Manifest::path())?;
    let old = match old_text.as_deref().map(Manifest::parse) {
        None => None,
        Some(Ok(m)) => Some(m),
        Some(Err(e)) => {
            issues.push(crate::issue(
                "manifest",
                Severity::Warning,
                format!("the manifest cannot be read ({e}); files already on disk are treated as not owned"),
                Vec::new(),
            ));
            None
        }
    };
    let empty = Manifest::default();
    let old_m = old.as_ref().unwrap_or(&empty);

    let (names, rename_list) = Names::resolve(&view, &target.name_rules(), &old_m.renames, renames);
    issues.extend(view.support_issues(target, &options));

    let mut outputs = target.generate(&view, &names, &options)?;
    if outputs.len() > MAX_FILES {
        return Err(CodegenError::Target(format!("{} files is more than {MAX_FILES}", outputs.len())));
    }
    let mut total = 0usize;
    let mut seen = BTreeSet::new();
    for o in &mut outputs {
        relative_path(&o.path)?;
        if o.path == STATE_DIR || o.path.starts_with(&format!("{STATE_DIR}/")) {
            return Err(CodegenError::Path(format!("`{}` is inside the generator's own folder", clip(&o.path, 80))));
        }
        if !seen.insert(o.path.clone()) {
            return Err(CodegenError::Target(format!("`{}` is emitted twice", clip(&o.path, 80))));
        }
        o.text = crate::normalize_newlines(&o.text);
        if o.text.len() > MAX_FILE_BYTES {
            return Err(CodegenError::Target(format!("`{}` is larger than {MAX_FILE_BYTES} bytes", clip(&o.path, 80))));
        }
        total = total.saturating_add(o.text.len());
        if total > MAX_TOTAL_BYTES {
            return Err(CodegenError::Target(format!("output is larger than {MAX_TOTAL_BYTES} bytes")));
        }
    }
    outputs.sort_by(|a, b| a.path.cmp(&b.path));

    let mut files: Vec<PlanFile> = Vec::new();
    let mut rows: Vec<FileEntry> = Vec::new();
    let syntax = target.single_file().then(|| target.comment());
    for o in outputs {
        let on_disk = disk.read(&o.path)?;
        let entry = old_m.entry(&o.path);
        let (file, found) = decide(&o.path, o.text, on_disk, entry, disk, syntax)?;
        issues.extend(found);
        if file.action == Action::Conflict {
            if let Some(e) = entry {
                rows.push(e.clone());
            }
        } else if let Some(g) = &file.generated {
            rows.push(FileEntry { path: file.path.clone(), sha256: sha256_hex(g.as_bytes()) });
        }
        files.push(file);
    }
    // Owned files the target no longer emits.
    for e in &old_m.files {
        if seen.contains(&e.path) {
            continue;
        }
        let Some(d) = disk.read(&e.path)? else { continue };
        if sha256_hex(d.as_bytes()) == e.sha256 {
            files.push(PlanFile {
                path: e.path.clone(),
                action: Action::Delete,
                diff: merge::unified(&e.path, &d, ""),
                write: None,
                generated: None,
                hand_edited: false,
            });
        } else {
            rows.push(e.clone());
            files.push(PlanFile {
                path: e.path.clone(),
                action: Action::Conflict,
                diff: String::new(),
                write: None,
                generated: None,
                hand_edited: true,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    for f in files.iter().filter(|f| f.action == Action::Conflict) {
        let why = if !seen.contains(&f.path) {
            "was edited by hand and is no longer generated; delete it or keep it outside the output folder"
        } else if f.hand_edited {
            "has hand edits that overlap the lines generation changes; resolve them and run again"
        } else {
            "exists but was not written by the generator; move it away or delete it"
        };
        issues.push(crate::issue("conflict", Severity::Error, format!("{} {why}", f.path), Vec::new()));
    }
    let stopped = files.iter().any(|f| f.action == Action::Conflict);
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        generator: GENERATOR.to_string(),
        target: target.id().to_string(),
        spec_checksum: spec.checksum(),
        files: rows,
        renames: names.map().clone(),
    };
    let manifest_changed = old_text.as_deref() != Some(manifest.to_text().as_str());
    Ok(Plan { target: target.id().to_string(), files, issues, renames: rename_list, manifest, manifest_changed, stopped })
}

/// The action for one emitted file, given what is on disk and what the manifest says, with the
/// issues the protected regions raise. `syntax` is the comment syntax of a single-file target,
/// whose region bodies are carried over from disk and kept out of the merge.
fn decide(
    path: &str,
    pure: String,
    on_disk: Option<String>,
    entry: Option<&FileEntry>,
    disk: &dyn Disk,
    syntax: Option<(&str, &str)>,
) -> Result<(PlanFile, Vec<Issue>), CodegenError> {
    let file = |action, diff, write, generated, hand_edited| PlanFile { path: path.to_string(), action, diff, write, generated, hand_edited };
    let Some(d) = on_disk else {
        let diff = merge::unified(path, "", &pure);
        return Ok((file(Action::Create, diff, Some(pure.clone()), Some(pure), false), Vec::new()));
    };
    // What generation makes of this model with the hand-written regions on disk filled in: the
    // last generated copy and the checksum in the manifest.
    let carry = |text: &str| match syntax {
        Some((open, close)) => regions::carry(path, text, &d, open, close),
        None => (text.to_string(), Vec::new()),
    };
    let strip = |text: &str| match syntax {
        Some((open, close)) => regions::strip(text, open, close),
        None => text.to_string(),
    };
    let (generated, issues) = carry(&pure);
    let Some(entry) = entry else {
        // Not owned: adopt an identical file, refuse anything else.
        if d == generated {
            return Ok((file(Action::Unchanged, String::new(), None, Some(generated), false), issues));
        }
        let diff = merge::unified(path, &d, &generated);
        return Ok((file(Action::Conflict, diff, None, None, false), issues));
    };
    if sha256_hex(d.as_bytes()) == entry.sha256 {
        if d == generated {
            return Ok((file(Action::Unchanged, String::new(), None, Some(generated), false), issues));
        }
        let diff = merge::unified(path, &d, &generated);
        return Ok((file(Action::Update, diff, Some(generated.clone()), Some(generated), false), issues));
    }
    // Edited by hand since the last run.
    let Some(base) = disk.read(&Manifest::last_path(path))? else {
        let diff = merge::unified(path, &d, &generated);
        return Ok((file(Action::Conflict, diff, None, None, true), issues));
    };
    if base == generated {
        // Nothing new to bring in; the hand edit stays as it is.
        return Ok((file(Action::Unchanged, String::new(), None, Some(generated), true), issues));
    }
    match merge::three_way(&strip(&base), &strip(&d), &pure) {
        Ok(m) => {
            let (merged, _) = carry(&m);
            let diff = merge::unified(path, &d, &merged);
            let write = (merged != d).then_some(merged);
            Ok((file(Action::Merge, diff, write, Some(generated), true), issues))
        }
        Err(marked) => {
            let diff = merge::unified(path, &d, &marked);
            Ok((file(Action::Conflict, diff, None, None, true), issues))
        }
    }
}

/// Carry out `plan` in the output folder `root`. A stopped plan writes nothing.
pub fn apply(plan: &Plan, root: &Path) -> Result<Report, CodegenError> {
    let mut report = Report { issues: plan.issues.clone(), ..Default::default() };
    if plan.stopped {
        report.stopped = true;
        return Ok(report);
    }
    if root.parent().is_none() {
        return Err(CodegenError::Path(format!("{} is a filesystem root; name a folder for the output", root.display())));
    }
    std::fs::create_dir_all(root).map_err(|e| io(root, e))?;
    for f in &plan.files {
        match f.action {
            Action::Create | Action::Update | Action::Merge => {
                if let Some(text) = &f.write {
                    write_file(root, &f.path, text)?;
                }
                if let Some(g) = &f.generated {
                    write_if_changed(root, &Manifest::last_path(&f.path), g)?;
                }
                if f.action == Action::Merge {
                    report.merged.push(f.path.clone());
                } else {
                    report.written.push(f.path.clone());
                }
            }
            Action::Unchanged => {
                if let Some(g) = &f.generated {
                    write_if_changed(root, &Manifest::last_path(&f.path), g)?;
                }
                report.unchanged.push(f.path.clone());
            }
            Action::Delete => {
                remove_file(root, &f.path)?;
                remove_file(root, &Manifest::last_path(&f.path))?;
                report.deleted.push(f.path.clone());
            }
            Action::Conflict => {}
        }
    }
    write_if_changed(root, &Manifest::path(), &plan.manifest.to_text())?;
    Ok(report)
}

/// The full path of `rel` under `root`, creating the folders on the way and refusing a link or a
/// file where a folder should be.
fn prepare(root: &Path, rel: &str, create: bool) -> Result<PathBuf, CodegenError> {
    let rel = relative_path(rel)?;
    let parts: Vec<&str> = rel.split('/').collect();
    let mut p = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        p.push(part);
        let last = i + 1 == parts.len();
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => return Err(CodegenError::Path(format!("{rel}: goes through a symbolic link"))),
            Ok(m) if !last && !m.is_dir() => return Err(CodegenError::Path(format!("{rel}: `{part}` is a file, not a folder"))),
            Ok(m) if last && m.is_dir() => return Err(CodegenError::Path(format!("{rel}: is a folder"))),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !last && create {
                    std::fs::create_dir(&p).map_err(|e| io(&p, e))?;
                }
                if !last && !create {
                    return Ok(root.join(&rel));
                }
            }
            Err(e) => return Err(io(&p, e)),
        }
    }
    Ok(p)
}

/// Write `text` to `rel` atomically: a temporary file beside it, then a rename over it.
fn write_file(root: &Path, rel: &str, text: &str) -> Result<(), CodegenError> {
    let p = prepare(root, rel, true)?;
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = p.with_file_name(format!(".{name}.vizcraft-tmp"));
    if let Err(e) = std::fs::write(&tmp, text.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(&tmp, e));
    }
    if let Err(e) = std::fs::rename(&tmp, &p) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(&p, e));
    }
    Ok(())
}

/// Write `text` to `rel` unless it already holds exactly that.
fn write_if_changed(root: &Path, rel: &str, text: &str) -> Result<(), CodegenError> {
    if FsDisk::new(root).read(rel)?.as_deref() == Some(text) {
        return Ok(());
    }
    write_file(root, rel, text)
}

/// Remove `rel` if it is there.
fn remove_file(root: &Path, rel: &str) -> Result<(), CodegenError> {
    let p = prepare(root, rel, false)?;
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(&p, e)),
    }
}
