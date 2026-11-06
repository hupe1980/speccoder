//! Phases 5–7a of a task run: the attempt. From one checkpoint the model
//! (or an external agent) works, the verifier judges, and a failed verdict
//! is retried with its digest; with `best_of`, independent samples are
//! judged and the best by evidence kept. What reached the implementer and
//! what it changed beyond its task are recorded on the report.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

use super::open::Opened;
use super::ready::Ready;
use super::tests::TestsFirst;
use super::*;
use crate::pipeline::*;

/// The attempt that was kept, and what the judge-side steps need of it.
pub(super) struct Attempted {
    pub(super) report: RunReport,
    pub(super) attempt_status: String,
    pub(super) before: crate::worktree::Checkpoint,
    pub(super) frozen: Vec<String>,
    pub(super) bug_weak: Option<speccoder_core::strength::Permitted>,
    pub(super) refused_account: bool,
}

/// Phases 5–7a. `Err`: the run ends here, with what a person sees.
#[allow(clippy::too_many_lines)] // one attempt loop: retries and samples
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    path: &Path,
    o: &Opened,
    r: &Ready,
    t: &mut TestsFirst,
    carried: &mut Carried,
    task: &str,
    state: &Path,
    secret: Option<&str>,
    options: &RunOptions,
) -> Result<Attempted, Box<CommandResult>> {
    let Opened {
        s,
        feature_name,
        live,
        wt_path,
        project,
        wt_root,
        tmp,
        marker_path,
        autopilot,
        interrupted,
        format,
        parsed_task,
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
    } = o;
    let (format, live) = (*format, Arc::clone(live));
    let (autopilot, contained) = (*autopilot, *contained);
    let Ready {
        rt,
        bundle_value,
        instructions,
        connected,
        present,
        plane,
        copied_files,
        prepares,
        ..
    } = r;
    let TestsFirst {
        ref mut bug_fix,
        ref bug_candidate,
        ref test_layout,
        ref test_layout_text,
        ref judged_by,
        ref scenarios,
        ref story_oracle,
        ref oracle_note,
        ref before_run,
        ref oracle_spent,
        ref acceptance,
        ref held_out_store,
        ..
    } = *t;
    let Carried {
        ref head_now,
        ref marker_guard,
        ref set_aside,
        ref baseline,
        ref mut notices,
        ..
    } = *carried;
    let approver = options.approver.clone();
    let bug_ctx = super::tests::bug_ctx(o, r, task);
    let attempt_instructions: &[crate::instructions::Instruction] = if executor.is_none() {
        instructions
    } else {
        &[]
    };
    let fail = |status: CommandStatus, m: String| super::ended(&*live, task, secret, status, m);
    // 5. Checkpoint, attempt, checkpoint — again after a failed verdict,
    //    from the same checkpoint, with the verifier's digest.
    let before = match &interrupted {
        Some(m) => worktree::Checkpoint(m.before.clone()),
        None => match worktree::checkpoint(wt_root, tmp) {
            Ok(c) => c,
            Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
        },
    };
    let acceptance_saved = acceptance
        .as_ref()
        .and_then(|a| serde_json::to_value(a).ok());
    // Tests that exist now are frozen for this attempt.
    let frozen: Vec<String> = match worktree::files(wt_root, &before) {
        Ok(f) => f,
        Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
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
                        return Err(Box::new(fail(CommandStatus::Error, e)));
                    }
                    (d.for_run(feature_name, task, this_run.as_str()), once)
                }
                Err(e) => return Err(Box::new(fail(CommandStatus::ConfigError, e))),
            };
            // This run's grants as they stand now: journaled with the input.
            let run_approved: Vec<crate::tools::policy::Object> = options
                .run_grants
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            // Everything the attempt's decisions depend on is in its journaled input.
            let input = substrate::attempt::input(
                with_oracle(bundle_value, oracle_note),
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
                    Err(stop) => return Err(Box::new(*stop)),
                }
            } else if let Some(exec) = &executor {
                let allow: Vec<Vec<String>> = check_argvs
                    .iter()
                    .chain(&run_config.allow)
                    .cloned()
                    .collect();
                let prompt = external::prompt_in(
                    format.wording(),
                    &with_oracle(bundle_value, oracle_note),
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
                            Err(e) => {
                                return Err(Box::new(fail(
                                    CommandStatus::RunFailed,
                                    e.to_string(),
                                )));
                            }
                        };
                        let tools = match tools::WorkspaceTools::new(wt_root, policy.clone()) {
                            Ok(t) => t.with_runner(runner.clone()),
                            Err(e) => {
                                return Err(Box::new(fail(
                                    CommandStatus::Error,
                                    format!("{}: {e}", wt_root.display()),
                                )));
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
                            return Err(Box::new(login(fail, &a.agent, agent, ways)));
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
                            .and_then(|_| serde_json::to_value(before_run).ok()),
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
                    Err(e) => return Err(Box::new(fail(CommandStatus::RunFailed, e.0))),
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
                    Err(e) => return Err(Box::new(fail(CommandStatus::RunFailed, e.0))),
                };
                if let Some(task_row) = waiting {
                    // The attempt waits as it is: its tree and its resume
                    // marker stay, so the next run continues it.
                    marker_guard.keep();
                    let action = &task_row.justification.proposed_action;
                    let mut report = bare_report(feature_name, task, wt_path, "needs_approval");
                    report.judged.clone_from(judged_by);
                    report.run_id = outcome.run_id.to_string();
                    report.notices = std::mem::take(notices);
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
                    return Err(Box::new(
                        CommandResult::new("run", status, &redact(&message, secret))
                            .with_data(CommandData::Run(Box::new(report))),
                    ));
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
            if *prepares {
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
                        .and_then(|saved| worktree::restore(wt_root, tmp, &before).map(|_| saved));
                        let message = match saved {
                            Ok(saved) => format!(
                                "{} — the attempt is saved as {} and undone",
                                f.message, saved.reference
                            ),
                            Err(e) => format!("{} — saving the attempt failed: {}", f.message, e.0),
                        };
                        return Err(Box::new(
                            fail(CommandStatus::ConfigError, message)
                                .with_code("PREPARE-FAILED")
                                .with_fix(f.fix)
                                .with_commands(f.commands),
                        ));
                    }
                }
            }
            let after = match worktree::checkpoint(wt_root, tmp) {
                Ok(c) => c,
                Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
            };
            // What the audit reads: never judged blind.
            let raw_changes = match worktree::diff(wt_root, &before, &after) {
                Ok(c) => c,
                Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
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
                Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
            };
            // A value copied from the person's secret files, written into a
            // file git would commit: undone, never judged — named, never shown.
            if let Some(name) = crate::include::added_secret(&patch) {
                let _ = worktree::restore(wt_root, tmp, &before);
                return Err(Box::new(fail(
                    CommandStatus::NeedsHuman,
                    format!(
                        "the attempt's change adds the value of {name}, copied from your checkout ([worktree] include): undone, nothing judged or committed"
                    ),
                )
                .with_code("SECRET-IN-DIFF")));
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
                    (baseline, &oracle_files),
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
                    return Err(Box::new(fail(
                        CommandStatus::ConfigError,
                        format!("verification: {e}"),
                    )));
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
                        report.acceptance = Some(super::story::evidence(o, turned, before_run));
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
                    &store, &sha, scenarios, &known, wt_root, project, tmp, &before, &patch,
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
                    baseline,
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
            Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
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
                    return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0)));
                }
            }
            chosen.samples = summaries;
            break (chosen, status);
        }
        if let Err(e) = worktree::restore(wt_root, tmp, &before) {
            return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0)));
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
    Ok(Attempted {
        report,
        attempt_status,
        before,
        frozen,
        bug_weak,
        refused_account,
    })
}
