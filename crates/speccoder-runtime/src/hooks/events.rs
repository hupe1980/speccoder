//! `task.ticked` and `task.stopped`, read back from git: a tick commit on a
//! unit's branch and an attempt saved under `refs/speccoder/attempts/…`.
//! The same record gives the same event, with the same id, in any process;
//! the projection carries ids, counts and words speccoder wrote, never test
//! names, file contents, model words or tool arguments.

use std::path::Path;

use serde_json::{Value, json};
use speccoder_core::hooks::{
    Count, GateMark, GateState, TaskStopped, TaskTicked, stopped_summary, ticked_summary,
};

use crate::git::Git;

/// Where the published schemas live.
const SCHEMAS: &str = "https://hupe1980.github.io/speccoder/schemas/v2/events";

/// One event, ready to deliver.
#[derive(Debug, Clone)]
pub struct Event {
    /// `tick:<commit>` or `attempt:<commit>`.
    pub id: String,
    /// `task.ticked` or `task.stopped`.
    pub kind: &'static str,
    /// The CloudEvents 1.0 envelope (structured mode).
    pub envelope: Value,
}

/// A commit's message.
/// A commit's trailers only: free text above them (a reason, a model's
/// words) never becomes a field.
fn message(git: &Git, rev: &str) -> Option<String> {
    git.run(&[
        "log",
        "-1",
        "--format=%(trailers:only=true,unfold=true)",
        rev,
    ])
    .ok()
    .map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn trailer<'a>(message: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}: ");
    message
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .map(str::trim)
}

fn gates(message: &str) -> Vec<GateMark> {
    message
        .lines()
        .filter_map(|l| l.strip_prefix("Gate: "))
        .filter_map(|v| {
            let (name, outcome) = v.trim().rsplit_once(' ')?;
            Some(GateMark {
                name: name.to_owned(),
                outcome: match outcome {
                    "passed" => GateState::Passed,
                    "blocked" => GateState::Blocked,
                    _ => GateState::NotRun,
                },
            })
        })
        .collect()
}

fn spec_task(message: &str) -> Option<(String, String)> {
    let (feature, task) = trailer(message, "Spec-Task")?.split_once('/')?;
    Some((feature.to_owned(), task.to_owned()))
}

/// `task.ticked` for the tick commit `commit`, if it is one.
#[must_use]
pub fn ticked(git: &Git, commit: &str) -> Option<TaskTicked> {
    let m = message(git, commit)?;
    let (feature, task) = spec_task(&m)?;
    let verdict = trailer(&m, "Verdict")?.to_owned();
    let held_out = trailer(&m, "Held-Out")
        .and_then(|v| v.strip_prefix("passed="))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.split_once('/'))
        .and_then(|(p, t)| {
            Some(Count {
                passed: p.parse().ok()?,
                total: t.parse().ok()?,
            })
        });
    let scenarios = trailer(&m, "Scenarios")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().split('@').next().unwrap_or_default().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut t = TaskTicked {
        feature,
        task,
        verdict,
        commit: commit.to_owned(),
        scenarios,
        held_out,
        acceptance_strength: trailer(&m, "Acceptance-Strength")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok()),
        gates: gates(&m),
        executor: trailer(&m, "Executor").map(str::to_owned),
        run: trailer(&m, "Run").map(str::to_owned),
        summary: String::new(),
    };
    t.summary = ticked_summary(&t);
    Some(t)
}

/// The code a saved attempt's outcome stands for.
fn code_of(outcome: &str) -> Option<&'static str> {
    Some(match outcome {
        "verification_failed" => "VERIFICATION-FAILED",
        "needs_approval" => "NEEDS-APPROVAL",
        "blocked" => "BLOCKED",
        "incomplete" => "ATTEMPT-INCOMPLETE",
        "budget_exhausted" | "denied" => "BUDGET-EXHAUSTED",
        "suspected_gaming" => "SUSPECTED-GAMING",
        "acceptance_weak" => "ACCEPTANCE-WEAK",
        "blocked_by_gate" => "HOOK-BLOCKED",
        "interrupted" => "INTERRUPTED",
        _ => return None,
    })
}

