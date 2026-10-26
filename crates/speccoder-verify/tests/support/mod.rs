//! Repositories with ticks of every kind, signed with a throw-away SSH key:
//! the fixtures of the branch verifier. The statements are written the way
//! `attest::record_tick` and `record_decision` write them, so the
//! verifier's rules see what speccoder produces.
//!
//! Shared with `crates/speccoder/tests` (`#[path]`), so `speccoder attest
//! verify --branch` is checked on the same repositories.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use speccoder_core::predicate::{self, TaskVerificationV0};
use speccoder_runtime::attest::dsse::{self, Envelope};
use speccoder_runtime::attest::statement::{self, Phase, PhaseResult, Subject};

/// The unit every fixture has.
pub const FEATURE: &str = "001-login";

const CONSTITUTION: &str = "# Constitution\n\nTests first.\n";

/// The spec: three requirements, one story.
pub const SPEC: &str = "# Feature Specification: Login\n\n## User Scenarios & Testing\n\n### User Story 1 - Login (Priority: P1)\n\nUsers log in.\n\n**Acceptance Scenarios**:\n\n1. **Given** an account, **When** the user logs in, **Then** they are in.\n\n## Requirements\n\n- **FR-001**: Users MUST log in.\n- **FR-002**: Users MUST log out.\n- **FR-003**: Sessions MUST expire after an hour.\n";

/// What the unit's tasks are: `(id, title)`.
pub fn tasks(n: usize) -> Vec<(String, String)> {
    (1..=n)
        .map(|i| {
            // T009 alone cites FR-003 (the fixtures' stale requirement).
            let fr = if i == 9 { 3 } else { (i - 1) % 2 + 1 };
            (
                format!("T{i:03}"),
                format!("Task {i} of the login (FR-{fr:03})"),
            )
        })
        .collect()
}

fn tasks_md(n: usize) -> String {
    let mut out = String::from("# Tasks: Login\n\n## Phase 1: Setup\n\n");
    for (id, title) in tasks(n) {
        out.push_str(&format!("- [ ] {id} {title}\n"));
    }
    out
}

/// git in `dir`, as Ada, never signing commits, no hooks.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
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

/// Whether `program` can run here.
pub fn has(program: &str) -> bool {
    Command::new(program).arg("-?").output().is_ok()
}

/// A throw-away ed25519 key in `dir`, and an allowed-signers file trusting
/// it for `principal`.
pub fn key(dir: &Path, name: &str, principal: &str) -> (PathBuf, PathBuf) {
    let key = dir.join(name);
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", principal, "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    let public = fs::read_to_string(dir.join(format!("{name}.pub"))).unwrap();
    let allowed = dir.join(format!("{name}.allowed_signers"));
    fs::write(
        &allowed,
        format!("{principal} namespaces=\"in-toto\" {public}"),
    )
    .unwrap();
    (key, allowed)
}

/// How a fixture tick is signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// With the trusted key.
    Trusted,
    /// With a key nobody trusts.
    Stranger,
    /// Not at all.
    No,
}

/// A fixture repository on branch `feature`, forked from `main`.
pub struct Repo {
    /// The repository.
    pub dir: tempfile::TempDir,
    keys: tempfile::TempDir,
    /// The trusted key.
    pub key: PathBuf,
    /// The allowed-signers file trusting it (outside the repository).
    pub allowed: PathBuf,
    stranger: PathBuf,
    /// How many tasks the unit has.
    pub n: usize,
}

