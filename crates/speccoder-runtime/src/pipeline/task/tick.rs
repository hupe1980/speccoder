//! Phases 8–9 of a task run: the tick and the result. A passed attempt is
//! ticked by the engine and committed with its evidence; one that did not
//! pass is saved under an attempt ref and undone; either way the person is
//! told what happened and the one next step.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::attempt::Attempted;
use super::judge::Judged;
use super::open::Opened;
use super::ready::Ready;
use super::tests::TestsFirst;
use super::*;
use crate::pipeline::*;

/// Phases 8–9: the run's end, with what a person sees.
#[allow(clippy::too_many_lines)] // the tick, the save and the result
#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    path: &Path,
    feature: Option<&str>,
    o: &Opened,
    r: &Ready,
    tests: TestsFirst,
    attempted: Attempted,
    carried: Carried,
    judged: Judged,
    task: &str,
    state: &Path,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let Opened {
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
        planning,
        policy,
        ..
    } = o;
    let (format, live) = (*format, Arc::clone(live));
    let checklists_overridden = *checklists_overridden;
    let Ready {
        bundle_value,
        instructions,
        start,
        ..
    } = r;
    let TestsFirst {
        mut bug_fix,
        bug_candidate,
        story_scenarios,
        story_key,
        in_story,
        scenarios,
        review_frozen,
        story_oracle,
        oracle_empty,
        ruled,
        acceptance,
        ..
    } = tests;
    let Attempted {
        mut report,
        mut attempt_status,
        bug_weak,
        refused_account,
        ..
    } = attempted;
    let Carried { marker_guard, .. } = carried;
    let Judged {
        verdict_passed,
        suspected,
        strength_dir,
    } = judged;
    let bug_ctx = super::tests::bug_ctx(o, r, task);
    let attempt_instructions: &[crate::instructions::Instruction] = if executor.is_none() {
        instructions
    } else {
        &[]
    };
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
                    let done =
                        s.commit_and_tick(plane_dir, file, &t.line_hash, &message, &digest, || {
                            format.tick(file, task, &t.line_hash)
                        });
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
        // A fixed bug closes the task list its home keeps (Kiro's), in
        // the verdict commit: each task's evidence is the bug's verdict.
        let closed = if matches!(verdict.as_str(), "verified" | "attested") {
            format.close_bug(&project.join(&n.dir)).unwrap_or_else(|e| {
                report
                    .integrity
                    .push(format!("the bug's task list was not closed: {e}"));
                Vec::new()
            })
        } else {
            Vec::new()
        };
        let closes: String = closed
            .iter()
            .map(|id| format!("Closes-Task: {feature_name}/{id}\n"))
            .collect();
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
                + &ruling_trailers(&ruled)
                + &closes,
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
