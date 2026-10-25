//! GitHub through a scripted `gh` (F-083): readiness and its reasons, issues
//! (assigned first, then the freshest), one issue read in full, a pull
//! request's state and checks.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_runtime::github;

/// A `gh` that answers from a table of argument lines; anything else fails.
fn fake(dir: &Path, cases: &[(&str, &str)], otherwise: &str) -> PathBuf {
    let mut script = String::from("#!/bin/sh\ncase \"$*\" in\n");
    for (args, out) in cases {
        script.push_str(&format!("  \"{args}\") cat <<'JSON'\n{out}\nJSON\n  ;;\n"));
    }
    script.push_str(&format!("  *) echo \"{otherwise}\" >&2; exit 1;;\nesac\n"));
    let gh = dir.join("gh");
    fs::write(&gh, script).unwrap();
    assert!(
        Command::new("chmod")
            .arg("+x")
            .arg(&gh)
            .status()
            .unwrap()
            .success()
    );
    gh
}

const ISSUES: &str = r#"[
 {"number":7,"title":"Old","url":"https://github.com/o/r/issues/7","labels":[],"assignees":[],"updatedAt":"2026-10-01T00:00:00Z"},
 {"number":42,"title":"Greet people","url":"https://github.com/o/r/issues/42","labels":[{"name":"feature"}],"assignees":[{"login":"me"}],"updatedAt":"2026-09-01T00:00:00Z"},
 {"number":9,"title":"Fresh","url":"https://github.com/o/r/issues/9","labels":[],"assignees":[{"login":"other"}],"updatedAt":"2026-10-20T00:00:00Z"}
]"#;

#[test]
fn issues_come_assigned_first_then_freshest_and_one_reads_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let gh = fake(
        dir.path(),
        &[
            ("--version", "gh version 2.80.0"),
            (
                "repo view --json nameWithOwner,url",
                r#"{"nameWithOwner":"o/r","url":"https://github.com/o/r"}"#,
            ),
            (
                "repo view --json nameWithOwner",
                r#"{"nameWithOwner":"o/r"}"#,
            ),
            ("api user --jq .login", "me"),
            (
                "issue list --state open --limit 100 --json number,title,url,labels,assignees,updatedAt",
                ISSUES,
            ),
            (
                "issue view 42 --json number,title,body,url,labels",
                r#"{"number":42,"title":"Greet people","body":"By name, politely.","url":"https://github.com/o/r/issues/42","labels":[{"name":"feature"}]}"#,
            ),
        ],
        "unexpected",
    );
    let gh = gh.display().to_string();
    let s = github::status(&gh, dir.path());
    assert!(s.ready && s.repo.as_deref() == Some("o/r"), "{s:?}");

    let started = std::collections::BTreeMap::from([(
        "https://github.com/o/r/issues/9".to_owned(),
        "003-fresh".to_owned(),
    )]);
    let r = github::issues(&gh, dir.path(), &started).unwrap();
    let order: Vec<u64> = r.issues.iter().map(|i| i.number).collect();
    assert_eq!(
        order,
        [42, 9, 7],
        "assigned first, then the most recently updated"
    );
    assert!(r.issues[0].mine && r.issues[0].labels == ["feature"]);
    assert_eq!(r.issues[1].feature.as_deref(), Some("003-fresh"));
    assert_eq!(r.repo.as_deref(), Some("o/r"));

    for reference in ["42", "#42"] {
        let i = github::issue(&gh, dir.path(), reference).unwrap();
        let d = github::description(&i);
        assert!(d.starts_with("Greet people\n\nBy name, politely."), "{d}");
        assert!(
            d.ends_with("GitHub issue: https://github.com/o/r/issues/42"),
            "{d}"
        );
    }
    // Not a reference: refused before `gh` runs.
    for bad in [
        "",
        "abc",
        "--web",
        "42; rm -rf /",
        "https://github.com/o/r/pull/3",
    ] {
        assert!(github::issue(&gh, dir.path(), bad).is_err(), "{bad}");
    }
}

#[test]
fn a_missing_gh_or_login_is_explained() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-gh").display().to_string();
    let s = github::status(&missing, dir.path());
    assert!(
        !s.installed && s.reason.as_deref().unwrap_or("").contains("not installed"),
        "{s:?}"
    );

    let gh = fake(
        dir.path(),
        &[("--version", "gh version 2.80.0")],
        "To get started with GitHub CLI, please run:  gh auth login",
    );
    let s = github::status(&gh.display().to_string(), dir.path());
    assert!(s.installed && !s.ready, "{s:?}");
    assert_eq!(s.reason.as_deref(), Some("not logged in: `gh auth login`"));
    let refused = github::refused(s.reason.as_deref().unwrap());
    assert_eq!(refused.code().map(|c| c.name), Some("GH-AUTH"));
    assert_eq!(refused.fix(), ["gh auth login"]);
}

#[test]
fn a_pull_request_reads_its_state_review_and_checks() {
    let dir = tempfile::tempdir().unwrap();
    let pr = r#"{"number":5,"title":"speccoder: 001-graph","url":"https://github.com/o/r/pull/5","state":"OPEN","isDraft":true,"reviewDecision":"CHANGES_REQUESTED",
      "statusCheckRollup":[{"status":"COMPLETED","conclusion":"SUCCESS"},{"status":"COMPLETED","conclusion":"FAILURE"},{"status":"IN_PROGRESS","conclusion":""},{"state":"SUCCESS"},{"state":"PENDING"}]}"#;
    let gh = fake(
        dir.path(),
        &[(
            "pr view speccoder/001-graph --json number,title,url,state,isDraft,reviewDecision,statusCheckRollup",
            pr,
        )],
        "no pull requests found for branch \"speccoder/002-x\"",
    );
    let gh = gh.display().to_string();
    let p = github::pr_for(&gh, dir.path(), "speccoder/001-graph")
        .unwrap()
        .unwrap();
    assert_eq!((p.number, p.state.as_str(), p.draft), (5, "OPEN", true));
    assert_eq!(p.review.as_deref(), Some("CHANGES_REQUESTED"));
    assert_eq!(p.checks, (2, 1, 2), "passed, failed, running");
    assert!(
        github::pr_for(&gh, dir.path(), "speccoder/002-x")
            .unwrap()
            .is_none()
    );
}
