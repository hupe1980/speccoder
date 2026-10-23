//! Public benchmarks as evaluation cases.
//!
//! `speccoder bench import` turns instances in the SWE-bench format
//! (`instance_id`, `repo`, `base_commit`, `problem_statement`, `test_patch`,
//! `FAIL_TO_PASS`, …: SWE-bench, SWE-bench Verified/Multilingual/Pro, and
//! benchmarks that reuse the format) into cases of the evaluation suite:
//!
//! - `project/`: the repository at `base_commit` as a Spec Kit project — the
//!   issue becomes the user story of `spec.md`, with one task to resolve it;
//! - `oracle/`: the files the benchmark's `test_patch` adds or changes, which
//!   the agent never sees and the oracle runs after the attempt;
//! - `case.toml`: the task, `expect = "complete"`, the test command.
//!
//! Then `speccoder eval <out> [--executor claude]` runs them like any case.
//! Downloading the datasets and cloning the repositories is the person's
//! (`--repos` holds one clone per repository, named `owner__name`), and so
//! is the environment the tests need: SWE-bench runs each instance in a
//! container image; here the tests run on this machine, in the sandbox.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use speccoder_core::{BenchReport, CommandData, CommandResult, CommandStatus};

use crate::git::Git;

/// One benchmark instance.
#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    /// e.g. `owner__repo-1234`.
    pub instance_id: String,
    /// `owner/name`.
    pub repo: String,
    /// The commit the issue was filed against.
    pub base_commit: String,
    /// The issue text.
    pub problem_statement: String,
    /// The tests that decide the instance, as a patch.
    pub test_patch: String,
    /// Tests that must go from failing to passing (a list, or a JSON string of one).
    #[serde(default, rename = "FAIL_TO_PASS")]
    pub fail_to_pass: Value,
    /// The command that runs the tests, when the dataset gives one.
    #[serde(default)]
    pub test_cmd: Option<Value>,
}

/// How to import.
#[derive(Debug, Clone)]
pub struct ImportOptions<'a> {
    /// One clone per repository: `<repos>/<owner>__<name>`.
    pub repos: &'a Path,
    /// The suite directory to write cases into.
    pub out: &'a Path,
    /// The test command, if the dataset has none (default: detected).
    pub test_cmd: Option<Vec<String>>,
    /// At most this many instances.
    pub limit: Option<usize>,
}

