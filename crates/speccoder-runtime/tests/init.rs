//! `speccoder init` in real directories.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use speccoder_core::{CommandData, CommandStatus};
use speccoder_runtime::init::{Found, detect, init_with};

fn no_keys(_: &str) -> Option<String> {
    None
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

    let res = init_with(r, None, None, None, false, None, &no_keys);
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
    let again = init_with(r, None, None, None, false, None, &no_keys);
    assert_eq!(again.status(), CommandStatus::ConfigError);
    // It exists: change it, or start over — never "init" again.
    assert_eq!(again.code().map(|c| c.name), Some("CONFIG-EXISTS"));
    assert_eq!(again.fix(), ["speccoder config", "speccoder init --force"]);
    let forced = init_with(r, Some("openai"), None, None, true, None, &no_keys);
    assert_eq!(forced.status(), CommandStatus::Ok, "{}", forced.message());
    assert!(
        fs::read_to_string(r.join("speccoder.toml"))
            .unwrap()
            .contains("model = \"gpt-5.4-mini\"")
    );
    // An unknown provider is a usage error.
    assert_eq!(
        init_with(r, Some("acme"), None, None, true, None, &no_keys).status(),
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
    let res = init_with(r, None, None, None, false, Some(found), &no_keys);
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
    let res = init_with(dir.path(), None, None, None, false, None, &no_keys);
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

/// A view of the environment: these variables, and a home whose
/// `.aws/config` has an `aws login` profile `dev` in `us-west-2`.
fn aws_env(
    home: &std::path::Path,
    vars: &[(&str, &str)],
) -> impl Fn(&str) -> Option<String> + use<> {
    fs::create_dir_all(home.join(".aws")).unwrap();
    fs::write(
        home.join(".aws/config"),
        "[profile dev]\nlogin_session = arn:aws:iam::123456789012:user/me\nregion = us-west-2\n[profile bare]\nlogin_session = arn:aws:iam::1:user/x\n",
    )
    .unwrap();
    let mut vars: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    vars.push(("HOME".to_owned(), home.to_string_lossy().into_owned()));
    move |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
}

fn report(r: &speccoder_core::CommandResult) -> &speccoder_core::InitReport {
    match r.data() {
        Some(CommandData::Init(i)) => i,
        _ => panic!("{}", r.message()),
    }
}

#[test]
fn bedrock_is_chosen_when_aws_credentials_are_the_only_ones_found() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let env = aws_env(home.path(), &[("AWS_PROFILE", "dev")]);
    let res = init_with(dir.path(), None, None, None, false, None, &env);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let text = fs::read_to_string(dir.path().join("speccoder.toml")).unwrap();
    assert!(
        text.contains("# The model. Bedrock's credentials come from AWS — a Bedrock API key in\n# AWS_BEARER_TOKEN_BEDROCK, or a profile (aws login, SSO) — never from this file.\n[model]\nprovider = \"bedrock\"\nmodel = \"global.anthropic.claude-sonnet-5-5\"\nregion = \"us-west-2\"\n"),
        "{text}"
    );
    let m = speccoder_runtime::substrate::model_config::parse(&text)
        .unwrap()
        .unwrap();
    assert_eq!(m.region.as_deref(), Some("us-west-2"));
    let i = report(&res);
    assert_eq!(i.model, "bedrock/global.anthropic.claude-sonnet-5-5");
    assert_eq!(i.region.as_deref(), Some("us-west-2"));
    assert!(i.key.set);
    assert_eq!(
        i.key.source.as_deref(),
        Some("aws login session (profile dev)")
    );

    // A key of Anthropic or OpenAI keeps today's choice.
    let both = aws_env(
        home.path(),
        &[("AWS_PROFILE", "dev"), ("ANTHROPIC_API_KEY", "k")],
    );
    let res = init_with(dir.path(), None, None, None, true, None, &both);
    assert_eq!(report(&res).model, "anthropic/claude-sonnet-5-5");
}

#[test]
fn bedrock_takes_the_flags_and_without_a_region_says_to_set_one() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let keyed = aws_env(
        home.path(),
        &[
            ("AWS_BEARER_TOKEN_BEDROCK", "sk-test-secret-123"),
            ("AWS_REGION", "eu-west-1"),
        ],
    );
    let res = init_with(
        dir.path(),
        Some("bedrock"),
        Some("us.anthropic.claude-haiku-4-5"),
        Some("eu-central-1"),
        false,
        None,
        &keyed,
    );
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let text = fs::read_to_string(dir.path().join("speccoder.toml")).unwrap();
    assert!(
        text.contains("model = \"us.anthropic.claude-haiku-4-5\"\nregion = \"eu-central-1\"\n"),
        "{text}"
    );
    assert_eq!(
        report(&res).key.source.as_deref(),
        Some("Bedrock API key (AWS_BEARER_TOKEN_BEDROCK)")
    );
    assert!(
        !serde_json::to_string(&res)
            .unwrap()
            .contains("sk-test-secret-123")
    );
    assert!(!text.contains("sk-test-secret-123"));

    // No region anywhere: written commented out, and said.
    let bare = aws_env(home.path(), &[("AWS_PROFILE", "bare")]);
    let res = init_with(dir.path(), Some("bedrock"), None, None, true, None, &bare);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert!(
        res.message()
            .ends_with("no AWS region was found — speccoder config set model.region <region>"),
        "{}",
        res.message()
    );
    let text = fs::read_to_string(dir.path().join("speccoder.toml")).unwrap();
    assert!(
        text.contains("# region = \"…\"  # required: set it\n"),
        "{text}"
    );
    assert!(report(&res).region.is_none());
    let e = speccoder_runtime::substrate::model_config::parse(&text).unwrap_err();
    assert!(e.contains("speccoder config set model.region"), "{e}");

    // --region is Bedrock's.
    let r = init_with(
        dir.path(),
        Some("openai"),
        None,
        Some("us-east-1"),
        true,
        None,
        &bare,
    );
    assert_eq!(r.status(), CommandStatus::UsageError, "{}", r.message());
}