/// `task.stopped` for the attempt saved under `reference` at `commit`.
#[must_use]
pub fn stopped(git: &Git, reference: &str, commit: &str) -> Option<TaskStopped> {
    let m = message(git, commit)?;
    let (feature, task) = spec_task(&m)?;
    let outcome = trailer(&m, "Outcome")?.to_owned();
    let mut s = TaskStopped {
        feature,
        task,
        code: code_of(&outcome).map(str::to_owned),
        outcome,
        attempt_ref: reference.to_owned(),
        gates: gates(&m),
        run: trailer(&m, "Run").map(str::to_owned),
        summary: String::new(),
    };
    s.summary = stopped_summary(&s);
    Some(s)
}

/// The CloudEvents 1.0 envelope of an event about `feature`'s `task`.
#[must_use]
pub fn envelope(
    project: &str,
    kind: &str,
    id: &str,
    feature: &str,
    task: &str,
    data: &Value,
) -> Value {
    json!({
        "specversion": "1.0",
        "id": id,
        "source": format!("speccoder://{project}/{feature}"),
        "type": format!("dev.speccoder.{kind}"),
        "subject": task,
        "time": crate::attest::now_rfc3339(),
        "datacontenttype": "application/json",
        "dataschema": format!("{SCHEMAS}/{kind}.json"),
        "data": data,
    })
}

/// A record's event, if `commit` on `reference` is one.
#[must_use]
pub fn of(git: &Git, project: &str, reference: &str, commit: &str) -> Option<Event> {
    if reference.starts_with(crate::attempts::PREFIX) {
        let s = stopped(git, reference, commit)?;
        let id = format!("attempt:{commit}");
        let data = serde_json::to_value(&s).ok()?;
        Some(Event {
            envelope: envelope(project, "task.stopped", &id, &s.feature, &s.task, &data),
            id,
            kind: "task.stopped",
        })
    } else {
        let t = ticked(git, commit)?;
        let id = format!("tick:{commit}");
        let data = serde_json::to_value(&t).ok()?;
        Some(Event {
            envelope: envelope(project, "task.ticked", &id, &t.feature, &t.task, &data),
            id,
            kind: "task.ticked",
        })
    }
}

