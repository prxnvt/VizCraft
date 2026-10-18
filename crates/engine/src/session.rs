//! The editing session: document, selection, undo/redo by snapshot, view state, compound
//! transactions, the transaction log and script recording.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::doc::Document;
use crate::{CommandInfo, EngineError, Registry};

/// The selection state: selected entity IDs, primary element, and an optional continuous range
/// (such as a temporal span in a DAW or animation, or bounds).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Selection {
    /// Primary selected element identifier, if any.
    pub primary: Option<String>,
    /// Set of multi-selected element identifiers.
    pub items: Vec<String>,
    /// Optional range (such as temporal [t0, t1] in ms or coordinate offsets).
    pub range: Option<(u64, u64)>,
}

impl Selection {
    /// An empty selection.
    pub fn empty() -> Self {
        Selection::default()
    }
    /// A single selected item.
    pub fn single(id: impl Into<String>) -> Self {
        let id = id.into();
        Selection { primary: Some(id.clone()), items: vec![id], range: None }
    }
    /// A range selection.
    pub fn range(start: u64, end: u64) -> Self {
        Selection { primary: None, items: Vec::new(), range: Some((start, end)) }
    }
    /// Whether the selection is empty.
    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.items.is_empty() && self.range.is_none()
    }
    /// Whether `id` is part of the selection.
    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|i| i == id) || self.primary.as_deref() == Some(id)
    }
    /// Select a single item.
    pub fn select_single(&mut self, id: impl Into<String>) {
        let id = id.into();
        self.primary = Some(id.clone());
        self.items = vec![id];
    }
    /// Toggle item selection.
    pub fn toggle(&mut self, id: impl Into<String>) {
        let id = id.into();
        if let Some(pos) = self.items.iter().position(|i| i == &id) {
            self.items.remove(pos);
            if self.primary.as_deref() == Some(&id) {
                self.primary = self.items.first().cloned();
            }
        } else {
            self.items.push(id.clone());
            if self.primary.is_none() {
                self.primary = Some(id);
            }
        }
    }
    /// Clear the selection.
    pub fn clear(&mut self) {
        self.primary = None;
        self.items.clear();
        self.range = None;
    }
}

/// View state that commands change (View tab, status bar). The UI reads it every frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ViewState {
    /// 1.0 = 100%.
    pub zoom: f32,
    /// 2D pan offset in canvas coordinate units.
    pub pan: [f32; 2],
    pub dark_mode: bool,
    pub nav_pane: bool,
    pub props_pane: bool,
    /// Size of the canvas in screen points as the UI last laid it out, `None` with no window.
    /// The UI writes it every frame so `view.zoomFit` can do its arithmetic here, beside zoom and
    /// pan, and answer with the new zoom in the same call. It is a measurement, not a setting,
    /// so it is never serialized.
    #[serde(skip)]
    pub canvas: Option<[f32; 2]>,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState { zoom: 1.0, pan: [0.0, 0.0], dark_mode: false, nav_pane: true, props_pane: true, canvas: None }
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

/// Transaction log entries kept; beyond this the oldest is dropped.
pub const MAX_LOG: usize = 200;

/// Command ids one log entry keeps; a longer batch is cut here (the undo step is whole anyway).
pub const MAX_LOG_COMMANDS: usize = 256;

/// Most commands one transaction runs, nested ones included.
pub const MAX_TXN_COMMANDS: usize = 10_000;

/// Deepest nesting of transactions (a batch inside a script inside a batch ...).
pub const MAX_TXN_DEPTH: u32 = 8;

/// Most commands one recorded script holds; later ones are not recorded.
pub const MAX_SCRIPT: usize = 10_000;

/// Where a command came from, for the transaction log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The person at the window: menus, ribbon, shortcuts, canvas, palette.
    #[default]
    Ui,
    /// The JSON control channel.
    Control,
    /// The MCP server.
    Mcp,
    /// A replayed command script (`script.run`).
    Script,
}

/// One committed undo step as the transaction log keeps it: what to call it, who asked, and the
/// command ids that ran inside it (in the order they started, nested batches included).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnRecord {
    /// The undo label ("Paste", or the label a batch was given).
    pub label: String,
    /// Who ran it.
    pub origin: Origin,
    /// The command ids it ran, at most [`MAX_LOG_COMMANDS`].
    pub commands: Vec<String>,
}

/// The open transaction: the command ids run so far and whether one of them replaced the
/// document (then there is no undo step to make).
#[derive(Default)]
struct Txn {
    depth: u32,
    ids: Vec<String>,
    replaced: bool,
}

