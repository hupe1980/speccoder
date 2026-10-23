//! Application services of speccoder.
//!
//! The only crate that executes anything: every side effect (file writes,
//! processes, model calls) goes through here, and model calls and tool calls
//! through the embedded `agentplane` runtime (see [`substrate`]).
//! The CLI only parses arguments, calls a service and renders its result.

pub mod approvals;
pub mod attempts;
pub mod bench;
pub mod context;
pub mod controls;
pub mod convergence;
pub mod eval;
pub mod external;
pub mod git;
pub mod handoff;
pub mod hooks;
pub mod hosting;
pub mod init;
pub mod integrity;
pub mod mcp;
pub mod mutation;
pub mod progress;
pub mod resume;
pub mod sandbox;
pub mod skills;
pub mod status;
pub mod substrate;
pub mod tools;
pub mod verify;
pub mod workflow;
pub mod worktree;

use std::path::Path;
use std::sync::Arc;

use speccoder_core::snapshot::{ProjectSnapshot, Severity};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

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
fn status_of(s: &ProjectSnapshot) -> CommandStatus {
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

fn summary(s: &ProjectSnapshot) -> String {
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
    match worktree::state_dir() {
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
    let artifacts = worktree::Artifacts {
        specify_dir: join(".specify"),
        feature_dir: join(&f.feature.dir),
    };

    match worktree::ensure_worktree(&repo_root, state, &name, &artifacts) {
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

// ── propose / replay ───────────────────────────────────────────────

/// Where a project's run state lives, and its model configuration.
struct Setup {
    snapshot: Box<speccoder_core::snapshot::ProjectSnapshot>,
    model: substrate::model_config::ModelConfig,
    plane_dir: std::path::PathBuf,
}

fn setup(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    command: &str,
) -> Result<Setup, Box<CommandResult>> {
    let inspected = inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::new(
            command,
            inspected.status(),
            inspected.message(),
        )));
    };
    let snapshot = snapshot.clone();
    let config_error =
        |m: String| Box::new(CommandResult::new(command, CommandStatus::ConfigError, &m));
    let root = Path::new(&snapshot.root);
    let config_dir = verify::find_config(root).ok_or_else(|| {
        config_error(format!(
            "no speccoder.toml at or above {}; add [model] provider = \"anthropic\", model = \"…\"",
            root.display()
        ))
    })?;
    let text = std::fs::read_to_string(config_dir.join(verify::config::FILE))
        .map_err(|e| config_error(format!("speccoder.toml: {e}")))?;
    let model = substrate::model_config::parse(&text)
        .map_err(|e| config_error(format!("speccoder.toml: {e}")))?
        .ok_or_else(|| config_error("speccoder.toml has no [model] section".to_owned()))?;
    Ok(Setup {
        plane_dir: worktree::project_state_dir(state, root),
        snapshot,
        model,
    })
}

fn tokio_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the async runtime: {e}"))
}

/// Replaces every occurrence of `secret` (never shown, never journaled by speccoder).
fn redact(text: &str, secret: Option<&str>) -> String {
    match secret {
        Some(s) if !s.is_empty() => text.replace(s, "[redacted]"),
        _ => text.to_owned(),
    }
}

/// `speccoder propose`: the provider comes from `[model]`, its key from
/// `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` (never from files).
#[must_use]
pub fn propose(path: &Path, feature: Option<&str>, task: &str) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
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
        run_status: substrate::status_word(&outcome.status),
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
    let Some(state) = worktree::state_dir() else {
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
        Err(e) => {
            let diverged = e.0.to_lowercase().contains("diverg");
            let status = if diverged {
                CommandStatus::ReplayDiverged
            } else {
                CommandStatus::RunFailed
            };
            CommandResult::new("replay", status, &e.0).with_data(CommandData::Replay(report(
                if diverged { "diverged" } else { "error" },
                e.0.clone(),
            )))
        }
    }
}

// ── run ────────────────────────────────────────────────────────────

