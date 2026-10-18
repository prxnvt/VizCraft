//! Saved macros: a recorded script kept under a name, one JSON file per macro in the folder
//! [`set_dir`] names. The Macros group of the View tab records with `script.recordStart` and
//! `script.recordStop` and saves, runs and deletes through these commands.
//!
//! Names are cut down to a safe file stem (letters, digits, `_` and `-`), so a name can never
//! reach outside the folder. At most [`MAX_MACROS`] macros of [`MAX_MACRO_BYTES`] each are kept.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::batch::{results_of, steps_of};
use super::short;
use crate::session::Origin;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Environment variable that names the macro folder; it wins over [`set_dir`], so a headless run
/// or a test can point at a folder of its own.
pub const DIR_ENV: &str = "VIZCRAFT_MACROS_DIR";
/// Most macros kept.
pub const MAX_MACROS: usize = 256;
/// Largest macro file, in bytes.
pub const MAX_MACRO_BYTES: usize = 64 * 1024;
/// Longest safe file stem, in characters.
const MAX_STEM: usize = 64;

#[cfg(not(test))]
static DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

// Unit tests run in parallel threads of one process; a process-wide folder would let one test's
// macros show up in another's listing.
#[cfg(test)]
thread_local! {
    static DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Name the folder macros are kept in (`None` leaves macros without a folder, and the commands
/// say so). The folder is created when the first macro is saved.
pub fn set_dir(dir: Option<PathBuf>) {
    #[cfg(not(test))]
    if let Ok(mut lock) = DIR.write() {
        *lock = dir;
    }
    #[cfg(test)]
    DIR.with(|d| *d.borrow_mut() = dir);
}

fn configured_dir() -> Option<PathBuf> {
    #[cfg(not(test))]
    return DIR.read().ok().and_then(|l| l.clone());
    #[cfg(test)]
    return DIR.with(|d| d.borrow().clone());
}

/// The macro folder in effect: `VIZCRAFT_MACROS_DIR` when set and not empty, else [`set_dir`]'s.
pub fn dir() -> Result<PathBuf, EngineError> {
    if let Some(d) = std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    configured_dir().ok_or_else(|| EngineError::Failed("macros have no folder to live in here".into()))
}

/// The macro commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("macro.save", "Save Macro", "View > Macros", save).params(
            r#"{"name": string, "script"?: [{command, params?}] | string (that list as JSON)} -> {name, commands}; without `script` the recording in progress is stopped and saved"#,
        ),
        CommandSpec::new("macro.list", "List Macros", "View > Macros", list).params("{} -> {macros: [{name, commands}]}"),
        CommandSpec::new("macro.run", "Run Macro", "View > Macros", run)
            .params(r#"{"name": string} -> {name, completed, results: [{command, result}]}; one transaction, one undo step"#),
        CommandSpec::new("macro.delete", "Delete Macro", "View > Macros", delete).params(r#"{"name": string}"#),
    ]
}

/// A safe file stem for `name`: letters, digits, `_` and `-` kept, runs of anything else (spaces,
/// dots, slashes) becoming one `_`, at most [`MAX_STEM`] characters. Errors when nothing is left.
pub fn stem_of(name: &str) -> Result<String, EngineError> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out: String = out.trim_matches('_').chars().take(MAX_STEM).collect();
    if out.is_empty() {
        return Err(EngineError::Params("`name` needs at least one letter or digit".into()));
    }
    Ok(out)
}

fn path_of(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.json"))
}

/// Every macro file's stem in `dir`, sorted; an unreadable or missing folder holds none.
fn stems(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<String> = read
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(str::to_string))
        .take(MAX_MACROS * 2)
        .collect();
    v.sort();
    v
}

/// A macro read from disk: its display name and steps.
struct Saved {
    name: String,
    script: Value,
}

fn read_macro(dir: &Path, stem: &str) -> Result<Saved, EngineError> {
    let path = path_of(dir, stem);
    let meta = std::fs::metadata(&path).map_err(|_| EngineError::Params(format!("no macro named `{}`", short(stem))))?;
    if meta.len() > MAX_MACRO_BYTES as u64 {
        return Err(EngineError::Failed(format!("macro `{}` is too large", short(stem))));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| EngineError::Failed(format!("could not read macro `{}`: {e}", short(stem))))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| EngineError::Failed(format!("macro `{}` is not JSON: {e}", short(stem))))?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or(stem).chars().take(MAX_STEM).collect();
    Ok(Saved { name, script: v.get("script").cloned().unwrap_or(Value::Null) })
}

