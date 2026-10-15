//! The sample document model: a titled list of notes. A real craft replaces this module (and
//! `io.rs`, `sample.rs`) with its own model; everything else in the engine stays.

use serde::{Deserialize, Serialize};

/// Document metadata, the part every craft's model carries in some form.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreProps {
    /// Who made the document.
    pub author: String,
    /// Who saved it last.
    pub last_modified_by: String,
    /// ISO-8601 timestamps, empty until the first save.
    pub created: String,
    pub modified: String,
    /// Save count.
    pub revision: u32,
}

/// One note: a title and a plain-text body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Note {
    pub title: String,
    pub body: String,
}

impl Note {
    /// A note with this title and body.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Note { title: title.into(), body: body.into() }
    }
}

/// The sample document: a title, notes and metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Document {
    /// Shown in the title bar when the document has no file yet.
    pub title: String,
    pub notes: Vec<Note>,
    pub core: CoreProps,
}

impl Document {
    /// An empty document with one blank note, so there is always something to type into.
    pub fn new() -> Self {
        Document { title: String::new(), notes: vec![Note::default()], core: CoreProps::default() }
    }

    /// Make sure there is at least one note (commands and the UI assume it).
    pub fn ensure_nonempty(&mut self) {
        if self.notes.is_empty() {
            self.notes.push(Note::default());
        }
    }

    /// Characters in every note body, for the status bar.
    pub fn char_count(&self) -> usize {
        self.notes.iter().map(|n| n.body.chars().count()).sum()
    }
}
