//! `speccoder init` in real directories.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::init::{Found, detect, init_with};

fn no_keys(_: &str) -> bool {
    false
}

#[test]
fn detects_a_node_project_and_writes_a_file_speccoder_accepts() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::write(
        r.join("package.json"),
        r#"{ "scripts": { "test": "vitest run", "lint": "eslint .", "dev": "vite" } }"#,
    )
    .unwrap();
    fs::write(r.join("pnpm-lock.yaml"), "").unwrap();
    assert_eq!(detect(r).node_manager.as_deref(), Some("pnpm"));

    let res = init_with(r, None, None, false, None, &no_keys);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let text = fs::read_to_string(r.join("speccoder.toml")).unwrap();
    let checks = speccoder_runtime::verify::config::parse(&text).unwrap();
    let shown: Vec<String> = checks
        .iter()
        .map(|c| format!("{} {}", c.name, c.argv.join(" ")))
        .collect();
    assert_eq!(shown, ["test pnpm run test", "lint pnpm run lint"]);
    assert!(text.contains("provider = \"anthropic\""));
    // The commands the model and the acceptance tests need, narrowly.
    let run = speccoder_runtime::substrate::run_config::parse(&text).unwrap();
    assert_eq!(
        run.allow,
        [
            vec!["pnpm".to_owned(), "test".to_owned()],
            vec!["pnpm".to_owned(), "run".to_owned(), "test".to_owned()]
        ]
    );
    let Some(CommandData::Init(i)) = res.data() else {
        panic!()
    };
    // `node_modules/` too: npm's output, ignored from the start.
    assert_eq!(i.wrote, ["speccoder.toml", ".gitignore"]);

    // An existing file is not replaced without --force.
    let again = init_with(r, None, None, false, None, &no_keys);
    assert_eq!(again.status(), CommandStatus::ConfigError);
    // It exists: change it, or start over — never "init" again.
    assert_eq!(again.code().map(|c| c.name), Some("CONFIG-EXISTS"));
    assert_eq!(again.fix(), ["speccoder config", "speccoder init --force"]);
    let forced = init_with(r, Some("openai"), None, true, None, &no_keys);
    assert_eq!(forced.status(), CommandStatus::Ok, "{}", forced.message());
    assert!(
        fs::read_to_string(r.join("speccoder.toml"))
            .unwrap()
            .contains("model = \"gpt-5.4-mini\"")
    );
    // An unknown provider is a usage error.
    assert_eq!(
        init_with(r, Some("acme"), None, true, None, &no_keys).status(),
        CommandStatus::UsageError
    );
}

#[test]
fn a_nextest_check_comes_with_its_profile_and_keeps_an_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    fs::write(r.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    fs::create_dir_all(r.join(".config")).unwrap();
    fs::write(
        r.join(".config/nextest.toml"),
        "[profile.default]\nretries = 0\n",
    )
    .unwrap();
    let found = Found {
        cargo: true,
        nextest: true,
        ..Found::default()
    };
    let res = init_with(r, None, None, false, Some(found), &no_keys);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let nextest = fs::read_to_string(r.join(".config/nextest.toml")).unwrap();
    assert!(
        nextest.starts_with("[profile.default]\nretries = 0\n"),
        "{nextest}"
    );
    assert!(nextest.contains("[profile.speccoder.junit]\npath = \"junit.xml\""));
    let Some(CommandData::Init(i)) = res.data() else {
        panic!()
    };
    // `target/` too: cargo's output, ignored from the start.
    assert_eq!(
        i.wrote,
        ["speccoder.toml", ".config/nextest.toml", ".gitignore"]
    );
    // Run again with --force: the profile is not added twice.
    init_with(
        r,
        None,
        None,
        true,
        Some(Found {
            cargo: true,
            nextest: true,
            ..Found::default()
        }),
        &no_keys,
    );
    assert_eq!(
        fs::read_to_string(r.join(".config/nextest.toml"))
            .unwrap()
            .matches("[profile.speccoder]")
            .count(),
        1
    );
}

#[test]
fn nothing_detected_still_writes_a_valid_file_and_says_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let res = init_with(dir.path(), None, None, false, None, &no_keys);
    assert_eq!(res.status(), CommandStatus::Ok);
    assert!(
        res.message().contains("without checks"),
        "{}",
        res.message()
    );
    let text = fs::read_to_string(dir.path().join("speccoder.toml")).unwrap();
    assert!(
        speccoder_runtime::verify::config::parse(&text)
            .unwrap()
            .is_empty()
    );
}