impl Repo {
    /// A Spec Kit project with one unit of `n` open tasks, committed on
    /// `main`, the checkout on `feature`.
    pub fn new(n: usize) -> Self {
        let keys = tempfile::tempdir().unwrap();
        let (stranger, _) = key(keys.path(), "eve", "eve@example.com");
        let (key, allowed) = key(keys.path(), "ada", "ada@example.com");
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        fs::create_dir_all(r.join(".specify/memory")).unwrap();
        fs::write(r.join(".specify/memory/constitution.md"), CONSTITUTION).unwrap();
        let unit = r.join("specs").join(FEATURE);
        fs::create_dir_all(&unit).unwrap();
        fs::write(unit.join("spec.md"), SPEC).unwrap();
        fs::write(unit.join("plan.md"), "# Plan\n\nRust.\n").unwrap();
        fs::write(unit.join("tasks.md"), tasks_md(n)).unwrap();
        fs::create_dir_all(r.join("src")).unwrap();
        fs::write(r.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        git(r, &["init", "-q", "-b", "main"]);
        git(r, &["config", "gpg.format", "ssh"]);
        git(
            r,
            &["config", "user.signingKey", &key.display().to_string()],
        );
        git(r, &["add", "-A"]);
        git(r, &["commit", "-q", "-m", "init"]);
        git(r, &["checkout", "-q", "-b", "feature"]);
        Self {
            dir,
            keys,
            key,
            allowed,
            stranger,
            n,
        }
    }

    /// The repository's path.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// git here.
    pub fn git(&self, args: &[&str]) -> String {
        git(self.path(), args)
    }

    /// The tasks file, repository-relative.
    pub fn tasks_path() -> String {
        format!("specs/{FEATURE}/tasks.md")
    }

    fn title(&self, task: &str) -> String {
        tasks(self.n)
            .into_iter()
            .find(|(id, _)| id == task)
            .map(|(_, t)| t)
            .unwrap()
    }

    /// Ticks `task` in the checkout's task list (`[ ]` → `[X]`); returns
    /// the sha256 of its line read as open.
    pub fn tick_line(&self, task: &str) -> String {
        let file = self.path().join(Self::tasks_path());
        let text = fs::read_to_string(&file).unwrap();
        let open = format!("- [ ] {task} {}", self.title(task));
        assert!(text.contains(&open), "{task} is not open");
        fs::write(
            &file,
            text.replace(&open, &format!("- [X] {task} {}", self.title(task))),
        )
        .unwrap();
        speccoder_runtime::integrity::sha256_hex(open.as_bytes())
    }

    /// Commits everything with `message` (trailers included).
    pub fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// A tick by speccoder: the line ticked, a change, a commit with the
    /// task's trailers and its statements as the commit's note. `extra`
    /// fields go into the predicate (a newer revision's).
    pub fn tick(&self, task: &str, verdict: &str, sign: Sign) -> String {
        self.tick_with(task, verdict, sign, &[])
    }

    /// [`Repo::tick`] with predicate fields this build does not know.
    pub fn tick_with(
        &self,
        task: &str,
        verdict: &str,
        sign: Sign,
        extra: &[(&str, Value)],
    ) -> String {
        let line_hash = self.tick_line(task);
        fs::write(
            self.path().join(format!("src/{}.rs", task.to_lowercase())),
            format!("pub fn {}() {{}}\n", task.to_lowercase()),
        )
        .unwrap();
        let mut message =
            format!("{task}: done\n\nSpec-Task: {FEATURE}/{task}\nVerdict: {verdict}\n");
        if verdict == "attested" {
            message.push_str("Accepted-By: Ada\n");
        }
        let commit = self.commit(&message);
        self.note(&commit, task, verdict, &line_hash, sign, extra);
        commit
    }

    /// A hand tick: the line ticked and committed, no trailers, no note.
    pub fn hand_tick(&self, task: &str) -> String {
        self.tick_line(task);
        self.commit(&format!("tick {task} by hand"))
    }

    fn predicate(
        &self,
        task: &str,
        verdict: &str,
        line_hash: &str,
        tree: &str,
    ) -> TaskVerificationV0 {
        let title = self.title(task);
        let fr = title
            .split(['(', ')'])
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let decision = verdict == "attested";
        TaskVerificationV0 {
            revision: Some(predicate::REVISION),
            verdict: verdict.to_owned(),
            unit: predicate::Unit {
                format: "speckit".to_owned(),
                feature: FEATURE.to_owned(),
                dir: format!("specs/{FEATURE}"),
            },
            task: predicate::TaskRef {
                id: task.to_owned(),
                line_hash: line_hash.to_owned(),
                title,
            },
            tree: tree.to_owned(),
            spec: (!decision).then(|| predicate::SpecRef {
                section: None,
                section_hash: String::new(),
            }),
            trace: (!decision).then(|| {
                json!([
                    { "requirement": fr, "requirement_hash": "00", "phase": "green", "result": verdict },
                    { "requirement": "US1", "scenario": "US1.1", "test": format!("tests/{}.rs", task.to_lowercase()), "phase": "green", "result": verdict }
                ])
            }),
            integrity: (!decision).then(|| json!({ "findings": [] })),
            monitor: (!decision).then(|| json!({ "state": "off", "findings": [] })),
            mutation: (!decision).then(|| json!({ "caught": 3, "missed": 1 })),
            sandbox: (!decision).then(|| json!({ "mode": "off" })),
            executor: (!decision).then(|| json!({ "kind": "speccoder" })),
            model: (!decision).then_some(Value::Null),
            run: (!decision).then_some(Value::Null),
            decisions: (!decision).then(|| json!([])),
            strength: (!decision).then(|| json!({ "visible": { "share": 0.82 }, "combined": { "share": 0.91 }, "near_misses": [], "permitted": [] })),
            instructions: (!decision).then(|| json!([])),
            decider: decision.then(|| predicate::Decider {
                by: "Ada".to_owned(),
                reason: "checked by hand".to_owned(),
            }),
            policy_sha256: String::new(),
            time: "2026-10-26T00:00:00Z".to_owned(),
            red: (verdict == "verified").then(|| predicate::Red {
                kind: predicate::RedKind::Assertion,
                excerpt: Some("assertion failed: logged_in".to_owned()),
                excerpt_sha256: Some(speccoder_runtime::integrity::sha256_hex(b"assertion failed: logged_in")),
                output_sha256: "ff".to_owned(),
                withheld: false,
            }),
            bug: None,
            acceptance: None,
            context: None,
            dependencies: None,
            extra: std::collections::BTreeMap::new(),
        }
    }

    /// Writes `commit`'s note as speccoder would for `verdict`.
    pub fn note(
        &self,
        commit: &str,
        task: &str,
        verdict: &str,
        line_hash: &str,
        sign: Sign,
        extra: &[(&str, Value)],
    ) {
        let tree = self.git(&["rev-parse", &format!("{commit}^{{tree}}")]);
        let subject = Subject {
            name: format!("{FEATURE}/{task}"),
            commit: commit.to_owned(),
        };
        let mut tv = json!(self.predicate(task, verdict, line_hash, &tree));
        for (k, v) in extra {
            tv[*k] = v.clone();
        }
        let file = format!("tests/{}.rs", task.to_lowercase());
        let mut named: Vec<(String, Value)> = Vec::new();
        if verdict != "attested" {
            let mut phases = vec![PhaseResult {
                phase: Phase::Baseline,
                result: "PASSED",
                passed: Vec::new(),
                warned: Vec::new(),
                failed: Vec::new(),
                config_sha256: "aa".to_owned(),
            }];
            if verdict == "verified" {
                phases.push(PhaseResult {
                    phase: Phase::Red,
                    result: "FAILED",
                    passed: Vec::new(),
                    warned: Vec::new(),
                    failed: vec![file.clone()],
                    config_sha256: "bb".to_owned(),
                });
            }
            phases.push(PhaseResult {
                phase: Phase::Green,
                result: "PASSED",
                passed: if verdict == "verified" {
                    vec!["checks::all".to_owned(), file.clone()]
                } else {
                    vec!["checks::all".to_owned()]
                },
                warned: Vec::new(),
                failed: Vec::new(),
                config_sha256: "cc".to_owned(),
            });
            if verdict == "verified" {
                phases.push(PhaseResult {
                    phase: Phase::HeldOut,
                    result: "PASSED",
                    passed: Vec::new(),
                    warned: Vec::new(),
                    failed: Vec::new(),
                    config_sha256: format!("{:0>64}", task.to_lowercase()),
                });
            }
            for p in &phases {
                named.push((
                    format!("test-result/{}", p.phase.name()),
                    statement::statement(
                        &subject,
                        statement::TEST_RESULT,
                        statement::test_result(p),
                    ),
                ));
            }
        }
        named.push((
            "task-verification".to_owned(),
            statement::statement(&subject, statement::TASK_VERIFICATION, tv),
        ));
        let mut lines = Vec::new();
        let mut inputs = Vec::new();
        for (name, s) in &named {
            let bytes = statement::canonical(s);
            inputs.push((
                name.clone(),
                speccoder_runtime::integrity::sha256_hex(&bytes),
            ));
            lines.push(Envelope::unsigned(&bytes));
        }
        if verdict != "attested" {
            let summary = statement::statement(
                &subject,
                statement::VSA,
                statement::vsa(&statement::Summary {
                    speccoder: "0.2.0".to_owned(),
                    agentplane: "0.53.0".to_owned(),
                    time: "2026-10-26T00:00:00Z".to_owned(),
                    resource_uri: format!("git:{commit}"),
                    policy_sha256: String::new(),
                    inputs,
                    verdict: verdict.to_owned(),
                }),
            );
            lines.push(Envelope::unsigned(&statement::canonical(&summary)));
        }
        let key = match sign {
            Sign::Trusted => Some(&self.key),
            Sign::Stranger => Some(&self.stranger),
            Sign::No => None,
        };
        if let Some(key) = key {
            self.git(&["config", "user.signingKey", &key.display().to_string()]);
            let signer = speccoder_runtime::attest::sign::signer(
                self.path(),
                &speccoder_runtime::substrate::run_config::AttestConfig::default(),
            )
            .unwrap();
            assert_eq!(
                speccoder_runtime::attest::sign_all(&mut lines, &signer),
                None
            );
            self.git(&["config", "user.signingKey", &self.key.display().to_string()]);
        }
        speccoder_runtime::attest::notes::write(self.path(), commit, &dsse::encode(&lines))
            .unwrap();
    }

    /// The note of `commit`.
    pub fn read_note(&self, commit: &str) -> String {
        self.git(&["notes", "--ref=speccoder", "show", commit])
    }

    /// Replaces `commit`'s note.
    pub fn set_note(&self, commit: &str, text: &str) {
        let file = self.keys.path().join("note");
        fs::write(&file, text).unwrap();
        self.git(&[
            "notes",
            "--ref=speccoder",
            "add",
            "-f",
            "-F",
            &file.display().to_string(),
            commit,
        ]);
    }

    /// Rewrites `commit`'s task-verification with `edit`, keeping its
    /// signature: what a tampered note looks like.
    pub fn tamper(&self, commit: &str, edit: impl Fn(&mut Value)) {
        let note = self.read_note(commit);
        let mut out = Vec::new();
        for line in note.lines() {
            let mut e: Value = serde_json::from_str(line).unwrap();
            let payload = e["payload"].as_str().unwrap();
            use base64::Engine as _;
            let engine = base64::engine::general_purpose::STANDARD;
            let mut s: Value = serde_json::from_slice(&engine.decode(payload).unwrap()).unwrap();
            if s["predicateType"] == statement::TASK_VERIFICATION {
                edit(&mut s["predicate"]);
                e["payload"] = json!(engine.encode(statement::canonical(&s)));
            }
            out.push(e.to_string());
        }
        self.set_note(commit, &(out.join("\n") + "\n"));
    }

    /// Edits the checkout's spec: `from` → `to`, committed.
    pub fn edit_spec(&self, from: &str, to: &str) -> String {
        let file = self.path().join(format!("specs/{FEATURE}/spec.md"));
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains(from));
        fs::write(&file, text.replace(from, to)).unwrap();
        self.commit("edit the spec")
    }

