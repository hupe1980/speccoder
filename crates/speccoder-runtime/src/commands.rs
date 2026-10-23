//! Read-side and single-shot services: `inspect`, `worktree`, `propose`,
//! `replay` and `eval`.

use std::path::Path;

use speccoder_core::snapshot::{ProjectSnapshot, Severity};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{context, git, substrate};

/// `speccoder inspect`: report what speccoder sees in the Spec Kit project
/// containing `path`. Read-only.
#[must_use]
pub fn inspect(path: &Path, feature: Option<&str>) -> CommandResult {
    let env = std::env::var("SPECIFY_FEATURE_DIRECTORY")
        .ok()
        .filter(|v| !v.is_empty());
    match speccoder_speckit::inspect_with(path, feature, env.as_deref()) {
        Ok(snapshot) => {
            let status = status_of(&snapshot);
            let message = summary(&snapshot);
            CommandResult::new("inspect", status, &message)
                .with_data(CommandData::Inspect(Box::new(snapshot)))
        }
        Err(speccoder_speckit::NotAProject) => CommandResult::new(
            "inspect",
            CommandStatus::NotAProject,
            &format!(
                "no Spec Kit project (.specify/) at or above {}; run `specify init` first",
                path.display()
            ),
        ),
    }
}

/// Exit precedence: selection failure > error diagnostics > ok.
pub(crate) fn status_of(s: &ProjectSnapshot) -> CommandStatus {
    let selection_failed = s.feature.is_none()
        && s.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code.starts_with("select."));
    if selection_failed {
        CommandStatus::SelectionNeeded
    } else if s.has_errors() {
        CommandStatus::DiagnosticsError
    } else {
        CommandStatus::Ok
    }
}

pub(crate) fn summary(s: &ProjectSnapshot) -> String {
    let features = match s.features.len() {
        1 => "1 feature".to_owned(),
        n => format!("{n} features"),
    };
    let selected = match &s.feature {
        Some(f) => match &f.tasks {
            Some(t) => format!(
                "{} selected: {} tasks, {} done, {} open",
                f.feature.name, t.totals.total, t.totals.done, t.totals.open
            ),
            None => format!("{} selected: no tasks.md", f.feature.name),
        },
        None => "none selected".to_owned(),
    };
    let errors = s
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = s
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    format!("{features}, {selected}; {errors} errors, {warnings} warnings")
}

/// `speccoder worktree`: create or reuse the git worktree speccoder works in
/// for the selected feature of the Spec Kit project containing `path`.
#[must_use]
pub fn worktree(path: &Path, feature: Option<&str>) -> CommandResult {
    match crate::worktree::state_dir() {
        Some(state) => worktree_with(path, feature, &state),
        None => CommandResult::new(
            "worktree",
            CommandStatus::GitPrecondition,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        ),
    }
}

/// [`worktree()`] with an explicit state directory.
pub fn worktree_with(path: &Path, feature: Option<&str>, state: &Path) -> CommandResult {
    use speccoder_core::{RefusedPath, WorktreeReport};

    let inspected = inspect(path, feature);
    let snapshot = match inspected.data() {
        Some(CommandData::Inspect(s)) => s.clone(),
        _ => return CommandResult::new("worktree", inspected.status(), inspected.message()),
    };
    let Some(f) = &snapshot.feature else {
        return CommandResult::new(
            "worktree",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        );
    };
    let name = f.feature.name.clone();
    let mut report = WorktreeReport {
        feature: name.clone(),
        path: None,
        project_dir: None,
        branch: None,
        base: None,
        created: false,
        refused_paths: Vec::new(),
        hint: None,
    };
    let refuse = |mut report: WorktreeReport, message: &str, hint: &str| {
        report.hint = Some(hint.to_owned());
        CommandResult::new("worktree", CommandStatus::GitPrecondition, message)
            .with_data(CommandData::Worktree(report))
    };

    let project = Path::new(&snapshot.root);
    let repo_root = match git::Git::at(project).toplevel() {
        Ok(Some(root)) => std::fs::canonicalize(&root).unwrap_or(root),
        Ok(None) => {
            return refuse(
                report,
                "the Spec Kit project is not inside a git repository",
                "run `git init` and commit the project",
            );
        }
        Err(e) => return refuse(report, &e.0, "install git"),
    };
    // The Spec Kit root may be a subdirectory of the repository (monorepo).
    let rel = project.strip_prefix(&repo_root).unwrap_or(Path::new(""));
    let join = |p: &str| {
        let joined = rel.join(p);
        let s = joined.to_string_lossy().replace('\\', "/");
        if s.is_empty() { ".".to_owned() } else { s }
    };
    let artifacts = crate::worktree::Artifacts {
        specify_dir: join(".specify"),
        feature_dir: join(&f.feature.dir),
    };

    match crate::worktree::ensure_worktree(&repo_root, state, &name, &artifacts) {
        Ok(wt) => {
            report.project_dir = Some(wt.path.join(rel).display().to_string());
            report.path = Some(wt.path.display().to_string());
            report.branch = Some(wt.branch);
            report.base = Some(wt.base);
            report.created = wt.created;
            let message = if report.created {
                "worktree created"
            } else {
                "worktree reused"
            };
            CommandResult::new("worktree", CommandStatus::Ok, message)
                .with_data(CommandData::Worktree(report))
        }
        Err(r) => {
            report.refused_paths = r
                .paths
                .into_iter()
                .map(|(path, reason)| RefusedPath { path, reason })
                .collect();
            refuse(report, &r.message, &r.hint)
        }
    }
}

