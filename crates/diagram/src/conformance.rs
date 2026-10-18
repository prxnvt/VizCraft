//! The conformance profile: which well-formedness rules a document is held to.
//!
//! A document says whether its rules run at all (`enabled`), which named rulesets of the notation
//! packs apply (`rulesets`, such as `flowchart.iso5807` or `drawing.tidy`; empty means every rule
//! of every pack the model uses), per-rule changes (`overrides`: another severity, or off) and
//! issues silenced with a reason (`suppressions`, by `Issue::key`). The rules engine
//! (`vizcraft_notation::rules`) reads it before anything runs.
//!
//! Free drawing starts with the profile off. Until the user sets it (`user_set`), the notation
//! sync turns it on with the first ruleset of each pack the document starts to use, so choosing
//! a notation brings its rules along and a sketch stays quiet. Everything here is capped and
//! read tolerantly, because files and command params are hostile.

use serde::{Deserialize, Serialize};
use vizcraft_model::Severity;

use crate::serde_util::{is_default, lenient, lenient_vec};

/// Most rulesets one profile names.
pub const MAX_RULESETS: usize = 64;
/// Most rule overrides one profile holds.
pub const MAX_OVERRIDES: usize = 1_000;
/// Most suppressions one profile holds.
pub const MAX_SUPPRESSIONS: usize = 10_000;
/// Longest ruleset or rule id kept, in bytes.
pub const MAX_ID: usize = 256;
/// Longest suppressed issue key kept, in bytes (keys list subjects and items).
pub const MAX_KEY: usize = 4_096;
/// Longest suppression reason kept, in bytes.
pub const MAX_REASON: usize = 1_000;

/// A document's conformance profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "ConformanceRepr")]
pub struct Conformance {
    /// Whether any rule runs. Off for free drawing.
    #[serde(skip_serializing_if = "is_default")]
    pub enabled: bool,
    /// The user (or an agent) chose `enabled` and `rulesets`; the sync no longer changes them.
    #[serde(skip_serializing_if = "is_default")]
    pub user_set: bool,
    /// Ruleset ids, pack-qualified. A pack id on its own stands for every rule of that pack.
    /// Empty: every rule of every pack the model uses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rulesets: Vec<String>,
    /// Per-rule changes, at most one per rule (the first wins).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<RuleOverride>,
    /// Silenced issues.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suppressions: Vec<Suppression>,
}

/// A change to one rule: another severity, or switched off.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleOverride {
    /// The rule id (`flowchart.singleExit`).
    #[serde(deserialize_with = "lenient")]
    pub rule: String,
    /// The severity its issues get instead of the declared one.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "lenient")]
    pub severity: Option<Severity>,
    /// The rule does not run.
    #[serde(skip_serializing_if = "is_default", deserialize_with = "lenient")]
    pub disabled: bool,
}

/// One silenced issue.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Suppression {
    /// The issue's key (`Issue::key`): rule, subjects, items and page.
    #[serde(deserialize_with = "lenient")]
    pub key: String,
    /// Why it is accepted, in plain words.
    #[serde(skip_serializing_if = "String::is_empty", deserialize_with = "lenient")]
    pub reason: String,
}

/// The profile as read: every field tolerant, then capped by [`Conformance::sanitize`].
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ConformanceRepr {
    #[serde(deserialize_with = "lenient")]
    enabled: bool,
    #[serde(deserialize_with = "lenient")]
    user_set: bool,
    #[serde(deserialize_with = "lenient_vec")]
    rulesets: Vec<String>,
    #[serde(deserialize_with = "lenient_vec")]
    overrides: Vec<RuleOverride>,
    #[serde(deserialize_with = "lenient_vec")]
    suppressions: Vec<Suppression>,
}

impl From<ConformanceRepr> for Conformance {
    fn from(r: ConformanceRepr) -> Self {
        let mut c =
            Conformance { enabled: r.enabled, user_set: r.user_set, rulesets: r.rulesets, overrides: r.overrides, suppressions: r.suppressions };
        c.sanitize();
        c
    }
}

