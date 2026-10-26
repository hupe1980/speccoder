//! The judge's vocabulary: check results, baseline comparison, and verdicts
//!.
//!
//! Pure: running checks is the runtime's job; deciding what their results
//! mean happens here, where it is property-tested.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Exited 0.
    Passed,
    /// Exited non-zero.
    Failed,
    /// Killed at its deadline.
    TimedOut,
    /// Not applicable (its `when_exists` path is absent); not evidence of anything.
    Skipped,
    /// Could not be run at all (e.g. command not found).
    Error,
}

impl CheckStatus {
    /// Whether the check counts against verification.
    #[must_use]
    pub const fn is_failure(self) -> bool {
        matches!(self, Self::Failed | Self::TimedOut | Self::Error)
    }
}

/// Outcome of one test from a JUnit report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestOutcome {
    /// Passed.
    Passed,
    /// Failed (an assertion, a `<failure>` in JUnit).
    Failed,
    /// Skipped or ignored.
    Skipped,
    /// Errored before or outside an assertion (a `<error>` in JUnit: setup,
    /// collection, an unexpected exception).
    Error,
}

impl TestOutcome {
    /// Whether the test counts as failing (for regressions and digests).
    #[must_use]
    pub const fn is_failing(self) -> bool {
        matches!(self, Self::Failed | Self::Error)
    }
}

/// One test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TestResult {
    /// `classname::name`, or `name` when there is no class.
    pub id: String,
    /// What happened.
    pub outcome: TestOutcome,
}

/// One check run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckResult {
    /// Name from `speccoder.toml`.
    pub name: String,
    /// The command, as run.
    pub argv: Vec<String>,
    /// What happened.
    pub status: CheckStatus,
    /// Process exit code, if it exited.
    pub exit_code: Option<i32>,
    /// Wall-clock duration.
    pub duration_ms: u64,
    /// Whether the check declares a JUnit report.
    pub junit: bool,
    /// Tests from the JUnit report, if one was declared and freshly written.
    pub tests: Option<Vec<TestResult>>,
    /// Last part of stdout (lossy UTF-8).
    pub stdout_tail: String,
    /// Last part of stderr (lossy UTF-8).
    pub stderr_tail: String,
    /// Output exceeded the capture cap.
    pub truncated: bool,
    /// Why the check was skipped, errored, or has no tests.
    pub note: Option<String>,
    /// It failed once and passed when run again: named, never trusted as
    /// task evidence.
    #[serde(default)]
    pub flaky: bool,
}

impl CheckResult {
    /// Counts of (passed, failed, skipped) tests.
    #[must_use]
    pub fn test_counts(&self) -> Option<(usize, usize, usize)> {
        self.tests.as_ref().map(|tests| {
            let n = |o| tests.iter().filter(|t| t.outcome == o).count();
            (
                n(TestOutcome::Passed),
                n(TestOutcome::Failed) + n(TestOutcome::Error),
                n(TestOutcome::Skipped),
            )
        })
    }

    /// Whether this check, as a task-specific check, is evidence for the
    /// task: it passed, and either declares no report (a command check) or
    /// its report has at least one passing test.
    #[must_use]
    pub fn is_evidence(&self) -> bool {
        !self.flaky
            && self.status == CheckStatus::Passed
            && (!self.junit
                || self
                    .tests
                    .as_ref()
                    .is_some_and(|t| t.iter().any(|t| t.outcome == TestOutcome::Passed)))
    }

    fn failing_tests(&self) -> BTreeSet<&str> {
        self.tests
            .iter()
            .flatten()
            .filter(|t| t.outcome.is_failing())
            .map(|t| t.id.as_str())
            .collect()
    }
}

/// Everything one `verify` run established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VerifyReport {
    /// Project root the checks ran in.
    pub root: String,
    /// `fast` or `full`.
    pub tier: String,
    /// One result per configured check, in configuration order.
    pub checks: Vec<CheckResult>,
    /// Comparison with a baseline, if one was given.
    pub comparison: Option<Comparison>,
    /// Whether verification passed (see [`passes`]).
    pub passed: bool,
}

