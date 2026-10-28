//! What to do next, from a command's result: its status and payload
//! classified into an [`Outcome`], whose steps every renderer shows — the
//! human `Next:` block, `data.next`, the TUI footer.

use speccoder_core::next::{Outcome, next};
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};

/// `result` with its next steps and its code attached; `feature` is the
/// `--feature` the command was given, carried to speccoder's own commands.
/// A code the result already names is kept; otherwise the outcome's, or
/// the status's default — every non-ok result leaves with one.
#[must_use]
pub fn attach(result: CommandResult, feature: Option<&str>) -> CommandResult {
    // A code that says exactly what is wrong decides what comes next; the
    // status alone ("a config error") would send a missing key to `init`.
    if result.next().is_empty()
        && let Some(steps) = result.code().and_then(|c| by_code(c.name))
    {
        return result.with_next(steps);
    }
    let outcome = classify(&result);
    // Steps already attached (an in-place run's) are kept.
    let mut result = match &outcome {
        Some(o) if result.next().is_empty() => {
            let steps = next(o, feature);
            result.with_next(steps)
        }
        _ => result,
    };
    if !result.has_explicit_code() {
        let task = outcome.as_ref().and_then(outcome_task);
        let named = outcome
            .as_ref()
            .and_then(outcome_code)
            .or_else(|| regression_only(&result).then_some("REGRESSION-ONLY"))
            .and_then(speccoder_core::codes::lookup)
            .filter(|e| e.status == result.status())
            .or_else(|| speccoder_core::codes::default_for(result.status()));
        if let Some(entry) = named {
            let mut fix =
                speccoder_core::codes::fixes(entry, result.command(), task.as_deref(), feature);
            if entry.name == "REGRESSION-ONLY"
                && let Some(why) = regression_fix(&result, feature)
            {
                fix = why;
            }
            result = result.with_code(entry.name).with_fix(fix);
        }
    }
    result
}

/// Next steps for a failure whose code names it precisely.
fn by_code(code: &str) -> Option<Vec<speccoder_core::next::NextStep>> {
    let step = |command: &str, why: &str| speccoder_core::next::NextStep {
        command: command.to_owned(),
        why: why.to_owned(),
    };
    Some(match code {
        "KEY-MISSING" => vec![step(
            "speccoder doctor",
            "check the rest of the setup once the key is set",
        )],
        "SETUP-INCOMPLETE" => vec![
            step(
                "speccoder fix --dry-run",
                "what the fix commits, changing nothing",
            ),
            step("speccoder run", "run again once it is committed"),
        ],
        "CONFIG-INVALID" => vec![step(
            "speccoder config",
            "every key, its value, and what is wrong",
        )],
        "NOTHING-TO-HAND-OFF" => vec![step(
            "speccoder run",
            "tick tasks first: a pull request carries their evidence",
        )],
        "NO-CHECKS" => vec![step(
            "speccoder config",
            "every key: add a check under Checks",
        )],
        "CONFIG-EXISTS" => vec![step(
            "speccoder config",
            "every key and its value: change what you need",
        )],
        "CONFIG-NOT-TRACKED" => vec![step(
            "speccoder config commit",
            "speccoder reads committed configuration only",
        )],
        "GITHUB-AUTH" => vec![step(
            "speccoder auth login --with-token",
            "sign in to GitHub (the token goes to the keychain)",
        )],
        "GITHUB-FAILED" | "NO-REMOTE" => {
            vec![step("speccoder doctor", "check the rest of the setup")]
        }
        // Outside a project: start one, or see what speccoder does first.
        "NO-PROJECT" | "NO-REPOSITORY" => vec![
            step(
                "speccoder new \"<what you are building>\"",
                "start a project here: the stack's and the spec format's own tools, then one commit",
            ),
            step(
                "speccoder sample",
                "a recorded run, replayed — no model, no key, no network",
            ),
        ],
        _ => return None,
    })
}

/// The catalog code an outcome stands for.
const fn outcome_code(outcome: &Outcome) -> Option<&'static str> {
    Some(match outcome {
        Outcome::NeedsApproval { .. } => "NEEDS-APPROVAL",
        Outcome::Blocked { .. } => "BLOCKED",
        Outcome::BudgetExhausted { .. } => "BUDGET-EXHAUSTED",
        Outcome::VerificationFailed { .. } => "VERIFICATION-FAILED",
        Outcome::SuspectedGaming { .. } => "SUSPECTED-GAMING",
        Outcome::TestsOverStrict { .. } => "TESTS-OVER-STRICT",
        Outcome::AcceptanceWeak { .. } => "ACCEPTANCE-WEAK",
        Outcome::NotDemonstrated { .. } => "NOT-DEMONSTRATED",
        Outcome::RunFailed { .. } => "RUN-FAILED",
        Outcome::Interrupted { .. } => "INTERRUPTED",
        Outcome::NothingReady => "TASK-NOT-READY",
        Outcome::ChecklistsOpen => "CHECKLISTS-OPEN",
        Outcome::ConfigError => "CONFIG",
        Outcome::GitPrecondition => "GIT",
        Outcome::ArchiveAwaiting { .. } => "ARCHIVE-AWAITING",
        _ => return None,
    })
}

