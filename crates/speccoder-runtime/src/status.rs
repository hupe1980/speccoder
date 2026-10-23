//! `speccoder report`: a feature's tasks and the evidence behind
//! each tick, read from what is durable and reviewable — `tasks.md` at the
//! tip of `speccoder/<feature>` and the trailers of its commits. Read-only:
//! no model, no worktree, no state directory.

use std::collections::BTreeMap;
use std::path::Path;

use speccoder_core::snapshot::{ParsedTasks, TaskState};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ConvergenceRound, StatusReport, StatusTotals,
    TaskStatus,
};

use crate::git::Git;

/// Field and record separators for `git log` output.
const FS: char = '\u{1f}';
const RS: char = '\u{1e}';

/// What a speccoder commit says about a task.
#[derive(Debug, Clone)]
struct Evidence {
    commit: String,
    verdict: String,
    run: String,
    reason: String,
}

/// Reads the trailers speccoder writes (`Spec-Task`, `Verdict`, `Run`) and
/// its convergence commits from `log` (newest first): the latest commit per
/// task wins; rounds come back oldest first.
fn read_log(log: &str, feature: &str) -> (BTreeMap<String, Evidence>, Vec<ConvergenceRound>) {
    let mut evidence: BTreeMap<String, Evidence> = BTreeMap::new();
    let mut rounds = Vec::new();
    for record in log.split(RS) {
        let fields: Vec<&str> = record.trim_start_matches('\n').split(FS).collect();
        let [commit, subject, task, verdict, run, reason, body] = fields[..] else {
            continue;
        };
        if let Some(task) = task.trim().strip_prefix(&format!("{feature}/")) {
            evidence.entry(task.to_owned()).or_insert(Evidence {
                commit: commit.to_owned(),
                verdict: verdict.trim().to_owned(),
                run: run.trim().to_owned(),
                reason: reason.trim().to_owned(),
            });
        } else if let Some(n) = subject
            .strip_prefix("speccoder: converge round ")
            .and_then(|r| r.split(' ').next())
            .and_then(|n| n.parse::<u32>().ok())
        {
            rounds.push(ConvergenceRound {
                round: n,
                commit: commit.to_owned(),
                tasks: body
                    .lines()
                    .filter(|l| {
                        l.strip_prefix('T')
                            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
                    })
                    .map(str::to_owned)
                    .collect(),
            });
        }
    }
    rounds.reverse();
    (evidence, rounds)
}

/// Each task's state: evidence for ticked tasks, the schedule for open ones.
fn statuses(
    parsed: &ParsedTasks,
    evidence: &BTreeMap<String, Evidence>,
    saved: &[crate::attempts::Saved],
    skipped: &BTreeMap<String, String>,
) -> Vec<TaskStatus> {
    let (schedule, _) = speccoder_core::graph::schedule(parsed);
    parsed
        .tasks
        .iter()
        .map(|t| {
            let scheduled = schedule.tasks.iter().find(|s| s.id == t.id);
            let found = evidence.get(&t.id);
            let state = if t.done {
                match found.map(|e| e.verdict.as_str()) {
                    Some(v @ ("verified" | "regression_only" | "attested")) => v.to_owned(),
                    _ => "ticked_without_evidence".to_owned(),
                }
            } else if skipped.contains_key(&t.id) {
                "skipped".to_owned()
            } else if scheduled.is_some_and(|s| s.state == TaskState::Ready) {
                "ready".to_owned()
            } else {
                "blocked".to_owned()
            };
            TaskStatus {
                id: t.id.clone(),
                title: t.title.clone(),
                phase: t.phase.clone().unwrap_or_default(),
                blocked_by: if t.done {
                    Vec::new()
                } else {
                    scheduled.map(|s| s.blocked_by.clone()).unwrap_or_default()
                },
                commit: found.filter(|_| t.done).map(|e| e.commit.clone()),
                run_id: found
                    .filter(|e| t.done && !e.run.is_empty())
                    .map(|e| e.run.clone()),
                // Newest first: the first saved attempt of an open task.
                last_attempt: saved.iter().find(|a| !t.done && a.task == t.id).map(|a| {
                    speccoder_core::LastAttempt {
                        outcome: a.outcome.clone(),
                        reason: a.reason.clone(),
                        run_id: a.run_id.clone(),
                        reference: a.reference.clone(),
                    }
                }),
                note: if t.done {
                    found.map(|e| e.reason.clone()).filter(|r| !r.is_empty())
                } else {
                    skipped.get(&t.id).cloned()
                },
                state,
            }
        })
        .collect()
}

fn totals(tasks: &[TaskStatus]) -> StatusTotals {
    let n = |s: &str| tasks.iter().filter(|t| t.state == s).count();
    StatusTotals {
        verified: n("verified"),
        regression_only: n("regression_only"),
        ticked_without_evidence: n("ticked_without_evidence"),
        attested: n("attested"),
        skipped: n("skipped"),
        ready: n("ready"),
        blocked: n("blocked"),
    }
}