/// `speccoder run`: one attempt at a ready task in its worktree, judged by
/// the verifier. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn run(path: &Path, feature: Option<&str>, task: &str, options: &RunOptions) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
        return CommandResult::new(
            "run",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    let model = match setup(path, feature, &state, "run") {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return CommandResult::new("run", CommandStatus::ConfigError, &e),
    };
    match driver {
        Ok(provider) => run_task(path, feature, task, &state, provider, Some(&key), options),
        Err(e) => CommandResult::new("run", CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// [`run`] with an explicit state directory and model driver.
pub fn run_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
) -> CommandResult {
    run_task(
        path,
        feature,
        task,
        state,
        provider,
        secret,
        &RunOptions::default(),
    )
}

/// [`run_with`] asking `approver` inline when a call needs approval.
#[must_use]
pub fn run_with_approver(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    approver: Arc<dyn approvals::Approver>,
) -> CommandResult {
    let options = RunOptions {
        approver: Some(approver),
        ..RunOptions::default()
    };
    run_task(path, feature, task, state, provider, secret, &options)
}

#[allow(clippy::too_many_lines)] // one linear procedure, kept in reading order
fn run_task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    use speccoder_core::RunReport;
    let approver = options.approver.clone();
    use speccoder_core::snapshot::TaskState;
    use speccoder_core::verify::{compare, passes, verdict};

    let refuse =
        |status: CommandStatus, m: String| CommandResult::new("run", status, &redact(&m, secret));

    // 1. The feature, as selected in the user's checkout.
    let s = match setup(path, feature, state, "run") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let Some(f) = s.snapshot.feature.as_ref() else {
        return refuse(
            CommandStatus::SelectionNeeded,
            "no feature selected; pass --feature".to_owned(),
        );
    };
    let feature_name = f.feature.name.clone();
    // A lane keeps its own journal and live state; a person's
    // decisions are the feature's.
    let plane_dir = match &options.lane {
        Some(lane) => s.plane_dir.join("lanes").join(lane),
        None => s.plane_dir.clone(),
    };
    // Live progress: live.json for `speccoder status`, and the printer.
    let live: Arc<dyn progress::Progress> = Arc::new(progress::Live::new(
        &plane_dir,
        s.model.prices,
        options.progress.clone(),
    ));

    // 2. The worktree.
    let wt = worktree_with(path, Some(&feature_name), state);
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::new("run", wt.status(), wt.message());
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::new("run", wt.status(), wt.message())
            .with_data(CommandData::Worktree(w.clone()));
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path".to_owned());
    };
    let (wt_path, project_dir) = match &options.lane {
        None => (wt_path, project_dir),
        Some(lane) => {
            // The lane starts where the feature branch is now.
            let start = match git::Git::at(Path::new(&wt_path)).line(&["rev-parse", "HEAD"]) {
                Ok(c) => c,
                Err(e) => return refuse(CommandStatus::GitPrecondition, e.0),
            };
            let repo_root = match git::Git::at(path).toplevel() {
                Ok(Some(r)) => r,
                _ => {
                    return refuse(
                        CommandStatus::GitPrecondition,
                        "not a git repository".to_owned(),
                    );
                }
            };
            let guard = LANE_SETUP.lock();
            let made = worktree::ensure_lane(&repo_root, state, &feature_name, lane, &start);
            drop(guard);
            match made {
                Ok(l) => {
                    let rel = Path::new(&project_dir)
                        .strip_prefix(&wt_path)
                        .map(Path::to_path_buf)
                        .unwrap_or_default();
                    let lane_path = l.path.display().to_string();
                    let lane_project = l.path.join(rel).display().to_string();
                    (lane_path, lane_project)
                }
                Err(e) => return refuse(CommandStatus::GitPrecondition, e.message),
            }
        }
    };
    let project = std::path::PathBuf::from(&project_dir);
    let wt_root = std::path::PathBuf::from(&wt_path);
    let tmp = plane_dir.join("tmp");

    // 2b. Leftovers of an interrupted run (or edits by hand) are saved under
    //     an attempt ref and set aside: a run starts from the branch tip
    //    . Without per-task commits, uncommitted work is expected.
    let commits_per_task = verify::find_config(&project)
        .and_then(|root| std::fs::read_to_string(root.join(verify::config::FILE)).ok())
        .and_then(|t| substrate::run_config::parse(&t).ok())
        .is_none_or(|c| c.commit_per_task);
    // an attempt a killed process left behind is resumed, not set
    // aside — the worktree holds what its finished tool calls wrote.
    let marker_path = resume::path(&plane_dir, &feature_name, task);
    let head_now = git::Git::at(&wt_root)
        .line(&["rev-parse", "HEAD"])
        .unwrap_or_default();
    let interrupted = resume::load(&marker_path)
        .filter(|m| m.head == head_now && options.lane.is_none() && !head_now.is_empty());
    let _marker = resume::Guard(marker_path.clone());
    let mut set_aside = None;
    if interrupted.is_none() && commits_per_task && worktree::require_clean(&wt_root).is_err() {
        let saved = attempts::save(
            &wt_root,
            &tmp,
            &feature_name,
            task,
            &attempts::Record {
                run: None,
                outcome: "interrupted",
                reason: "the worktree had uncommitted changes when a run started (an interrupted run, or edits by hand); saved and set aside",
                pending: None,
                usage: None,
            },
        )
        .and_then(|saved| attempts::restore_head(&wt_root, &tmp).map(|()| saved));
        match saved {
            Ok(saved) => set_aside = Some(saved.reference),
            Err(e) => return refuse(CommandStatus::GitPrecondition, e.0),
        }
    }

    // 3. Spec, checks and policy as committed in the worktree.
    let snap = match speccoder_speckit::inspect(&project, Some(&feature_name)) {
        Ok(snap) => snap,
        Err(_) => {
            return refuse(
                CommandStatus::NotAProject,
                "the worktree has no Spec Kit project".to_owned(),
            );
        }
    };
    // The task must exist and be ready in the worktree, where progress
    // (ticks committed on speccoder/<feature>) lives.
    let Some(st) = snap
        .feature
        .as_ref()
        .and_then(|f| f.schedule.as_ref())
        .and_then(|sch| sch.tasks.iter().find(|t| t.id == task))
    else {
        return refuse(
            CommandStatus::NotFound,
            format!("no task {task} in {feature_name}"),
        );
    };
    match st.state {
        TaskState::Done => {
            return refuse(
                CommandStatus::TaskNotReady,
                format!("{task} is already done"),
            );
        }
        TaskState::Blocked => {
            return refuse(
                CommandStatus::TaskNotReady,
                format!("{task} is blocked by {}", st.blocked_by.join(", ")),
            );
        }
        TaskState::Ready => {}
    }
    let parsed_task = snap
        .feature
        .as_ref()
        .and_then(|f| f.tasks.as_ref())
        .and_then(|t| t.tasks.iter().find(|t| t.id == task).cloned());
    let tasks_file = snap
        .feature
        .as_ref()
        .and_then(|f| f.tasks.as_ref())
        .map(|t| project.join(&t.path));

    let bundle = match context::bundle(&snap, task) {
        Ok(b) => b,
        Err(_) => {
            return refuse(
                CommandStatus::NotFound,
                format!("no task {task} in the worktree's {feature_name}"),
            );
        }
    };
    let Some(config_root) = verify::find_config(&project) else {
        return refuse(
            CommandStatus::ConfigError,
            "the worktree has no speccoder.toml (is it committed?)".to_owned(),
        );
    };
    let config_text =
        std::fs::read_to_string(config_root.join(verify::config::FILE)).unwrap_or_default();
    let run_config = match substrate::run_config::parse(&config_text) {
        Ok(c) => c,
        Err(e) => return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
    };
    let executor = match external::parse(&config_text) {
        Ok(e) => e,
        Err(e) => return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
    };
    // Every command this task starts runs confined to the worktree.
    let sandbox_config = match sandbox::parse(&config_text) {
        Ok(c) => c,
        Err(e) => return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
    };
    let (runner, sandbox_notice) = match sandbox::runner(&wt_root, &sandbox_config) {
        Ok(r) => r,
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };
    let mutation_config = match mutation::parse(&config_text) {
        Ok(c) => c,
        Err(e) => return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
    };
    let check_argvs = verify::check_argvs(&config_root);
    let policy = tools::policy::Policy::new(&check_argvs, &run_config.allow);

    let wt_root_canonical = std::fs::canonicalize(&wt_root)
        .unwrap_or_else(|_| wt_root.clone())
        .display()
        .to_string();

    // A person's approvals and rejections for this task.
    let decisions = match approvals::Decisions::load(&s.plane_dir) {
        Ok(d) => d.for_task(&feature_name, task),
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };

    live.event(&progress::Event::TaskStarted {
        feature: feature_name.clone(),
        task: task.to_owned(),
        title: parsed_task
            .as_ref()
            .map(|t| t.title.clone())
            .unwrap_or_default(),
    });
    // From here on, an early end is recorded as the task run's end.
    let fail = |status: CommandStatus, m: String| {
        live.event(&progress::Event::TaskFinished {
            task: task.to_owned(),
            outcome: "error".to_owned(),
            message: redact(&m, secret),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            usd: None,
        });
        refuse(status, m)
    };
    live.event(&progress::Event::Phase {
        task: task.to_owned(),
        phase: "baseline checks".to_owned(),
    });
    // 4. Baseline: no verdict without a judge.
    let baseline = match verify::run_checks(&project, Some(verify::config::Tier::Fast), &runner) {
        Ok(b) => b,
        Err(e) => return fail(CommandStatus::ConfigError, format!("baseline: {e}")),
    };

    let rt = match tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return fail(CommandStatus::RunFailed, e),
    };
    let bundle_value = match serde_json::to_value(&bundle) {
        Ok(v) => v,
        Err(e) => return fail(CommandStatus::Error, e.to_string()),
    };

    // MCP servers, started in the worktree for this task's attempts.
    let servers = match mcp::parse(&config_text) {
        Ok(s) => s,
        Err(e) => return fail(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
    };
    let mut connected = if servers.is_empty() {
        mcp::Connected::default()
    } else {
        live.event(&progress::Event::Phase {
            task: task.to_owned(),
            phase: format!("starting {} MCP server(s)", servers.len()),
        });
        rt.block_on(mcp::connect(&servers, &project, &runner))
    };
    // An allowed tool its server changed since review needs approval again.
    let approved_tools: Vec<String> = decisions
        .approved
        .iter()
        .filter_map(|g| match g {
            tools::policy::Grant::Tool { name } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let pinned = mcp::pin_reviewed(&s.plane_dir, &mut connected.tools, &approved_tools);
    connected.notices.extend(pinned);

    // One plane for the whole task. Its tool server enforces denials; the
    // frozen-test approvals are the skill's, from its journaled input.
    let tools_server = match tools::WorkspaceTools::new(&wt_root, policy.clone()) {
        Ok(t) => std::sync::Arc::new(t.with_runner(runner.clone())),
        Err(e) => return fail(CommandStatus::Error, e.to_string()),
    };
    let plane = match rt.block_on(async {
        substrate::Plane::open_with(
            &plane_dir,
            &s.model,
            provider,
            Some(tools_server),
            substrate::Hooks {
                approver: approver.clone(),
                progress: Some(Arc::clone(&live)),
                mcp: connected.clients.clone(),
                mcp_tools: connected.tools.clone(),
            },
        )
    }) {
        Ok(p) => p,
        Err(e) => return fail(CommandStatus::RunFailed, e.0),
    };
    let mut notices: Vec<String> = plane
        .replaced_journal
        .iter()
        .map(|p| {
            format!(
                "the journal could not be read by this build and was moved to {}; past runs stay in git",
                p.display()
            )
        })
        .chain(connected.notices.iter().cloned())
        .chain(sandbox_notice.clone())
        .collect();

    // A Convergence task is the converge model's claim that a gap exists;
    // it changes no code until a failing test demonstrates the gap.
    let demand_red = run_config.acceptance && context::is_convergence(&bundle.task.phase);
    // Where this run started: an attempt that does not pass is undone to here.
    let start = match worktree::checkpoint(&wt_root, &tmp) {
        Ok(c) => c,
        Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
    };

    live.event(&progress::Event::Phase {
        task: task.to_owned(),
        phase: "acceptance tests (judge-side)".to_owned(),
    });
    // 4b. Acceptance tests derived from the spec, written before the
    //     attempt so they are frozen, and required to be red now.
    let acceptance = if let Some(prepared) = interrupted
        .as_ref()
        .and_then(|m| m.acceptance.clone())
        .and_then(|a| serde_json::from_value::<PreparedAcceptance>(a).ok())
    {
        // The interrupted attempt's tests, already in the worktree.
        Some(prepared)
    } else if run_config.acceptance && bundle.story.to_lowercase().contains("acceptance") {
        Some(prepare_acceptance(&AcceptanceSetup {
            rt: &rt,
            plane: &plane,
            project: &project,
            worktree: &wt_root,
            tmp: &tmp,
            policy: &policy,
            feature: &feature_name,
            task,
            bundle: bundle_value.clone(),
            runner: &runner,
        }))
    } else {
        None
    };

    if demand_red && !acceptance.as_ref().is_some_and(|a| a.red) {
        let why = match &acceptance {
            None => "the spec has no acceptance examples to test it against".to_owned(),
            Some(a) => a.report["skipped"].as_str().map_or_else(
                || "its acceptance tests already pass before any change".to_owned(),
                |s| format!("no acceptance tests ({s})"),
            ),
        };
        let reason = format!(
            "convergence gap not demonstrated: {why}; a person decides whether {task} is real"
        );
        // The undemonstrated tests are kept under an attempt ref, not in the worktree.
        let saved = attempts::save(
            &wt_root,
            &tmp,
            &feature_name,
            task,
            &attempts::Record {
                run: None,
                outcome: "not_demonstrated",
                reason: &reason,
                pending: None,
                usage: None,
            },
        )
        .ok()
        .map(|s| s.reference);
        let _ = worktree::restore(&wt_root, &tmp, &start);
        let report = RunReport {
            run_id: String::new(),
            feature: feature_name.clone(),
            task: task.to_owned(),
            worktree: wt_path.clone(),
            attempt_status: "not_demonstrated".to_owned(),
            model_report: None,
            reason: Some(reason.clone()),
            pending: None,
            tool_calls: Vec::new(),
            changes: Vec::new(),
            verification: None,
            verdict: None,
            usage: None,
            integrity: Vec::new(),
            ticked: false,
            commit: None,
            acceptance: acceptance.as_ref().map(|a| a.report.clone()),
            earlier: Vec::new(),
            retry_stop: None,
            saved_attempt: saved,
            set_aside,
            notices: notices.clone(),
            monitor: None,
            mutation: None,
            samples: Vec::new(),
        };
        live.event(&progress::Event::TaskFinished {
            task: task.to_owned(),
            outcome: "not_demonstrated".to_owned(),
            message: redact(&reason, secret),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            usd: None,
        });
        return CommandResult::new("run", CommandStatus::NeedsHuman, &redact(&reason, secret))
            .with_data(CommandData::Run(Box::new(report)));
    }

    // 5. Checkpoint, attempt, checkpoint — again after a failed verdict,
    //    from the same checkpoint, with the verifier's digest.
    let before = match &interrupted {
        Some(m) => worktree::Checkpoint(m.before.clone()),
        None => match worktree::checkpoint(&wt_root, &tmp) {
            Ok(c) => c,
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        },
    };
    let acceptance_saved = acceptance
        .as_ref()
        .and_then(|a| serde_json::to_value(a).ok());
    // Tests that exist now are frozen for this attempt.
    let frozen: Vec<String> = worktree::files(&wt_root, &before)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| tools::paths::is_test_path(p))
        .collect();
    let mut usage: Option<serde_json::Value> = None;
    // Best-of-n: independent samples from the same checkpoint, each
    // with its own retries; the best by evidence is kept.
    let mut samples: Vec<(RunReport, String, worktree::Checkpoint, serde_json::Value)> = Vec::new();
    let (mut report, attempt_status) = loop {
        let mut earlier: Vec<speccoder_core::EarlierAttempt> = Vec::new();
        let mut digest: Option<serde_json::Value> = None;
        let mut attempt_no = 1u32;
        if run_config.best_of > 1 {
            live.event(&progress::Event::Phase {
                task: task.to_owned(),
                phase: format!("sample {} of {}", samples.len() + 1, run_config.best_of),
            });
        }
        let (mut report, attempt_status) = loop {
            // Everything the attempt's decisions depend on is in its journaled input.
            let input = substrate::attempt::input(
                bundle_value.clone(),
                &substrate::attempt::Settings {
                    checks: &check_argvs,
                    allow: &run_config.allow,
                    max_turns: run_config.max_turns,
                    max_tool_calls: run_config.max_tool_calls,
                    frozen: &frozen,
                    approved: &decisions.approved,
                    rejected: &decisions.rejected,
                    nudges: &decisions.nudges,
                    mcp: &connected.tools,
                    root: &wt_root_canonical,
                    frozen_additions: run_config.frozen_additions,
                    ask: approver.is_some(),
                    network_allow: &sandbox_config.network_allow,
                },
                digest.as_ref(),
            );
            live.event(&progress::Event::Phase {
                task: task.to_owned(),
                phase: format!("attempt {attempt_no}"),
            });
            // The attempt itself: speccoder's governed loop, or an external
            // agent whose result is judged from the diff alike.
            let (run_id, succeeded, mut output) = if let Some(exec) = &executor {
                let allow: Vec<Vec<String>> = check_argvs
                    .iter()
                    .chain(&run_config.allow)
                    .cloned()
                    .collect();
                let prompt = external::prompt(&bundle_value, digest.as_ref());
                let o = external::run(exec, &wt_root, &prompt, &allow, &tmp);
                (o.run_id, true, o.output)
            } else {
                // The same key as an interrupted attempt resumes it.
                let key = match &interrupted {
                    Some(m) if m.attempt == attempt_no && m.sample == samples.len() => {
                        m.key.clone()
                    }
                    _ => resume::new_key(&feature_name, task),
                };
                let _ = resume::save(
                    &marker_path,
                    &resume::Marker {
                        key: key.clone(),
                        before: before.0.clone(),
                        head: head_now.clone(),
                        attempt: attempt_no,
                        sample: samples.len(),
                        acceptance: acceptance_saved.clone(),
                    },
                );
                let outcome = match rt.block_on(plane.attempt_keyed(
                    &feature_name,
                    task,
                    input,
                    &key,
                )) {
                    Ok((o, resumed)) => {
                        if resumed {
                            notices.push(format!(
                                    "resumed run {} of an interrupted process: finished model and tool calls were read back from the journal, not repeated",
                                    o.run_id
                                ));
                        }
                        o
                    }
                    Err(e) => return fail(CommandStatus::RunFailed, e.0),
                };
                let succeeded = matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded);
                let mut output = outcome
                    .output
                    .as_ref()
                    .map(|o| o.peek().clone())
                    .unwrap_or(serde_json::Value::Null);
                if !succeeded && output["reason"].as_str().is_none() {
                    if !output.is_object() {
                        output = serde_json::json!({});
                    }
                    output["reason"] =
                        serde_json::Value::String(redact(&format!("{:?}", outcome.status), secret));
                }
                (outcome.run_id.to_string(), succeeded, output)
            };
            // The configured formatter, run by speccoder (not the model) on the
            // attempt's result; its changes are part of the attempt and audited.
            if let Some(argv) = run_config.format.as_ref().filter(|_| {
                succeeded && matches!(output["status"].as_str(), Some("completed" | "incomplete"))
            }) {
                use verify::process::CommandRunner as _;
                let ran = runner.run(&verify::process::Invocation {
                    argv: argv.clone(),
                    cwd: project.clone(),
                    env: verify::process::scrubbed_env(std::env::vars()),
                    timeout: std::time::Duration::from_secs(300),
                    output_cap: verify::process::OUTPUT_CAP,
                });
                if ran.exit_code != Some(0) {
                    notices.push(format!(
                        "the formatter (`{}`) did not succeed",
                        argv.join(" ")
                    ));
                }
            }
            let after = match worktree::checkpoint(&wt_root, &tmp) {
                Ok(c) => c,
                Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
            };
            let raw_changes = worktree::diff(&wt_root, &before, &after).unwrap_or_default();
            // A frozen test a person approved writing (for the task, or inline)
            // is not a violation; the audit still flags removed tests and skip
            // markers in it — approval permits editing a test, not weakening it.
            let approved_writes: std::collections::BTreeSet<String> = decisions
                .approved
                .iter()
                .filter_map(|g| match g {
                    tools::policy::Grant::Write { path } => Some(path.clone()),
                    _ => None,
                })
                .chain(
                    output["tool_calls"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .filter(|c| c["approved_by"] == "person")
                        .filter_map(|c| {
                            match tools::policy::Grant::of(c["tool"].as_str()?, &c["arguments"]) {
                                Some(tools::policy::Grant::Write { path }) => Some(path),
                                _ => None,
                            }
                        }),
                )
                .collect();
            let findings: Vec<String> = integrity::audit(
                &raw_changes,
                &worktree::patch(&wt_root, &before, &after).unwrap_or_default(),
                &frozen
                    .iter()
                    .filter(|p| !approved_writes.contains(*p))
                    .cloned()
                    .collect(),
                run_config.frozen_additions,
            )
            .into_iter()
            .map(|f| format!("integrity: {f}"))
            .collect();
            let changes = raw_changes
                .into_iter()
                .map(|c| {
                    let kind = match c.kind {
                        worktree::ChangeKind::Added => "added",
                        worktree::ChangeKind::Modified => "modified",
                        worktree::ChangeKind::Deleted => "deleted",
                    };
                    (c.path, kind.to_owned())
                })
                .collect();

            // 6. The attempt's own account.
            // A resumed attempt that was killed while a command ran: nobody
            // can know whether the command finished, so agentplane did not
            // run it again. The attempt is saved and undone below.
            let quarantined = !succeeded
                && output["reason"]
                    .as_str()
                    .is_some_and(|r| r.starts_with("Quarantined("));
            if quarantined {
                output["reason"] = serde_json::Value::String(
                    "the attempt was interrupted while a tool call was running; whether it finished cannot be known, so it was not run again. The attempt is saved and undone; run again to start a fresh attempt".to_owned(),
                );
            }
            let attempt_status = if succeeded {
                output["status"].as_str().unwrap_or("failed").to_owned()
            } else if quarantined {
                "interrupted".to_owned()
            } else {
                "failed".to_owned()
            };
            let mut report = RunReport {
                run_id: run_id.clone(),
                feature: feature_name.clone(),
                task: task.to_owned(),
                worktree: wt_path.clone(),
                attempt_status: attempt_status.clone(),
                model_report: output.get("report").cloned(),
                reason: output["reason"].as_str().map(str::to_owned),
                pending: output.get("pending").cloned(),
                tool_calls: output["tool_calls"].as_array().cloned().unwrap_or_default(),
                changes,
                verification: None,
                verdict: None,
                usage: output.get("usage").cloned(),
                integrity: findings.clone(),
                ticked: false,
                commit: None,
                acceptance: acceptance.as_ref().map(|a| a.report.clone()),
                earlier: Vec::new(),
                retry_stop: None,
                saved_attempt: None,
                set_aside: set_aside.clone(),
                notices: notices.clone(),
                monitor: None,
                mutation: None,
                samples: Vec::new(),
            };
            record_inline_approvals(&s.plane_dir, &feature_name, task, &report.tool_calls);
            // A person approving an MCP call reviewed the tool as it is now.
            for c in report
                .tool_calls
                .iter()
                .filter(|c| c["approved_by"] == "person")
            {
                if let Some(name) = c["tool"].as_str().filter(|n| n.starts_with(mcp::PREFIX)) {
                    mcp::mark_reviewed(&s.plane_dir, &connected.tools, name);
                }
            }

            live.event(&progress::Event::Phase {
                task: task.to_owned(),
                phase: "verifying".to_owned(),
            });
            // 7. The verdict: only the verifier decides.
            if matches!(attempt_status.as_str(), "completed" | "incomplete") {
                let mut current =
                    match verify::run_checks(&project, Some(verify::config::Tier::Fast), &runner) {
                        Ok(r) => r,
                        Err(e) => {
                            return fail(CommandStatus::ConfigError, format!("verification: {e}"));
                        }
                    };
                // Green: the spec-derived tests must pass now. Red-then-green is
                // task-specific evidence; green without red is not.
                let mut task_checks: Vec<&str> = Vec::new();
                if let Some(a) = acceptance.as_ref().filter(|a| a.argv.is_some()) {
                    let green =
                        run_acceptance(&project, a.argv.as_deref().unwrap_or_default(), &runner);
                    if let Some(r) = report.acceptance.as_mut() {
                        r["green"] = serde_json::json!(
                            green.status == speccoder_core::verify::CheckStatus::Passed
                        );
                    }
                    if a.red {
                        task_checks.push("acceptance");
                    }
                    current.checks.push(green);
                }
                let cmp = compare(&baseline, &current);
                current.passed = passes(&current.checks, Some(&cmp));
                // Task-specific checks arrive with M2; until then no check is task evidence.
                let judged = verdict(&task_checks, &current.checks, Some(&cmp));
                // Any integrity finding fails the verdict, whatever the checks said.
                report.verdict = Some(if findings.is_empty() {
                    judged
                } else {
                    let mut reasons = findings.clone();
                    if let speccoder_core::verify::Verdict::Failed { reasons: more } = judged {
                        reasons.extend(more);
                    }
                    speccoder_core::verify::Verdict::Failed { reasons }
                });
                current.comparison = Some(cmp);
                report.verification = Some(current);
            }

            // 7a. Mutation testing of a passing attempt's changes.
            let passing = attempt_status == "completed"
                && matches!(
                    report.verdict,
                    Some(
                        speccoder_core::verify::Verdict::Verified
                            | speccoder_core::verify::Verdict::RegressionOnly
                    )
                );
            if let Some(m) = mutation_config.as_ref().filter(|_| passing) {
                live.event(&progress::Event::Phase {
                    task: task.to_owned(),
                    phase: "mutation testing".to_owned(),
                });
                let patch = worktree::patch(&wt_root, &before, &after).unwrap_or_default();
                let scratch = runner.scratch().unwrap_or_else(|| tmp.clone());
                if let Some(result) = mutation::run(m, &project, &patch, &scratch, &runner) {
                    if m.mode == mutation::Mode::Require
                        && let Some(reason) = mutation::failure(&result)
                    {
                        report.verdict = Some(speccoder_core::verify::Verdict::Failed {
                            reasons: vec![reason],
                        });
                    }
                    report.mutation = Some(result);
                }
            }

            usage = add_usage(usage.as_ref(), report.usage.as_ref());
            // retry a completed attempt the verifier failed, when it can help.
            let (Some(speccoder_core::verify::Verdict::Failed { reasons }), Some(verification)) =
                (report.verdict.clone(), report.verification.as_ref())
            else {
                break (report, attempt_status);
            };
            if attempt_status != "completed" {
                break (report, attempt_status);
            }
            let discarded: Vec<String> = report.changes.iter().map(|(p, _)| p.clone()).collect();
            let d = speccoder_core::retry::digest(attempt_no, &reasons, verification, &discarded);
            let fingerprint = speccoder_core::retry::fingerprint(&d);
            let stop = if !findings.is_empty() {
                Some("integrity findings are not retried".to_owned())
            } else if let Some(infra) = speccoder_core::retry::infrastructure(verification) {
                Some(format!("not retried: {infra}"))
            } else if earlier.last().is_some_and(|e| e.fingerprint == fingerprint) {
                Some(format!("same failure twice: {fingerprint}"))
            } else if attempt_no >= run_config.max_attempts {
                Some(format!(
                    "{attempt_no} of {} attempts used",
                    run_config.max_attempts
                ))
            } else {
                None
            };
            if let Some(stop) = stop {
                report.retry_stop = Some(stop);
                break (report, attempt_status);
            }
            if let Err(e) = worktree::restore(&wt_root, &tmp, &before) {
                report.retry_stop = Some(format!("cannot undo the attempt: {}", e.0));
                break (report, attempt_status);
            }
            earlier.push(speccoder_core::EarlierAttempt {
                run_id: report.run_id.clone(),
                attempt_status: attempt_status.clone(),
                verdict: report.verdict.clone(),
                fingerprint,
                digest: d.clone(),
                usage: report.usage.clone(),
            });
            digest = serde_json::to_value(&d).ok();
            attempt_no += 1;
        };
        report.earlier = earlier;
        if run_config.best_of <= 1 {
            break (report, attempt_status);
        }
        let end = match worktree::checkpoint(&wt_root, &tmp) {
            Ok(c) => c,
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        };
        let patch = worktree::patch(&wt_root, &before, &end).unwrap_or_default();
        let summary = sample_summary(&report, &attempt_status, &patch);
        samples.push((report, attempt_status, end, summary));
        if samples.len() >= run_config.best_of as usize {
            let best = best_sample(&samples);
            let summaries: Vec<serde_json::Value> = samples
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut v = s.3.clone();
                    v["kept"] = serde_json::json!(i == best);
                    v
                })
                .collect();
            let (mut chosen, status, end, _) = samples.swap_remove(best);
            if best != samples.len() {
                // Not the sample the worktree holds: put the chosen one back.
                if let Err(e) = worktree::restore(&wt_root, &tmp, &end) {
                    return fail(CommandStatus::GitPrecondition, e.0);
                }
            }
            chosen.samples = summaries;
            break (chosen, status);
        }
        if let Err(e) = worktree::restore(&wt_root, &tmp, &before) {
            return fail(CommandStatus::GitPrecondition, e.0);
        }
    };
    report.usage = usage;

    // 7b. The gaming monitor: reasoning-blind, judge-side, add-only.
    //     It sees the task, the diff and the tool calls — not the model's
    //     account — and a suspicion only stops the tick for a person.
    let verdict_passed = attempt_status == "completed"
        && matches!(
            report.verdict,
            Some(
                speccoder_core::verify::Verdict::Verified
                    | speccoder_core::verify::Verdict::RegressionOnly
            )
        );
    let mut suspected = false;
    if run_config.monitor && verdict_passed {
        live.event(&progress::Event::Phase {
            task: task.to_owned(),
            phase: "gaming monitor (judge-side)".to_owned(),
        });
        let patch = worktree::checkpoint(&wt_root, &tmp)
            .and_then(|now| worktree::patch(&wt_root, &start, &now))
            .unwrap_or_default();
        let calls: Vec<serde_json::Value> = report
            .tool_calls
            .iter()
            .map(|c| {
                let tool = c["tool"].as_str().unwrap_or_default();
                serde_json::json!({ "tool": tool, "target": progress::target(tool, &c["arguments"]), "decision": c["decision"] })
            })
            .collect();
        let input = substrate::monitor::input(&bundle_value, &patch, &calls, &frozen);
        report.monitor = Some(
            match rt.block_on(plane.monitor(&feature_name, task, input)) {
                Ok(o) => {
                    let out = o
                        .output
                        .as_ref()
                        .map(|v| v.peek().clone())
                        .unwrap_or(serde_json::Value::Null);
                    suspected = out["answer"]["suspected"] == true;
                    serde_json::json!({
                        "suspected": suspected,
                        "findings": out["answer"]["findings"],
                        "usage": out["usage"],
                        "run_id": o.run_id.to_string(),
                    })
                }
                // A monitor that fails blocks nothing: it can only add doubt.
                Err(e) => serde_json::json!({ "error": redact(&e.0, secret) }),
            },
        );
    }

    // 8. A passed task is ticked by the engine and committed.
    let passed = verdict_passed && !suspected;
    if passed && let (Some(t), Some(file)) = (&parsed_task, &tasks_file) {
        match speccoder_speckit::mark_task_complete(file, task, &t.line_hash) {
            Ok(()) => report.ticked = true,
            Err(e) => report.integrity.push(format!("tick refused: {e}")),
        }
        if report.ticked && run_config.commit_per_task {
            let verdict_word = report
                .verdict
                .as_ref()
                .and_then(|v| serde_json::to_value(v).ok())
                .and_then(|v| v["verdict"].as_str().map(str::to_owned))
                .unwrap_or_default();
            // Attempts and the judge-side acceptance call together.
            let usage = add_usage(
                report.usage.as_ref(),
                report.acceptance.as_ref().and_then(|a| a.get("usage")),
            );
            let message = format!(
                "{}\n\n{}\n\nSpec-Task: {feature_name}/{task}\nVerdict: {verdict_word}\nRun: {}\n{}{}",
                commit_subject(task, &t.title),
                t.title,
                report.run_id,
                attempts::usage_trailer(usage.as_ref()),
                report
                    .mutation
                    .as_ref()
                    .and_then(mutation::trailer)
                    .unwrap_or_default()
            );
            match commit_attempt(&wt_root, &message) {
                Ok(id) => report.commit = Some(id),
                Err(e) => report.integrity.push(format!("commit failed: {e}")),
            }
        }
    }

    // 9. An attempt that did not pass is saved under an attempt ref and
    //    undone: the next attempt starts where this one did.
    if !passed {
        let outcome = match (attempt_status.as_str(), &report.verdict) {
            _ if suspected => "suspected_gaming".to_owned(),
            ("completed", _) => "verification_failed".to_owned(),
            (other, _) => other.to_owned(),
        };
        let reason = match &report.verdict {
            _ if suspected => monitor_summary(report.monitor.as_ref()),
            Some(speccoder_core::verify::Verdict::Failed { reasons }) => reasons.join("; "),
            _ => report
                .reason
                .clone()
                .unwrap_or_else(|| attempt_status.clone()),
        };
        let run = (!report.run_id.is_empty()).then_some(report.run_id.as_str());
        let reason = redact(&reason, secret);
        let record = attempts::Record {
            run,
            outcome: &outcome,
            reason: &reason,
            pending: report.pending.as_ref(),
            usage: report.usage.as_ref(),
        };
        match attempts::save(&wt_root, &tmp, &feature_name, task, &record)
            .and_then(|saved| worktree::restore(&wt_root, &tmp, &start).map(|_| saved))
        {
            Ok(saved) => report.saved_attempt = Some(saved.reference),
            Err(e) => report
                .integrity
                .push(format!("saving the attempt failed: {}", e.0)),
        }
    }

    let (status, message) = match (&report.verdict, attempt_status.as_str()) {
        _ if suspected => (
            CommandStatus::NeedsHuman,
            format!(
                "the checks passed, but the gaming monitor suspects: {}; the attempt is saved, not ticked — review it, then `speccoder accept` or run again",
                monitor_summary(report.monitor.as_ref())
            ),
        ),
        (_, "incomplete") => (
            CommandStatus::NeedsHuman,
            format!(
                "attempt incomplete by the model's own account{}",
                if report.changes.is_empty() {
                    "; no files changed"
                } else {
                    ""
                }
            ),
        ),
        (Some(speccoder_core::verify::Verdict::Failed { reasons }), _) => (
            CommandStatus::VerificationFailed,
            format!("verdict: failed — {}", reasons.join("; ")),
        ),
        (Some(v), _) => (
            CommandStatus::Ok,
            format!(
                "verdict: {}",
                serde_json::to_value(v)
                    .ok()
                    .and_then(|v| v["verdict"].as_str().map(str::to_owned))
                    .unwrap_or_default()
            ),
        ),
        (None, "blocked" | "needs_approval" | "budget_exhausted" | "denied" | "interrupted") => (
            CommandStatus::NeedsHuman,
            format!(
                "attempt {attempt_status}: {}",
                report
                    .reason
                    .clone()
                    .or_else(|| report
                        .pending
                        .as_ref()
                        .map(|p| p["reason"].as_str().unwrap_or_default().to_owned()))
                    .unwrap_or_default()
            ),
        ),
        (None, _) => (
            CommandStatus::RunFailed,
            format!(
                "attempt failed: {}",
                report.reason.clone().unwrap_or_default()
            ),
        ),
    };
    let total = add_usage(
        report.usage.as_ref(),
        report.acceptance.as_ref().and_then(|a| a.get("usage")),
    );
    let tokens = |k: &str| total.as_ref().and_then(|u| u[k].as_u64()).unwrap_or(0);
    live.event(&progress::Event::TaskFinished {
        task: task.to_owned(),
        // The verdict for a completed attempt; otherwise how the attempt ended
        // (an incomplete attempt with green checks is not a pass).
        outcome: if suspected {
            "suspected_gaming".to_owned()
        } else if attempt_status == "completed" {
            report
                .verdict
                .as_ref()
                .and_then(|v| serde_json::to_value(v).ok())
                .and_then(|v| v["verdict"].as_str().map(str::to_owned))
                .unwrap_or_else(|| attempt_status.clone())
        } else {
            attempt_status.clone()
        },
        message: redact(&message, secret),
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        cache_read_tokens: tokens("cache_read_tokens"),
        usd: None,
    });
    CommandResult::new("run", status, &redact(&message, secret))
        .with_data(CommandData::Run(Box::new(report)))
}