fn outcome_task(outcome: &Outcome) -> Option<String> {
    match outcome {
        Outcome::NeedsApproval { task }
        | Outcome::Blocked { task }
        | Outcome::BudgetExhausted { task }
        | Outcome::VerificationFailed { task }
        | Outcome::SuspectedGaming { task }
        | Outcome::TestsOverStrict { task }
        | Outcome::AcceptanceWeak { task }
        | Outcome::NotDemonstrated { task }
        | Outcome::Interrupted { task }
        | Outcome::TaskPassed { task, .. } => Some(task.clone()),
        Outcome::RunFailed { task } => task.clone(),
        _ => None,
    }
}

/// The fix for why a tick was regression-only (the last such tick's reason).
fn regression_fix(result: &CommandResult, feature: Option<&str>) -> Option<Vec<String>> {
    use speccoder_core::regression::RegressionReason as R;
    let reason = match result.data() {
        Some(CommandData::Run(r)) => r.regression.clone(),
        Some(CommandData::Feature(f)) => f.attempts.iter().rev().find_map(|a| a.regression.clone()),
        _ => None,
    }?
    .reason;
    let f = feature.map_or_else(String::new, |f| format!(" --feature {f}"));
    let checks = format!("speccoder doctor{f}");
    let converge = format!("speccoder converge{f}");
    Some(match reason {
        R::AcceptanceOff => vec!["→ set [acceptance] generate = true in speccoder.toml".to_owned()],
        R::NoScenarios { story } => vec![
            format!("→ add an acceptance scenario to {story}, then converge"),
            converge,
        ],
        R::AlreadyPassing { .. } | R::NoStory => vec![
            format!("{checks}  (the plan checks: behaviour planned before its story)"),
            converge,
        ],
        R::AuthorWroteNone
        | R::AuthorFailed { .. }
        | R::NotRed { .. }
        | R::TestsNotCollected
        | R::Unrecorded => vec![checks, converge],
        // Design documents: nothing to test, nothing to do.
        R::PlanningWork => Vec::new(),
    })
}

/// An `ok` run that ticked a task on the regression checks alone.
fn regression_only(result: &CommandResult) -> bool {
    use speccoder_core::verify::Verdict;
    let is = |r: &RunReport| r.ticked && r.verdict == Some(Verdict::RegressionOnly);
    result.status() == CommandStatus::Ok
        && match result.data() {
            Some(CommandData::Run(r)) => is(r),
            Some(CommandData::Feature(f)) => f.attempts.iter().any(is),
            _ => false,
        }
}

/// The steps of a result from an in-place run, in place: `speccoder run`,
/// `accept` and `converge` gain `--in-place` (a worktree-mode run would not
/// see git-ignored specs, and is refused while the session holds them).
#[must_use]
pub fn in_place(result: CommandResult) -> CommandResult {
    let steps = result
        .next()
        .iter()
        .cloned()
        .map(|mut s| {
            for verb in ["run", "accept", "converge"] {
                let head = format!("speccoder {verb}");
                if (s.command == head || s.command.starts_with(&format!("{head} ")))
                    && !s.command.contains("--in-place")
                {
                    s.command = format!("{head} --in-place{}", &s.command[head.len()..]);
                }
            }
            s
        })
        .collect();
    result.with_next(steps)
}