/// A script being recorded. Each entry carries the undo-step count after it ran, so Undo can
/// take back the entries of the step it undid and Redo can put them back.
#[derive(Default)]
struct Recording {
    steps: Vec<(Value, u64)>,
    undone: Vec<Vec<(Value, u64)>>,
}

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
    /// The active selection.
    pub sel: Selection,
    /// Internal clipboard, used when the host gives `edit.paste` no text.
    pub clipboard: String,
    /// Live issues: what the live rules of the model's notation packs report for the document as
    /// it is now. Recomputed after every document change, undo, redo and open.
    pub issues: Vec<vizcraft_model::Issue>,
    history: Vec<Undo>,
    redo: Vec<Undo>,
    /// Consecutive `Typing` steps are one undo step until another command runs.
    typing_open: bool,
    /// Set by [`Session::set_document`] and [`Session::mark_saved`]: the running command replaced
    /// the document or only stamped it, so there is no step to undo.
    no_undo: bool,
    rev: u64,
    /// Who is running commands now; set by [`Session::run_as`].
    pub(crate) origin: Origin,
    txn: Txn,
    log: VecDeque<TxnRecord>,
    recording: Option<Recording>,
    /// Undo steps made minus steps undone, for matching recorded commands to undo steps.
    steps: u64,
}

impl Session {
    /// A session on this document, with the full registry.
    pub fn new(doc: Document) -> Self {
        let mut s = Session {
            doc,
            path: None,
            dirty: false,
            author: "VizCraft User".into(),
            view: ViewState::default(),
            registry: crate::cmd::registry(),
            ui_requests: Vec::new(),
            status: String::new(),
            sel: Selection::default(),
            clipboard: String::new(),
            issues: Vec::new(),
            history: Vec::new(),
            redo: Vec::new(),
            typing_open: false,
            no_undo: false,
            rev: 1,
            origin: Origin::Ui,
            txn: Txn::default(),
            log: VecDeque::new(),
            recording: None,
            steps: 0,
        };
        s.refresh_issues();
        s
    }

