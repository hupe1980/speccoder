//! Spec Kit extension hooks around what speccoder runs.
//!
//! Extensions register hooks in `.specify/extensions.yml` (`hooks.<event>`:
//! `command`, `enabled`, `optional`, `priority`, `condition`, `prompt`).
//! Spec Kit's agents execute them; Spec Kit's engine only describes them.
//! speccoder replaces the agent for `implement` and `converge` (and hosts
//! planning skills), so it owns that step, deterministically:
//! mandatory hooks (`optional: false`) run as hosted skills under the
//! planning policy; optional ones are listed for the person; hooks with a
//! `condition` are skipped (evaluating it is Spec Kit's executor's job).

use std::path::Path;

use serde::Deserialize;

/// The hooks file, relative to the project root.
pub const FILE: &str = ".specify/extensions.yml";

/// One hook entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hook {
    /// The extension that registered it.
    #[serde(default)]
    pub extension: String,
    /// The command, e.g. `speckit.git.commit`.
    #[serde(default)]
    pub command: String,
    /// Absent means enabled.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Absent means optional (Spec Kit's default).
    #[serde(default = "yes")]
    pub optional: bool,
    /// Lower runs first.
    #[serde(default)]
    pub priority: Option<i64>,
    /// An expression for Spec Kit's executor.
    #[serde(default)]
    pub condition: Option<serde_json::Value>,
    /// What an optional hook asks.
    #[serde(default)]
    pub prompt: Option<String>,
    /// What it does.
    #[serde(default)]
    pub description: Option<String>,
}

fn yes() -> bool {
    true
}

impl Hook {
    /// The skill a command installs as (`speckit.git.commit` → `speckit-git-commit`).
    #[must_use]
    pub fn skill(&self) -> String {
        self.command.replace('.', "-")
    }
}

/// What to do for one event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Mandatory hooks to run, in priority order.
    pub run: Vec<Hook>,
    /// Optional hooks, offered to the person.
    pub offer: Vec<Hook>,
    /// Hooks skipped, with why.
    pub skipped: Vec<(Hook, String)>,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    hooks: std::collections::BTreeMap<String, serde_json::Value>,
}

/// The plan for `event` in the project at `root`. A missing file is no
/// hooks; an unreadable one is an error (it would silently skip mandatory
/// hooks otherwise).
pub fn plan(root: &Path, event: &str) -> Result<Plan, String> {
    let path = root.join(FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Plan::default()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    plan_from(&text, event).map_err(|e| format!("{FILE}: {e} — no hooks were checked"))
}

/// [`plan`] from the file's text.
pub fn plan_from(text: &str, event: &str) -> Result<Plan, String> {
    let file: RawFile = if text.trim().is_empty() {
        RawFile {
            hooks: Default::default(),
        }
    } else {
        serde_saphyr::from_str(text).map_err(|e| e.to_string())?
    };
    let entries = match file.hooks.get(event) {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(items)) => items.clone(),
        Some(single @ serde_json::Value::Object(_)) => vec![single.clone()],
        Some(_) => return Err(format!("hooks.{event} must be a list")),
    };
    let mut hooks: Vec<Hook> = entries
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Hook>(v).ok())
        .filter(|h| h.enabled && !h.command.is_empty())
        .collect();
    hooks.sort_by_key(|h| h.priority.unwrap_or(10));
    let mut out = Plan::default();
    for h in hooks {
        let conditional = h
            .condition
            .as_ref()
            .is_some_and(|c| !matches!(c, serde_json::Value::Null) && c.as_str() != Some(""));
        if conditional {
            out.skipped.push((
                h,
                "has a condition (evaluated by Spec Kit's hook executor)".to_owned(),
            ));
        } else if h.optional {
            out.offer.push(h);
        } else {
            out.run.push(h);
        }
    }
    Ok(out)
}

/// The line a person sees for an optional hook.
#[must_use]
pub fn offer_line(event: &str, h: &Hook) -> String {
    format!(
        "optional {event} hook ({}): {} — run it with `speccoder spec {}`",
        h.extension,
        h.prompt
            .clone()
            .or_else(|| h.description.clone())
            .unwrap_or_default(),
        h.skill()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE_TEXT: &str = r#"
installed: [git]
settings:
  auto_execute_hooks: true
hooks:
  before_implement:
    - extension: git
      command: speckit.git.commit
      enabled: true
      optional: true
      priority: 10
      prompt: Commit outstanding changes before implementation?
    - extension: audit
      command: speckit.audit.check
      optional: false
      priority: 5
    - extension: x
      command: speckit.x.conditional
      optional: false
      condition: "config.x.enabled"
    - extension: off
      command: speckit.off.never
      enabled: false
      optional: false
  after_implement: []
"#;

    #[test]
    fn mandatory_hooks_run_optional_ones_are_offered_conditional_ones_skipped() {
        let p = plan_from(FILE_TEXT, "before_implement").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            p.run.iter().map(Hook::skill).collect::<Vec<_>>(),
            ["speckit-audit-check"]
        );
        assert_eq!(
            p.offer.iter().map(Hook::skill).collect::<Vec<_>>(),
            ["speckit-git-commit"]
        );
        assert_eq!(p.skipped.len(), 1);
        assert!(offer_line("before_implement", &p.offer[0]).contains("Commit outstanding changes"));
        assert_eq!(
            plan_from(FILE_TEXT, "after_implement").unwrap_or_default(),
            Plan::default()
        );
        assert_eq!(
            plan_from("", "before_implement").unwrap_or_default(),
            Plan::default()
        );
        assert!(plan_from("hooks: [", "x").is_err());
    }
}