/// What changed relative to a baseline (pass-to-pass).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Comparison {
    /// Tests that passed in the baseline and now fail or are missing.
    pub regressions: Vec<Regression>,
    /// Tests failing in both runs.
    pub pre_existing: Vec<String>,
    /// Tests not in the baseline.
    pub new_tests: Vec<String>,
    /// Tests collected in the baseline.
    pub count_before: usize,
    /// Tests collected now.
    pub count_after: usize,
    /// Checks failing now only because of tests that already failed in the baseline.
    pub failing_as_before: Vec<String>,
}

/// A test that passed in the baseline and no longer does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Regression {
    /// `check/test`.
    pub id: String,
    /// `failed`, `skipped`, or `missing`.
    pub now: String,
}

/// Compares `current` with `baseline`, test by test and check by check.
#[must_use]
pub fn compare(baseline: &VerifyReport, current: &VerifyReport) -> Comparison {
    let index = |r: &VerifyReport| -> BTreeMap<String, TestOutcome> {
        r.checks
            .iter()
            .flat_map(|c| {
                c.tests
                    .iter()
                    .flatten()
                    .map(move |t| (format!("{}/{}", c.name, t.id), t.outcome))
            })
            .collect()
    };
    let before = index(baseline);
    let after = index(current);

    let mut cmp = Comparison {
        count_before: before.len(),
        count_after: after.len(),
        ..Comparison::default()
    };
    for (id, was) in &before {
        match (was, after.get(id)) {
            (TestOutcome::Passed, Some(TestOutcome::Passed)) => {}
            (TestOutcome::Passed, Some(now)) => cmp.regressions.push(Regression {
                id: id.clone(),
                now: match now {
                    TestOutcome::Failed => "failed",
                    TestOutcome::Error => "error",
                    _ => "skipped",
                }
                .to_owned(),
            }),
            (TestOutcome::Passed, None) => cmp.regressions.push(Regression {
                id: id.clone(),
                now: "missing".to_owned(),
            }),
            (was, Some(now)) if was.is_failing() && now.is_failing() => {
                cmp.pre_existing.push(id.clone());
            }
            _ => {}
        }
    }
    cmp.new_tests = after
        .keys()
        .filter(|id| !before.contains_key(*id))
        .cloned()
        .collect();

    // A check that failed before and now fails only on tests that failed
    // before is not blamed on the change. Without a report this can't be
    // known, so such checks stay failures.
    for c in current
        .checks
        .iter()
        .filter(|c| c.status == CheckStatus::Failed)
    {
        let Some(old) = baseline.checks.iter().find(|b| b.name == c.name) else {
            continue;
        };
        let now = c.failing_tests();
        if old.status == CheckStatus::Failed
            && c.tests.is_some()
            && !now.is_empty()
            && now.is_subset(&old.failing_tests())
        {
            cmp.failing_as_before.push(c.name.clone());
        }
    }
    cmp
}

/// Checks that count against verification: failures not explained by the baseline.
fn failures<'r>(checks: &'r [CheckResult], cmp: Option<&Comparison>) -> Vec<&'r CheckResult> {
    checks
        .iter()
        .filter(|c| c.status.is_failure())
        .filter(|c| !cmp.is_some_and(|cmp| cmp.failing_as_before.contains(&c.name)))
        .collect()
}

/// Whether a run passes: no unexplained failing check and no regression.
#[must_use]
pub fn passes(checks: &[CheckResult], cmp: Option<&Comparison>) -> bool {
    failures(checks, cmp).is_empty() && cmp.is_none_or(|c| c.regressions.is_empty())
}

/// What a red run of acceptance tests showed.
///
/// Only [`RedClass::Assertion`] and [`RedClass::Absent`] are red: a test was
/// collected and failed on an assertion, or the tests could not build only
/// because symbols the task is to introduce do not exist yet. Everything
/// else is not demonstrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedClass {
    /// A collected test failed on an assertion.
    Assertion,
    /// The only build or import errors name symbols that do not exist yet.
    Absent,
    /// Any other build error.
    BuildError,
    /// Tests errored at setup or collection.
    CollectionError,
    /// The run hit its deadline.
    Timeout,
    /// Nothing was collected.
    NoTests,
    /// Red once, not red when confirmed.
    Flaky,
    /// The output was not recognised.
    Unclassified,
    /// The tests passed.
    Passed,
}

impl RedClass {
    /// Whether this is red: evidence the tests fail without the task's work.
    #[must_use]
    pub const fn is_red(self) -> bool {
        matches!(self, Self::Assertion | Self::Absent)
    }
}