    /// Recompute [`Session::issues`] from the live rules. A panic inside a rule must not take the
    /// app down: it leaves the list empty and says so in the status bar.
    pub fn refresh_issues(&mut self) {
        let doc = &self.doc;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vizcraft_notation::rules::live(doc, vizcraft_notation::catalog()))) {
            Ok(issues) => self.issues = issues,
            Err(_) => {
                self.issues = Vec::new();
                self.status = "the live checks failed; issues are not shown".into();
            }
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
        self.steps = self.steps.saturating_sub(1);
        self.touch();
        self.refresh_issues();
        true
    }

    /// Do the last undone step again. False when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.typing_open = false;
        let Some(u) = self.redo.pop() else { return false };
        let cur = Undo { label: u.label.clone(), doc: std::mem::replace(&mut self.doc, u.doc), sel: std::mem::replace(&mut self.sel, u.sel) };
        self.history.push(cur);
        self.steps = self.steps.saturating_add(1);
        self.touch();
        self.refresh_issues();
        true
    }

    /// Replace the document (new, open): fresh history, not dirty.
    pub fn set_document(&mut self, doc: Document) {
        self.doc = doc;
        self.sel = Selection::default();
        self.history.clear();
        self.redo.clear();
        self.typing_open = false;
        self.no_undo = true;
        if self.txn.depth > 0 {
            self.txn.replaced = true;
        }
        self.rev = self.rev.wrapping_add(1);
        self.dirty = false;
        self.refresh_issues();
    }

    /// The document on disk now matches: not dirty, and the metadata stamped for the save is not
    /// an undo step (undoing back past a save is still possible).
    pub fn mark_saved(&mut self) {
        self.no_undo = true;
        self.rev = self.rev.wrapping_add(1);
        self.dirty = false;
    }

    /// Keep the selection inside valid bounds.
    pub fn clamp_selection(&mut self) {}

    /// Who is running commands now.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Sets who the next commands run for and returns the previous origin, for callers such as
    /// the UI's control channel that wrap more than [`Session::run`] and must restore it after.
    pub fn set_origin(&mut self, origin: Origin) -> Origin {
        std::mem::replace(&mut self.origin, origin)
    }

    /// [`Session::run`] on behalf of `origin` (the control channel, the MCP server), so the
    /// transaction log says who made the step. Commands it runs in turn (a batch's) share it.
    pub fn run_as(&mut self, origin: Origin, id: &str, params: &Value) -> Result<Value, EngineError> {
        let before = std::mem::replace(&mut self.origin, origin);
        let r = self.run(id, params);
        self.origin = before;
        r
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
        let before_sel = self.sel.clone();
        self.no_undo = false;
        let run = spec.run;
        let manages_undo = matches!(id, "edit.undo" | "edit.redo") || is_transaction(id);
        // A bug inside one command must not take the app down with the user's work in it. The
        // model sync runs inside the same guard: it changes the document as part of the command.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let r = run(self, params);
            // Views and model are brought into step after every change (connectors bound to
            // relationships, names mirrored), inside the same undo step, before rerouting. Not
            // after a command that replaced the document (open, new): `before_doc` is another file.
            if r.is_ok() && !manages_undo && !self.no_undo && self.doc != before_doc {
                vizcraft_notation::bind::sync(&mut self.doc, &before_doc, vizcraft_notation::catalog());
            }
            r
        }));
        let result = match result {
            Ok(r) => r,
            Err(p) => {
                let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                Err(EngineError::Failed(format!("{id} panicked: {msg}")))
            }
        };
        match &result {
            Ok(_) => {
                // Connectors that avoid shapes are rerouted after every change, inside the same
                // undo step, so a moved shape never leaves a route running through another.
                let changed = self.doc != before_doc;
                if !manages_undo && changed {
                    crate::cmd::page::auto_size(&mut self.doc);
                    vizcraft_diagram::autoroute::refresh(&mut self.doc);
                }
                let manages_history = manages_undo || self.no_undo;
                if !manages_history && changed {
                    // Inside a transaction the transaction makes the one step when it commits.
                    if self.txn.depth == 0 {
                        self.record(spec.label, vec![id.to_string()], before_doc, before_sel);
                    }
                    self.touch();
                    self.refresh_issues();
                } else if !manages_history {
                    // A view or selection command: typing before and after it is separate steps.
                    self.typing_open = false;
                }
                self.clamp_selection();
                self.note(id, params);
            }
            Err(e) => {
                self.doc = before_doc;
                self.sel = before_sel;
                self.status = e.to_string();
            }
        }
        result
    }

    /// Push an undo step and its log entry; consecutive `Typing` steps coalesce into one of each.
    fn record(&mut self, label: &str, mut commands: Vec<String>, doc: Document, sel: Selection) {
        let typing = label == "Typing";
        if typing && self.typing_open {
            self.redo.clear();
            if let Some(last) = self.log.back_mut() {
                let room = MAX_LOG_COMMANDS.saturating_sub(last.commands.len());
                last.commands.extend(commands.into_iter().take(room));
            }
            return;
        }
        self.typing_open = typing;
        self.history.push(Undo { label: label.to_string(), doc, sel });
        if self.history.len() > MAX_UNDO {
            self.history.remove(0);
        }
        self.redo.clear();
        self.steps = self.steps.saturating_add(1);
        if let Some(rec) = self.recording.as_mut() {
            rec.undone.clear();
        }
        commands.truncate(MAX_LOG_COMMANDS);
        self.log.push_back(TxnRecord { label: label.to_string(), origin: self.origin, commands });
        if self.log.len() > MAX_LOG {
            self.log.pop_front();
        }
    }

    /// The transaction log: one entry per committed undo step, oldest first, at most [`MAX_LOG`].
    /// Undo and redo do not add entries.
    pub fn transactions(&self) -> &VecDeque<TxnRecord> {
        &self.log
    }

    /// Run `steps` (`(command id, params)`) as one transaction: one undo step labelled `label`
    /// when the document changed, one log entry. When a step fails, every step before it is taken
    /// back (document, selection, path, history) and the error names the failing step as
    /// `commands[i] (id) failed: ...`. Undo and redo cannot run inside a transaction; a
    /// transaction may nest up to [`MAX_TXN_DEPTH`] deep and then joins the outer one. Returns
    /// each step's result.
    pub fn transaction(&mut self, label: &str, steps: &[(String, Value)]) -> Result<Vec<Value>, EngineError> {
        if self.txn.depth >= MAX_TXN_DEPTH {
            return Err(EngineError::Params(format!("transactions nest more than {MAX_TXN_DEPTH} deep")));
        }
        if self.txn.ids.len().saturating_add(steps.len()) > MAX_TXN_COMMANDS {
            return Err(EngineError::Params(format!("a transaction runs at most {MAX_TXN_COMMANDS} commands")));
        }
        let outer = self.txn.depth == 0;
        let before_doc = self.doc.clone();
        let before_sel = self.sel.clone();
        let before_path = self.path.clone();
        let before_dirty = self.dirty;
        let before_typing = self.typing_open;
        let before_ids = self.txn.ids.len();
        let before_replaced = self.txn.replaced;
        // The outer transaction holds the stacks aside so a command that replaces the document
        // (which clears them) can still be taken back whole.
        let (history, redo) = if outer { (std::mem::take(&mut self.history), std::mem::take(&mut self.redo)) } else { (Vec::new(), Vec::new()) };
        self.txn.depth += 1;
        let mut results = Vec::with_capacity(steps.len());
        let mut failure = None;
        for (i, (id, params)) in steps.iter().enumerate() {
            if matches!(id.as_str(), "edit.undo" | "edit.redo") {
                failure = Some((i, EngineError::Params(format!("`{id}` cannot run inside a batch"))));
                break;
            }
            if self.txn.ids.len() >= MAX_TXN_COMMANDS {
                failure = Some((i, EngineError::Params(format!("a transaction runs at most {MAX_TXN_COMMANDS} commands"))));
                break;
            }
            self.txn.ids.push(id.clone());
            match self.run(id, params) {
                Ok(v) => results.push(v),
                Err(e) => {
                    failure = Some((i, e));
                    break;
                }
            }
        }
        self.txn.depth = self.txn.depth.saturating_sub(1);
        if let Some((i, e)) = failure {
            self.doc = before_doc;
            self.sel = before_sel;
            self.path = before_path;
            self.dirty = before_dirty;
            self.typing_open = before_typing;
            self.txn.ids.truncate(before_ids);
            self.txn.replaced = before_replaced;
            if outer {
                self.history = history;
                self.redo = redo;
                self.txn = Txn::default();
            }
            self.rev = self.rev.wrapping_add(1);
            self.refresh_issues();
            let id = steps.get(i).map(|(id, _)| crate::cmd::short(id)).unwrap_or_default();
            return Err(EngineError::Failed(format!("commands[{i}] ({id}) failed: {e}; nothing was changed")));
        }
        if outer {
            let txn = std::mem::take(&mut self.txn);
            if !txn.replaced {
                self.history = history;
                self.redo = redo;
                if self.doc != before_doc {
                    self.typing_open = false;
                    self.record(label, txn.ids, before_doc, before_sel);
                } else {
                    self.dirty = before_dirty;
                    self.typing_open = before_typing;
                }
            }
        }
        Ok(results)
    }

    /// Start recording a script; a recording already running starts over.
    pub fn record_start(&mut self) {
        self.recording = Some(Recording::default());
    }

    /// Whether a script is being recorded.
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording and return the script: `[{command, params}]`, `None` when nothing was
    /// being recorded.
    pub fn record_stop(&mut self) -> Option<Vec<Value>> {
        self.recording.take().map(|r| r.steps.into_iter().map(|(v, _)| v).collect())
    }

    /// Add a command that just succeeded to the recording. Only commands run directly are
    /// recorded (a batch is recorded as the batch), and never view, UI, app, help, file or script
    /// commands. Undo takes back the recorded commands of the step it undid, Redo puts them back,
    /// so the script replays to the document as it is.
    fn note(&mut self, id: &str, params: &Value) {
        if self.txn.depth > 0 {
            return;
        }
        let steps = self.steps;
        let Some(rec) = self.recording.as_mut() else { return };
        match id {
            "edit.undo" => {
                let keep = rec.steps.iter().position(|(_, n)| *n > steps).unwrap_or(rec.steps.len());
                let group = rec.steps.split_off(keep);
                if !group.is_empty() {
                    rec.undone.push(group);
                }
            }
            "edit.redo" => {
                if let Some(group) = rec.undone.pop() {
                    rec.steps.extend(group);
                }
            }
            _ if !recordable(id) => {}
            _ if rec.steps.len() >= MAX_SCRIPT => {}
            _ => rec.steps.push((serde_json::json!({"command": id, "params": params}), steps)),
        }
    }
}

/// Commands that run other commands as one transaction and make the undo step themselves.
fn is_transaction(id: &str) -> bool {
    matches!(id, "edit.batch" | "script.run")
}

/// Whether a script records this command: not view-only, UI, app, help, file or script commands.
pub fn recordable(id: &str) -> bool {
    !["view.", "ui.", "app.", "help.", "file.", "script."].iter().any(|p| id.starts_with(p))
}
