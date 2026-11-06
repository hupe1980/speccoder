//! Phases 4a–4c and 5a of a task run: the tests first. What judges the
//! task is decided by code from the plan; the story's oracle (or a bug's
//! reproductions, or the task's own acceptance tests) is written and shown
//! red before any code, then the held-out set is sealed. A task with nothing
//! to do, an oracle that cannot be written or a gap not shown red ends here.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

use super::Carried;
use super::open::Opened;
use super::ready::Ready;
use crate::pipeline::*;
use crate::{attempts, git, progress, verify, worktree};

/// What judges the task, written before its attempt.
pub(super) struct TestsFirst {
    pub(super) bug_fix: Option<super::super::bug::Fix>,
    pub(super) bug_candidate: Option<(String, speccoder_core::bug::Candidate)>,
    pub(super) test_layout: crate::layout::TestLayout,
    pub(super) test_layout_text: String,
    pub(super) story_scenarios: Vec<speccoder_core::spec::ScenarioRef>,
    pub(super) story_key: Option<String>,
    pub(super) judged_by: Option<speccoder_core::story::Judged>,
    pub(super) in_story: bool,
    pub(super) scenarios: Vec<speccoder_core::spec::ScenarioRef>,
    pub(super) review_frozen: Option<(speccoder_core::acceptance::PreFrozenAcceptance, String)>,
    pub(super) story_oracle: Option<super::super::story::StoryOracle>,
    pub(super) oracle_note: serde_json::Value,
    pub(super) oracle_empty: bool,
    pub(super) before_run: super::super::story::Run,
    pub(super) oracle_spent: Option<serde_json::Value>,
    pub(super) ruled: Vec<crate::acceptance_at::Ruling>,
    pub(super) related: Vec<String>,
    pub(super) told: Vec<String>,
    pub(super) acceptance: Option<PreparedAcceptance>,
    pub(super) held_out_store: std::path::PathBuf,
}

/// What a bug task's stages need from the run.
pub(super) fn bug_ctx<'a>(o: &'a Opened, r: &'a Ready, task: &'a str) -> super::super::bug::Ctx<'a> {
    super::super::bug::Ctx {
        rt: &r.rt,
        plane: &r.plane,
        project: &o.project,
        wt_root: &o.wt_root,
        wt_path: &o.wt_path,
        tmp: &o.tmp,
        policy: &o.policy,
        runner: &o.runner,
        state: &o.s.plane_dir,
        feature: &o.feature_name,
        task,
        bundle: &r.bundle_value,
        instructions: &r.instructions,
        live: &*o.live,
        approver: o.approver.as_deref(),
        held_out: o.run_config.held_out,
        max_tries: o.run_config.reproduce_tries,
        bug_test: o.run_config.bug_test.as_deref(),
        start: &r.start,
    }
}