/// `speccoder report [PATH] [--feature]`.
#[must_use]
pub fn report(path: &Path, feature: Option<&str>) -> CommandResult {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return CommandResult::new("report", inspected.status(), inspected.message());
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return CommandResult::new(
            "report",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        );
    };
    let Some(checkout_tasks) = f.tasks.as_ref() else {
        return CommandResult::new(
            "report",
            CommandStatus::NotFound,
            &format!("{} has no tasks.md yet", f.feature.name),
        );
    };
    let name = f.feature.name.clone();
    let branch = format!("speccoder/{name}");
    let root = Path::new(&snapshot.root);
    let git = Git::at(root);
    let has_branch = git
        .succeeds(&[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ])
        .unwrap_or(false);

    let (parsed, source, log, unmerged) = if has_branch {
        // tasks.md as committed at the branch tip, by its repository path.
        let top = git.toplevel().ok().flatten();
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let rel = top
            .as_deref()
            .and_then(|t| {
                canonical(root)
                    .strip_prefix(canonical(t))
                    .ok()
                    .map(Path::to_path_buf)
            })
            .unwrap_or_default();
        let repo_path = rel
            .join(&checkout_tasks.path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = git
            .run(&["show", &format!("{branch}:{repo_path}")])
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        let Ok(text) = text else {
            return CommandResult::new(
                "report",
                CommandStatus::GitPrecondition,
                &format!("{branch} has no {repo_path}"),
            );
        };
        let (parsed, _) = speccoder_speckit::tasks::parse(&checkout_tasks.path, &text);
        let format = format!(
            "--format=%H{FS}%s{FS}%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Verdict,valueonly){FS}%(trailers:key=Run,valueonly){FS}%(trailers:key=Reason,valueonly){FS}%b{RS}"
        );
        let log = git
            .run(&["log", &format, &branch])
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let unmerged = git
            .line(&["rev-list", "--count", &format!("HEAD..{branch}")])
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0);
        (parsed, "branch", log, unmerged)
    } else {
        (checkout_tasks.clone(), "checkout", String::new(), 0)
    };

    let (evidence, convergence) = read_log(&log, &name);
    let saved = crate::attempts::list(root, &name);
    let skipped = crate::controls::skipped(root, &name);
    let tasks = statuses(&parsed, &evidence, &saved, &skipped);
    let totals = totals(&tasks);
    let done =
        totals.verified + totals.regression_only + totals.attested + totals.ticked_without_evidence;
    let mut message = format!(
        "{done}/{} done: {} verified, {} regression only, {} attested, {} without evidence; {} ready, {} blocked, {} skipped",
        tasks.len(),
        totals.verified,
        totals.regression_only,
        totals.attested,
        totals.ticked_without_evidence,
        totals.ready,
        totals.blocked,
        totals.skipped
    );
    if !convergence.is_empty() {
        message.push_str(&format!("; {} convergence round(s)", convergence.len()));
    }
    if has_branch {
        if unmerged > 0 {
            message.push_str(&format!("; {unmerged} commit(s) on {branch} not merged"));
        }
    } else {
        message.push_str("; speccoder has not worked on this feature yet");
    }
    CommandResult::new("report", CommandStatus::Ok, &message).with_data(CommandData::Report(
        Box::new(StatusReport {
            feature: name,
            branch: has_branch.then_some(branch),
            source: source.to_owned(),
            unmerged_commits: unmerged,
            tasks,
            convergence,
            totals,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailers_and_rounds_are_read_newest_first() {
        let rec = |c: &str, s: &str, t: &str, v: &str, r: &str, b: &str| {
            format!("{c}{FS}{s}{FS}{t}\n{FS}{v}\n{FS}{r}\n{FS}{FS}{b}{RS}\n")
        };
        let log = [
            rec(
                "c4",
                "speccoder: T003 again",
                "f/T003",
                "verified",
                "run_b",
                "",
            ),
            rec(
                "c3",
                "speccoder: converge round 1 (1 task)",
                "",
                "",
                "",
                "T003 Fix it per FR-1 (missing)\n\nSpec-Feature: f\nRun: run_c\n",
            ),
            rec(
                "c2",
                "speccoder: T003 first",
                "f/T003",
                "regression_only",
                "run_a",
                "",
            ),
            rec("c1", "init", "", "", "", ""),
            rec("c0", "other feature", "g/T001", "verified", "x", ""),
        ]
        .concat();
        let (ev, rounds) = read_log(&log, "f");
        assert_eq!(ev.len(), 1);
        assert_eq!(
            (ev["T003"].commit.as_str(), ev["T003"].verdict.as_str()),
            ("c4", "verified")
        );
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].tasks, ["T003 Fix it per FR-1 (missing)"]);
    }
}
