//! Spec-derived acceptance tests: written before the attempt, red then green.

use std::path::Path;

use speccoder_core::verify::{RedClass, classify_red};

use crate::{sandbox, substrate, tools, verify, worktree};

pub(crate) struct AcceptanceSetup<'a> {
    pub(crate) rt: &'a tokio::runtime::Runtime,
    pub(crate) plane: &'a substrate::Plane,
    pub(crate) project: &'a Path,
    pub(crate) worktree: &'a Path,
    pub(crate) tmp: &'a Path,
    pub(crate) policy: &'a tools::policy::Policy,
    pub(crate) feature: &'a str,
    pub(crate) task: &'a str,
    pub(crate) bundle: serde_json::Value,
    /// The task's scenarios, which each test cites.
    pub(crate) scenarios: &'a [speccoder_core::spec::ScenarioRef],
    /// The repository's instructions (test conventions live there too).
    pub(crate) instructions: &'a [crate::instructions::Instruction],
    pub(crate) runner: &'a sandbox::Runner,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedAcceptance {
    /// What the run report shows.
    pub(crate) report: serde_json::Value,
    /// The command, when the tests were written.
    pub(crate) argv: Option<Vec<String>>,
    /// They failed before the attempt, on an assertion or for an absent
    /// symbol.
    pub(crate) red: bool,
    /// What the red run showed.
    #[serde(default)]
    pub(crate) red_class: Option<speccoder_core::verify::RedClass>,
    /// The held-out manifest's hash, when a held-out set was kept: a resumed
    /// attempt reuses it, never re-authors it.
    #[serde(default)]
    pub(crate) held_out: Option<String>,
    /// Why the author call itself failed (the provider refused, the run
    /// failed) — not "the spec has no examples": the task stops for a person.
    #[serde(default)]
    pub(crate) generation_failed: Option<String>,
}

pub(crate) const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
];

pub(crate) fn prepare_acceptance(a: &AcceptanceSetup<'_>) -> PreparedAcceptance {
    use serde_json::json;
    let skipped = |why: String, report: serde_json::Value| {
        let mut report = report;
        report["skipped"] = json!(why);
        PreparedAcceptance {
            report,
            argv: None,
            red: false,
            red_class: None,
            held_out: None,
            generation_failed: None,
        }
    };
    let failed = |why: String, report: serde_json::Value| PreparedAcceptance {
        generation_failed: Some(why.clone()),
        ..skipped(why, report)
    };

    let files: Vec<String> = worktree::checkpoint(a.worktree, a.tmp)
        .and_then(|c| worktree::files(a.worktree, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let input = substrate::acceptance::input(
        a.bundle.clone(),
        a.scenarios,
        &files,
        &manifest,
        a.instructions,
    );
    let outcome = a.rt.block_on(a.plane.acceptance(a.feature, a.task, input));
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return failed(format!("generation failed: {}", e.0), json!({})),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    let base = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"], "notes": answer["notes"] });
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        let status = format!("{:?}", outcome.status);
        let why = substrate::provider_refusal(&status).map_or_else(
            || format!("generation did not succeed: {}", outcome.status.as_str()),
            |r| format!("the model provider refused the request: {r}"),
        );
        return failed(why, base);
    }

    // Validate: new test files only, a command the policy allows.
    let protected = a.policy.protected();
    let mut written = Vec::new();
    let mut planned = Vec::new();
    // Each test and the scenarios it cites, of those the spec has.
    let mut tests = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
            continue;
        };
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(e) => return skipped(format!("`{path}`: {}", e.0), base),
        };
        if !tools::paths::is_test_path(&rel) || protected.write_class(&rel).is_some() {
            return skipped(format!("`{rel}` is not a new test file"), base);
        }
        if a.project.join(&rel).exists() {
            return skipped(format!("`{rel}` already exists"), base);
        }
        let cited: Vec<&str> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter(|id| a.scenarios.iter().any(|s| s.id == *id))
            .collect();
        tests.push(json!({ "path": rel, "scenarios": cited }));
        planned.push((rel, content.to_owned()));
    }
    if planned.is_empty() {
        return skipped("no acceptance tests were generated".to_owned(), base);
    }
    let argv: Vec<String> = answer["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if a.policy.classify("run_command", &json!({ "argv": argv })) != tools::policy::Decision::Allow
    {
        return skipped(
            format!("`{}` is not an allowed command", argv.join(" ")),
            base,
        );
    }
    for (rel, content) in &planned {
        let full = a.project.join(rel);
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&full, content) {
            return skipped(format!("`{rel}`: {e}"), base);
        }
        written.push(rel.clone());
    }

    // Red: they must fail before the task is done — on an assertion, or
    // because what the task introduces does not exist yet; confirmed by a
    // second run.
    let (first, summary) = execute(a.project, &argv, a.runner);
    let mut class = classify_red(&first, summary.as_ref());
    if class.is_red() {
        let (second, summary) = execute(a.project, &argv, a.runner);
        if !classify_red(&second, summary.as_ref()).is_red() {
            class = RedClass::Flaky;
        }
    }
    let red = class.is_red();
    let mut report = base;
    report["files"] = json!(written);
    report["tests"] = json!(tests);
    report["argv"] = json!(argv);
    report["red"] = json!(red);
    report["red_class"] = json!(class);
    if !red {
        report["note"] = json!(match class {
            RedClass::Passed => "the acceptance tests already pass before the attempt, so they are no evidence for it".to_owned(),
            RedClass::Flaky => "the acceptance tests failed once and passed when run again: not demonstrated".to_owned(),
            other => format!(
                "the acceptance tests did not fail on an assertion ({}): not demonstrated",
                class_word(other)
            ),
        });
        report["excerpt"] = json!(excerpt(&first));
    }
    PreparedAcceptance {
        report,
        argv: Some(argv),
        red,
        red_class: Some(class),
        held_out: None,
        generation_failed: None,
    }
}

