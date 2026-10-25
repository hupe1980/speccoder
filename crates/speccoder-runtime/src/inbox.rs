//! `speccoder inbox`: every decision that waits for a person,
//! across the project's features, the most urgent first. When agents write
//! the code, this is the person's work: approve or refuse a call, rule on
//! tests, look at a suspected shortcut, settle a changed spec, decide what
//! to do with a task that did not pass, commit a plan, hand off a branch.
//! Read-only; every item carries the actions that decide it.

use std::path::Path;

use speccoder_core::actions::{Action, Runnable};
use speccoder_core::{
    ApprovalsReport, CommandData, CommandResult, CommandStatus, InboxItem, InboxReport,
    StatusReport,
};

use crate::actions::action;

/// The kinds, most urgent first: what blocks a run, what questions the
/// evidence, what stops a task, what blocks a hand-off.
pub const KINDS: [&str; 10] = [
    "approval", "ruling", "gaming", "stale", "question", "review", "revise", "stuck", "commit",
    "handoff",
];

/// `speccoder inbox [--feature]`: the decisions waiting in every feature of
/// the project at `path`, or in `feature` only.
#[must_use]
pub fn inbox(path: &Path, feature: Option<&str>) -> CommandResult {
    inbox_with(path, feature, crate::worktree::state_dir().as_deref())
}

/// [`inbox`] with an explicit state directory (where waiting calls and
/// held attempts are kept).
#[must_use]
pub fn inbox_with(path: &Path, feature: Option<&str>, state: Option<&Path>) -> CommandResult {
    let listed = crate::current::list(path, None);
    let Some(CommandData::Features(f)) = listed.data() else {
        return CommandResult::carried("inbox", &listed);
    };
    // `--feature` takes a name or a number, as everywhere: the one it selects.
    let only = match feature {
        Some(_) => match crate::current::list(path, feature).data() {
            Some(CommandData::Features(one)) => one.current.clone(),
            _ => None,
        },
        None => None,
    };
    if feature.is_some() && only.is_none() {
        return CommandResult::carried("inbox", &crate::status::report(path, feature));
    }
    let names: Vec<String> = f
        .features
        .iter()
        .map(|e| e.name.clone())
        .filter(|n| only.as_ref().is_none_or(|o| o == n))
        .collect();
    let several = names.len() > 1;
    let mut items = Vec::new();
    for name in &names {
        items.extend(of_feature(path, name, state, several));
    }
    let rank = |k: &str| KINDS.iter().position(|x| *x == k).unwrap_or(KINDS.len());
    items.sort_by(|a, b| {
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then(
                a.since
                    .unwrap_or(u64::MAX)
                    .cmp(&b.since.unwrap_or(u64::MAX)),
            )
            .then(a.feature.cmp(&b.feature))
            .then(a.task.cmp(&b.task))
    });
    let message = summary(&items, names.len());
    CommandResult::new("inbox", CommandStatus::Ok, &message).with_data(CommandData::Inbox(
        Box::new(InboxReport {
            items,
            features: names.len(),
        }),
    ))
}

/// `n` with the word for one or for many: `1 tick`, `2 ticks`.
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `3 decisions waiting in 2 features: 1 approval, 2 stuck`.
fn summary(items: &[InboxItem], features: usize) -> String {
    if items.is_empty() {
        return format!(
            "nothing waits for you in {}: the agents can work",
            count(features, "feature", "features")
        );
    }
    let mut in_features: Vec<&str> = items.iter().map(|i| i.feature.as_str()).collect();
    in_features.sort_unstable();
    in_features.dedup();
    let parts: Vec<String> = KINDS
        .iter()
        .filter_map(|k| {
            let n = items.iter().filter(|i| i.kind == *k).count();
            (n > 0).then(|| format!("{n} {k}"))
        })
        .collect();
    format!(
        "{} waiting in {}: {}",
        count(items.len(), "decision", "decisions"),
        count(in_features.len(), "feature", "features"),
        parts.join(", ")
    )
}