    /// Edits a ticked task's line after its tick.
    pub fn edit_task_line(&self, task: &str, suffix: &str) -> String {
        let file = self.path().join(Self::tasks_path());
        let text = fs::read_to_string(&file).unwrap();
        let line = format!("- [X] {task} {}", self.title(task));
        assert!(text.contains(&line));
        fs::write(&file, text.replace(&line, &format!("{line}{suffix}"))).unwrap();
        self.commit(&format!("reword {task}"))
    }

    /// Records a person's skip of `task` as speccoder does (a ref).
    pub fn skip(&self, task: &str) {
        let head = self.git(&["rev-parse", "HEAD"]);
        self.git(&[
            "update-ref",
            &format!("refs/speccoder/skipped/{FEATURE}/{task}"),
            &head,
        ]);
    }
}

/// The verifier binary built for these tests, or (from another crate's
/// tests) the one in the same target directory.
pub fn verifier() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_speccoder-verify") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(Path::parent).unwrap();
    dir.join(format!("speccoder-verify{}", std::env::consts::EXE_SUFFIX))
}

/// Runs the verifier in `repo` with `args`; (exit code, stdout, stderr).
pub fn run(repo: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(verifier())
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `check --json` with the trusted signers; (exit code, the envelope).
pub fn check(r: &Repo, extra: &[&str]) -> (i32, Value) {
    let allowed = r.allowed.display().to_string();
    let mut args = vec!["check", "--json", "--allowed-signers", &allowed];
    args.extend_from_slice(extra);
    let (code, out, err) = run(r.path(), &args);
    let v: Value =
        serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{e}: {out} / {err}"));
    (code, v)
}

/// `(task, outcome, verdict or reason)` per tick of an envelope.
pub fn outcomes(v: &Value) -> Vec<(String, String, String)> {
    v["data"]["ticks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let o = t["outcome"].as_str().unwrap().to_owned();
            let what = t["verdict"]
                .as_str()
                .filter(|_| o != "unverifiable")
                .or_else(|| t["reason"].as_str())
                .unwrap_or_default()
                .to_owned();
            (t["tick"]["task"].as_str().unwrap().to_owned(), o, what)
        })
        .collect()
}
