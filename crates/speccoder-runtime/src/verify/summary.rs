//! Test runners' own summaries → counts, for runs without a JUnit report
//! (D-064). Each runner's summary lines are stable, documented output:
//! libtest and nextest, pytest, jest and vitest, `go test`.
//!
//! `unresolved_only` is set when the code under test did not build or
//! import, and every diagnostic names something that does not exist yet —
//! the start of a task that introduces it ("absent" red).

use std::sync::LazyLock;

use regex::Regex;
use speccoder_core::verify::RunnerSummary;

/// rustc's unresolved-name codes: value, import, path, type, method.
const RUST_UNRESOLVED: &[&str] = &[
    "E0412", "E0422", "E0423", "E0425", "E0432", "E0433", "E0599",
];
/// TypeScript's: module has no export, module not found, no property, no name.
const TS_UNRESOLVED: &[&str] = &["TS2304", "TS2305", "TS2307", "TS2339", "TS2552", "TS2724"];

/// The patterns are constants below; a bad one matches nothing (and fails
/// the tests).
fn re(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

static LIBTEST: LazyLock<Option<Regex>> =
    LazyLock::new(|| re(r"test result: \w+\. (\d+) passed; (\d+) failed"));
static NEXTEST: LazyLock<Option<Regex>> =
    LazyLock::new(|| re(r"\d+ tests? run: (\d+) passed(?:[^,\n]*)?(?:, (\d+) failed)?"));
static RUSTC_CODE: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^error\[(E\d{4})\]"));
static RUSTC_PLAIN: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^error: (.*)$"));
static PYTEST: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^=+ (.*) in [\d.]+s.* =+$"));
static PY_COUNT: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(\d+) (passed|failed|errors?)"));
static PY_E_LINE: LazyLock<Option<Regex>> =
    LazyLock::new(|| re(r"(?m)^E\s+(\w+(?:Error|Exception)):"));
static JEST: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^Tests:\s+(.*)$"));
static VITEST: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^\s*Tests\s+(.*\(\d+\))\s*$"));
static JS_COUNT: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(\d+) (passed|failed)"));
static SUITE_FAILED: LazyLock<Option<Regex>> =
    LazyLock::new(|| re(r"(?m)^\s*Test (?:Suites|Files):?\s+(\d+) failed"));
static TS_CODE: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"\berror (TS\d{4})\b"));
static GO_FAIL: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^\s*--- FAIL: "));
static GO_PASS: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^\s*--- PASS: "));
static GO_COMPILE: LazyLock<Option<Regex>> = LazyLock::new(|| re(r"(?m)^\S+\.go:\d+:\d+: (.*)$"));

fn caps<'t>(r: &'t Option<Regex>, text: &'t str) -> impl Iterator<Item = regex::Captures<'t>> {
    r.iter().flat_map(move |r| r.captures_iter(text))
}

fn finds<'t>(r: &'t Option<Regex>, text: &'t str) -> impl Iterator<Item = regex::Match<'t>> {
    r.iter().flat_map(move |r| r.find_iter(text))
}

fn matches(r: &Option<Regex>, text: &str) -> bool {
    r.as_ref().is_some_and(|r| r.is_match(text))
}

/// Reads the runner's summary from a run's output; `None` when no known
/// summary is present.
#[must_use]
pub fn parse(stdout: &str, stderr: &str, exit_code: Option<i32>) -> Option<RunnerSummary> {
    let text = format!("{stdout}\n{stderr}");
    rust(&text)
        .or_else(|| python(&text, exit_code))
        .or_else(|| javascript(&text))
        .or_else(|| go(&text))
}

