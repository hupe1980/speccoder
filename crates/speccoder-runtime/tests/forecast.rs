//! The merge forecast: a feature's branch paired with each
//! sibling — conflicts from the merge, breaks from the frozen oracles on
//! the merged tree, honest reasons when nothing judged, and a second look
//! that runs nothing.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use speccoder_core::forecast::{ForecastReport, PairVerdict};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::forecast::{Options, last, run};

const FEATURE: &str = "001-graph";
const MINE: &str = "speccoder/001-graph";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=Dana",
            "-c",
            "user.email=d@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn sandboxed() -> bool {
    speccoder_runtime::sandbox::backend().is_ok()
}

fn sha(text: &str) -> String {
    format!(
        "sha256:{}",
        speccoder_runtime::integrity::sha256_hex(text.as_bytes())
    )
}

/// Freezes `unit`'s `story` oracle in the project's state: one test per
/// `(path, content, scenarios)`, each run by `sh <path>`.
fn freeze(state: &Path, repo: &Path, unit: &str, story: &str, tests: &[(&str, &str, &[&str])]) {
    let plane = speccoder_runtime::worktree::project_state_dir(state, repo);
    let dir = plane.join("oracle").join(unit).join(story);
    fs::create_dir_all(&dir).unwrap();
    let tests: Vec<_> = tests
        .iter()
        .map(|(p, c, s)| {
            serde_json::json!({ "path": p, "sha256": sha(c), "argv": ["sh", p], "scenarios": s })
        })
        .collect();
    fs::write(
        dir.join("frozen.json"),
        serde_json::json!({
            "requirement_sha256": "r", "wording": "w", "manifest_sha256": "m", "tests": tests
        })
        .to_string(),
    )
    .unwrap();
}

/// Commits `files` on `branch` (made from `from` when new).
fn commit(r: &Path, branch: &str, from: &str, files: &[(&str, &str)], message: &str) {
    if git(r, &["branch", "--list", branch]).is_empty() {
        git(r, &["branch", branch, from]);
    }
    git(r, &["checkout", "-q", branch]);
    for (p, c) in files {
        let path = r.join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, c).unwrap();
    }
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", message]);
    git(r, &["checkout", "-q", "main"]);
}

const US1: &str = "grep -q answer=42 value.txt && grep -q feature=on lib.txt\n";
const ORACLE_MSG: &str = "speccoder: oracle US1 — 1 scenario(s), 1 to turn green\n\nUS1-1: the answer is set → it is 42\n\nOracle: US1\nOracle-Scenarios: US1-1=red\nOracle-Tests: 1\nOracle-By: speccoder run\n";

/// main has `value.txt`; my branch adds `lib.txt` and its oracle.
fn repo(state: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let r = dir.path();
    fs::write(r.join("value.txt"), "answer=42\n").unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);
    commit(r, MINE, "main", &[("lib.txt", "feature=on\n")], "lib");
    commit(r, MINE, MINE, &[("tests/us1.sh", US1)], ORACLE_MSG);
    freeze(
        state,
        r,
        FEATURE,
        "US1",
        &[("tests/us1.sh", US1, &["US1-1"])],
    );
    dir
}

fn forecast(r: &Path, state: &Path, named: &[String]) -> (CommandResult, ForecastReport) {
    let res = run(
        r,
        &Options {
            feature: Some(FEATURE),
            fetch: false,
            named,
            state: Some(state),
            progress: None,
        },
    );
    let Some(CommandData::Forecast(f)) = res.data() else {
        panic!("{:?}: {}", res.status(), res.message())
    };
    let f = (**f).clone();
    (res, f)
}

fn verdict_of<'a>(
    f: &'a ForecastReport,
    branch: &str,
) -> &'a speccoder_core::forecast::PairForecast {
    f.pairs
        .iter()
        .find(|p| p.branch == branch)
        .unwrap_or_else(|| panic!("{branch} not paired: {:?}", f.pairs))
}

