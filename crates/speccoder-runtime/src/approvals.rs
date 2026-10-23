//! A person's approvals and rejections.
//!
//! When an attempt stops for approval, its pending call is saved with the
//! attempt. `speccoder approve` turns that call's class — the path
//! of a write, the exact argv of a command — into an approval for the task;
//! `speccoder reject` into a denial with a reason the model is told. The
//! next attempt carries the task's decisions in its journaled input, so
//! strict replay needs nothing else. Standing approvals for commands are
//! `[commands] allow` in `speccoder.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use speccoder_core::{
    ApprovalsReport, CommandData, CommandResult, CommandStatus, DecidedCall, PendingCall,
};

use crate::tools::policy::{Grant, Rejection};

/// The decisions file in a project's state directory.
pub const FILE: &str = "approvals.json";

/// One task's decisions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDecisions {
    /// Approved classes.
    #[serde(default)]
    pub approved: Vec<Grant>,
    /// Rejected classes with reasons.
    #[serde(default)]
    pub rejected: Vec<Rejection>,
    /// A person's notes for the task's next attempts (`speccoder nudge`).
    #[serde(default)]
    pub nudges: Vec<String>,
    /// Every decision, in order, with who made it.
    #[serde(default)]
    pub decided: Vec<Decided>,
}

/// Who decided, and through what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    /// The person: the operating-system user, or `unknown`.
    pub name: String,
    /// `terminal`, `editor <name>` or `cli`.
    pub via: String,
}

impl Who {
    /// The local person, deciding through `via`.
    #[must_use]
    pub fn local(via: &str) -> Self {
        Self {
            name: who(),
            via: via.to_owned(),
        }
    }

    /// One line: `hupe (terminal)`.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} ({})", self.name, self.via)
    }
}

/// The local person's name — the operating-system user; `unknown` when
/// there is none, never a guess.
#[must_use]
pub fn who() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// One decision, as it was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decided {
    /// `approve` or `reject`.
    pub verb: String,
    /// The call class.
    pub grant: Grant,
    /// A rejection's reason.
    #[serde(default)]
    pub reason: Option<String>,
    /// An approval for the rest of the task (not just once).
    #[serde(default)]
    pub for_task: bool,
    /// Who decided.
    pub by: Who,
    /// Seconds since the epoch.
    pub at: u64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// All decisions, by `feature/task`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decisions {
    /// By `feature/task`.
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskDecisions>,
}

impl Decisions {
    /// Reads the decisions in `dir`; none when there is no file yet.
    pub fn load(dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(dir.join(FILE)) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{}: {e}", dir.join(FILE).display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", dir.join(FILE).display())),
        }
    }

    /// Writes the decisions to `dir` (temporary file, then rename).
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dir.join(FILE)).map_err(|e| format!("{}: {e}", dir.display()))
    }

    /// The decisions for `feature/task`.
    #[must_use]
    pub fn for_task(&self, feature: &str, task: &str) -> TaskDecisions {
        self.tasks
            .get(&format!("{feature}/{task}"))
            .cloned()
            .unwrap_or_default()
    }

    /// Records an approval; a rejection of the same class is withdrawn.
    pub fn approve(&mut self, feature: &str, task: &str, grant: Grant, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.rejected.retain(|r| r.grant != grant);
        if !d.approved.contains(&grant) {
            d.approved.push(grant.clone());
        }
        d.decided.push(Decided {
            verb: "approve".to_owned(),
            grant,
            reason: None,
            for_task: true,
            by: by.clone(),
            at: now(),
        });
    }

    /// Records a decision that changes nothing for the task (an inline
    /// approval or rejection of one call): the history only.
    pub fn note(&mut self, feature: &str, task: &str, decided: Decided) {
        self.tasks
            .entry(format!("{feature}/{task}"))
            .or_default()
            .decided
            .push(decided);
    }

    /// Records a rejection; an approval of the same class is withdrawn.
    pub fn reject(&mut self, feature: &str, task: &str, grant: Grant, reason: &str, by: &Who) {
        let d = self.tasks.entry(format!("{feature}/{task}")).or_default();
        d.approved.retain(|g| *g != grant);
        d.rejected.retain(|r| r.grant != grant);
        d.rejected.push(Rejection {
            grant: grant.clone(),
            reason: reason.to_owned(),
        });
        d.decided.push(Decided {
            verb: "reject".to_owned(),
            grant,
            reason: Some(reason.to_owned()),
            for_task: true,
            by: by.clone(),
            at: now(),
        });
    }

    /// Whether `grant` was decided for `feature/task` either way.
    #[must_use]
    pub fn decided(&self, feature: &str, task: &str, grant: &Grant) -> bool {
        let d = self.for_task(feature, task);
        d.approved.contains(grant) || d.rejected.iter().any(|r| &r.grant == grant)
    }
}

