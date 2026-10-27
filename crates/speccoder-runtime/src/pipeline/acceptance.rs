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
    /// The project's test layout: where acceptance files go (`None`: the
    /// author places them by the old naming rule).
    pub(crate) layout: Option<&'a crate::layout::TestLayout>,
    /// The story files speccoder created on this branch (`Story-File:`
    /// trailers): the only existing files an author may add a section to.
    pub(crate) story_files: &'a [String],
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedAcceptance {
    /// What the run report shows.
    pub(crate) report: serde_json::Value,
    /// The command, when the tests were written.
    pub(crate) argv: Option<Vec<String>>,
    /// Further commands of the set (a bug's reproductions run one test
    /// each); every one must pass.
    #[serde(default)]
    pub(crate) more_argv: Vec<Vec<String>>,
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
    /// Why no acceptance test was kept, typed: what a regression-only
    /// reason is decided from (`report["skipped"]` is its display).
    #[serde(default)]
    pub(crate) skip_cause: Option<speccoder_core::regression::SkipCause>,
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
    use speccoder_core::regression::SkipCause;
    let skipped_by = |why: String, cause: Option<SkipCause>, report: serde_json::Value| {
        let mut report = report;
        report["skipped"] = json!(why);
        PreparedAcceptance {
            report,
            argv: None,
            more_argv: Vec::new(),
            red: false,
            red_class: None,
            held_out: None,
            generation_failed: None,
            skip_cause: cause,
        }
    };
    let failed = |why: String, report: serde_json::Value| PreparedAcceptance {
        generation_failed: Some(why.clone()),
        ..skipped_by(why, None, report)
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
    // Where the tests go: the story's file in the project's layout (one
    // file per story, grown by sections), a convergence task's own file,
    // or no place the runner collects (FR-028, FR-033).
    let placement = match placement(a) {
        Ok(p) => p,
        Err(()) => {
            return skipped_by(
                "no place the runner collects".to_owned(),
                Some(SkipCause::NoPlace),
                json!({}),
            );
        }
    };
    let input = substrate::acceptance::input(
        a.bundle.clone(),
        a.scenarios,
        &files,
        &manifest,
        a.instructions,
        placement.as_ref(),
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
    let mut written = Vec::new();
    let story = placement
        .as_ref()
        .and_then(|p| p.existing.as_deref().map(|old| (p.path.as_str(), old)));
    let (valid, refused) = validate_with(&answer, a.project, a.policy, a.scenarios, story);
    if let Some(r) = refused.first() {
        return skipped_by(
            r.message.clone(),
            Some(SkipCause::Refused {
                path: r.path.clone(),
                why: refusal_words(r.why).to_owned(),
            }),
            base,
        );
    }
    // Each test and the scenarios it cites, of those the spec has.
    let tests: Vec<serde_json::Value> = valid
        .iter()
        .map(|f| json!({ "path": f.path, "scenarios": f.cited }))
        .collect();
    let planned: Vec<(String, String)> = valid.into_iter().map(|f| (f.path, f.content)).collect();
    if planned.is_empty() {
        return skipped_by(
            "no acceptance tests were generated".to_owned(),
            Some(SkipCause::NoFiles),
            base,
        );
    }
    let argv: Vec<String> = answer["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if a.policy
        .classify("run_command", &json!({ "argv": argv }), None)
        != tools::policy::Decision::Allow
    {
        return skipped_by(
            format!("`{}` is not an allowed command", argv.join(" ")),
            Some(SkipCause::CommandNotAllowed {
                argv: argv.join(" "),
            }),
            base,
        );
    }
    for (rel, content) in &planned {
        let full = a.project.join(rel);
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&full, content) {
            return skipped_by(
                format!("`{rel}`: {e}"),
                Some(SkipCause::WriteFailed { path: rel.clone() }),
                base,
            );
        }
        written.push(rel.clone());
    }

    // Red: they must fail before the task is done — on an assertion, or
    // because what the task introduces does not exist yet; confirmed by a
    // second run.
    let (first, summary) = execute(a.project, &argv, a.runner);
    let red_tests = speccoder_core::verify::collected_count(&first, summary.as_ref());
    let mut class = classify_red(&first, summary.as_ref());
    // A section added to the story's file is red only by the tests it adds:
    // the same command on the file without it (FR-030).
    if let Some((path, old)) = story.filter(|(p, _)| written.iter().any(|w| w == p)) {
        let full = a.project.join(path);
        if let Ok(new) = std::fs::read_to_string(&full)
            && std::fs::write(&full, old).is_ok()
        {
            let (without, wsum) = execute(a.project, &argv, a.runner);
            let _ = std::fs::write(&full, new);
            class = speccoder_core::verify::classify_red_added(
                (&first, summary.as_ref()),
                (&without, wsum.as_ref()),
            );
        }
    }
    if class.is_red() {
        let (second, summary) = execute(a.project, &argv, a.runner);
        if !classify_red(&second, summary.as_ref()).is_red() {
            class = RedClass::Flaky;
        }
    }
    let red = class.is_red();
    let mut report = base;
    report["files"] = json!(written);
    if let Some(p) = placement.as_ref().filter(|p| p.story.is_some()) {
        report["story_file"] = json!(p.path);
        report["appended"] = json!(p.existing.is_some());
    }
    report["tests"] = json!(tests);
    report["argv"] = json!(argv);
    report["red"] = json!(red);
    report["red_class"] = json!(class);
    if let Some(n) = red_tests {
        report["red_tests"] = json!(n);
    }
    if red {
        // What the attestation's red excerpt is cut from (clean-checked there).
        report["red_output"] = json!(
            format!("{}\n{}", first.stdout_tail, first.stderr_tail)
                .trim()
                .to_owned()
        );
    } else {
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
        more_argv: Vec::new(),
        red,
        red_class: Some(class),
        held_out: None,
        generation_failed: None,
        skip_cause: None,
    }
}

/// Where the visible author writes, from the layout: `Ok(None)` when no
/// runner speccoder knows was found (the author names the files), `Err`
/// when the layout offers no place the runner collects.
fn placement(a: &AcceptanceSetup<'_>) -> Result<Option<substrate::acceptance::Placement>, ()> {
    let Some(layout) = a.layout.filter(|l| !l.runners.is_empty()) else {
        return Ok(None);
    };
    let task = a.task.to_owned();
    let story = a.bundle["task"]["story"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    // Go: the package the task names, else the module's root.
    let package =
        speccoder_core::scope::named_paths(a.bundle["task"]["title"].as_str().unwrap_or_default())
            .into_iter()
            .find(|p| p.ends_with(".go") || !p.contains('.'))
            .map(|p| {
                if p.ends_with(".go") {
                    p.rsplit_once('/')
                        .map_or(String::new(), |(d, _)| d.to_owned())
                } else {
                    p.trim_end_matches('/').to_owned()
                }
            });
    let per_task =
        |layout: &crate::layout::TestLayout| layout.story_file(&task, package.as_deref());
    let (path, story, existing) = match story.as_ref() {
        Some(st) => {
            let Some(path) = layout.story_file(st, package.as_deref()) else {
                return Err(());
            };
            let exists = a.project.join(&path).exists();
            if !exists {
                (path, story.clone(), None)
            } else if a.story_files.contains(&path) {
                let old = std::fs::read_to_string(a.project.join(&path)).unwrap_or_default();
                (path, story.clone(), Some(old))
            } else {
                // A file of the person's at that path: this task's own file.
                (per_task(layout).ok_or(())?, None, None)
            }
        }
        None => (per_task(layout).ok_or(())?, None, None),
    };
    let comment = if path.ends_with(".py") { "#" } else { "//" }.to_owned();
    Ok(Some(substrate::acceptance::Placement {
        command: layout.command_for(&path).unwrap_or_default(),
        path,
        existing,
        task,
        story,
        scenarios: a.scenarios.iter().map(|s| s.id.clone()).collect(),
        comment,
        layout: layout.text(),
    }))
}

/// speccoder's words for a refusal's kind (a regression-only reason's detail).
pub(crate) fn refusal_words(why: &str) -> &'static str {
    match why {
        "exists" => "already exists",
        "not_collected" => "is not where the runner collects tests",
        "changes_existing" => "changes existing lines",
        _ => "is not a new test file",
    }
}

/// One test file of an author's answer that may be written.
pub(crate) struct ValidFile {
    /// Normalised, project-relative.
    pub(crate) path: String,
    pub(crate) content: String,
    /// The scenario ids it cites, of those given (may be empty).
    pub(crate) cited: Vec<String>,
    /// The command that runs only it, when the answer gives one.
    pub(crate) argv: Vec<String>,
}

/// A file of an answer that may not be written, and why.
pub(crate) struct RefusedFile {
    /// Its path as given.
    pub(crate) path: String,
    /// `not_a_new_test_file` or `exists`.
    pub(crate) why: &'static str,
    /// What a person is told.
    pub(crate) message: String,
}

/// The acceptance author's files, checked (the one validation run time and
/// review share): new test paths only, nothing protected, nothing that
/// exists in `project`; each with the scenarios it cites of `scenarios`.
pub(crate) fn validate_answer(
    answer: &serde_json::Value,
    project: &Path,
    policy: &tools::policy::Policy,
    scenarios: &[speccoder_core::spec::ScenarioRef],
) -> (Vec<ValidFile>, Vec<RefusedFile>) {
    validate_with(answer, project, policy, scenarios, None)
}

/// [`validate_answer`] with the story's file: the one existing path an
/// answer may write, and only by adding lines to `old` (D-048).
pub(crate) fn validate_with(
    answer: &serde_json::Value,
    project: &Path,
    policy: &tools::policy::Policy,
    scenarios: &[speccoder_core::spec::ScenarioRef],
    story: Option<(&str, &str)>,
) -> (Vec<ValidFile>, Vec<RefusedFile>) {
    let protected = policy.protected();
    let mut valid = Vec::new();
    let mut refused = Vec::new();
    for f in answer["files"].as_array().into_iter().flatten() {
        let (Some(path), Some(content)) = (f["path"].as_str(), f["content"].as_str()) else {
            continue;
        };
        let rel = match tools::paths::normalise(path) {
            Ok(r) => r,
            Err(e) => {
                refused.push(RefusedFile {
                    path: path.to_owned(),
                    why: "not_a_new_test_file",
                    message: format!("`{path}`: {}", e.0),
                });
                continue;
            }
        };
        if !tools::paths::is_test_path(&rel) || protected.write_class(&rel).is_some() {
            refused.push(RefusedFile {
                message: format!("`{rel}` is not a new test file"),
                path: rel,
                why: "not_a_new_test_file",
            });
            continue;
        }
        match story {
            Some((path, old)) if path == rel => {
                if content == old || !crate::integrity::only_additions(old, content) {
                    refused.push(RefusedFile {
                        message: format!("changes existing lines of `{rel}`"),
                        path: rel,
                        why: "changes_existing",
                    });
                    continue;
                }
            }
            _ if project.join(&rel).exists() => {
                refused.push(RefusedFile {
                    message: format!("`{rel}` already exists"),
                    path: rel,
                    why: "exists",
                });
                continue;
            }
            _ => {
                // A new file the runner would never collect is no evidence.
                if crate::collection::statically_refused(project, &rel, content).is_some() {
                    refused.push(RefusedFile {
                        message: format!("`{rel}` is not where the runner collects tests"),
                        path: rel,
                        why: "not_collected",
                    });
                    continue;
                }
            }
        }
        let cited: Vec<String> = f["scenarios"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter(|id| scenarios.iter().any(|s| s.id == *id))
            .map(str::to_owned)
            .collect();
        let argv: Vec<String> = f["argv"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        valid.push(ValidFile {
            path: rel,
            content: content.to_owned(),
            cited,
            argv,
        });
    }
    (valid, refused)
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
        a.layout.map(crate::layout::TestLayout::text).as_deref(),
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
        if a.policy
            .classify("run_command", &json!({ "argv": argv }), None)
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
                red_tests: None,
            },
            content.to_owned(),
        ));
    }

    // Red now, on the tree the attempt starts from.
    let checkpoint = worktree::checkpoint(a.worktree, a.tmp)
        .map(|c| c.0)
        .unwrap_or_default();
    // Red by what the file adds: a failure its command shows without it is
    // another test's.
    match crate::held_out::overlay_red(a.worktree, a.project, a.tmp, &kept, a.runner, a.policy) {
        Ok(runs) => {
            let mut red = Vec::new();
            for ((mut file, content), ran) in kept.into_iter().zip(runs) {
                if ran.result.is_none() {
                    discard(&file.path, "collision");
                } else if crate::held_out::red_class(&ran).is_red() {
                    file.red_tests = ran
                        .result
                        .as_ref()
                        .and_then(|(r, s)| speccoder_core::verify::collected_count(r, s.as_ref()));
                    red.push((file, content));
                } else {
                    discard(&file.path, "not_red");
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
        // Green is evidence only when it collected what red did (FR-008).
        let passed = ran.result.as_ref().is_some_and(|(r, sum)| {
            r.status == CheckStatus::Passed
                && file.red_tests.is_none_or(|red| {
                    speccoder_core::verify::collected_count(r, sum.as_ref())
                        .is_some_and(|green| green >= red)
                })
        });
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
    if a.policy
        .classify("run_command", &json!({ "argv": argv }), None)
        != tools::policy::Decision::Allow
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
/// label, in a story with at least two scenarios.
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

    /// One pre-frozen record of `producer` naming `tests/a.txt`.
    fn record(
        producer: speccoder_core::acceptance::Producer,
        content: &str,
    ) -> speccoder_core::acceptance::PreFrozenAcceptance {
        speccoder_core::acceptance::PreFrozenAcceptance {
            unit: "001-x".to_owned(),
            requirement: "US1".to_owned(),
            wording_digest: "sha256:w".to_owned(),
            approval_digest: "sha256:a".to_owned(),
            visible: vec![speccoder_core::acceptance::VisibleTest {
                path: "tests/a.txt".to_owned(),
                sha256: format!(
                    "sha256:{}",
                    crate::integrity::sha256_hex(content.as_bytes())
                ),
                cites: vec!["US1-1".to_owned()],
            }],
            held_out_sealed: None,
            producer,
            decided: speccoder_core::acceptance::Decided {
                by: "Ada".to_owned(),
                via: speccoder_core::actions::Via::Terminal,
                at: 0,
            },
        }
    }

    #[test]
    fn a_review_s_and_a_bug_s_frozen_sets_go_through_the_one_reuse_each_with_its_red_rule() {
        use speccoder_core::acceptance::Producer;
        let summary = |passed: u8, failed: u8| {
            format!(
                "echo 'test result: {}. {passed} passed; {failed} failed; 0 ignored; 0 measured; 0 filtered out'; exit {}",
                if failed > 0 { "FAILED" } else { "ok" },
                if failed > 0 { 101 } else { 0 }
            )
        };
        let content = "checks\n";
        for (producer, rule) in [
            (Producer::Review, RedRule::RunTime),
            (Producer::BugConfirmation, RedRule::BugAssertionOnly),
        ] {
            let rec = record(producer, content);
            let files = vec![("tests/a.txt".to_owned(), content.to_owned())];
            let run = |script: String| {
                let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
                let tests = [ReusedTest {
                    path: "tests/a.txt".to_owned(),
                    cites: vec!["US1-1".to_owned()],
                    argv: vec!["sh".to_owned(), "-c".to_owned(), script],
                    green: false,
                }];
                let got = reuse_pre_frozen(
                    dir.path(),
                    &rec,
                    &files,
                    &tests,
                    &sandbox::Runner::Plain,
                    rule,
                    None,
                );
                assert!(
                    dir.path().join("tests/a.txt").is_file(),
                    "written at the checkpoint"
                );
                got
            };
            // Red by one assertion, twice: reused, the source the producer's.
            let red = run(summary(0, 1)).unwrap_or_else(|e| panic!("{producer:?}: {e}"));
            assert!(red.red);
            assert_eq!(red.report["source"], serde_json::json!(producer.source()));
            // Already passing: a review's set is not demonstrated, a bug's
            // reproduction no longer reproduces — the person looks.
            let passing = run(summary(1, 0));
            match rule {
                RedRule::RunTime => {
                    let p = passing.unwrap_or_else(|e| panic!("{e}"));
                    assert!(!p.red);
                    assert!(
                        p.report["note"]
                            .as_str()
                            .is_some_and(|n| n.contains("already passing"))
                    );
                }
                RedRule::BugAssertionOnly => assert!(passing.is_err()),
            }
            // A failure of more than one test is a bug's no reproduction.
            if rule == RedRule::BugAssertionOnly {
                assert!(run(summary(1, 1)).is_err(), "one test's assertion only");
            }
        }
    }
}

/// How a reused set must be red at the checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RedRule {
    /// A review's oracle: the run-time rule (an assertion, or an absent
    /// symbol), twice; a set that is no longer red is reported as at run
    /// time — not demonstrated — rather than refused.
    RunTime,
    /// A bug's reproductions: an assertion only, both runs.
    BugAssertionOnly,
}

/// One test of a reused set: its file, what it cites and how it runs.
#[derive(Debug)]
pub(crate) struct ReusedTest {
    /// Project-relative path.
    pub(crate) path: String,
    /// The statement or scenario ids.
    pub(crate) cites: Vec<String>,
    /// The command that runs only it.
    pub(crate) argv: Vec<String>,
    /// It must pass at the checkpoint instead (an *Unchanged* test).
    pub(crate) green: bool,
}

/// Where a reused set's sealed held-out tests are red-checked: overlaid on
/// the checkpoint, run and removed.
pub(crate) struct SealedCheck<'a> {
    /// The project's state directory.
    pub(crate) state: &'a Path,
    /// The unit the set is sealed under.
    pub(crate) unit: &'a str,
    /// The worktree's root.
    pub(crate) wt_root: &'a Path,
    /// Scratch for checkpoints.
    pub(crate) tmp: &'a Path,
    /// The task's policy.
    pub(crate) policy: &'a tools::policy::Policy,
}

/// Acceptance tests decided before the run, reused at the checkpoint:
/// written into the worktree (the one function every pre-frozen producer
/// uses), each red twice by `rule` — or green, for a test that guards what
/// must not change — with no acceptance author call. `Err`: why the set
/// cannot stand (the person looks).
pub(crate) fn reuse_pre_frozen(
    project: &Path,
    record: &speccoder_core::acceptance::PreFrozenAcceptance,
    files: &[(String, String)],
    tests: &[ReusedTest],
    runner: &sandbox::Runner,
    rule: RedRule,
    sealed: Option<&SealedCheck<'_>>,
) -> Result<PreparedAcceptance, String> {
    use serde_json::json;
    for (rel, content) in files {
        let full = project.join(rel);
        if let Ok(now) = std::fs::read_to_string(&full) {
            if now != *content {
                return Err(format!(
                    "`{rel}` exists and differs from the confirmed test; it is not overwritten"
                ));
            }
            continue;
        }
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&full, content).map_err(|e| format!("`{rel}`: {e}"))?;
    }
    // A bug's reproduction is red only by its own test: a run of more than
    // one test cannot say whose assertion failed.
    let class_of = |r: &speccoder_core::verify::CheckResult,
                    s: Option<&speccoder_core::verify::RunnerSummary>| match rule
    {
        RedRule::BugAssertionOnly => speccoder_core::verify::classify_red_one(r, s, ""),
        RedRule::RunTime => classify_red(r, s),
    };
    let mut classes = Vec::new();
    for t in tests {
        let (first, summary) = execute(project, &t.argv, runner);
        let one = class_of(&first, summary.as_ref());
        let class = if t.green {
            one
        } else {
            match rule {
                RedRule::BugAssertionOnly if one == RedClass::Assertion => {
                    let (second, summary) = execute(project, &t.argv, runner);
                    speccoder_core::bug::bug_class(one, Some(class_of(&second, summary.as_ref())))
                }
                RedRule::BugAssertionOnly => one,
                RedRule::RunTime if one.is_red() => {
                    let (second, summary) = execute(project, &t.argv, runner);
                    if classify_red(&second, summary.as_ref()).is_red() {
                        one
                    } else {
                        RedClass::Flaky
                    }
                }
                RedRule::RunTime => one,
            }
        };
        let holds = if t.green {
            class == RedClass::Passed
        } else {
            match rule {
                RedRule::BugAssertionOnly => class == RedClass::Assertion,
                RedRule::RunTime => class.is_red(),
            }
        };
        if !holds && rule == RedRule::RunTime && !t.green {
            return Ok(not_red_frozen(record, files, tests, t, class));
        }
        if !holds {
            return Err(if t.green {
                format!(
                    "the confirmed test `{}` ({}) no longer passes on the branch ({})",
                    t.path,
                    t.cites.join(", "),
                    class_word(class)
                )
            } else {
                format!(
                    "the confirmed test `{}` ({}) no longer fails on an assertion ({}): the behaviour may have changed on the branch",
                    t.path,
                    t.cites.join(", "),
                    class_word(class)
                )
            });
        }
        classes.push(class);
    }
    let red: Vec<&ReusedTest> = tests.iter().filter(|t| !t.green).collect();
    let Some(first) = red.first() else {
        return Err("the confirmed set has no test that must fail first".to_owned());
    };
    let more: Vec<Vec<String>> = tests
        .iter()
        .filter(|t| !std::ptr::eq(*t, *first))
        .map(|t| t.argv.clone())
        .collect();
    let mut held_sha = record
        .held_out_sealed
        .as_ref()
        .map(|h| h.manifest_sha256.clone());
    let mut held_out = match &record.held_out_sealed {
        Some(h) => {
            json!({ "state": "on", "manifest": format!("sha256:{}", h.manifest_sha256), "kept": h.count })
        }
        None => json!({ "state": "absent", "reason": "none_kept" }),
    };
    // The sealed held-out set, red on the same checkpoint (overlaid, run,
    // removed): a set that is not red decides nothing.
    if let (Some(check), Some(h)) = (sealed, &record.held_out_sealed) {
        let (manifest, contents) = crate::held_out::open_sealed(check.state, check.unit, h)
            .map_err(|t| format!("the frozen held-out set was tampered with: {}", t.0))?;
        let set: Vec<(crate::held_out::HeldOutFile, String)> = manifest
            .files
            .iter()
            .cloned()
            .zip(contents.into_iter().map(|(_, c)| c))
            .collect();
        let runs = crate::held_out::overlay_red(
            check.wt_root,
            project,
            check.tmp,
            &set,
            runner,
            check.policy,
        )?;
        let all_red = runs
            .iter()
            .all(|r| r.result.is_some() && crate::held_out::red_class(r).is_red());
        if !all_red {
            held_sha = None;
            held_out = json!({ "state": "absent", "reason": "not_red" });
        }
    }
    let class = classes.first().copied().unwrap_or(RedClass::Assertion);
    Ok(PreparedAcceptance {
        report: json!({
            "files": files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            "tests": tests.iter().map(|t| json!({ "path": t.path, "scenarios": t.cites })).collect::<Vec<_>>(),
            "argv": first.argv,
            "red": true,
            "red_class": class,
            "source": record.producer.source(),
            "pre_frozen": {
                "manifest": crate::pre_frozen::manifest_digest(record),
                "approval": record.approval_digest,
                "wording": record.wording_digest,
                "by": record.decided.by,
                "via": record.decided.via,
            },
            "held_out": held_out,
        }),
        argv: Some(first.argv.clone()),
        more_argv: more,
        red: true,
        red_class: Some(class),
        held_out: held_sha,
        generation_failed: None,
        skip_cause: None,
    })
}

/// A frozen set no longer red at the checkpoint: reported as a run-time
/// set that is not red — not demonstrated — with nothing re-authored.
fn not_red_frozen(
    record: &speccoder_core::acceptance::PreFrozenAcceptance,
    files: &[(String, String)],
    tests: &[ReusedTest],
    failing: &ReusedTest,
    class: RedClass,
) -> PreparedAcceptance {
    use serde_json::json;
    let note = match class {
        RedClass::Passed => "frozen at review, already passing: the acceptance tests already pass before the attempt, so they are no evidence for it".to_owned(),
        RedClass::Flaky => "frozen at review: the acceptance tests failed once and passed when run again: not demonstrated".to_owned(),
        other => format!(
            "frozen at review: the acceptance tests did not fail on an assertion ({}): not demonstrated",
            class_word(other)
        ),
    };
    PreparedAcceptance {
        report: json!({
            "files": files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            "tests": tests.iter().map(|t| json!({ "path": t.path, "scenarios": t.cites })).collect::<Vec<_>>(),
            "argv": failing.argv,
            "red": false,
            "red_class": class,
            "note": note,
            "source": record.producer.source(),
            "pre_frozen": {
                "manifest": crate::pre_frozen::manifest_digest(record),
                "approval": record.approval_digest,
                "wording": record.wording_digest,
                "by": record.decided.by,
                "via": record.decided.via,
            },
            "held_out": { "state": "absent", "reason": "not_red" },
        }),
        argv: Some(failing.argv.clone()),
        more_argv: tests
            .iter()
            .filter(|t| !std::ptr::eq(*t, failing))
            .map(|t| t.argv.clone())
            .collect(),
        red: false,
        red_class: Some(class),
        held_out: None,
        generation_failed: None,
        skip_cause: None,
    }
}
