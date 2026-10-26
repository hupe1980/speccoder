//! The verifier on fixture repositories: every kind of tick gets its
//! outcome and reason, the branch its conclusion, from git objects and the
//! caller's trust only — and each tick's verification equals
//! `attest verify`'s for its commit (one implementation, FR-002).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;

use serde_json::{Value, json};
use speccoder_core::CommandData;
use speccoder_runtime::attest::verify::{Trust, verify_at};
use support::{FEATURE, Repo, Sign, check, has, outcomes, run};

fn ssh() -> bool {
    if has("ssh-keygen") {
        true
    } else {
        eprintln!("skipped: no ssh-keygen");
        false
    }
}

fn row(task: &str, outcome: &str, what: &str) -> (String, String, String) {
    (task.to_owned(), outcome.to_owned(), what.to_owned())
}

/// Ticks of every kind on one branch.
fn mixed() -> (Repo, Vec<String>) {
    let r = Repo::new(10);
    let mut commits = Vec::new();
    commits.push(r.tick("T001", "verified", Sign::Trusted));
    commits.push(r.tick_with(
        "T002",
        "regression_only",
        Sign::Trusted,
        &[
            ("provenance", json!({ "builder": "next" })),
            ("revision", json!(9)),
        ],
    ));
    commits.push(r.tick("T003", "attested", Sign::Trusted));
    r.hand_tick("T004");
    let t5 = r.tick("T005", "verified", Sign::Trusted);
    r.tamper(&t5, |p| p["verdict"] = json!("regression_only"));
    commits.push(t5);
    commits.push(r.tick("T006", "verified", Sign::Stranger));
    commits.push(r.tick("T007", "verified", Sign::No));
    commits.push(r.tick("T008", "verified", Sign::Trusted));
    r.edit_task_line("T008", " (reworded)");
    // T009 cites FR-003, which changes after its tick.
    commits.push(r.tick("T009", "verified", Sign::Trusted));
    r.edit_spec(
        "Sessions MUST expire after an hour.",
        "Sessions MUST expire after a day.",
    );
    r.skip("T010");
    (r, commits)
}

