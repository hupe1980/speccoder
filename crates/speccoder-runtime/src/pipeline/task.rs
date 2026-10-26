//! `speccoder run --task`: one task from baseline to tick.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{
    approvals, attempts, external, git, integrity, mcp, mutation, progress, resume, sandbox,
    substrate, tools, verify, worktree,
};

/// `speccoder run`: one attempt at a ready task in its worktree, judged by
/// the verifier. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn run(path: &Path, feature: Option<&str>, task: &str, options: &RunOptions) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
        return CommandResult::new(
            "run",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR");
    };
    let model = match setup(path, feature, &state, "run") {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return substrate::model_config::refused("run", &e),
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

/// [`run_with`] with explicit [`RunOptions`].
#[must_use]
pub fn run_with_options(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    run_task(path, feature, task, state, provider, secret, options)
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

/// Runs a bug of the format's own bug structure: a unit no task list
/// names, its task `bug:<slug>`, ticked nowhere.
#[must_use]
pub fn run_native_bug(
    root: &Path,
    bug: &speccoder_core::bug::BugReport,
    state: &Path,
    provider: Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let options = RunOptions {
        bug_home: Some(Arc::new(super::bug::NativeBug::of(bug))),
        ..options.clone()
    };
    run_task(root, None, &bug.task, state, provider, secret, &options)
}

#[allow(clippy::too_many_lines)] // one linear procedure, kept in reading order
pub(crate) fn run_task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    use speccoder_core::RunReport;
    // In place: the person's checkout, switched and switched back around the task.
    if options.in_place && options.session.is_none() {
        return in_session("run", path, feature, state, options, |o| {
            run_task(path, feature, task, state, Arc::clone(&provider), secret, o)
        });
    }
    let approver = options.approver.clone();
    use speccoder_core::snapshot::TaskState;

    let refuse =
        |status: CommandStatus, m: String| CommandResult::new("run", status, &redact(&m, secret));

    // 1. The feature, as selected in the user's checkout.
    let s = match setup(path, feature, state, "run") {
        Ok(s) => s,
        Err(r) => return *r,
    };
    // A bug in the format's own bug structure: a unit no task list names.
    let native = options.bug_home.clone();
    let feature_name = match (&native, s.snapshot.feature.as_ref()) {
        (Some(n), _) => n.unit.clone(),
        (None, Some(f)) => f.feature.name.clone(),
        (None, None) => {
            return refuse(
                CommandStatus::SelectionNeeded,
                "no feature selected; pass --feature".to_owned(),
            );
        }
    };
    // A lane keeps its own journal and live state; a person's
    // decisions are the feature's.
    let plane_dir = match &options.lane {
        Some(lane) => s.plane_dir.join("lanes").join(lane),
        None => s.plane_dir.clone(),
    };
    // Live progress: live.json for `speccoder report --live`, and the printer.
    let live: Arc<dyn progress::Progress> = Arc::new(progress::Live::new(
        &plane_dir,
        s.model.prices,
        options.progress.clone(),
    ));

    // 2. The worktree.
    let wt = match &native {
        Some(n) => crate::commands::worktree_of(&s.snapshot, &n.unit, &n.dir, state),
        None => workspace(path, Some(&feature_name), state, options),
    };
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::carried("run", &wt);
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::carried("run", &wt).with_data(CommandData::Worktree(w.clone()));
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path".to_owned());
    };
    let (wt_path, project_dir) = match &options.lane {
        None => (wt_path, project_dir),
        Some(lane) => match lane_worktree(path, state, &feature_name, lane, &wt_path, &project_dir)
        {
            Ok(paths) => paths,
            Err(e) => return refuse(CommandStatus::GitPrecondition, e),
        },
    };
    let project = std::path::PathBuf::from(&project_dir);
    let wt_root = std::path::PathBuf::from(&wt_path);
    let tmp = plane_dir.join("tmp");

    // 2a. A task waiting for a person holds the worktree with its attempt:
    //     another task starts once it is decided and has resumed.
    if options.lane.is_none()
        && let Some((other, covers)) =
            crate::approvals::waiting_elsewhere(&project, &plane_dir, &feature_name, task)
    {
        return refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{other} waits for a person ({covers}) and its attempt holds the worktree: decide it (`speccoder task approve {other}` or `reject`), then `speccoder run --task {other}`"
            ),
        );
    }
    // 2a'. So does an attempt held for a ruling on its tests.
    if options.lane.is_none()
        && let Some(other) = crate::strength::held_elsewhere(&plane_dir, &feature_name, task)
    {
        return refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{other}'s acceptance tests reject an implementation the spec appears to allow, and its attempt holds the worktree: `speccoder task rule {other} --tests-right --reason …` or `speccoder run --task {other} --fresh-acceptance`"
            ),
        )
        .with_code("TESTS-OVER-STRICT");
    }
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
    // A fresh start for the task's tests: its held (or interrupted) attempt
    // is set aside below like any leftover, and new tests are written.
    if options.fresh_acceptance && options.lane.is_none() {
        let _ = std::fs::remove_file(&marker_path);
        crate::strength::release(&crate::strength::dir(&s.plane_dir, &feature_name, task));
    }
    let head_now = git::Git::at(&wt_root)
        .line(&["rev-parse", "HEAD"])
        .unwrap_or_default();
    let interrupted = resume::load(&marker_path)
        .filter(|m| m.head == head_now && options.lane.is_none() && !head_now.is_empty());
    // A resumed attempt continues its run: this invocation adopts the
    // run's id, its asking count and its grants.
    if let Some(m) = &interrupted {
        if let (Some(id), Ok(mut run)) = (&m.run_id, options.run_id.lock()) {
            *run = crate::pipeline::RunId(id.clone());
        }
        if let Ok(mut asked) = options.asked.lock() {
            *asked = (*asked).max(m.asked);
        }
        if let Ok(mut g) = options.run_grants.lock() {
            for o in &m.run_grants {
                if !g.contains(o) {
                    g.push(o.clone());
                }
            }
        }
    }
    let marker_guard = resume::Guard::new(marker_path.clone());
    // Where the interrupted attempt was, from the live record it left
    // (read before this run overwrites it).
    let resumed_from = interrupted.as_ref().map(|_| {
        std::fs::read_to_string(plane_dir.join(progress::FILE))
            .ok()
            .and_then(|t| serde_json::from_str::<progress::LiveState>(&t).ok())
            .filter(|l| l.task == task)
            .map_or((None, None), |l| (l.stage, l.last_call))
    });
    let mut set_aside = None;
    // In place, a dirty checkout is the person's: the session guard refuses it.
    if interrupted.is_none()
        && commits_per_task
        && options.session.is_none()
        && worktree::require_clean(&wt_root).is_err()
    {
        // What the checks leave behind — build output not ignored, a
        // lockfile never committed — is no one's edit: setting it aside
        // would store it in git on every run. The setup is fixed once.
        let left = worktree::leftovers(&wt_root);
        if !left.is_empty() {
            let mut what = Vec::new();
            if !left.commit.is_empty() {
                what.push(format!("{} is not committed", left.commit.join(", ")));
            }
            if !left.ignore.is_empty() {
                what.push(format!("{} is not git-ignored", left.ignore.join(", ")));
            }
            let checkout = std::path::Path::new(&s.snapshot.root);
            let remedy = crate::code::setup_remedy(checkout, Some(&feature_name));
            let command = remedy
                .as_ref()
                .map_or_else(|| left.fix(checkout, &wt_root, &[]), |r| r.command.clone());
            return refuse(
                CommandStatus::GitPrecondition,
                format!(
                    "{} — the checks write it on every run, so each run would find it in the worktree and set it aside",
                    what.join(" and "),
                ),
            )
            .with_code("SETUP-INCOMPLETE")
            .with_fix(vec!["speccoder fix".to_owned(), command])
            .with_remedy(remedy);
        }
        match set_aside_leftovers(&wt_root, &tmp, &feature_name, task) {
            Ok(reference) => set_aside = Some(reference),
            Err(e) => return refuse(CommandStatus::GitPrecondition, e),
        }
    }

    // 3. Spec, checks and policy as committed in the worktree.
    let format = crate::format::at(&project);
    let Some(snap) = format.inspect(
        &project,
        if native.is_some() {
            None
        } else {
            Some(&feature_name)
        },
    ) else {
        return refuse(
            CommandStatus::NotAProject,
            format.messages().no_project_in_worktree.to_owned(),
        );
    };
    if native.is_some()
        && crate::bug::state::load(&s.plane_dir, &feature_name, task)
            .is_some_and(|b| b.outcome == speccoder_core::bug::Outcome::Fixed)
    {
        return refuse(
            CommandStatus::TaskNotReady,
            format!("{task} is already fixed"),
        )
        .with_code("TASK-DONE");
    }
    // The task must exist and be ready in the worktree, where progress
    // (ticks committed on speccoder/<feature>) lives.
    if native.is_none() {
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
                )
                .with_code("TASK-DONE");
            }
            TaskState::Blocked => {
                return refuse(
                    CommandStatus::TaskNotReady,
                    format!("{task} is blocked by {}", st.blocked_by.join(", ")),
                )
                .with_code("TASK-BLOCKED");
            }
            TaskState::Ready => {}
        }
    }
    // Unfinished checklists stop the task before any model call (the
    // committed checklists in the worktree are what counts).
    let checklists_overridden = if native.is_some() {
        false
    } else {
        match checklist_gate(&project, Some(&feature_name), options.ignore_checklists) {
            Ok(overridden) => overridden,
            Err(stop) => return *stop,
        }
    };
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

    let bundle = match native
        .as_ref()
        .map(|n| n.bundle(&project))
        .map_or_else(|| format.bundle(&snap, task), Ok)
    {
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
        )
        .with_code("CONFIG-MISSING");
    };
    let config_text =
        std::fs::read_to_string(config_root.join(verify::config::FILE)).unwrap_or_default();
    let run_config = match substrate::run_config::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };
    // `[review] required`: the spec as committed in the worktree must be
    // approved, every requirement on its current wording.
    if run_config.review_required
        && let Some(f) = snap.feature.as_ref()
        && let Some(t) = crate::review::totals_of(&project, format, &feature_name, &f.feature.dir)
        && !t.complete()
    {
        return refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{feature_name}'s spec is not approved yet ({} approved; {} to review, {} with changes requested, {} changed since, {} open question(s); committed review only) and [review] required is on",
                t.approved, t.open, t.changes_requested, t.lapsed, t.questions
            ),
        )
        .with_code("REVIEW-PENDING");
    }
    let executor = match external::parse(&config_text) {
        Ok(e) => e,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };
    // Every command this task starts runs confined to the worktree.
    let sandbox_config = match sandbox::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };
    let (runner, sandbox_notice) = match sandbox::runner(&wt_root, &sandbox_config) {
        Ok(r) => r,
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };
    let mutation_config = match mutation::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };
    let check_argvs = verify::check_argvs(&config_root);
    let policy = tools::policy::Policy::new(&check_argvs, &run_config.allow)
        .with_prefixes(run_config.allow_prefix.clone())
        .rules(format.rules());
    // In place: the model's view of the checkout confined (git that moves
    // it denied, ignored files hidden, `.git` read-only to commands).
    let (policy, runner, carve_notice) = match options.session.as_ref().and_then(|s| s.lock().ok())
    {
        Some(s) => s.confine(policy, runner),
        None => (policy, runner, None),
    };
    let sandbox_notice = sandbox_notice.or(carve_notice);
    // In place: the checkout as speccoder left it, the intent as the run
    // started with it — before the task, and again before the tick.
    if let Some(e) = options.session.as_ref().and_then(|s| {
        s.lock().ok().and_then(|s| {
            // A resumed attempt's writes are in the checkout already.
            if interrupted.is_some() {
                s.check_before_tick(&policy).err()
            } else {
                s.check(&policy).err()
            }
        })
    }) {
        return e.result("run");
    }

    let wt_root_canonical = std::fs::canonicalize(&wt_root)
        .unwrap_or_else(|_| wt_root.clone())
        .display()
        .to_string();

    // A person's approvals (of this run only: a grant ends with its run)
    // and rejections for this task.
    let this_run = options.run();
    let (decisions, task_grants) = match approvals::Decisions::load(&s.plane_dir) {
        Ok(mut d) => {
            // Decisions on a call a lane stopped belong to the run that
            // now attempts the task.
            if d.claim(&feature_name, task, this_run.as_str())
                && let Err(e) = d.save(&s.plane_dir)
            {
                return refuse(CommandStatus::Error, e);
            }
            (
                d.for_task(&feature_name, task),
                d.for_run(&feature_name, task, this_run.as_str()),
            )
        }
        Err(e) => return refuse(CommandStatus::ConfigError, e),
    };

    if let Some((stage, last_call)) = &resumed_from {
        live.event(&progress::Event::Resumed {
            task: task.to_owned(),
            stage: *stage,
            last_call: last_call.clone(),
        });
    }
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
            run: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: None,
        });
        refuse(status, m)
    };
    // 4. Baseline: no verdict without a judge.
    let began = std::time::Instant::now();
    stage(
        &*live,
        task,
        Stage::Baseline,
        StageState::Started,
        None,
        None,
    );
    let baseline = match verify::run_checks(&project, Some(verify::config::Tier::Fast), &runner) {
        Ok(b) => b,
        Err(e) => {
            stage(
                &*live,
                task,
                Stage::Baseline,
                StageState::Failed,
                None,
                Some(began),
            );
            return fail(CommandStatus::ConfigError, format!("baseline: {e}"));
        }
    };
    // A check that cannot run on the untouched code — it failed without
    // running a test: not found, a build or configuration error, a file the
    // worktree lacks because it was never committed — can never judge the
    // task. Stop before any model call rather than spend attempts on it.
    if !options.allow_failing_baseline
        && let Some((c, why)) = baseline
            .checks
            .iter()
            .find_map(|c| broken(c).map(|w| (c, w)))
    {
        stage(
            &*live,
            task,
            Stage::Baseline,
            StageState::Failed,
            Some(format!("`{}` cannot run", c.name)),
            Some(began),
        );
        // Configuration the checkout never committed is the likely cause,
        // and one speccoder can fix.
        let checkout = std::path::Path::new(&s.snapshot.root);
        if let Some(remedy) = crate::code::setup_remedy(checkout, Some(&feature_name)) {
            return fail(
                CommandStatus::GitPrecondition,
                format!(
                    "check `{}` cannot run in the worktree: your checkout has not committed its setup — {why}",
                    c.name
                ),
            )
            .with_code("SETUP-INCOMPLETE")
            .with_fix(vec!["speccoder fix".to_owned(), remedy.command.clone()])
            .with_remedy(Some(remedy));
        }
        return fail(
            CommandStatus::ConfigError,
            // A cause speccoder recognised comes with its fix; else the
            // general one.
            if why.contains("git add") || why.contains("--allow-failing-baseline") {
                format!(
                    "check `{}` cannot run on the untouched code, so it could never judge {task}: {why}",
                    c.name
                )
            } else {
                format!(
                    "check `{}` cannot run on the untouched code, so it could never judge {task}: {why} — fix the check (a file it needs may be uncommitted: `git status`), or run with --allow-failing-baseline",
                    c.name
                )
            },
        )
        .with_code("CHECK-BROKEN");
    }
    // Failing tests at the baseline are reported, not blamed: the stage held.
    let failing = baseline
        .checks
        .iter()
        .filter(|c| c.status.is_failure())
        .count();
    stage(
        &*live,
        task,
        Stage::Baseline,
        StageState::Passed,
        (failing > 0).then(|| format!("{failing} check(s) already failing")),
        Some(began),
    );

    let rt = match tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return fail(CommandStatus::RunFailed, e),
    };
    let bundle_value = match serde_json::to_value(&bundle) {
        Ok(v) => v,
        Err(e) => return fail(CommandStatus::Error, e.to_string()),
    };
    // The repository's instructions for agents, from the tree the
    // attempt starts in: for speccoder's own calls (the attempt, the test
    // authors); an external agent reads its own files.
    let instructions = crate::instructions::collect(
        &wt_root,
        &project,
        &speccoder_core::scope::named_paths(&bundle.task.title),
        &run_config.instructions,
    );
    let attempt_instructions: &[crate::instructions::Instruction] = if executor.is_none() {
        &instructions
    } else {
        &[]
    };

    // MCP servers, started in the worktree for this task's attempts.
    let servers = match mcp::parse(&config_text) {
        Ok(s) => s,
        Err(e) => {
            return fail(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID");
        }
    };
    let mut connected = if servers.is_empty() {
        mcp::Connected::default()
    } else {
        live.event(&progress::Event::Notice {
            text: format!("starting {} MCP server(s)", servers.len()),
        });
        rt.block_on(mcp::connect(&servers, &project, &runner))
    };
    // An allowed tool its server changed since review needs approval again.
    let approved_tools: Vec<String> = task_grants
        .iter()
        .filter_map(|g| g.tool_name().map(str::to_owned))
        .collect();
    let pinned = mcp::pin_reviewed(&s.plane_dir, &mut connected.tools, &approved_tools);
    connected.notices.extend(pinned);

    // One plane for the whole task. Its tool server enforces denials; the
    // frozen-test approvals are the skill's, from its journaled input.
    let tools_server = match tools::WorkspaceTools::new(&wt_root, policy.clone()) {
        Ok(t) => std::sync::Arc::new(t.with_runner(runner.clone())),
        Err(e) => return fail(CommandStatus::Error, e.to_string()),
    };
    // Calls past their deadline are denied as expired before anything resumes.
    let _ = crate::approvals::expire_due(&plane_dir, agentplane::core::Timestamp::now_utc());
    // What a person is shown about the sandbox, for every call they decide.
    let present = crate::approvals::PresentContext {
        root: None,
        sandbox: runner.is_sandboxed().then(|| {
            if cfg!(target_os = "macos") {
                "Seatbelt".to_owned()
            } else {
                "Landlock+seccomp".to_owned()
            }
        }),
        network: match sandbox_config.network {
            sandbox::Network::Off => "none",
            sandbox::Network::Loopback => "loopback",
            sandbox::Network::On => "unrestricted",
        }
        .to_owned(),
        file_text: None,
    };
    let plane = match rt.block_on(async {
        substrate::Plane::open_with(
            &plane_dir,
            &s.model,
            provider,
            Some(tools_server),
            substrate::Hooks {
                progress: Some(Arc::clone(&live)),
                mcp: connected.clients.clone(),
                mcp_tools: connected.tools.clone(),
                format: Some(format),
                present: present.clone(),
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
        .chain(resumed_from.as_ref().map(|(stage, last_call)| {
            format!(
                "Resuming — {task} was interrupted during {}{}; finished calls will not repeat.",
                stage.map_or("a run", speccoder_core::stage::Stage::name),
                last_call
                    .as_ref()
                    .map(|c| format!(" ({c})"))
                    .unwrap_or_default()
            )
        }))
        .collect();

    // A Convergence task is the converge model's claim that a gap exists;
    // it changes no code until a failing test demonstrates the gap.
    let demand_red = run_config.acceptance && format.is_convergence(&bundle.task.phase);
    // Where this run started: an attempt that does not pass is undone to here.
    // Where an attempt that does not pass is undone to: the tree the task
    // started from. A resumed attempt (interrupted, or waiting for a
    // person) left its own writes in the tree, so its start is the branch
    // tip it began at — a task always starts from a clean tip.
    let start = match &interrupted {
        Some(_) => match git::Git::at(&wt_root).line(&["rev-parse", "HEAD^{tree}"]) {
            Ok(tree) => worktree::Checkpoint(tree),
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        },
        None => match worktree::checkpoint(&wt_root, &tmp) {
            Ok(c) => c,
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        },
    };

    let began = std::time::Instant::now();
    // 4b. Acceptance tests derived from the spec, written before the
    //     attempt so they are frozen, and required to be red now.
    // 4a. A bug task: reproduce (and the person's confirmation) before
    //     anything else; once confirmed, its reproductions are the visible set.
    let bug_ctx = super::bug::Ctx {
        rt: &rt,
        plane: &plane,
        project: &project,
        wt_root: &wt_root,
        wt_path: &wt_path,
        tmp: &tmp,
        policy: &policy,
        runner: &runner,
        state: &s.plane_dir,
        feature: &feature_name,
        task,
        bundle: &bundle_value,
        instructions: &instructions,
        live: &*live,
        approver: approver.as_deref(),
        held_out: run_config.held_out,
        max_tries: run_config.reproduce_tries,
        start: &start,
    };
    let mut bug_fix = match super::bug::phase(&s.plane_dir, &feature_name, task, &project, format) {
        Err(e) => return fail(CommandStatus::NeedsHuman, e),
        Ok(None) => None,
        Ok(Some(super::bug::Phase::Fix(f))) => Some(*f),
        Ok(Some(super::bug::Phase::Reproduce(b))) if interrupted.is_none() => {
            match super::bug::reproduce(&bug_ctx, *b) {
                Ok(f) => Some(f),
                Err(stop) => return *stop,
            }
        }
        Ok(Some(super::bug::Phase::Reproduce(_))) => None,
    };
    let bug_candidate = bug_fix.as_ref().and_then(|f| {
        f.bug.candidate.clone().and_then(|c| {
            crate::bug::state::extra(&s.plane_dir, &f.bug.unit, &f.bug.task)
                .candidate_patch
                .map(|d| (d, c))
        })
    });
    let scenarios = match &bug_fix {
        Some(f) => super::bug::scenarios(&f.bug),
        None => format.scenarios(&snap, task),
    };
    let mut acceptance = if let Some(prepared) = interrupted
        .as_ref()
        .and_then(|m| m.acceptance.clone())
        .and_then(|a| serde_json::from_value::<PreparedAcceptance>(a).ok())
    {
        // The interrupted attempt's tests, already in the worktree. A bug
        // task's held-out set is the one its pre-frozen record seals now: a
        // special case's regeneration re-sealed it after the marker was kept.
        let mut prepared = prepared;
        if let Some(sealed) = bug_fix
            .as_ref()
            .and_then(|f| f.record.held_out_sealed.as_ref())
        {
            prepared.held_out = Some(sealed.manifest_sha256.clone());
            prepared.report["held_out"] = serde_json::json!({
                "state": "on",
                "manifest": format!("sha256:{}", sealed.manifest_sha256),
                "kept": sealed.count,
            });
        }
        Some(prepared)
    } else if let Some(f) = &bug_fix {
        stage(&*live, task, Stage::Red, StageState::Started, None, None);
        match super::bug::reuse(&bug_ctx, f) {
            Ok(a) => Some(a),
            Err(e) => {
                let _ = worktree::restore(&wt_root, &tmp, &start);
                stage(
                    &*live,
                    task,
                    Stage::Red,
                    StageState::Failed,
                    Some(e.clone()),
                    Some(began),
                );
                return fail(CommandStatus::NeedsHuman, e);
            }
        }
    } else if run_config.acceptance && format.acceptance_testable(&bundle) {
        stage(&*live, task, Stage::Red, StageState::Started, None, None);
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
            scenarios: &scenarios,
            instructions: &instructions,
            runner: &runner,
        }))
    } else {
        None
    };
    // 4c. The held-out set: written after a red visible set, kept in
    //     the state directory, never shown to the implementer.
    let held_out_store = match &bug_fix {
        Some(f) => crate::held_out::sealed_dir(&s.plane_dir, &f.bug.unit, &f.bug.requirement),
        None => crate::held_out::dir(&s.plane_dir, &feature_name, task),
    };
    if let Some(a) = acceptance
        .as_mut()
        .filter(|a| a.report.get("held_out").is_none())
    {
        if !run_config.held_out {
            a.report["held_out"] = serde_json::json!({ "state": "off" });
        } else if a.argv.is_none() {
            a.report["held_out"] =
                serde_json::json!({ "state": "absent", "reason": "no_visible_tests" });
        } else if !a.red {
            a.report["held_out"] = serde_json::json!({ "state": "absent", "reason": "not_red" });
        } else if scenarios.is_empty() {
            a.report["held_out"] =
                serde_json::json!({ "state": "absent", "reason": "no_scenarios" });
        } else {
            let (held, sha) = prepare_held_out(
                &AcceptanceSetup {
                    rt: &rt,
                    plane: &plane,
                    project: &project,
                    worktree: &wt_root,
                    tmp: &tmp,
                    policy: &policy,
                    feature: &feature_name,
                    task,
                    bundle: bundle_value.clone(),
                    scenarios: &scenarios,
                    instructions: &instructions,
                    runner: &runner,
                },
                a,
                &scenarios,
                &held_out_store,
                story_checkpoint(
                    snap.feature.as_ref().and_then(|f| f.tasks.as_ref()),
                    task,
                    &scenarios,
                ),
            );
            a.report["held_out"] = held;
            a.held_out = sha;
        }
    }
    if run_config.held_out && !run_config.acceptance {
        notices.push(
            "[acceptance] held_out has no effect while generate = false: no held-out tests"
                .to_owned(),
        );
    }
    match &acceptance {
        None => stage(
            &*live,
            task,
            Stage::Red,
            StageState::Skipped,
            Some("no acceptance examples".to_owned()),
            None,
        ),
        Some(a) if a.red => stage(
            &*live,
            task,
            Stage::Red,
            StageState::Passed,
            a.red_class.map(class_word),
            Some(began),
        ),
        Some(a) => stage(
            &*live,
            task,
            Stage::Red,
            StageState::Failed,
            Some(a.red_class.map_or_else(
                || "not demonstrated".to_owned(),
                |c| format!("not demonstrated ({})", class_word(c)),
            )),
            Some(began),
        ),
    }

    // The author call itself failed: without its tests the task could only
    // be ticked on the regression checks, which would look like "the spec
    // has no examples". A person decides; nothing was attempted.
    if let Some(why) = acceptance
        .as_ref()
        .and_then(|a| a.generation_failed.clone())
    {
        let reason = format!(
            "the acceptance tests could not be written ({why}); nothing was attempted — run again once the model answers, or set [acceptance] generate = false to rely on the checks alone"
        );
        let _ = worktree::restore(&wt_root, &tmp, &start);
        let report = RunReport {
            reason: Some(reason.clone()),
            acceptance: acceptance.as_ref().map(|a| a.report.clone()),
            set_aside,
            notices: notices.clone(),
            ..bare_report(&feature_name, task, &wt_path, "acceptance_failed")
        };
        live.event(&finished_event(
            task,
            &report,
            "acceptance_failed",
            false,
            redact(&reason, secret),
        ));
        return CommandResult::new("run", CommandStatus::NeedsHuman, &redact(&reason, secret))
            .with_code("ACCEPTANCE-UNWRITTEN")
            .with_data(CommandData::Run(Box::new(report)));
    }

    if demand_red && !acceptance.as_ref().is_some_and(|a| a.red) {
        let why = undemonstrated_why(acceptance.as_ref());
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
            reason: Some(reason.clone()),
            acceptance: acceptance.as_ref().map(|a| a.report.clone()),
            saved_attempt: saved,
            set_aside,
            notices: notices.clone(),
            ..bare_report(&feature_name, task, &wt_path, "not_demonstrated")
        };
        live.event(&finished_event(
            task,
            &report,
            "not_demonstrated",
            false,
            redact(&reason, secret),
        ));
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
    // Each judged attempt's held-out outcome, across samples and retries.
    let mut held_out_log: Vec<serde_json::Value> = Vec::new();
    // A special case that survived a bug task's reproductions.
    let mut bug_weak: Option<speccoder_core::strength::Permitted> = None;
    let (mut report, attempt_status) = loop {
        let mut earlier: Vec<speccoder_core::EarlierAttempt> = Vec::new();
        let mut digest: Option<serde_json::Value> = None;
        // A resumed attempt continues at its own number: starting over at 1
        // would admit a fresh run and repeat the model's work.
        let mut attempt_no = interrupted
            .as_ref()
            .filter(|m| m.sample == samples.len())
            .map_or(1, |m| m.attempt.max(1));
        let (mut report, attempt_status) = loop {
            // In place: what git ignored at the session's start.
            let hidden: Vec<String> = options
                .session
                .as_ref()
                .and_then(|s| s.lock().ok().map(|s| s.hidden.clone()))
                .unwrap_or_default();
            // The task's grants of this run as they stand now (an earlier
            // attempt's "for the task" answers included): journaled.
            let task_grants: Vec<crate::tools::policy::Object> =
                approvals::Decisions::load(&s.plane_dir)
                    .map(|d| d.for_run(&feature_name, task, this_run.as_str()))
                    .unwrap_or_else(|_| task_grants.clone());
            // This run's grants as they stand now: journaled with the input.
            let run_approved: Vec<crate::tools::policy::Object> = options
                .run_grants
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            // Everything the attempt's decisions depend on is in its journaled input.
            let input = substrate::attempt::input(
                bundle_value.clone(),
                &substrate::attempt::Settings {
                    checks: &check_argvs,
                    allow: &run_config.allow,
                    allow_prefix: &run_config.allow_prefix,
                    max_turns: run_config.max_turns,
                    max_tool_calls: run_config.max_tool_calls,
                    reasoning: s.model.reasoning_summary,
                    frozen: &frozen,
                    approved: &task_grants,
                    rejected: &decisions.rejected,
                    run_approved: &run_approved,
                    nudges: &decisions.nudges,
                    mcp: &connected.tools,
                    root: &wt_root_canonical,
                    frozen_additions: run_config.frozen_additions,
                    // Lanes stop (their worktree is merged or dropped with
                    // the group); every other attempt waits for a person.
                    asks: if options.lane.is_some() {
                        "stop"
                    } else {
                        "tasks"
                    },
                    wait_secs: run_config.approval_wait.as_secs(),
                    network_allow: &sandbox_config.network_allow,
                    network_allow_prefix: &sandbox_config.network_allow_prefix,
                    run_id: this_run.as_str(),
                    asked: options.asked_now(),
                    budget: run_config.approval_budget,
                    prompts: approver.is_some() && options.lane.is_none(),
                    in_place: options.session.is_some(),
                    hidden: &hidden,
                },
                digest.as_ref(),
                attempt_instructions,
            );
            let attempt_began = std::time::Instant::now();
            let attempt_detail = if run_config.best_of > 1 {
                format!(
                    "{attempt_no}/{} · sample {}/{}",
                    run_config.max_attempts,
                    samples.len() + 1,
                    run_config.best_of
                )
            } else {
                format!("{attempt_no}/{}", run_config.max_attempts)
            };
            stage(
                &*live,
                task,
                Stage::Attempt,
                StageState::Started,
                Some(attempt_detail.clone()),
                None,
            );
            // Judge inputs as the attempt finds them, git-ignored ones
            // included (an agent loads `.claude/settings.local.json` too).
            let judged_before = integrity::JudgeSnapshot::take(&wt_root, &policy);
            // The attempt itself: speccoder's governed loop, or an external
            // agent whose result is judged from the diff alike.
            let (run_id, succeeded, mut output) = if let Some((diff, cand)) = &bug_candidate {
                let held: Vec<String> = bug_fix
                    .as_ref()
                    .and_then(|f| {
                        f.bug
                            .held_out_sealed
                            .clone()
                            .map(|h| (f.bug.unit.clone(), h))
                    })
                    .and_then(|(unit, h)| {
                        crate::held_out::open_sealed(&s.plane_dir, &unit, &h).ok()
                    })
                    .map(|(m, _)| m.files.into_iter().map(|f| f.path).collect())
                    .unwrap_or_default();
                match super::bug::apply_candidate(&bug_ctx, diff, cand, &frozen, &held) {
                    Ok(x) => x,
                    Err(stop) => return *stop,
                }
            } else if let Some(exec) = &executor {
                let allow: Vec<Vec<String>> = check_argvs
                    .iter()
                    .chain(&run_config.allow)
                    .cloned()
                    .collect();
                let prompt = external::prompt_in(format.wording(), &bundle_value, digest.as_ref());
                let withheld = acceptance
                    .as_ref()
                    .and_then(|a| a.held_out.as_ref())
                    .map(|_| held_out_store.as_path());
                if withheld.is_some()
                    && !notices
                        .iter()
                        .any(|n| n.starts_with("held-out set withheld"))
                {
                    notices.push("held-out set withheld from the external agent, not sealed: it runs with your permissions; a copied held-out literal stops the tick".to_owned());
                }
                let o = external::run(exec, &wt_root, &prompt, &allow, &tmp, withheld);
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
                        run_id: Some(this_run.to_string()),
                        asked: options.asked_now(),
                        run_grants: run_approved.clone(),
                    },
                );
                marker_guard.arm();
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
                // A call waits for a person. With one here they
                // decide now, recorded with who they are, and the attempt
                // continues; "later" or nobody leaves it waiting.
                let context = crate::approvals::PresentContext {
                    root: Some(wt_root.clone()),
                    ..present.clone()
                };
                let (outcome, waiting) = match rt.block_on(plane.decide_inline(
                    outcome,
                    approver.as_deref(),
                    &context,
                )) {
                    Ok(x) => {
                        // Always Allow: the call ran once; its exact argv
                        // joins the allow list through the one action.
                        notices.extend(crate::actions::allow_always_inline(
                            path,
                            &feature_name,
                            &x.always,
                            state,
                        ));
                        (x.outcome, x.waiting)
                    }
                    Err(e) => return fail(CommandStatus::RunFailed, e.0),
                };
                if let Some(task_row) = waiting {
                    // The attempt waits as it is: its tree and its resume
                    // marker stay, so the next run continues it.
                    marker_guard.keep();
                    let action = &task_row.justification.proposed_action;
                    let mut report = bare_report(&feature_name, task, &wt_path, "needs_approval");
                    report.run_id = outcome.run_id.to_string();
                    report.notices = notices;
                    report.tool_calls = action["log"].as_array().cloned().unwrap_or_default();
                    // The waiting call itself, as the stop recorded it before.
                    report.tool_calls.push(serde_json::json!({
                        "tool": action["tool"],
                        "arguments": action["arguments"],
                        "decision": "approval",
                        "reason": action["request"]["reason"],
                        "object": action["request"]["object"],
                        "waiting": true,
                        "call": task_row.id.to_hex(),
                    }));
                    report.pending = Some(serde_json::json!({
                        "tool": action["tool"],
                        "arguments": action["arguments"],
                        "reason": action["request"]["reason"],
                        "object": action["request"]["object"],
                        "request": action["request"],
                        "call": task_row.id.to_hex(),
                        "deadline": task_row.due_at.map(|t| t.unix_timestamp()),
                        "waiting": true,
                        "batched": action["request"]["batched"],
                    }));
                    // The prompts the attempt showed before it waits count
                    // for the run.
                    if let (Some(n), Ok(mut asked)) = (
                        action["request"]["asked"]
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok()),
                        options.asked.lock(),
                    ) {
                        *asked = (*asked).max(n);
                    }
                    report.reason = Some(format!(
                        "waiting for a person: {} — `speccoder task approve {task}` or `speccoder task reject {task} --reason …`, then `speccoder run`",
                        task_row.justification.summary.peek()
                    ));
                    let (status, message) = status_and_message(&report, "needs_approval", false);
                    live.event(&finished_event(
                        task,
                        &report,
                        "needs_approval",
                        false,
                        redact(&message, secret),
                    ));
                    return CommandResult::new("run", status, &redact(&message, secret))
                        .with_data(CommandData::Run(Box::new(report)));
                }
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
            if bug_candidate.is_some()
                && let Some(c) = bug_fix.as_mut().and_then(|f| f.bug.candidate.as_mut())
            {
                c.applied = true;
            }
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
            // The prompts the attempt showed count for the run.
            if let (Some(n), Ok(mut asked)) = (
                output["asked"].as_u64().and_then(|n| u32::try_from(n).ok()),
                options.asked.lock(),
            ) {
                *asked = n;
            }
            let approved_writes = approved_writes(&task_grants, &output["tool_calls"]);
            let findings = attempt_findings(&Audited {
                wt_root: &wt_root,
                policy: &policy,
                judged_before: &judged_before,
                raw_changes: &raw_changes,
                patch: &worktree::patch(&wt_root, &before, &after).unwrap_or_default(),
                frozen: &frozen,
                frozen_additions: run_config.frozen_additions,
                approved_writes: &approved_writes,
                command_drift: &output["integrity"],
            });
            let changes = changes_of(raw_changes);

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
            let mut attempt_status = if succeeded {
                output["status"].as_str().unwrap_or("failed").to_owned()
            } else if quarantined {
                "interrupted".to_owned()
            } else {
                "failed".to_owned()
            };
            let mut report = RunReport {
                run_id: run_id.clone(),
                model_report: output.get("report").cloned(),
                reason: output["reason"].as_str().map(str::to_owned),
                pending: output.get("pending").cloned(),
                tool_calls: output["tool_calls"].as_array().cloned().unwrap_or_default(),
                changes,
                usage: output.get("usage").cloned(),
                integrity: findings.clone(),
                acceptance: acceptance.as_ref().map(|a| a.report.clone()),
                set_aside: set_aside.clone(),
                notices: notices.clone(),
                ..bare_report(&feature_name, task, &wt_path, &attempt_status)
            };
            record_inline_approvals(
                &s.plane_dir,
                &feature_name,
                task,
                &report.tool_calls,
                &options.run_grants,
                this_run.as_str(),
            );
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

            stage(
                &*live,
                task,
                Stage::Attempt,
                if attempt_status == "completed" {
                    StageState::Passed
                } else {
                    StageState::Failed
                },
                Some(if attempt_status == "completed" {
                    attempt_detail
                } else {
                    format!("{attempt_detail} {attempt_status}")
                }),
                Some(attempt_began),
            );
            // A command changed a judge input: the attempt is over and fails,
            // not retried (integrity findings never are).
            if attempt_status == "integrity" {
                report.verdict = Some(speccoder_core::verify::Verdict::Failed {
                    reasons: findings.clone(),
                });
            }
            // 7. The verdict: only the verifier decides.
            let judged = matches!(attempt_status.as_str(), "completed" | "incomplete");
            if judged {
                let began = std::time::Instant::now();
                stage(&*live, task, Stage::Verify, StageState::Started, None, None);
                if let Err(e) = judge_checks(
                    &project,
                    &runner,
                    &baseline,
                    acceptance.as_ref(),
                    &findings,
                    &mut report,
                ) {
                    stage(
                        &*live,
                        task,
                        Stage::Verify,
                        StageState::Failed,
                        None,
                        Some(began),
                    );
                    return fail(CommandStatus::ConfigError, format!("verification: {e}"));
                }
                let checks_held = report.verification.as_ref().is_some_and(|v| v.passed);
                stage_with_tail(
                    &*live,
                    task,
                    Stage::Verify,
                    if checks_held {
                        StageState::Passed
                    } else {
                        StageState::Failed
                    },
                    None,
                    Some(began),
                    report
                        .verification
                        .as_ref()
                        .map(failing_tail)
                        .unwrap_or_default(),
                );
            }
            if judged || attempt_status == "integrity" {
                stage(
                    &*live,
                    task,
                    Stage::Audit,
                    if findings.is_empty() {
                        StageState::Passed
                    } else {
                        StageState::Failed
                    },
                    findings
                        .first()
                        .map(|f| f.trim_start_matches("integrity: ").to_owned()),
                    None,
                );
            }

            // 7.5 The held-out tests, on an attempt that passed visibly with a
            //     clean audit: a failure fails the verdict naming the scenarios
            //     only; tampering or a leak stops for a person.
            let visibly_passed = judged
                && findings.is_empty()
                && attempt_status == "completed"
                && matches!(
                    report.verdict,
                    Some(
                        speccoder_core::verify::Verdict::Verified
                            | speccoder_core::verify::Verdict::RegressionOnly
                    )
                );
            if let Some(sha) = acceptance
                .as_ref()
                .and_then(|a| a.held_out.clone())
                .filter(|_| visibly_passed)
            {
                let patch = worktree::patch(&wt_root, &before, &after).unwrap_or_default();
                let mut known = format!(
                    "{}\n{}\n{}",
                    bundle_value["story"].as_str().unwrap_or_default(),
                    bundle_value["plan"].as_str().unwrap_or_default(),
                    bundle_value["constitution"].as_str().unwrap_or_default()
                );
                for p in report
                    .acceptance
                    .as_ref()
                    .and_then(|a| a["files"].as_array().cloned())
                    .unwrap_or_default()
                {
                    if let Some(p) = p.as_str() {
                        known.push_str(
                            &std::fs::read_to_string(project.join(p)).unwrap_or_default(),
                        );
                    }
                }
                match judge_held_out(
                    &held_out_store,
                    &sha,
                    &scenarios,
                    &known,
                    &wt_root,
                    &project,
                    &tmp,
                    &before,
                    &patch,
                    &runner,
                    &policy,
                ) {
                    Ok(j) => {
                        let failed_ids: Vec<String> =
                            j.failed.iter().map(|s| s.id.clone()).collect();
                        held_out_log.push(serde_json::json!({
                            "attempt": held_out_log.len() + 1,
                            "outcome": if failed_ids.is_empty() { "passed" } else { "failed" },
                            "scenarios": failed_ids,
                        }));
                        if let Some(v) = report.verdict.take() {
                            let mut v = speccoder_core::verify::with_held_out(v, &j.held_out);
                            if let speccoder_core::verify::Verdict::Failed { reasons } = &mut v
                                && !j.failed.is_empty()
                                && let Some(last) = reasons.last_mut()
                            {
                                *last = speccoder_core::retry::held_out_reason(&j.failed);
                            }
                            report.verdict = Some(v);
                        }
                    }
                    Err((outcome, why)) => {
                        held_out_log.push(serde_json::json!({
                            "attempt": held_out_log.len() + 1,
                            "outcome": outcome,
                        }));
                        report.verdict = None;
                        report.reason = Some(why);
                        attempt_status = "held_out".to_owned();
                    }
                }
            }
            // 7.6 A bug task: red → green per reproduction, the pass-to-pass
            //     set, the revert and the special cases.
            if let Some(fix) = bug_fix.as_mut().filter(|_| {
                attempt_status == "completed"
                    && findings.is_empty()
                    && matches!(
                        report.verdict,
                        Some(
                            speccoder_core::verify::Verdict::Verified
                                | speccoder_core::verify::Verdict::RegressionOnly
                        )
                    )
            }) {
                let keep = crate::strength::dir(&s.plane_dir, &feature_name, task);
                let judging = super::bug::Judging {
                    before: &before,
                    baseline: &baseline,
                    verification: report.verification.as_ref(),
                    base: report
                        .verdict
                        .clone()
                        .unwrap_or(speccoder_core::verify::Verdict::RegressionOnly),
                    keep: &keep,
                };
                match super::bug::judge(&bug_ctx, fix, &judging) {
                    Ok(j) => {
                        report.verdict = Some(j.verdict);
                        if let Some(weak) = j.weak {
                            report.reason = Some(format!(
                                "reproduction too weak: a fix that special-cases {}'s input passes ({}); `speccoder task rule {task} --tests-right --reason …` if the tests are right",
                                weak.scenario, weak.rationale
                            ));
                            bug_weak = Some(weak);
                            attempt_status = "repro_weak".to_owned();
                        }
                    }
                    Err(e) => {
                        report.integrity.push(e.clone());
                        report.reason = Some(e);
                        report.verdict = None;
                        attempt_status = "integrity".to_owned();
                    }
                }
            }
            if let Some(h) = report
                .acceptance
                .as_mut()
                .and_then(|a| a.get_mut("held_out"))
                .filter(|h| h["state"] == "on")
            {
                let failed = held_out_log
                    .iter()
                    .filter(|o| o["outcome"] == "failed")
                    .count();
                h["attempts"] = serde_json::json!(held_out_log);
                h["gap"] = serde_json::json!(failed > 0);
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
            if passing && mutation_config.as_ref().is_none_or(|m| !m.suite) {
                stage(
                    &*live,
                    task,
                    Stage::Mutation,
                    StageState::Skipped,
                    Some(if mutation_config.is_none() {
                        "not configured".to_owned()
                    } else {
                        "suite = false: for acceptance strength only".to_owned()
                    }),
                    None,
                );
            }
            if let Some(m) = mutation_config.as_ref().filter(|m| passing && m.suite) {
                let began = std::time::Instant::now();
                stage(
                    &*live,
                    task,
                    Stage::Mutation,
                    StageState::Started,
                    None,
                    None,
                );
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
                let held = !matches!(
                    report.verdict,
                    Some(speccoder_core::verify::Verdict::Failed { .. })
                );
                stage(
                    &*live,
                    task,
                    Stage::Mutation,
                    if held {
                        StageState::Passed
                    } else {
                        StageState::Failed
                    },
                    report
                        .mutation
                        .as_ref()
                        .and_then(crate::mutation::trailer)
                        .map(|t| t.trim().trim_start_matches("Mutation: ").to_owned()),
                    Some(began),
                );
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
            let stop = if bug_candidate.is_some() {
                Some("a candidate is judged, never retried".to_owned())
            } else {
                retry_stop(
                    &findings,
                    verification,
                    earlier.last().map(|e| e.fingerprint.as_str()),
                    &fingerprint,
                    attempt_no,
                    run_config.max_attempts,
                )
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
    // Scope: what the attempt changed beyond the paths its task names.
    report.scope = parsed_task.as_ref().map(|t| scope_of(&report, &t.title));

    // 7c. Acceptance strength: the set a verified verdict
    //     rests on, run against wrong and permitted candidates. Judge-side;
    //     the implementer never sees a variant, a survivor or a finding.
    let strength_dir = crate::strength::dir(&s.plane_dir, &feature_name, task);
    let mut attempt_status = attempt_status;
    let verified = attempt_status == "completed"
        && matches!(
            report.verdict,
            Some(speccoder_core::verify::Verdict::Verified)
        );
    let visible_argv = acceptance
        .as_ref()
        .filter(|a| a.red)
        .and_then(|a| a.argv.clone());
    if verified && !run_config.strength.enabled {
        stage(
            &*live,
            task,
            Stage::Strength,
            StageState::Skipped,
            Some("strength = false".to_owned()),
            None,
        );
    }
    if let Some(argv) = visible_argv.filter(|_| verified && run_config.strength.enabled) {
        use speccoder_core::strength::{Gate, gate, over_strict};
        let began = std::time::Instant::now();
        stage(
            &*live,
            task,
            Stage::Strength,
            StageState::Started,
            None,
            None,
        );
        let fin = worktree::checkpoint(&wt_root, &tmp);
        let patch = fin
            .as_ref()
            .ok()
            .and_then(|now| worktree::patch(&wt_root, &start, now).ok())
            .unwrap_or_default();
        let mut notes = Vec::new();
        let held: Vec<(crate::held_out::HeldOutFile, String)> =
            match acceptance.as_ref().and_then(|a| a.held_out.clone()) {
                None => Vec::new(),
                Some(sha) => match crate::held_out::load_verified(&held_out_store, &sha) {
                    Ok((manifest, contents)) => manifest
                        .files
                        .into_iter()
                        .zip(contents.into_iter().map(|(_, c)| c))
                        .collect(),
                    Err(t) => {
                        notes.push(format!("held-out set not overlaid: {}", t.0));
                        Vec::new()
                    }
                },
            };
        let rulings = crate::strength::rulings(&strength_dir);
        let mut commands = vec![argv];
        let measure = |commands: &[Vec<String>]| {
            crate::strength::measure(
                &crate::strength::Setup {
                    wt_root: &wt_root,
                    project: &project,
                    tmp: &tmp,
                    runner: &runner,
                    policy: &policy,
                    acceptance: commands,
                    held_out: &held,
                    mutation: mutation_config.as_ref(),
                    patch: &patch,
                    config: &run_config.strength,
                    keep: &strength_dir,
                    rulings: &rulings,
                },
                |changed| {
                    // Source files only: the author never sees a test.
                    let input = substrate::near_miss::input(
                        &bundle_value,
                        changed,
                        &crate::mutation::source_patch(&patch),
                        run_config.strength.near_miss,
                        run_config.strength.permitted,
                    );
                    match rt.block_on(plane.near_miss(&feature_name, task, input)) {
                        Ok(o) => {
                            let out = o
                                .output
                                .as_ref()
                                .map(|v| v.peek().clone())
                                .unwrap_or_default();
                            let ok = matches!(o.status, agentplane::prelude::RunStatus::Succeeded)
                                && !out["answer"].is_null();
                            crate::strength::Authored {
                                answer: if ok {
                                    Ok(out["answer"].clone())
                                } else {
                                    Err(format!(
                                        "the variant author did not answer ({})",
                                        o.status.as_str()
                                    ))
                                },
                                run_id: Some(o.run_id.to_string()),
                                usage: Some(out["usage"].clone()).filter(|u| !u.is_null()),
                            }
                        }
                        Err(e) => crate::strength::Authored {
                            answer: Err(redact(&e.0, secret)),
                            run_id: None,
                            usage: None,
                        },
                    }
                },
            )
        };
        let mut measured = measure(&commands);
        // The gate: one regeneration, told what got through; then it holds
        // or the set is too weak for a tick.
        let min = run_config.strength.min;
        if let (Ok(a), Ok(fin)) = (&measured, &fin)
            && gate(min, &a.combined, false) == Gate::Failed
            && over_strict(&a.permitted).is_empty()
        {
            let visible_tests: Vec<(String, String)> = report
                .acceptance
                .as_ref()
                .and_then(|a| a["files"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p.as_str())
                .map(|p| {
                    (
                        p.to_owned(),
                        std::fs::read_to_string(project.join(p)).unwrap_or_default(),
                    )
                })
                .collect();
            let setup = AcceptanceSetup {
                rt: &rt,
                plane: &plane,
                project: &project,
                worktree: &wt_root,
                tmp: &tmp,
                policy: &policy,
                feature: &feature_name,
                task,
                bundle: bundle_value.clone(),
                scenarios: &scenarios,
                instructions: &instructions,
                runner: &runner,
            };
            let weak = crate::strength::weak(a);
            match strengthen(&setup, &visible_tests, weak, &before, fin) {
                Ok(added) => {
                    if let Some(acc) = report.acceptance.as_mut() {
                        if let Some(files) = acc["files"].as_array_mut() {
                            files.extend(added.files.iter().map(|f| serde_json::json!(f)));
                        }
                        acc["strengthened"] = serde_json::json!({
                            "files": added.files,
                            "argv": added.argv,
                            "run_id": added.report["run_id"],
                            "usage": added.report["usage"],
                        });
                    }
                    report.usage = add_usage(report.usage.as_ref(), Some(&added.report["usage"]));
                    commands.push(added.argv);
                    let mut again = measure(&commands);
                    if let Ok(b) = again.as_mut() {
                        b.gate = gate(min, &b.combined, true);
                        b.notes.insert(
                            0,
                            "strengthened once with added acceptance tests".to_owned(),
                        );
                    }
                    measured = again;
                }
                Err(why) => {
                    if let Ok(a) = measured.as_mut() {
                        a.notes
                            .insert(0, format!("could not strengthen the set: {why}"));
                    }
                }
            }
        }
        match measured {
            Ok(mut a) => {
                if a.gate == Gate::Off {
                    a.gate = gate(min, &a.combined, false);
                }
                a.notes.extend(notes);
                let findings = over_strict(&a.permitted);
                if !findings.is_empty() {
                    attempt_status = "over_strict".to_owned();
                    report.reason = Some(format!(
                        "an acceptance test rejects an implementation the spec appears to allow: {}",
                        findings
                            .iter()
                            .map(|p| format!("{} ({})", p.scenario, p.rationale))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                } else if a.gate == Gate::Failed {
                    attempt_status = "acceptance_weak".to_owned();
                    report.reason = Some(format!(
                        "acceptance set too weak: strength {} is below min_strength {}{}",
                        speccoder_core::strength::summary(&a.combined, None),
                        min.unwrap_or_default(),
                        a.notes
                            .first()
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default()
                    ));
                } else {
                    crate::strength::release(&strength_dir);
                }
                // The variant author's tokens count with the task's.
                report.usage = add_usage(report.usage.as_ref(), a.usage.as_ref());
                report.strength = Some(a);
            }
            Err(e) => {
                report.integrity.push(e.clone());
                report.reason = Some(e);
                report.verdict = None;
                attempt_status = "integrity".to_owned();
            }
        }
        report.attempt_status.clone_from(&attempt_status);
        let held_up = attempt_status == "completed";
        stage(
            &*live,
            task,
            Stage::Strength,
            if held_up {
                StageState::Passed
            } else {
                StageState::Failed
            },
            report.strength.as_ref().map(|a| {
                speccoder_core::strength::summary(&a.combined, a.notes.first().map(String::as_str))
            }),
            Some(began),
        );
    }

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
    if !run_config.monitor && verdict_passed {
        stage(
            &*live,
            task,
            Stage::Monitor,
            StageState::Skipped,
            Some("not enabled".to_owned()),
            None,
        );
    }
    if run_config.monitor && verdict_passed {
        let began = std::time::Instant::now();
        stage(
            &*live,
            task,
            Stage::Monitor,
            StageState::Started,
            None,
            None,
        );
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
        let (monitor, suspicion) = monitor_attempt(&rt, &plane, &feature_name, task, input, secret);
        report.monitor = Some(monitor);
        suspected = suspicion;
        stage(
            &*live,
            task,
            Stage::Monitor,
            if suspected {
                StageState::Failed
            } else {
                StageState::Passed
            },
            suspected.then(|| "suspects gaming".to_owned()),
            Some(began),
        );
    }

    // 8. A passed task is ticked by the engine and committed. In place, the
    //    spec and the checkout must still be as the run found them.
    let in_place_stop = options
        .session
        .as_ref()
        .filter(|_| verdict_passed && !suspected)
        .and_then(|s| {
            s.lock()
                .ok()
                .and_then(|s| s.check_before_tick(&policy).err())
        });
    if let Some(e) = &in_place_stop {
        report.reason = Some(e.line());
        report.verdict = None;
        // The saved attempt names the cause: `intent_changed`, `judge_changed`, …
        attempt_status = e.code.to_lowercase().replace('-', "_");
        report.attempt_status.clone_from(&attempt_status);
    }
    let passed = verdict_passed && !suspected && in_place_stop.is_none();
    if passed && let (Some(t), Some(file)) = (&parsed_task, &tasks_file) {
        let began = std::time::Instant::now();
        stage(&*live, task, Stage::Commit, StageState::Started, None, None);
        let message = tick_message(&TickFacts {
            feature: &feature_name,
            task,
            title: &t.title,
            report: &report,
            red: acceptance
                .as_ref()
                .and_then(|a| a.red_class.filter(|c| a.red && c.is_red())),
            checklists_overridden,
            extra: bug_fix
                .as_ref()
                .map(|f| {
                    super::bug::trailers(
                        f,
                        &report
                            .verdict
                            .as_ref()
                            .map(super::outcome::verdict_word)
                            .unwrap_or_default(),
                        &crate::strength::rulings(&strength_dir),
                    )
                })
                .unwrap_or_default(),
        });
        match &options.session {
            // Worktree: tick, then commit the tick with the evidence.
            None => {
                match format.tick(file, task, &t.line_hash) {
                    Ok(()) => report.ticked = true,
                    Err(e) => report.integrity.push(format!("tick refused: {e}")),
                }
                if report.ticked && run_config.commit_per_task {
                    match commit_attempt(&wt_root, &message) {
                        Ok(id) => report.commit = Some(id),
                        Err(e) => report.integrity.push(format!("commit failed: {e}")),
                    }
                }
            }
            // In place: evidence first, then the tick; a git-ignored
            // tasks.md is never in the commit, so its tick is local.
            Some(session) => {
                let digest =
                    crate::in_place::intent_digest(&project, &policy, format.messages().tasks_file);
                if let Ok(mut s) = session.lock() {
                    let done = s.commit_and_tick(
                        &plane_dir,
                        file,
                        &t.line_hash,
                        &message,
                        &digest,
                        || format.tick(file, task, &t.line_hash),
                    );
                    report.ticked = done.ticked;
                    report.commit = done.commit;
                    report.integrity.extend(done.problems);
                }
            }
        }
        report.instructions = attempt_instructions
            .iter()
            .map(|i| speccoder_core::InstructionFile {
                path: i.path.clone(),
                sha256: i.sha256.clone(),
                bytes: i.bytes,
                cut: i.cut,
            })
            .collect();
        // Signed, portable evidence of the tick: never undoes it.
        if let Some(commit) = report
            .commit
            .clone()
            .filter(|_| report.ticked && run_config.attest.enabled)
        {
            let verdict = report
                .verdict
                .as_ref()
                .map(super::outcome::verdict_word)
                .unwrap_or_default();
            let sandboxed = sandbox_config.mode != sandbox::Mode::Off && sandbox::backend().is_ok();
            let feature_dir = snap
                .feature
                .as_ref()
                .map(|f| f.feature.dir.clone())
                .unwrap_or_default();
            let requirements = format.requirement_ids(Path::new(&snap.root), &feature_dir);
            let tick = crate::attest::Tick {
                repo: &wt_root,
                config: &config_text,
                attest: &run_config.attest,
                format: &format!("{:?}", format.kind()).to_lowercase(),
                feature: &feature_name,
                feature_dir: &feature_dir,
                task,
                title: &t.title,
                line_hash: &t.line_hash,
                section: t.story.as_deref(),
                section_text: bundle_value["story"].as_str().unwrap_or_default(),
                requirements: &requirements,
                commit: &commit,
                verdict: &verdict,
                sandbox: serde_json::json!({
                    "mode": format!("{:?}", sandbox_config.mode).to_lowercase(),
                    "backend": sandboxed.then_some(if cfg!(target_os = "macos") { "seatbelt" } else { "landlock" }),
                    "network": format!("{:?}", sandbox_config.network).to_lowercase(),
                }),
                executor: serde_json::json!({
                    "kind": if bug_candidate.is_some() { "candidate".to_owned() } else { executor.as_ref().map_or_else(|| "native".to_owned(), |e| match &e.kind {
                        external::Kind::Claude => "claude".to_owned(),
                        external::Kind::Codex => "codex".to_owned(),
                        external::Kind::Command(_) => "command".to_owned(),
                    }) },
                    "candidate": bug_candidate.as_ref().map(|(_, c)| serde_json::json!({ "origin": c.origin, "digest": c.digest, "head_sha": c.head_sha })),
                }),
                model: serde_json::json!({ "provider": s.model.provider, "model": s.model.model }),
                bug: bug_fix
                    .as_ref()
                    .map(|f| super::bug::predicate_fields(&f.bug, &f.record)),
            };
            report.attestation = Some(crate::attest::record_tick(&tick, &report));
        }
        let committed = report.ticked && (report.commit.is_some() || !run_config.commit_per_task);
        stage(
            &*live,
            task,
            Stage::Commit,
            if committed {
                StageState::Passed
            } else {
                StageState::Failed
            },
            report.commit.as_ref().map(|c| c.chars().take(7).collect()),
            Some(began),
        );
    }
    // 8'. A bug in the format's own bug structure: nothing is ticked; its
    //     verdict commit carries the evidence (a trailer-only task).
    if passed && let Some(n) = &native {
        let began = std::time::Instant::now();
        stage(&*live, task, Stage::Commit, StageState::Started, None, None);
        let verdict = report
            .verdict
            .as_ref()
            .map(super::outcome::verdict_word)
            .unwrap_or_default();
        let message = tick_message(&TickFacts {
            feature: &feature_name,
            task,
            title: &n.title,
            report: &report,
            red: acceptance
                .as_ref()
                .and_then(|a| a.red_class.filter(|c| a.red && c.is_red())),
            checklists_overridden: false,
            extra: bug_fix
                .as_ref()
                .map(|f| {
                    super::bug::trailers(f, &verdict, &crate::strength::rulings(&strength_dir))
                })
                .unwrap_or_default(),
        });
        match commit_attempt(&wt_root, &message) {
            Ok(id) => report.commit = Some(id),
            Err(e) => report.integrity.push(format!("commit failed: {e}")),
        }
        if let Some(commit) = report.commit.clone().filter(|_| run_config.attest.enabled) {
            let sandboxed = sandbox_config.mode != sandbox::Mode::Off && sandbox::backend().is_ok();
            let tick = crate::attest::Tick {
                repo: &wt_root,
                config: &config_text,
                attest: &run_config.attest,
                format: &format!("{:?}", format.kind()).to_lowercase(),
                feature: &feature_name,
                feature_dir: &n.dir,
                task,
                title: &n.title,
                line_hash: "",
                section: None,
                section_text: &n.story,
                requirements: &[],
                commit: &commit,
                verdict: &verdict,
                sandbox: serde_json::json!({
                    "mode": format!("{:?}", sandbox_config.mode).to_lowercase(),
                    "backend": sandboxed.then_some(if cfg!(target_os = "macos") { "seatbelt" } else { "landlock" }),
                    "network": format!("{:?}", sandbox_config.network).to_lowercase(),
                }),
                executor: serde_json::json!({
                    "kind": if bug_candidate.is_some() { "candidate" } else if executor.is_some() { "external" } else { "native" },
                }),
                model: serde_json::json!({ "provider": s.model.provider, "model": s.model.model }),
                bug: bug_fix
                    .as_ref()
                    .map(|f| super::bug::predicate_fields(&f.bug, &f.record)),
            };
            report.attestation = Some(crate::attest::record_tick(&tick, &report));
        }
        stage(
            &*live,
            task,
            Stage::Commit,
            if report.commit.is_some() {
                StageState::Passed
            } else {
                StageState::Failed
            },
            report.commit.as_ref().map(|c| c.chars().take(7).collect()),
            Some(began),
        );
    }

    // 9. An attempt whose tests reject what the spec appears to allow is
    //    held for a person, as a waiting call is: its tree and resume marker
    //    stay, and after a ruling the next run judges it again from the
    //    journal, repeating no model call.
    if attempt_status == "repro_weak"
        && let Some(weak) = &bug_weak
        && let Err(e) = crate::strength::hold(&strength_dir, &[weak])
    {
        report
            .integrity
            .push(format!("holding the attempt failed: {e}"));
    }
    if attempt_status == "over_strict" || attempt_status == "repro_weak" {
        let findings = report
            .strength
            .as_ref()
            .map(|a| speccoder_core::strength::over_strict(&a.permitted))
            .unwrap_or_default();
        if attempt_status == "over_strict"
            && let Err(e) = crate::strength::hold(&strength_dir, &findings)
        {
            report
                .integrity
                .push(format!("holding the attempt failed: {e}"));
        }
        marker_guard.keep();
    }
    // 9. An attempt that did not pass is saved under an attempt ref and
    //    undone: the next attempt starts where this one did.
    if !passed && attempt_status != "over_strict" && attempt_status != "repro_weak" {
        let (outcome, reason) = saved_outcome(&report, &attempt_status, suspected);
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

    let (status, message) = status_and_message(&report, &attempt_status, suspected);
    if let Some(fix) = bug_fix.as_mut() {
        return super::bug::finished(
            &bug_ctx,
            fix,
            report,
            &attempt_status,
            passed,
            status,
            redact(&message, secret),
        );
    }
    live.event(&finished_event(
        task,
        &report,
        &attempt_status,
        suspected,
        redact(&message, secret),
    ));
    CommandResult::new("run", status, &redact(&message, secret))
        .with_data(CommandData::Run(Box::new(report)))
}

/// `note`, with what the check itself said last (its own reason) and, for
/// the causes that are known, the fix.
fn with_cause(note: &str, c: &speccoder_core::verify::CheckResult) -> String {
    let said = c
        .stderr_tail
        .lines()
        .chain(c.stdout_tail.lines())
        .map(str::trim)
        .find(|l| l.starts_with("error"))
        .map(|l| l.chars().take(200).collect::<String>());
    let fix = match said.as_deref() {
        Some(l) if l.contains("profile `speccoder` not found") => Some(
            "`.config/nextest.toml` is not committed — `git add .config/nextest.toml && git commit -m \"speccoder: nextest profile\"`",
        ),
        Some(l) if l.contains("no tests to run") => Some(
            "the project has no tests yet; if the first task writes them, run with --allow-failing-baseline, or add `--no-tests=pass` to the check's argv",
        ),
        Some(l) if l.contains("must be present") || l.contains("no targets") => Some(
            "the project has nothing to build yet; if the first task creates it, run with --allow-failing-baseline",
        ),
        _ => None,
    };
    match (said, fix) {
        (Some(s), Some(f)) => format!("{note} ({s}); {f}"),
        (Some(s), None) => format!("{note} ({s})"),
        _ => note.to_owned(),
    }
}

/// Why a failing baseline check is broken rather than failing tests: it
/// could not start, or it failed without any test result to show for it.
/// `None`: it passed, or its tests ran and some failed.
#[must_use]
pub fn broken(c: &speccoder_core::verify::CheckResult) -> Option<String> {
    use speccoder_core::verify::CheckStatus;
    if !c.status.is_failure() || c.status == CheckStatus::TimedOut {
        return None;
    }
    if let Some(note) = &c.note {
        return Some(with_cause(note, c));
    }
    if c.tests.as_ref().is_some_and(|t| !t.is_empty()) {
        return None;
    }
    let summary = crate::verify::summary::parse(&c.stdout_tail, &c.stderr_tail, c.exit_code);
    // Only what is unambiguous: a command that cannot start, a build that
    // fails, or a test runner that ran no test. A script or `grep` that
    // fails until the task is done is a failing check, not a broken one.
    let runner = c
        .argv
        .first()
        .map(|a| a.rsplit('/').next().unwrap_or(a))
        .is_some_and(|a| {
            matches!(
                a,
                "cargo"
                    | "pytest"
                    | "python"
                    | "python3"
                    | "uv"
                    | "npm"
                    | "npx"
                    | "pnpm"
                    | "yarn"
                    | "bun"
                    | "go"
                    | "jest"
                    | "vitest"
                    | "mvn"
                    | "gradle"
                    | "dotnet"
                    | "mix"
                    | "swift"
            )
        });
    let cannot_start = matches!(c.exit_code, Some(126 | 127));
    let is_broken = match &summary {
        _ if cannot_start => true,
        Some(s) if s.build_failed => true,
        Some(s) => s.passed + s.failed + s.errors == 0,
        None => runner,
    };
    match is_broken {
        false => None,
        true => {
            let first = c
                .stderr_tail
                .lines()
                .chain(c.stdout_tail.lines())
                .map(str::trim)
                .find(|l| l.to_lowercase().contains("error"))
                .or_else(|| c.stderr_tail.lines().map(str::trim).find(|l| !l.is_empty()))
                .unwrap_or("it failed without running a test")
                .chars()
                .take(160)
                .collect::<String>();
            Some(first)
        }
    }
}
