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
}

impl Issue {
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
    /// The same issue on this page.
    pub fn on_page(mut self, page: impl Into<String>) -> Self {
        self.page = Some(page.into());
        self
    }
    /// A key that is equal for the same problem found twice (rule, subjects, items and page,
    /// not the message), used to tell which issues a command batch raised.
    pub fn key(&self) -> String {
        format!("{}|{}|{}|{}", self.rule, self.subjects.join(","), self.items.join(","), self.page.as_deref().unwrap_or(""))
    }
}