/// The names of the saved macros, sorted, for a menu. Reads the folder directly instead of
/// running `macro.list`, so drawing a menu while a script is being recorded does not add a step.
pub fn names() -> Vec<String> {
    let Ok(dir) = dir() else { return Vec::new() };
    stems(&dir).into_iter().take(MAX_MACROS).filter_map(|stem| read_macro(&dir, &stem).ok().map(|m| m.name)).collect()
}

fn save(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let stem = stem_of(name)?;
    let dir = dir()?;
    let script = match v.get("script") {
        Some(Value::String(text)) => {
            if text.len() > MAX_MACRO_BYTES {
                return Err(EngineError::Params("the script is too large to keep as a macro".into()));
            }
            serde_json::from_str::<Value>(text).map_err(|e| EngineError::Params(format!("the script is not JSON: {e}")))?
        }
        Some(Value::Null) | None => match s.record_stop() {
            Some(list) => Value::Array(list),
            None => return Err(EngineError::Params("give a `script`, or record one first".into())),
        },
        Some(list) => list.clone(),
    };
    let steps = steps_of(&script, "script")?;
    if steps.is_empty() {
        return Err(EngineError::Params("the script is empty".into()));
    }
    let shown: String = name.trim().chars().take(MAX_STEM).collect();
    let body = serde_json::to_string_pretty(&json!({"name": shown, "script": script})).map_err(|e| EngineError::Failed(e.to_string()))?;
    if body.len() > MAX_MACRO_BYTES {
        return Err(EngineError::Params(format!("a macro is at most {} KB", MAX_MACRO_BYTES / 1024)));
    }
    if !path_of(&dir, &stem).exists() && stems(&dir).len() >= MAX_MACROS {
        return Err(EngineError::Params(format!("at most {MAX_MACROS} macros are kept; delete one first")));
    }
    std::fs::create_dir_all(&dir).map_err(|e| EngineError::Failed(format!("could not create the macro folder: {e}")))?;
    // Written beside the target and renamed over it, so a failed write never leaves half a macro.
    let tmp = dir.join(format!("{stem}.tmp"));
    std::fs::write(&tmp, body).map_err(|e| EngineError::Failed(format!("could not save the macro: {e}")))?;
    std::fs::rename(&tmp, path_of(&dir, &stem)).map_err(|e| EngineError::Failed(format!("could not save the macro: {e}")))?;
    Ok(json!({"name": shown, "commands": steps.len()}))
}

fn list(_: &mut Session, _: &Value) -> CmdResult {
    let dir = dir()?;
    let macros: Vec<Value> = stems(&dir)
        .into_iter()
        .take(MAX_MACROS)
        .filter_map(|stem| {
            let m = read_macro(&dir, &stem).ok()?;
            Some(json!({"name": m.name, "commands": m.script.as_array().map_or(0, Vec::len)}))
        })
        .collect();
    Ok(json!({"macros": macros}))
}

fn run(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let dir = dir()?;
    let m = read_macro(&dir, &stem_of(name)?)?;
    let steps = steps_of(&m.script, "script")?;
    if steps.is_empty() {
        return Err(EngineError::Failed("the macro is empty".into()));
    }
    let before = std::mem::replace(&mut s.origin, Origin::Script);
    let results = s.transaction(&m.name, &steps);
    s.origin = before;
    let results = results?;
    Ok(json!({"name": m.name, "completed": results.len(), "results": results_of(&steps, results)}))
}