/// `s` cut to at most `max` bytes at a character boundary.
fn cut(s: &mut String, max: usize) {
    if s.len() > max {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

impl Conformance {
    /// Whether this profile is the default one: off, never set, nothing listed.
    pub fn is_default(&self) -> bool {
        *self == Conformance::default()
    }

    /// Apply the caps: lists cut to their limits, over-long ids dropped (a cut id would name
    /// something else), reasons cut, empty and repeated entries removed.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.rulesets.retain(|r| !r.trim().is_empty() && r.len() <= MAX_ID && seen.insert(r.clone()));
        self.rulesets.truncate(MAX_RULESETS);
        let mut seen = std::collections::HashSet::new();
        self.overrides.retain(|o| !o.rule.is_empty() && o.rule.len() <= MAX_ID && seen.insert(o.rule.clone()));
        self.overrides.truncate(MAX_OVERRIDES);
        let mut seen = std::collections::HashSet::new();
        self.suppressions.retain(|s| !s.key.is_empty() && s.key.len() <= MAX_KEY && seen.insert(s.key.clone()));
        self.suppressions.truncate(MAX_SUPPRESSIONS);
        for s in &mut self.suppressions {
            cut(&mut s.reason, MAX_REASON);
        }
    }

    /// The override for `rule`, if any.
    pub fn override_for(&self, rule: &str) -> Option<&RuleOverride> {
        self.overrides.iter().find(|o| o.rule == rule)
    }

    /// Whether `rule` is switched off by an override.
    pub fn is_disabled(&self, rule: &str) -> bool {
        self.override_for(rule).is_some_and(|o| o.disabled)
    }

    /// The severity `rule` reports with: the override's, else `declared`.
    pub fn severity_of(&self, rule: &str, declared: Severity) -> Severity {
        self.override_for(rule).and_then(|o| o.severity).unwrap_or(declared)
    }

    /// Whether an issue with this key is silenced.
    pub fn is_suppressed(&self, key: &str) -> bool {
        self.suppressions.iter().any(|s| s.key == key)
    }

    /// Silence `key` with `reason`; `false` when it was already silenced, too long, or the list
    /// is full.
    pub fn suppress(&mut self, key: &str, reason: &str) -> bool {
        if key.is_empty() || key.len() > MAX_KEY || self.suppressions.len() >= MAX_SUPPRESSIONS || self.is_suppressed(key) {
            return false;
        }
        let mut reason = reason.to_string();
        cut(&mut reason, MAX_REASON);
        self.suppressions.push(Suppression { key: key.to_string(), reason });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_left_out() {
        let c = Conformance::default();
        assert!(!c.enabled && !c.user_set && c.is_default());
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!({}));
    }

    #[test]
    fn junk_reads_tolerantly_and_is_capped() {
        let v = serde_json::json!({
            "enabled": "yes",
            "userSet": true,
            "rulesets": ["flowchart.iso5807", 3, "", "flowchart.iso5807", "x".repeat(MAX_ID + 1)],
            "overrides": [{"rule": "a.b", "severity": "fatal"}, {"rule": "a.b", "disabled": true}, 7, {"rule": "c.d", "severity": "info"}],
            "suppressions": [{"key": "k", "reason": "r".repeat(5000)}, {"key": ""}, "junk"]
        });
        let c: Conformance = serde_json::from_value(v).unwrap();
        assert!(!c.enabled && c.user_set);
        assert_eq!(c.rulesets, ["flowchart.iso5807"]);
        assert_eq!(c.overrides.len(), 2);
        assert_eq!((c.overrides[0].severity, c.overrides[1].severity), (None, Some(Severity::Info)));
        assert_eq!(c.severity_of("c.d", Severity::Error), Severity::Info);
        assert_eq!(c.severity_of("z.z", Severity::Error), Severity::Error);
        assert_eq!(c.suppressions.len(), 1);
        assert_eq!(c.suppressions[0].reason.len(), MAX_REASON);
        let c: Conformance = serde_json::from_value(serde_json::json!("not an object")).unwrap_or_default();
        assert!(c.is_default());
    }

    #[test]
    fn suppress_once_per_key() {
        let mut c = Conformance::default();
        assert!(c.suppress("r|a||p1", "accepted"));
        assert!(!c.suppress("r|a||p1", "again"));
        assert!(!c.suppress("", ""));
        assert!(c.is_suppressed("r|a||p1") && !c.is_suppressed("r|b||p1"));
        let back: Conformance = serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }
}
