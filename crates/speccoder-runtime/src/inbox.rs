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
/// evidence, what stops a task, what blocks a hand-off — derived from the
/// one registry, `speccoder_core::inbox::Kind`.
pub const KINDS: [&str; 18] = [
    Kind::ALL[0].as_str(),
    Kind::ALL[1].as_str(),
    Kind::ALL[2].as_str(),
    Kind::ALL[3].as_str(),
    Kind::ALL[4].as_str(),
    Kind::ALL[5].as_str(),
    Kind::ALL[6].as_str(),
    Kind::ALL[7].as_str(),
    Kind::ALL[8].as_str(),
    Kind::ALL[9].as_str(),
    Kind::ALL[10].as_str(),
    Kind::ALL[11].as_str(),
    Kind::ALL[12].as_str(),
    Kind::ALL[13].as_str(),
    Kind::ALL[14].as_str(),
    Kind::ALL[15].as_str(),
    Kind::ALL[16].as_str(),
    Kind::ALL[17].as_str(),
];

use speccoder_core::inbox::Kind;

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
    // Bugs of the format's own bug structure: no feature lists them.
    if only.is_none()
        && let Some(s) = state
        && let Some(project) = crate::format::at(path).detect(path)
    {
        let plane = crate::worktree::project_state_dir(s, &project);
        items.extend(
            crate::bug::state::all(&plane)
                .iter()
                .filter(|b| b.home.kind.native())
                .filter_map(|b| bug_item(b, &|c| c)),
        );
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

/// A kind counted in words: `2 approvals`, `1 open question`.
fn counted(kind: &str, n: usize) -> String {
    let (one, many) = match kind {
        "unknown" => (
            "feature whose waiting calls could not be read",
            "features whose waiting calls could not be read",
        ),
        "approval" => ("approval", "approvals"),
        "approval_batch" => (
            "run whose calls wait together",
            "runs whose calls wait together",
        ),
        "ruling" => ("ruling", "rulings"),
        "gaming" => ("suspected shortcut", "suspected shortcuts"),
        "stale" => (
            "tick the spec changed under",
            "ticks the spec changed under",
        ),
        "question" => ("open question", "open questions"),
        "review" => ("spec to review", "specs to review"),
        "revise" => ("change request", "change requests"),
        "stuck" => ("task that did not pass", "tasks that did not pass"),
        "aside" => ("set-aside change", "set-aside changes"),
        "commit" => ("plan to commit", "plans to commit"),
        "handoff" => ("branch to hand off", "branches to hand off"),
        "bug_confirm" => (
            "bug reproduction to confirm",
            "bug reproductions to confirm",
        ),
        "bug_cannot_reproduce" => (
            "bug that could not be reproduced",
            "bugs that could not be reproduced",
        ),
        "bug_unchanged_not_holding" => (
            "bug whose Unchanged statement does not hold",
            "bugs whose Unchanged statements do not hold",
        ),
        "bug_repro_weak" => ("bug reproduction too weak", "bug reproductions too weak"),
        "bug_already_satisfied" => ("bug already fixed", "bugs already fixed"),
        k => (k, k),
    };
    count(n, one, many)
}

/// `3 decisions waiting in 2 features: 1 approval, 2 tasks that did not pass`.
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
            (n > 0).then(|| counted(k, n))
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
        state.and_then(
            |s| match crate::approvals::list_with(path, Some(feature), s).data() {
                Some(CommandData::Approvals(a)) => Some((**a).clone()),
                _ => None,
            },
        );
    let flag = |command: String| {
        if several {
            format!("{command} --feature {feature}")
        } else {
            command
        }
    };
    let item = |kind: &str, task: Option<&str>, title: String, detail: String, actions| InboxItem {
        kind: kind.to_owned(),
        group: Kind::named(kind)
            .map_or("blocked_on_you", |k| k.group().as_str())
            .to_owned(),
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

    // The journal could not be read: what waits there is unknown, and the
    // person is told so rather than shown an empty list.
    if let Some(why) = approvals.as_ref().and_then(|a| a.unreadable.as_ref()) {
        out.push(item(
            "unknown",
            None,
            format!("Calls waiting in {feature} could not be read"),
            format!("{why} — some may be waiting for you"),
            Vec::new(),
        ));
    }

    // A run that used its asking budget: its waiting calls are one item,
    // decided together.
    let batched_runs: Vec<String> = approvals
        .iter()
        .flat_map(|a| &a.batch)
        .map(|b| b.run.clone())
        .collect();
    for b in approvals.iter().flat_map(|a| &a.batch) {
        let calls: Vec<String> = b.groups.iter().flat_map(|g| g.calls.clone()).collect();
        let of_run: Vec<&speccoder_core::PendingCall> = approvals
            .iter()
            .flat_map(|a| &a.pending)
            .filter(|p| p.run.as_deref() == Some(b.run.as_str()))
            .collect();
        let detail: Vec<String> = b
            .groups
            .iter()
            .map(|g| {
                let shown: Vec<String> = of_run
                    .iter()
                    .filter(|p| p.object.as_ref().map_or("write", |o| o.class.as_str()) == g.class)
                    .map(|p| {
                        let what = p
                            .object
                            .as_ref()
                            .map_or(p.covers.clone(), |o| o.short.clone());
                        let risk = p.presentation.as_ref().map_or("", |v| v.risk.as_str());
                        let copied = if p.sources.is_empty() {
                            ""
                        } else {
                            ", copied from content"
                        };
                        format!("{what} ({}, {risk}{copied})", p.task)
                    })
                    .collect();
                format!("{}: {}", g.class, shown.join("; "))
            })
            .collect();
        let decide = |verdict, resume_asking| Runnable::DecideBatch {
            run: b.run.clone(),
            verdict,
            calls: calls.clone(),
            digest: b.digest.clone(),
            resume_asking,
        };
        let command = |verb: &str| {
            flag(format!(
                "speccoder approvals batch {} {verb} --digest {}",
                b.run, b.digest
            ))
        };
        let yes = action(
            "next",
            format!("approve the {} waiting call(s) once", calls.len()),
            "each listed call runs once; the run keeps waiting at its next call",
            command("approve"),
        )
        .with_run(decide(
            speccoder_core::actions::BatchVerdict::Approve,
            false,
        ));
        let again = action(
            "next",
            "approve once and resume asking",
            "each listed call runs once, and the run prompts you again",
            format!("{} --resume-asking", command("approve")),
        )
        .with_run(decide(speccoder_core::actions::BatchVerdict::Approve, true));
        let mut no = action(
            "next",
            format!("reject the {} waiting call(s)", calls.len()),
            "the model is told your reason for each",
            format!("{} --reason \"…\"", command("reject")),
        )
        .with_run(decide(speccoder_core::actions::BatchVerdict::Reject, false));
        no.input = Some("Why (the model is told)".to_owned());
        let mut i = item(
            "approval_batch",
            None,
            format!(
                "{} call(s) wait for you in run {} (its approval budget is used)",
                calls.len(),
                b.run
            ),
            detail.join(" · "),
            vec![yes, again, no],
        );
        i.since = of_run.iter().filter_map(|p| p.waiting_since).min();
        out.push(i);
    }

    // A run waits on these.
    for p in approvals
        .iter()
        .flat_map(|a| &a.pending)
        .filter(|p| p.run.as_ref().is_none_or(|r| !batched_runs.contains(r)))
    {
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
        yes = yes.with_run(Runnable::Approve {
            task: p.task.clone(),
            call: p.id(),
            digest: p.digest.clone(),
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
        no = no.with_run(Runnable::Reject {
            task: p.task.clone(),
            call: p.id(),
            digest: p.digest.clone(),
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
        if let Some(i) = project_state
            .as_deref()
            .and_then(|s| crate::bug::state::load(s, feature, &t.id))
            .and_then(|b| bug_item(&b, &flag))
        {
            out.push(i);
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
            rule = rule.with_run(Runnable::Rule { task: t.id.clone() });
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
            again = again.with_run(Runnable::Run {
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

    // Changes a run found and set aside: kept on the branch, or let go.
    for t in &r.tasks {
        let Some(a) = &t.set_aside else { continue };
        let n = a.files.len();
        let mut keep = action(
            "task",
            format!("keep the changes set aside before {}", t.id),
            "committed onto the feature's branch as yours",
            flag(format!("speccoder task restore {}", t.id)),
        );
        keep = keep.with_run(Runnable::KeepSetAside { task: t.id.clone() });
        let mut drop = action(
            "task",
            format!("discard the changes set aside before {}", t.id),
            "deleted; the branch stays as it is",
            flag(format!("speccoder task drop {}", t.id)),
        );
        drop = drop.with_run(Runnable::DropSetAside { task: t.id.clone() });
        drop.confirm = Some(format!(
            "Discard the {n} changed file(s) set aside before {}? They are deleted, not kept anywhere.",
            t.id
        ));
        out.push(item(
            "aside",
            Some(&t.id),
            format!("Changes were set aside before {} ran", t.id),
            format!(
                "{n} file(s) were changed in the worktree when the run started — your edits, or an interrupted run's: {}",
                a.files.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
                    + if n > 4 { ", …" } else { "" }
            ),
            vec![keep, drop],
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
        a = a.with_run(Runnable::Converge);
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
                a = a
                    .with_run(Runnable::Skill {
                        name: skill.clone(),
                        arguments: String::new(),
                    })
                    .with_role(crate::format::at(path).planning().role_of(skill));
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
            all = all.with_run(Runnable::Review {
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
                a = a
                    .with_run(Runnable::Skill {
                        name: skill.clone(),
                        arguments: rv.revision.clone().unwrap_or_default(),
                    })
                    .with_role(crate::format::at(path).planning().role_of(skill));
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
        a = a.with_run(Runnable::Commit {
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
        pr = pr.with_run(Runnable::PullRequest);
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
    a = a.with_run(run);
    a.input = Some(prompt.to_owned());
    a
}

/// The Needs You item of a bug that stopped for the person, if it did.
fn bug_item(
    b: &speccoder_core::bug::BugReport,
    flag: &dyn Fn(String) -> String,
) -> Option<InboxItem> {
    use speccoder_core::bug::Outcome;
    let key = b.key();
    let kind = match b.outcome {
        Outcome::AwaitingConfirmation => Kind::BugConfirm,
        Outcome::CannotReproduce => Kind::BugCannotReproduce,
        Outcome::UnchangedNotHolding => Kind::BugUnchangedNotHolding,
        Outcome::AlreadySatisfied => Kind::BugAlreadySatisfied,
        _ if b.code.as_deref() == Some("BUG-REPRO-WEAK") => Kind::BugReproWeak,
        _ => return None,
    };
    let task = || b.task.clone();
    let mut actions = Vec::new();
    match kind {
        Kind::BugConfirm => {
            let digest = b.confirm_digest.clone().unwrap_or_default();
            let mut yes = action(
                "next",
                format!("confirm {}'s reproductions", b.task),
                "they show the bug: they are frozen and the fix runs",
                format!("speccoder bug confirm {key} --digest {digest}"),
            );
            yes = yes.with_run(Runnable::BugConfirm {
                task: key.clone(),
                digest: digest.clone(),
                reject: false,
            });
            let mut no = action(
                "next",
                format!("reject {}'s reproductions", b.task),
                "the author tries again, told why",
                format!("speccoder bug confirm {key} --reject --reason \"…\""),
            );
            no = no.with_run(Runnable::BugConfirm {
                task: key.clone(),
                digest,
                reject: true,
            });
            no.input = Some("Why they do not show the bug".to_owned());
            actions.extend([yes, no]);
        }
        Kind::BugAlreadySatisfied => {
            let mut keep = action(
                "next",
                format!("keep {}'s reproduction as a regression test", b.task),
                "accepted on your word: attested, not verified",
                flag(format!(
                    "speccoder task accept {} --reason \"kept as a regression test\"",
                    b.task
                )),
            );
            keep = keep.with_run(Runnable::Accept { task: task() });
            keep.input = Some("Why it is kept".to_owned());
            let mut drop = action(
                "task",
                format!("drop {}", b.task),
                "it is fixed already",
                flag(format!("speccoder task skip {} --reason \"…\"", b.task)),
            );
            drop = drop.with_run(Runnable::Skip { task: task() });
            drop.input = Some("Why it is dropped".to_owned());
            actions.extend([keep, drop]);
        }
        Kind::BugReproWeak => {
            let mut rule = action(
                "task",
                format!("rule on {}", b.task),
                "the tests are right: the special case is not a fix",
                flag(format!(
                    "speccoder task rule {} --tests-right --reason \"…\"",
                    b.task
                )),
            );
            rule = rule.with_run(Runnable::Rule { task: task() });
            rule.input = Some("Why the tests are right".to_owned());
            actions.push(rule);
            actions.push(action(
                "task",
                "bring your own test",
                "one per Expected statement",
                format!(
                    "speccoder bug \"{}\" --expected \"…\" --test <file>::<test>",
                    b.triad.current.text
                ),
            ));
        }
        _ => {
            let mut again = action(
                "next",
                format!("reproduce {} again", b.task),
                "a fresh set of tries",
                format!("speccoder bug run --task {key}"),
            );
            again = again.with_run(Runnable::BugRun {
                slug: None,
                task: Some(key.clone()),
            });
            actions.push(again);
            actions.push(action(
                "task",
                "bring your own test",
                "one per Expected statement",
                format!(
                    "speccoder bug \"{}\" --expected \"…\" --test <file>::<test>",
                    b.triad.current.text
                ),
            ));
            if !b.home.kind.native() {
                actions.push(action(
                    "task",
                    format!("reword {}", b.requirement),
                    "the triad in other words (what was confirmed lapses)",
                    format!(
                        "speccoder review edit {} \"…\" --feature {}",
                        b.requirement, b.unit
                    ),
                ));
                let mut drop = action(
                    "task",
                    format!("drop {}", b.task),
                    "nothing is fixed",
                    flag(format!("speccoder task skip {} --reason \"…\"", b.task)),
                );
                drop = drop.with_run(Runnable::Skip { task: task() });
                drop.input = Some("Why it is dropped".to_owned());
                actions.push(drop);
            }
        }
    }
    let detail = b
        .handoff
        .as_ref()
        .map(|h| {
            format!(
                "{} tries; reproduced: {}; not reproduced: {}; already passing: {}",
                h.tries.len(),
                if h.statements.reproduced.is_empty() {
                    "none".to_owned()
                } else {
                    h.statements.reproduced.join(", ")
                },
                if h.statements.not_reproduced.is_empty() {
                    "none".to_owned()
                } else {
                    h.statements.not_reproduced.join(", ")
                },
                if h.statements.satisfied.is_empty() {
                    "none".to_owned()
                } else {
                    h.statements.satisfied.join(", ")
                },
            )
        })
        .unwrap_or_else(|| format!("{} — {}", b.title, b.outcome.as_str().replace('_', " ")));
    Some(InboxItem {
        kind: kind.as_str().to_owned(),
        group: kind.group().as_str().to_owned(),
        feature: b.unit.clone(),
        task: Some(b.task.clone()),
        title: match kind {
            Kind::BugConfirm => format!(
                "{}: {} reproduction(s) wait for your confirmation",
                b.task,
                b.reproductions.iter().filter(|r| r.reproduces()).count()
            ),
            Kind::BugAlreadySatisfied => format!(
                "{}: already fixed — keep the reproduction or drop it",
                b.task
            ),
            Kind::BugReproWeak => {
                format!("{}: a special-case fix passes the reproductions", b.task)
            }
            Kind::BugUnchangedNotHolding => {
                format!("{}: an Unchanged statement does not hold today", b.task)
            }
            _ => format!("{}: could not be reproduced", b.task),
        },
        detail,
        since: b.confirmation.as_ref().map(|c| c.at),
        actions,
    })
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
