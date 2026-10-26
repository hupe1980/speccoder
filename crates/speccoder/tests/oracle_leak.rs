//! `oracle_leak` (spec 093, SC-003, FR-022/FR-023): with a requirement's
//! tests written and frozen at review and a held-out set sealed beside
//! them, no response of the web host — every read endpoint, a job's result
//! and its event stream — carries a held-out path or literal; the file and
//! diff endpoints refuse the state directory and a commit no branch reaches.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use speccoder_core::acceptance::Decided;
use speccoder_core::actions::Via;
use speccoder_core::oracle::{AuthoredBy, HeldOutSummary};
use speccoder_core::verify::RedClass;

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const SPEC: &str = "# Spec\n\n### User Story 1 - Login (Priority: P1)\n\nUsers log in.\n\n**Acceptance Scenarios**:\n\n1. **Given** a user, **When** they log in, **Then** a `login` function exists.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n";

const SECRET: &str = "Zq9-held-out-only-literal";
const HELD_OUT_PATH: &str = "tests/held_out_us1_secret.txt";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn sha(text: &str) -> String {
    speccoder_runtime::integrity::sha256_hex(text.as_bytes())
}

/// A project whose US1 has written, frozen tests and a sealed held-out
/// set — as the review leaves them — without a model.
fn project(state: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speccoder-speckit/tests/fixtures/graph");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::write(r.join("specs/001-graph/spec.md"), SPEC).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(
        r.join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n\n[checks.has-ok]\nargv = [\"grep\", \"-q\", \"fn ok\", \"src/lib.rs\"]\n",
    )
    .unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "init"]);

    let plane = speccoder_runtime::worktree::project_state_dir(state, r);
    let inspected = speccoder_runtime::inspect(r, None);
    let Some(speccoder_core::CommandData::Inspect(snap)) = inspected.data() else {
        panic!("{}", inspected.message())
    };
    let req = speccoder_runtime::format::of(snap)
        .oracle_requirements(snap)
        .into_iter()
        .find(|q| q.key == "US1")
        .unwrap();
    // The sealed held-out set: the only place the secret is.
    let held = format!("login \"{SECRET}\"\n");
    let manifest = speccoder_runtime::held_out::Manifest {
        version: 1,
        feature: "001-graph".to_owned(),
        task: "req-US1".to_owned(),
        author_run: "run-1".to_owned(),
        checkpoint: "0".to_owned(),
        visible_scenarios: vec!["US1-1".to_owned()],
        files: vec![speccoder_runtime::held_out::HeldOutFile {
            path: HELD_OUT_PATH.to_owned(),
            sha256: sha(&held),
            scenarios: vec!["US1-1".to_owned()],
            argv: vec![
                "grep".to_owned(),
                "-q".to_owned(),
                "fn login".to_owned(),
                "src/lib.rs".to_owned(),
            ],
            composition: false,
        }],
        discarded: Vec::new(),
    };
    let sealed = speccoder_runtime::held_out::seal(
        &plane,
        "001-graph",
        "US1",
        &manifest,
        &[(HELD_OUT_PATH.to_owned(), held)],
    )
    .unwrap();
    let visible = "checks fn login\n";
    let draft = speccoder_runtime::oracle::Draft {
        requirement: speccoder_runtime::oracle::DraftRequirement {
            key: req.key.clone(),
            title: req.title.clone(),
            text: req.text.clone(),
            sha256: req.digest(),
        },
        samples: 2,
        tests: vec![speccoder_runtime::oracle::DraftTest {
            path: "tests/acceptance_us1.txt".to_owned(),
            sha256: format!("sha256:{}", sha(visible)),
            scenarios: vec!["US1-1".to_owned()],
            argv: vec![
                "grep".to_owned(),
                "-q".to_owned(),
                "fn login".to_owned(),
                "src/lib.rs".to_owned(),
            ],
            red_class: RedClass::Assertion,
            authored_by: AuthoredBy::Model,
            edited_by: None,
        }],
        discarded: Vec::new(),
        rejected: Vec::new(),
        expectations: speccoder_runtime::oracle::Samples::default(),
        untestable: speccoder_runtime::oracle::Samples::default(),
        ambiguities: Vec::new(),
        held_out: HeldOutSummary {
            state: "kept".to_owned(),
            count: 1,
            manifest_sha256: Some(format!("sha256:{}", sealed.manifest_sha256)),
            reason: None,
        },
        sealed: Some(sealed),
        notes: Vec::new(),
        runs: Vec::new(),
        context: Vec::new(),
    };
    speccoder_runtime::oracle::store(
        &plane,
        "001-graph",
        &draft,
        &[("tests/acceptance_us1.txt".to_owned(), visible.to_owned())],
    )
    .unwrap();
    let f = speccoder_runtime::oracle::freezable(&plane, "001-graph", &[req]).unwrap();
    let rec = speccoder_runtime::oracle::freeze(
        &plane,
        "001-graph",
        &f[0],
        "sha256:a",
        Decided {
            by: "Ada".to_owned(),
            via: Via::Web,
            at: 1,
        },
    )
    .unwrap();
    fs::write(
        r.join("specs/001-graph/review.json"),
        serde_json::to_string_pretty(&json!({
            "requirements": {},
            "oracle": { "US1": {
                "requirement_sha256": rec.wording_digest,
                "manifest_sha256": f[0].manifest,
                "held_out_sha256": format!("sha256:{}", rec.held_out_sealed.as_ref().unwrap().manifest_sha256),
                "by": "Ada", "via": "web", "at": 1,
            } },
            "approval": { "sha256": "sha256:a", "mode": "ask", "by": "Ada", "via": "web", "at": 1 },
        }))
        .unwrap(),
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "review"]);
    dir
}

