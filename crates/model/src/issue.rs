//! Structured issues: what a rule or the identity registry found wrong, in a form the Problems
//! tab, the control channel and agents over MCP can act on.

use serde::{Deserialize, Serialize};

/// How bad an issue is. Ordered from most to least severe, so sorting puts errors first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The model breaks a rule of its notation.
    Error,
    /// Probably a mistake.
    #[default]
    Warning,
    /// Worth knowing.
    Info,
}

impl Severity {
    /// The name used in params and results: `error`, `warning`, `info`.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
    /// The severity for a name, or `None` for an unknown name.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "info" => Some(Severity::Info),
            _ => None,
        }
    }
}

/// One problem: which rule found it, how bad it is, what it is about and a message in plain words.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Issue {
    /// Pack-qualified rule id (`flowchart.illegalFlow`), or `model.<name>` for checks that belong
    /// to no pack.
    pub rule: String,
    /// How bad it is.
    pub severity: Severity,
    /// What is wrong, in plain words, naming elements by name or tag.
    pub message: String,
    /// Element and relationship ids the issue is about, most relevant first.
    pub subjects: Vec<String>,
    /// Shape and connector ids that draw the subjects (or that the issue is about directly, such
    /// as a connector with a free end), so a person or agent can go to them.
    pub items: Vec<String>,
    /// Id of the page the items are on, when the issue is about one page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Commands that would repair the issue, best first, at most [`Issue::MAX_FIXES`]. Left out
    /// of the serde form when empty, so issues without fixes read as before.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
}

/// A repair a person or agent can run: a command id and its params, with a label for a menu.
/// Running `command` with `params` through the engine applies it; nothing runs on its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Fix {
    /// What the fix does, in plain words (`Reroute around shapes`).
    pub label: String,
    /// The engine command id (`connector.reroute`).
    pub command: String,
    /// The command's params.
    pub params: serde_json::Value,
}

impl Fix {
    /// A fix running `command` with `params`.
    pub fn new(label: impl Into<String>, command: impl Into<String>, params: serde_json::Value) -> Self {
        Fix { label: label.into(), command: command.into(), params }
    }
}

impl Issue {
    /// Most fixes one issue offers; more are dropped, so a list of issues stays small.
    pub const MAX_FIXES: usize = 4;

    /// An issue with a rule, severity and message and nothing it points at yet.
    pub fn new(rule: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Issue { rule: rule.into(), severity, message: message.into(), ..Issue::default() }
    }
    /// The same issue about these model items.
    pub fn with_subjects(mut self, subjects: Vec<String>) -> Self {
        self.subjects = subjects;
        self
    }
    /// The same issue pointing at these view items.
    pub fn with_items(mut self, items: Vec<String>) -> Self {
        self.items = items;
        self
    }
    /// The same issue offering one more fix (ignored past [`Issue::MAX_FIXES`]).
    pub fn with_fix(mut self, fix: Fix) -> Self {
        if self.fixes.len() < Self::MAX_FIXES {
            self.fixes.push(fix);
        }
        self
    }
    /// The same issue on this page.
    pub fn on_page(mut self, page: impl Into<String>) -> Self {
        self.page = Some(page.into());
        self
    }
    /// A key that is equal for the same problem found twice (rule, subjects, items and page,
    /// not the message, severity or fixes), used to tell which issues a command batch raised.
    pub fn key(&self) -> String {
        format!("{}|{}|{}|{}", self.rule, self.subjects.join(","), self.items.join(","), self.page.as_deref().unwrap_or(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_are_capped_and_left_out_when_empty() {
        let mut i = Issue::new("t.r", Severity::Error, "m");
        let plain = serde_json::to_value(&i).unwrap();
        assert!(plain.get("fixes").is_none());
        for n in 0..10 {
            i = i.with_fix(Fix::new(format!("fix {n}"), "connector.reroute", serde_json::json!({"ids": ["c1"]})));
        }
        assert_eq!(i.fixes.len(), Issue::MAX_FIXES);
        let v = serde_json::to_value(&i).unwrap();
        assert_eq!(v["fixes"][0]["command"], "connector.reroute");
        let back: Issue = serde_json::from_value(v).unwrap();
        assert_eq!(back, i);
        // Fixes do not change which problem it is.
        assert_eq!(back.key(), Issue::new("t.r", Severity::Info, "other").key());
    }
}