// ── Inline approvals ────────────────────────────────────────────────

/// A call that needs a person, as shown to them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// `feature/task`.
    pub task: String,
    /// The tool.
    pub tool: String,
    /// Its arguments as the model sent them.
    pub arguments: serde_json::Value,
    /// Why it needs approval.
    pub reason: String,
    /// What a "for this task" answer would cover.
    pub covers: String,
    /// `high` (runs arbitrary code or uses the network), `medium` (changes
    /// dependencies, frozen tests or repository state) or `low`.
    pub risk: String,
}

/// A person's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Run the call.
    pub approved: bool,
    /// Also approve its class for the rest of the task (recorded like `approve`).
    pub for_task: bool,
    /// For a rejection: why — the model is told.
    pub reason: Option<String>,
    /// Who answered; journaled with the answer.
    #[serde(default)]
    pub by: Option<Who>,
}

/// Asks a person, synchronously, while an attempt waits (a terminal prompt
/// in the CLI). The answer is journaled; replay never asks again.
pub trait Approver: Send + Sync + std::fmt::Debug {
    /// The person's answer to `request`.
    fn ask(&self, request: &Request) -> Answer;

    /// The person's free-text answer to a question a hosted skill asks
    ///; `None` when nobody can answer.
    fn answer(&self, question: &str) -> Option<String> {
        let _ = question;
        None
    }
}

/// The risk tier of a call that needs approval, from the policy's reason.
#[must_use]
pub fn risk(tool: &str, reason: &str) -> &'static str {
    let r = reason.to_lowercase();
    if tool == "run_command" && (r.contains("arbitrary") || r.contains("network")) {
        "high"
    } else if r.contains("manifest")
        || r.contains("frozen")
        || r.contains("repository state")
        || r.contains("recursive")
    {
        "medium"
    } else {
        "low"
    }
}

// ── `speccoder approvals` / `approve` / `reject` ────────────────────────────

/// The feature, its repository root and its state directory.
fn locate(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    state: &Path,
) -> Result<(String, std::path::PathBuf, std::path::PathBuf), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::new(
            command,
            inspected.status(),
            inspected.message(),
        )));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = std::path::PathBuf::from(&snapshot.root);
    Ok((
        f.feature.name.clone(),
        crate::worktree::project_state_dir(state, &root),
        root,
    ))
}

/// Open tasks' latest attempts that stopped for approval and are undecided.
fn pending(
    root: &Path,
    feature: &str,
    open: &[String],
    decisions: &Decisions,
) -> Vec<(PendingCall, Grant)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for saved in crate::attempts::list(root, feature) {
        if !open.contains(&saved.task) || !seen.insert(saved.task.clone()) {
            continue; // newest first: only each open task's latest attempt
        }
        let Some(p) = saved.pending.filter(|_| saved.outcome == "needs_approval") else {
            continue;
        };
        let tool = p["tool"].as_str().unwrap_or_default().to_owned();
        let Some(grant) = Grant::of(&tool, &p["arguments"]) else {
            continue;
        };
        if decisions.decided(feature, &saved.task, &grant) {
            continue;
        }
        out.push((
            PendingCall {
                task: saved.task.clone(),
                covers: grant.describe(),
                tool,
                arguments: p["arguments"].clone(),
                reason: p["reason"].as_str().unwrap_or_default().to_owned(),
                reference: saved.reference.clone(),
            },
            grant,
        ));
    }
    out
}

fn decided(feature: &str, decisions: &Decisions) -> Vec<DecidedCall> {
    let prefix = format!("{feature}/");
    let mut out = Vec::new();
    for (key, d) in &decisions.tasks {
        let Some(task) = key.strip_prefix(&prefix) else {
            continue;
        };
        let by = |g: &Grant| {
            d.decided
                .iter()
                .rev()
                .find(|x| x.grant == *g)
                .map(|x| x.by.describe())
        };
        out.extend(d.approved.iter().map(|g| DecidedCall {
            task: task.to_owned(),
            decision: "approved".to_owned(),
            covers: g.describe(),
            reason: None,
            by: by(g),
        }));
        out.extend(d.rejected.iter().map(|r| DecidedCall {
            task: task.to_owned(),
            decision: "rejected".to_owned(),
            covers: r.grant.describe(),
            reason: Some(r.reason.clone()),
            by: by(&r.grant),
        }));
    }
    out
}

