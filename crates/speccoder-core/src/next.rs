//! What to do next after a stop or a finish (F-053): a closed set of
//! outcomes and, for each, the commands that continue — deterministic, never
//! model text. The human `Next:` block, the TUI footer and `data.next` are
//! all this list.

use serde::Serialize;

/// One step a person can take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct NextStep {
    /// A copy-pasteable command (`"…"` marks text the person writes).
    pub command: String,
    /// Why, in one short line.
    pub why: String,
}

/// How a command ended, as far as what comes next is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A task passed; `ready` tasks can run now.
    TaskPassed {
        /// The task.
        task: String,
        /// Tasks ready after it.
        ready: usize,
    },
    /// Every task of the feature is ticked.
    FeatureDone,
    /// A call waits for a person.
    NeedsApproval {
        /// The task.
        task: String,
    },
    /// The model stopped honestly: the task cannot be done as written.
    Blocked {
        /// The task.
        task: String,
    },
    /// Out of turns or calls, or the model did not finish.
    BudgetExhausted {
        /// The task.
        task: String,
    },
    /// The checks or the acceptance tests failed after the retries.
    VerificationFailed {
        /// The task.
        task: String,
    },
    /// The gaming monitor suspects a shortcut.
    SuspectedGaming {
        /// The task.
        task: String,
    },
    /// An acceptance test rejects an implementation the spec appears to
    /// allow; the attempt is held for a ruling (F-060).
    TestsOverStrict {
        /// The task.
        task: String,
    },
    /// The acceptance set stayed below `min_strength` (F-060).
    AcceptanceWeak {
        /// The task.
        task: String,
    },
    /// A convergence gap no failing test shows, or unfinished checklists.
    NotDemonstrated {
        /// The task.
        task: String,
    },
    /// The run itself failed (provider, store, …).
    RunFailed {
        /// The task, when one was running.
        task: Option<String>,
    },
    /// Killed while running; the next run resumes.
    Interrupted {
        /// The task.
        task: String,
    },
    /// A run is in progress.
    Running,
    /// No task can run.
    NothingReady,
    /// The feature is being planned: it has no task list yet; these
    /// planning skills come next.
    Planning {
        /// The skills, in order (`plan`, `tasks`).
        skills: Vec<String>,
    },
    /// `report` shown.
    ReportShown {
        /// Tasks ready to run.
        ready: usize,
        /// Calls waiting for a person.
        open_approvals: usize,
        /// Ticks proven against an earlier wording of what they cite.
        stale: usize,
    },
    /// `approvals` listed.
    ApprovalsListed {
        /// The first task with a call waiting.
        first: Option<String>,
    },
    /// A person's decision recorded (approve, reject, accept, skip, nudge).
    Decided,
    /// The feature's checklists have unchecked items; nothing ran.
    ChecklistsOpen,
    /// `speccoder.toml` or the environment is wrong.
    ConfigError,
    /// git refused (dirty tree, uncommitted artifacts, …).
    GitPrecondition,
    /// An OpenSpec archive waits for a person's yes.
    ArchiveAwaiting {
        /// The change.
        change: String,
    },
}

