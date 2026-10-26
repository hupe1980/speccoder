//! The hand-off of a bug that stopped before a fix, composed by the engine
//! from what the tries recorded — never written by a model. The author's
//! last words appear only as a labelled claim.

use speccoder_core::bug::{BugReport, Claim, Handoff, Outcome, Statements};
use speccoder_core::next::NextStep;
use speccoder_core::verify::RedClass;

/// What the person can do next with `bug` in `outcome`.
#[must_use]
pub fn next(bug: &BugReport, outcome: Outcome) -> Vec<NextStep> {
    let key = bug.key();
    let step = |command: String, why: &str| NextStep {
        command,
        why: why.to_owned(),
    };
    let reword = || {
        if bug.home.kind.native() {
            step(
                format!(
                    "speccoder bug --slug {}",
                    bug.task.trim_start_matches("bug:")
                ),
                "after rewording the assessment's statements",
            )
        } else {
            step(
                format!(
                    "speccoder review edit {} --feature {} \"…\"",
                    bug.requirement, bug.unit
                ),
                "reword the triad (the confirmation lapses)",
            )
        }
    };
    match outcome {
        Outcome::CannotReproduce | Outcome::UnchangedNotHolding => vec![
            step(
                format!(
                    "speccoder bug \"{}\" --expected \"…\" --test <file>::<test>",
                    bug.triad.current.text
                ),
                "bring a failing test yourself",
            ),
            reword(),
            step(
                format!("speccoder bug run --task {key}"),
                "try reproducing again",
            ),
            step(
                format!(
                    "speccoder task skip {} --feature {} --reason \"…\"",
                    bug.task, bug.unit
                ),
                "drop it",
            ),
        ],
        Outcome::AlreadySatisfied => vec![
            step(
                format!(
                    "speccoder task accept {} --feature {} --reason \"kept as a regression test\"",
                    bug.task, bug.unit
                ),
                "keep the reproduction as a regression test (attested)",
            ),
            step(
                format!(
                    "speccoder task skip {} --feature {} --reason \"already fixed\"",
                    bug.task, bug.unit
                ),
                "drop it",
            ),
        ],
        Outcome::AwaitingConfirmation => vec![
            step(
                format!(
                    "speccoder bug confirm {key} --digest {}",
                    bug.confirm_digest.clone().unwrap_or_default()
                ),
                "the reproductions show the bug: fix it",
            ),
            step(
                format!("speccoder bug confirm {key} --reject --reason \"…\""),
                "they do not: the author tries again, told why",
            ),
        ],
        Outcome::Recorded => vec![step(
            format!("speccoder bug run --task {key}"),
            "reproduce it",
        )],
        Outcome::Fixed | Outcome::Failed => vec![step(
            format!("speccoder report --feature {}", bug.unit),
            "the evidence",
        )],
    }
}

/// The hand-off for `bug` stopping in `outcome`: tries, per-statement
/// classes, what was and was not reproduced, cost, next steps.
#[must_use]
pub fn compose(
    bug: &BugReport,
    outcome: Outcome,
    elapsed_ms: u64,
    cost: Option<serde_json::Value>,
    claim: Option<String>,
) -> Handoff {
    let mut statements = Statements::default();
    for e in bug.expected_ids() {
        let classes: Vec<RedClass> = bug
            .tries
            .iter()
            .filter_map(|t| {
                t.per_statement
                    .iter()
                    .find(|s| s.statement == e)
                    .map(|s| s.class)
            })
            .collect();
        if classes.contains(&RedClass::Assertion) {
            statements.reproduced.push(e);
        } else if !classes.is_empty() && classes.iter().all(|c| *c == RedClass::Passed) {
            statements.satisfied.push(e);
        } else {
            statements.not_reproduced.push(e);
        }
    }
    let mut tries = bug.tries.clone();
    for t in &mut tries {
        for s in &mut t.per_statement {
            s.excerpt = s.excerpt.as_ref().map(|x| cut(x, 20));
        }
    }
    Handoff {
        outcome,
        tries,
        statements,
        cost,
        elapsed_ms,
        next: next(bug, outcome),
        claim: claim.filter(|c| !c.trim().is_empty()).map(|text| Claim {
            text,
            by: "author".to_owned(),
        }),
    }
}

/// The first `n` lines.
#[must_use]
pub fn cut(text: &str, n: usize) -> String {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .collect::<Vec<_>>()
        .join("\n")
}