/// One feature's items. `several`: commands name the feature.
fn of_feature(path: &Path, feature: &str, state: Option<&Path>, several: bool) -> Vec<InboxItem> {
    let report = crate::status::report(path, Some(feature));
    let Some(CommandData::Report(r)) = report.data() else {
        return Vec::new();
    };
    let approvals: Option<ApprovalsReport> =
        state.and_then(|s| {
            match crate::approvals::decide_with(
                path,
                Some(feature),
                crate::approvals::Verb::List,
                s,
            )
            .data()
            {
                Some(CommandData::Approvals(a)) => Some((**a).clone()),
                _ => None,
            }
        });
    let flag = |command: String| {
        if several {
            format!("{command} --feature {feature}")
        } else {
            command
        }
    };
    let item = |kind: &str, task: Option<&str>, title: String, detail: String, actions| InboxItem {
        kind: kind.to_owned(),
        feature: feature.to_owned(),
        task: task.map(str::to_owned),
        title,
        detail,
        since: None,
        actions,
    };
    let mut out = Vec::new();
    let waiting: Vec<&str> = approvals
        .iter()
        .flat_map(|a| &a.pending)
        .map(|p| p.task.as_str())
        .collect();

    // A run waits on these.
    for p in approvals.iter().flat_map(|a| &a.pending) {
        let question = p.kind == "question";
        let mut yes = action(
            "next",
            if question {
                format!("answer {}", p.task)
            } else {
                format!("approve {}", p.task)
            },
            if question {
                "the attempt resumes with your answer".to_owned()
            } else {
                format!("allow {} for the task", p.covers)
            },
            flag(format!("speccoder task approve {}", p.task)),
        );
        yes.run = Some(Runnable::Approve {
            task: p.task.clone(),
        });
        if question {
            yes.input = Some("Your answer".to_owned());
        }
        let mut no = action(
            "next",
            format!("reject {}", p.task),
            "the model is told your reason",
            flag(format!("speccoder task reject {} --reason \"…\"", p.task)),
        );
        no.run = Some(Runnable::Reject {
            task: p.task.clone(),
        });
        no.input = Some("Why (the model is told)".to_owned());
        let mut i = item(
            "approval",
            Some(&p.task),
            if question {
                format!("{} asks you", p.task)
            } else {
                format!("{} waits for your approval", p.task)
            },
            p.question
                .clone()
                .filter(|_| question)
                .unwrap_or_else(|| format!("{} — {}", p.covers, p.reason)),
            vec![yes, no],
        );
        i.since = p.waiting_since;
        out.push(i);
    }

    let root = Path::new(path);
    let project_state = state.and_then(|s| {
        crate::format::at(root)
            .detect(root)
            .map(|project| crate::worktree::project_state_dir(s, &project))
    });
    for t in &r.tasks {
        let done = matches!(
            t.state.as_str(),
            "verified" | "regression_only" | "attested" | "ticked_without_evidence" | "skipped"
        );
        if done || waiting.contains(&t.id.as_str()) {
            continue;
        }
        let held = project_state
            .as_deref()
            .and_then(|s| crate::strength::held(&crate::strength::dir(s, feature, &t.id)));
        if let Some(held) = held {
            let mut rule = action(
                "task",
                format!("rule on {}", t.id),
                "the tests are right: the flagged alternatives are not what the spec allows",
                flag(format!(
                    "speccoder task rule {} --tests-right --reason \"…\"",
                    t.id
                )),
            );
            rule.run = Some(Runnable::Rule { task: t.id.clone() });
            rule.input = Some("Why the tests are right".to_owned());
            let findings: Vec<String> = held
                .findings
                .iter()
                .map(|p| format!("{} ({})", p.scenario, p.rationale))
                .collect();
            out.push(item(
                "ruling",
                Some(&t.id),
                format!("{}'s tests reject an alternative the spec may allow", t.id),
                findings.join("; "),
                vec![rule, nudge(t, &flag), skip(t, &flag)],
            ));
            continue;
        }
        let Some(a) = &t.last_attempt else {
            continue;
        };
        if a.outcome == "suspected_gaming" {
            out.push(item(
                "gaming",
                Some(&t.id),
                format!("{}: the monitor suspects a shortcut", t.id),
                a.reason.clone(),
                vec![nudge(t, &flag), skip(t, &flag), accept(t, &flag)],
            ));
            continue;
        }
        let mut actions = Vec::new();
        if t.state == "ready" {
            let mut again = action(
                "work",
                format!("run {} again", t.id),
                "with what the last attempt left: its facts and your notes",
                flag(format!("speccoder run --task {}", t.id)),
            );
            again.run = Some(Runnable::Run {
                task: Some(t.id.clone()),
            });
            again.blocked = if !r.uncommitted.is_empty() {
                Some("commit the planning files first".to_owned())
            } else {
                // A waiting call's attempt holds the worktree.
                waiting.first().map(|w| {
                    format!("{w} waits for your decision and holds the worktree: decide it first")
                })
            };
            actions.push(again);
        }
        actions.extend([nudge(t, &flag), skip(t, &flag), accept(t, &flag)]);
        out.push(item(
            "stuck",
            Some(&t.id),
            format!("{}'s last attempt: {}", t.id, a.outcome.replace('_', " ")),
            a.reason.clone(),
            actions,
        ));
    }

    let stale: Vec<String> = r
        .tasks
        .iter()
        .filter_map(|t| {
            let d = t.drift.as_ref().filter(|d| d.stale())?;
            let ids: Vec<&str> = d.requirements.iter().map(|q| q.id.as_str()).collect();
            Some(format!("{} ({})", t.id, ids.join(", ")))
        })
        .collect();
    if !stale.is_empty() {
        let mut a = action(
            "next",
            "re-check against the spec",
            "convergence assesses the code against the spec as it is now",
            flag("speccoder run --converge".to_owned()),
        );
        a.run = Some(Runnable::Converge);
        out.push(item(
            "stale",
            None,
            format!(
                "the spec changed under {}",
                count(stale.len(), "tick", "ticks")
            ),
            format!("proven against an earlier wording: {}", stale.join(", ")),
            vec![a],
        ));
    }

    // The spec, reviewed requirement by requirement.
    if let Some(rv) = crate::review::summary(path, Some(feature)).as_ref() {
        let t = rv.totals;
        // The documents' open questions: answered before anything is built.
        if !rv.questions.is_empty() {
            let read = action(
                "plan",
                "answer the open questions",
                "your words replace each question where it stands",
                flag("speccoder review".to_owned()),
            );
            let mut actions = vec![read];
            if let Some(skill) = &rv.revise_with {
                let mut a = action(
                    "plan",
                    format!("clarify with {skill}"),
                    "the format's skill asks you the questions and writes the answers in",
                    flag(format!("speccoder spec {skill}")),
                );
                a.run = Some(Runnable::Skill {
                    name: skill.clone(),
                    arguments: String::new(),
                });
                actions.push(a);
            }
            out.push(item(
                "question",
                None,
                count(
                    rv.questions.len(),
                    "open question in the spec",
                    "open questions in the spec",
                ),
                rv.questions
                    .iter()
                    .take(3)
                    .map(|q| format!("{}: {}", q.id, q.text))
                    .collect::<Vec<_>>()
                    .join("; "),
                actions,
            ));
        }
        if t.open + t.lapsed > 0 {
            let pending: Vec<&str> = rv
                .requirements
                .iter()
                .filter(|i| i.state == "open" || i.state == "lapsed")
                .map(|i| i.id.as_str())
                .collect();
            let read = action(
                "plan",
                "read the requirements",
                "each one, as the result will be judged against it",
                flag("speccoder review".to_owned()),
            );
            let mut all = action(
                "plan",
                format!(
                    "approve {}",
                    count(pending.len(), "requirement", "requirements")
                ),
                "on their current wording, as you",
                flag("speccoder review approve".to_owned()),
            );
            all.run = Some(Runnable::Review {
                ids: Vec::new(),
                verdict: "approve".to_owned(),
            });
            out.push(item(
                "review",
                None,
                if pending.len() == 1 {
                    "1 requirement waits for your review".to_owned()
                } else {
                    format!("{} requirements wait for your review", pending.len())
                },
                if t.lapsed > 0 {
                    format!(
                        "{}; {} changed since you decided",
                        pending.join(", "),
                        t.lapsed
                    )
                } else {
                    pending.join(", ")
                },
                vec![read, all],
            ));
        }
        if t.changes_requested > 0 {
            let asked: Vec<String> = rv
                .requirements
                .iter()
                .filter(|i| i.state == "changes_requested")
                .map(|i| format!("{}: {}", i.id, i.comment.as_deref().unwrap_or_default()))
                .collect();
            let mut actions = Vec::new();
            if let Some(skill) = &rv.revise_with {
                let mut a = action(
                    "plan",
                    format!("revise the spec ({skill})"),
                    "the format's skill rewrites what you asked; you review the result",
                    flag(format!("speccoder spec {skill} \"…\"")),
                );
                a.run = Some(Runnable::Skill {
                    name: skill.clone(),
                    arguments: rv.revision.clone().unwrap_or_default(),
                });
                actions.push(a);
            }
            actions.push(action(
                "plan",
                "edit the spec yourself",
                "then approve the new wording",
                flag("speccoder review".to_owned()),
            ));
            out.push(item(
                "revise",
                None,
                format!(
                    "{} on the spec",
                    count(t.changes_requested, "change request", "change requests")
                ),
                asked.join("; "),
                actions,
            ));
        }
    }

    if !r.uncommitted.is_empty() {
        let mut a = action(
            "next",
            format!("commit the planning files ({})", r.uncommitted.len()),
            "a run reads committed files only",
            format!(
                "git add {0} && git commit -m \"spec: …\" -- {0}",
                r.uncommitted.join(" ")
            ),
        );
        a.run = Some(Runnable::Commit {
            paths: r.uncommitted.clone(),
        });
        out.push(item(
            "commit",
            None,
            "planning files not committed yet".to_owned(),
            format!("read them, then commit: {}", r.uncommitted.join(", ")),
            vec![a],
        ));
    }

    if handoff(r) && stale.is_empty() {
        let mut pr = action(
            "review",
            "open a draft pull request",
            "push the branch and its evidence; open (or update) the draft",
            flag("speccoder pr --yes".to_owned()),
        );
        pr.run = Some(Runnable::PullRequest);
        pr.confirm = Some(
            "Push the feature's speccoder branch and its attestation notes to `origin`, and open (or update) its draft pull request on GitHub, as you (your token).".to_owned(),
        );
        out.push(item(
            "handoff",
            None,
            format!("every task is done: review {} and hand it off", r.feature),
            format!(
                "{} on {} not merged",
                count(r.unmerged_commits, "commit", "commits"),
                r.branch.as_deref().unwrap_or_default()
            ),
            vec![pr],
        ));
    }
    out
}

