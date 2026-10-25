//! `speccoder report --live` starts fast: the median of 20 runs of the release
//! binary is under 100 ms, with no run recorded and with a
//! finished run's live state. Ignored in the debug suite; `just cold-start`
//! runs it on a release build (`cargo test --release … -- --ignored`).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// The budget for the median.
const BUDGET: Duration = Duration::from_millis(100);
const WARM_UPS: usize = 3;
const RUNS: usize = 20;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Median and maximum of `RUNS` timed runs of `speccoder report --live` in `project`.
fn time(project: &Path, state: &Path) -> (Duration, Duration) {
    let run = || {
        let started = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_speccoder"))
            .args(["report", "--live"])
            .current_dir(project)
            .env("SPECCODER_STATE_DIR", state)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "status failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        started.elapsed()
    };
    for _ in 0..WARM_UPS {
        run();
    }
    let mut times: Vec<Duration> = (0..RUNS).map(|_| run()).collect();
    times.sort();
    (times[RUNS / 2], times[RUNS - 1])
}

#[test]
#[ignore = "release-build timing; run with `just cold-start`"]
fn status_starts_within_budget() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-speckit/tests/fixtures/compat/1.2.3");
    std::fs::create_dir_all(&project).unwrap();
    let copied = Command::new("cp")
        .arg("-R")
        .arg(fixture.join("."))
        .arg(&project)
        .status()
        .unwrap()
        .success();
    assert!(copied);
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-q", "-m", "init"]);
    let state = dir.path().join("state");

    let (median, max) = time(&project, &state);
    println!("cold start, no run recorded: median {median:?}, max {max:?}");
    assert!(median < BUDGET, "no run: median {median:?} ≥ {BUDGET:?}");

    // A finished run's live state, where `status` reads it.
    let root = std::fs::canonicalize(&project).unwrap();
    let live = speccoder_runtime::worktree::project_state_dir(&state, &root)
        .join(speccoder_runtime::progress::FILE);
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    let finished = speccoder_runtime::progress::LiveState {
        pid: 1,
        feature: "001-sample".to_owned(),
        task: "T001".to_owned(),
        finished: Some("passed".to_owned()),
        ..speccoder_runtime::progress::LiveState::default()
    };
    std::fs::write(&live, serde_json::to_string(&finished).unwrap()).unwrap();
    let (median, max) = time(&project, &state);
    println!("cold start, a finished run: median {median:?}, max {max:?}");
    assert!(
        median < BUDGET,
        "finished run: median {median:?} ≥ {BUDGET:?}"
    );
}
