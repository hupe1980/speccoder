//! The judge's vocabulary: check results, baseline comparison, and verdicts
//!.
//!
//! Pure: running checks is the runtime's job; deciding what their results
//! mean happens here, where it is property-tested.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestOutcome {
    /// Passed.
    Passed,
    /// Failed or errored.
    Failed,
    /// Skipped or ignored.
    Skipped,
}

/// One test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    /// `classname::name`, or `name` when there is no class.
    pub id: String,
    /// What happened.
    pub outcome: TestOutcome,
}

/// One check run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl CheckResult {
    /// Counts of (passed, failed, skipped) tests.
    #[must_use]
    pub fn test_counts(&self) -> Option<(usize, usize, usize)> {
        self.tests.as_ref().map(|tests| {
            let n = |o| tests.iter().filter(|t| t.outcome == o).count();
            (
                n(TestOutcome::Passed),
                n(TestOutcome::Failed),
                n(TestOutcome::Skipped),
            )
        })
    }

    /// Whether this check, as a task-specific check, is evidence for the
    /// task: it passed, and either declares no report (a command check) or
    /// its report has at least one passing test (research R4).
    #[must_use]
    pub fn is_evidence(&self) -> bool {
        self.status == CheckStatus::Passed
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
            .filter(|t| t.outcome == TestOutcome::Failed)
            .map(|t| t.id.as_str())
            .collect()
    }
}

/// Everything one `verify` run established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
                    _ => "skipped",
                }
                .to_owned(),
            }),
            (TestOutcome::Passed, None) => cmp.regressions.push(Regression {
                id: id.clone(),
                now: "missing".to_owned(),
            }),
            (TestOutcome::Failed, Some(TestOutcome::Failed)) => cmp.pre_existing.push(id.clone()),
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

/// How strong the evidence for a task is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
            prop_oneof![Just(T::Passed), Just(T::Failed), Just(T::Skipped)]
        }

        fn checks() -> impl Strategy<Value = Vec<CheckResult>> {
            prop::collection::vec(
                (
                    statuses(),
                    prop::option::of(prop::collection::vec(outcomes(), 0..5)),
                ),
                1..5,
            )
            .prop_map(|v| {
                v.into_iter()
                    .enumerate()
                    .map(|(i, (status, tests))| CheckResult {
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
                    })
                    .collect()
            })
        }

        proptest! {
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
