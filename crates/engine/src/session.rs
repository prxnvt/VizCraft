//! The editing session: document, selection, undo/redo by snapshot, view state.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::doc::Document;
use crate::{CommandInfo, EngineError, Registry};

/// The selection: a character range in one note's body. `start <= end` is not required
/// (`end` is where the caret is); [`Selection::ordered`] sorts them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Selection {
    /// Index of the current note.
    pub note: usize,
    /// Character offsets into the note body.
    pub start: usize,
    pub end: usize,
}

impl Selection {
    /// A collapsed selection (a caret) at the start of a note.
    pub fn caret(note: usize, at: usize) -> Self {
        Selection { note, start: at, end: at }
    }
    /// Nothing is selected.
    pub fn is_collapsed(&self) -> bool {
        self.start == self.end
    }
    /// `(low, high)` character offsets.
    pub fn ordered(&self) -> (usize, usize) {
        if self.start <= self.end { (self.start, self.end) } else { (self.end, self.start) }
    }
}

/// View state that commands change (View tab, status bar). The UI reads it every frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ViewState {
    /// 1.0 = 100%.
    pub zoom: f32,
    pub dark_mode: bool,
    pub nav_pane: bool,
    pub props_pane: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState { zoom: 1.0, dark_mode: false, nav_pane: true, props_pane: true }
    }
}

/// One undo step: the document and selection as they were before a command, and that
/// command's label for "Undo Paste".
struct Undo {
    label: String,
    doc: Document,
    sel: Selection,
}

/// Steps kept; beyond this the oldest is dropped.
const MAX_UNDO: usize = 500;

/// An editing session: the single entry point for the UI, CLI and control channel.
pub struct Session {
    pub doc: Document,
    /// The file the document came from or was saved to.
    pub path: Option<std::path::PathBuf>,
    /// Unsaved changes.
    pub dirty: bool,
    /// User name, stamped into the metadata on save.
    pub author: String,
    pub view: ViewState,
    pub registry: Registry,
    /// Requests from commands to the UI (`{"open": "about"}`, `{"close": true}`), drained by the
    /// UI after each command. Programmatic callers never see a dialog.
    pub ui_requests: Vec<Value>,
    /// One-shot message for the status bar, taken by the UI.
    pub status: String,
    /// The current note and the selected range in it.
    pub sel: Selection,
    /// Internal clipboard, used when the host gives `edit.paste` no text.
    pub clipboard: String,
    history: Vec<Undo>,
    redo: Vec<Undo>,
    /// Consecutive `Typing` steps are one undo step until another command runs.
    typing_open: bool,
    /// Set by [`Session::set_document`] and [`Session::mark_saved`]: the running command replaced
    /// the document or only stamped it, so there is no step to undo.
    no_undo: bool,
    rev: u64,
}

impl Session {
    /// A session on this document, with the full registry.
    pub fn new(mut doc: Document) -> Self {
        doc.ensure_nonempty();
        Session {
            doc,
            path: None,
            dirty: false,
            author: "ProtoCraft User".into(),
            view: ViewState::default(),
            registry: crate::cmd::registry(),
            ui_requests: Vec::new(),
            status: String::new(),
            sel: Selection::default(),
            clipboard: String::new(),
            history: Vec::new(),
            redo: Vec::new(),
            typing_open: false,
            no_undo: false,
            rev: 1,
        }
    }