/// `speccoder propose`: the provider comes from `[model]`, its key from
/// `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` (never from files).
#[must_use]
pub fn propose(path: &Path, feature: Option<&str>, task: &str) -> CommandResult {
    let Some(state) = crate::worktree::state_dir() else {
        return CommandResult::new(
            "propose",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    // Validate everything that needs no key first: a wrong task or a missing
    // [model] is reported before a missing credential.
    match setup(path, feature, &state, "propose") {
        Err(r) => return *r,
        Ok(s) => {
            if let Err(e) = context::bundle(&s.snapshot, task) {
                let msg = match e {
                    context::BundleError::NotFound(id) => {
                        format!("no task {id} in the selected feature")
                    }
                    context::BundleError::NoTasks => {
                        "the selected feature has no tasks.md".to_owned()
                    }
                    context::BundleError::NoFeature => {
                        "no feature selected; pass --feature".to_owned()
                    }
                };
                return CommandResult::new("propose", CommandStatus::NotFound, &msg);
            }
        }
    }
    let model = match setup(path, feature, &state, "propose") {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return CommandResult::new("propose", CommandStatus::ConfigError, &e),
    };
    let provider = match driver {
        Ok(p) => p,
        Err(e) => {
            return CommandResult::new(
                "propose",
                CommandStatus::ConfigError,
                &redact(&e, Some(&key)),
            );
        }
    };
    propose_with(path, feature, task, &state, provider, Some(&key))
}

/// [`propose`] with an explicit state directory and model driver; `secret`
/// is redacted from anything reported.
pub fn propose_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
) -> CommandResult {
    use speccoder_core::ProposeReport;

    let s = match setup(path, feature, state, "propose") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let bundle = match context::bundle(&s.snapshot, task) {
        Ok(b) => b,
        Err(context::BundleError::NotFound(id)) => {
            return CommandResult::new(
                "propose",
                CommandStatus::NotFound,
                &format!("no task {id} in the selected feature"),
            );
        }
        Err(context::BundleError::NoTasks) => {
            return CommandResult::new(
                "propose",
                CommandStatus::NotFound,
                "the selected feature has no tasks.md",
            );
        }
        Err(context::BundleError::NoFeature) => {
            return CommandResult::new(
                "propose",
                CommandStatus::SelectionNeeded,
                "no feature selected; pass --feature",
            );
        }
    };
    let feature_name = bundle.feature.clone();
    let input = match serde_json::to_value(&bundle) {
        Ok(v) => v,
        Err(e) => return CommandResult::new("propose", CommandStatus::Error, &e.to_string()),
    };
    let fail =
        |m: String| CommandResult::new("propose", CommandStatus::RunFailed, &redact(&m, secret));

    let rt = match tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return fail(e),
    };
    let outcome = rt.block_on(async {
        let plane = substrate::Plane::open(&s.plane_dir, &s.model, provider)?;
        // The person's committed spec, plan and task: trusted.
        plane
            .propose(
                &feature_name,
                task,
                agentplane::core::Tainted::trusted(input),
            )
            .await
    });
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return fail(e.0),
    };
    let output = outcome.output.as_ref().map(|o| o.peek().clone());
    let report = ProposeReport {
        run_id: outcome.run_id.to_string(),
        feature: feature_name,
        task: task.to_owned(),
        run_status: outcome.status.as_str().to_owned(),
        proposal: output
            .as_ref()
            .and_then(|o| o.get("proposal").cloned())
            .filter(|p| !p.is_null()),
        usage: output.as_ref().and_then(|o| o.get("usage").cloned()),
    };
    let (status, message) = match (&outcome.status, &report.proposal) {
        (agentplane::prelude::RunStatus::Succeeded, Some(_)) => (
            CommandStatus::Ok,
            format!("proposal recorded in run {}", report.run_id),
        ),
        (agentplane::prelude::RunStatus::Succeeded, None) => (
            CommandStatus::RunFailed,
            format!(
                "run {} succeeded but the model's answer is not a proposal",
                report.run_id
            ),
        ),
        (other, _) => (
            CommandStatus::RunFailed,
            redact(
                &format!("run {} ended {}: {other:?}", report.run_id, other.as_str()),
                secret,
            ),
        ),
    };
    CommandResult::new("propose", status, &message)
        .with_data(CommandData::Propose(Box::new(report)))
}

