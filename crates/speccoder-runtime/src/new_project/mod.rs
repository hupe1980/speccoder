//! A new project from one line of intent (F-130): the plan — the stack's
//! own scaffold, the spec format's own init, `speccoder init --yes` and
//! one commit as the person — computed read-only, shown, confirmed, and
//! then run exactly as shown. speccoder writes none of the stack's or the
//! format's files itself (constitution VII); `speccoder.toml` is written
//! only by `init`.
//!
//! The plan is bound to a digest over its commands and the folder as it
//! was seen; [`perform`] recomputes it and refuses a plan that changed.
//! A run that stopped resumes: each step's effect is detected, never
//! repeated. Undo ([`undo()`]) moves a folder speccoder created to the
//! Trash, or reverts speccoder's commit in an existing repository — only
//! while nothing else happened there.

mod plan;
mod run;
mod stacks;
mod steps;
pub mod undo;

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::actions::Via;
use speccoder_core::new_project::{
    GitIdentity, NewChoices, NewProjectReport, PlanRefusal, PlanStepResult, ProjectMode,
    ProjectPlan, Stack, UndoOffer,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

pub(crate) use plan::{Kind, Spec, format_kind, plan_with, unfinished};
pub use run::perform;
pub use undo::{undo, undo_offer};

const COMMAND: &str = "new";

/// The creation record: what Undo, the recent projects and the new
/// project's first view read. In speccoder's state, never the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Creation {
    /// The folder, canonical.
    pub path: String,
    /// `new_folder` or `existing_repository`.
    pub mode: ProjectMode,
    /// speccoder's commit, once made.
    pub commit: Option<String>,
    /// The plan it ran.
    pub plan_digest: String,
    /// The project's name.
    pub name: String,
    /// What the person is building, verbatim (never in an event).
    pub idea: String,
    /// The stack.
    pub stack: Stack,
    /// The spec format.
    pub format: String,
    /// The person's git identity.
    pub by: Option<GitIdentity>,
    /// Through what.
    pub via: Via,
    /// Unix seconds.
    pub at: u64,
    /// The New Feature sheet was offered with the idea once.
    #[serde(default)]
    pub first_feature_offered: bool,
    /// When it was undone.
    #[serde(default)]
    pub undone_at: Option<u64>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// SHA-256 of `bytes`, hex.
fn sha(bytes: &[u8]) -> String {
    crate::integrity::sha256_hex(bytes)
}

/// The record's directory for the folder `target` (canonical).
fn record_dir(state: &Path, target: &Path) -> PathBuf {
    let id: String = sha(target.to_string_lossy().as_bytes())
        .chars()
        .take(16)
        .collect();
    state.join("new").join(id)
}

/// SHA-256 of the canonical path: what an event names instead of the path.
#[must_use]
pub fn path_digest(target: &Path) -> String {
    format!("sha256:{}", sha(target.to_string_lossy().as_bytes()))
}

/// The creation record of the project at `target`, if speccoder created it.
#[must_use]
pub fn creation(state: &Path, target: &Path) -> Option<Creation> {
    let t = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let text = std::fs::read_to_string(record_dir(state, &t).join("creation.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_creation(state: &Path, c: &Creation) -> Result<(), String> {
    let dir = record_dir(state, Path::new(&c.path));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    let f = dir.join("creation.json");
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &f).map_err(|e| format!("{}: {e}", f.display()))
}

/// The idea of a project speccoder created, offered once while it has no
/// unit: the New Feature sheet opens prefilled with it (FR-028). Marks it
/// offered.
#[must_use]
pub fn first_feature(state: &Path, root: &Path) -> Option<String> {
    let mut c = creation(state, root)?;
    if c.first_feature_offered || c.commit.is_none() || c.undone_at.is_some() || c.idea.is_empty() {
        return None;
    }
    let has_unit = crate::format::open(root, None)
        .map(|o| !o.snapshot.features.is_empty())
        .unwrap_or(true);
    if has_unit {
        return None;
    }
    c.first_feature_offered = true;
    write_creation(state, &c).ok()?;
    Some(c.idea)
}

/// The folder `path` names, absolute, with `.` and `..` resolved and
/// symbolic links of the part that exists resolved: what the plan names.
fn resolve(path: &Path) -> Result<PathBuf, PlanRefusal> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| {
                refusal(
                    "USAGE",
                    &format!("no current directory: {e}"),
                    "→ give an absolute path",
                )
            })?
            .join(path)
    };
    let mut lexical = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                lexical.pop();
            }
            Component::CurDir => {}
            other => lexical.push(other),
        }
    }
    if std::fs::symlink_metadata(&lexical).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(refusal(
            "NEW-TARGET-NOT-EMPTY",
            &format!("{} is a symbolic link", lexical.display()),
            "→ choose the folder it points to, or another one",
        ));
    }
    // The longest part that exists, canonical; the rest appended.
    let mut existing = lexical.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(n), Some(p)) => {
                rest.push(n.to_os_string());
                existing = p.to_path_buf();
            }
            _ => break,
        }
    }
    let mut out = std::fs::canonicalize(&existing).unwrap_or(existing);
    for n in rest.iter().rev() {
        out.push(n);
    }
    Ok(out)
}