fn rust(text: &str) -> Option<RunnerSummary> {
    let mut s = RunnerSummary::default();
    let mut seen = false;
    for c in caps(&LIBTEST, text) {
        seen = true;
        s.passed += num(c.get(1));
        s.failed += num(c.get(2));
    }
    for c in caps(&NEXTEST, text) {
        seen = true;
        s.passed += num(c.get(1));
        s.failed += num(c.get(2));
    }
    if text.contains("error: could not compile") {
        let codes: Vec<&str> = caps(&RUSTC_CODE, text)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        // Plain `error:` lines other than cargo's own wrap-up are other build errors.
        let other_plain = caps(&RUSTC_PLAIN, text).any(|c| {
            let m = c.get(1).map_or("", |m| m.as_str());
            !(m.starts_with("could not compile")
                || m.starts_with("aborting due to")
                || m.starts_with("Recipe")
                || m.starts_with("test failed"))
        });
        s.build_failed = true;
        s.unresolved_only =
            !codes.is_empty() && !other_plain && codes.iter().all(|c| RUST_UNRESOLVED.contains(c));
        return Some(s);
    }
    seen.then_some(s)
}

fn python(text: &str, exit_code: Option<i32>) -> Option<RunnerSummary> {
    let line = caps(&PYTEST, text).last()?;
    let mut s = RunnerSummary::default();
    for c in caps(&PY_COUNT, line.get(1).map_or("", |m| m.as_str())) {
        match c.get(2).map_or("", |m| m.as_str()) {
            "passed" => s.passed += num(c.get(1)),
            "failed" => s.failed += num(c.get(1)),
            _ => s.errors += num(c.get(1)),
        }
    }
    // pytest exits 2 on collection errors, 5 when nothing was collected.
    let collection = text.contains("ERROR collecting") || exit_code == Some(2);
    if collection && s.failed == 0 {
        let kinds: Vec<&str> = caps(&PY_E_LINE, text)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        let unresolved = |k: &&str| {
            matches!(
                *k,
                "ImportError" | "ModuleNotFoundError" | "AttributeError" | "NameError"
            )
        };
        if !kinds.is_empty() && kinds.iter().all(unresolved) {
            s.build_failed = true;
            s.unresolved_only = true;
        } else if s.errors == 0 {
            s.errors = 1;
        }
    }
    Some(s)
}

fn javascript(text: &str) -> Option<RunnerSummary> {
    let ts: Vec<&str> = caps(&TS_CODE, text)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    let line = caps(&JEST, text)
        .last()
        .or_else(|| caps(&VITEST, text).last());
    if line.is_none() && ts.is_empty() {
        return None;
    }
    let mut s = RunnerSummary::default();
    if let Some(line) = line {
        for c in caps(&JS_COUNT, line.get(1).map_or("", |m| m.as_str())) {
            match c.get(2).map_or("", |m| m.as_str()) {
                "passed" => s.passed += num(c.get(1)),
                _ => s.failed += num(c.get(1)),
            }
        }
    }
    if !ts.is_empty() {
        s.build_failed = true;
        s.unresolved_only = ts.iter().all(|c| TS_UNRESOLVED.contains(c));
    } else if s.passed + s.failed == 0 && matches(&SUITE_FAILED, text) {
        // A suite failed before any test ran: it did not load.
        s.errors = 1;
    }
    Some(s)
}

fn go(text: &str) -> Option<RunnerSummary> {
    let build = text.contains("[build failed]") || text.contains("[setup failed]");
    let failed = finds(&GO_FAIL, text).count();
    let passed = finds(&GO_PASS, text).count();
    if !build && failed + passed == 0 {
        return None;
    }
    let mut s = RunnerSummary {
        passed,
        failed,
        ..RunnerSummary::default()
    };
    if build {
        let diags: Vec<&str> = caps(&GO_COMPILE, text)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        s.build_failed = true;
        s.unresolved_only = !diags.is_empty()
            && diags
                .iter()
                .all(|d| d.starts_with("undefined: ") || d.contains("has no field or method"));
    }
    Some(s)
}