fn git_init(dir: &std::path::Path) {
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn claude_codes_linked_folders_are_proposed_for_worktree_link() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    git_init(r);
    fs::write(r.join(".gitignore"), "node_modules/\n.venv/\n").unwrap();
    fs::create_dir_all(r.join(".claude")).unwrap();
    // Ignored ones only; nothing outside the project, nothing tracked.
    fs::write(
        r.join(".claude/settings.json"),
        r#"{ "worktree": { "symlinkDirectories": ["node_modules", ".venv/", "dist", "../shared", ".git"] } }"#,
    )
    .unwrap();
    assert_eq!(detect(r).link, ["node_modules", ".venv"]);
    let res = init_with(r, None, None, None, false, None, &no_keys);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let Some(CommandData::Init(i)) = res.data() else {
        panic!()
    };
    assert_eq!(i.link, ["node_modules", ".venv"]);
    assert!(!i.link_active);
    let text = fs::read_to_string(r.join("speccoder.toml")).unwrap();
    assert!(
        text.contains("# [worktree]\n# link = [\"node_modules\", \".venv\"]\n"),
        "{text}"
    );
    // Written commented: not in force until confirmed.
    assert!(
        speccoder_runtime::prepare::parse(&text)
            .unwrap()
            .link
            .is_empty()
    );
    speccoder_runtime::init::activate_link(r).unwrap();
    let text = fs::read_to_string(r.join("speccoder.toml")).unwrap();
    assert_eq!(
        speccoder_runtime::prepare::parse(&text).unwrap().link,
        ["node_modules", ".venv"]
    );
    assert!(speccoder_runtime::settings::validate(&text).is_ok());
    assert!(speccoder_runtime::init::activate_link(r).is_err(), "once");
}

#[test]
fn without_claude_codes_setting_worktree_is_explained_commented() {
    let dir = tempfile::tempdir().unwrap();
    git_init(dir.path());
    let res = init_with(dir.path(), None, None, None, false, None, &no_keys);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    let text = fs::read_to_string(dir.path().join("speccoder.toml")).unwrap();
    assert!(
        text.contains("# [worktree]\n# prepare = \"auto\""),
        "{text}"
    );
    assert!(text.contains("\n# link = [\"node_modules\"]"), "{text}");
    assert_eq!(
        speccoder_runtime::prepare::parse(&text).unwrap(),
        speccoder_runtime::prepare::Config::default()
    );
    assert!(speccoder_runtime::init::activate_link(dir.path()).is_err());
}