fn classify(result: &CommandResult) -> Option<Outcome> {
    let status = result.status();
    match (result.command(), status, result.data()) {
        // An in-place refusal or stop names its own fix.
        (_, _, Some(CommandData::InPlace(_))) => None,
        (_, _, Some(CommandData::Checklists(_))) => Some(Outcome::ChecklistsOpen),
        ("spec", CommandStatus::NeedsHuman, Some(CommandData::Archive(a)))
            if a.stage == "awaiting_confirmation" =>
        {
            Some(Outcome::ArchiveAwaiting {
                change: a.change.clone(),
            })
        }
        ("run", _, Some(CommandData::Run(r))) => Some(of_run(status, r)),
        ("run", _, Some(CommandData::Feature(f))) => match (status, f.attempts.last()) {
            (CommandStatus::Ok, _) if f.stop.starts_with("feature done") => {
                Some(Outcome::FeatureDone)
            }
            (CommandStatus::Ok, Some(last)) => Some(Outcome::TaskPassed {
                task: last.task.clone(),
                ready: 1,
            }),
            (CommandStatus::Ok, None) => Some(Outcome::FeatureDone),
            (CommandStatus::TaskNotReady, _) => Some(Outcome::NothingReady),
            (_, Some(last)) => Some(of_run(status, last)),
            (_, None) => of_status(status),
        },
        ("report", CommandStatus::Ok, Some(CommandData::Report(r))) if !r.planning.is_empty() => {
            Some(Outcome::Planning {
                skills: r.planning.clone(),
            })
        }
        ("report", CommandStatus::Ok, Some(CommandData::Report(r))) => Some(Outcome::ReportShown {
            ready: r.totals.ready,
            stale: r.totals.stale,
            open_approvals: r
                .tasks
                .iter()
                .filter(|t| {
                    t.last_attempt
                        .as_ref()
                        .is_some_and(|a| a.outcome == "needs_approval")
                })
                .count(),
        }),
        ("approvals", CommandStatus::Ok, Some(CommandData::Approvals(a))) => {
            Some(Outcome::ApprovalsListed {
                first: a.pending.first().map(|p| p.task.clone()),
            })
        }
        ("approve" | "reject" | "accept" | "skip" | "nudge", CommandStatus::Ok, _) => {
            Some(Outcome::Decided)
        }
        _ => of_status(status),
    }
}

/// One task run's outcome.
fn of_run(status: CommandStatus, r: &RunReport) -> Outcome {
    let task = r.task.clone();
    let suspected = r.monitor.as_ref().is_some_and(|m| m["suspected"] == true);
    match (status, r.attempt_status.as_str()) {
        (CommandStatus::Ok, _) => Outcome::TaskPassed { task, ready: 1 },
        _ if suspected => Outcome::SuspectedGaming { task },
        (_, "over_strict") => Outcome::TestsOverStrict { task },
        (_, "acceptance_weak") => Outcome::AcceptanceWeak { task },
        (_, "needs_approval") => Outcome::NeedsApproval { task },
        (_, "blocked") => Outcome::Blocked { task },
        (_, "budget_exhausted" | "incomplete" | "denied") => Outcome::BudgetExhausted { task },
        (_, "interrupted") => Outcome::Interrupted { task },
        (_, "not_demonstrated") => Outcome::NotDemonstrated { task },
        (CommandStatus::VerificationFailed, _) => Outcome::VerificationFailed { task },
        (CommandStatus::ConfigError, _) => Outcome::ConfigError,
        (CommandStatus::GitPrecondition, _) => Outcome::GitPrecondition,
        _ => Outcome::RunFailed { task: Some(task) },
    }
}

/// Steps that depend only on how a command failed.
fn of_status(status: CommandStatus) -> Option<Outcome> {
    match status {
        CommandStatus::ConfigError => Some(Outcome::ConfigError),
        CommandStatus::GitPrecondition => Some(Outcome::GitPrecondition),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_and_config_errors_get_their_steps() {
        let decided = attach(CommandResult::ok("approve", "approved"), Some("001-x"));
        assert_eq!(decided.next()[0].command, "speccoder run --feature 001-x");
        let config = attach(
            CommandResult::new("verify", CommandStatus::ConfigError, "x"),
            None,
        );
        assert_eq!(config.next()[0].command, "speccoder init");
        assert!(
            attach(CommandResult::ok("inspect", "x"), None)
                .next()
                .is_empty()
        );
    }

    #[test]
    fn steps_after_an_in_place_run_stay_in_place() {
        let decided = in_place(attach(
            CommandResult::ok("approve", "approved"),
            Some("001-x"),
        ));
        assert_eq!(
            decided.next()[0].command,
            "speccoder run --in-place --feature 001-x"
        );
        let refused = attach(
            CommandResult::new("run", CommandStatus::GitPrecondition, "x")
                .with_data(CommandData::InPlace(Box::new(
                    speccoder_core::InPlaceReport { paths: Vec::new() },
                )))
                .with_code("DIRTY-CHECKOUT")
                .with_fix(vec!["git stash -u".to_owned()]),
            None,
        );
        assert!(refused.next().is_empty(), "the fix lines say what to do");
    }
}
