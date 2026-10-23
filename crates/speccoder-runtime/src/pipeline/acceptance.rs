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
    pub(crate) runner: &'a sandbox::Runner,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedAcceptance {
    /// What the run report shows.
    pub(crate) report: serde_json::Value,
    /// The command, when the tests were written.
    pub(crate) argv: Option<Vec<String>>,
    /// They failed before the attempt, on an assertion or for an absent
    /// symbol (D-064).
    pub(crate) red: bool,
    /// What the red run showed.
    #[serde(default)]
    pub(crate) red_class: Option<speccoder_core::verify::RedClass>,
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
        }
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
    let input = substrate::acceptance::input(a.bundle.clone(), &files, &manifest);
    let outcome = a.rt.block_on(a.plane.acceptance(a.feature, a.task, input));
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => return skipped(format!("generation failed: {}", e.0), json!({})),
    };
    let output = outcome
        .output
        .as_ref()
        .map(|o| o.peek().clone())
        .unwrap_or_default();
    let answer = output["answer"].clone();
    let base = json!({ "run_id": outcome.run_id.to_string(), "usage": output["usage"], "notes": answer["notes"] });
    if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null() {
        return skipped(
            format!("generation did not succeed: {}", outcome.status.as_str()),
            base,
        );
    }

    // Validate: new test files only, a command the policy allows.
    let protected = tools::paths::Protected::default();
    let mut written = Vec::new();
    let mut planned = Vec::new();
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
    // second run (D-064).
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