/// Counts read from a test runner's own summary output (libtest, pytest,
/// jest/vitest, go test), when it writes no JUnit report.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct RunnerSummary {
    /// Tests that passed.
    pub passed: usize,
    /// Tests that failed on an assertion.
    pub failed: usize,
    /// Tests that errored at setup or collection.
    pub errors: usize,
    /// The test code did not build or import.
    pub build_failed: bool,
    /// Every build or import diagnostic is an unresolved name.
    pub unresolved_only: bool,
}

/// How many tests a run collected — ran, passed or failed (skipped and
/// errored at setup excluded): from its JUnit report, else the runner's
/// summary; `None` when neither says.
#[must_use]
pub fn collected_count(result: &CheckResult, summary: Option<&RunnerSummary>) -> Option<usize> {
    if let Some(tests) = result.tests.as_ref() {
        return Some(
            tests
                .iter()
                .filter(|t| matches!(t.outcome, TestOutcome::Passed | TestOutcome::Failed))
                .count(),
        );
    }
    summary
        .filter(|s| !s.build_failed)
        .map(|s| s.passed + s.failed)
}

/// Classifies a red run: a JUnit report first, then the runner's summary,
/// then the check's status. Nothing recognised is [`RedClass::Unclassified`].
#[must_use]
pub fn classify_red(result: &CheckResult, summary: Option<&RunnerSummary>) -> RedClass {
    if result.status == CheckStatus::TimedOut {
        return RedClass::Timeout;
    }
    if let Some(tests) = result.tests.as_ref() {
        if tests.iter().any(|t| t.outcome == TestOutcome::Failed) {
            return RedClass::Assertion;
        }
        if tests.iter().any(|t| t.outcome == TestOutcome::Error) {
            return RedClass::CollectionError;
        }
        if tests.is_empty() {
            return RedClass::NoTests;
        }
        if result.status == CheckStatus::Passed {
            return RedClass::Passed;
        }
    }
    if let Some(s) = summary {
        if s.build_failed {
            return if s.unresolved_only {
                RedClass::Absent
            } else {
                RedClass::BuildError
            };
        }
        if s.failed > 0 {
            return RedClass::Assertion;
        }
        if s.errors > 0 {
            return RedClass::CollectionError;
        }
        if s.passed == 0 {
            return RedClass::NoTests;
        }
    }
    match result.status {
        CheckStatus::Passed => RedClass::Passed,
        CheckStatus::Error => RedClass::BuildError,
        _ => RedClass::Unclassified,
    }
}

/// Classifies a red run against the same command run without the file
/// under test: red by an assertion only where the file adds a failing test —
/// one that did not fail without it (by id when both runs name their tests,
/// else by more failures). A failure the command shows anyway is another
/// test's, never this file's red: it is [`RedClass::Passed`].
#[must_use]
pub fn classify_red_added(
    with: (&CheckResult, Option<&RunnerSummary>),
    without: (&CheckResult, Option<&RunnerSummary>),
) -> RedClass {
    let class = classify_red(with.0, with.1);
    if class != RedClass::Assertion {
        return class;
    }
    let failing = |r: &CheckResult| {
        r.tests.as_ref().map(|t| {
            t.iter()
                .filter(|t| t.outcome == TestOutcome::Failed)
                .map(|t| t.id.clone())
                .collect::<std::collections::BTreeSet<_>>()
        })
    };
    let added = match (failing(with.0), failing(without.0)) {
        (Some(w), Some(o)) => w.difference(&o).next().is_some(),
        _ => match (with.1, without.1) {
            (Some(w), Some(o)) => w.failed > o.failed,
            // Without the file the command reports no count — a real runner
            // refuses a missing test file: no assertion failed there, so the
            // failure with it is the file's own.
            (Some(_), None) => classify_red(without.0, None) != RedClass::Assertion,
            _ => false,
        },
    };
    if added {
        RedClass::Assertion
    } else if with.1.is_none() && with.0.tests.is_none() {
        RedClass::Unclassified
    } else {
        RedClass::Passed
    }
}

