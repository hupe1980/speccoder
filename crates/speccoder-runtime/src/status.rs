//! `speccoder report`: a feature's tasks and the evidence behind
//! each tick, read from what is durable and reviewable — `tasks.md` at the
//! tip of `speccoder/<feature>` and the trailers of its commits. Read-only:
//! no model, no worktree, no state directory.

use std::collections::BTreeMap;
use std::path::Path;

use speccoder_core::snapshot::{ParsedTasks, TaskState};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ConvergenceRound, StatusReport, StatusTotals,
    TaskStatus, TraceRow,
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
    /// The task's line, unticked, when the commit was made in place.
    task_line: String,
    /// The intent it was judged against, when made in place.
    intent: String,
    /// `Acceptance-Strength:` (F-060).
    strength: String,
}

/// Reads the trailers speccoder writes (`Spec-Task`, `Verdict`, `Run`) and
/// its convergence commits from `log` (newest first): the latest commit per
/// task wins; rounds come back oldest first.
fn read_log(log: &str, feature: &str) -> (BTreeMap<String, Evidence>, Vec<ConvergenceRound>) {
    let mut evidence: BTreeMap<String, Evidence> = BTreeMap::new();
    let mut rounds = Vec::new();
    for record in log.split(RS) {
        let fields: Vec<&str> = record.trim_start_matches('\n').split(FS).collect();
        let [
            commit,
            subject,
            task,
            verdict,
            run,
            reason,
            task_line,
            intent,
            strength,
            body,
        ] = fields[..]
        else {
            continue;
        };
        if let Some(task) = task.trim().strip_prefix(&format!("{feature}/")) {
            evidence.entry(task.to_owned()).or_insert(Evidence {
                commit: commit.to_owned(),
                verdict: verdict.trim().to_owned(),
                run: run.trim().to_owned(),
                reason: reason.trim().to_owned(),
                task_line: task_line.trim().to_owned(),
                intent: intent.trim().trim_start_matches("sha256:").to_owned(),
                strength: strength.trim().to_owned(),
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
/// `lines` (the open-line hash per task, for local ticks) decides whether a
/// commit naming a `Task-Line` still matches the task as written.
fn statuses(
    parsed: &ParsedTasks,
    evidence: &BTreeMap<String, Evidence>,
    lines: &BTreeMap<String, String>,
    intent_now: Option<&str>,
    saved: &[crate::attempts::Saved],
    skipped: &BTreeMap<String, String>,
) -> Vec<TaskStatus> {
    let (schedule, _) = speccoder_core::graph::schedule(parsed);
    parsed
        .tasks
        .iter()
        .map(|t| {
            let scheduled = schedule.tasks.iter().find(|s| s.id == t.id);
            let found = evidence.get(&t.id).filter(|e| {
                e.task_line.is_empty() || lines.get(&t.id).is_none_or(|h| *h == e.task_line)
            });
            // In place: a commit for exactly this line, the tick not written.
            let untick = found.filter(|e| !t.done && !e.task_line.is_empty() && !lines.is_empty());
            let state = if untick.is_some() {
                "evidence_without_tick".to_owned()
            } else if t.done {
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
                commit: found
                    .filter(|_| t.done || untick.is_some())
                    .map(|e| e.commit.clone()),
                attestation: None,
                strength: found
                    .filter(|e| t.done && !e.strength.is_empty())
                    .map(|e| e.strength.clone()),
                intent_changed: found.is_some_and(|e| {
                    !e.intent.is_empty() && intent_now.is_some_and(|now| now != e.intent)
                }),
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
        evidence_without_tick: n("evidence_without_tick"),
        without_attestation: tasks
            .iter()
            .filter(|t| t.attestation.as_deref() == Some("missing"))
            .count(),
        attested: n("attested"),
        skipped: n("skipped"),
        ready: n("ready"),
        blocked: n("blocked"),
    }
}

/// `speccoder report [--feature]`.
#[must_use]
pub fn report(path: &Path, feature: Option<&str>) -> CommandResult {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return CommandResult::carried("report", &inspected);
    };
    let format = crate::format::of(snapshot);
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
            &format!(
                "{} has no {} yet",
                f.feature.name,
                format.messages().tasks_file
            ),
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
        // In place, an ignored task list is never committed: its ticks are
        // the checkout's, checked against the commits' `Task-Line`.
        let local = text.is_err()
            && !git
                .succeeds(&["ls-files", "--error-unmatch", "--", &repo_path])
                .unwrap_or(false);
        let parsed = match text {
            Ok(text) => format.parse_tasks(&checkout_tasks.path, &text).0,
            Err(_) if local => checkout_tasks.clone(),
            Err(_) => {
                return CommandResult::new(
                    "report",
                    CommandStatus::GitPrecondition,
                    &format!("{branch} has no {repo_path}"),
                );
            }
        };
        let format = format!(
            "--format=%H{FS}%s{FS}%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Verdict,valueonly){FS}%(trailers:key=Run,valueonly){FS}%(trailers:key=Reason,valueonly){FS}%(trailers:key=Task-Line,valueonly){FS}%(trailers:key=Intent-Digest,valueonly){FS}%(trailers:key=Acceptance-Strength,valueonly){FS}%b{RS}"
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
        (
            parsed,
            if local { "checkout" } else { "branch" },
            log,
            unmerged,
        )
    } else {
        (checkout_tasks.clone(), "checkout", String::new(), 0)
    };
    let local_ticks = has_branch && source == "checkout";
    let lines = if local_ticks {
        open_lines(&root.join(&checkout_tasks.path), &parsed)
    } else {
        BTreeMap::new()
    };
    let intent_now = local_ticks.then(|| {
        crate::in_place::intent_digest(
            root,
            &crate::tools::policy::Policy::new(&[], &[]).rules(format.rules()),
            format.messages().tasks_file,
        )
    });

    let (evidence, convergence) = read_log(&log, &name);
    let saved = crate::attempts::list(root, &name);
    let skipped = crate::controls::skipped(root, &name);
    let mut tasks = statuses(
        &parsed,
        &evidence,
        &lines,
        intent_now.as_deref(),
        &saved,
        &skipped,
    );
    // What each committed tick's note says (signatures are `attest verify`'s).
    for t in tasks.iter_mut() {
        if let Some(commit) = t.commit.as_deref() {
            t.attestation = Some(crate::attest::note_state(root, commit));
        }
    }
    let trace = trace(&format.requirement_ids(root, &f.feature.dir), &tasks);
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
    let uncited = trace.iter().filter(|r| r.task.is_none()).count();
    if uncited > 0 {
        message.push_str(&format!("; {uncited} requirement(s) no task cites"));
    }
    if totals.without_attestation > 0 {
        message.push_str(&format!(
            "; {} tick(s) without an attestation",
            totals.without_attestation
        ));
    }
    if totals.evidence_without_tick > 0 {
        message.push_str(&format!(
            "; {} committed without their local tick (the next `run --in-place` ticks them)",
            totals.evidence_without_tick
        ));
    }
    let changed = tasks.iter().filter(|t| t.intent_changed).count();
    if changed > 0 {
        message.push_str(&format!(
            "; {changed} judged against an earlier spec (it changed since)"
        ));
    }
    if local_ticks {
        message.push_str(&format!(
            "; ticks are local to this checkout ({} is not in git)",
            format.messages().tasks_file
        ));
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
            selected_by: crate::current::reason_text(snapshot.selection.reason, format),
            unmerged_commits: unmerged,
            tasks,
            convergence,
            trace,
            totals,
        }),
    ))
}

/// `speccoder task show TASK`: the report narrowed to one task.
#[must_use]
pub fn show(path: &Path, feature: Option<&str>, task: &str) -> CommandResult {
    let full = report(path, feature);
    let Some(CommandData::Report(r)) = full.data() else {
        return CommandResult::carried("task show", &full);
    };
    let Some(t) = r.tasks.iter().find(|t| t.id == task) else {
        return CommandResult::new(
            "task show",
            CommandStatus::NotFound,
            &format!("{} has no task {task}", r.feature),
        )
        .with_code("NOT-FOUND")
        .with_fix(vec![format!("speccoder report --feature {}", r.feature)]);
    };
    let mut one = (**r).clone();
    one.tasks = vec![t.clone()];
    one.trace.retain(|row| row.task.as_deref() == Some(task));
    one.convergence.clear();
    one.totals = totals(&one.tasks);
    let message = format!("{task}: {}", t.state.replace('_', " "));
    CommandResult::ok("task show", &message).with_data(CommandData::Report(Box::new(one)))
}

/// One row per requirement and task citing it (by its identifier as a word
/// in the title); a requirement no task cites is a row of its own.
fn trace(requirements: &[(String, String)], tasks: &[TaskStatus]) -> Vec<TraceRow> {
    requirements
        .iter()
        .flat_map(|(id, _)| {
            let rows: Vec<TraceRow> = tasks
                .iter()
                .filter(|t| crate::attest::cites(&t.title, id))
                .map(|t| TraceRow {
                    requirement: id.clone(),
                    scenario: None,
                    test: None,
                    task: Some(t.id.clone()),
                    verdict: Some(t.state.clone()),
                })
                .collect();
            if rows.is_empty() {
                vec![TraceRow {
                    requirement: id.clone(),
                    scenario: None,
                    test: None,
                    task: None,
                    verdict: None,
                }]
            } else {
                rows
            }
        })
        .collect()
}

/// Each task's line with its box read as open, hashed: what an in-place
/// commit's `Task-Line` names.
fn open_lines(file: &Path, parsed: &ParsedTasks) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    parsed
        .tasks
        .iter()
        .filter_map(|t| {
            let line = lines.get(usize::try_from(t.line).ok()?.checked_sub(1)?)?;
            let open = crate::in_place::untick(line.as_bytes());
            Some((t.id.clone(), crate::integrity::sha256_hex(&open)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_requirement_no_task_cites_is_a_row_of_its_own() {
        let task = |id: &str, title: &str, state: &str| TaskStatus {
            id: id.to_owned(),
            title: title.to_owned(),
            phase: "Phase 3".to_owned(),
            state: state.to_owned(),
            intent_changed: false,
            blocked_by: Vec::new(),
            commit: None,
            run_id: None,
            last_attempt: None,
            note: None,
            attestation: None,
            strength: None,
        };
        let ids = |v: &[&str]| {
            v.iter()
                .map(|i| ((*i).to_owned(), String::new()))
                .collect::<Vec<_>>()
        };
        let tasks = [
            task("T001", "Parse the config (FR-001, FR-002)", "verified"),
            task("T002", "Render it per FR-002", "ready"),
            task("T003", "Unrelated FR-0010 work", "ready"),
        ];
        let rows = trace(&ids(&["FR-001", "FR-002", "FR-003"]), &tasks);
        let shown: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.requirement.as_str(),
                    r.task.as_deref(),
                    r.verdict.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("FR-001", Some("T001"), Some("verified")),
                ("FR-002", Some("T001"), Some("verified")),
                ("FR-002", Some("T002"), Some("ready")),
                ("FR-003", None, None),
            ]
        );
    }

    #[test]
    fn trailers_and_rounds_are_read_newest_first() {
        let rec = |c: &str, s: &str, t: &str, v: &str, r: &str, b: &str| {
            format!("{c}{FS}{s}{FS}{t}\n{FS}{v}\n{FS}{r}\n{FS}{FS}{FS}{FS}{FS}{b}{RS}\n")
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
