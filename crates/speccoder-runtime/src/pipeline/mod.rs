//! The run pipeline: one task ([`task`]), a feature ([`feature`]), and the
//! pieces they share.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::snapshot::{Diagnostic, ProjectSnapshot, Severity};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::commands::*;
use crate::{approvals, progress, substrate, verify, worktree};

pub mod acceptance;
pub mod bug;
pub mod commit;
pub mod feature;
pub mod full;
pub mod judge;
pub mod lanes;
pub mod outcome;
pub mod samples;
pub mod task;

pub(crate) use acceptance::*;
pub(crate) use bug::{reject as bug_reject, triad_now as bug_triad_now};
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
        return Err(Box::new(CommandResult::carried(command, &inspected)));
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

/// One `speccoder run` from its fresh start to its end: `run-<unix
/// secs>-<8 hex>`. Never one attempt's agentplane run (a retry is a new one
/// of those, in the same run).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RunId(pub String);

impl RunId {
    /// A fresh id.
    #[must_use]
    pub fn fresh() -> Self {
        use sha2::{Digest as _, Sha256};
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let digest = Sha256::digest(format!(
            "{}/{}/{}/{n}",
            now.as_nanos(),
            std::process::id(),
            std::thread::current().name().unwrap_or_default()
        ));
        let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
        Self(format!("run-{}-{hex}", now.as_secs()))
    }

    /// Its text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::fresh()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl RunOptions {
    /// The run's id now.
    #[must_use]
    pub fn run(&self) -> RunId {
        self.run_id
            .lock()
            .map(|r| r.clone())
            .unwrap_or_else(|_| RunId(String::new()))
    }

    /// The run's prompts so far.
    #[must_use]
    pub fn asked_now(&self) -> u32 {
        self.asked.lock().map(|a| *a).unwrap_or(0)
    }
}

/// How a run behaves beyond its configuration.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// At most this many tasks in a feature run.
    pub max_tasks: Option<usize>,
    /// Who to ask inline when a call needs approval; without one,
    /// the attempt stops and `speccoder task approve` decides.
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
    /// Set a task's held or interrupted attempt aside and write its
    /// acceptance tests anew (`run --task T --fresh-acceptance`).
    pub fresh_acceptance: bool,
    /// Attempt although a check cannot run on the untouched code
    /// (`run --allow-failing-baseline`); otherwise the run stops before any
    /// model call (`CHECK-BROKEN`).
    pub allow_failing_baseline: bool,
    /// Objects a person approved "for this run": shared by every task (and
    /// lane) of one `speccoder run`, gone with it.
    pub run_grants: Arc<std::sync::Mutex<Vec<crate::tools::policy::Object>>>,
    /// The run: one per `speccoder run` from its fresh start; an
    /// invocation that resumes a suspended attempt adopts that attempt's.
    /// Grants, the asking count and the batch are keyed by it.
    pub run_id: Arc<std::sync::Mutex<RunId>>,
    /// Prompts the run has shown a person (`[approvals] budget` caps them).
    pub asked: Arc<std::sync::Mutex<u32>>,
    /// Run in the person's checkout on `speccoder/<feature>` instead of a
    /// worktree (`run --in-place`).
    pub in_place: bool,
    /// The in-place session the runtime opened for this run.
    pub session: Option<Arc<std::sync::Mutex<crate::in_place::Session>>>,
    /// Run a bug of the format's own bug structure (no task list names it).
    pub bug_home: Option<Arc<bug::NativeBug>>,
    /// How the run asks for a person (spec 093): `Ask` as today, or
    /// `Autopilot` — nothing asks, a call that would is refused for the run
    /// unless a grant in `run_grants` (the ones the person ticked) covers it.
    pub mode: speccoder_core::oracle::RunMode,
}

/// Where a task runs, as `worktree` reports it: speccoder's worktree, or in
/// place the person's checkout on `speccoder/<feature>`.
pub(crate) fn workspace(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    options: &RunOptions,
) -> CommandResult {
    let Some(session) = options
        .session
        .as_ref()
        .and_then(|s| s.lock().ok().map(|s| s.clone()))
    else {
        if let Some(e) = crate::in_place::session_elsewhere(path, feature, state) {
            return e.result("run");
        }
        return worktree_with(path, feature, state);
    };
    CommandResult::new("worktree", CommandStatus::Ok, "in place").with_data(CommandData::Worktree(
        speccoder_core::WorktreeReport {
            feature: session.feature.clone(),
            path: Some(session.checkout.display().to_string()),
            project_dir: Some(session.project.display().to_string()),
            branch: Some(session.branch.clone()),
            base: Some(session.tip.clone()),
            created: false,
            refused_paths: Vec::new(),
        },
    ))
}