    /// Document revision; bumps on every document change (the UI caches by it).
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Record a change: new revision, unsaved.
    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.dirty = true;
    }

    /// Every command with its enablement for this session.
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.registry
            .all()
            .iter()
            .map(|c| {
                let reason = (c.enabled)(self);
                CommandInfo {
                    id: c.id.into(),
                    label: c.label.into(),
                    shortcut: c.shortcut.into(),
                    location: c.location.into(),
                    params: c.params.into(),
                    enabled: reason.is_none(),
                    reason: reason.unwrap_or("").into(),
                }
            })
            .collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Label of the command Undo would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.history.last().map(|u| u.label.as_str())
    }
    /// Label of the command Redo would do again.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|u| u.label.as_str())
    }

    /// Go back one step. False when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.typing_open = false;
        let Some(u) = self.history.pop() else { return false };
        let cur = Undo { label: u.label.clone(), doc: std::mem::replace(&mut self.doc, u.doc), sel: std::mem::replace(&mut self.sel, u.sel) };
        self.redo.push(cur);
        self.touch();
        true
    }

    /// Do the last undone step again. False when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.typing_open = false;
        let Some(u) = self.redo.pop() else { return false };
        let cur = Undo { label: u.label.clone(), doc: std::mem::replace(&mut self.doc, u.doc), sel: std::mem::replace(&mut self.sel, u.sel) };
        self.history.push(cur);
        self.touch();
        true
    }

    /// Replace the document (new, open): fresh history, not dirty.
    pub fn set_document(&mut self, mut doc: Document) {
        doc.ensure_nonempty();
        self.doc = doc;
        self.sel = Selection::default();
        self.history.clear();
        self.redo.clear();
        self.typing_open = false;
        self.no_undo = true;
        self.rev = self.rev.wrapping_add(1);
        self.dirty = false;
    }

    /// The document on disk now matches: not dirty, and the metadata stamped for the save is not
    /// an undo step (undoing back past a save is still possible).
    pub fn mark_saved(&mut self) {
        self.no_undo = true;
        self.rev = self.rev.wrapping_add(1);
        self.dirty = false;
    }

    /// Keep the selection inside the document.
    pub fn clamp_selection(&mut self) {
        self.doc.ensure_nonempty();
        self.sel.note = self.sel.note.min(self.doc.notes.len() - 1);
        let len = self.doc.notes.get(self.sel.note).map_or(0, |n| n.body.chars().count());
        self.sel.start = self.sel.start.min(len);
        self.sel.end = self.sel.end.min(len);
    }

    /// Body of the current note.
    pub fn current_body(&self) -> &str {
        self.doc.notes.get(self.sel.note).map_or("", |n| n.body.as_str())
    }

    /// The selected text.
    pub fn selected_text(&self) -> String {
        let (a, b) = self.sel.ordered();
        self.current_body().chars().skip(a).take(b - a).collect()
    }

    /// Run a command by id with JSON params. Unknown ids and disabled commands are errors. A
    /// command that changes the document becomes an undo step (consecutive typing is one step);
    /// a command that fails, or panics, leaves the document as it was.
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, EngineError> {
        let Some(spec) = self.registry.get(id).cloned() else { return Err(EngineError::Unknown(id.to_string())) };
        if let Some(why) = (spec.enabled)(self) {
            return Err(EngineError::Disabled(format!("{id}: {why}")));
        }
        let before_doc = self.doc.clone();
        let before_sel = self.sel;
        self.no_undo = false;
        let run = spec.run;
        // A bug inside one command must not take the app down with the user's work in it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(self, params)));
        let result = match result {
            Ok(r) => r,
            Err(p) => {
                let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                Err(EngineError::Failed(format!("{id} panicked: {msg}")))
            }
        };
        match &result {
            Ok(_) => {
                let manages_history = matches!(id, "edit.undo" | "edit.redo") || self.no_undo;
                if !manages_history && self.doc != before_doc {
                    self.record(spec.label, before_doc, before_sel);
                    self.touch();
                } else if !manages_history {
                    // A view or selection command: typing before and after it is separate steps.
                    self.typing_open = false;
                }
                self.clamp_selection();
            }
            Err(e) => {
                self.doc = before_doc;
                self.sel = before_sel;
                self.status = e.to_string();
            }
        }
        result
    }

    /// Push an undo step; consecutive `Typing` steps coalesce.
    fn record(&mut self, label: &str, doc: Document, sel: Selection) {
        let typing = label == "Typing";
        if typing && self.typing_open {
            self.redo.clear();
            return;
        }
        self.typing_open = typing;
        self.history.push(Undo { label: label.to_string(), doc, sel });
        if self.history.len() > MAX_UNDO {
            self.history.remove(0);
        }
        self.redo.clear();
    }
}