/// Every task finished and the branch carries work not merged.
fn handoff(r: &StatusReport) -> bool {
    !r.tasks.is_empty()
        && r.totals.ready == 0
        && r.totals.blocked == 0
        && r.unmerged_commits > 0
        && r.branch.is_some()
}

fn with_input(
    t: &speccoder_core::TaskStatus,
    verb: &str,
    why: &str,
    prompt: &str,
    run: Runnable,
    flag: &dyn Fn(String) -> String,
) -> Action {
    let option = if verb == "nudge" {
        "--note"
    } else {
        "--reason"
    };
    let mut a = action(
        "task",
        format!("{verb} {}", t.id),
        why,
        flag(format!("speccoder task {verb} {} {option} \"…\"", t.id)),
    );
    a.run = Some(run);
    a.input = Some(prompt.to_owned());
    a
}

fn nudge(t: &speccoder_core::TaskStatus, flag: &dyn Fn(String) -> String) -> Action {
    with_input(
        t,
        "nudge",
        "a note its next attempts read",
        "The note",
        Runnable::Nudge { task: t.id.clone() },
        flag,
    )
}

fn skip(t: &speccoder_core::TaskStatus, flag: &dyn Fn(String) -> String) -> Action {
    with_input(
        t,
        "skip",
        "runs pass it by; what depends on it stays blocked",
        "Why it is skipped",
        Runnable::Skip { task: t.id.clone() },
        flag,
    )
}

fn accept(t: &speccoder_core::TaskStatus, flag: &dyn Fn(String) -> String) -> Action {
    with_input(
        t,
        "accept",
        "tick it on your word (attested, never shown as verified)",
        "Why it is done",
        Runnable::Accept { task: t.id.clone() },
        flag,
    )
}