#[test]
fn every_kind_of_tick_gets_its_outcome_and_the_branch_fails() {
    if !ssh() {
        return;
    }
    let (r, _) = mixed();
    let (code, v) = check(&r, &["--base", "main"]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["payload"], "branch_verify");
    assert_eq!(v["command"], "speccoder-verify check");
    let d = &v["data"];
    assert_eq!(d["conclusion"], "failure");
    assert_eq!(d["mode"], "touched");
    assert_eq!(
        outcomes(&v),
        [
            row("T001", "evidenced", "verified"),
            row("T002", "evidenced", "regression_only"),
            row("T003", "evidenced", "attested"),
            row("T004", "unverifiable", "ticked_without_evidence"),
            row("T005", "unverifiable", "bad_signature"),
            row("T006", "unverifiable", "untrusted_signer"),
            row("T007", "unverifiable", "unsigned"),
            row("T008", "unverifiable", "mismatch"),
            row("T009", "stale", "verified"),
        ]
    );
    // The unknown fields are tolerated and named, never deciding.
    assert_eq!(d["newer_fields"], json!(["provenance", "revision 9"]));
    assert_eq!(d["not_evaluated"], json!(["d121_regressions"]));
    assert_eq!(d["skipped"][0]["task"], "T010");
    assert_eq!(d["totals"]["skipped"], 1);
    assert_eq!(d["ticks"][2]["decider"], "Ada");
    // The stale tick points at the changed requirement's line.
    let t9 = &d["ticks"][8];
    assert_eq!(t9["stale"][0]["id"], "FR-003");
    assert_eq!(
        t9["stale_anchor"]["path"],
        format!("specs/{FEATURE}/spec.md")
    );
    let spec = fs::read_to_string(r.path().join(format!("specs/{FEATURE}/spec.md"))).unwrap();
    let line = spec.lines().position(|l| l.contains("FR-003")).unwrap() + 1;
    assert_eq!(t9["stale_anchor"]["line"], line);
    // A trace row per requirement, and the signed tests.
    let reqs: Vec<&str> = d["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["test"].is_null())
        .map(|t| t["requirement"].as_str().unwrap())
        .collect();
    assert!(
        reqs.contains(&"FR-001") && reqs.contains(&"FR-003"),
        "{reqs:?}"
    );
}

#[test]
fn each_ticks_verification_is_attest_verifys_for_its_commit() {
    if !ssh() {
        return;
    }
    let (r, commits) = mixed();
    let (_, v) = check(&r, &[]);
    let trust = Trust {
        allowed_signers: Some(r.allowed.clone()),
        ..Trust::default()
    };
    let mut compared = 0;
    for t in v["data"]["ticks"].as_array().unwrap() {
        let Some(commit) = t["commit"].as_str() else {
            continue;
        };
        assert!(commits.iter().any(|c| c == commit));
        let res = verify_at(r.path(), Some(FEATURE), Some(commit), &trust, true);
        let Some(CommandData::Attest(a)) = res.data() else {
            panic!("{}", res.message())
        };
        assert_eq!(
            t["verification"],
            serde_json::to_value(&a.objects[0]).unwrap(),
            "{}",
            t["tick"]["task"]
        );
        compared += 1;
    }
    assert_eq!(compared, 8);
}

#[test]
fn only_verified_ticks_succeed_and_nothing_to_check_is_neutral() {
    if !ssh() {
        return;
    }
    let r = Repo::new(3);
    let (code, v) = check(&r, &["--base", "main"]);
    assert_eq!(code, 78, "{v}");
    assert_eq!(v["data"]["conclusion"], "neutral");
    r.tick("T001", "verified", Sign::Trusted);
    r.tick("T002", "regression_only", Sign::Trusted);
    let (code, v) = check(&r, &["--base", "main"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["conclusion"], "success");
    assert_eq!(v["status"], "ok");
    let (code, v) = check(&r, &["--base", "main", "--require-verified"]);
    assert_eq!(code, 1);
    assert_eq!(v["data"]["conclusion"], "failure");
}

#[test]
fn no_trust_is_always_refused() {
    let r = Repo::new(1);
    for extra in [&[][..], &["--require-signed=false"][..]] {
        let mut args = vec!["check"];
        args.extend_from_slice(extra);
        let (code, _, err) = run(r.path(), &args);
        assert_eq!(code, 1);
        assert!(
            err.contains("no trust configured: pass --allowed-signers or --certificate-identity/--certificate-oidc-issuer"),
            "{err}"
        );
    }
}

#[test]
fn unsigned_ticks_pass_only_when_accepted_and_the_report_says_so() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    r.tick("T001", "verified", Sign::No);
    let (code, v) = check(&r, &[]);
    assert_eq!(code, 1);
    assert_eq!(outcomes(&v), [row("T001", "unverifiable", "unsigned")]);
    let (code, v) = check(&r, &["--require-signed=false"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["accepted_unsigned"], true);
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("unsigned ticks were accepted")
    );
}

#[test]
fn trust_is_read_from_the_base_revision_never_the_head() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    // The base trusts Ada; the branch adds its own trust file trusting
    // nobody — and must not be what is read.
    let path = ".github/speccoder/allowed_signers";
    r.git(&["checkout", "-q", "main"]);
    fs::create_dir_all(r.path().join(".github/speccoder")).unwrap();
    fs::copy(&r.allowed, r.path().join(path)).unwrap();
    r.commit("trust Ada");
    r.git(&["checkout", "-q", "feature"]);
    r.git(&["rebase", "-q", "main"]);
    fs::write(r.path().join(path), "").unwrap();
    r.commit("trust nobody");
    r.tick("T001", "verified", Sign::Trusted);
    let spec = format!("main:{path}");
    let (code, out, err) = run(
        r.path(),
        &[
            "check",
            "--json",
            "--base",
            "main",
            "--allowed-signers-rev",
            &spec,
        ],
    );
    assert_eq!(code, 0, "{out} {err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["data"]["trust"]["ssh_allowed_signers"]["rev"], "main");
    let head = format!("HEAD:{path}");
    let (code, out, _) = run(
        r.path(),
        &["check", "--json", "--allowed-signers-rev", &head],
    );
    assert_eq!(code, 1);
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(
        outcomes(&v),
        [row("T001", "unverifiable", "untrusted_signer")]
    );
}

#[test]
fn a_squash_merge_leaves_the_evidence_unreachable() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    r.tick("T001", "verified", Sign::Trusted);
    r.git(&["checkout", "-q", "main"]);
    r.git(&["merge", "-q", "--squash", "feature"]);
    r.commit("squashed feature");
    let (code, v) = check(&r, &[]);
    assert_eq!(code, 1);
    assert_eq!(
        outcomes(&v),
        [row("T001", "unverifiable", "commit_unreachable")]
    );
    assert!(
        v["data"]["ticks"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("merge commit")
    );
}

#[test]
fn a_bug_claim_this_version_cannot_check_fails_the_branch() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    r.tick("T001", "verified", Sign::Trusted);
    fs::write(r.path().join("src/fix.rs"), "pub fn fix() {}\n").unwrap();
    r.commit(&format!(
        "fix: empty title\n\nSpec-Task: {FEATURE}/bug:empty-title\nVerdict: verified\n"
    ));
    let (code, v) = check(&r, &["--base", "main"]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["data"]["conclusion"], "failure");
    assert!(
        v["data"]["not_evaluated"]
            .as_array()
            .unwrap()
            .contains(&json!("trailer_only_tasks"))
    );
}

