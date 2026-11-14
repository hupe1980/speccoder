//! `speccoder extensions` through the binary (spec 096): add a folder,
//! list what it contributes, trust it by digest in CI, remove it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
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
}

fn project() -> tempfile::TempDir {
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
    std::fs::write(
        dir.path().join("speccoder.toml"),
        "[model]\nprovider = \"openai\"\nmodel = \"m\"\n\n[checks.ok]\nargv = [\"true\"]\n",
    )
    .unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

fn kitchen() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speccoder-runtime/tests/fixtures/extensions/kitchen")
        .to_string_lossy()
        .into_owned()
}

fn speccoder(dir: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .args(args)
        .current_dir(dir)
        .env("SPECCODER_STATE_DIR", state)
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn none_says_how_to_add_one() {
    let p = project();
    let state = tempfile::tempdir().unwrap();
    let out = speccoder(p.path(), state.path(), &["extensions"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("speccoder extensions add ./path/to/folder"),
        "{text}"
    );
}

#[test]
fn add_list_trust_in_ci_and_remove() {
    let p = project();
    let state = tempfile::tempdir().unwrap();
    let added = json(&speccoder(
        p.path(),
        state.path(),
        &["extensions", "add", &kitchen(), "--json"],
    ));
    assert_eq!(added["status"], "ok", "{added}");
    assert_eq!(added["payload"], "extensions");
    assert_eq!(added["data"]["added"], "kitchen");
    // Not committed yet: named, and trusted only once it is.
    assert!(
        added["message"]
            .as_str()
            .unwrap()
            .contains("commit it, then trust it")
    );
    git(p.path(), &["add", "-A"]);
    git(p.path(), &["commit", "-q", "-m", "kitchen"]);
    let listed = json(&speccoder(
        p.path(),
        state.path(),
        &["extensions", "--json"],
    ));
    let e = &listed["data"]["extensions"][0];
    assert_eq!(e["trust"], "untrusted");
    let flag = e["trust_flag"].as_str().unwrap().to_owned();

    // The terminal listing names each contribution and how to trust it.
    let human = speccoder(p.path(), state.path(), &["extensions"]);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("gate before tick: runs ./scripts/check.sh — no network"),
        "{text}"
    );
    assert!(
        text.contains("trust it: speccoder extensions trust kitchen"),
        "{text}"
    );

    // Without a terminal, `trust` never trusts on its own.
    let asked = speccoder(p.path(), state.path(), &["extensions", "trust", "kitchen"]);
    assert_eq!(asked.status.code(), Some(23));

    // CI: the printed flag trusts it; a stale digest is refused.
    let stale = json(&speccoder(
        p.path(),
        state.path(),
        &[
            "extensions",
            "--json",
            "--trust-extension",
            "kitchen=sha256:00",
        ],
    ));
    assert_eq!(stale["code"], "EXTENSION-UNTRUSTED", "{stale}");
    let value = flag.trim_start_matches("--trust-extension ");
    let trusted = json(&speccoder(
        p.path(),
        state.path(),
        &["extensions", "--json", "--trust-extension", value],
    ));
    assert_eq!(
        trusted["data"]["extensions"][0]["trust"], "trusted",
        "{trusted}"
    );

    // Its hooks list as the extension's, trusted with it.
    let hooks = json(&speccoder(p.path(), state.path(), &["hooks", "--json"]));
    let names: Vec<&str> = hooks["data"]["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["kitchen.check", "kitchen.notify"]);
    assert_eq!(hooks["data"]["hooks"][0]["state"], "trusted");

    let removed = json(&speccoder(
        p.path(),
        state.path(),
        &["extensions", "remove", "kitchen", "--json"],
    ));
    assert_eq!(removed["status"], "ok", "{removed}");
    assert!(removed["data"]["extensions"].as_array().unwrap().is_empty());
}

/// A Spec Kit extension's optional hook is an action at its step of the
/// journey — once the extension is trusted, never before.
#[test]
fn a_spec_kit_extensions_optional_hook_is_offered_once_trusted() {
    let p = project();
    let r = p.path();
    let ext = r.join(".specify/extensions/ts");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(
        ext.join("extension.yml"),
        "schema_version: \"1.0\"\nextension:\n  id: ts\n  version: \"0.2.1\"\nprovides:\n  commands:\n    - name: speckit.ts.check\n      file: commands/check.md\nhooks:\n  before_implement:\n    command: speckit.ts.check\n    optional: true\n    prompt: \"Run the security gap check before implementing?\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(r.join(".claude/skills/speckit-ts-check")).unwrap();
    std::fs::write(
        r.join(".claude/skills/speckit-ts-check/SKILL.md"),
        "---\nname: speckit-ts-check\ndescription: Check the threats.\n---\n\nCheck.\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec kit extension"]);
    let state = tempfile::tempdir().unwrap();
    let offered = |extra: &[&str]| {
        let mut args = vec!["actions", "--json"];
        args.extend_from_slice(extra);
        let out = json(&speccoder(r, state.path(), &args));
        assert_eq!(out["status"], "ok", "{out}");
        out["data"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["label"] == "Run the security gap check before implementing")
    };
    assert!(!offered(&[]), "an untrusted extension offers nothing");
    let listed = json(&speccoder(r, state.path(), &["extensions", "--json"]));
    let e = &listed["data"]["extensions"][0];
    assert_eq!(e["source"], "spec_kit");
    let flag = e["trust_flag"]
        .as_str()
        .unwrap()
        .trim_start_matches("--trust-extension ")
        .to_owned();
    let trusted = json(&speccoder(
        r,
        state.path(),
        &["extensions", "--json", "--trust-extension", &flag],
    ));
    assert_eq!(
        trusted["data"]["extensions"][0]["trust"], "trusted",
        "{trusted}"
    );
    assert!(offered(&[]), "offered at its step once trusted");
}