#[test]
fn us1_each_sibling_breaks_conflicts_holds_or_is_not_judged_and_a_second_look_runs_nothing() {
    if !sandboxed() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    commit(
        r,
        "speccoder/breaker",
        "main",
        &[("value.txt", "answer=41\n")],
        "wrong answer",
    );
    commit(
        r,
        "speccoder/clash",
        "main",
        &[("lib.txt", "feature=off\n")],
        "lib too",
    );
    commit(
        r,
        "speccoder/docs",
        "main",
        &[("NOTES.md", "# Notes\n")],
        "notes",
    );
    commit(
        r,
        "speccoder/clean",
        "main",
        &[("other.txt", "x\n")],
        "other",
    );
    // A remote twin of a local sibling is checked once.
    let clean = git(r, &["rev-parse", "speccoder/clean"]);
    git(r, &["update-ref", "refs/remotes/origin/clean", &clean]);

    let (res, f) = forecast(r, state.path(), &[]);
    assert_eq!(res.status(), CommandStatus::NeedsHuman, "{}", res.message());
    assert_eq!(res.code().map(|c| c.name), Some("FORECAST-CONFLICT"));
    assert_eq!(f.pairs.len(), 4, "{:?}", f.pairs);
    assert_eq!((f.oracle.scenarios, f.oracle.green), (1, 1));

    let b = verdict_of(&f, "speccoder/breaker");
    assert_eq!(b.verdict, PairVerdict::Breaks, "{b:?}");
    assert_eq!(b.broken[0].scenario, "US1-1");
    assert!(b.broken[0].mine);
    assert_eq!(
        b.broken[0].contract.as_deref(),
        Some("the answer is set → it is 42")
    );
    assert!(
        b.fixes[0].contains("git merge --no-commit --no-ff speccoder/breaker"),
        "{b:?}"
    );
    assert_eq!(b.author, "Dana");

    let c = verdict_of(&f, "speccoder/clash");
    assert_eq!(
        (c.verdict, c.files.as_slice()),
        (PairVerdict::Conflicts, &["lib.txt".to_owned()][..])
    );

    let d = verdict_of(&f, "speccoder/docs");
    assert_eq!(
        (d.verdict, d.why.as_deref()),
        (PairVerdict::NotJudged, Some("docs only"))
    );

    let ok = verdict_of(&f, "speccoder/clean");
    assert_eq!((ok.verdict, ok.held), (PairVerdict::NoConflictObserved, 1));

    // Problems first.
    assert!(f.pairs[0].verdict.is_problem() && f.pairs[1].verdict.is_problem());
    assert!(!f.headline().contains("safe"));
    assert!(f.ran > 0);

    // Unchanged: nothing runs again, the answer is the same.
    let (_, again) = forecast(r, state.path(), &[]);
    assert_eq!(again.ran, 0, "{again:?}");
    assert!(again.reused > 0);
    assert_eq!(
        again.pairs.iter().map(|p| p.verdict).collect::<Vec<_>>(),
        f.pairs.iter().map(|p| p.verdict).collect::<Vec<_>>()
    );

    // A new commit on one sibling re-checks only that merge.
    commit(
        r,
        "speccoder/clean",
        "speccoder/clean",
        &[("more.txt", "y\n")],
        "more",
    );
    let (_, third) = forecast(r, state.path(), &[]);
    assert_eq!(third.ran, 1, "only the new merge runs: {third:?}");

    // The last forecast is stored for the web app.
    let stored = last(r, Some(FEATURE), Some(state.path()));
    assert!(matches!(stored.data(), Some(CommandData::Forecast(_))));
}

#[test]
fn us1_a_merge_that_changes_a_frozen_test_rewrites_what_judges_the_work() {
    if !sandboxed() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    commit(
        r,
        "speccoder/lax",
        MINE,
        &[("tests/us1.sh", "true\n")],
        "lax",
    );
    commit(r, MINE, MINE, &[("more.txt", "z\n")], "more");
    let (_, f) = forecast(r, state.path(), &[]);
    let p = verdict_of(&f, "speccoder/lax");
    assert_eq!(p.verdict, PairVerdict::Breaks, "{p:?}");
    assert_eq!(p.rewritten, ["tests/us1.sh"]);
    assert!(p.broken.is_empty());
}

#[test]
fn us1_a_test_failing_once_on_the_merge_is_flaky_not_a_break() {
    if !sandboxed() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    let once = "[ -f other.txt ] || exit 0\n[ -f .tried ] && exit 0\ntouch .tried\nexit 1\n";
    commit(r, MINE, MINE, &[("tests/once.sh", once)], "once");
    freeze(
        state.path(),
        r,
        FEATURE,
        "US1",
        &[
            ("tests/us1.sh", US1, &["US1-1"]),
            ("tests/once.sh", once, &["US1-1"]),
        ],
    );
    commit(
        r,
        "speccoder/other",
        "main",
        &[("other.txt", "x\n")],
        "other",
    );
    let (res, f) = forecast(r, state.path(), &[]);
    let p = verdict_of(&f, "speccoder/other");
    assert_eq!(p.verdict, PairVerdict::NoConflictObserved, "{p:?}");
    assert_eq!(p.flaky, ["tests/once.sh"]);
    assert_eq!(res.status(), CommandStatus::Ok);
}

