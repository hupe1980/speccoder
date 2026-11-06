//! Phases 7c and 7b of a task run: judging the kept attempt beyond its
//! verdict. A verified verdict's tests are run against wrong and permitted
//! candidates (strength), and the gaming monitor reads the task, the diff and
//! the calls — never the model's account. Both are judge-side; the
//! implementer sees neither.

use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};

use super::attempt::Attempted;
use super::open::Opened;
use super::ready::Ready;
use super::tests::TestsFirst;
use super::*;
use crate::pipeline::*;

/// What judging the attempt found besides its verdict.
pub(super) struct Judged {
    /// The verdict passed and the attempt completed.
    pub(super) verdict_passed: bool,
    /// The monitor suspects gaming: the tick waits for a person.
    pub(super) suspected: bool,
    /// Where the strength step keeps what it held for a ruling.
    pub(super) strength_dir: std::path::PathBuf,
}

/// Phases 7c, 7b: the attempt back, its status and report judged.
#[allow(clippy::too_many_lines)] // strength, then the monitor
pub(super) fn after(
    o: &Opened,
    r: &Ready,
    t: &TestsFirst,
    attempted: Attempted,
    task: &str,
    secret: Option<&str>,
) -> (Attempted, Judged) {
    let Opened {
        s,
        feature_name,
        live,
        project,
        wt_root,
        tmp,
        run_config,
        runner,
        mutation_config,
        policy,
        ..
    } = o;
    let live = Arc::clone(live);
    let Ready {
        rt,
        bundle_value,
        instructions,
        plane,
        start,
        ..
    } = r;
    let TestsFirst {
        test_layout,
        judged_by,
        scenarios,
        story_oracle,
        related,
        told,
        acceptance,
        held_out_store,
        ..
    } = t;
    let Attempted {
        mut report,
        attempt_status,
        before,
        frozen,
        bug_weak,
        refused_account,
        ..
    } = attempted;
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
                scenarios,
                instructions,
                runner,
                layout: Some(test_layout),
                told,
                related,
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

    (
        Attempted {
            report,
            attempt_status,
            before,
            frozen,
            bug_weak,
            refused_account,
        },
        Judged {
            verdict_passed,
            suspected,
            strength_dir,
        },
    )
}