/// Runs `f` inside an in-place session when one is asked for and none is
/// open yet: checks and switches the checkout first, switches it back after
/// — on every way out.
pub(crate) fn in_session(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    options: &RunOptions,
    f: impl FnOnce(&RunOptions) -> CommandResult,
) -> CommandResult {
    if !options.in_place || options.session.is_some() {
        return f(options);
    }
    if options.parallel > 1 {
        return crate::in_place::InPlaceError {
            code: "IN-PLACE-PARALLEL",
            message: "an in-place run has one checkout; it cannot run tasks in parallel".to_owned(),
            paths: Vec::new(),
            fix: vec!["speccoder run --in-place   (without --parallel)".to_owned()],
        }
        .result(command);
    }
    let opened = match crate::format::open(path, feature) {
        Ok(o) => o,
        Err(r) => return CommandResult::new(command, r.status, &r.message),
    };
    let Some(f_name) = opened
        .snapshot
        .feature
        .as_ref()
        .map(|f| f.feature.name.clone())
    else {
        return CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            "no feature selected; pass --feature",
        );
    };
    let root = std::path::PathBuf::from(&opened.snapshot.root);
    let plane_dir = worktree::project_state_dir(state, &root);
    let policy = crate::tools::policy::Policy::new(&[], &[]).rules(opened.adapter.rules());
    let mut session = match crate::in_place::Session::begin(&root, &plane_dir, &f_name, &policy) {
        Ok(s) => s,
        Err(e) => return e.result(command),
    };
    // Ticks a killed run committed but did not write, read on the branch.
    let format = crate::format::at(&root);
    if let Some(tasks) = format
        .inspect(&root, Some(&f_name))
        .and_then(|snap| snap.feature.and_then(|f| f.tasks))
    {
        let file = session.project.join(&tasks.path);
        session.reconcile(&plane_dir, &file, &tasks, |file, task, hash| {
            format.tick(file, task, hash)
        });
    }
    let original = session.original.describe();
    if let Some(p) = &options.progress {
        p.event(&progress::Event::Notice {
            text: format!(
                "in place: on {} in {}; returns to {original} at the end — do not use this checkout until then",
                session.branch,
                session.checkout.display()
            ),
        });
        for note in &session.notes {
            p.event(&progress::Event::Notice { text: note.clone() });
        }
    }
    let shared = Arc::new(std::sync::Mutex::new(session));
    let inner = RunOptions {
        session: Some(Arc::clone(&shared)),
        ..options.clone()
    };
    let res = f(&inner);
    let session = shared.lock().map(|s| s.clone());
    // An attempt waiting for a person keeps the checkout as it left it: the
    // session stays, and the next `run --in-place` takes it over and resumes.
    if let Some(task) = waiting_task(&res) {
        let notes: String = session
            .as_ref()
            .map(|s| s.notes.iter().map(|n| format!("\n{n}")).collect())
            .unwrap_or_default();
        let message = format!(
            "{}{notes}\nthe checkout stays on {} while {task} waits: decide it (`speccoder task approve {task}` or `reject`), then `speccoder run --in-place` continues it — or `speccoder run --in-place --return` gives the checkout back",
            res.message(),
            session
                .as_ref()
                .map(|s| s.branch.as_str())
                .unwrap_or("the speccoder branch"),
        );
        let status = res.status();
        return res.reworded(status, &message);
    }
    let (back, notes) = match session {
        Ok(s) => (s.finish(&plane_dir), s.notes),
        Err(_) => (Ok(()), Vec::new()),
    };
    let told: String = notes.iter().map(|n| format!("\n{n}")).collect();
    match back {
        Ok(()) => {
            let message = format!("{} — back on {original}{told}", res.message());
            let status = res.status();
            res.reworded(status, &message)
        }
        Err(e) => {
            let message = format!("{}{told}\n{}", res.message(), e.line());
            res.reworded(CommandStatus::NeedsHuman, &message)
                .with_code(e.code)
                .with_fix(e.fix.clone())
        }
    }
}

/// The task whose attempt the result left waiting for a person, if any.
fn waiting_task(res: &CommandResult) -> Option<String> {
    let waits = |r: &speccoder_core::RunReport| {
        r.pending
            .as_ref()
            .is_some_and(|p| p["waiting"] == true)
            .then(|| r.task.clone())
    };
    match res.data() {
        Some(CommandData::Run(r)) => waits(r),
        Some(CommandData::Feature(f)) => f.attempts.last().and_then(waits),
        _ => None,
    }
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