fn http(port: u16, method: &str, path: &str, auth: &str, body: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

fn clean(what: &str, text: &str) {
    assert!(!text.contains(SECRET), "{what}: held-out literal in {text}");
    assert!(
        !text.contains(HELD_OUT_PATH),
        "{what}: held-out path in {text}"
    );
    assert!(
        !text.contains("held_out_us1"),
        "{what}: held-out name in {text}"
    );
}

#[test]
fn oracle_leak_no_endpoint_or_event_carries_held_out_material() {
    let state = tempfile::tempdir().unwrap();
    let repo = project(state.path());
    // A dangling commit carrying the secret: no branch reaches it.
    let blob = {
        let mut c = Command::new("git")
            .args(["hash-object", "-w", "--stdin"])
            .current_dir(repo.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        c.stdin
            .take()
            .unwrap()
            .write_all(SECRET.as_bytes())
            .unwrap();
        String::from_utf8(c.wait_with_output().unwrap().stdout).unwrap()
    };
    let tree = {
        let mut c = Command::new("git")
            .args(["mktree"])
            .current_dir(repo.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        c.stdin
            .take()
            .unwrap()
            .write_all(format!("100644 blob {}\theld_out_us1_secret.txt\n", blob.trim()).as_bytes())
            .unwrap();
        String::from_utf8(c.wait_with_output().unwrap().stdout).unwrap()
    };
    let dangling = git(repo.path(), &["commit-tree", tree.trim(), "-m", "sealed"]);
    let dangling = dangling.trim().to_owned();

    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(["--json", "ui", "--no-open", "--port", "0", "-C"])
        .arg(repo.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home.path())
        .env("SPECCODER_STATE_DIR", state.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let _host = Killed(child);
    let ready: Value = serde_json::from_str(&first).unwrap();
    let port = u16::try_from(ready["data"]["port"].as_u64().unwrap()).unwrap();
    let url = ready["data"]["url"].as_str().unwrap();
    let auth = format!(
        "Authorization: Bearer {}\r\n",
        url.split_once("#token=").unwrap().1
    );

    // The event stream, read while everything below runs.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let listen = format!(
        "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    std::thread::spawn(move || {
        use std::io::Write;
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(listen.as_bytes()).unwrap();
        for line in BufReader::new(s).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // The review shows the frozen oracle: its count and digest, no more.
    let (status, review) = http(port, "GET", "/api/review", &auth, "");
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&review).unwrap();
    let o = &v["data"]["oracle"][0];
    assert_eq!(o["state"], "frozen", "{o}");
    assert_eq!(o["held_out"]["count"], 1);
    clean("/api/review", &review);

    let state_file = state.path().display().to_string();
    for path in [
        "/api/project",
        "/api/report",
        "/api/actions",
        "/api/inbox",
        "/api/review/comments",
        "/api/approvals",
        "/api/evidence?task=T005",
        "/api/tour?task=T005",
        "/api/documents",
        "/api/job",
        "/api/live",
        "/api/config",
        "/api/cost",
        "/api/changes",
        "/api/tree",
        "/api/file?path=specs/001-graph/review.json",
        &format!("/api/file?path={HELD_OUT_PATH}"),
        &format!("/api/file?path={state_file}/x"),
        "/api/file?path=../x",
        &format!("/api/diff?commit={dangling}"),
        "/api/diff?ref=refs/speccoder/held-out/001-graph",
        "/api/setup",
    ] {
        let (_, body) = http(port, "GET", path, &auth, "");
        if path.contains("held_out") || path.contains("held-out") {
            // The asked path is echoed back; the content never is.
            assert!(!body.contains(SECRET), "{path}: held-out literal in {body}");
        } else {
            clean(path, &body);
        }
    }
    let (_, diff) = http(
        port,
        "GET",
        &format!("/api/diff?commit={dangling}"),
        &auth,
        "",
    );
    assert_eq!(
        serde_json::from_str::<Value>(&diff).unwrap()["patch"],
        "",
        "{diff}"
    );
    let (_, file) = http(
        port,
        "GET",
        &format!("/api/file?path={state_file}/x"),
        &auth,
        "",
    );
    assert!(file.contains("not a project file"), "{file}");

    // A job: the person tries to reject the frozen set's test (refused: it
    // is frozen); its result and every event of it are clean too.
    let start = json!({
        "run": { "kind": "oracle_tests", "req": "US1", "op": "reject", "path": "tests/acceptance_us1.txt", "scenario": null, "reading": null },
        "input": "asserts too little",
        "feature": null,
        "command": "speccoder review tests US1 --reject tests/acceptance_us1.txt --reason \"…\"",
    });
    let (status, body) = http(port, "POST", "/api/run", &auth, &start.to_string());
    assert_eq!(status, 200, "{body}");
    let until = Instant::now() + Duration::from_secs(60);
    let mut done = None;
    while Instant::now() < until {
        let Ok(line) = rx.recv_timeout(Duration::from_secs(5)) else {
            continue;
        };
        clean("/api/events", &line);
        if line.contains("\"type\":\"done\"") {
            done = Some(line);
            break;
        }
    }
    let done = done.expect("the job ended");
    assert!(done.contains("ORACLE-FROZEN"), "{done}");
}