fn refusal(code: &str, message: &str, fix: &str) -> PlanRefusal {
    PlanRefusal {
        code: code.to_owned(),
        message: message.to_owned(),
        fix: fix.to_owned(),
    }
}

/// `~/x` for a path under the home directory.
fn shown(p: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(h) if p.starts_with(&h) => {
            format!(
                "~/{}",
                p.strip_prefix(&h)
                    .map_or_else(|_| p.display().to_string(), |r| r.display().to_string())
            )
        }
        _ => p.display().to_string(),
    }
}

/// SHA-256 over what the confirmation binds: everything the plan runs and
/// the folder as seen — not the informational options beside it.
#[must_use]
pub fn digest(plan: &ProjectPlan) -> String {
    let bound = serde_json::json!({
        "target": plan.target,
        "name": plan.name,
        "mode": plan.mode,
        "stack": plan.stack,
        "format": plan.format,
        "extensions": plan.extensions,
        "preset": plan.preset,
        "steps": plan.steps,
        "sandbox": plan.sandbox,
        "identity": plan.identity,
        "refusal": plan.refusal,
        "observed": plan.observed,
    });
    format!("sha256:{}", sha(bound.to_string().as_bytes()))
}

/// The plan for `path` with `choices` (read-only; the sheet's and
/// `--dry-run`'s).
#[must_use]
pub fn plan(path: &Path, choices: &NewChoices, state: &Path) -> ProjectPlan {
    plan_with(path, choices, state, false).0
}

fn report(plan: ProjectPlan, results: Vec<PlanStepResult>, dry_run: bool) -> NewProjectReport {
    NewProjectReport {
        plan,
        results,
        init: None,
        commit: None,
        undo: UndoOffer {
            available: false,
            kind: "none".to_owned(),
            reason: None,
        },
        dry_run,
        first_feature: None,
    }
}

fn refused_result(plan: ProjectPlan, results: Vec<PlanStepResult>) -> CommandResult {
    let r = plan
        .refusal
        .clone()
        .unwrap_or_else(|| refusal("USAGE", "refused", ""));
    let status =
        speccoder_core::codes::lookup(&r.code).map_or(CommandStatus::UsageError, |e| e.status);
    CommandResult::new(COMMAND, status, &r.message)
        .with_code(&r.code)
        .with_fix(vec![r.fix.clone()])
        .with_data(CommandData::NewProject(Box::new(report(
            plan, results, false,
        ))))
}

/// `speccoder new --dry-run`: the plan and its digest; nothing changes.
#[must_use]
pub fn dry_run(path: &Path, choices: &NewChoices, state: &Path) -> CommandResult {
    let plan = plan(path, choices, state);
    if plan.refusal.is_some() {
        let r = refused_result(plan, Vec::new());
        return r;
    }
    let message = format!(
        "plan for {} in {}: {} commands (nothing ran)",
        plan.name,
        shown(Path::new(&plan.target)),
        plan.steps.len()
    );
    CommandResult::new(COMMAND, CommandStatus::Ok, &message).with_data(CommandData::NewProject(
        Box::new(report(plan, Vec::new(), true)),
    ))
}
