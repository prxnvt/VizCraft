//! VizCraft engine (L4): the [`Session`], undo, and the command registry.
//!
//! **Everything is a command.** Each user-visible action is a [`CommandSpec`] with a stable id
//! (`file.save`, `edit.undo`, `view.zoomIn`), a label, the ribbon/menu location, a default
//! shortcut, a params description and a `run` function. Menus, ribbon, shortcuts, the command
//! palette, the CLI and the JSON control channel all call [`Session::run`].
//!
//! Nothing here knows about egui, so the engine serves the desktop app, the CLI, the web shell
//! and (later) the MCP server alike.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod cmd;
pub mod doc;
pub mod io;
pub mod sample;
mod session;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use doc::{CoreProps, Document};
pub use session::{Selection, Session, ViewState};

/// Why a command did not run (or did not finish).
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// No command with this id.
    #[error("unknown command `{0}`")]
    Unknown(String),
    /// The params were missing something or had the wrong shape.
    #[error("bad parameters: {0}")]
    Params(String),
    /// The command exists but is disabled right now (the reason says why).
    #[error("not available: {0}")]
    Disabled(String),
    /// Anything else: I/O, a bad file, a panic inside the command.
    #[error("{0}")]
    Failed(String),
}

/// What a command returns: a JSON value for the caller (UI, CLI, control channel).
pub type CmdResult = Result<Value, EngineError>;

/// A registered command.
#[derive(Clone)]
pub struct CommandSpec {
    /// Stable id: `file.save`, `edit.undo`, `view.zoomIn`.
    pub id: &'static str,
    /// Short label as shown in menus and on buttons.
    pub label: &'static str,
    /// Where it lives in the UI (`Home > Clipboard`), for tooltips and parity checks.
    pub location: &'static str,
    /// Default shortcut in a portable form: `Mod+S`, `Mod+Shift+Z`, `F1`. `Mod` is Cmd on macOS
    /// and Ctrl elsewhere; ` / ` separates alternatives.
    pub shortcut: &'static str,
    /// One-line params doc for agents: `{"path"?: string}`.
    pub params: &'static str,
    /// `None` = enabled; `Some(reason)` = disabled, with the reason for tooltips.
    pub enabled: fn(&Session) -> Option<&'static str>,
    /// The command itself.
    pub run: fn(&mut Session, &Value) -> Result<Value, EngineError>,
}

fn always(_: &Session) -> Option<&'static str> {
    None
}

impl CommandSpec {
    /// A command with no shortcut, no params and always enabled; chain the builders to add them.
    pub const fn new(id: &'static str, label: &'static str, location: &'static str, run: fn(&mut Session, &Value) -> CmdResult) -> Self {
        CommandSpec { id, label, location, shortcut: "", params: "{}", enabled: always, run }
    }
    /// Set the default shortcut.
    pub const fn key(mut self, s: &'static str) -> Self {
        self.shortcut = s;
        self
    }
    /// Set the params doc.
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    /// Set the enablement check.
    pub const fn when(mut self, f: fn(&Session) -> Option<&'static str>) -> Self {
        self.enabled = f;
        self
    }
}

/// A command as the UI, CLI and control channel list it: the spec's text plus whether it is
/// enabled for the session it was asked of.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    pub shortcut: String,
    pub location: String,
    pub params: String,
    pub enabled: bool,
    /// Why it is disabled; empty when enabled.
    pub reason: String,
}

/// All commands by id.
#[derive(Clone)]
pub struct Registry {
    specs: Vec<CommandSpec>,
    by_id: HashMap<&'static str, usize>,
}

impl Registry {
    /// Build a registry from specs (a later spec with the same id wins).
    pub fn new(specs: Vec<CommandSpec>) -> Self {
        let mut by_id = HashMap::new();
        for (i, s) in specs.iter().enumerate() {
            by_id.insert(s.id, i);
        }
        Registry { specs, by_id }
    }
    /// The command with this id.
    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.by_id.get(id).and_then(|i| self.specs.get(*i))
    }
    /// Every command, in registration order (which is also menu order).
    pub fn all(&self) -> &[CommandSpec] {
        &self.specs
    }
    /// The command bound to a shortcut (`Mod+S`), if any. Case-insensitive.
    pub fn by_shortcut(&self, key: &str) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| !s.shortcut.is_empty() && s.shortcut.split(" / ").any(|k| k.eq_ignore_ascii_case(key)))
    }
}

impl Default for Registry {
    fn default() -> Self {
        cmd::registry()
    }
}

/// Parameter helpers: every reader tolerates junk, because agents send junk.
pub mod p {
    use serde_json::Value;

    use crate::EngineError;

    /// A string param.
    pub fn str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
        v.get(k).and_then(Value::as_str)
    }
    /// A required string param.
    pub fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
        str(v, k).ok_or_else(|| EngineError::Params(format!("`{k}` (string) is required")))
    }
    /// A finite number param.
    pub fn f32(v: &Value, k: &str) -> Option<f32> {
        v.get(k).and_then(Value::as_f64).map(|x| x as f32).filter(|x| x.is_finite())
    }
    /// A non-negative integer param (floats are truncated).
    pub fn usize(v: &Value, k: &str) -> Option<usize> {
        v.get(k).and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0 && f.is_finite()).map(|f| f as u64))).map(|n| n as usize)
    }
    /// A non-negative 64-bit integer param.
    pub fn u64(v: &Value, k: &str) -> Option<u64> {
        v.get(k).and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0 && f.is_finite()).map(|f| f as u64)))
    }
    /// A boolean param.
    pub fn bool(v: &Value, k: &str) -> Option<bool> {
        v.get(k).and_then(Value::as_bool)
    }
}

#[cfg(test)]
mod tests;