/// Phases 4a–4c, 5a. `Err`: the run ends here, with what a person sees.
#[allow(clippy::too_many_lines)] // the tests before the code, in order
pub(super) fn first(
    o: &Opened,
    r: &Ready,
    carried: &mut Carried,
    task: &str,
    state: &Path,
    secret: Option<&str>,
    options: &RunOptions,
) -> Result<TestsFirst, Box<CommandResult>> {
    let Opened {
        s,
        feature_name,
        live,
        wt_path,
        project,
        wt_root,
        tmp,
        interrupted,
        format,
        snap,
        parsed_task,
        bundle,
        acceptance_story,
        story_task,
        run_config,
        runner,
        check_argvs,
        unit_dir,
        planning,
        policy,
        ..
    } = o;
    let (format, live) = (*format, Arc::clone(live));
    let Ready {
        rt,
        bundle_value,
        instructions,
        plane,
        demand_red,
        start,
        ..
    } = r;
    let Carried {
        head_now,
        set_aside,
        baseline,
        notices,
        ..
    } = carried;
    let fail = |status: CommandStatus, m: String| super::ended(&*live, task, secret, status, m);
    let began = std::time::Instant::now();
    // 4b. Acceptance tests derived from the spec, written before the
    //     attempt so they are frozen, and required to be red now.
    // 4a. A bug task: reproduce (and the person's confirmation) before
    //     anything else; once confirmed, its reproductions are the visible set.
    let bug_ctx = bug_ctx(o, r, task);
    let bug_fix = match super::bug::phase(&s.plane_dir, feature_name, task, project, format) {
        Err(e) => return Err(Box::new(fail(CommandStatus::NeedsHuman, e))),
        Ok(None) => None,
        Ok(Some(super::bug::Phase::Fix(f))) => Some(*f),
        Ok(Some(super::bug::Phase::Reproduce(b))) if interrupted.is_none() => {
            match super::bug::reproduce(&bug_ctx, *b) {
                Ok(f) => Some(f),
                Err(stop) => return Err(Box::new(*stop)),
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
    // The test layout, detected from the checkout: a fact for the
    // implementer and every test author, never a judge input.
    let test_layout = crate::layout::detect(project, check_argvs);
    let test_layout_text = test_layout.text();
    let story_scenarios = match &bug_fix {
        Some(f) => super::bug::scenarios(&f.bug),
        None => format.scenarios(snap, story_task.as_deref().unwrap_or(task)),
    };
    // What judges this task — decided by code from the plan, never by a
    // test author's choice (speccoder_core::story): a task of a story by
    // the progress it makes on the story's oracle; a task outside any
    // story by the scenarios it cites (a convergence gap). A bug's
    // reproductions judge a bug.
    let rules = format.rules().judged;
    let whole_unit = format.kind() == speccoder_core::spec::FormatKind::Openspec;
    // A task outside any story citing scenarios of one story (a
    // convergence gap): judged by that story's oracle — each scenario has
    // one test, never a second the task's own author would write.
    let linked_story: Option<(crate::format::OracleRequirement, Vec<String>)> = if bug_fix.is_some()
        || acceptance_story.is_some()
        || whole_unit
        || parsed_task.as_ref().is_none_or(|t| t.story.is_some())
    {
        None
    } else {
        parsed_task.as_ref().and_then(|t| {
            super::acceptance::linked_story(
                t,
                &story_scenarios,
                unit_dir,
                &rules,
                &format.oracle_requirements(snap),
            )
        })
    };
    let story_scenarios = linked_story
        .as_ref()
        .map_or(story_scenarios, |(r, _)| r.scenarios.clone());
    // The story it is judged in: its label, the story a person had its
    // acceptance written at (`acceptance at`), or the whole unit (OpenSpec).
    let story_key: Option<String> = if bug_fix.is_some() || story_scenarios.is_empty() {
        None
    } else {
        acceptance_story
            .clone()
            .or_else(|| parsed_task.as_ref().and_then(|t| t.story.clone()))
            .or_else(|| whole_unit.then(|| feature_name.clone()))
            .or_else(|| linked_story.as_ref().map(|(r, _)| r.key.clone()))
    };
    let parsed_tasks = snap.feature.as_ref().and_then(|f| f.tasks.as_ref());
    let story_members: Vec<&speccoder_core::snapshot::Task> = match (&story_key, parsed_tasks) {
        (Some(key), Some(pt)) => pt
            .tasks
            .iter()
            .filter(|t| whole_unit || t.story.as_deref() == Some(key.as_str()))
            .collect(),
        _ => Vec::new(),
    };
    let judged_by: Option<speccoder_core::story::Judged> = if bug_fix.is_some() {
        None
    } else if let Some((_, links)) = &linked_story {
        Some(speccoder_core::story::Judged::Linked {
            scenarios: links.clone(),
        })
    } else if story_key.is_some() {
        let ids: Vec<String> = story_scenarios.iter().map(|s| s.id.clone()).collect();
        let st = super::story::story_tasks(&story_members, unit_dir, &rules);
        speccoder_core::story::judged(&st, task, &ids).or_else(|| {
            acceptance_story
                .is_some()
                .then_some(speccoder_core::story::Judged::Progress)
        })
    } else {
        parsed_task
            .as_ref()
            .and_then(|t| super::acceptance::outside_story(t, &story_scenarios, unit_dir, &rules))
    };
    // A task of a story is judged by its story's oracle: no acceptance
    // tests of its own. Per-task tests remain outside stories.
    let in_story =
        story_key.is_some() && judged_by.is_some() && run_config.acceptance && planning.is_none();
    let scenarios: Vec<speccoder_core::spec::ScenarioRef> = match &judged_by {
        Some(speccoder_core::story::Judged::Linked { scenarios: l }) if story_key.is_none() => {
            story_scenarios
                .iter()
                .filter(|s| l.contains(&s.id))
                .cloned()
                .collect()
        }
        _ => story_scenarios.clone(),
    };
    // A requirement's oracle frozen at review: reused by the
    // first task citing its scenarios while its wording is unchanged.
    let mut review_frozen: Option<(speccoder_core::acceptance::PreFrozenAcceptance, String)> = None;
    let mut review_reuse = None;
    // 5a. The story's oracle: written once, through the product, before
    //     the story's first code task implements anything — committed on
    //     the unit branch and frozen. Every task of the story is judged by
    //     the progress it makes on it.
    let mut story_oracle: Option<super::story::StoryOracle> = None;
    // What the implementer is told of the story's oracle.
    let mut oracle_note = serde_json::Value::Null;
    // The author found nothing testable in the story: judged by its checks.
    let mut oracle_empty = false;
    let mut before_run = super::story::Run::default();
    // What writing the story's oracle cost in this run: the task's too.
    let mut oracle_spent: Option<serde_json::Value> = None;
    if in_story && let Some(key) = story_key.as_deref() {
        stage(&*live, task, Stage::Red, StageState::Started, None, None);
        match super::story::ensure(&super::story::Ensure {
            key,
            scenarios: &story_scenarios,
            feature: feature_name,
            project,
            wt_root,
            state,
            // A story's oracle, rulings and sealed sets are the project's,
            // never a lane's.
            plane_dir: &s.plane_dir,
            runner,
            rt,
            plane,
            options,
            format,
            snapshot: snap,
            fresh: options.fresh_acceptance,
            resuming: interrupted.is_some(),
        }) {
            Ok((oracle, wrote, said)) => {
                notices.extend(said);
                oracle_spent.clone_from(&oracle.usage);
                if wrote {
                    notices.push(format!(
                        "{key}'s oracle: {} test(s) for {} scenario(s), committed {}",
                        oracle.tests.len(),
                        oracle.scenarios.len(),
                        oracle.commit.get(..7).unwrap_or(&oracle.commit)
                    ));
                    // The attempt starts on the oracle's commit: a resume
                    // marker names it.
                    *head_now = git::Git::at(wt_root)
                        .line(&["rev-parse", "HEAD"])
                        .unwrap_or_default();
                    // The baseline is the tree the attempt starts from: with
                    // the oracle's red tests in it.
                    match verify::run_checks(project, Some(verify::config::Tier::Fast), runner) {
                        Ok(b) => *baseline = b,
                        Err(e) => {
                            return Err(Box::new(fail(CommandStatus::ConfigError, format!("baseline: {e}"))));
                        }
                    }
                }
                // A resumed attempt is judged against the tree it started
                // from, as recorded — never its own changed tree.
                before_run = interrupted
                    .as_ref()
                    .and_then(|m| m.story_before.clone())
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_else(|| super::story::run(project, &oracle, runner));
                // The implementer is told the story's red scenarios, what
                // they must hold, which this task must turn, and how to
                // report a test that contradicts the spec.
                if !oracle.tests.is_empty() {
                    oracle_note = serde_json::json!({
                        "red": oracle
                            .scenarios
                            .iter()
                            .filter(|s| {
                                before_run.states.get(&s.id)
                                    != Some(&speccoder_core::story::ScenarioState::Green)
                                    && !oracle.untestable.contains_key(&s.id)
                            })
                            .map(|s| serde_json::json!({
                                "id": s.id,
                                "contract": oracle.contracts.get(&s.id),
                                "tests": oracle
                                    .tests
                                    .iter()
                                    .filter(|t| t.scenarios.contains(&s.id))
                                    .map(|t| t.path.clone())
                                    .collect::<Vec<_>>(),
                            }))
                            .collect::<Vec<_>>(),
                        "links": judged_by.as_ref().map(speccoder_core::story::Judged::links).unwrap_or_default(),
                        "checkpoint": matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint)),
                    });
                }
                // Frozen at review: the ticks name the approval and manifest.
                if let Some((record, manifest)) = oracle.reviewed.clone() {
                    review_frozen = Some((record, manifest));
                }
                // A gap its story's oracle already holds: nothing to do, and
                // no model call to learn it.
                if let Some((_, links)) = &linked_story
                    && !oracle.tests.is_empty()
                    && links.iter().all(|l| {
                        before_run.states.get(l)
                            == Some(&speccoder_core::story::ScenarioState::Green)
                    })
                {
                    stage(
                        &*live,
                        task,
                        Stage::Red,
                        StageState::Skipped,
                        Some("already holds".to_owned()),
                        None,
                    );
                    let reason = format!(
                        "{} already hold{} — {key}'s oracle passes {}: nothing for {task} to do",
                        links.join(", "),
                        if links.len() == 1 { "s" } else { "" },
                        if links.len() == 1 {
                            "its test"
                        } else {
                            "their tests"
                        },
                    );
                    let report = speccoder_core::RunReport {
                        reason: Some(reason.clone()),
                        notices: notices.clone(),
                        usage: oracle.usage.clone(),
                        ..bare_report(feature_name, task, wt_path, "holds")
                    };
                    return Err(Box::new(CommandResult::new("run", CommandStatus::NeedsHuman, &reason)
                        .with_code("TASK-HOLDS")
                        .with_data(CommandData::Run(Box::new(report)))));
                }
                // The story's last task waits on a person for every scenario
                // its author called untestable: no attempt can turn it.
                if matches!(judged_by, Some(speccoder_core::story::Judged::Checkpoint))
                    && !oracle.untestable.is_empty()
                {
                    let accepted = super::story::accepted_on_branch(wt_root, "HEAD");
                    let open: Vec<(&String, &String)> = oracle
                        .untestable
                        .iter()
                        .filter(|(id, _)| !accepted.contains(id))
                        .collect();
                    if let Some((first, _)) = open.first() {
                        stage(
                            &*live,
                            task,
                            Stage::Red,
                            StageState::Skipped,
                            Some("untestable".to_owned()),
                            None,
                        );
                        let said: Vec<String> = open
                            .iter()
                            .map(|(id, why)| format!("{id} (\"{why}\")"))
                            .collect();
                        let reason = format!(
                            "{key} closes with {task}, but its test author called {} untestable — a person accepts each, or has its test written again",
                            said.join(", ")
                        );
                        let report = speccoder_core::RunReport {
                            reason: Some(reason.clone()),
                            notices: notices.clone(),
                            usage: oracle.usage.clone(),
                            ..bare_report(feature_name, task, wt_path, "untestable")
                        };
                        return Err(Box::new(CommandResult::new("run", CommandStatus::NeedsHuman, &reason)
                            .with_code("STORY-UNTESTABLE")
                            .with_fix(vec![
                                format!("speccoder story accept {first} --reason \"…\""),
                                format!("speccoder task rule {task} --tests-wrong --scenario {first} --reason \"…\""),
                            ])
                            .with_data(CommandData::Run(Box::new(report)))));
                    }
                }
                if oracle.tests.is_empty() {
                    oracle_empty = true;
                } else {
                    story_oracle = Some(oracle);
                }
            }
            Err(r) => {
                stage(
                    &*live,
                    task,
                    Stage::Red,
                    StageState::Failed,
                    Some(r.message().to_owned()),
                    None,
                );
                let reason = format!(
                    "the acceptance tests could not be written ({}); nothing was attempted",
                    r.message()
                );
                let report = speccoder_core::RunReport {
                    reason: Some(redact(&reason, secret)),
                    notices: notices.clone(),
                    ..bare_report(feature_name, task, wt_path, "acceptance_failed")
                };
                let code = r.code().map_or("ORACLE-UNWRITTEN", |c| c.name);
                return Err(Box::new(CommandResult::new(
                    "run",
                    CommandStatus::NeedsHuman,
                    &redact(&reason, secret),
                )
                .with_code(code)
                // The oracle's own way on (its story's tests), then the task's.
                .with_fix(if r.given_fix().is_empty() {
                    Vec::new()
                } else {
                    r.given_fix()
                        .iter()
                        .cloned()
                        .chain([format!("speccoder run --task {task}")])
                        .collect()
                })
                .with_data(CommandData::Run(Box::new(report)))));
            }
        }
    }
    // A person ruled the task's own test wrong (a task outside a story):
    // every author of its tests is told why — the visible, held-out and
    // strengthening ones, a resumed attempt's too — and nothing frozen
    // earlier is reused; the tick records the rulings and clears them.
    // An unreadable list stops the run: never read as "no ruling".
    let ruled: Vec<crate::acceptance_at::Ruling> = if bug_fix.is_none() && !in_story {
        match crate::acceptance_at::rulings_for_task(&s.plane_dir, feature_name, task) {
            Ok(r) => r,
            Err(e) => {
                return Err(Box::new(fail(
                    CommandStatus::Error,
                    format!("the person's rulings cannot be read: {e}"),
                )));
            }
        }
    } else {
        Vec::new()
    };
    let told: Vec<String> = ruled
        .iter()
        .map(|r| format!("{} ruled the tests wrong: {}", r.by, r.reason))
        .collect();
    // The tasks on the same requirements: their tests are the author's to
    // agree with.
    let related: Vec<String> = match (parsed_task.as_ref(), parsed_tasks) {
        (Some(t), Some(pt)) => super::acceptance::related(t, &pt.tasks),
        _ => Vec::new(),
    };
    if bug_fix.is_none()
        && run_config.acceptance
        && !options.fresh_acceptance
        && planning.is_none()
        && !in_story
        && ruled.is_empty()
    {
        match crate::oracle::for_task(&s.plane_dir, project, wt_root, format, snap, &scenarios) {
            Some(r @ crate::oracle::TaskReuse::Ready { .. }) => review_reuse = Some(r),
            Some(crate::oracle::TaskReuse::Reused { req }) => notices.push(format!(
                "{req}'s frozen tests were reused by an earlier task of this run: this task's acceptance tests are authored"
            )),
            Some(crate::oracle::TaskReuse::Elsewhere { req }) => {
                notices.push(format!("{req}: frozen oracle not present here — authoring"))
            }
            Some(crate::oracle::TaskReuse::Lapsed {
                req,
                frozen,
                current,
            }) => {
                if crate::oracle::note_lapse(&s.plane_dir, feature_name, &req, &frozen) {
                    live.event(&progress::Event::Oracle {
                        step: progress::OracleEvent::Lapsed {
                            unit: feature_name.clone(),
                            requirement: req.clone(),
                            frozen_wording_sha256: frozen,
                            current_wording_sha256: current,
                        },
                    });
                }
                notices.push(format!(
                    "{req}'s wording changed since its tests were frozen at review: the frozen oracle lapsed — authoring"
                ));
            }
            Some(crate::oracle::TaskReuse::Tampered(why)) => {
                return Err(Box::new(fail(
                    CommandStatus::NeedsHuman,
                    format!("{why}: the task stops for a person, nothing is ticked"),
                )
                .with_code("ORACLE-TAMPERED")));
            }
            Some(crate::oracle::TaskReuse::Unreadable(why)) => {
                return Err(Box::new(fail(
                    CommandStatus::NeedsHuman,
                    format!("{why}: whether its tests are frozen is unknown, so the task stops for a person, nothing is ticked"),
                )
                .with_code("REVIEW-UNREADABLE")));
            }
            None => {}
        }
    }
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
                let _ = worktree::restore(wt_root, tmp, start);
                stage(
                    &*live,
                    task,
                    Stage::Red,
                    StageState::Failed,
                    Some(e.clone()),
                    Some(began),
                );
                return Err(Box::new(fail(CommandStatus::NeedsHuman, e)));
            }
        }
    } else if let Some(reuse) = review_reuse.take() {
        // A frozen oracle: its tests at this checkpoint, red by
        // the run-time rule, its sealed held-out set red too — no author call.
        stage(&*live, task, Stage::Red, StageState::Started, None, None);
        let crate::oracle::TaskReuse::Ready {
            record,
            files,
            tests,
            manifest,
        } = reuse
        else {
            return Err(Box::new(fail(CommandStatus::Error, "no frozen oracle".to_owned())));
        };
        match reuse_pre_frozen(
            project,
            &record,
            &files,
            &tests,
            runner,
            RedRule::RunTime,
            Some(&SealedCheck {
                state: &s.plane_dir,
                unit: feature_name,
                wt_root,
                tmp,
                policy,
            }),
        ) {
            Ok(mut a) => {
                a.report["pre_frozen"]["manifest"] = serde_json::json!(manifest);
                notices.push(format!(
                    "acceptance tests frozen at review by {} ({}) for {}: no author call",
                    record.decided.by, record.decided.via, record.requirement
                ));
                review_frozen = Some((*record, manifest));
                Some(a)
            }
            Err(e) => {
                let _ = worktree::restore(wt_root, tmp, start);
                stage(
                    &*live,
                    task,
                    Stage::Red,
                    StageState::Failed,
                    Some(e.clone()),
                    Some(began),
                );
                return Err(Box::new(fail(CommandStatus::NeedsHuman, e).with_code("ORACLE-TAMPERED")));
            }
        }
    } else if run_config.acceptance
        && planning.is_none()
        && !in_story
        && format.acceptance_testable(bundle)
    {
        stage(&*live, task, Stage::Red, StageState::Started, None, None);
        Some(prepare_acceptance(&AcceptanceSetup {
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
        }))
    } else {
        None
    };
    if review_frozen.is_none()
        && acceptance
            .as_ref()
            .is_some_and(|a| a.report["source"] == "review")
        && let Some(crate::oracle::TaskReuse::Ready {
            record, manifest, ..
        }) = review_reuse.take()
    {
        review_frozen = Some((*record, manifest));
    }
    // 4c. The held-out set: written after a red visible set, kept in
    //     the state directory, never shown to the implementer.
    let held_out_store = match (&bug_fix, &review_frozen) {
        (Some(f), _) => crate::held_out::sealed_dir(&s.plane_dir, &f.bug.unit, &f.bug.requirement),
        (None, Some((r, _))) => {
            crate::held_out::sealed_dir(&s.plane_dir, feature_name, &r.requirement)
        }
        (None, None) => crate::held_out::dir(&s.plane_dir, feature_name, task),
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
                },
                a,
                &scenarios,
                &held_out_store,
                false,
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
            Some(if planning.is_some() {
                "planning work: design documents".to_owned()
            } else if in_story {
                "judged by its story's oracle".to_owned()
            } else {
                "no acceptance examples".to_owned()
            }),
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
        let _ = worktree::restore(wt_root, tmp, start);
        let report = RunReport {
            reason: Some(reason.clone()),
            acceptance: acceptance.as_ref().map(|a| a.report.clone()),
            set_aside: set_aside.take(),
            notices: notices.clone(),
            ..bare_report(feature_name, task, wt_path, "acceptance_failed")
        };
        live.event(&finished_event(
            task,
            &report,
            "acceptance_failed",
            false,
            redact(&reason, secret),
        ));
        return Err(Box::new(CommandResult::new("run", CommandStatus::NeedsHuman, &redact(&reason, secret))
            .with_code("ACCEPTANCE-UNWRITTEN")
            .with_data(CommandData::Run(Box::new(report)))));
    }

    // A gap judged by its story's oracle is shown by that oracle's red test.
    if *demand_red && story_oracle.is_none() && !acceptance.as_ref().is_some_and(|a| a.red) {
        let why = undemonstrated_why(acceptance.as_ref());
        let reason = format!(
            "convergence gap not demonstrated: {why}; a person decides whether {task} is real"
        );
        // The undemonstrated tests are kept under an attempt ref, not in the worktree.
        let saved = attempts::save(
            wt_root,
            tmp,
            feature_name,
            task,
            &attempts::Record {
                run: None,
                outcome: "not_demonstrated",
                reason: &reason,
                pending: None,
                usage: None,
                story_open: &[],
                own_test: None,
            },
        )
        .ok()
        .map(|s| s.reference);
        let _ = worktree::restore(wt_root, tmp, start);
        let report = RunReport {
            reason: Some(reason.clone()),
            acceptance: acceptance.as_ref().map(|a| a.report.clone()),
            saved_attempt: saved,
            set_aside: set_aside.take(),
            notices: notices.clone(),
            ..bare_report(feature_name, task, wt_path, "not_demonstrated")
        };
        live.event(&finished_event(
            task,
            &report,
            "not_demonstrated",
            false,
            redact(&reason, secret),
        ));
        return Err(Box::new(CommandResult::new("run", CommandStatus::NeedsHuman, &redact(&reason, secret))
            .with_data(CommandData::Run(Box::new(report)))));
    }

    Ok(TestsFirst {
        bug_fix,
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
    })
}