#[test]
fn us3_their_oracle_judges_my_code_too() {
    if !sandboxed() {
        return;
    }
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    let theirs = "! grep -q feature=on lib.txt 2>/dev/null\n";
    commit(
        r,
        "speccoder/002-other",
        "main",
        &[("other.txt", "x\n")],
        "other",
    );
    commit(
        r,
        "speccoder/002-other",
        "speccoder/002-other",
        &[("tests/other.sh", theirs)],
        &ORACLE_MSG.replace(
            "US1-1: the answer is set → it is 42",
            "US1-1: no feature flag → none is on",
        ),
    );
    freeze(
        state.path(),
        r,
        "002-other",
        "US1",
        &[("tests/other.sh", theirs, &["US1-1"])],
    );
    let (_, f) = forecast(r, state.path(), &[]);
    let p = verdict_of(&f, "speccoder/002-other");
    assert!(p.their_oracle, "{p:?}");
    assert_eq!(p.verdict, PairVerdict::Breaks);
    assert_eq!(p.broken.len(), 1, "{p:?}");
    assert!(!p.broken[0].mine);
    assert_eq!(p.broken[0].unit, "002-other");
    assert!(p.fixes[0].contains("002-other's US1-1"), "{}", p.fixes[0]);
}

#[test]
fn us2_without_an_oracle_or_a_branch_it_says_why() {
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    // No frozen oracle here: textual only, each pair says why.
    fs::remove_dir_all(
        speccoder_runtime::worktree::project_state_dir(state.path(), r).join("oracle"),
    )
    .unwrap();
    commit(
        r,
        "speccoder/clean",
        "main",
        &[("other.txt", "x\n")],
        "other",
    );
    let (res, f) = forecast(r, state.path(), &[]);
    assert_eq!(res.status(), CommandStatus::Ok);
    let p = verdict_of(&f, "speccoder/clean");
    assert_eq!(p.verdict, PairVerdict::NotJudged);
    assert_eq!(p.why.as_deref(), Some("no frozen oracle here to judge it"));
    assert_eq!(f.ran, 0);

    // A named branch that is not there is the person's mistake.
    let res = run(
        r,
        &Options {
            feature: Some(FEATURE),
            named: &["nope".to_owned()],
            state: Some(state.path()),
            ..Options::default()
        },
    );
    assert_eq!(res.status(), CommandStatus::UsageError);

    // No feature branch: nothing to forecast.
    git(r, &["branch", "-D", MINE]);
    let res = run(
        r,
        &Options {
            feature: Some(FEATURE),
            state: Some(state.path()),
            ..Options::default()
        },
    );
    assert_eq!(res.code().map(|c| c.name), Some("FORECAST-NO-BRANCH"));
}

#[test]
fn us1_siblings_are_never_the_base_a_lane_or_a_stale_twin_and_a_name_counts_once() {
    let state = tempfile::tempdir().unwrap();
    let dir = repo(state.path());
    let r = dir.path();
    fs::remove_dir_all(
        speccoder_runtime::worktree::project_state_dir(state.path(), r).join("oracle"),
    )
    .unwrap();
    commit(
        r,
        "speccoder/003-import",
        "main",
        &[("import.txt", "1\n")],
        "import",
    );
    // An older remote copy of 003-import, a lane, and the checkout on 003.
    let old = git(r, &["rev-parse", "speccoder/003-import"]);
    commit(
        r,
        "speccoder/003-import",
        "speccoder/003-import",
        &[("import.txt", "2\n")],
        "more",
    );
    git(
        r,
        &[
            "update-ref",
            "refs/remotes/origin/speccoder/003-import",
            &old,
        ],
    );
    commit(
        r,
        "speccoder-lane/001-graph/a",
        MINE,
        &[("lane.txt", "x\n")],
        "lane",
    );
    git(r, &["checkout", "-q", "speccoder/003-import"]);

    let (_, f) = forecast(r, state.path(), &[]);
    let names: Vec<_> = f.pairs.iter().map(|p| p.branch.as_str()).collect();
    assert_eq!(names, ["speccoder/003-import"], "{f:?}");
    assert_eq!(f.base.as_deref(), Some("main"));

    let twice = [
        "speccoder/003-import".to_owned(),
        "speccoder/003-import".to_owned(),
        MINE.to_owned(),
    ];
    let (_, f) = forecast(r, state.path(), &twice);
    assert_eq!(f.pairs.len(), 1, "{f:?}");
}
