//! Mutation testing as evidence: after a passing verdict, the
//! attempt's changed code is mutated and the tests run against each mutant.
//! A mutant the tests do not catch ("survivor") is code the tests do not
//! really check — the shape of a test written to pass rather than to verify.
//!
//! `[mutation] tool = "cargo-mutants"` runs `cargo mutants --in-diff` on
//! the attempt's patch. `tool = "command"` runs any tool that writes the
//! standard mutation-testing JSON report (Stryker, Infection, PIT with its
//! plugin, …): `argv` may use `{diff}`, `{out}`, `{files}` (one argument per
//! changed source file) and `{files_csv}`, and `report` names the JSON file.
//!
//! `mode = "report"` (default) records the result as evidence; `"require"`
//! fails the verdict when mutants survive, and the retry digest names them.
//!
//! The same tool measures the acceptance set's strength: the
//! mutants run against the acceptance command only ([`run_against`]).
//! `suite = false` keeps the tool for that alone; `acceptance_argv` is the
//! command tool's invocation then, with `{tests}` for the acceptance
//! command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::verify::process::{CommandRunner, Invocation, OUTPUT_CAP, scrubbed_env};

/// Which tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    /// `cargo mutants --in-diff`.
    CargoMutants,
    /// Any command writing the mutation-testing JSON report at `report`.
    Command {
        /// With placeholders.
        argv: Vec<String>,
        /// Relative to the project (or `{out}`).
        report: String,
    },
}

/// What survivors mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Evidence only.
    Report,
    /// Survivors fail the verdict (and are retried with the digest).
    Require,
}

/// `[mutation]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Which tool.
    pub tool: Tool,
    /// What survivors mean.
    pub mode: Mode,
    /// The whole run's limit.
    pub timeout: Duration,
    /// Run against the whole suite after a passing verdict; `false`:
    /// the tool serves acceptance strength only.
    pub suite: bool,
    /// The command tool's argv against the acceptance tests (`{tests}`).
    pub acceptance_argv: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    tool: String,
    #[serde(default)]
    argv: Vec<String>,
    report: Option<String>,
    mode: Option<String>,
    timeout_secs: Option<u64>,
    suite: Option<bool>,
    acceptance_argv: Option<Vec<String>>,
}

/// Reads `[mutation]`; `None` when absent.
pub fn parse(text: &str) -> Result<Option<Config>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(v) = table.get("mutation") else {
        return Ok(None);
    };
    let raw: Raw = v
        .clone()
        .try_into()
        .map_err(|e| format!("[mutation]: {e}"))?;
    let tool = match raw.tool.as_str() {
        "cargo-mutants" => Tool::CargoMutants,
        "command" => match (raw.argv.is_empty(), raw.report) {
            (false, Some(report)) => Tool::Command {
                argv: raw.argv,
                report,
            },
            _ => return Err("[mutation]: tool = \"command\" needs `argv` and `report`".to_owned()),
        },
        o => {
            return Err(format!(
                "[mutation] tool = \"{o}\" (cargo-mutants or command)"
            ));
        }
    };
    let mode = match raw.mode.as_deref() {
        None | Some("report") => Mode::Report,
        Some("require") => Mode::Require,
        Some(o) => return Err(format!("[mutation] mode = \"{o}\" (report or require)")),
    };
    if raw.acceptance_argv.is_some() && tool == Tool::CargoMutants {
        return Err(
            "[mutation]: acceptance_argv is for tool = \"command\"; cargo-mutants runs the acceptance command itself".to_owned(),
        );
    }
    Ok(Some(Config {
        tool,
        mode,
        timeout: Duration::from_secs(raw.timeout_secs.unwrap_or(900)),
        suite: raw.suite.unwrap_or(true),
        acceptance_argv: raw.acceptance_argv.filter(|a| !a.is_empty()),
    }))
}

/// Source files a patch changes (`+++ b/<path>`), tests excluded.
#[must_use]
pub fn changed_sources(patch: &str) -> Vec<String> {
    let mut out: Vec<String> = patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(str::to_owned)
        .filter(|p| !crate::tools::paths::is_test_path(p))
        .collect();
    out.dedup();
    out
}

/// `patch` with only the sections of changed source files: test files —
/// the acceptance tests among them — removed.
#[must_use]
pub fn source_patch(patch: &str) -> String {
    let mut out = String::new();
    let mut keep = false;
    for line in patch.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.trim_end().rsplit(" b/").next().unwrap_or_default();
            keep = !crate::tools::paths::is_test_path(path);
        }
        if keep {
            out.push_str(line);
        }
    }
    out
}

