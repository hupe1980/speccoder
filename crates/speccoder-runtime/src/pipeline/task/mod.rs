//! `speccoder run --task`: one task from baseline to tick.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{
    approvals, attempts, external, integrity, mcp, mutation, progress, resume, sandbox,
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

/// One task: its attempt, and — when a planning attempt changed code —
/// the task again as code (the planning attempt saved and undone, its
/// saved outcome the reason it is code from now on).
mod open;
mod ready;
mod tests;

/// A phase's result, or the run's end with what a person sees.
macro_rules! ok {
    ($phase:expr) => {
        match $phase {
            Ok(x) => x,
            Err(end) => return *end,
        }
    };
}

/// What opening the task started that later phases carry on.
pub(super) struct Started {
    /// Removes the resume marker when the run ends, unless kept.
    pub(super) marker_guard: crate::resume::Guard,
    /// The worktree's head an interrupted attempt is resumed at.
    pub(super) head_now: String,
    /// Leftovers set aside before the run, named in its report.
    pub(super) set_aside: Option<String>,
}

/// What one phase sets and a later phase changes: handed on explicitly,
/// changed only by a phase that takes it mutably.
pub(super) struct Carried {
    /// The worktree's head an interrupted attempt is resumed at (moved by
    /// the story's oracle commit).
    pub(super) head_now: String,
    /// Removes the resume marker when the run ends, unless kept.
    pub(super) marker_guard: crate::resume::Guard,
    /// Leftovers set aside before the run, named in its report.
    pub(super) set_aside: Option<String>,
    /// The checks on the tree the attempt starts from (re-run after the
    /// story's oracle is committed).
    pub(super) baseline: speccoder_core::verify::VerifyReport,
    /// What the run tells a person, gathered by every phase.
    pub(super) notices: Vec<String>,
}

/// An early end recorded as the task run's end (`TaskFinished`), redacted.
pub(super) fn ended(
    live: &dyn progress::Progress,
    task: &str,
    secret: Option<&str>,
    status: CommandStatus,
    m: String,
) -> CommandResult {
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
        regression: None,
    });
    CommandResult::new("run", status, &redact(&m, secret))
}

pub(crate) fn run_task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    provider: std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let first = run_task_once(
        path,
        feature,
        task,
        state,
        Arc::clone(&provider),
        secret,
        options,
    );
    match first.data() {
        Some(CommandData::Run(r)) if r.attempt_status == CHANGED_CODE => {
            if let Some(p) = &options.progress {
                p.event(&progress::Event::Notice {
                    text: format!("{task} changed code, so it runs as code"),
                });
            }
            run_task_once(path, feature, task, state, provider, secret, options)
        }
        _ => first,
    }
}

/// The outcome of a planning attempt that changed something outside its
/// unit: saved and undone, and the task is code from then on.
pub const CHANGED_CODE: &str = "planning_changed_code";