fn num(m: Option<regex::Match<'_>>) -> usize {
    m.and_then(|m| m.as_str().parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(stdout: &str, code: i32) -> RunnerSummary {
        parse(stdout, "", Some(code)).unwrap()
    }

    #[test]
    fn libtest_assertion_failure() {
        let out = "running 2 tests\ntest a ... ok\ntest b ... FAILED\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let r = s(out, 101);
        assert_eq!((r.passed, r.failed, r.build_failed), (1, 1, false));
    }

    #[test]
    fn rustc_missing_function_is_unresolved_only() {
        let err = "error[E0425]: cannot find function `slugify` in this scope\n --> tests/acceptance_t004.rs:3:5\n\nerror[E0432]: unresolved import `crate::slug`\n\nerror: could not compile `demo` (test \"acceptance_t004\") due to 2 previous errors\n";
        let r = parse("", err, Some(101)).unwrap();
        assert!(r.build_failed && r.unresolved_only);
    }

    #[test]
    fn rustc_syntax_error_is_a_build_error() {
        let err = "error: expected one of `,`, `:`, or `}`, found `=`\n --> tests/a.rs:1:9\n\nerror: could not compile `demo` due to 1 previous error\n";
        let r = parse("", err, Some(101)).unwrap();
        assert!(r.build_failed && !r.unresolved_only);
        let typed = "error[E0308]: mismatched types\nerror: could not compile `demo`\n";
        assert!(!parse("", typed, Some(101)).unwrap().unresolved_only);
    }

    #[test]
    fn nextest_summary() {
        let out = "     Summary [   0.031s] 3 tests run: 1 passed, 2 failed, 0 skipped\n";
        let r = s(out, 100);
        assert_eq!((r.passed, r.failed), (1, 2));
    }

    #[test]
    fn pytest_outcomes() {
        let r = s("============ 1 failed, 2 passed in 0.12s =============", 1);
        assert_eq!((r.passed, r.failed), (2, 1));
        let import = "==== ERRORS ====\n___ ERROR collecting tests/test_slug.py ___\nE   ImportError: cannot import name 'slugify' from 'slug'\n=========== 1 error in 0.05s ===========";
        let r = s(import, 2);
        assert!(r.build_failed && r.unresolved_only);
        let syntax = "___ ERROR collecting tests/test_slug.py ___\nE   SyntaxError: invalid syntax\n=========== 1 error in 0.05s ===========";
        let r = s(syntax, 2);
        assert!(!r.build_failed && r.errors == 1);
        let none = s("=========== no tests ran in 0.01s ===========", 5);
        assert_eq!((none.passed, none.failed, none.errors), (0, 0, 0));
    }

    #[test]
    fn jest_vitest_and_typescript() {
        let r = s(
            "Test Suites: 1 failed, 1 total\nTests:       1 failed, 3 passed, 4 total\n",
            1,
        );
        assert_eq!((r.passed, r.failed), (3, 1));
        let r = s(
            " Test Files  1 failed (1)\n      Tests  2 failed | 5 passed (7)\n",
            1,
        );
        assert_eq!((r.passed, r.failed), (5, 2));
        let ts = "src/a.test.ts:1:10 - error TS2305: Module '\"./slug\"' has no exported member 'slugify'.\nTest Suites: 1 failed, 1 total\nTests:       0 total\n";
        let r = s(ts, 1);
        assert!(r.build_failed && r.unresolved_only);
        let other =
            "a.test.ts:2:3 - error TS2322: Type 'string' is not assignable to type 'number'.\n";
        assert!(!s(other, 1).unresolved_only);
        let suite = "Test Suites: 1 failed, 1 total\nTests:       0 total\n";
        assert_eq!(s(suite, 1).errors, 1);
    }

    #[test]
    fn go_test_outcomes() {
        let r = s(
            "--- FAIL: TestSlug (0.00s)\n--- PASS: TestOther (0.00s)\nFAIL\n",
            1,
        );
        assert_eq!((r.passed, r.failed), (1, 1));
        let undefined = "# demo [demo.test]\n./slug_test.go:6:9: undefined: Slugify\nFAIL\tdemo [build failed]\n";
        let r = s(undefined, 1);
        assert!(r.build_failed && r.unresolved_only);
        let other =
            "./slug_test.go:6:9: syntax error: unexpected newline\nFAIL\tdemo [build failed]\n";
        assert!(!s(other, 1).unresolved_only);
    }

    #[test]
    fn unknown_output_has_no_summary() {
        assert!(parse("something happened", "", Some(1)).is_none());
    }
}