/// The result of `cargo mutants` (`mutants.out/outcomes.json`).
#[must_use]
pub fn from_cargo_mutants(outcomes: &Value) -> Value {
    let n = |k: &str| outcomes[k].as_u64().unwrap_or(0);
    let survivors: Vec<String> = outcomes["outcomes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["summary"] == "MissedMutant")
        .filter_map(|o| o["scenario"]["Mutant"]["name"].as_str().map(str::to_owned))
        .collect();
    json!({
        "tool": "cargo-mutants",
        "caught": n("caught"),
        "missed": n("missed"),
        "timeout": n("timeout"),
        "unviable": n("unviable"),
        "survivors": survivors,
    })
}

/// The result in the mutation-testing report schema (Stryker and others):
/// `files.<path>.mutants[].status` — `Killed`, `Survived`, `NoCoverage`,
/// `Timeout`, `CompileError`, `RuntimeError`, `Ignored`.
#[must_use]
pub fn from_report_schema(report: &Value, tool: &str) -> Value {
    let (mut caught, mut missed, mut timeout, mut unviable) = (0u64, 0u64, 0u64, 0u64);
    let mut survivors = Vec::new();
    for (file, f) in report["files"].as_object().into_iter().flatten() {
        for m in f["mutants"].as_array().into_iter().flatten() {
            match m["status"].as_str().unwrap_or_default() {
                "Killed" => caught += 1,
                "Survived" | "NoCoverage" => {
                    missed += 1;
                    survivors.push(format!(
                        "{file}:{}: {} → {}",
                        m["location"]["start"]["line"],
                        m["mutatorName"].as_str().unwrap_or("mutant"),
                        m["replacement"].as_str().unwrap_or("?")
                    ));
                }
                "Timeout" => timeout += 1,
                "CompileError" | "RuntimeError" => unviable += 1,
                _ => {}
            }
        }
    }
    json!({ "tool": tool, "caught": caught, "missed": missed, "timeout": timeout, "unviable": unviable, "survivors": survivors })
}

fn expand(argv: &[String], diff: &Path, out: &Path, files: &[String], tests: &str) -> Vec<String> {
    let mut res = Vec::new();
    for a in argv {
        match a.as_str() {
            "{files}" => res.extend(files.iter().cloned()),
            _ => res.push(
                a.replace("{diff}", &diff.to_string_lossy())
                    .replace("{out}", &out.to_string_lossy())
                    .replace("{files_csv}", &files.join(","))
                    .replace("{tests}", tests),
            ),
        }
    }
    res
}

/// What `cargo mutants` passes to `cargo test` to run only `acceptance`
/// (`cargo test --test accept` → `--test accept`); `None` when the
/// acceptance command is not a `cargo test`.
#[must_use]
pub fn cargo_test_args(acceptance: &[String]) -> Option<Vec<String>> {
    match acceptance {
        [cargo, test, rest @ ..] if cargo == "cargo" && test == "test" => Some(rest.to_vec()),
        _ => None,
    }
}

/// The argv that mutates the changed code and runs `acceptance` against
/// each mutant; `Err`: why it cannot be measured.
pub fn acceptance_invocation(
    config: &Config,
    acceptance: &[String],
    diff: &Path,
    out: &Path,
    files: &[String],
) -> Result<Vec<String>, String> {
    match &config.tool {
        Tool::CargoMutants => {
            let args = cargo_test_args(acceptance).ok_or_else(|| {
                format!(
                    "the acceptance command `{}` is not `cargo test …`, which cargo-mutants needs",
                    acceptance.join(" ")
                )
            })?;
            let mut argv = cargo_mutants_argv(diff, out);
            if !args.is_empty() {
                argv.push("--".to_owned());
                argv.extend(args);
            }
            Ok(argv)
        }
        Tool::Command { .. } => config
            .acceptance_argv
            .as_ref()
            .map(|a| {
                expand(
                    a,
                    diff,
                    out,
                    files,
                    &crate::approvals::present::shell_quote(acceptance),
                )
            })
            .ok_or_else(|| "[mutation] acceptance_argv is not set for the command tool".to_owned()),
    }
}

fn cargo_mutants_argv(diff: &Path, out: &Path) -> Vec<String> {
    vec![
        "cargo".to_owned(),
        "mutants".to_owned(),
        "--in-diff".to_owned(),
        diff.to_string_lossy().into_owned(),
        "--output".to_owned(),
        out.to_string_lossy().into_owned(),
        "--no-shuffle".to_owned(),
    ]
}

/// Runs `config`'s tool on `patch` in `project`; `None` when the patch
/// changes no source file. Scratch files go to `tmp`, outside the worktree.
pub fn run(
    config: &Config,
    project: &Path,
    patch: &str,
    tmp: &Path,
    runner: &dyn CommandRunner,
) -> Option<Value> {
    run_with(config, project, patch, tmp, runner, None)
}

/// [`run`] with the mutants judged by `acceptance` alone: the
/// result as [`run`]'s, or `{"error"}` when it cannot be measured.
pub fn run_against(
    config: &Config,
    project: &Path,
    patch: &str,
    tmp: &Path,
    runner: &dyn CommandRunner,
    acceptance: &[String],
) -> Option<Value> {
    run_with(config, project, patch, tmp, runner, Some(acceptance))
}

fn run_with(
    config: &Config,
    project: &Path,
    patch: &str,
    tmp: &Path,
    runner: &dyn CommandRunner,
    acceptance: Option<&[String]>,
) -> Option<Value> {
    let files = changed_sources(patch);
    if files.is_empty() {
        return None;
    }
    let dir: PathBuf = tmp.join(format!(
        "mutation-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let error = |e: String| Some(json!({ "error": e }));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return error(e.to_string());
    }
    let diff = dir.join("attempt.patch");
    if let Err(e) = std::fs::write(&diff, patch) {
        return error(e.to_string());
    }
    let out = dir.join("out");
    let argv = match (acceptance, &config.tool) {
        (Some(a), _) => match acceptance_invocation(config, a, &diff, &out, &files) {
            Ok(argv) => argv,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                return error(e);
            }
        },
        (None, Tool::CargoMutants) => cargo_mutants_argv(&diff, &out),
        (None, Tool::Command { argv, .. }) => expand(argv, &diff, &out, &files, ""),
    };
    let ran = runner.run(&Invocation {
        argv: argv.clone(),
        cwd: project.to_path_buf(),
        env: scrubbed_env(std::env::vars()),
        timeout: config.timeout,
        output_cap: OUTPUT_CAP,
    });
    if let Some(e) = ran.spawn_error {
        return error(e);
    }
    if ran.timed_out {
        return error(format!(
            "`{}` did not finish within {} s",
            argv.join(" "),
            config.timeout.as_secs()
        ));
    }
    let report_path = match &config.tool {
        Tool::CargoMutants => out.join("mutants.out/outcomes.json"),
        Tool::Command { report, .. } => {
            let r = report.replace("{out}", &out.to_string_lossy());
            let p = PathBuf::from(&r);
            if p.is_absolute() { p } else { project.join(r) }
        }
    };
    let parsed = std::fs::read_to_string(&report_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let _ = std::fs::remove_dir_all(&dir);
    let Some(report) = parsed else {
        let tail = String::from_utf8_lossy(&ran.stderr.kept);
        let tail: String = tail
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        return error(format!(
            "`{}` exited {:?} and wrote no report: {tail}",
            argv.join(" "),
            ran.exit_code
        ));
    };
    let mut result = match &config.tool {
        Tool::CargoMutants => from_cargo_mutants(&report),
        Tool::Command { argv, .. } => from_report_schema(&report, &argv[0]),
    };
    result["files"] = json!(files);
    Some(result)
}

/// The verdict reason for survivors (`mode = "require"`), if any; a tool
/// that could not run is a failure too (no evidence is not good evidence).
#[must_use]
pub fn failure(result: &Value) -> Option<String> {
    if let Some(e) = result["error"].as_str() {
        return Some(format!("mutation testing could not run: {e}"));
    }
    let missed = result["missed"].as_u64().unwrap_or(0);
    if missed == 0 {
        return None;
    }
    let total = missed + result["caught"].as_u64().unwrap_or(0);
    let names: Vec<&str> = result["survivors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(8)
        .collect();
    Some(format!(
        "mutation: {missed} of {total} mutants of the changed code survived the tests — strengthen the tests so they fail for: {}",
        names.join("; ")
    ))
}

/// The commit trailer value.
#[must_use]
pub fn trailer(result: &Value) -> Option<String> {
    result["caught"].as_u64().map(|c| {
        format!(
            "Mutation: caught={c} missed={} timeout={} unviable={}\n",
            result["missed"].as_u64().unwrap_or(0),
            result["timeout"].as_u64().unwrap_or(0),
            result["unviable"].as_u64().unwrap_or(0)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_reports_parse() {
        assert!(parse("").unwrap_or_default().is_none());
        let c = parse("[mutation]\ntool = \"cargo-mutants\"\nmode = \"require\"\n")
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("config"));
        assert_eq!((c.tool, c.mode), (Tool::CargoMutants, Mode::Require));
        assert!(parse("[mutation]\ntool = \"command\"\n").is_err());

        let cm = json!({ "caught": 6, "missed": 1, "timeout": 0, "unviable": 2, "outcomes": [
            { "summary": "Success", "scenario": "Baseline" },
            { "summary": "MissedMutant", "scenario": { "Mutant": { "name": "src/lib.rs:2:10: replace < with <= in clamp" } } } ] });
        let r = from_cargo_mutants(&cm);
        assert_eq!(
            r["survivors"],
            json!(["src/lib.rs:2:10: replace < with <= in clamp"])
        );
        assert!(failure(&r).unwrap_or_default().contains("1 of 7 mutants"));
        assert_eq!(
            trailer(&r).unwrap_or_default(),
            "Mutation: caught=6 missed=1 timeout=0 unviable=2\n"
        );

        let st = json!({ "files": { "src/a.ts": { "mutants": [
            { "status": "Killed" },
            { "status": "Survived", "mutatorName": "EqualityOperator", "replacement": "a <= b", "location": { "start": { "line": 3 } } } ] } } });
        let r = from_report_schema(&st, "npx");
        assert_eq!(
            (r["caught"].as_u64(), r["missed"].as_u64()),
            (Some(1), Some(1))
        );
        assert_eq!(r["survivors"][0], "src/a.ts:3: EqualityOperator → a <= b");
        assert_eq!(
            changed_sources("+++ b/src/lib.rs\n+++ b/tests/t.rs\n"),
            ["src/lib.rs"]
        );
        let p = "diff --git a/src/lib.rs b/src/lib.rs\n+++ b/src/lib.rs\n+x\ndiff --git a/tests/acceptance_t5.rs b/tests/acceptance_t5.rs\n+++ b/tests/acceptance_t5.rs\n+secret\n";
        assert_eq!(
            source_patch(p),
            "diff --git a/src/lib.rs b/src/lib.rs\n+++ b/src/lib.rs\n+x\n"
        );
    }

    #[test]
    fn mutants_run_against_the_acceptance_command_alone() {
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let c = parse("[mutation]\ntool = \"cargo-mutants\"\nsuite = false\n")
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("config"));
        assert!(!c.suite);
        let argv = acceptance_invocation(
            &c,
            &v(&["cargo", "test", "--test", "accept_t005"]),
            Path::new("/d"),
            Path::new("/o"),
            &[],
        )
        .unwrap_or_default();
        assert_eq!(argv[..2], v(&["cargo", "mutants"]));
        assert_eq!(argv[argv.len() - 3..], v(&["--", "--test", "accept_t005"]));
        assert!(
            acceptance_invocation(
                &c,
                &v(&["pytest", "t.py"]),
                Path::new("/d"),
                Path::new("/o"),
                &[]
            )
            .unwrap_err()
            .contains("not `cargo test")
        );
        let c = parse("[mutation]\ntool = \"command\"\nargv = [\"stryker\"]\nreport = \"r.json\"\nacceptance_argv = [\"stryker\", \"--mutate\", \"{files_csv}\", \"--command\", \"{tests}\"]\n")
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("config"));
        let argv = acceptance_invocation(
            &c,
            &v(&["npx", "vitest", "run", "tests/accept t5.test.ts"]),
            Path::new("/d"),
            Path::new("/o"),
            &v(&["src/a.ts", "src/b.ts"]),
        )
        .unwrap_or_default();
        assert_eq!(
            argv,
            v(&[
                "stryker",
                "--mutate",
                "src/a.ts,src/b.ts",
                "--command",
                "npx vitest run 'tests/accept t5.test.ts'"
            ])
        );
        assert!(
            parse("[mutation]\ntool = \"cargo-mutants\"\nacceptance_argv = [\"x\"]\n").is_err()
        );
    }
}