/// The class as written in reports.
pub(crate) fn class_word(class: RedClass) -> String {
    serde_json::to_value(class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Up to 20 lines from the first error line of a run's output (else its
/// last 20 lines).
fn excerpt(result: &speccoder_core::verify::CheckResult) -> String {
    let text = format!("{}\n{}", result.stdout_tail, result.stderr_tail);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let from = lines
        .iter()
        .position(|l| {
            let l = l.to_lowercase();
            l.contains("error") || l.contains("fail")
        })
        .unwrap_or_else(|| lines.len().saturating_sub(20));
    lines
        .iter()
        .skip(from)
        .take(20)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// One run of an acceptance command, with the runner's summary.
pub(crate) fn acceptance_run(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> (
    speccoder_core::verify::CheckResult,
    Option<speccoder_core::verify::RunnerSummary>,
) {
    execute(project, argv, runner)
}

pub(crate) fn run_acceptance(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> speccoder_core::verify::CheckResult {
    execute(project, argv, runner).0
}

/// Runs the acceptance tests once; the runner's summary is read from the
/// whole captured output, before it is cut to tails.
fn execute(
    project: &Path,
    argv: &[String],
    runner: &sandbox::Runner,
) -> (
    speccoder_core::verify::CheckResult,
    Option<speccoder_core::verify::RunnerSummary>,
) {
    use speccoder_core::verify::{CheckResult, CheckStatus};
    use verify::process::CommandRunner;
    let out = runner.run(&verify::process::Invocation {
        argv: argv.to_vec(),
        cwd: project.to_path_buf(),
        env: verify::process::scrubbed_env(std::env::vars()),
        timeout: std::time::Duration::from_secs(600),
        output_cap: verify::process::OUTPUT_CAP,
    });
    let status = if out.spawn_error.is_some() {
        CheckStatus::Error
    } else if out.timed_out {
        CheckStatus::TimedOut
    } else if out.exit_code == Some(0) {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };
    let summary = verify::summary::parse(
        &String::from_utf8_lossy(&out.stdout.kept),
        &String::from_utf8_lossy(&out.stderr.kept),
        out.exit_code,
    );
    let tail = |b: &[u8]| {
        let start = b.len().saturating_sub(4096);
        String::from_utf8_lossy(&b[start..]).into_owned()
    };
    let result = CheckResult {
        name: "acceptance".to_owned(),
        argv: argv.to_vec(),
        status,
        exit_code: out.exit_code,
        duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
        junit: false,
        tests: None,
        stdout_tail: tail(&out.stdout.kept),
        stderr_tail: tail(&out.stderr.kept),
        truncated: out.stdout.truncated() || out.stderr.truncated(),
        note: out.spawn_error,
        flaky: false,
    };
    (result, summary)
}

/// The held-out set's author, store and red check: after a visible
/// set that was red. Returns the report part (`acceptance.held_out`) and
/// the stored manifest's hash when files were kept.
pub(crate) fn prepare_held_out(
    a: &AcceptanceSetup<'_>,
    visible: &PreparedAcceptance,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    store: &Path,
    composition: bool,
) -> (serde_json::Value, Option<String>) {
    use serde_json::json;
    let absent = |reason: &str, mut report: serde_json::Value| {
        report["state"] = json!("absent");
        report["reason"] = json!(reason);
        (report, None)
    };
    let visible_files: Vec<(String, String)> = visible.report["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str())
        .map(|p| {
            (
                p.to_owned(),
                std::fs::read_to_string(a.project.join(p)).unwrap_or_default(),
            )
        })
        .collect();
    let files: Vec<String> = worktree::checkpoint(a.worktree, a.tmp)
        .and_then(|c| worktree::files(a.worktree, &c))
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest_text = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let input = substrate::acceptance::held_out_input(
        a.bundle.clone(),
        &files,
        &manifest_text,
        scenarios,
        &visible_files,
        composition,
        a.instructions,
    );
    let outcome = match a.rt.block_on(a.plane.acceptance(a.feature, a.task, input)) {
        Ok(o) => o,
        Err(e) => return absent("generation_failed", json!({ "error": e.0 })),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    let base = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"], "notes": answer["notes"] });
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return absent("generation_failed", base);
    }

    // Validate: new test paths, known scenarios, an allowed
    // command each, at most `LIMIT`.
    let known: std::collections::BTreeSet<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
    let protected = a.policy.protected();
    let mut kept: Vec<(crate::held_out::HeldOutFile, String)> = Vec::new();
    let mut discarded: Vec<crate::held_out::Discarded> = Vec::new();
    let mut discard = |path: &str, reason: &str| {
        discarded.push(crate::held_out::Discarded {
            path: path.to_owned(),
            reason: reason.to_owned(),
        });
    };
    let given = answer["files"].as_array().cloned().unwrap_or_default();
    if given.is_empty() {
        return absent("none_generated", base);
    }
    for f in &given {
        let path = f["path"].as_str().unwrap_or_default();
        let content = f["content"].as_str().unwrap_or_default();
        let cited: Vec<String> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        let argv: Vec<String> = f["argv"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(_) => {
                discard(path, "path");
                continue;
            }
        };
        let taken =
            visible_files.iter().any(|(p, _)| *p == rel) || kept.iter().any(|(k, _)| k.path == rel);
        if !tools::paths::is_test_path(&rel)
            || protected.write_class(&rel).is_some()
            || a.project.join(&rel).exists()
            || taken
        {
            discard(&rel, "path");
            continue;
        }
        if cited.is_empty() || cited.iter().any(|c| !known.contains(c.as_str())) {
            discard(&rel, "unknown_scenario");
            continue;
        }
        let composed = f["composition"].as_bool().unwrap_or(false);
        if composed && (!composition || cited.len() < 2) {
            discard(&rel, "composition");
            continue;
        }
        if a.policy.classify("run_command", &json!({ "argv": argv }))
            != tools::policy::Decision::Allow
        {
            discard(&rel, "command");
            continue;
        }
        if kept.len() >= crate::held_out::LIMIT {
            discard(&rel, "limit");
            continue;
        }
        kept.push((
            crate::held_out::HeldOutFile {
                path: rel,
                sha256: crate::integrity::sha256_hex(content.as_bytes()),
                scenarios: cited,
                argv,
                composition: composed,
            },
            content.to_owned(),
        ));
    }

    // Red now, on the tree the attempt starts from.
    let checkpoint = worktree::checkpoint(a.worktree, a.tmp)
        .map(|c| c.0)
        .unwrap_or_default();
    match crate::held_out::overlay_run(a.worktree, a.project, a.tmp, &kept, a.runner, a.policy) {
        Ok(runs) => {
            let mut red = Vec::new();
            for ((file, content), ran) in kept.into_iter().zip(runs) {
                match ran.result {
                    None => discard(&file.path, "collision"),
                    Some((result, summary)) if classify_red(&result, summary.as_ref()).is_red() => {
                        red.push((file, content));
                    }
                    Some(_) => discard(&file.path, "not_red"),
                }
            }
            kept = red;
        }
        Err(e) => {
            return absent(
                "generation_failed",
                json!({ "error": e, "run_id": base["run_id"] }),
            );
        }
    }
    let mut report = base;
    report["discarded"] = json!(discarded);
    if kept.is_empty() {
        return absent("all_discarded", report);
    }
    let manifest = crate::held_out::Manifest {
        version: 1,
        feature: a.feature.to_owned(),
        task: a.task.to_owned(),
        author_run: report["run_id"].as_str().unwrap_or_default().to_owned(),
        checkpoint,
        visible_scenarios: scenarios.iter().map(|s| s.id.clone()).collect(),
        files: kept.iter().map(|(f, _)| f.clone()).collect(),
        discarded,
    };
    let contents: Vec<(String, String)> = kept
        .iter()
        .map(|(f, c)| (f.path.clone(), c.clone()))
        .collect();
    match crate::held_out::store(store, &manifest, &contents) {
        Ok(sha) => {
            report["state"] = json!("on");
            report["manifest"] = json!(format!("sha256:{sha}"));
            report["kept"] = json!(kept.len());
            (report, Some(sha))
        }
        Err(e) => absent("generation_failed", json!({ "error": e })),
    }
}

/// The held-out verdict on the tree an attempt left: `Ok` with what the
/// tests said, or `Err` with why a person must look (tampering, a leak, a
/// run that would not leave the tree as it was).
pub(crate) struct HeldOutJudged {
    /// The verdict input.
    pub(crate) held_out: speccoder_core::verify::HeldOut,
    /// The failed scenarios, with the spec's words, for the retry digest.
    pub(crate) failed: Vec<speccoder_core::spec::ScenarioRef>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn judge_held_out(
    store: &Path,
    manifest_sha: &str,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    known_text: &str,
    wt_root: &Path,
    project: &Path,
    tmp: &Path,
    start: &worktree::Checkpoint,
    patch: &str,
    runner: &sandbox::Runner,
    policy: &tools::policy::Policy,
) -> Result<HeldOutJudged, (&'static str, String)> {
    use speccoder_core::verify::{CheckStatus, HeldOut};
    let (manifest, contents) =
        crate::held_out::load_verified(store, manifest_sha).map_err(|t| ("tampered", t.0))?;
    let files: Vec<(crate::held_out::HeldOutFile, String)> = manifest
        .files
        .iter()
        .cloned()
        .zip(contents.into_iter().map(|(_, c)| c))
        .collect();
    let leaks = crate::held_out::leaked(&files, known_text, wt_root, start, patch);
    if !leaks.is_empty() {
        return Err((
            "leak_suspected",
            format!(
                "the attempt contains {} literal(s) only the held-out tests hold; a person reviews it",
                leaks.len()
            ),
        ));
    }
    let runs = crate::held_out::overlay_run(wt_root, project, tmp, &files, runner, policy)
        .map_err(|e| ("tampered", e))?;
    let mut failed_ids: Vec<String> = Vec::new();
    for ((file, _), ran) in files.iter().zip(&runs) {
        let passed = ran
            .result
            .as_ref()
            .is_some_and(|(r, _)| r.status == CheckStatus::Passed);
        if !passed {
            failed_ids.extend(file.scenarios.iter().cloned());
        }
    }
    failed_ids.sort();
    failed_ids.dedup();
    let held_out = if failed_ids.is_empty() {
        HeldOut::Passed { files: files.len() }
    } else {
        HeldOut::Failed {
            scenarios: failed_ids.clone(),
        }
    };
    let failed = failed_ids
        .iter()
        .map(|id| {
            scenarios
                .iter()
                .find(|s| &s.id == id)
                .cloned()
                .unwrap_or_else(|| speccoder_core::spec::ScenarioRef {
                    id: id.clone(),
                    text: String::new(),
                })
        })
        .collect();
    Ok(HeldOutJudged { held_out, failed })
}

/// Added acceptance tests that strengthen a weak set.
pub(crate) struct Strengthened {
    /// The new test files, written into the worktree.
    pub(crate) files: Vec<String>,
    /// The command that runs them.
    pub(crate) argv: Vec<String>,
    /// The author's run and tokens.
    pub(crate) report: serde_json::Value,
}

/// Asks the test author once more, telling it what got through (`weak`),
/// and keeps its new tests only if they fail on the attempt's starting tree
/// (`before`) and pass on its final tree (`after`), where they are left.
/// `Err`: why the set could not be strengthened (no worktree change stays).
pub(crate) fn strengthen(
    a: &AcceptanceSetup<'_>,
    visible: &[(String, String)],
    weak: serde_json::Value,
    before: &worktree::Checkpoint,
    after: &worktree::Checkpoint,
) -> Result<Strengthened, String> {
    use serde_json::json;
    let files: Vec<String> = worktree::files(a.worktree, after)
        .unwrap_or_default()
        .into_iter()
        .take(400)
        .collect();
    let manifest = MANIFESTS
        .iter()
        .find_map(|m| {
            std::fs::read_to_string(a.project.join(m))
                .ok()
                .map(|t| format!("{m}:\n{t}"))
        })
        .map(|t| t.chars().take(8192).collect::<String>())
        .unwrap_or_default();
    let input = substrate::acceptance::strengthen_input(
        a.bundle.clone(),
        a.scenarios,
        &files,
        &manifest,
        visible,
        weak,
        a.instructions,
    );
    let outcome =
        a.rt.block_on(a.plane.acceptance(a.feature, a.task, input))
            .map_err(|e| format!("the test author failed: {}", e.0))?;
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let report = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"] });
    let answer = &output["answer"];
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return Err(format!(
            "the test author did not answer ({})",
            outcome.status.as_str()
        ));
    }
    let protected = a.policy.protected();
    let mut planned = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
            continue;
        };
        let rel = tools::paths::normalise(path).map_err(|e| format!("`{path}`: {}", e.0))?;
        if !tools::paths::is_test_path(&rel)
            || protected.write_class(&rel).is_some()
            || a.project.join(&rel).exists()
        {
            return Err(format!("`{rel}` is not a new test file"));
        }
        planned.push((rel, content.to_owned()));
    }
    if planned.is_empty() {
        return Err("the test author added no tests".to_owned());
    }
    let argv: Vec<String> = answer["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if a.policy.classify("run_command", &json!({ "argv": argv })) != tools::policy::Decision::Allow
    {
        return Err(format!("`{}` is not an allowed command", argv.join(" ")));
    }
    let write = || -> Result<(), String> {
        for (rel, content) in &planned {
            let full = a.project.join(rel);
            if let Some(parent) = full.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&full, content).map_err(|e| format!("`{rel}`: {e}"))?;
        }
        Ok(())
    };
    let back =
        |to: &worktree::Checkpoint| worktree::restore(a.worktree, a.tmp, to).map_err(|e| e.0);
    // Red on the starting tree: they fail without the task's work.
    back(before)?;
    let red = write().map(|()| {
        let (r, summary) = execute(a.project, &argv, a.runner);
        classify_red(&r, summary.as_ref())
    });
    back(after)?;
    let red = red?;
    if !red.is_red() {
        return Err(format!(
            "the added tests do not fail without the change ({})",
            class_word(red)
        ));
    }
    // Green on the final tree: the implementation passes them.
    write()?;
    let (green, _) = execute(a.project, &argv, a.runner);
    if green.status != speccoder_core::verify::CheckStatus::Passed {
        back(after)?;
        let cited: Vec<String> = answer["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|f| f["scenarios"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        return Err(format!(
            "the added tests fail the implementation{}",
            if cited.is_empty() {
                String::new()
            } else {
                format!(" (scenarios {})", cited.join(", "))
            }
        ));
    }
    Ok(Strengthened {
        files: planned.into_iter().map(|(p, _)| p).collect(),
        argv,
        report,
    })
}

/// Whether `task` completes its story: the last task carrying its story
/// label, in a story with at least two scenarios (US4).
pub(crate) fn story_checkpoint(
    tasks: Option<&speccoder_core::snapshot::ParsedTasks>,
    task: &str,
    scenarios: &[speccoder_core::spec::ScenarioRef],
) -> bool {
    let Some(tasks) = tasks else { return false };
    let Some(label) = tasks
        .tasks
        .iter()
        .find(|t| t.id == task)
        .and_then(|t| t.story.as_deref())
    else {
        return false;
    };
    let last = tasks
        .tasks
        .iter()
        .rfind(|t| t.story.as_deref() == Some(label))
        .map(|t| t.id.as_str());
    last == Some(task) && scenarios.len() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_task_of_a_story_with_two_scenarios_composes() {
        let (tasks, _) = speccoder_speckit::tasks::parse(
            "tasks.md",
            "## Phase 3: User Story 1 - Login\n\n- [ ] T005 [US1] a\n- [ ] T006 [US1] b\n\n## Phase 4: User Story 2 - X\n\n- [ ] T007 [US2] c\n",
        );
        let two: Vec<speccoder_core::spec::ScenarioRef> = ["US1-1", "US1-2"]
            .iter()
            .map(|id| speccoder_core::spec::ScenarioRef {
                id: (*id).to_owned(),
                text: String::new(),
            })
            .collect();
        assert!(story_checkpoint(Some(&tasks), "T006", &two));
        assert!(!story_checkpoint(Some(&tasks), "T005", &two));
        assert!(!story_checkpoint(Some(&tasks), "T006", &two[..1]));
        assert!(!story_checkpoint(None, "T006", &two));
    }
}