/// `speccoder replay`: strict replay of a recorded run; calls no model.
#[must_use]
pub fn replay(path: &Path, run: &str) -> CommandResult {
    let Some(state) = crate::worktree::state_dir() else {
        return CommandResult::new(
            "replay",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    replay_with(path, run, &state)
}

/// [`replay`] with an explicit state directory.
pub fn replay_with(path: &Path, run: &str, state: &Path) -> CommandResult {
    use speccoder_core::ReplayReport;

    let s = match setup(path, None, state, "replay") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let rt = match tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return CommandResult::new("replay", CommandStatus::RunFailed, &e),
    };
    let result = rt.block_on(substrate::replay_strict(&s.plane_dir, &s.model, run));
    let report = |result: &str, detail: String| ReplayReport {
        run_id: run.to_owned(),
        result: result.to_owned(),
        detail,
    };
    match result {
        Ok(Some(v)) => {
            let (result, status, message) = replay_verdict(&v.finding);
            CommandResult::new("replay", status, &message)
                .with_data(CommandData::Replay(report(result, message.clone())))
        }
        Ok(None) => CommandResult::new(
            "replay",
            CommandStatus::NotFound,
            &format!("no run {run} in this project's journal"),
        )
        .with_data(CommandData::Replay(report("not_found", String::new()))),
        // agentplane names every divergence in the verdict's finding; an
        // error is a missing run, a journal that does not verify, or the store.
        Err(e) => CommandResult::new("replay", CommandStatus::RunFailed, &e.0)
            .with_data(CommandData::Replay(report("error", e.0.clone()))),
    }
}

/// `speccoder eval`: the suite's `[model]` decides the provider; its key
/// comes from the environment.
#[must_use]
pub fn eval(
    dir: &Path,
    only: Option<&str>,
    keep: Option<&Path>,
    repeat: usize,
    executor: Option<&str>,
) -> CommandResult {
    let suite = match crate::eval::suite_config(dir) {
        Ok(s) => s,
        Err(e) => return CommandResult::new("eval", CommandStatus::ConfigError, &e),
    };
    let model = match substrate::model_config::parse(&suite) {
        Ok(Some(m)) => m,
        Ok(None) => {
            return CommandResult::new(
                "eval",
                CommandStatus::ConfigError,
                "eval.toml has no [model]",
            );
        }
        Err(e) => {
            return CommandResult::new(
                "eval",
                CommandStatus::ConfigError,
                &format!("eval.toml: {e}"),
            );
        }
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return CommandResult::new("eval", CommandStatus::ConfigError, &e),
    };
    match driver {
        Ok(p) => crate::eval::run_suite_as(dir, only, &p, Some(&key), keep, repeat, executor),
        Err(e) => CommandResult::new("eval", CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// What a replay's finding means for the person: a faithful
/// replay of any ending is reproduced; a different effect or ending is a
/// divergence; a run this build cannot re-derive is neither.
pub(crate) fn replay_verdict(
    finding: &agentplane::runtime::Finding,
) -> (&'static str, CommandStatus, String) {
    use agentplane::runtime::Finding;
    match finding {
        Finding::Verified { outcome } => (
            "reproduced",
            CommandStatus::Ok,
            format!(
                "reproduced: every recorded effect again, and the recorded ending ({outcome}); no model was called"
            ),
        ),
        Finding::Diverged(d) => (
            "diverged",
            CommandStatus::ReplayDiverged,
            format!("diverged: {d:?}"),
        ),
        Finding::OutcomeDiffers { recorded, replayed } => (
            "diverged",
            CommandStatus::ReplayDiverged,
            format!(
                "diverged: every effect matched, but the run ended {replayed} instead of {recorded}"
            ),
        ),
        Finding::CannotReplay(why) => (
            "not_replayable",
            CommandStatus::NotReplayable,
            format!("this build cannot re-derive the run: {why:?}"),
        ),
        other => (
            "not_replayable",
            CommandStatus::NotReplayable,
            format!("{other:?}"),
        ),
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    use agentplane::runtime::{CannotReplay, Finding};

    #[test]
    fn findings_become_reproduced_diverged_or_not_replayable() {
        let (r, s, _) = replay_verdict(&Finding::Verified {
            outcome: "failed".to_owned(),
        });
        assert_eq!((r, s), ("reproduced", CommandStatus::Ok));
        let (r, s, m) = replay_verdict(&Finding::OutcomeDiffers {
            recorded: "succeeded".to_owned(),
            replayed: "failed".to_owned(),
        });
        assert_eq!((r, s), ("diverged", CommandStatus::ReplayDiverged));
        assert!(m.contains("instead of succeeded"));
        let (r, s, _) = replay_verdict(&Finding::CannotReplay(CannotReplay::NotConcluded));
        assert_eq!((r, s), ("not_replayable", CommandStatus::NotReplayable));
        assert_eq!(s.exit_code().code(), 25);
    }
}