/// The class of each test in `wanted` from a run's JUnit report: a test
/// matches by its id or its last `::` segments; one the report does not
/// name was not collected. `None` when the run has no per-test report —
/// then each test is run on its own, by name.
#[must_use]
pub fn classify_red_each(
    result: &CheckResult,
    wanted: &[String],
) -> Option<Vec<(String, RedClass)>> {
    if result.status == CheckStatus::TimedOut {
        return Some(
            wanted
                .iter()
                .map(|w| (w.clone(), RedClass::Timeout))
                .collect(),
        );
    }
    let tests = result.tests.as_ref()?;
    Some(
        wanted
            .iter()
            .map(|w| {
                let found = tests.iter().find(|t| {
                    t.id == *w
                        || t.id.ends_with(&format!("::{w}"))
                        || t.id.ends_with(&format!(".{w}"))
                });
                let class = match found.map(|t| t.outcome) {
                    Some(TestOutcome::Failed) => RedClass::Assertion,
                    Some(TestOutcome::Error) => RedClass::CollectionError,
                    Some(TestOutcome::Passed) => RedClass::Passed,
                    Some(TestOutcome::Skipped) | None => RedClass::NoTests,
                };
                (w.clone(), class)
            })
            .collect(),
    )
}

/// The class of the one test `test` from a run of the command that is to
/// run only it: by name from a per-test report; without one, from the
/// runner's summary only when it ran exactly one test — a run of several
/// tests cannot say which of them failed, so it is
/// [`RedClass::Unclassified`], never an assertion of `test`'s.
#[must_use]
pub fn classify_red_one(
    result: &CheckResult,
    summary: Option<&RunnerSummary>,
    test: &str,
) -> RedClass {
    if test.is_empty() {
        // No name: a per-test report must hold exactly one test.
        if result.tests.as_ref().is_some_and(|t| t.len() > 1) {
            return RedClass::Unclassified;
        }
    } else if let Some(each) = classify_red_each(result, &[test.to_owned()]) {
        return each.first().map_or(RedClass::NoTests, |(_, c)| *c);
    }
    let class = classify_red(result, summary);
    match summary {
        Some(s) if !s.build_failed && s.passed + s.failed + s.errors > 1 => RedClass::Unclassified,
        _ => class,
    }
}

/// How strong the evidence for a task is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "verdict")]
pub enum Verdict {
    /// A task-specific check produced evidence, nothing failed, nothing regressed.
    Verified,
    /// Nothing failed or regressed, but nothing task-specific was established.
    RegressionOnly,
    /// Something failed or regressed.
    Failed {
        /// One line per reason.
        reasons: Vec<String>,
    },
}

/// The verdict for a task whose task-specific checks are named in `task_checks`.
#[must_use]
pub fn verdict(task_checks: &[&str], checks: &[CheckResult], cmp: Option<&Comparison>) -> Verdict {
    let mut reasons: Vec<String> = failures(checks, cmp)
        .iter()
        .map(|c| format!("check `{}` {:?}", c.name, c.status).to_lowercase())
        .collect();
    if let Some(cmp) = cmp {
        reasons.extend(
            cmp.regressions
                .iter()
                .map(|r| format!("regression: {} is {}", r.id, r.now)),
        );
    }
    if !reasons.is_empty() {
        return Verdict::Failed { reasons };
    }
    let evidence = checks
        .iter()
        .any(|c| task_checks.contains(&c.name.as_str()) && c.is_evidence());
    if evidence {
        Verdict::Verified
    } else {
        Verdict::RegressionOnly
    }
}

/// What the held-out acceptance tests said about an attempt that passed
/// visibly: the judge's own tests, never shown to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum HeldOut {
    /// `[acceptance] held_out = false`.
    Off,
    /// No held-out test was kept (none generated, all discarded, …).
    Absent {
        /// Why.
        reason: String,
    },
    /// Every kept held-out test passed.
    Passed {
        /// How many files.
        files: usize,
    },
    /// These scenarios' held-out tests failed.
    Failed {
        /// Scenario IDs, sorted.
        scenarios: Vec<String>,
    },
}