/// The steps that continue after `outcome`; `--feature F` is appended to
/// speccoder's own commands when given. Total, non-empty, deterministic.
#[must_use]
pub fn next(outcome: &Outcome, feature: Option<&str>) -> Vec<NextStep> {
    use Outcome as O;
    let steps: Vec<(String, &str)> = match outcome {
        O::NeedsApproval { task } => vec![
            (
                "speccoder report --approvals".to_owned(),
                "see the call waiting and why it stopped",
            ),
            (
                format!("speccoder task approve {task}"),
                "allow it; the attempt resumes on the next run",
            ),
            (
                format!("speccoder task reject {task} --reason \"…\""),
                "refuse it; the model is told your reason",
            ),
            ("speccoder run".to_owned(), "continue the feature"),
        ],
        O::Blocked { task } => vec![
            (
                format!("speccoder task nudge {task} --note \"…\""),
                "tell the next attempt what it is missing",
            ),
            (
                format!("speccoder task accept {task} --reason \"…\""),
                "tick it on your word (attested)",
            ),
            (
                format!("speccoder task skip {task} --reason \"…\""),
                "pass it by; its dependents stay blocked",
            ),
        ],
        O::BudgetExhausted { task } => vec![
            (
                format!("speccoder task nudge {task} --note \"…\""),
                "narrow the next attempt",
            ),
            (format!("speccoder run --task {task}"), "try the task again"),
            (
                format!("speccoder task skip {task} --reason \"…\""),
                "pass it by; its dependents stay blocked",
            ),
        ],
        O::VerificationFailed { task } => vec![
            (
                "speccoder report".to_owned(),
                "see what failed and the saved attempt",
            ),
            (
                format!("speccoder task nudge {task} --note \"…\""),
                "tell the next attempt what to fix",
            ),
            (format!("speccoder run --task {task}"), "try the task again"),
        ],
        O::SuspectedGaming { task } => vec![
            (
                "speccoder report".to_owned(),
                "review the saved attempt and the findings",
            ),
            (format!("speccoder run --task {task}"), "try the task again"),
            (
                format!("speccoder task skip {task} --reason \"…\""),
                "pass it by; its dependents stay blocked",
            ),
        ],
        O::TestsOverStrict { task } => vec![
            (
                format!("speccoder task rule {task} --tests-right --reason \"…\""),
                "the tests are right: the spec does not allow that alternative; the next run ticks the held attempt",
            ),
            (
                format!("speccoder run --task {task} --fresh-acceptance"),
                "the tests are too strict: write them again and attempt anew",
            ),
            (
                format!("speccoder task accept {task} --reason \"…\""),
                "tick it on your word (attested, not verified)",
            ),
        ],
        O::AcceptanceWeak { task } => vec![
            (
                "speccoder report".to_owned(),
                "see which wrong implementations the tests let through",
            ),
            (
                format!("speccoder run --task {task}"),
                "try the task again with fresh acceptance tests",
            ),
            (
                format!("speccoder task accept {task} --reason \"…\""),
                "tick it on your word (attested, not verified)",
            ),
        ],
        O::NotDemonstrated { task } => vec![
            (
                "speccoder report".to_owned(),
                "see why nothing was demonstrated",
            ),
            (
                format!("speccoder task nudge {task} --note \"…\""),
                "tell the next attempt what is meant",
            ),
            (
                format!("speccoder task accept {task} --reason \"…\""),
                "tick it on your word (attested)",
            ),
        ],
        O::RunFailed { task } => {
            let mut v = vec![(
                "speccoder report --live".to_owned(),
                "see where the run stopped",
            )];
            v.push(match task {
                Some(t) => (format!("speccoder run --task {t}"), "try the task again"),
                None => ("speccoder run".to_owned(), "try again"),
            });
            v
        }
        O::Interrupted { .. } => vec![(
            "speccoder run".to_owned(),
            "resume; finished calls will not repeat",
        )],
        O::Running => vec![
            ("speccoder tui".to_owned(), "watch the run"),
            ("speccoder report --live".to_owned(), "where it is now"),
        ],
        O::TaskPassed { ready, .. } if *ready > 0 => vec![
            ("speccoder run".to_owned(), "continue with the ready tasks"),
            ("speccoder report".to_owned(), "every task and its evidence"),
        ],
        O::TaskPassed { .. } | O::FeatureDone => vec![
            ("speccoder report".to_owned(), "every task and its evidence"),
            (
                "speccoder pr".to_owned(),
                "hand the branch off as a draft pull request",
            ),
        ],
        O::NothingReady => vec![
            (
                "speccoder report".to_owned(),
                "see what blocks the open tasks",
            ),
            (
                "speccoder report --approvals".to_owned(),
                "calls waiting for a person",
            ),
        ],
        O::Planning { skills } => {
            let mut steps: Vec<(String, &str)> = skills
                .iter()
                .map(|s| {
                    let why = match s.as_str() {
                        "plan" => "write the plan from the spec",
                        "tasks" => "break the plan into tasks",
                        "clarify" => "settle what the spec leaves open",
                        _ => "the next planning step",
                    };
                    (format!("speccoder spec {s}"), why)
                })
                .collect();
            steps.push((
                "speccoder run".to_owned(),
                "after you review and commit what they write",
            ));
            steps
        }
        O::ReportShown { open_approvals, .. } if *open_approvals > 0 => vec![
            (
                "speccoder report --approvals".to_owned(),
                "calls waiting for a person",
            ),
            ("speccoder run".to_owned(), "continue the feature"),
        ],
        O::ReportShown { stale, .. } if *stale > 0 => vec![(
            "speccoder run --converge".to_owned(),
            "the spec changed: re-check the code against it as it is now",
        )],
        O::ReportShown { ready, .. } if *ready > 0 => {
            vec![("speccoder run".to_owned(), "continue with the ready tasks")]
        }
        O::ReportShown { .. } => vec![(
            "speccoder pr".to_owned(),
            "hand the branch off as a draft pull request",
        )],
        O::ApprovalsListed { first: Some(task) } => vec![
            (
                format!("speccoder task approve {task}"),
                "allow it; the attempt resumes on the next run",
            ),
            (
                format!("speccoder task reject {task} --reason \"…\""),
                "refuse it; the model is told your reason",
            ),
        ],
        O::ApprovalsListed { first: None } | O::Decided => {
            vec![("speccoder run".to_owned(), "continue the feature")]
        }
        O::ChecklistsOpen => vec![(
            "speccoder run --ignore-checklists".to_owned(),
            "run anyway; recorded on every commit",
        )],
        O::ConfigError => vec![
            (
                "speccoder init".to_owned(),
                "write or repair speccoder.toml",
            ),
            (
                "speccoder verify".to_owned(),
                "check the configured checks run",
            ),
        ],
        O::GitPrecondition => vec![
            ("git status".to_owned(), "see what is uncommitted"),
            ("speccoder run".to_owned(), "run again once it is committed"),
        ],
        O::ArchiveAwaiting { change } => vec![
            (
                format!("openspec show {change} --diff"),
                "see what archiving changes in the main specs",
            ),
            (
                format!("speccoder spec archive {change} --yes"),
                "archive it and commit the result",
            ),
        ],
    };
    steps
        .into_iter()
        .map(|(command, why)| NextStep {
            command: match feature {
                Some(f) if command.starts_with("speccoder ") => format!("{command} --feature {f}"),
                _ => command,
            },
            why: why.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands(o: &Outcome) -> Vec<String> {
        next(o, None).into_iter().map(|s| s.command).collect()
    }

    #[test]
    fn every_outcome_has_its_steps() {
        let t = || "T004".to_owned();
        let table: Vec<(Outcome, Vec<&str>)> = vec![
            (
                Outcome::NeedsApproval { task: t() },
                vec![
                    "speccoder report --approvals",
                    "speccoder task approve T004",
                    "speccoder task reject T004 --reason \"…\"",
                    "speccoder run",
                ],
            ),
            (
                Outcome::Blocked { task: t() },
                vec![
                    "speccoder task nudge T004 --note \"…\"",
                    "speccoder task accept T004 --reason \"…\"",
                    "speccoder task skip T004 --reason \"…\"",
                ],
            ),
            (
                Outcome::BudgetExhausted { task: t() },
                vec![
                    "speccoder task nudge T004 --note \"…\"",
                    "speccoder run --task T004",
                    "speccoder task skip T004 --reason \"…\"",
                ],
            ),
            (
                Outcome::VerificationFailed { task: t() },
                vec![
                    "speccoder report",
                    "speccoder task nudge T004 --note \"…\"",
                    "speccoder run --task T004",
                ],
            ),
            (
                Outcome::SuspectedGaming { task: t() },
                vec![
                    "speccoder report",
                    "speccoder run --task T004",
                    "speccoder task skip T004 --reason \"…\"",
                ],
            ),
            (
                Outcome::NotDemonstrated { task: t() },
                vec![
                    "speccoder report",
                    "speccoder task nudge T004 --note \"…\"",
                    "speccoder task accept T004 --reason \"…\"",
                ],
            ),
            (
                Outcome::RunFailed { task: Some(t()) },
                vec!["speccoder report --live", "speccoder run --task T004"],
            ),
            (
                Outcome::RunFailed { task: None },
                vec!["speccoder report --live", "speccoder run"],
            ),
            (Outcome::Interrupted { task: t() }, vec!["speccoder run"]),
            (
                Outcome::Running,
                vec!["speccoder tui", "speccoder report --live"],
            ),
            (
                Outcome::TaskPassed {
                    task: t(),
                    ready: 2,
                },
                vec!["speccoder run", "speccoder report"],
            ),
            (
                Outcome::TaskPassed {
                    task: t(),
                    ready: 0,
                },
                vec!["speccoder report", "speccoder pr"],
            ),
            (
                Outcome::FeatureDone,
                vec!["speccoder report", "speccoder pr"],
            ),
            (
                Outcome::NothingReady,
                vec!["speccoder report", "speccoder report --approvals"],
            ),
            (
                Outcome::ReportShown {
                    ready: 1,
                    open_approvals: 1,
                    stale: 1,
                },
                vec!["speccoder report --approvals", "speccoder run"],
            ),
            (
                Outcome::ReportShown {
                    ready: 1,
                    open_approvals: 0,
                    stale: 0,
                },
                vec!["speccoder run"],
            ),
            (
                Outcome::ReportShown {
                    ready: 0,
                    open_approvals: 0,
                    stale: 2,
                },
                vec!["speccoder run --converge"],
            ),
            (
                Outcome::ReportShown {
                    ready: 0,
                    open_approvals: 0,
                    stale: 0,
                },
                vec!["speccoder pr"],
            ),
            (
                Outcome::ApprovalsListed { first: Some(t()) },
                vec![
                    "speccoder task approve T004",
                    "speccoder task reject T004 --reason \"…\"",
                ],
            ),
            (
                Outcome::ApprovalsListed { first: None },
                vec!["speccoder run"],
            ),
            (Outcome::Decided, vec!["speccoder run"]),
            (
                Outcome::ChecklistsOpen,
                vec!["speccoder run --ignore-checklists"],
            ),
            (
                Outcome::ConfigError,
                vec!["speccoder init", "speccoder verify"],
            ),
            (
                Outcome::GitPrecondition,
                vec!["git status", "speccoder run"],
            ),
        ];
        for (outcome, expected) in &table {
            assert_eq!(commands(outcome), *expected, "{outcome:?}");
            let steps = next(outcome, None);
            assert!(!steps.is_empty());
            assert_eq!(steps, next(outcome, None), "deterministic");
            for s in &steps {
                assert!(s.why.chars().count() <= 60, "{}", s.why);
                assert!(
                    s.why.chars().next().is_some_and(char::is_lowercase),
                    "{}",
                    s.why
                );
            }
        }
    }

    #[test]
    fn the_feature_is_carried_to_speccoder_commands_only() {
        let steps = next(&Outcome::GitPrecondition, Some("001-x"));
        assert_eq!(steps[0].command, "git status");
        assert_eq!(steps[1].command, "speccoder run --feature 001-x");
    }
}
