//! `speccoder run --task`: one task from baseline to tick.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{
    approvals, attempts, external, integrity, mcp, mutation, progress, resume, sandbox,
    substrate, tools, worktree,
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
mod attempt;

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
    // In place: the person's checkout, switched and switched back around the task.
    if options.in_place && options.session.is_none() {
        return in_session("run", path, feature, state, options, |o| {
            run_task(path, feature, task, state, Arc::clone(&provider), secret, o)
        });
    }
    let (opened, started) = ok!(open::task(path, feature, task, state, secret, options));
    let open::Opened {
        s,
        native,
        feature_name,
        plane_dir,
        live,
        project,
        wt_root,
        tmp,
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
        planning,
        policy,
        ..
    } = &opened;
    let (format, live) = (*format, Arc::clone(live));
    let checklists_overridden = *checklists_overridden;
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
        plane,
        start,
        ..
    } = &ready;
    let attempt_instructions: &[crate::instructions::Instruction] = if executor.is_none() {
        instructions
    } else {
        &[]
    };

    let mut tests = ok!(tests::first(
        &opened,
        &ready,
        &mut carried,
        task,
        state,
        secret,
        options,
    ));
    let attempted = ok!(attempt::run(
        path,
        &opened,
        &ready,
        &mut tests,
        &mut carried,
        task,
        state,
        secret,
        options,
    ));
    let attempt::Attempted {
        mut report,
        attempt_status,
        before,
        frozen,
        bug_weak,
        refused_account,
        ..
    } = attempted;
    let tests::TestsFirst {
        mut bug_fix,
        bug_candidate,
        test_layout,
        story_scenarios,
        story_key,
        judged_by,
        in_story,
        scenarios,
        review_frozen,
        story_oracle,
        oracle_empty,
        ruled,
        related,
        told,
        acceptance,
        held_out_store,
        ..
    } = tests;
    let Carried {
        marker_guard,
        ..
    } = carried;
    let bug_ctx = tests::bug_ctx(&opened, &ready, task);

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