#[allow(clippy::too_many_lines)] // one linear procedure, kept in reading order
fn run_task_once(
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
    let (opened, started) = ok!(open::task(path, feature, task, state, secret, options));
    let open::Opened {
        approver,
        s,
        native,
        feature_name,
        plane_dir,
        live,
        wt_path,
        project,
        wt_root,
        tmp,
        marker_path,
        autopilot,
        interrupted,
        format,
        snap,
        checklists_overridden,
        parsed_task,
        tasks_file,
        acceptance_story,
        config_text,
        run_config,
        executor,
        sandbox_config,
        runner,
        mutation_config,
        worktree_config,
        check_argvs,
        planning,
        policy,
        contained,
        wt_root_canonical,
        this_run,
        decisions,
        ..
    } = &opened;
    let (format, live) = (*format, Arc::clone(live));
    let (autopilot, contained, checklists_overridden) = (*autopilot, *contained, *checklists_overridden);
    let (ready, mut carried) = ok!(ready::make(
        &opened,
        started,
        task,
        state,
        secret,
        options,
        Arc::clone(&provider),
    ));
    let ready::Ready {
        rt,
        bundle_value,
        instructions,
        connected,
        present,
        plane,
        start,
        copied_files,
        prepares,
        ..
    } = &ready;
    let prepares = *prepares;
    let attempt_instructions: &[crate::instructions::Instruction] = if executor.is_none() {
        instructions
    } else {
        &[]
    };
    // From here on, an early end is recorded as the task run's end.
    let fail = |status: CommandStatus, m: String| ended(&*live, task, secret, status, m);

    let tests = ok!(tests::first(
        &opened,
        &ready,
        &mut carried,
        task,
        state,
        secret,
        options,
    ));
    let tests::TestsFirst {
        mut bug_fix,
        bug_candidate,
        test_layout,
        test_layout_text,
        story_scenarios,
        story_key,
        judged_by,
        in_story,
        scenarios,
        review_frozen,
        story_oracle,
        oracle_note,
        oracle_empty,
        before_run,
        oracle_spent,
        ruled,
        related,
        told,
        acceptance,
        held_out_store,
    } = tests;
    let Carried {
        marker_guard,
        head_now,
        set_aside,
        baseline,
        mut notices,
    } = carried;
    let bug_ctx = tests::bug_ctx(&opened, &ready, task);
    // 5. Checkpoint, attempt, checkpoint — again after a failed verdict,
    //    from the same checkpoint, with the verifier's digest.
    let before = match &interrupted {
        Some(m) => worktree::Checkpoint(m.before.clone()),
        None => match worktree::checkpoint(wt_root, tmp) {
            Ok(c) => c,
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        },
    };
    let acceptance_saved = acceptance
        .as_ref()
        .and_then(|a| serde_json::to_value(a).ok());
    // Tests that exist now are frozen for this attempt.
    let frozen: Vec<String> = match worktree::files(wt_root, &before) {
        Ok(f) => f,
        Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
    }
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
    // The provider refused the account (its code says so): no retry helps.
    let mut refused_account = false;
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
            // attempt's "for the task" answers included), and the calls a
            // person approved once, which this attempt alone is given:
            // journaled. Decisions that cannot be read stop the task — the
            // grants read when it began may hold one rejected since.
            let (task_grants, approved_once) = match approvals::Decisions::load(&s.plane_dir) {
                // Autopilot: only the ticked grants (`run_grants`) — no
                // answer kept for the task, no once-grant is taken.
                Ok(_) if autopilot => (Vec::new(), Vec::new()),
                Ok(mut d) => {
                    let once = d.take_once(feature_name, task, this_run.as_str());
                    if !once.is_empty()
                        && let Err(e) = d.save(&s.plane_dir)
                    {
                        return fail(CommandStatus::Error, e);
                    }
                    (d.for_run(feature_name, task, this_run.as_str()), once)
                }
                Err(e) => return fail(CommandStatus::ConfigError, e),
            };
            // This run's grants as they stand now: journaled with the input.
            let run_approved: Vec<crate::tools::policy::Object> = options
                .run_grants
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            // Everything the attempt's decisions depend on is in its journaled input.
            let input = substrate::attempt::input(
                with_oracle(bundle_value, &oracle_note),
                &substrate::attempt::Settings {
                    checks: check_argvs,
                    allow: &run_config.allow,
                    allow_prefix: &run_config.allow_prefix,
                    max_turns: run_config.max_turns,
                    max_tool_calls: run_config.max_tool_calls,
                    reasoning: s.model.reasoning_summary,
                    frozen: &frozen,
                    approved: &task_grants,
                    rejected: &decisions.rejected,
                    run_approved: &run_approved,
                    approved_once: &approved_once,
                    nudges: &decisions.nudges,
                    mcp: &connected.tools,
                    root: wt_root_canonical,
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
                    autopilot: options.mode == speccoder_core::oracle::RunMode::Autopilot,
                    planning: planning.as_deref(),
                    contained,
                    // The task's share of the run's cap, less what the task
                    // spent so far (its tests' authoring, earlier attempts).
                    left: options.left.map(|cap| {
                        let share = substrate::attempt::Left {
                            cap,
                            prices: s.model.prices,
                        };
                        let so_far = add_usage(usage.as_ref(), oracle_spent.as_ref());
                        substrate::attempt::Left {
                            cap: cap
                                .less(share.spent(&substrate::attempt::usage_of(so_far.as_ref()))),
                            ..share
                        }
                    }),
                },
                digest.as_ref(),
                attempt_instructions,
                // No runner found: nothing to say (and nothing of the worktree's).
                (!test_layout.runners.is_empty()).then_some(test_layout_text.as_str()),
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
            let judged_before = integrity::JudgeSnapshot::take(wt_root, policy);
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
                let prompt = external::prompt_in(
                    format.wording(),
                    &with_oracle(bundle_value, &oracle_note),
                    digest.as_ref(),
                );
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
                let _ = allow;
                let o = match &exec.kind {
                    external::Kind::Command(argv) => {
                        external::run(argv, exec.timeout, wt_root, &prompt, tmp)
                    }
                    external::Kind::Acp(agent) => {
                        // The agent's calls are decided by the policy a native
                        // attempt's are, from the same settings.
                        let policy = match substrate::attempt::policy_of(
                            &input.peek()["settings"],
                            format.rules(),
                        ) {
                            Ok(p) => p,
                            Err(e) => return fail(CommandStatus::RunFailed, e.to_string()),
                        };
                        let tools = match tools::WorkspaceTools::new(wt_root, policy.clone()) {
                            Ok(t) => t.with_runner(runner.clone()),
                            Err(e) => {
                                return fail(
                                    CommandStatus::Error,
                                    format!("{}: {e}", wt_root.display()),
                                );
                            }
                        };
                        let governor = Arc::new(crate::acp::govern::Governor::new(
                            crate::acp::govern::Setup {
                                root: std::fs::canonicalize(wt_root)
                                    .unwrap_or_else(|_| wt_root.clone()),
                                tools,
                                policy,
                                // A lane stops for a person; every other attempt asks.
                                approver: approver.clone().filter(|_| options.lane.is_none()),
                                present: present.clone(),
                                task: format!("{feature_name}/{task}"),
                                task_title: parsed_task
                                    .as_ref()
                                    .map(|t| t.title.clone())
                                    .unwrap_or_default(),
                                run: Some(this_run.to_string()),
                                frozen_additions: run_config.frozen_additions,
                                run_approved: run_approved.clone(),
                                approved_once: approved_once.clone(),
                                progress: Some((task.to_owned(), Arc::clone(&live))),
                                copied: copied_files.clone(),
                                agent: agent.id.clone(),
                            },
                        ));
                        let a = external::run_acp(
                            exec,
                            agent,
                            wt_root,
                            &prompt,
                            runner,
                            governor,
                            Some((task.to_owned(), Arc::clone(&live))),
                        );
                        if let Some(ways) = &a.sign_in {
                            return login(fail, &a.agent, agent, ways);
                        }
                        for n in a.outcome.output["acp"]["notices"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(serde_json::Value::as_str)
                        {
                            notices.push(format!("{}: {n}", a.agent));
                        }
                        a.outcome
                    }
                };
                (o.run_id, true, o.output)
            } else {
                // The same key as an interrupted attempt resumes it.
                let key = match &interrupted {
                    Some(m) if m.attempt == attempt_no && m.sample == samples.len() => {
                        m.key.clone()
                    }
                    _ => resume::new_key(feature_name, task),
                };
                let _ = resume::save(
                    marker_path,
                    &resume::Marker {
                        key: key.clone(),
                        before: before.0.clone(),
                        head: head_now.clone(),
                        attempt: attempt_no,
                        sample: samples.len(),
                        acceptance: acceptance_saved.clone(),
                        story_before: story_oracle
                            .as_ref()
                            .and_then(|_| serde_json::to_value(&before_run).ok()),
                        run_id: Some(this_run.to_string()),
                        asked: options.asked_now(),
                        run_grants: run_approved.clone(),
                    },
                );
                marker_guard.arm();
                let outcome = match rt.block_on(plane.attempt_keyed(
                    feature_name,
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
                            feature_name,
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
                    let mut report = bare_report(feature_name, task, wt_path, "needs_approval");
                    report.judged.clone_from(&judged_by);
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
                refused_account = outcome
                    .failed_effect
                    .as_ref()
                    .is_some_and(agentplane::journal::FailedEffect::refuses_the_account);
                let mut output = outcome
                    .output
                    .as_ref()
                    .map(|o| o.peek().clone())
                    .unwrap_or(serde_json::Value::Null);
                if !succeeded && output["reason"].as_str().is_none() {
                    if !output.is_object() {
                        output = serde_json::json!({});
                    }
                    // A decision an earlier release recorded is named, with
                    // the fix, not left as a decoding error.
                    let reason = if rt.block_on(plane.holds_a_pre_054_decision(outcome.run_id)) {
                        substrate::PRE_054_DECISION.to_owned()
                    } else if let agentplane::prelude::RunStatus::Exhausted(limit) = &outcome.status
                    {
                        // The plane's ceiling, not the skill's: the same stop.
                        output["status"] = serde_json::json!("budget_exhausted");
                        substrate::exhausted(limit)
                    } else if let agentplane::prelude::RunStatus::Quarantined(why) = &outcome.status
                    {
                        output["status"] = serde_json::json!("quarantined");
                        why.clone()
                    } else {
                        substrate::why_not(&outcome.status)
                    };
                    output["reason"] = serde_json::Value::String(redact(&reason, secret));
                }
                // Stopped by the plane (its budget, a crash): the skill never
                // reported what it used; the journal's turns did.
                if output.get("usage").is_none_or(serde_json::Value::is_null)
                    && let Ok(Some(used)) =
                        rt.block_on(substrate::spent(&s.plane_dir, &outcome.run_id.to_string()))
                {
                    output["usage"] = used;
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
            // A lockfile the attempt changed: its root installed again for
            // it before the checks, so they — and the tick's evidence — are
            // about the lockfile committed.
            if prepares {
                let began = std::time::Instant::now();
                match crate::prepare::recheck(&crate::prepare::Prepare {
                    worktree: wt_root,
                    project,
                    config: worktree_config,
                    runner,
                    state: Some(state),
                }) {
                    Ok(done) if done.is_empty() => {}
                    Ok(done) => {
                        stage(
                            &*live,
                            task,
                            Stage::Prepare,
                            StageState::Passed,
                            Some(format!(
                                "{}: the attempt changed its lockfile",
                                done.join(", ")
                            )),
                            Some(began),
                        );
                    }
                    Err(f) => {
                        stage(
                            &*live,
                            task,
                            Stage::Prepare,
                            StageState::Failed,
                            Some(f.message.clone()),
                            Some(began),
                        );
                        // The attempt is saved and undone like any failed
                        // one: the next run starts from the lockfile that
                        // installs.
                        let reason = redact(&f.message, secret);
                        let saved = attempts::save(
                            wt_root,
                            tmp,
                            feature_name,
                            task,
                            &attempts::Record {
                                run: (!run_id.is_empty()).then_some(run_id.as_str()),
                                outcome: "prepare_failed",
                                reason: &reason,
                                pending: None,
                                usage: output.get("usage"),
                                story_open: &[],
                                own_test: None,
                            },
                        )
                        .and_then(|saved| {
                            worktree::restore(wt_root, tmp, &before).map(|_| saved)
                        });
                        let message = match saved {
                            Ok(saved) => format!(
                                "{} — the attempt is saved as {} and undone",
                                f.message, saved.reference
                            ),
                            Err(e) => format!("{} — saving the attempt failed: {}", f.message, e.0),
                        };
                        return fail(CommandStatus::ConfigError, message)
                            .with_code("PREPARE-FAILED")
                            .with_fix(f.fix)
                            .with_commands(f.commands);
                    }
                }
            }
            let after = match worktree::checkpoint(wt_root, tmp) {
                Ok(c) => c,
                Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
            };
            // What the audit reads: never judged blind.
            let raw_changes = match worktree::diff(wt_root, &before, &after) {
                Ok(c) => c,
                Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
            };
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
            let patch = match worktree::patch(wt_root, &before, &after) {
                Ok(p) => p,
                Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
            };
            // A value copied from the person's secret files, written into a
            // file git would commit: undone, never judged — named, never shown.
            if let Some(name) = crate::include::added_secret(&patch) {
                let _ = worktree::restore(wt_root, tmp, &before);
                return fail(
                    CommandStatus::NeedsHuman,
                    format!(
                        "the attempt's change adds the value of {name}, copied from your checkout ([worktree] include): undone, nothing judged or committed"
                    ),
                )
                .with_code("SECRET-IN-DIFF");
            }
            let findings = attempt_findings(&Audited {
                wt_root,
                policy,
                judged_before: &judged_before,
                raw_changes: &raw_changes,
                patch: &patch,
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
            let quarantined = !succeeded && output["status"] == "quarantined";
            if quarantined {
                output["reason"] = serde_json::Value::String(
                    "the attempt was interrupted while a tool call was running; whether it finished cannot be known, so it was not run again. The attempt is saved and undone; run again to start a fresh attempt".to_owned(),
                );
            }
            let mut attempt_status = if succeeded {
                output["status"].as_str().unwrap_or("failed").to_owned()
            } else if quarantined {
                "interrupted".to_owned()
            } else if output["status"] == "budget_exhausted" {
                "budget_exhausted".to_owned()
            } else {
                "failed".to_owned()
            };
            // An external agent that ended its turn without a change and
            // without saying why stopped, in effect: the person decides
            // (I did it, a hint and a new attempt, or skip).
            if attempt_status == "completed" && changes.is_empty() && output.get("acp").is_some() {
                attempt_status = "blocked".to_owned();
                output["reason"] = serde_json::json!(format!(
                    "{} ended its turn without changing anything",
                    output["acp"]["agent"].as_str().unwrap_or("the agent")
                ));
            }
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
                agent: external::agent_attempt(&output),
                judged: judged_by.clone(),
                ..bare_report(feature_name, task, wt_path, &attempt_status)
            };
            if let Err(e) = record_inline_approvals(
                &s.plane_dir,
                feature_name,
                task,
                &report.tool_calls,
                &options.run_grants,
                this_run.as_str(),
            ) {
                report.notices.push(format!(
                    "the decisions made in this attempt were not recorded ({e}): later attempts ask again"
                ));
            }
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
                let oracle_files: Vec<String> = story_oracle
                    .iter()
                    .flat_map(|o| o.tests.iter().map(|t| t.path.clone()))
                    .collect();
                if let Err(e) = judge_checks(
                    project,
                    runner,
                    (&baseline, &oracle_files),
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
                // 7a. A task of a story: the progress it made on the story's
                //     oracle decides what its tick claims.
                if let Some(o) = &story_oracle {
                    // The oracle judges only as frozen: a test the attempt
                    // changed — even by adding lines — fails it.
                    let touched = super::story::changed(project, o);
                    if !touched.is_empty() {
                        let why = format!(
                            "the story's oracle tests were changed: {}",
                            touched.join(", ")
                        );
                        report.verdict = Some(match report.verdict.take() {
                            Some(speccoder_core::verify::Verdict::Failed { mut reasons }) => {
                                reasons.push(why);
                                speccoder_core::verify::Verdict::Failed { reasons }
                            }
                            _ => speccoder_core::verify::Verdict::Failed { reasons: vec![why] },
                        });
                    }
                    let after = super::story::states(project, o, runner);
                    super::story::judge(
                        o,
                        &before_run.states,
                        &after,
                        judged_by
                            .as_ref()
                            .map(speccoder_core::story::Judged::links)
                            .unwrap_or_default(),
                        matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint)),
                        &mut report,
                    );
                    // The evidence of what it turned: red before, green after.
                    if let Some(speccoder_core::story::Progress::Verified { turned }) =
                        &report.progress
                    {
                        report.acceptance = Some(super::story::evidence(o, turned, &before_run));
                    }
                }
                // 7''. A planning task writes documents of its unit only:
                //      one that changed anything else, or wrote nothing
                //      there, fails — never a planning tick.
                if let Some(dir) = planning.as_ref().filter(|_| {
                    matches!(
                        report.verdict,
                        Some(
                            speccoder_core::verify::Verdict::Verified
                                | speccoder_core::verify::Verdict::RegressionOnly
                        )
                    )
                }) {
                    let inside = format!("{dir}/");
                    let outside: Vec<&str> = report
                        .changes
                        .iter()
                        .filter(|(p, _)| !p.starts_with(&inside))
                        .map(|(p, _)| p.as_str())
                        .collect();
                    let reason = if !outside.is_empty() {
                        Some(format!(
                            "a planning task changed code: {}",
                            outside.join(", ")
                        ))
                    } else if !report.changes.iter().any(|(_, kind)| kind != "deleted") {
                        Some(format!("wrote no planning file under {inside}"))
                    } else {
                        None
                    };
                    if !outside.is_empty() {
                        // Not retried as planning: the task runs again as code.
                        attempt_status = CHANGED_CODE.to_owned();
                        report.attempt_status = CHANGED_CODE.to_owned();
                    }
                    if let Some(r) = reason {
                        report.verdict =
                            Some(speccoder_core::verify::Verdict::Failed { reasons: vec![r] });
                    }
                }
                // 7'. A test file the attempt wrote that the run never
                //     collected is evidence of nothing: it fails the verdict;
                //     one speccoder cannot tell about stops for a person.
                let otherwise_passing = attempt_status == "completed"
                    && findings.is_empty()
                    && matches!(
                        report.verdict,
                        Some(
                            speccoder_core::verify::Verdict::Verified
                                | speccoder_core::verify::Verdict::RegressionOnly
                        )
                    );
                match judge_collection(
                    (project, wt_root, tmp),
                    runner,
                    otherwise_passing,
                    &mut report,
                    &*live,
                    task,
                ) {
                    Some(Collected::Unknown(why)) => {
                        attempt_status = "collection_unknown".to_owned();
                        report.verdict = None;
                        report.reason = Some(why);
                    }
                    // A listing changed the tree: never committed, never retried.
                    Some(Collected::Changed(finding)) => {
                        report.integrity.push(finding.clone());
                        report.verdict = Some(speccoder_core::verify::Verdict::Failed {
                            reasons: vec![finding],
                        });
                        attempt_status = "integrity".to_owned();
                    }
                    None => {}
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
            // A task's own held-out set, or — at its story's checkpoint —
            // the story oracle's sealed one.
            let held = acceptance
                .as_ref()
                .and_then(|a| a.held_out.clone())
                .map(|sha| (held_out_store.clone(), sha))
                .or_else(|| {
                    story_oracle
                        .as_ref()
                        .filter(|_| {
                            matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint))
                        })
                        .and_then(|o| o.held_out.as_ref())
                        .map(|h| {
                            (
                                crate::held_out::sealed_dir(&s.plane_dir, feature_name, &h.key),
                                h.manifest_sha256.clone(),
                            )
                        })
                });
            // The story's held-out set at its checkpoint: the report and the
            // tick name it as a task's own set is named — or why there is none.
            let at_checkpoint =
                matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint));
            if at_checkpoint
                && visibly_passed
                && held.is_none()
                && let Some(summary) = story_oracle
                    .as_ref()
                    .and_then(|o| o.held_out_summary.as_ref())
            {
                let entry = report
                    .acceptance
                    .get_or_insert_with(|| serde_json::json!({ "source": "authored" }));
                entry["held_out"] = match summary.state.as_str() {
                    "off" => serde_json::json!({ "state": "off" }),
                    _ => serde_json::json!({
                        "state": "absent",
                        "reason": summary.reason,
                        "discarded": summary
                            .discarded
                            .iter()
                            .map(|r| serde_json::json!({ "reason": r }))
                            .collect::<Vec<_>>(),
                    }),
                };
            }
            if acceptance
                .as_ref()
                .and_then(|a| a.held_out.as_ref())
                .is_none()
                && let (Some((_, sha)), Some(o)) = (&held, &story_oracle)
                && visibly_passed
            {
                let entry = report.acceptance.get_or_insert_with(
                    || serde_json::json!({ "source": "authored", "oracle": o.commit }),
                );
                entry["held_out"] = serde_json::json!({
                    "state": "on",
                    "manifest": format!("sha256:{sha}"),
                    "kept": o.held_out.as_ref().map_or(0, |h| h.count),
                });
            }
            if let Some((store, sha)) = held.filter(|_| visibly_passed) {
                let patch = worktree::patch(wt_root, &before, &after).unwrap_or_default();
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
                    &store, &sha, &scenarios, &known, wt_root, project, tmp, &before, &patch,
                    runner, policy,
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
                let keep = crate::strength::dir(&s.plane_dir, feature_name, task);
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
                let patch = worktree::patch(wt_root, &before, &after).unwrap_or_default();
                let scratch = runner.scratch().unwrap_or_else(|| tmp.clone());
                if let Some(result) = mutation::run(m, project, &patch, &scratch, runner) {
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
            let mut d =
                speccoder_core::retry::digest(attempt_no, &reasons, verification, &discarded);
            d.uncollected = report
                .collection
                .as_ref()
                .map(speccoder_core::collection::Collection::retry_lines)
                .unwrap_or_default();
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
            if let Err(e) = worktree::restore(wt_root, tmp, &before) {
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
        let end = match worktree::checkpoint(wt_root, tmp) {
            Ok(c) => c,
            Err(e) => return fail(CommandStatus::GitPrecondition, e.0),
        };
        let patch = worktree::patch(wt_root, &before, &end).unwrap_or_default();
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
                if let Err(e) = worktree::restore(wt_root, tmp, &end) {
                    return fail(CommandStatus::GitPrecondition, e.0);
                }
            }
            chosen.samples = summaries;
            break (chosen, status);
        }
        if let Err(e) = worktree::restore(wt_root, tmp, &before) {
            return fail(CommandStatus::GitPrecondition, e.0);
        }
    };
    report.usage = super::commit::add_usage(usage.as_ref(), oracle_spent.as_ref());
    // What reached the implementer, by digest: the bundle's parts and the
    // test layout (constitution VIII).
    {
        use speccoder_core::context::ContextKind as K;
        let part = |k: &str| {
            bundle_value[k]
                .as_str()
                .unwrap_or_default()
                .as_bytes()
                .to_vec()
        };
        let mut m = crate::context::Manifest::new();
        m.push(
            K::Constitution,
            "worktree",
            "/bundle/constitution",
            &part("constitution"),
        )
        .push(K::Plan, "worktree", "/bundle/plan", &part("plan"))
        .push(K::Story, "worktree", "/bundle/story", &part("story"))
        .push(
            K::Task,
            "worktree",
            "/bundle/task",
            serde_json::to_string(&bundle_value["task"])
                .unwrap_or_default()
                .as_bytes(),
        )
        .push(
            K::TestLayout,
            "worktree",
            "/test_layout",
            test_layout_text.as_bytes(),
        );
        report.context = Some(m.finish());
    }
    // Scope: what the attempt changed beyond the paths its task names.
    report.scope = parsed_task.as_ref().map(|t| scope_of(&report, &t.title));

    // 7c. Acceptance strength: the set a verified verdict
    //     rests on, run against wrong and permitted candidates. Judge-side;
    //     the implementer never sees a variant, a survivor or a finding.
    let strength_dir = crate::strength::dir(&s.plane_dir, feature_name, task);
    let mut attempt_status = attempt_status;
    let verified = attempt_status == "completed"
        && matches!(
            report.verdict,
            Some(speccoder_core::verify::Verdict::Verified)
        );
    // The commands a verified verdict rests on: the task's own acceptance
    // tests, or the story oracle's tests of the scenarios it turned.
    let visible: Vec<Vec<String>> = match acceptance
        .as_ref()
        .filter(|a| a.red)
        .and_then(|a| a.argv.clone())
    {
        Some(argv) => vec![argv],
        None => report
            .acceptance
            .as_ref()
            .filter(|a| a["oracle"].is_string() && a["red"] == true)
            .and_then(|a| a["tests"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|t| serde_json::from_value::<Vec<String>>(t["argv"].clone()).ok())
            .collect(),
    };
    let visible_argv = visible.first().cloned();
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
        let fin = worktree::checkpoint(wt_root, tmp);
        let patch = fin
            .as_ref()
            .ok()
            .and_then(|now| worktree::patch(wt_root, start, now).ok())
            .unwrap_or_default();
        let mut notes = Vec::new();
        // The task's own held-out set, or at its story's checkpoint the
        // story oracle's sealed one.
        let held_set: Option<(std::path::PathBuf, String)> = acceptance
            .as_ref()
            .and_then(|a| a.held_out.clone())
            .map(|sha| (held_out_store.clone(), sha))
            .or_else(|| {
                story_oracle
                    .as_ref()
                    .filter(|_| {
                        matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint))
                    })
                    .and_then(|o| o.held_out.as_ref())
                    .map(|h| {
                        (
                            crate::held_out::sealed_dir(&s.plane_dir, feature_name, &h.key),
                            h.manifest_sha256.clone(),
                        )
                    })
            });
        let held: Vec<(crate::held_out::HeldOutFile, String)> = match held_set {
            None => Vec::new(),
            Some((store, sha)) => match crate::held_out::load_verified(&store, &sha) {
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
        let mut commands = if visible.is_empty() {
            vec![argv]
        } else {
            visible.clone()
        };
        let measure = |commands: &[Vec<String>]| {
            crate::strength::measure(
                &crate::strength::Setup {
                    wt_root,
                    project,
                    tmp,
                    runner,
                    policy,
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
                        bundle_value,
                        changed,
                        &crate::mutation::source_patch(&patch),
                        run_config.strength.near_miss,
                        run_config.strength.permitted,
                    );
                    match rt.block_on(plane.near_miss(feature_name, task, input)) {
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
                                        crate::substrate::why_not(&o.status)
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
                rt,
                plane,
                project,
                worktree: wt_root,
                tmp,
                policy,
                feature: feature_name,
                task,
                bundle: bundle_value.clone(),
                scenarios: &scenarios,
                instructions,
                runner,
                layout: Some(&test_layout),
                told: &told,
                related: &related,
                oracle: false,
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
        let patch = worktree::checkpoint(wt_root, tmp)
            .and_then(|now| worktree::patch(wt_root, start, &now))
            .unwrap_or_default();
        let calls: Vec<serde_json::Value> = report
            .tool_calls
            .iter()
            .map(|c| {
                let tool = c["tool"].as_str().unwrap_or_default();
                serde_json::json!({ "tool": tool, "target": progress::target(tool, &c["arguments"]), "decision": c["decision"] })
            })
            .collect();
        let input = substrate::monitor::input(bundle_value, &patch, &calls, &frozen);
        let (monitor, suspicion) = monitor_attempt(rt, plane, feature_name, task, input, secret);
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
                .and_then(|s| s.check_before_tick(policy).err())
        });
    if let Some(e) = &in_place_stop {
        report.reason = Some(e.line());
        report.verdict = None;
        // The saved attempt names the cause: `intent_changed`, `judge_changed`, …
        attempt_status = e.code.to_lowercase().replace('-', "_");
        report.attempt_status.clone_from(&attempt_status);
    }
    let passed = verdict_passed && !suspected && in_place_stop.is_none();
    // 8a. Why a regression-only tick is one: decided by code from the run's
    //     facts; already-passing tests are searched for the tick that made
    //     them pass (never changing the verdict, never a model call).
    if passed
        && matches!(
            report.verdict,
            Some(speccoder_core::verify::Verdict::RegressionOnly)
        )
    {
        use speccoder_core::regression::{Regression, RegressionFacts, RegressionReason, decide};
        let facts = RegressionFacts {
            acceptance_on: run_config.acceptance,
            // A bug's requirement is its story.
            story: acceptance_story
                .clone()
                .or_else(|| bug_fix.as_ref().map(|f| f.bug.requirement.clone()))
                .or_else(|| parsed_task.as_ref().and_then(|t| t.story.clone())),
            scenarios: scenarios.len(),
            skip: acceptance.as_ref().and_then(|a| a.skip_cause.clone()),
            // Red first but green only on a re-run: the evidence was flaky.
            red_class: acceptance.as_ref().and_then(|a| {
                if a.red
                    && report
                        .acceptance
                        .as_ref()
                        .is_some_and(|r| r["green_flaky"] == true)
                {
                    Some(speccoder_core::verify::RedClass::Flaky)
                } else {
                    a.red_class
                }
            }),
        };
        let reason = if planning.is_some() {
            RegressionReason::PlanningWork
        } else if in_story && oracle_empty {
            RegressionReason::AuthorWroteNone
        } else if in_story {
            // A task of a story: what its progress on the oracle says.
            match &report.progress {
                // Green before any work of the story: built earlier.
                Some(
                    speccoder_core::story::Progress::Closed
                    | speccoder_core::story::Progress::InProgress,
                ) if story_oracle
                    .as_ref()
                    .is_some_and(super::story::StoryOracle::green_from_the_start) =>
                {
                    RegressionReason::AlreadyPassing
                }
                Some(speccoder_core::story::Progress::Closed) => RegressionReason::StoryVerified,
                _ if super::story::unchecked(&report) => RegressionReason::RegressionUnchecked,
                _ => RegressionReason::InProgress,
            }
        } else {
            decide(&facts)
        };
        report.regression = Some(Regression::of(reason));
    }
    let recorded_from = report.integrity.len();
    if passed && let (Some(t), Some(file)) = (&parsed_task, &tasks_file) {
        let began = std::time::Instant::now();
        stage(&*live, task, Stage::Commit, StageState::Started, None, None);
        let message = tick_message(&TickFacts {
            feature: feature_name,
            task,
            title: &t.title,
            report: &report,
            scenarios: &story_scenarios,
            red: acceptance
                .as_ref()
                .and_then(|a| a.red_class.filter(|c| a.red && c.is_red()))
                .or_else(|| story_red(&report)),
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
                // A set frozen at review: the one pre-frozen trailer form.
                .or_else(|| review_frozen.as_ref().map(|(r, m)| r.trailer(m)))
                .unwrap_or_default()
                + &ruling_trailers(&ruled),
        });
        match &options.session {
            // Worktree: tick, then commit the tick with the evidence.
            None => {
                match format.tick(file, task, &t.line_hash) {
                    Ok(()) => report.ticked = true,
                    Err(e) => report.integrity.push(format!("tick refused: {e}")),
                }
                if report.ticked && run_config.commit_per_task {
                    match commit_attempt(wt_root, &message) {
                        Ok(id) => report.commit = Some(id),
                        Err(e) => report.integrity.push(format!("commit failed: {e}")),
                    }
                }
            }
            // In place: evidence first, then the tick; a git-ignored
            // tasks.md is never in the commit, so its tick is local.
            Some(session) => {
                let digest =
                    crate::in_place::intent_digest(project, policy, format.messages().tasks_file);
                if let Ok(mut s) = session.lock() {
                    let done = s.commit_and_tick(
                        plane_dir,
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
                repo: wt_root,
                config: config_text,
                attest: &run_config.attest,
                format: &format!("{:?}", format.kind()).to_lowercase(),
                feature: feature_name,
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
                    "kind": if bug_candidate.is_some() { "candidate".to_owned() } else { executor.as_ref().map_or_else(|| "native".to_owned(), |e| e.kind.name().to_owned()) },
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
            feature: feature_name,
            task,
            title: &n.title,
            report: &report,
            scenarios: &story_scenarios,
            red: acceptance
                .as_ref()
                .and_then(|a| a.red_class.filter(|c| a.red && c.is_red()))
                .or_else(|| story_red(&report)),
            checklists_overridden: false,
            extra: bug_fix
                .as_ref()
                .map(|f| {
                    super::bug::trailers(f, &verdict, &crate::strength::rulings(&strength_dir))
                })
                .unwrap_or_default()
                + &ruling_trailers(&ruled),
        });
        match commit_attempt(wt_root, &message) {
            Ok(id) => report.commit = Some(id),
            Err(e) => report.integrity.push(format!("commit failed: {e}")),
        }
        if let Some(commit) = report.commit.clone().filter(|_| run_config.attest.enabled) {
            let sandboxed = sandbox_config.mode != sandbox::Mode::Off && sandbox::backend().is_ok();
            let tick = crate::attest::Tick {
                repo: wt_root,
                config: config_text,
                attest: &run_config.attest,
                format: &format!("{:?}", format.kind()).to_lowercase(),
                feature: feature_name,
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

    // A passing attempt whose tick or commit failed is not done: saved and
    // undone below, the person told how to bring it back.
    let unrecorded = report.integrity[recorded_from..]
        .iter()
        .find(|m| m.starts_with("tick refused") || m.starts_with("commit failed"))
        .cloned();
    let passed = passed && unrecorded.is_none();
    if let Some(why) = unrecorded {
        attempt_status = "tick_failed".to_owned();
        report.ticked = false;
        report.reason = Some(why);
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
    // A ticked task's rulings are acted on: its tests were written again.
    if report.ticked && !ruled.is_empty() {
        let _ = crate::acceptance_at::acted_on_task(&s.plane_dir, feature_name, task);
    }
    // The task's own test at fault, when the stop names one (offered a ruling).
    let mut conflict: Option<String> = None;
    // 9. An attempt that did not pass is saved under an attempt ref and
    //    undone: the next attempt starts where this one did.
    if !passed && attempt_status != "over_strict" && attempt_status != "repro_weak" {
        let (outcome, reason) = saved_outcome(&report, &attempt_status, suspected);
        let run = (!report.run_id.is_empty()).then_some(report.run_id.as_str());
        let reason = redact(&reason, secret);
        // The story's last task stopped with scenarios red: each can be
        // ruled wrong from the stop.
        // So can those of an oracle test the implementer named wrong
        // (its stop, `wrong_test`: speccoder's words name the test).
        let story_open: Vec<String> = match &report.progress {
            Some(speccoder_core::story::Progress::StoryOpen { scenarios }) => scenarios.clone(),
            _ if outcome == "blocked" => story_oracle
                .as_ref()
                .and_then(|o| {
                    let named = frozen_test_named(report.reason.as_deref()?)?;
                    o.tests
                        .iter()
                        .find(|t| t.path == named)
                        .map(|t| t.scenarios.clone())
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        // A task outside a story whose own test failed — or that the
        // implementer called wrong — can have it ruled wrong from the stop.
        let own_files: Vec<String> = report
            .acceptance
            .as_ref()
            // Only where the ruling takes no scenario: outside any story.
            .filter(|_| story_oracle.is_none() && story_key.is_none())
            .and_then(|a| a["files"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect();
        let own_failed = report.verification.as_ref().is_some_and(|v| {
            v.checks
                .iter()
                .any(|c| c.name == "acceptance" && c.status.is_failure())
        });
        let named = (outcome == "blocked")
            .then(|| frozen_test_named(report.reason.as_deref()?))
            .flatten();
        let own_test = named
            .filter(|n| own_files.contains(n))
            .or_else(|| own_failed.then(|| own_files.first().cloned()).flatten());
        // Everything the attempt cost, as a tick would count it.
        let spent = super::commit::all_usage(&report);
        let record = attempts::Record {
            run,
            outcome: &outcome,
            reason: &reason,
            pending: report.pending.as_ref(),
            usage: spent.as_ref(),
            story_open: &story_open,
            own_test: own_test.as_deref(),
        };
        conflict.clone_from(&own_test);
        match attempts::save(wt_root, tmp, feature_name, task, &record)
            .and_then(|saved| worktree::restore(wt_root, tmp, start).map(|_| saved))
        {
            Ok(saved) => report.saved_attempt = Some(saved.reference),
            Err(e) => report
                .integrity
                .push(format!("saving the attempt failed: {}", e.0)),
        }
    }

    let (status, mut message) = status_and_message(&report, &attempt_status, suspected);
    // Bedrock's refusals name where the call went: a model the region does
    // not offer reads the same as one the account may not use.
    if status == CommandStatus::RunFailed
        && let Some(region) = s.model.region.as_deref()
        && report
            .reason
            .as_deref()
            .and_then(substrate::provider_refusal)
            .is_some()
    {
        message.push_str(&format!(
            " — Amazon Bedrock, region {region}, model {}",
            s.model.model
        ));
    }
    // The provider refused the account: no retry helps, and for Bedrock the
    // first fix renews the credentials the calls were signed with.
    let refusal = |result: CommandResult| {
        if !result.has_explicit_code()
            && refused_account
            && result.status() == CommandStatus::RunFailed
        {
            result.with_code("PROVIDER-REFUSED").with_fix(
                substrate::model_config::provider_refused_fix(
                    &s.model,
                    crate::aws::detect_here().as_ref(),
                    feature,
                ),
            )
        } else {
            result
        }
    };
    // A quick change or bug fix that grew: its offer, said once it holds.
    let offered = crate::ladder::offer_for(
        path,
        &worktree::project_state_dir(state, path),
        &format!("{feature_name}/{task}"),
    )
    .map(|o| progress::Event::LadderOffered {
        unit: o.unit,
        task: o.task,
        signals: o.signals.into_iter().map(|s| s.id).collect(),
    });
    if let Some(fix) = bug_fix.as_mut() {
        let done = refusal(super::bug::finished(
            &bug_ctx,
            fix,
            report,
            &attempt_status,
            passed,
            status,
            redact(&message, secret),
        ));
        if let Some(e) = &offered {
            live.event(e);
        }
        return done;
    }
    live.event(&finished_event(
        task,
        &report,
        &attempt_status,
        suspected,
        redact(&message, secret),
    ));
    if let Some(e) = &offered {
        live.event(e);
    }
    // The story's last task left scenarios red: named, with a ruling each.
    let code = collection_code(&report, &attempt_status).or_else(|| {
        matches!(
            report.progress,
            Some(speccoder_core::story::Progress::StoryOpen { .. })
        )
        .then_some("STORY-OPEN")
    });
    // A red scenario whose frozen test can never pass, and why.
    let never: Vec<(String, &str)> = match &report.progress {
        Some(
            speccoder_core::story::Progress::StoryOpen { scenarios }
            | speccoder_core::story::Progress::LinksOpen { scenarios },
        ) => scenarios
            .iter()
            .filter_map(|id| {
                let t = story_oracle
                    .as_ref()?
                    .tests
                    .iter()
                    .find(|t| t.scenarios.contains(id))?;
                let text = std::fs::read_to_string(project.join(&t.path)).ok()?;
                crate::integrity::never_passes(&t.path, &text).map(|why| (id.clone(), why))
            })
            .collect(),
        _ => Vec::new(),
    };
    for (id, why) in &never {
        message.push_str(&format!(" — {id}'s test can never pass: {why}"));
    }
    let rule = |id: &str| {
        let f = feature.map_or_else(String::new, |f| format!(" --feature {f}"));
        let why = never.iter().find(|(s, _)| s == id).map_or("…", |(_, w)| *w);
        format!("speccoder task rule {task}{f} --tests-wrong --scenario {id} --reason \"{why}\"")
    };
    let nudge = || {
        let f = feature.map_or_else(String::new, |f| format!(" --feature {f}"));
        format!("speccoder task nudge {task}{f} --note \"…\"")
    };
    let story_open_fix: Vec<String> = match &report.progress {
        // The rulings first: a toast shows the first line.
        Some(speccoder_core::story::Progress::StoryOpen { scenarios }) if !report.ticked => {
            scenarios
                .iter()
                .map(|id| rule(id))
                .chain(std::iter::once(nudge()))
                .collect()
        }
        // The scenarios the task names are still red: a nudge first, or
        // each one's test ruled wrong — first when it can never pass.
        Some(speccoder_core::story::Progress::LinksOpen { scenarios }) if !report.ticked => {
            let rules = scenarios.iter().map(|id| rule(id));
            if never.is_empty() {
                std::iter::once(nudge()).chain(rules).collect()
            } else {
                rules.chain(std::iter::once(nudge())).collect()
            }
        }
        // The task's own test failed, or was called wrong: rule it.
        _ => match &conflict {
            Some(path) if !report.ticked => {
                let f = feature.map_or_else(String::new, |f| format!(" --feature {f}"));
                message.push_str(&format!(
                    " — its own test `{path}` failed: if it asserts what the spec does not say, rule it wrong"
                ));
                vec![
                    format!("speccoder task rule {task}{f} --tests-wrong --reason \"…\""),
                    format!("speccoder task nudge {task}{f} --note \"…\""),
                ]
            }
            _ => Vec::new(),
        },
    };
    let result = CommandResult::new("run", status, &redact(&message, secret));
    let result = if story_open_fix.is_empty() {
        result
    } else {
        result.with_fix(story_open_fix)
    };
    match code {
        Some(c) => result.with_code(c),
        None => refusal(result),
    }
    .with_data(CommandData::Run(Box::new(report)))
}

/// The code of a run the collection check decided: a file that never ran
/// (`TESTS-NOT-COLLECTED`), or one speccoder cannot tell about.
fn collection_code(
    report: &speccoder_core::RunReport,
    attempt_status: &str,
) -> Option<&'static str> {
    if attempt_status == "collection_unknown" {
        return Some("TESTS-COLLECTION-UNKNOWN");
    }
    match &report.verdict {
        Some(speccoder_core::verify::Verdict::Failed { reasons })
            if reasons
                .first()
                .is_some_and(|r| r.starts_with(speccoder_core::collection::REASON_PREFIX)) =>
        {
            Some("TESTS-NOT-COLLECTED")
        }
        _ => None,
    }
}

/// Judges the attempt's changed test files against what the run collected:
/// a file no test ran from fails the verdict (its reason
/// first); for an otherwise passing attempt, a file speccoder cannot tell
/// about returns the stop's message. Listings run only then.
/// What the collection check came to beyond the verdict's reasons.
enum Collected {
    /// A file speccoder cannot tell about: the stop's message.
    Unknown(String),
    /// A listing changed the worktree: the integrity finding.
    Changed(String),
}

fn judge_collection(
    dirs: (&Path, &Path, &Path),
    runner: &sandbox::Runner,
    otherwise_passing: bool,
    report: &mut speccoder_core::RunReport,
    live: &dyn progress::Progress,
    task: &str,
) -> Option<Collected> {
    let (project, wt_root, tmp) = dirs;
    // Listings run the project's code: the tree they leave must be the one judged.
    let before = worktree::checkpoint(wt_root, tmp).ok();
    let checks = std::fs::read_to_string(project.join(crate::verify::config::FILE))
        .ok()
        .and_then(|t| crate::verify::config::parse(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.tier == crate::verify::config::Tier::Fast)
        .collect::<Vec<_>>();
    let results = report
        .verification
        .as_ref()
        .map(|v| v.checks.clone())
        .unwrap_or_default();
    let c = crate::collection::judge(&crate::collection::Input {
        root: project,
        changes: &report.changes,
        checks: &checks,
        results: &results,
        runner,
        listings: otherwise_passing,
    })?;
    if !c.listing.is_empty()
        && let Some(before) = &before
    {
        let changed: Vec<String> = worktree::checkpoint(wt_root, tmp)
            .and_then(|after| worktree::diff(wt_root, before, &after))
            .map(|d| {
                d.into_iter()
                    .map(|c| c.path)
                    .filter(|p| !speccoder_core::collection::is_build_or_dependency(p))
                    .collect()
            })
            .unwrap_or_else(|e| vec![format!("(the tree could not be read: {})", e.0)]);
        if !changed.is_empty() {
            report.collection = Some(c);
            return Some(Collected::Changed(format!(
                "integrity: a test listing changed the worktree: {}",
                changed.join(", ")
            )));
        }
    }
    live.event(&progress::Event::CollectionChecked {
        task: task.to_owned(),
        files: c
            .files
            .iter()
            .map(|f| progress::CollectionFile {
                path: f.path.clone(),
                state: f.state,
                decided_by: f.decided_by,
            })
            .collect(),
    });
    let never = c.reasons();
    let unknown: Vec<String> = c
        .unknown()
        .map(|f| {
            format!(
                "speccoder cannot tell whether the runner ran `{}` ({})",
                f.path,
                f.why.as_deref().unwrap_or("no report or listing names it")
            )
        })
        .collect();
    report.collection = Some(c);
    if !never.is_empty() {
        let mut reasons = never;
        if let Some(speccoder_core::verify::Verdict::Failed { reasons: more }) =
            report.verdict.take()
        {
            reasons.extend(more);
        }
        report.verdict = Some(speccoder_core::verify::Verdict::Failed { reasons });
        return None;
    }
    (otherwise_passing && !unknown.is_empty()).then(|| Collected::Unknown(unknown.join("; ")))
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
/// What the sandbox denied it comes last: often the cause itself.
#[must_use]
pub fn broken(c: &speccoder_core::verify::CheckResult) -> Option<String> {
    let why = broken_why(c)?;
    Some(match c.sandbox.denied.first() {
        Some(first) => {
            let others = c.sandbox.denied.len() - 1 + c.sandbox.more;
            let more = if others > 0 {
                format!(" (and {others} more)")
            } else {
                String::new()
            };
            format!("{why} — the sandbox denied {first}{more}")
        }
        None => why,
    })
}

fn broken_why(c: &speccoder_core::verify::CheckResult) -> Option<String> {
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

/// The task stops for the agent's sign-in: its own ways, a command one
/// names as a step of the person's own (a terminal of theirs: it reads the
/// keyboard). Nothing was attempted.
fn login(
    fail: impl FnOnce(CommandStatus, String) -> CommandResult,
    shown: &str,
    agent: &crate::acp::agent::Agent,
    ways: &[crate::acp::session::SignIn],
) -> CommandResult {
    let words: Vec<String> = ways
        .iter()
        .filter(|w| w.command.is_none())
        .map(|w| {
            format!(
                "→ {}",
                w.description.clone().unwrap_or_else(|| w.name.clone())
            )
        })
        .collect();
    let mut commands: Vec<Vec<String>> = ways
        .iter()
        .filter_map(|w| w.command.as_deref())
        .map(|c| c.split_whitespace().map(str::to_owned).collect())
        .collect();
    if words.is_empty()
        && commands.is_empty()
        && let Some(p) = agent.preset()
    {
        commands.push(p.login.split_whitespace().map(str::to_owned).collect());
    }
    commands.dedup();
    let argvs: Vec<speccoder_core::step::Argvs> = commands.into_iter().map(|c| vec![c]).collect();
    let mut fix: Vec<String> = argvs
        .iter()
        .map(|a| speccoder_core::step::render(a))
        .collect();
    fix.extend(words);
    fix.push("speccoder doctor".to_owned());
    fail(
        CommandStatus::NeedsHuman,
        format!("{shown} needs signing in before it can make the attempt; nothing was attempted"),
    )
    .with_code("EXECUTOR-LOGIN")
    .with_fix(fix)
    .with_commands(argvs)
}

/// The red class of a story task's turned scenarios, from the evidence
/// the oracle gave it.
fn story_red(report: &speccoder_core::RunReport) -> Option<speccoder_core::verify::RedClass> {
    let a = report
        .acceptance
        .as_ref()
        .filter(|a| a["oracle"].is_string())?;
    serde_json::from_value::<speccoder_core::verify::RedClass>(a["red_class"].clone())
        .ok()
        .filter(|c| c.is_red())
}

/// The bundle the implementer is given: with the story's oracle, when the
/// task is judged by one.
fn with_oracle(bundle: &serde_json::Value, oracle: &serde_json::Value) -> serde_json::Value {
    let mut b = bundle.clone();
    if !oracle.is_null() {
        b["oracle"] = oracle.clone();
    }
    b
}

/// The tick's record of a person's rulings on the task's tests:
/// `Ruling: tests-wrong by <who> — <why>`.
fn ruling_trailers(ruled: &[crate::acceptance_at::Ruling]) -> String {
    ruled
        .iter()
        .map(|r| {
            format!(
                "Ruling: tests-wrong by {} — {}\n",
                r.by,
                r.reason.replace('\n', " ")
            )
        })
        .collect()
}

/// The frozen test a `wrong_test` stop names (speccoder's own words:
/// "frozen test `tests/x.py`").
fn frozen_test_named(reason: &str) -> Option<String> {
    Some(
        reason
            .split("frozen test `")
            .nth(1)?
            .split('`')
            .next()?
            .trim_start_matches("./")
            .to_owned(),
    )
}
