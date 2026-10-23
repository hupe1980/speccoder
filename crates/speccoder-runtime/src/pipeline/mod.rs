//! The run pipeline: one task ([`task`]), a feature ([`feature`]), and the
//! pieces they share.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::snapshot::{Diagnostic, ProjectSnapshot, Severity};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::commands::*;
use crate::{approvals, progress, substrate, verify, worktree};

pub mod acceptance;
pub mod commit;
pub mod feature;
pub mod full;
pub mod judge;
pub mod lanes;
pub mod outcome;
pub mod samples;
pub mod task;

pub(crate) use acceptance::*;
pub(crate) use commit::*;
pub use feature::*;
pub(crate) use judge::*;
pub(crate) use lanes::*;
pub(crate) use outcome::*;
pub(crate) use samples::*;
pub use task::*;

/// Where a project's run state lives, and its model configuration.
pub(crate) struct Setup {
    pub(crate) snapshot: Box<speccoder_core::snapshot::ProjectSnapshot>,
    pub(crate) model: substrate::model_config::ModelConfig,
    pub(crate) plane_dir: std::path::PathBuf,
    /// The project's format.
    pub(crate) format: &'static dyn crate::format::Adapter,
}

pub(crate) fn setup(
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
    preflight(&snapshot, command)?;
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
        format: crate::format::of(&snapshot),
        plane_dir: worktree::project_state_dir(state, root),
        snapshot,
        model,
    })
}

/// The format's preflight: errors of the selected unit (and, for `run`,
/// what the format's own installed tools say) stop a run or proposal before
/// any model call.
fn preflight(snapshot: &ProjectSnapshot, command: &str) -> Result<(), Box<CommandResult>> {
    if !matches!(command, "run" | "propose") {
        return Ok(());
    }
    let format = crate::format::of(snapshot);
    let mut extra = Vec::new();
    if command == "run" {
        match crate::sandbox::runner_at(Path::new(&snapshot.root)) {
            Ok((runner, _)) => extra = format.crosscheck(snapshot, &runner),
            Err(e) => extra.push(Diagnostic::new(
                Severity::Info,
                "crosscheck_skipped",
                format!("cross-check skipped: {e}"),
            )),
        }
    }
    let problems = format.blocking(snapshot).len()
        + extra
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
    if problems == 0 {
        return Ok(());
    }
    let mut shown = snapshot.clone();
    shown.diagnostics.extend(extra);
    let name = shown
        .feature
        .as_ref()
        .map(|f| f.feature.name.clone())
        .unwrap_or_default();
    Err(Box::new(
        CommandResult::new(
            command,
            CommandStatus::DiagnosticsError,
            &format!(
                "{} {name} cannot run: {problems} problem(s)",
                format.wording().unit
            ),
        )
        .with_data(CommandData::Inspect(Box::new(shown))),
    ))
}

pub(crate) fn tokio_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start the async runtime: {e}"))
}

/// Replaces every occurrence of `secret` (never shown, never journaled by speccoder).
pub(crate) fn redact(text: &str, secret: Option<&str>) -> String {
    match secret {
        Some(s) if !s.is_empty() => text.replace(s, "[redacted]"),
        _ => text.to_owned(),
    }
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
    /// Run although the feature's checklists have unchecked items; the
    /// override is recorded on every commit of the run.
    pub ignore_checklists: bool,
    /// Classes a person approved "for this run": shared by every task (and
    /// lane) of one `speccoder run`, gone with it.
    pub run_grants: Arc<std::sync::Mutex<Vec<crate::tools::policy::Grant>>>,
}

/// The format's checklist gate: an unchecked item in the feature's
/// checklists stops a run before any model call, unless a person
/// overrides it (`run --ignore-checklists`). `Ok(true)`: overridden.
pub(crate) fn checklist_gate(
    project: &Path,
    feature: Option<&str>,
    ignore: bool,
) -> Result<bool, Box<CommandResult>> {
    let format = crate::format::at(project);
    let Some(snap) = format.inspect(project, feature) else {
        return Ok(false);
    };
    let Some(f) = snap.feature.as_ref() else {
        return Ok(false);
    };
    let lists = format.checklists(Path::new(&snap.root), &f.feature.dir);
    let open: Vec<String> = lists
        .iter()
        .filter(|c| c.open > 0)
        .map(|c| format!("{} ({} of {} open)", c.file, c.open, c.total))
        .collect();
    if open.is_empty() {
        return Ok(false);
    }
    if ignore {
        return Ok(true);
    }
    Err(Box::new(
        CommandResult::new(
            "run",
            CommandStatus::NeedsHuman,
            &(format.messages().checklists)(&open.join(", ")),
        )
        .with_data(CommandData::Checklists(lists)),
    ))
}

/// One stage event of `task`; `since` (when the stage began) gives an ended
/// stage its duration.
pub(crate) fn stage(
    live: &dyn crate::progress::Progress,
    task: &str,
    stage: speccoder_core::stage::Stage,
    state: speccoder_core::stage::StageState,
    detail: Option<String>,
    since: Option<std::time::Instant>,
) {
    stage_with_tail(live, task, stage, state, detail, since, Vec::new());
}

/// [`stage`] with the last lines of what failed.
pub(crate) fn stage_with_tail(
    live: &dyn crate::progress::Progress,
    task: &str,
    stage: speccoder_core::stage::Stage,
    state: speccoder_core::stage::StageState,
    detail: Option<String>,
    since: Option<std::time::Instant>,
    tail: Vec<String>,
) {
    live.event(&crate::progress::Event::Stage {
        task: task.to_owned(),
        stage,
        state,
        detail,
        elapsed_ms: since.map(|t| u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)),
        tail,
    });
}

/// The tail of the first failing check of `report`, for a failed stage.
pub(crate) fn failing_tail(report: &speccoder_core::verify::VerifyReport) -> Vec<String> {
    report
        .checks
        .iter()
        .find(|c| c.status.is_failure())
        .map(|c| crate::progress::tail(&format!("{}\n{}", c.stdout_tail, c.stderr_tail)))
        .unwrap_or_default()
}