fn names(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a
            .iter()
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) => serde_json::from_str::<Vec<String>>(s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The test command for a project, by what it contains.
fn detect_test_cmd(project: &Path) -> Option<Vec<String>> {
    let cmd = |a: &[&str]| Some(a.iter().map(|s| (*s).to_owned()).collect());
    if project.join("Cargo.toml").exists() {
        cmd(&["cargo", "test", "--quiet"])
    } else if project.join("package.json").exists() {
        cmd(&["npm", "test", "--silent"])
    } else if project.join("pyproject.toml").exists() || project.join("setup.py").exists() {
        cmd(&["python3", "-m", "pytest", "-q"])
    } else if project.join("go.mod").exists() {
        cmd(&["go", "test", "./..."])
    } else {
        None
    }
}

fn slug(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn toml_list(argv: &[String]) -> String {
    let items: Vec<String> = argv.iter().map(|a| format!("{a:?}")).collect();
    format!("[{}]", items.join(", "))
}

/// Paths a patch writes (`+++ b/<path>`), deletions excluded.
fn patched_paths(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|p| p.trim().to_owned())
        .collect()
}

/// A checkout of `src` at `commit` in `dir`, without history.
fn checkout(src: &Path, commit: &str, dir: &Path) -> Result<(), String> {
    let parent = dir.parent().ok_or("no parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    Git::at(parent)
        .run(&[
            "clone",
            "--quiet",
            "--no-checkout",
            &src.to_string_lossy(),
            &dir.to_string_lossy(),
        ])
        .map_err(|e| e.0)?;
    Git::at(dir)
        .run(&["checkout", "--quiet", commit])
        .map_err(|e| e.0)?;
    Ok(())
}

fn import_one(inst: &Instance, opts: &ImportOptions<'_>) -> Result<PathBuf, String> {
    let src = opts.repos.join(inst.repo.replace('/', "__"));
    if !src.join(".git").exists() && !src.join("HEAD").exists() {
        return Err(format!(
            "no clone of {} at {} (git clone https://github.com/{} {})",
            inst.repo,
            src.display(),
            inst.repo,
            src.display()
        ));
    }
    let case = opts.out.join(slug(&inst.instance_id));
    if case.exists() {
        return Err(format!("{} exists", case.display()));
    }
    let project = case.join("project");
    checkout(&src, &inst.base_commit, &project)?;
    fs::remove_dir_all(project.join(".git")).map_err(|e| e.to_string())?;

    // The oracle: the files the test patch writes, taken from a checkout
    // with the patch applied — the agent's worktree never has them.
    let scratch = case.join(".oracle-checkout");
    checkout(&src, &inst.base_commit, &scratch)?;
    let applied = Git::at(&scratch).run_with_stdin(
        &["apply", "--whitespace=nowarn", "-"],
        inst.test_patch.as_bytes(),
    );
    if let Err(e) = applied {
        let _ = fs::remove_dir_all(&case);
        return Err(format!("the test patch does not apply: {}", e.0));
    }
    for rel in patched_paths(&inst.test_patch) {
        let from = scratch.join(&rel);
        let to = case.join("oracle").join(&rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&from, &to).map_err(|e| format!("{rel}: {e}"))?;
    }
    fs::remove_dir_all(&scratch).map_err(|e| e.to_string())?;

    let test_cmd = inst
        .test_cmd
        .as_ref()
        .and_then(|v| match v {
            Value::String(s) => Some(s.split_whitespace().map(str::to_owned).collect::<Vec<_>>()),
            Value::Array(_) => Some(names(v)),
            _ => None,
        })
        .or_else(|| opts.test_cmd.clone())
        .or_else(|| detect_test_cmd(&project))
        .ok_or("no test command: pass --test-cmd")?;

    // The Spec Kit project around the issue.
    let feature = format!("001-{}", slug(&inst.instance_id));
    let fdir = project.join("specs").join(&feature);
    fs::create_dir_all(&fdir).map_err(|e| e.to_string())?;
    fs::create_dir_all(project.join(".specify/memory")).map_err(|e| e.to_string())?;
    let write = |p: PathBuf, text: String| fs::write(p, text).map_err(|e| e.to_string());
    write(
        project.join(".specify/init-options.json"),
        "{\"speckit_version\": \"1.2.3\", \"integration\": \"claude\", \"script\": \"sh\"}\n"
            .to_owned(),
    )?;
    write(
        project.join(".specify/memory/constitution.md"),
        "# Constitution\n\n### I. Minimal\nChange only what the issue needs; keep existing behaviour and tests.\n\n### II. Honest\nA requirement that cannot be met as written is reported, never worked around.\n".to_owned(),
    )?;
    let first_line = inst
        .problem_statement
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Resolve the issue");
    let title: String = first_line
        .trim()
        .trim_start_matches('#')
        .trim()
        .chars()
        .take(70)
        .collect();
    write(
        fdir.join("spec.md"),
        format!(
            "# Feature Specification: {}\n\n## User Scenarios & Testing\n\n### User Story 1 - {title} (Priority: P1)\n\nAn issue reported against `{}`:\n\n{}\n\n## Requirements\n\n- **FR-001**: The behaviour the issue asks for works.\n- **FR-002**: Existing behaviour and tests are unchanged.\n",
            inst.instance_id,
            inst.repo,
            inst.problem_statement.trim()
        ),
    )?;
    write(
        fdir.join("plan.md"),
        format!(
            "# Implementation Plan: {}\n\nExisting repository `{}` at `{}`. Find the code the issue concerns and change it there; do not rewrite unrelated code. Tests: `{}`.\n",
            inst.instance_id,
            inst.repo,
            inst.base_commit,
            test_cmd.join(" ")
        ),
    )?;
    write(
        fdir.join("tasks.md"),
        format!(
            "# Tasks: {}\n\n## Phase 1: User Story 1 - {title} (Priority: P1)\n\n- [ ] T001 [US1] Resolve the issue described in spec.md, with a test that fails before the change and passes after\n",
            inst.instance_id
        ),
    )?;
    write(
        project.join("speccoder.toml"),
        format!(
            "[checks.test]\nargv = {}\ntimeout_secs = 900\n",
            toml_list(&test_cmd)
        ),
    )?;
    let fail_to_pass = names(&inst.fail_to_pass);
    write(
        case.join("case.toml"),
        format!(
            "# Imported from {} ({}).\n# FAIL_TO_PASS: {}\ntask = \"T001\"\nexpect = \"complete\"\n\n[oracle]\nargv = {}\n",
            inst.instance_id,
            inst.repo,
            fail_to_pass.join(", "),
            toml_list(&test_cmd)
        ),
    )?;
    Ok(case)
}

/// `speccoder bench import`.
#[must_use]
pub fn import(instances: &Path, opts: &ImportOptions<'_>) -> CommandResult {
    let text = match fs::read_to_string(instances) {
        Ok(t) => t,
        Err(e) => {
            return CommandResult::new(
                "bench",
                CommandStatus::NotFound,
                &format!("{}: {e}", instances.display()),
            );
        }
    };
    // JSON Lines, or one JSON array.
    let parsed: Result<Vec<Instance>, String> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text).map_err(|e| e.to_string())
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
            .collect()
    };
    let list = match parsed {
        Ok(l) => l,
        Err(e) => {
            return CommandResult::new(
                "bench",
                CommandStatus::ConfigError,
                &format!("{}: {e}", instances.display()),
            );
        }
    };
    let mut report = BenchReport {
        out: opts.out.display().to_string(),
        imported: Vec::new(),
        skipped: Vec::new(),
    };
    for inst in list.iter().take(opts.limit.unwrap_or(usize::MAX)) {
        match import_one(inst, opts) {
            Ok(_) => report.imported.push(inst.instance_id.clone()),
            Err(e) => report.skipped.push(format!("{}: {e}", inst.instance_id)),
        }
    }
    let suite = opts.out.join(crate::eval::SUITE);
    if !report.imported.is_empty() && !suite.exists() {
        let _ = fs::write(
            &suite,
            "# The model every case runs with; the key comes from the environment.\n[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\nmax_output_tokens = 8000\n\n[budget]\nmax_turns = 40\nmax_tool_calls = 80\n",
        );
    }
    let status = if report.imported.is_empty() {
        CommandStatus::NotFound
    } else {
        CommandStatus::Ok
    };
    let message = format!(
        "{} imported, {} skipped; run them with `speccoder eval {}`",
        report.imported.len(),
        report.skipped.len(),
        opts.out.display()
    );
    CommandResult::new("bench", status, &message).with_data(CommandData::Bench(Box::new(report)))
}