fn delete(_: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let dir = dir()?;
    let stem = stem_of(name)?;
    std::fs::remove_file(path_of(&dir, &stem)).map_err(|_| EngineError::Params(format!("no macro named `{}`", short(name))))?;
    Ok(json!({"deleted": name}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vizcraft-macros-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Tests set the folder per thread; a `VIZCRAFT_MACROS_DIR` in the environment would win over it.
    fn env_free() -> bool {
        std::env::var_os(DIR_ENV).is_none_or(|d| d.is_empty())
    }

    #[test]
    fn stems_are_safe_file_names() {
        assert_eq!(stem_of("My Macro").unwrap(), "My_Macro");
        assert_eq!(stem_of("../../etc/passwd").unwrap(), "etc_passwd");
        assert_eq!(stem_of("  a..b  ").unwrap(), "a_b");
        assert_eq!(stem_of(&"x".repeat(500)).unwrap().len(), MAX_STEM);
        assert!(stem_of("...").is_err());
        assert!(stem_of("").is_err());
    }

    #[test]
    fn a_recorded_macro_is_saved_listed_run_and_deleted() {
        if !env_free() {
            return;
        }
        let dir = temp_dir("roundtrip");
        set_dir(Some(dir.clone()));
        let mut s = Session::new(vizcraft_diagram::Document::new());
        s.run("script.recordStart", &json!({})).unwrap();
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": "m1", "x": 10, "y": 10, "w": 40, "h": 20})).unwrap();
        let stopped = s.run("script.recordStop", &json!({})).unwrap();
        let saved = s.run("macro.save", &json!({"name": "Add box", "script": stopped["script"]})).unwrap();
        assert_eq!(saved["commands"], 1);
        assert!(dir.join("Add_box.json").is_file());
        let listed = s.run("macro.list", &json!({})).unwrap();
        assert_eq!(listed["macros"], json!([{"name": "Add box", "commands": 1}]));
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.page().unwrap().shape("m1").is_none());
        let ran = s.run("macro.run", &json!({"name": "Add box"})).unwrap();
        assert_eq!(ran["completed"], 1);
        assert!(s.doc.page().unwrap().shape("m1").is_some(), "the macro replayed");
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.page().unwrap().shape("m1").is_none(), "one undo step for the whole macro");
        s.run("macro.delete", &json!({"name": "Add box"})).unwrap();
        assert!(s.run("macro.run", &json!({"name": "Add box"})).is_err());
        assert!(s.run("macro.delete", &json!({"name": "Add box"})).is_err());
        assert_eq!(s.run("macro.list", &json!({})).unwrap()["macros"], json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_without_a_script_stops_the_recording_in_progress() {
        if !env_free() {
            return;
        }
        let dir = temp_dir("stop");
        set_dir(Some(dir.clone()));
        let mut s = Session::new(vizcraft_diagram::Document::new());
        assert!(s.run("macro.save", &json!({"name": "none"})).is_err(), "nothing recorded");
        s.run("script.recordStart", &json!({})).unwrap();
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "w": 10, "h": 10})).unwrap();
        s.run("macro.save", &json!({"name": "live"})).unwrap();
        assert!(!s.is_recording());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limits_and_hostile_input_are_refused() {
        if !env_free() {
            return;
        }
        let dir = temp_dir("limits");
        set_dir(Some(dir.clone()));
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let one = json!([{"command": "edit.selectAll", "params": {}}]);
        for bad in [
            json!({"name": 5, "script": one}),
            json!({"name": "ok", "script": "not json"}),
            json!({"name": "ok", "script": []}),
            json!({"name": "ok", "script": [{"params": {}}]}),
            json!({"name": "///", "script": one}),
            json!({"name": "big", "script": [{"command": "x", "params": {"pad": "y".repeat(MAX_MACRO_BYTES)}}]}),
        ] {
            assert!(s.run("macro.save", &bad).is_err(), "{bad}");
        }
        assert!(stems(&dir).is_empty(), "nothing was written");
        for i in 0..MAX_MACROS {
            s.run("macro.save", &json!({"name": format!("m{i}"), "script": one})).unwrap();
        }
        assert!(s.run("macro.save", &json!({"name": "one too many", "script": one})).is_err());
        s.run("macro.save", &json!({"name": "m0", "script": one})).expect("replacing a macro is not adding one");
        // A macro file that is not what we wrote is an error when run, not a panic.
        std::fs::write(dir.join("junk.json"), "[1,2").unwrap();
        assert!(s.run("macro.run", &json!({"name": "junk"})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_folder_the_commands_say_so() {
        if !env_free() {
            return;
        }
        set_dir(None);
        let mut s = Session::new(vizcraft_diagram::Document::new());
        assert!(s.run("macro.list", &json!({})).is_err());
        assert!(s.run("macro.save", &json!({"name": "a", "script": [{"command": "edit.selectAll"}]})).is_err());
    }
}