/// The refs whose records are events: every unit's branch, the branch the
/// checkout is on (in-place ticks), and every saved attempt; each with its
/// commit.
#[must_use]
pub fn tips(root: &Path) -> Vec<(String, String)> {
    let git = Git::at(root);
    let mut out: Vec<(String, String)> = git
        .run(&[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads/speccoder/",
            crate::attempts::PREFIX,
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(r, c)| (r.to_owned(), c.to_owned()))
        .collect();
    if let (Ok(head), Ok(commit)) = (
        git.line(&["symbolic-ref", "-q", "HEAD"]),
        git.line(&["rev-parse", "HEAD"]),
    ) && !out.iter().any(|(r, _)| *r == head)
    {
        out.push((head.trim().to_owned(), commit.trim().to_owned()));
    }
    out
}

/// The tick commits in `old..new` on a branch, oldest first (all of `new`'s
/// history when `old` is gone or not its ancestor — a rewrite invents
/// nothing: it is the caller's to start such a branch at its tip).
#[must_use]
pub fn ticks_between(git: &Git, old: &str, new: &str) -> Vec<String> {
    git.run(&[
        "log",
        "--reverse",
        "--format=%H",
        "--grep=^Verdict: ",
        &format!("{old}..{new}"),
    ])
    .map(|b| String::from_utf8_lossy(&b).into_owned())
    .unwrap_or_default()
    .lines()
    .map(str::to_owned)
    .collect()
}

/// Whether `old` is an ancestor of `new`.
#[must_use]
pub fn descends(git: &Git, old: &str, new: &str) -> bool {
    git.succeeds(&["merge-base", "--is-ancestor", old, new])
        .unwrap_or(false)
}

/// A sample `task.ticked` marked `test: true`, for `speccoder hooks test`.
#[must_use]
pub fn sample(project: &str) -> Value {
    let mut t = TaskTicked {
        feature: "TEST".to_owned(),
        task: "TEST".to_owned(),
        verdict: "verified".to_owned(),
        commit: "0".repeat(40),
        scenarios: vec!["US1-1".to_owned()],
        held_out: None,
        acceptance_strength: None,
        gates: Vec::new(),
        executor: None,
        run: None,
        summary: String::new(),
    };
    t.summary = format!("A test from speccoder: {}", ticked_summary(&t));
    let data = serde_json::to_value(&t).unwrap_or_default();
    let mut e = envelope(project, "task.ticked", "test:sample", "TEST", "TEST", &data);
    e["test"] = json!(true);
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@x",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "{args:?}");
    }

    #[test]
    fn a_tick_commit_projects_ids_counts_and_a_sentence_only() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q"]);
        let msg = "T004 Add login\n\nAdd login in src/login.rs, asserting `secret_name()` returns 42\n\nSpec-Task: 001-md2csv/T004\nVerdict: verified\nHeld-Out: passed=2/2 manifest=sha256:ab\nScenarios: US1-1@aaa, US1-2@bbb\nAcceptance-Strength: 0.83 near-miss=1/1\nGate: licence passed\nRun: run_01\n";
        git(d.path(), &["commit", "-q", "--allow-empty", "-m", msg]);
        let g = Git::at(d.path());
        let head = g.line(&["rev-parse", "HEAD"]).unwrap();
        let head = head.trim();
        let e = of(&g, "9a3b", "refs/heads/speccoder/001-md2csv", head).unwrap();
        assert_eq!(e.id, format!("tick:{head}"));
        assert_eq!(e.envelope["type"], "dev.speccoder.task.ticked");
        assert_eq!(e.envelope["source"], "speccoder://9a3b/001-md2csv");
        assert_eq!(e.envelope["subject"], "T004");
        let data = &e.envelope["data"];
        assert_eq!(data["scenarios"], json!(["US1-1", "US1-2"]));
        assert_eq!(data["held_out"], json!({"passed": 2, "total": 2}));
        assert_eq!(data["gates"][0]["name"], "licence");
        assert_eq!(
            data["summary"],
            "T004 verified in 001-md2csv — 2 scenarios, held-out 2 of 2"
        );
        // The task's words, its test names and the model's never leave.
        let text = e.envelope.to_string();
        for leak in ["Add login", "secret_name", "src/login.rs"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }

    #[test]
    fn a_saved_attempt_is_a_stop_with_its_code() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q"]);
        let msg = "speccoder attempt: T005 blocked_by_gate\n\nBlocked by licence — GPL in foo\n\nSpec-Task: 001-md2csv/T005\nOutcome: blocked_by_gate\nRun: run_02\nGate: licence blocked\nEarned-Verdict: verified\n";
        git(d.path(), &["commit", "-q", "--allow-empty", "-m", msg]);
        let g = Git::at(d.path());
        let head = g.line(&["rev-parse", "HEAD"]).unwrap();
        let r = format!("{}/001-md2csv/T005/run_02", crate::attempts::PREFIX);
        let e = of(&g, "9a3b", &r, head.trim()).unwrap();
        assert_eq!(e.kind, "task.stopped");
        assert_eq!(e.envelope["data"]["code"], "HOOK-BLOCKED");
        assert_eq!(
            e.envelope["data"]["summary"],
            "T005 blocked by licence in 001-md2csv"
        );
        assert!(!e.envelope.to_string().contains("GPL in foo"));
    }
}