/// The monitor's findings in one line: `kind: evidence; …`.
fn monitor_summary(monitor: Option<&serde_json::Value>) -> String {
    monitor
        .and_then(|m| m["findings"].as_array())
        .map(|f| {
            f.iter()
                .map(|x| {
                    format!(
                        "{}: {}",
                        x["kind"].as_str().unwrap_or("other"),
                        x["evidence"]
                            .as_str()
                            .unwrap_or_default()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "no details given".to_owned())
}

/// Inline "for this task" approvals become task decisions, as if made with
/// `speccoder approve`, so a retry or a later attempt is not asked again.
fn record_inline_approvals(dir: &Path, feature: &str, task: &str, calls: &[serde_json::Value]) {
    // Every inline decision, with who made it; an approval for the
    // task also joins the task's approvals.
    let decided: Vec<(approvals::Decided, bool)> = calls
        .iter()
        .filter(|c| c["approved_by"] == "person" || c["rejected_by"] == "person")
        .filter_map(|c| {
            let grant = tools::policy::Grant::of(c["tool"].as_str()?, &c["arguments"])?;
            let by = serde_json::from_value::<approvals::Who>(c["approver"].clone())
                .unwrap_or_else(|_| approvals::Who {
                    name: "unknown".to_owned(),
                    via: "inline".to_owned(),
                });
            let approved = c["approved_by"] == "person";
            let for_task = approved && c["for_task"] == true;
            Some((
                approvals::Decided {
                    verb: if approved { "approve" } else { "reject" }.to_owned(),
                    grant,
                    reason: c["rejection"].as_str().map(str::to_owned),
                    for_task,
                    by,
                    at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs()),
                },
                for_task,
            ))
        })
        .collect();
    if decided.is_empty() {
        return;
    }
    if let Ok(mut d) = approvals::Decisions::load(dir) {
        for (decision, for_task) in decided {
            if for_task {
                d.approve(feature, task, decision.grant.clone(), &decision.by);
            } else {
                d.note(feature, task, decision);
            }
        }
        let _ = d.save(dir);
    }
}

/// Sums two token usage objects key by key.
fn add_usage(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match (a, b) {
        (None, b) => b.cloned(),
        (a, None) => a.cloned(),
        (Some(a), Some(b)) => {
            let mut sum = serde_json::Map::new();
            for key in [
                "input_tokens",
                "output_tokens",
                "cache_read_tokens",
                "cache_write_tokens",
            ] {
                let n = a[key].as_u64().unwrap_or(0) + b[key].as_u64().unwrap_or(0);
                sum.insert(key.to_owned(), serde_json::json!(n));
            }
            Some(serde_json::Value::Object(sum))
        }
    }
}

// ── eval ───────────────────────────────────────────────────────────

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
    let suite = match eval::suite_config(dir) {
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
        Ok(p) => eval::run_suite_as(dir, only, &p, Some(&key), keep, repeat, executor),
        Err(e) => CommandResult::new("eval", CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// What a replay's finding means for the person: a faithful
/// replay of any ending is reproduced; a different effect or ending is a
/// divergence; a run this build cannot re-derive is neither.
fn replay_verdict(finding: &agentplane::runtime::Finding) -> (&'static str, CommandStatus, String) {
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

/// Commits everything in the worktree on its branch with speccoder's
/// identity, without hooks; returns the commit id. Never pushes.
fn commit_attempt(worktree: &Path, message: &str) -> Result<String, String> {
    let g = git::Git::at(worktree);
    let base = [
        "-c",
        "user.name=speccoder",
        "-c",
        "user.email=speccoder@users.noreply.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    let mut add = base.to_vec();
    add.extend(["add", "-A"]);
    g.run(&add).map_err(|e| e.0)?;
    let mut commit = base.to_vec();
    commit.extend(["commit", "-q", "--no-verify", "-m", message]);
    g.run(&commit).map_err(|e| e.0)?;
    g.line(&["rev-parse", "HEAD"]).map_err(|e| e.0)
}

/// `speccoder run` without `--task`: attempts ready tasks in document order
/// (read from the worktree) until the feature is done or an attempt does
/// not pass. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn run_feature(
    path: &Path,
    feature: Option<&str>,
    options: &RunOptions,
    converge: bool,
) -> CommandResult {
    with_driver("run", path, feature, |state, provider, key| {
        if converge {
            run_converged_with(path, feature, state, provider, Some(key), options)
        } else {
            run_feature_with(path, feature, state, provider, Some(key), options)
        }
    })
}

/// How a run behaves beyond its configuration.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// At most this many tasks in a feature run.
    pub max_tasks: Option<usize>,
    /// Who to ask inline when a call needs approval; without one,
    /// the attempt stops and `speccoder approve` decides.
    pub approver: Option<Arc<dyn approvals::Approver>>,
    /// Where live progress goes: the CLI's printer.
    pub progress: Option<Arc<dyn progress::Progress>>,
    /// Ready `[P]` tasks run at once, each in its own worktree; 0 or 1: one at a time.
    pub parallel: usize,
    /// Run the task in this lane's worktree instead of the feature's.
    pub lane: Option<String>,
    /// Set by a person (an editor's cancel): a feature run stops
    /// before its next task.
    pub cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}

/// `speccoder converge`: assess the finished feature and append what remains
///. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn converge(path: &Path, feature: Option<&str>) -> CommandResult {
    with_driver("converge", path, feature, |state, provider, key| {
        convergence::converge_with(path, feature, state, Arc::clone(provider), Some(key))
    })
}

/// Opens the configured model driver (key from the environment only) and
/// hands it to `f` with the state directory.
fn with_driver(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    f: impl FnOnce(&Path, &Arc<dyn agentplane::model::ModelProvider>, &str) -> CommandResult,
) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
        return CommandResult::new(
            command,
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    let model = match setup(path, feature, &state, command) {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return CommandResult::new(command, CommandStatus::ConfigError, &e),
    };
    match driver {
        Ok(provider) => f(&state, &provider, &key),
        Err(e) => CommandResult::new(command, CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// `run --converge`: work the feature through, converge, and work through
/// what convergence appended — until it converges, stops making progress,
/// reaches `[converge] max_rounds`, or an attempt needs a person.
pub fn run_converged_with(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: &Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let mut attempts = Vec::new();
    let mut rounds = Vec::new();
    loop {
        let res = run_feature_with(path, feature, state, provider, secret, options);
        let Some(CommandData::Feature(fr)) = res.data() else {
            return res;
        };
        let mut fr = (**fr).clone();
        attempts.append(&mut fr.attempts);
        let finished = res.status() == CommandStatus::Ok && fr.stop.starts_with("feature done");
        if !finished {
            fr.attempts = attempts;
            fr.converge = rounds;
            return CommandResult::new("run", res.status(), res.message())
                .with_data(CommandData::Feature(Box::new(fr)));
        }
        if let Some(p) = &options.progress {
            p.event(&progress::Event::Notice {
                text: format!("converge: assessing {} against its spec", fr.feature),
            });
        }
        let c = convergence::converge_with(path, feature, state, Arc::clone(provider), secret);
        if let Some(p) = &options.progress {
            p.event(&progress::Event::Notice {
                text: format!("converge: {}", c.message()),
            });
        }
        let outcome = match c.data() {
            Some(CommandData::Converge(r)) => {
                let outcome = r.outcome.clone();
                rounds.push((**r).clone());
                outcome
            }
            _ => String::new(),
        };
        if outcome != "tasks_appended" {
            fr.attempts = attempts;
            fr.converge = rounds;
            fr.stop = format!("converge: {}", c.message());
            return CommandResult::new("run", c.status(), &fr.stop.clone())
                .with_data(CommandData::Feature(Box::new(fr)));
        }
    }
}

/// [`run_feature`] with an explicit state directory and model driver.
pub fn run_feature_with(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let max_tasks = options.max_tasks;
    use speccoder_core::FeatureReport;
    use speccoder_core::snapshot::TaskState;

    let mut attempts = Vec::new();
    let mut feature_name = feature.map(str::to_owned);
    let hook_lines: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let parallel_lines: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let finish = |name: Option<String>, attempts, status: CommandStatus, stop: String| {
        CommandResult::new("run", status, &stop).with_data(CommandData::Feature(Box::new(
            FeatureReport {
                feature: name.unwrap_or_default(),
                attempts,
                stop,
                converge: Vec::new(),
                hooks: hook_lines.borrow().clone(),
                parallel: parallel_lines.borrow().clone(),
            },
        )))
    };
    // Tasks that must run on their own after a parallel group did not merge.
    let mut one_at_a_time: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut parallel_off = false;
    let mut budget = max_tasks.unwrap_or(usize::MAX);
    // Spec Kit's `before_implement` hooks.
    match hosting::run_hooks(
        "before_implement",
        "",
        path,
        state,
        provider,
        secret,
        options,
    ) {
        Ok(lines) => hook_lines.borrow_mut().extend(lines),
        Err(stop) => {
            return finish(
                feature_name,
                attempts,
                stop.status(),
                stop.message().to_owned(),
            );
        }
    }
    while budget > 0 {
        if options
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst))
        {
            return finish(
                feature_name,
                attempts,
                CommandStatus::NeedsHuman,
                "cancelled by the person before the next task".to_owned(),
            );
        }
        // The next ready task, read from the worktree.
        let wt = worktree_with(path, feature_name.as_deref(), state);
        let Some(CommandData::Worktree(w)) = wt.data() else {
            return CommandResult::new("run", wt.status(), wt.message());
        };
        if wt.status() != CommandStatus::Ok {
            return CommandResult::new("run", wt.status(), wt.message())
                .with_data(CommandData::Worktree(w.clone()));
        }
        feature_name = Some(w.feature.clone());
        let project = std::path::PathBuf::from(w.project_dir.clone().unwrap_or_default());
        let Ok(snap) = speccoder_speckit::inspect(&project, Some(&w.feature)) else {
            return CommandResult::new(
                "run",
                CommandStatus::NotAProject,
                "the worktree has no Spec Kit project",
            );
        };
        let schedule = snap
            .feature
            .as_ref()
            .and_then(|f| f.schedule.clone())
            .unwrap_or_else(|| speccoder_core::snapshot::Schedule {
                tasks: Vec::new(),
                totals: speccoder_core::snapshot::ScheduleTotals::default(),
            });
        // Skipped tasks are passed by; what depends on them stays blocked.
        let skipped = controls::skipped(&project, feature_name.as_deref().unwrap_or_default());
        let Some(next) = schedule
            .tasks
            .iter()
            .find(|t| t.state == TaskState::Ready && !skipped.contains_key(&t.id))
        else {
            let open: Vec<String> = schedule
                .tasks
                .iter()
                .filter(|t| t.state != TaskState::Done)
                .map(|t| {
                    if skipped.contains_key(&t.id) {
                        format!("{} (skipped)", t.id)
                    } else {
                        t.id.clone()
                    }
                })
                .collect();
            return if open.is_empty() {
                // Spec Kit's `after_implement` hooks.
                match hosting::run_hooks(
                    "after_implement",
                    "",
                    path,
                    state,
                    provider,
                    secret,
                    options,
                ) {
                    Ok(lines) => hook_lines.borrow_mut().extend(lines),
                    Err(stop) => {
                        return finish(
                            feature_name,
                            attempts,
                            stop.status(),
                            stop.message().to_owned(),
                        );
                    }
                }
                finish(
                    feature_name,
                    attempts,
                    CommandStatus::Ok,
                    "feature done: every task is ticked".to_owned(),
                )
            } else {
                finish(
                    feature_name,
                    attempts,
                    CommandStatus::TaskNotReady,
                    format!("no task can run; open: {}", open.join(", ")),
                )
            };
        };
        // ready [P] tasks run at once, each in its own lane.
        let parallel_ids: std::collections::BTreeSet<String> = snap
            .feature
            .as_ref()
            .and_then(|f| f.tasks.as_ref())
            .map(|t| {
                t.tasks
                    .iter()
                    .filter(|t| t.parallel)
                    .map(|t| t.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let group: Vec<String> = schedule
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Ready && !skipped.contains_key(&t.id))
            .filter(|t| parallel_ids.contains(&t.id) && !one_at_a_time.contains(&t.id))
            .map(|t| t.id.clone())
            .take(options.parallel.min(budget))
            .collect();
        if !parallel_off && group.len() >= 2 {
            budget = budget.saturating_sub(group.len());
            let feature = feature_name.clone().unwrap_or_default();
            let feature_wt = std::path::PathBuf::from(w.path.clone().unwrap_or_default());
            let lanes = run_lanes(path, &feature, &group, state, provider, secret, options);
            let outcome = merge_lanes(path, &feature_wt, &project, &feature, lanes);
            attempts.extend(outcome.reports);
            parallel_lines.borrow_mut().extend(outcome.lines);
            one_at_a_time.extend(outcome.one_at_a_time);
            parallel_off |= outcome.reverted;
            if let Some((task, status, message)) = outcome.stop {
                return finish(feature_name, attempts, status, format!("{task}: {message}"));
            }
            continue;
        }
        budget -= 1;
        let task = next.id.clone();
        let res = run_task(
            path,
            feature_name.as_deref(),
            &task,
            state,
            std::sync::Arc::clone(provider),
            secret,
            options,
        );
        if let Some(CommandData::Run(r)) = res.data() {
            attempts.push((**r).clone());
        }
        if res.status() != CommandStatus::Ok {
            return finish(
                feature_name,
                attempts,
                res.status(),
                format!("{task}: {}", res.message()),
            );
        }
    }
    finish(
        feature_name,
        attempts,
        CommandStatus::Ok,
        "stopped at --max-tasks".to_owned(),
    )
}

/// `speccoder: T004 <title>` cut to 72 characters on a char boundary.
fn commit_subject(task: &str, title: &str) -> String {
    let full = format!("speccoder: {task} {title}");
    if full.chars().count() <= 72 {
        return full;
    }
    let cut: String = full.chars().take(71).collect();
    format!("{}…", cut.trim_end())
}

// ── parallel lanes ────────────────────────────────────────────────

/// Lane worktrees are created one at a time (git's worktree bookkeeping).
static LANE_SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs each task of `group` in its own lane, at once. Lanes never ask
/// inline (prompts would interleave): a call that needs approval stops the
/// lane, and the person decides with `speccoder approvals`.
fn run_lanes(
    path: &Path,
    feature: &str,
    group: &[String],
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> Vec<(String, CommandResult)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = group
            .iter()
            .map(|task| {
                let lane_options = RunOptions {
                    max_tasks: None,
                    approver: None,
                    progress: options.progress.clone(),
                    parallel: 1,
                    lane: Some(task.clone()),
                    cancel: None,
                };
                let provider = std::sync::Arc::clone(provider);
                scope.spawn(move || {
                    let res = run_task(
                        path,
                        Some(feature),
                        task,
                        state,
                        provider,
                        secret,
                        &lane_options,
                    );
                    (task.clone(), res)
                })
            })
            .collect();
        handles
            .into_iter()
            .zip(group)
            .map(|(h, task)| {
                h.join().unwrap_or_else(|_| {
                    (
                        task.clone(),
                        CommandResult::new("run", CommandStatus::Error, "the lane panicked"),
                    )
                })
            })
            .collect()
    })
}

/// What merging a parallel group did.
#[derive(Default)]
struct LanesMerged {
    reports: Vec<speccoder_core::RunReport>,
    lines: Vec<String>,
    one_at_a_time: Vec<String>,
    reverted: bool,
    stop: Option<(String, CommandStatus, String)>,
}

/// Merges each passed lane's commit into the feature branch, then runs the
/// checks on the merged whole: tasks that pass alone can fail together.
fn merge_lanes(
    path: &Path,
    feature_wt: &Path,
    project: &Path,
    feature: &str,
    lanes: Vec<(String, CommandResult)>,
) -> LanesMerged {
    let mut out = LanesMerged::default();
    let g = git::Git::at(feature_wt);
    let pre = g.line(&["rev-parse", "HEAD"]).unwrap_or_default();
    let repo_root = git::Git::at(path).toplevel().ok().flatten();
    let (runner, _) = sandbox::runner_at(feature_wt).unwrap_or((sandbox::Runner::Plain, None));
    let tier = Some(verify::config::Tier::Fast);
    let baseline = verify::run_checks(project, tier, &runner).ok();
    let mut merged: Vec<String> = Vec::new();
    for (task, res) in lanes {
        let report = match res.data() {
            Some(CommandData::Run(r)) => Some((**r).clone()),
            _ => None,
        };
        match (res.status(), report.as_ref().and_then(|r| r.commit.clone())) {
            (CommandStatus::Ok, Some(commit)) => {
                match merge_lane(feature_wt, project, feature, &task, &commit) {
                    Ok(()) => merged.push(task.clone()),
                    Err(e) => {
                        out.lines.push(format!(
                            "{task}: not merged ({e}); it runs again on its own"
                        ));
                        out.one_at_a_time.push(task.clone());
                    }
                }
            }
            (CommandStatus::Ok, None) => {}
            (status, _) => {
                if out.stop.is_none() {
                    out.stop = Some((task.clone(), status, res.message().to_owned()));
                }
            }
        }
        if let Some(r) = report {
            out.reports.push(r);
        }
        if let Some(root) = &repo_root {
            let _ = worktree::remove_lane(root, feature, &task);
        }
    }
    if merged.is_empty() {
        return out;
    }
    let after = verify::run_checks(project, tier, &runner).ok();
    let together = match (&baseline, &after) {
        (Some(b), Some(a)) => {
            speccoder_core::verify::passes(&a.checks, Some(&speccoder_core::verify::compare(b, a)))
        }
        _ => false,
    };
    if together {
        out.lines.push(format!(
            "{} ran in parallel, merged and passed the checks together",
            merged.join(", ")
        ));
    } else {
        // The feature branch goes back to before the group; the lanes'
        // work stays in their reports.
        let _ = g.run(&["reset", "-q", "--hard", &pre]);
        out.reverted = true;
        out.one_at_a_time.extend(merged.iter().cloned());
        out.lines.push(format!(
            "{} passed alone but not together; the merge was undone and the remaining tasks run one at a time",
            merged.join(", ")
        ));
    }
    out
}

/// Applies a lane's task commit on the feature branch: its changes, and
/// the feature's `tasks.md` with this task ticked (lanes tick neighbouring
/// lines, which git would call a conflict).
fn merge_lane(
    feature_wt: &Path,
    project: &Path,
    feature: &str,
    task: &str,
    commit: &str,
) -> Result<(), String> {
    let g = git::Git::at(feature_wt);
    let ident = [
        "-c",
        "user.name=speccoder",
        "-c",
        "user.email=speccoder@users.noreply.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    let tasks_file = speccoder_speckit::inspect(project, Some(feature))
        .ok()
        .and_then(|s| s.feature)
        .and_then(|f| f.tasks)
        .map(|t| project.join(t.path))
        .ok_or("the feature has no tasks.md")?;
    let tasks_arg = tasks_file.to_string_lossy().into_owned();
    // Untracked files the commit adds were generated here by speccoder's own
    // check runs (a `Cargo.lock`, say); they would stop the cherry-pick.
    let added = g
        .run(&[
            "diff",
            "--name-only",
            "--diff-filter=A",
            &format!("{commit}^"),
            commit,
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let untracked = g
        .run(&["ls-files", "--others", "--exclude-standard"])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    for f in added.lines().filter(|f| untracked.lines().any(|u| u == *f)) {
        let _ = std::fs::remove_file(feature_wt.join(f));
    }
    let mut pick = ident.to_vec();
    pick.extend(["cherry-pick", "-n", commit]);
    let picked = g.run(&pick);
    let conflicted = g
        .run(&["diff", "--name-only", "--diff-filter=U"])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let tasks_rel = tasks_file
        .strip_prefix(feature_wt)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A failure that left no conflict applied nothing (an empty merge would
    // tick a task whose work is missing).
    if let Err(e) = &picked
        && conflicted.trim().is_empty()
    {
        let _ = g.run(&["reset", "-q", "--hard", "HEAD"]);
        return Err(e
            .0
            .lines()
            .next()
            .unwrap_or("cherry-pick failed")
            .to_owned());
    }
    let others: Vec<&str> = conflicted.lines().filter(|f| *f != tasks_rel).collect();
    if !others.is_empty() {
        let _ = g.run(&["reset", "-q", "--hard", "HEAD"]);
        return Err(format!("conflicts in {}", others.join(", ")));
    }
    g.run(&["checkout", "HEAD", "--", &tasks_arg])
        .map_err(|e| e.0)?;
    let line_hash = speccoder_speckit::inspect(project, Some(feature))
        .ok()
        .and_then(|s| s.feature)
        .and_then(|f| f.tasks)
        .and_then(|t| t.tasks.into_iter().find(|t| t.id == task))
        .map(|t| t.line_hash)
        .ok_or_else(|| format!("{task} is not in the feature's tasks.md"))?;
    speccoder_speckit::mark_task_complete(&tasks_file, task, &line_hash)
        .map_err(|e| e.to_string())?;
    let mut add = ident.to_vec();
    add.extend(["add", "-A"]);
    g.run(&add).map_err(|e| e.0)?;
    let mut c = ident.to_vec();
    c.extend(["commit", "-q", "--no-verify", "-C", commit]);
    g.run(&c).map_err(|e| e.0)?;
    Ok(())
}

// ── best-of-n ─────────────────────────────────────────────────────

/// One sample's evidence: verdict, surviving mutants, diff size, tokens.
fn sample_summary(
    report: &speccoder_core::RunReport,
    status: &str,
    patch: &str,
) -> serde_json::Value {
    use speccoder_core::verify::Verdict;
    let rank = match (status, &report.verdict) {
        ("completed", Some(Verdict::Verified)) => 2,
        ("completed", Some(Verdict::RegressionOnly)) => 1,
        _ => 0,
    };
    let changed = patch
        .lines()
        .filter(|l| {
            (l.starts_with('+') && !l.starts_with("+++"))
                || (l.starts_with('-') && !l.starts_with("---"))
        })
        .count();
    let tokens = report.usage.as_ref().map_or(0, |u| {
        u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0)
    });
    serde_json::json!({
        "run_id": report.run_id,
        "attempt_status": status,
        "verdict": report.verdict,
        "rank": rank,
        "survivors": report.mutation.as_ref().and_then(|m| m["missed"].as_u64()),
        "changed_lines": changed,
        "tokens": tokens,
    })
}

/// The best sample: passing first (verified over regression-only), then
/// fewer surviving mutants, a smaller diff, fewer tokens; ties keep the earlier.
fn best_sample(
    samples: &[(
        speccoder_core::RunReport,
        String,
        worktree::Checkpoint,
        serde_json::Value,
    )],
) -> usize {
    let key = |s: &serde_json::Value| {
        (
            std::cmp::Reverse(s["rank"].as_u64().unwrap_or(0)),
            s["survivors"].as_u64().unwrap_or(0),
            s["changed_lines"].as_u64().unwrap_or(0),
            s["tokens"].as_u64().unwrap_or(0),
        )
    };
    (0..samples.len())
        .min_by_key(|&i| (key(&samples[i].3), i))
        .unwrap_or(0)
}

// ── acceptance ─────────────────────────────────────────────────────

struct AcceptanceSetup<'a> {
    rt: &'a tokio::runtime::Runtime,
    plane: &'a substrate::Plane,
    project: &'a Path,
    worktree: &'a Path,
    tmp: &'a Path,
    policy: &'a tools::policy::Policy,
    feature: &'a str,
    task: &'a str,
    bundle: serde_json::Value,
    runner: &'a sandbox::Runner,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PreparedAcceptance {
    /// What the run report shows.
    report: serde_json::Value,
    /// The command, when the tests were written.
    argv: Option<Vec<String>>,
    /// They failed before the attempt.
    red: bool,
}

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
];

fn prepare_acceptance(a: &AcceptanceSetup<'_>) -> PreparedAcceptance {
    use serde_json::json;
    let skipped = |why: String, report: serde_json::Value| {
        let mut report = report;
        report["skipped"] = json!(why);
        PreparedAcceptance {
            report,
            argv: None,
            red: false,
        }
    };

    let files: Vec<String> = worktree::checkpoint(a.worktree, a.tmp)
        .and_then(|c| worktree::files(a.worktree, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let input = substrate::acceptance::input(a.bundle.clone(), &files, &manifest);
    let outcome = a.rt.block_on(a.plane.acceptance(a.feature, a.task, input));
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return skipped(format!("generation failed: {}", e.0), json!({})),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    let base = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"], "notes": answer["notes"] });
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return skipped(
            format!("generation did not succeed: {}", outcome.status.as_str()),
            base,
        );
    }

    // Validate: new test files only, a command the policy allows.
    let protected = tools::paths::Protected::default();
    let mut written = Vec::new();
    let mut planned = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
            continue;
        };
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(e) => return skipped(format!("`{path}`: {}", e.0), base),
        };
        if !tools::paths::is_test_path(&rel) || protected.write_class(&rel).is_some() {
            return skipped(format!("`{rel}` is not a new test file"), base);
        }
        if a.project.join(&rel).exists() {
            return skipped(format!("`{rel}` already exists"), base);
        }
        planned.push((rel, content.to_owned()));
    }
    if planned.is_empty() {
        return skipped("no acceptance tests were generated".to_owned(), base);
    }
    let argv: Vec<String> = answer["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if a.policy.classify("run_command", &json!({ "argv": argv })) != tools::policy::Decision::Allow
    {
        return skipped(
            format!("`{}` is not an allowed command", argv.join(" ")),
            base,
        );
    }
    for (rel, content) in &planned {
        let full = a.project.join(rel);
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&full, content) {
            return skipped(format!("`{rel}`: {e}"), base);
        }
        written.push(rel.clone());
    }

    // Red: they must fail before the task is done.
    let red = run_acceptance(a.project, &argv, a.runner).status
        != speccoder_core::verify::CheckStatus::Passed;
    let mut report = base;
    report["files"] = json!(written);
    report["argv"] = json!(argv);
    report["red"] = json!(red);
    if !red {
        report["note"] = json!(
            "the acceptance tests already pass before the attempt, so they are no evidence for it"
        );
    }
    PreparedAcceptance {
        report,
        argv: Some(argv),
        red,
    }
}

fn run_acceptance(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> speccoder_core::verify::CheckResult {
    use speccoder_core::verify::{CheckResult, CheckStatus};
    use verify::process::CommandRunner;
    let out = runner.run(&verify::process::Invocation {
        argv: argv.to_vec(),
        cwd: project.to_path_buf(),
        env: verify::process::scrubbed_env(std::env::vars()),
        timeout: std::time::Duration::from_secs(600),
        output_cap: verify::process::OUTPUT_CAP,
    });
    let status = if out.spawn_error.is_some() {
        CheckStatus::Error
    } else if out.timed_out {
        CheckStatus::TimedOut
    } else if out.exit_code == Some(0) {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };
    let tail = |b: &[u8]| {
        let start = b.len().saturating_sub(4096);
        String::from_utf8_lossy(&b[start..]).into_owned()
    };
    CheckResult {
        name: "acceptance".to_owned(),
        argv: argv.to_vec(),
        status,
        exit_code: out.exit_code,
        duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
        junit: false,
        tests: None,
        stdout_tail: tail(&out.stdout.kept),
        stderr_tail: tail(&out.stderr.kept),
        truncated: out.stdout.truncated() || out.stderr.truncated(),
        note: out.spawn_error,
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
