//! The sample document (`--sample`, `file.new {"template": "sample"}`): enough notes to make the
//! panes, undo and the status bar show something.

use crate::doc::{Document, Note};

/// A short set of notes about ProtoCraft itself.
pub fn sample_document() -> Document {
    Document {
        title: "Welcome to ProtoCraft".into(),
        notes: vec![
            Note::new(
                "What this is",
                "ProtoCraft is the baseline every craft app starts from: the window chrome, menus, ribbon, panes, \
                 preferences and control channel that WordCraft, GridCraft, DeckCraft and VectorCraft share.\n\n\
                 This sample document is a list of notes. A real craft replaces it with its own model.",
            ),
            Note::new(
                "Everything is a command",
                "Every button, menu item and shortcut runs a command by id: file.save, edit.undo, view.zoomIn.\n\
                 The command palette (search box in the title bar) lists them all, and so does the CLI:\n\n    protocraft-cli commands",
            ),
            Note::new(
                "Try it",
                "Type here. Undo with Mod+Z. Add a note from the Insert tab. Zoom with the slider in the status bar. \
                 Switch to dark mode from the View tab.",
            ),
        ],
        core: Default::default(),
    }
}