/// The verdict after the held-out tests: a failure fails it (naming the
/// scenarios only); anything else leaves it as it was.
#[must_use]
pub fn with_held_out(verdict: Verdict, held_out: &HeldOut) -> Verdict {
    let HeldOut::Failed { scenarios } = held_out else {
        return verdict;
    };
    let mut ids = scenarios.clone();
    ids.sort();
    ids.dedup();
    let reason = format!("held-out acceptance failed: {}", ids.join(", "));
    match verdict {
        Verdict::Failed { mut reasons } => {
            reasons.push(reason);
            Verdict::Failed { reasons }
        }
        Verdict::Verified | Verdict::RegressionOnly => Verdict::Failed {
            reasons: vec![reason],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_test_gets_its_own_red_class() {
        let mut r = check("acceptance", CheckStatus::Failed, Some(&[]));
        r.tests = Some(vec![
            TestResult {
                id: "title::empty_title_is_refused".to_owned(),
                outcome: TestOutcome::Failed,
            },
            TestResult {
                id: "tests.test_title.test_long".to_owned(),
                outcome: TestOutcome::Passed,
            },
            TestResult {
                id: "setup_broken".to_owned(),
                outcome: TestOutcome::Error,
            },
        ]);
        let wanted: Vec<String> = [
            "empty_title_is_refused",
            "test_long",
            "setup_broken",
            "gone",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let classes: Vec<RedClass> = classify_red_each(&r, &wanted)
            .unwrap_or_default()
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        assert_eq!(
            classes,
            [
                RedClass::Assertion,
                RedClass::Passed,
                RedClass::CollectionError,
                RedClass::NoTests
            ]
        );
        r.tests = None;
        assert!(classify_red_each(&r, &wanted).is_none(), "by name instead");
    }

    #[test]
    fn one_test_is_red_only_by_its_own_assertion() {
        let failed = check("acceptance", CheckStatus::Failed, None);
        let one = RunnerSummary {
            failed: 1,
            ..RunnerSummary::default()
        };
        assert_eq!(
            classify_red_one(&failed, Some(&one), "t"),
            RedClass::Assertion
        );
        // An unrelated test failing next to it says nothing about `t`.
        let many = RunnerSummary {
            passed: 3,
            failed: 1,
            ..RunnerSummary::default()
        };
        assert_eq!(
            classify_red_one(&failed, Some(&many), "t"),
            RedClass::Unclassified
        );
        // By name from a report: the unrelated failure is not `t`'s.
        let mut r = check("acceptance", CheckStatus::Failed, Some(&[]));
        r.tests = Some(vec![
            TestResult {
                id: "other::broken".to_owned(),
                outcome: TestOutcome::Failed,
            },
            TestResult {
                id: "repro::t".to_owned(),
                outcome: TestOutcome::Passed,
            },
        ]);
        assert_eq!(classify_red_one(&r, None, "t"), RedClass::Passed);
        assert_eq!(classify_red_one(&r, None, "broken"), RedClass::Assertion);
        assert_eq!(classify_red_one(&r, None, "gone"), RedClass::NoTests);
        // A reused set names files, not tests: the one-test rule alone.
        assert_eq!(classify_red_one(&r, None, ""), RedClass::Unclassified);
        assert_eq!(
            classify_red_one(&failed, Some(&many), ""),
            RedClass::Unclassified
        );
    }

    #[test]
    fn a_file_is_red_only_by_the_failures_it_adds() {
        use TestOutcome::{Failed, Passed};
        let without = check(
            "held-out",
            CheckStatus::Failed,
            Some(&[("old::broken", Failed)]),
        );
        // The command fails anyway: the file's own test passes — not red.
        let borrowed = check(
            "held-out",
            CheckStatus::Failed,
            Some(&[("old::broken", Failed), ("new::variant", Passed)]),
        );
        assert_eq!(
            classify_red_added((&borrowed, None), (&without, None)),
            RedClass::Passed
        );
        // The file's own test fails: red.
        let own = check(
            "held-out",
            CheckStatus::Failed,
            Some(&[("old::broken", Failed), ("new::variant", Failed)]),
        );
        assert_eq!(
            classify_red_added((&own, None), (&without, None)),
            RedClass::Assertion
        );
        // No per-test report: by the runner's counts.
        let plain = |status| check("held-out", status, None);
        let count = |failed| RunnerSummary {
            passed: 1,
            failed,
            ..RunnerSummary::default()
        };
        let (w, o) = (plain(CheckStatus::Failed), plain(CheckStatus::Failed));
        assert_eq!(
            classify_red_added((&w, Some(&count(1))), (&o, Some(&count(1)))),
            RedClass::Passed
        );
        assert_eq!(
            classify_red_added((&w, Some(&count(2))), (&o, Some(&count(1)))),
            RedClass::Assertion
        );
        // Without the file the runner refused (no count): the failure is the file's.
        let refused = plain(CheckStatus::Failed);
        assert_eq!(
            classify_red_added((&w, Some(&count(1))), (&refused, None)),
            RedClass::Assertion
        );
        // Not red on its own terms stays what it is.
        let built = plain(CheckStatus::Error);
        let broken = RunnerSummary {
            build_failed: true,
            ..RunnerSummary::default()
        };
        assert_eq!(
            classify_red_added((&built, Some(&broken)), (&o, Some(&count(1)))),
            RedClass::BuildError
        );
    }

    fn check(
        name: &str,
        status: CheckStatus,
        tests: Option<&[(&str, TestOutcome)]>,
    ) -> CheckResult {
        CheckResult {
            name: name.to_owned(),
            argv: vec!["x".to_owned()],
            status,
            exit_code: Some(i32::from(status != CheckStatus::Passed)),
            duration_ms: 1,
            junit: tests.is_some(),
            tests: tests.map(|t| {
                t.iter()
                    .map(|(id, o)| TestResult {
                        id: (*id).to_owned(),
                        outcome: *o,
                    })
                    .collect()
            }),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            truncated: false,
            note: None,
            flaky: false,
        }
    }

    fn report(checks: Vec<CheckResult>) -> VerifyReport {
        VerifyReport {
            root: "/p".to_owned(),
            tier: "fast".to_owned(),
            checks,
            comparison: None,
            passed: true,
        }
    }

    use CheckStatus::{Failed, Passed};
    use TestOutcome as T;

    #[test]
    fn comparison_finds_regressions_pre_existing_and_new() {
        let base = report(vec![check(
            "test",
            Failed,
            Some(&[("a", T::Passed), ("b", T::Passed), ("c", T::Failed)]),
        )]);
        let now = report(vec![check(
            "test",
            Failed,
            Some(&[
                ("a", T::Passed),
                ("b", T::Failed),
                ("c", T::Failed),
                ("d", T::Passed),
            ]),
        )]);
        let cmp = compare(&base, &now);
        assert_eq!(
            cmp.regressions,
            [Regression {
                id: "test/b".to_owned(),
                now: "failed".to_owned()
            }]
        );
        assert_eq!(cmp.pre_existing, ["test/c"]);
        assert_eq!(cmp.new_tests, ["test/d"]);
        assert_eq!((cmp.count_before, cmp.count_after), (3, 4));
        assert!(cmp.failing_as_before.is_empty(), "b is a new failure");
    }

    #[test]
    fn missing_test_is_a_regression() {
        let base = report(vec![check(
            "test",
            Passed,
            Some(&[("a", T::Passed), ("b", T::Passed)]),
        )]);
        let now = report(vec![check("test", Passed, Some(&[("a", T::Passed)]))]);
        let cmp = compare(&base, &now);
        assert_eq!(cmp.regressions[0].now, "missing");
        assert!(!passes(&now.checks, Some(&cmp)));
    }

    #[test]
    fn pre_existing_failures_alone_do_not_fail_against_a_baseline() {
        let base = report(vec![check(
            "test",
            Failed,
            Some(&[("a", T::Passed), ("c", T::Failed)]),
        )]);
        let now = report(vec![check(
            "test",
            Failed,
            Some(&[("a", T::Passed), ("c", T::Failed)]),
        )]);
        let cmp = compare(&base, &now);
        assert_eq!(cmp.failing_as_before, ["test"]);
        assert!(passes(&now.checks, Some(&cmp)));
        assert!(
            !passes(&now.checks, None),
            "without a baseline a failing check fails"
        );
    }

    #[test]
    fn verdict_levels() {
        let passing = check("unit", Passed, Some(&[("a", T::Passed)]));
        let empty = check("unit", Passed, Some(&[]));
        let lint = check("lint", Passed, None);
        assert_eq!(
            verdict(&["unit"], &[passing.clone(), lint.clone()], None),
            Verdict::Verified
        );
        assert_eq!(
            verdict(&[], &[passing.clone(), lint.clone()], None),
            Verdict::RegressionOnly
        );
        assert_eq!(
            verdict(&["unit"], &[empty], None),
            Verdict::RegressionOnly,
            "zero tests is no evidence"
        );
        assert_eq!(
            verdict(&["lint"], &[lint], None),
            Verdict::Verified,
            "a passing command check is evidence"
        );
        let failing = check("lint", Failed, None);
        assert!(matches!(
            verdict(&["unit"], &[passing, failing], None),
            Verdict::Failed { .. }
        ));
    }

    fn summary(failed: usize, build_failed: bool, unresolved_only: bool) -> RunnerSummary {
        RunnerSummary {
            passed: 0,
            failed,
            errors: 0,
            build_failed,
            unresolved_only,
        }
    }

    #[test]
    fn red_is_an_assertion_or_an_absent_symbol() {
        use TestOutcome as T;
        let failed = check("a", CheckStatus::Failed, Some(&[("t", T::Failed)]));
        assert_eq!(classify_red(&failed, None), RedClass::Assertion);
        let errored = check("a", CheckStatus::Failed, Some(&[("t", T::Error)]));
        assert_eq!(classify_red(&errored, None), RedClass::CollectionError);
        let empty = check("a", CheckStatus::Failed, Some(&[]));
        assert_eq!(classify_red(&empty, None), RedClass::NoTests);
        let plain = check("a", CheckStatus::Failed, None);
        assert_eq!(classify_red(&plain, None), RedClass::Unclassified);
        assert_eq!(
            classify_red(&plain, Some(&summary(2, false, false))),
            RedClass::Assertion
        );
        assert_eq!(
            classify_red(&plain, Some(&summary(0, true, false))),
            RedClass::BuildError
        );
        assert_eq!(
            classify_red(&plain, Some(&summary(0, true, true))),
            RedClass::Absent
        );
        assert_eq!(
            classify_red(&plain, Some(&summary(0, false, false))),
            RedClass::NoTests
        );
        let timed_out = check("a", CheckStatus::TimedOut, None);
        assert_eq!(
            classify_red(&timed_out, Some(&summary(1, false, false))),
            RedClass::Timeout
        );
        let passed = check("a", CheckStatus::Passed, Some(&[("t", T::Passed)]));
        assert_eq!(classify_red(&passed, None), RedClass::Passed);
        assert!(RedClass::Assertion.is_red() && RedClass::Absent.is_red());
        assert!(!RedClass::BuildError.is_red() && !RedClass::Flaky.is_red());
    }

    #[test]
    fn a_flaky_pass_is_not_evidence_and_errors_regress() {
        use TestOutcome as T;
        let mut c = check("acceptance", CheckStatus::Passed, None);
        assert!(c.is_evidence());
        c.flaky = true;
        assert!(!c.is_evidence());
        assert_eq!(
            verdict(&["acceptance"], &[c], None),
            Verdict::RegressionOnly
        );

        let before = report(vec![check(
            "t",
            CheckStatus::Passed,
            Some(&[("x", T::Passed)]),
        )]);
        let after = report(vec![check(
            "t",
            CheckStatus::Failed,
            Some(&[("x", T::Error)]),
        )]);
        let cmp = compare(&before, &after);
        assert_eq!(cmp.regressions[0].now, "error");
        let again = compare(&after, &after);
        assert_eq!(again.pre_existing, ["t/x"]);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn statuses() -> impl Strategy<Value = CheckStatus> {
            prop_oneof![
                Just(CheckStatus::Passed),
                Just(CheckStatus::Failed),
                Just(CheckStatus::TimedOut),
                Just(CheckStatus::Skipped),
                Just(CheckStatus::Error),
            ]
        }

        fn outcomes() -> impl Strategy<Value = TestOutcome> {
            prop_oneof![
                Just(T::Passed),
                Just(T::Failed),
                Just(T::Skipped),
                Just(T::Error)
            ]
        }

        fn checks() -> impl Strategy<Value = Vec<CheckResult>> {
            prop::collection::vec(
                (
                    statuses(),
                    prop::option::of(prop::collection::vec(outcomes(), 0..5)),
                    any::<bool>(),
                ),
                1..5,
            )
            .prop_map(|v| {
                v.into_iter()
                    .enumerate()
                    .map(|(i, (status, tests, flaky))| CheckResult {
                        name: format!("c{i}"),
                        argv: vec!["x".to_owned()],
                        status,
                        exit_code: None,
                        duration_ms: 0,
                        junit: tests.is_some(),
                        tests: tests.map(|t| {
                            t.into_iter()
                                .enumerate()
                                .map(|(j, outcome)| TestResult {
                                    id: format!("t{j}"),
                                    outcome,
                                })
                                .collect()
                        }),
                        stdout_tail: String::new(),
                        stderr_tail: String::new(),
                        truncated: false,
                        note: None,
                        flaky,
                    })
                    .collect()
            })
        }

        fn held_outs() -> impl Strategy<Value = HeldOut> {
            prop_oneof![
                Just(HeldOut::Off),
                Just(HeldOut::Absent {
                    reason: "not_red".to_owned()
                }),
                (0..4usize).prop_map(|files| HeldOut::Passed { files }),
                proptest::collection::vec("US[1-3]-[1-4]", 1..4)
                    .prop_map(|scenarios| HeldOut::Failed { scenarios }),
            ]
        }

        proptest! {
            #[test]
            fn a_held_out_failure_is_never_a_pass(cs in checks(), pick in 0..5usize, h in held_outs()) {
                let name = format!("c{pick}");
                let before = verdict(&[name.as_str()], &cs, None);
                let after = with_held_out(before.clone(), &h);
                match h {
                    HeldOut::Failed { .. } => {
                        let failed = matches!(after, Verdict::Failed { .. });
                        prop_assert!(failed);
                    }
                    _ => prop_assert_eq!(after, before),
                }
            }

            #[test]
            fn any_failing_check_means_failed(cs in checks(), pick in 0..5usize) {
                let task = [format!("c{pick}")];
                let task: Vec<&str> = task.iter().map(String::as_str).collect();
                let v = verdict(&task, &cs, None);
                let any_failure = cs.iter().any(|c| c.status.is_failure());
                prop_assert_eq!(matches!(v, Verdict::Failed { .. }), any_failure);
            }

            #[test]
            fn verified_requires_task_evidence(cs in checks(), pick in 0..5usize) {
                let name = format!("c{pick}");
                let v = verdict(&[name.as_str()], &cs, None);
                if v == Verdict::Verified {
                    let c = cs.iter().find(|c| c.name == name);
                    prop_assert!(c.is_some_and(CheckResult::is_evidence));
                }
            }

            #[test]
            fn a_flaky_task_check_is_never_verified(cs in checks(), pick in 0..5usize) {
                let name = format!("c{pick}");
                let v = verdict(&[name.as_str()], &cs, None);
                if cs.iter().find(|c| c.name == name).is_some_and(|c| c.flaky) {
                    prop_assert_ne!(v, Verdict::Verified);
                }
            }

            #[test]
            fn red_always_has_a_cause(
                cs in checks(),
                passed in 0..3usize,
                failed in 0..3usize,
                errors in 0..3usize,
                build_failed in any::<bool>(),
                unresolved_only in any::<bool>(),
                with_summary in any::<bool>(),
            ) {
                let summary = RunnerSummary { passed, failed, errors, build_failed, unresolved_only };
                let summary = with_summary.then_some(summary);
                for c in &cs {
                    let class = classify_red(c, summary.as_ref());
                    if class.is_red() {
                        let assertion_in_report = c
                            .tests
                            .iter()
                            .flatten()
                            .any(|t| t.outcome == T::Failed);
                        let by_summary = summary.is_some_and(|s| s.failed > 0 || (s.build_failed && s.unresolved_only));
                        prop_assert!(assertion_in_report || by_summary);
                        prop_assert_ne!(c.status, CheckStatus::TimedOut);
                    }
                }
            }

            #[test]
            fn a_run_compared_with_itself_has_no_regressions(cs in checks()) {
                let r = report(cs);
                let cmp = compare(&r, &r);
                prop_assert!(cmp.regressions.is_empty());
                prop_assert!(cmp.new_tests.is_empty());
                prop_assert_eq!(cmp.count_before, cmp.count_after);
            }

            #[test]
            fn regressions_always_fail_the_verdict(cs in checks()) {
                let before = report(cs.clone());
                let mut after = report(cs);
                for c in &mut after.checks {
                    if let Some(tests) = &mut c.tests {
                        tests.clear();
                    }
                }
                let cmp = compare(&before, &after);
                let had_passing = cmp.count_before > 0
                    && before.checks.iter().flat_map(|c| c.tests.iter().flatten()).any(|t| t.outcome == T::Passed);
                if had_passing {
                    let failed = matches!(verdict(&[], &after.checks, Some(&cmp)), Verdict::Failed { .. });
                    prop_assert!(failed);
                }
            }
        }
    }
}
