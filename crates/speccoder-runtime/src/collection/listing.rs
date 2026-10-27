//! The runners' own listings: which tests they would run, asked only when
//! the static rule and the JUnit report leave a file undecided. Each runs
//! like a check — sandboxed, scrubbed environment, the check's deadline.

use std::path::Path;
use std::time::Duration;

use speccoder_core::collection::ListingRun;
use speccoder_core::verify::CheckStatus;

use crate::verify::process::{CommandRunner, Invocation, OUTPUT_CAP, scrubbed_env};

/// A listing's output, or why there is none.
pub struct Listed {
    /// What the runner printed (stdout).
    pub stdout: String,
    /// The run, for the evidence.
    pub run: ListingRun,
}

/// Runs `argv` in `cwd`; `Err` says why it gave no listing.
pub fn run(
    runner: &dyn CommandRunner,
    cwd: &Path,
    argv: &[String],
    timeout: Duration,
) -> (Result<Listed, String>, ListingRun) {
    let out = runner.run(&Invocation {
        argv: argv.to_vec(),
        cwd: cwd.to_path_buf(),
        env: scrubbed_env(std::env::vars()),
        timeout,
        output_cap: OUTPUT_CAP,
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
    let run = ListingRun {
        argv: argv.to_vec(),
        duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
        status,
    };
    let shown = argv.iter().take(3).cloned().collect::<Vec<_>>().join(" ");
    let result = if let Some(e) = &out.spawn_error {
        Err(format!("`{shown}` could not run: {e}"))
    } else if out.timed_out {
        Err(format!("`{shown}` timed out"))
    } else {
        // pytest exits 5 when it collects nothing: a listing, not a failure.
        let pytest_empty = out.exit_code == Some(5) && argv.iter().any(|a| a.contains("pytest"));
        if out.exit_code == Some(0) || pytest_empty {
            Ok(Listed {
                stdout: String::from_utf8_lossy(&out.stdout.kept).into_owned(),
                run: run.clone(),
            })
        } else {
            Err(format!(
                "`{shown}` failed: exit {}",
                out.exit_code
                    .map_or_else(|| "signal".to_owned(), |c| c.to_string())
            ))
        }
    };
    (result, run)
}

/// `cargo nextest list --message-format json`: `binary-id::test` for every
/// test it would run (ignored ones left out).
#[must_use]
pub fn nextest_ids(stdout: &str) -> Option<Vec<String>> {
    let start = stdout.find('{')?;
    let v: serde_json::Value = serde_json::from_str(stdout[start..].trim()).ok()?;
    let mut out = Vec::new();
    for (binary, suite) in v["rust-suites"].as_object()? {
        for (name, case) in suite["testcases"].as_object().into_iter().flatten() {
            if case["ignored"] != true && case["filter-match"]["status"] != "mismatch" {
                out.push(format!("{binary}::{name}"));
            }
        }
    }
    Some(out)
}

/// `cargo test … -- --list --format terse`: `name: test` lines.
#[must_use]
pub fn libtest_names(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|l| l.strip_suffix(": test"))
        .map(str::to_owned)
        .collect()
}

/// `pytest --collect-only -q`: node ids (`path::name`).
#[must_use]
pub fn pytest_ids(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("::"))
        .map(str::to_owned)
        .collect()
}

/// `vitest list --json`: the files of the listed tests, relative to `root`.
#[must_use]
pub fn vitest_files(stdout: &str, root: &Path) -> Option<Vec<String>> {
    let start = stdout.find('[')?;
    let v: serde_json::Value = serde_json::from_str(stdout[start..].trim()).ok()?;
    Some(
        v.as_array()?
            .iter()
            .filter_map(|t| t["file"].as_str())
            .map(|f| relative(f, root))
            .collect(),
    )
}

/// `jest --listTests`: absolute file paths, one per line.
#[must_use]
pub fn jest_files(stdout: &str, root: &Path) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|f| relative(f, root))
        .collect()
}

/// `go test -list .`: test names (the `ok` summary lines left out).
#[must_use]
pub fn go_names(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty() && !l.starts_with("ok ") && !l.starts_with("?") && !l.contains('\t')
        })
        .map(str::to_owned)
        .collect()
}

/// `go test -list`: `importpath::Name` per test, the names grouped by the
/// `ok <importpath>` line that follows them.
#[must_use]
pub fn go_ids(stdout: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for l in stdout.lines().map(str::trim) {
        if let Some(rest) = l.strip_prefix("ok") {
            let pkg = rest.split_whitespace().next().unwrap_or_default();
            out.extend(names.drain(..).map(|n| format!("{pkg}::{n}")));
        } else if !l.is_empty() && !l.starts_with('?') && !l.contains('\t') && !l.contains(' ') {
            names.push(l);
        }
    }
    out
}

/// A configured `list` command: one test id or file path per line.
#[must_use]
pub fn lines(stdout: &str, root: &Path) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| relative(l, root))
        .collect()
}

fn relative(p: &str, root: &Path) -> String {
    let root = root.to_string_lossy();
    let canon = std::fs::canonicalize(root.as_ref())
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_default();
    for r in [root.as_ref(), canon.as_str()] {
        if !r.is_empty()
            && let Some(rest) = p.strip_prefix(r)
        {
            return rest.trim_start_matches('/').to_owned();
        }
    }
    p.trim_start_matches("./").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_parse() {
        let nx = r#"{"rust-build-meta":{},"rust-suites":{"greet::greet_name":{"testcases":{"a":{"ignored":false,"filter-match":{"status":"matches"}},"b":{"ignored":true,"filter-match":{"status":"matches"}}}}}}"#;
        assert_eq!(nextest_ids(nx).unwrap(), ["greet::greet_name::a"]);
        assert_eq!(
            libtest_names("tests::a: test\nbench: benchmark\n\n1 test, 0 benchmarks\n"),
            ["tests::a"]
        );
        assert_eq!(
            pytest_ids("tests/test_x.py::test_a\n\n1 test collected in 0.01s\n"),
            ["tests/test_x.py::test_a"]
        );
        let root = Path::new("/w");
        assert_eq!(
            vitest_files(r#"[{"name":"g","file":"/w/src/a.test.ts"}]"#, root).unwrap(),
            ["src/a.test.ts"]
        );
        assert_eq!(jest_files("/w/src/a.test.js\n", root), ["src/a.test.js"]);
        assert_eq!(
            go_names("TestA\nExampleB\nok  \tex.com/m/g\t0.01s\n"),
            ["TestA", "ExampleB"]
        );
        assert_eq!(
            go_ids("TestA\nok  \tex.com/m/g\t0.01s\nTestB\nok  \tex.com/m/h\t0.01s\n"),
            ["ex.com/m/g::TestA", "ex.com/m/h::TestB"]
        );
    }
}
