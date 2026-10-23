//! `speccoder run --task`: one task from baseline to tick.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{
    approvals, attempts, context, external, git, integrity, mcp, mutation, progress, resume,
    sandbox, substrate, tools, verify, worktree,
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
    let approver = options.approver.clone();
    use speccoder_core::snapshot::TaskState;

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
        Some(lane) => match lane_worktree(path, state, &feature_name, lane, &wt_path, &project_dir)
        {
            Ok(paths) => paths,
            Err(e) => return refuse(CommandStatus::GitPrecondition, e),
        },
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
    if interrupted.is_none() && commits_per_task && worktree::require_clean(&wt_root).is_err() {
        match set_aside_leftovers(&wt_root, &tmp, &feature_name, task) {
            Ok(reference) => set_aside = Some(reference),
            Err(e) => return refuse(CommandStatus::GitPrecondition, e),
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
    // Unfinished checklists stop the task before any model call (the
    // committed checklists in the worktree are what counts).
    let checklists_overridden =
        match checklist_gate(&project, Some(&feature_name), options.ignore_checklists) {
            Ok(overridden) => overridden,
            Err(stop) => return *stop,
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
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
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
    // Failing checks at the baseline are reported, not blamed: the stage held.
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

    // MCP servers, started in the worktree for this task's attempts.
    let servers = match mcp::parse(&config_text) {
        Ok(s) => s,
        Err(e) => return fail(CommandStatus::ConfigError, format!("speccoder.toml: {e}")),
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
    let demand_red = run_config.acceptance && context::is_convergence(&bundle.task.phase);
    // Where this run started: an attempt that does not pass is undone to here.
    let start = match worktree::checkpoint(&wt_root, &tmp) {
        Ok(c) => c,
        Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
    };

    let began = std::time::Instant::now();
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
            runner: &runner,
        }))
    } else {
        None
    };
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
    let (mut report, attempt_status) = loop {
        let mut earlier: Vec<speccoder_core::EarlierAttempt> = Vec::new();
        let mut digest: Option<serde_json::Value> = None;
        let mut attempt_no = 1u32;
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
            let approved_writes = approved_writes(&decisions.approved, &output["tool_calls"]);
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
            let attempt_status = if succeeded {
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

            // 7a. Mutation testing of a passing attempt's changes.
            let passing = attempt_status == "completed"
                && matches!(
                    report.verdict,
                    Some(
                        speccoder_core::verify::Verdict::Verified
                            | speccoder_core::verify::Verdict::RegressionOnly
                    )
                );
            if passing && mutation_config.is_none() {
                stage(
                    &*live,
                    task,
                    Stage::Mutation,
                    StageState::Skipped,
                    Some("not configured".to_owned()),
                    None,
                );
            }
            if let Some(m) = mutation_config.as_ref().filter(|_| passing) {
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
            let stop = retry_stop(
                &findings,
                verification,
                earlier.last().map(|e| e.fingerprint.as_str()),
                &fingerprint,
                attempt_no,
                run_config.max_attempts,
            );
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

    // 8. A passed task is ticked by the engine and committed.
    let passed = verdict_passed && !suspected;
    if passed && let (Some(t), Some(file)) = (&parsed_task, &tasks_file) {
        let began = std::time::Instant::now();
        stage(&*live, task, Stage::Commit, StageState::Started, None, None);
        match speccoder_speckit::mark_task_complete(file, task, &t.line_hash) {
            Ok(()) => report.ticked = true,
            Err(e) => report.integrity.push(format!("tick refused: {e}")),
        }
        if report.ticked && run_config.commit_per_task {
            let message = tick_message(&TickFacts {
                feature: &feature_name,
                task,
                title: &t.title,
                report: &report,
                red: acceptance
                    .as_ref()
                    .and_then(|a| a.red_class.filter(|c| a.red && c.is_red())),
                checklists_overridden,
            });
            match commit_attempt(&wt_root, &message) {
                Ok(id) => report.commit = Some(id),
                Err(e) => report.integrity.push(format!("commit failed: {e}")),
            }
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

    // 9. An attempt that did not pass is saved under an attempt ref and
    //    undone: the next attempt starts where this one did.
    if !passed {
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