/// What to do with a pending call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb<'a> {
    /// Only list.
    List,
    /// Approve the task's pending call.
    Approve(&'a str),
    /// Reject the task's pending call, with a reason.
    Reject(&'a str, &'a str),
}

/// `approvals`, `approve --task` and `reject --task --reason` against `state`.
pub fn decide_with(
    path: &Path,
    feature: Option<&str>,
    verb: Verb<'_>,
    state: &Path,
) -> CommandResult {
    let command = match verb {
        Verb::List => "approvals",
        Verb::Approve(_) => "approve",
        Verb::Reject(..) => "reject",
    };
    let (feature, dir, root) = match locate(command, path, feature, state) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let mut decisions = match Decisions::load(&dir) {
        Ok(d) => d,
        Err(e) => return CommandResult::new(command, CommandStatus::ConfigError, &e),
    };
    // Open tasks, from the speccoder branch when there is one.
    let status = crate::status::report(&root, Some(&feature));
    let open: Vec<String> = match status.data() {
        Some(CommandData::Report(r)) => r
            .tasks
            .iter()
            .filter(|t| matches!(t.state.as_str(), "ready" | "blocked"))
            .map(|t| t.id.clone())
            .collect(),
        _ => Vec::new(),
    };
    let waiting = pending(&root, &feature, &open, &decisions);

    let message = match verb {
        Verb::List => {
            if waiting.is_empty() {
                "nothing awaits a decision".to_owned()
            } else {
                format!("{} call(s) await a decision", waiting.len())
            }
        }
        Verb::Approve(task) | Verb::Reject(task, _) => {
            let Some((call, grant)) = waiting.iter().find(|(c, _)| c.task == task) else {
                return CommandResult::new(
                    command,
                    CommandStatus::NotFound,
                    &format!("{task} has no call awaiting a decision (see `speccoder approvals`)"),
                );
            };
            let what = match verb {
                Verb::Reject(_, reason) => {
                    decisions.reject(&feature, task, grant.clone(), reason, &Who::local("cli"));
                    format!(
                        "rejected: {} for {task} — the model will be told: {reason}",
                        call.covers
                    )
                }
                _ => {
                    decisions.approve(&feature, task, grant.clone(), &Who::local("cli"));
                    format!("approved: {} for {task}", call.covers)
                }
            };
            if let Err(e) = decisions.save(&dir) {
                return CommandResult::new(command, CommandStatus::Error, &e);
            }
            format!("{what}; run the task again to continue (speccoder run --task {task})")
        }
    };
    let report = ApprovalsReport {
        decided: decided(&feature, &decisions),
        pending: pending(&root, &feature, &open, &decisions)
            .into_iter()
            .map(|(c, _)| c)
            .collect(),
        feature,
    };
    CommandResult::new(command, CommandStatus::Ok, &message)
        .with_data(CommandData::Approvals(Box::new(report)))
}

/// [`decide_with`] in the default state directory.
#[must_use]
pub fn decide(path: &Path, feature: Option<&str>, verb: Verb<'_>) -> CommandResult {
    match crate::worktree::state_dir() {
        Some(state) => decide_with(path, feature, verb, &state),
        None => CommandResult::new(
            "approvals",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_round_trip_and_replace_each_other() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let g = Grant::Write {
            path: "Cargo.toml".to_owned(),
        };
        let mut d = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let by = Who::local("cli");
        d.reject("f", "T1", g.clone(), "no new dependencies", &by);
        d.approve("f", "T1", g.clone(), &by);
        assert_eq!(d.tasks["f/T1"].decided.len(), 2);
        assert_eq!(d.tasks["f/T1"].decided[1].by, by);
        d.save(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let back = Decisions::load(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back.for_task("f", "T1").approved, std::slice::from_ref(&g));
        assert!(back.for_task("f", "T1").rejected.is_empty());
        assert!(back.decided("f", "T1", &g) && !back.decided("f", "T2", &g));
    }
}