#[test]
fn only_the_touched_units_are_checked() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    // A second unit, hand-ticked on main: legacy, untouched by the branch.
    r.git(&["checkout", "-q", "main"]);
    let other = r.path().join("specs/002-other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("spec.md"), "# Spec\n").unwrap();
    fs::write(other.join("tasks.md"), "# Tasks\n\n- [X] T001 Legacy\n").unwrap();
    r.commit("legacy unit");
    r.git(&["checkout", "-q", "feature"]);
    r.git(&["rebase", "-q", "main"]);
    r.tick("T001", "verified", Sign::Trusted);
    let (code, v) = check(&r, &["--base", "main"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["features"], json!([FEATURE]));
    let (code, v) = check(&r, &["--base", "main", "--all"]);
    assert_eq!(code, 1);
    assert_eq!(v["data"]["features"], json!([FEATURE, "002-other"]));
}

/// A directory of shims for `tools`, each logging its name and running the
/// real one; only these are on the `PATH`.
fn shims(dir: &std::path::Path, tools: &[&str]) -> std::path::PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let log = dir.join("spawned.log");
    for tool in tools {
        let real = std::process::Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .output()
            .unwrap();
        let real = String::from_utf8_lossy(&real.stdout).trim().to_owned();
        assert!(!real.is_empty(), "{tool} not found");
        let shim = bin.join(tool);
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\necho {tool} >> '{}'\nexec '{real}' \"$@\"\n",
                log.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

#[test]
fn it_reads_no_working_tree_and_spawns_only_git_and_the_signature_tool() {
    if !ssh() {
        return;
    }
    let (r, _) = mixed();
    let (_, before) = check(&r, &["--base", "main"]);
    // Every file of the checkout gone (git's objects stay): same result.
    for entry in fs::read_dir(r.path()).unwrap().flatten() {
        if entry.file_name() != ".git" {
            let p = entry.path();
            if p.is_dir() {
                fs::remove_dir_all(p).unwrap();
            } else {
                fs::remove_file(p).unwrap();
            }
        }
    }
    let tools = tempfile::tempdir().unwrap();
    let bin = shims(tools.path(), &["git", "ssh-keygen"]);
    let allowed = r.allowed.display().to_string();
    let out = std::process::Command::new(support::verifier())
        .args([
            "check",
            "--json",
            "--base",
            "main",
            "--allowed-signers",
            &allowed,
        ])
        .current_dir(r.path())
        .env("PATH", &bin)
        .output()
        .unwrap();
    let after: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(after["data"], before["data"]);
    let spawned = fs::read_to_string(tools.path().join("spawned.log")).unwrap();
    let names: std::collections::BTreeSet<&str> = spawned.lines().collect();
    assert_eq!(names, ["git", "ssh-keygen"].into_iter().collect());
}

#[test]
fn the_check_payload_annotates_each_tick_that_is_not_plainly_verified() {
    if !ssh() {
        return;
    }
    let (r, _) = mixed();
    let file = r.path().join("check.json");
    let md = r.path().join("check.md");
    let file_arg = file.display().to_string();
    let md_arg = md.display().to_string();
    let (code, _) = check(
        &r,
        &[
            "--base",
            "main",
            "--github-check",
            &file_arg,
            "--markdown",
            &md_arg,
        ],
    );
    assert_eq!(code, 1);
    let body: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let create = &body["create"];
    assert_eq!(create["name"], "speccoder / evidence");
    assert_eq!(create["head_sha"], r.git(&["rev-parse", "HEAD"]));
    assert_eq!(create["conclusion"], "failure");
    assert_eq!(
        create["output"]["title"],
        "failure: 1 verified, 1 regression only, 1 attested, 1 stale, 5 unverifiable"
    );
    let levels: Vec<&str> = create["output"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["annotation_level"].as_str().unwrap())
        .collect();
    assert_eq!(
        levels,
        [
            "warning", "notice", "failure", "failure", "failure", "failure", "failure", "warning"
        ]
    );
    let text = create["output"]["text"].as_str().unwrap();
    assert_eq!(text.matches("<details>").count(), 9, "a tour per tick");
    assert!(
        fs::read_to_string(&md)
            .unwrap()
            .contains("**verified 1 · regression only 1")
    );
}

#[test]
fn the_tour_of_a_verified_tick_walks_its_six_steps_from_the_signed_statements() {
    if !ssh() {
        return;
    }
    let r = Repo::new(3);
    let commit = r.tick("T001", "verified", Sign::Trusted);
    r.tick("T002", "attested", Sign::Trusted);
    let (code, out, err) = run(
        r.path(),
        &["tour", "--json", "--feature", FEATURE, "--task", "T001"],
    );
    assert_eq!(code, 0, "{err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["payload"], "evidence_tour");
    let steps = v["data"]["steps"].as_array().unwrap();
    let kinds: Vec<&str> = steps.iter().map(|s| s["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "requirement",
            "frozen_tests",
            "red",
            "diff",
            "green",
            "strength"
        ]
    );
    let sources: Vec<&str> = steps
        .iter()
        .map(|s| s["source"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        [
            "committed",
            "signed",
            "signed",
            "committed",
            "signed",
            "signed"
        ]
    );
    assert_eq!(v["data"]["commit"], commit);
    assert!(
        steps[1]["body_markdown"]
            .as_str()
            .unwrap()
            .contains("tests/t001.rs")
    );
    assert!(
        steps[1]["body_markdown"]
            .as_str()
            .unwrap()
            .contains("where decided: not recorded")
    );
    assert!(
        steps[2]["body_markdown"]
            .as_str()
            .unwrap()
            .contains("assertion failed: logged_in")
    );
    assert!(
        steps[4]["body_markdown"]
            .as_str()
            .unwrap()
            .contains("held-out passed")
    );
    assert!(
        steps[5]["body_markdown"]
            .as_str()
            .unwrap()
            .contains("combined 0.91")
    );
    // An attested tick says what is missing, in the verdict's words.
    let (_, out, _) = run(
        r.path(),
        &["tour", "--json", "--feature", FEATURE, "--task", "T002"],
    );
    let v: Value = serde_json::from_str(out.trim()).unwrap();
    let red = &v["data"]["steps"][2];
    assert_eq!(red["source"], "not_recorded");
    assert!(
        red["body_markdown"]
            .as_str()
            .unwrap()
            .contains("a person's word, not evidence")
    );
    // In Markdown, collapsed.
    let (_, md, _) = run(r.path(), &["tour", "--feature", FEATURE, "--task", "T001"]);
    assert!(md.starts_with("<details><summary><b>T001</b>"), "{md}");
    assert!(
        md.contains("3. **Red** · signed — failed: `tests/t001.rs` (assertion)"),
        "{md}"
    );
}

#[test]
fn the_badge_is_the_share_of_evidenced_ticks() {
    if !ssh() {
        return;
    }
    let r = Repo::new(10);
    for i in 1..=8 {
        r.tick(&format!("T{i:03}"), "verified", Sign::Trusted);
    }
    r.hand_tick("T009");
    r.hand_tick("T010");
    let svg = r.path().join("b.svg");
    let endpoint = r.path().join("b.json");
    let allowed = r.allowed.display().to_string();
    let (code, out, err) = run(
        r.path(),
        &[
            "badge",
            "--allowed-signers",
            &allowed,
            "--svg",
            &svg.display().to_string(),
            "--endpoint",
            &endpoint.display().to_string(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("80%"), "{out}");
    let e: Value = serde_json::from_str(&fs::read_to_string(&endpoint).unwrap()).unwrap();
    assert_eq!(
        e,
        json!({ "schemaVersion": 1, "label": "evidence", "message": "80%", "color": "yellow" })
    );
    assert!(fs::read_to_string(&svg).unwrap().contains(">80%<"));
}

#[test]
fn the_fallback_prints_workflow_commands_and_appends_the_step_summary() {
    if !ssh() {
        return;
    }
    let (r, _) = mixed();
    let summary = r.path().join("summary.md");
    let allowed = r.allowed.display().to_string();
    let out = std::process::Command::new(support::verifier())
        .args([
            "check",
            "--base",
            "main",
            "--github-annotations",
            "--allowed-signers",
            &allowed,
        ])
        .current_dir(r.path())
        .env("GITHUB_STEP_SUMMARY", &summary)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("::error file=specs/{FEATURE}/tasks.md,line=")),
        "{stdout}"
    );
    assert!(stdout.contains("::notice file="));
    assert!(
        fs::read_to_string(&summary)
            .unwrap()
            .contains("## speccoder / evidence: failure")
    );
}

#[test]
fn a_task_line_hash_is_checked_against_the_head() {
    if !ssh() {
        return;
    }
    let r = Repo::new(2);
    let commit = r.tick("T001", "verified", Sign::Trusted);
    let (code, _) = check(&r, &[]);
    assert_eq!(code, 0);
    // Rewording the ticked line after the tick: its signed hash no longer holds.
    r.edit_task_line("T001", " now");
    let (code, v) = check(&r, &[]);
    assert_eq!(code, 1);
    assert_eq!(v["data"]["ticks"][0]["commit"], commit);
    assert!(
        v["data"]["ticks"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("task line changed")
    );
}
