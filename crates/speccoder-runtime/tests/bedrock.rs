//! Amazon Bedrock as the model provider: the POC fixture under
//! `provider = "bedrock"` with a scripted model — the prompts it is handed,
//! the verdict, redaction, strict replay and a refused session's renewal —
//! and, by hand, one real task (`SPECCODER_REAL_BEDROCK=1`).

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agentplane::model::ModelProvider;
use agentplane::model::fake::FakeProvider;
use serde_json::{Value, json};
use speccoder_core::verify::Verdict;
use speccoder_core::{CommandData, CommandResult, CommandStatus, RunReport};
use speccoder_runtime::run_with;

const CLAUDE: &str = "global.anthropic.claude-sonnet-5-5";

fn has(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args([
            "-c",
            "init.defaultBranch=main",
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
}

/// `[model]` for Bedrock, and the POC's checks.
fn config(model: &str, region: &str) -> String {
    format!(
        "[model]\nprovider = \"bedrock\"\nmodel = \"{model}\"\nregion = \"{region}\"\n\n[checks.test]\nargv = [\"cargo\", \"nextest\", \"run\", \"--profile\", \"speccoder\", \"--no-tests=warn\"]\njunit = \"target/nextest/speccoder/junit.xml\"\n\n[commands]\nallow = [[\"cargo\", \"test\"], [\"cargo\", \"check\"], [\"cargo\", \"build\"]]\n\n[acceptance]\nheld_out = false\n"
    )
}

/// The POC at its planning commit, as a committed repository.
fn poc(model: &str) -> tempfile::TempDir {
    poc_in(model, "eu-central-1")
}

fn poc_in(model: &str, region: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/poc-greet");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture.join("."))
            .arg(r)
            .status()
            .unwrap()
            .success()
    );
    fs::rename(r.join("Cargo.toml.fixture"), r.join("Cargo.toml")).unwrap();
    fs::write(r.join("speccoder.toml"), config(model, region)).unwrap();
    fs::create_dir_all(r.join(".config")).unwrap();
    fs::write(
        r.join(".config/nextest.toml"),
        "[profile.speccoder.junit]\npath = \"junit.xml\"\n",
    )
    .unwrap();
    fs::write(r.join(".gitignore"), "target\n").unwrap();
    assert!(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(r)
            .status()
            .unwrap()
            .success()
    );
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "spec(001-greet-function)"]);
    dir
}

fn done(summary: &str) -> Value {
    json!({ "summary": summary, "status": "done", "files_changed": [], "checks_run": [], "notes": [] })
}

fn report(r: &CommandResult) -> &RunReport {
    match r.data() {
        Some(CommandData::Run(rep)) => rep,
        other => panic!("{} → {other:?}", r.message()),
    }
}

fn run(repo: &Path, state: &Path, f: &Arc<FakeProvider>, secret: Option<&str>) -> CommandResult {
    let d: Arc<dyn ModelProvider> = f.clone();
    run_with(repo, None, "T001", state, d, secret)
}

/// Every `cache` mark in a prompt handed to the driver.
fn marks(prompt: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["system", "input"] {
        for part in prompt[key].as_array().into_iter().flatten() {
            if let Some(m) = part["cache"].as_str() {
                out.push(m.to_owned());
            }
        }
    }
    out
}

#[test]
fn a_claude_model_on_bedrock_gets_the_shared_shape_with_cache_marks() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let r = poc(CLAUDE);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(done("the crate structure is in place"));
    let res = run(r.path(), state.path(), &f, None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::RegressionOnly));
    let asked = f.asked();
    let prompt = &asked[0].prompt;
    assert!(prompt.get("messages").is_none(), "{prompt}");
    assert!(
        prompt["input"].as_array().is_some_and(|parts| parts
            .iter()
            .all(|p| p["type"] == "text" && p.get("cache_control").is_none())),
        "{prompt}"
    );
    // The constitution and the plan close a cached prefix each.
    assert_eq!(marks(prompt), ["5m", "5m"], "{prompt}");
    assert!(!prompt.to_string().contains("cache_control"));

    // Replayed strictly from its journal: no driver, so no credentials.
    let calls = f.calls();
    let run_id = report(&res).run_id.clone();
    let replayed = speccoder_runtime::replay_with(r.path(), &run_id, state.path());
    assert_eq!(
        replayed.status(),
        CommandStatus::Ok,
        "{}",
        replayed.message()
    );
    assert_eq!(f.calls(), calls, "replay called the model");
}

#[test]
fn another_family_on_bedrock_gets_no_cache_marks() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let r = poc("amazon.nova-pro-v1:0");
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_structure(done("the crate structure is in place"));
    let res = run(r.path(), state.path(), &f, None);
    assert_eq!(res.status(), CommandStatus::Ok, "{}", res.message());
    assert_eq!(report(&res).verdict, Some(Verdict::RegressionOnly));
    let prompt = &f.asked()[0].prompt;
    assert!(prompt["input"].is_array(), "{prompt}");
    assert!(marks(prompt).is_empty(), "{prompt}");
    assert!(!prompt.to_string().contains("cache_control"));
}

#[test]
fn a_bedrock_api_key_never_reaches_a_message_or_an_event() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    const KEY: &str = "sk-test-secret-123";
    let r = poc(CLAUDE);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_fail(agentplane::model::ModelError::Refused {
        model: agentplane::model::ModelId::new("bedrock", CLAUDE),
        detail: format!("UnrecognizedClientException: The security token {KEY} is invalid"),
        code: Some("UnrecognizedClientException".to_owned()),
    });
    let res = run(r.path(), state.path(), &f, Some(KEY));
    assert_ne!(res.status(), CommandStatus::Ok, "{}", res.message());
    let shown = serde_json::to_string(&res).unwrap();
    assert!(!shown.contains(KEY), "{shown}");
    // Nothing speccoder wrote under the state directory holds it either:
    // events, attempts, the run's records. The journal keeps the provider's
    // failure detail as agentplane records it, unredacted (AWS's errors do
    // not echo the key; a scripted one does).
    let mut stack = vec![state.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "journal.redb") {
                continue;
            } else if let Ok(bytes) = fs::read(&p) {
                assert!(
                    !bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()),
                    "{} holds the key",
                    p.display()
                );
            }
        }
    }
}

/// Names the test a process of its own runs (see [`in_child`]).
const CHILD: &str = "SPECCODER_TEST_BEDROCK_CHILD";

/// Runs the ignored test `name` in a process of its own, whose environment
/// has no `AWS_*` variable but `vars` — this process's cannot be changed.
fn in_child(name: &str, vars: &[(&str, &Path)]) {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args([name, "--exact", "--ignored", "--nocapture"]);
    for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("AWS_")) {
        child.env_remove(k);
    }
    for (k, v) in vars {
        child.env(k, v);
    }
    let out = child.env(CHILD, name).output().unwrap();
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Whether this process is the one [`in_child`] started for `name`.
fn child(name: &str) -> bool {
    std::env::var(CHILD).as_deref() == Ok(name)
}

#[test]
fn a_refused_aws_login_session_names_the_renewal() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let aws = tempfile::tempdir().unwrap();
    let config = aws.path().join("config");
    fs::write(
        &config,
        "[profile dev]\nlogin_session = arn:aws:iam::123456789012:user/me\nregion = eu-central-1\n",
    )
    .unwrap();
    in_child(
        "refused_session_child",
        &[
            ("AWS_PROFILE", Path::new("dev")),
            ("AWS_CONFIG_FILE", &config),
            (
                "AWS_SHARED_CREDENTIALS_FILE",
                &aws.path().join("credentials"),
            ),
        ],
    );
}

#[test]
#[ignore = "run by a_refused_aws_login_session_names_the_renewal, in a process of its own"]
fn refused_session_child() {
    if !child("refused_session_child") {
        return;
    }
    let r = poc(CLAUDE);
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    // Access denied, as agentplane's Bedrock driver reports it: refused,
    // the account's by its code.
    f.will_fail(agentplane::model::ModelError::Refused {
        model: agentplane::model::ModelId::new("bedrock", CLAUDE),
        detail:
            "AccessDeniedException: You don't have access to the model with the specified model ID."
                .to_owned(),
        code: Some("AccessDeniedException".to_owned()),
    });
    let res = run(r.path(), state.path(), &f, None);
    assert_eq!(res.status(), CommandStatus::RunFailed, "{}", res.message());
    assert_eq!(
        res.code().map(|c| c.name),
        Some("PROVIDER-REFUSED"),
        "{}",
        res.message()
    );
    assert_eq!(
        res.fix().first().map(String::as_str),
        Some("aws login --profile dev"),
        "{:?}",
        res.fix()
    );
    let rep = report(&res);
    assert!(!rep.ticked && rep.verdict.is_none());
    assert!(rep.saved_attempt.is_some(), "the attempt is saved");
}

#[test]
fn a_model_the_region_does_not_offer_names_the_region_and_the_model() {
    if !has("cargo-nextest") {
        eprintln!("skipped: cargo-nextest is not installed");
        return;
    }
    let r = poc_in("anthropic.claude-sonnet-5-5", "us-gov-west-1");
    let state = tempfile::tempdir().unwrap();
    let f = FakeProvider::new();
    f.will_fail(agentplane::model::ModelError::Refused {
        model: agentplane::model::ModelId::new("bedrock", "anthropic.claude-sonnet-5-5"),
        detail: "ValidationException: The provided model identifier is invalid.".to_owned(),
        code: Some("ValidationException".to_owned()),
    });
    let res = run(r.path(), state.path(), &f, None);
    assert_eq!(res.status(), CommandStatus::RunFailed, "{}", res.message());
    assert!(
        res.message().contains("region us-gov-west-1")
            && res.message().contains("model anthropic.claude-sonnet-5-5"),
        "{}",
        res.message()
    );
    // One request, not the account: not a refusal of the credentials.
    assert_ne!(res.code().map(|c| c.name), Some("PROVIDER-REFUSED"));
}

#[test]
fn the_driver_opens_from_an_api_key_and_from_nothing_it_can_see() {
    let aws = tempfile::tempdir().unwrap();
    let none = aws.path().join("none");
    let files: [(&str, &Path); 4] = [
        ("AWS_PROFILE", Path::new("speccoder-test-no-such-profile")),
        ("AWS_CONFIG_FILE", &none),
        ("AWS_SHARED_CREDENTIALS_FILE", &none),
        // No instance metadata is asked for here, nor anywhere else.
        ("AWS_EC2_METADATA_DISABLED", Path::new("true")),
    ];
    in_child("driver_child", &files);
    let mut keyed = files.to_vec();
    keyed.extend([
        ("AWS_BEARER_TOKEN_BEDROCK", Path::new("sk-test-secret-123")),
        // A real call, to a port nothing listens on, tried once.
        ("AWS_ENDPOINT_URL", Path::new("http://127.0.0.1:9")),
        ("AWS_MAX_ATTEMPTS", Path::new("1")),
    ]);
    in_child("keyed_driver_child", &keyed);
}

fn bedrock(model: &str) -> speccoder_runtime::substrate::model_config::ModelConfig {
    speccoder_runtime::substrate::model_config::parse(&format!(
        "[model]\nprovider = \"bedrock\"\nmodel = \"{model}\"\nregion = \"eu-central-1\"\n"
    ))
    .unwrap()
    .unwrap()
}

#[test]
#[ignore = "run by the_driver_opens_from_an_api_key_and_from_nothing_it_can_see"]
fn driver_child() {
    if !child("driver_child") {
        return;
    }
    use speccoder_runtime::substrate::model_config::open_driver;
    // Nothing speccoder can see offline: the driver still opens, since the
    // AWS chain may find what it cannot (an instance role) — the first call
    // decides. `doctor` is where the missing source is KEY-MISSING.
    let (_, key) = open_driver(&bedrock(CLAUDE)).unwrap();
    assert_eq!(key, None);
}

#[test]
#[ignore = "run by the_driver_opens_from_an_api_key_and_from_nothing_it_can_see"]
fn keyed_driver_child() {
    if !child("keyed_driver_child") {
        return;
    }
    use speccoder_runtime::substrate::model_config::open_driver;
    let (driver, key) = open_driver(&bedrock(CLAUDE)).unwrap();
    // The key is the secret every message is redacted of.
    assert_eq!(key.as_deref(), Some("sk-test-secret-123"));
    let id = agentplane::model::ModelId::new("bedrock", CLAUDE);
    assert_eq!(driver.request_profile(&id)["region"], "eu-central-1");
    // Opened from inside a runtime too (the web host's threads).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let inside = rt.block_on(async { open_driver(&bedrock("amazon.nova-pro-v1:0")).map(|_| ()) });
    assert_eq!(inside, Ok(()));
    // A failed call: the key is in no result, nor anywhere in the state
    // directory, the journal included.
    let r = poc(CLAUDE);
    let state = tempfile::tempdir().unwrap();
    let res = speccoder_runtime::propose_with(
        r.path(),
        None,
        "T001",
        state.path(),
        driver,
        key.as_deref(),
    );
    assert_eq!(res.status(), CommandStatus::RunFailed, "{}", res.message());
    // The connection failed: the driver took the prompt `to_parts` made
    // and sent it. A prompt it could not shape is refused before sending
    // ("system instruction must be a string or a list of text parts").
    assert!(
        res.message().contains("dispatch failure") && res.message().contains("tcp connect error"),
        "{}",
        res.message()
    );
    assert!(
        !res.message().contains("refused the request"),
        "{}",
        res.message()
    );
    assert!(
        !serde_json::to_string(&res)
            .unwrap()
            .contains("sk-test-secret-123")
    );
    let mut stack = vec![state.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(bytes) = fs::read(&p) {
                assert!(
                    !bytes.windows(18).any(|w| w == b"sk-test-secret-123"),
                    "{} holds the key",
                    p.display()
                );
            }
        }
    }
}

/// One task on Bedrock for real: `SPECCODER_REAL_BEDROCK=1`, AWS
/// credentials (an API key or a profile), `AWS_REGION` (default
/// `us-east-1`).
#[test]
#[ignore = "calls Amazon Bedrock: SPECCODER_REAL_BEDROCK=1 and AWS credentials"]
fn bedrock_runs_a_task_for_real() {
    if std::env::var("SPECCODER_REAL_BEDROCK").as_deref() != Ok("1") {
        eprintln!("skipped: SPECCODER_REAL_BEDROCK=1 is not set");
        return;
    }
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_owned());
    let r = poc_in(CLAUDE, &region);
    let state = tempfile::tempdir().unwrap();
    let text = fs::read_to_string(r.path().join("speccoder.toml")).unwrap();
    let model = speccoder_runtime::substrate::model_config::parse(&text)
        .unwrap()
        .unwrap();
    let (driver, key) = speccoder_runtime::substrate::model_config::open_driver(&model).unwrap();
    let res = run_with(r.path(), None, "T001", state.path(), driver, key.as_deref());
    let rep = report(&res);
    // What the model chose is its own: a verdict, or a call it stopped to
    // ask about. What this checks is that Bedrock answered.
    assert!(
        rep.verdict.is_some() || res.message().contains("needs approval"),
        "{:?}: {}",
        res.code().map(|c| c.name),
        res.message()
    );
    // The tools it called: a tool answering with a list (list_files,
    // search_code) must go through Bedrock as well as one answering with an
    // object.
    let tools: Vec<&str> = rep
        .tool_calls
        .iter()
        .filter_map(|c| c["tool"].as_str().or_else(|| c["name"].as_str()))
        .collect();
    eprintln!("tools: {tools:?}");
    // A finished attempt reports its tokens; one stopped to ask reports
    // them when it finishes, and has its run in the journal now.
    if rep.verdict.is_some() {
        let usage = rep
            .usage
            .as_ref()
            .expect("a finished attempt reports its usage");
        eprintln!("usage: {usage}");
        assert!(usage["input_tokens"].as_u64().unwrap_or(0) > 0, "{usage}");
    } else {
        assert!(!rep.run_id.is_empty(), "{}", res.message());
    }
}

#[test]
fn doctor_names_the_region_and_the_source_never_a_value() {
    use speccoder_core::DoctorLevel;
    let r = poc(CLAUDE);
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".aws")).unwrap();
    fs::write(
        home.path().join(".aws/config"),
        "[profile dev]\nlogin_session = arn:aws:iam::123456789012:user/me\n",
    )
    .unwrap();
    let h = home.path().to_string_lossy().into_owned();
    let rows = |vars: &[(&str, &str)]| {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .chain([("HOME".to_owned(), h.clone())])
            .collect();
        speccoder_runtime::doctor::checks(r.path(), None, &move |k| {
            vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        })
    };
    let row = |checks: &[speccoder_core::DoctorCheck], name: &str| {
        checks.iter().find(|c| c.check == name).cloned()
    };

    let login = rows(&[("AWS_PROFILE", "dev")]);
    let key = row(&login, "model key").unwrap();
    assert_eq!(key.level, DoctorLevel::Pass);
    assert_eq!(
        key.detail,
        "bedrock global.anthropic.claude-sonnet-5-5 · eu-central-1 · aws login session (profile dev)"
    );
    assert!(row(&login, "model region").is_none());

    let both = rows(&[
        ("AWS_PROFILE", "dev"),
        ("AWS_BEARER_TOKEN_BEDROCK", "sk-test-secret-123"),
        ("AWS_REGION", "us-west-2"),
    ]);
    let key = row(&both, "model key").unwrap();
    assert!(
        key.detail.ends_with(
            "Bedrock API key (AWS_BEARER_TOKEN_BEDROCK) (the API key is used; profile dev is not)"
        ),
        "{}",
        key.detail
    );
    let region = row(&both, "model region").unwrap();
    assert_eq!(region.level, DoctorLevel::Warn);
    assert_eq!(
        region.detail,
        "eu-central-1 in speccoder.toml, AWS_REGION is us-west-2 — the file is used"
    );
    assert!(
        !serde_json::to_string(&both)
            .unwrap()
            .contains("sk-test-secret-123")
    );

    let none = rows(&[("AWS_PROFILE", "gone")]);
    let key = row(&none, "model key").unwrap();
    assert_eq!(key.level, DoctorLevel::Fail);
    assert_eq!(key.code.as_deref(), Some("KEY-MISSING"));
    assert_eq!(
        key.detail,
        "AWS_BEARER_TOKEN_BEDROCK is not set and no AWS profile was found"
    );
    assert_eq!(
        key.fix[..2],
        ["export AWS_BEARER_TOKEN_BEDROCK=…", "aws login"]
    );
}

/// A tool result that is not a JSON object — a list, and a failed call —
/// reaches real Bedrock and is answered (Converse refused both before
/// agentplane 0.55).
#[test]
#[ignore = "calls real Bedrock: SPECCODER_REAL_BEDROCK=1"]
fn bedrock_takes_a_list_and_a_failed_tool_result() {
    use agentplane::model::{ModelId, Request, ToolCall, ToolDeclaration, ToolExchange};
    if std::env::var("SPECCODER_REAL_BEDROCK").as_deref() != Ok("1") {
        eprintln!("skipped: SPECCODER_REAL_BEDROCK=1 is not set");
        return;
    }
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_owned());
    let model = speccoder_runtime::substrate::model_config::parse(&format!(
        "[model]\nprovider = \"bedrock\"\nmodel = \"{CLAUDE}\"\nregion = \"{region}\"\n"
    ))
    .unwrap()
    .unwrap();
    let (driver, _) = speccoder_runtime::substrate::model_config::open_driver(&model).unwrap();
    let id = ModelId::new("bedrock", CLAUDE);
    let prompt = serde_json::json!({
        "system": "Answer in one short sentence.",
        "input": [{ "type": "text", "text": "List the files, then read README.md, and say how many files there are." }]
    });
    let tools = [
        ToolDeclaration::new(
            "list_files",
            "List the files.",
            serde_json::json!({"type": "object", "properties": {}}),
        ),
        ToolDeclaration::new(
            "read_file",
            "Read a file.",
            serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
        ),
    ];
    let call = |id: &str, name: &str, args: serde_json::Value| ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: args,
    };
    let exchanges = [
        ToolExchange::ok(
            call("t1", "list_files", serde_json::json!({})),
            serde_json::json!(["README.md", "src/lib.rs", "Cargo.toml"]),
        ),
        ToolExchange::failed(
            call("t2", "read_file", serde_json::json!({"path": "README.md"})),
            "README.md: no such file",
        ),
    ];
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let answer = rt.block_on(driver.complete(Request {
        model: &id,
        prompt: &prompt,
        max_output_tokens: 200,
        reasoning_effort: None,
        reasoning_summary: false,
        schema: None,
        tools: &tools,
        exchanges: &exchanges,
        told: &[],
        continuation: None,
        compaction: None,
        stream: None,
    }));
    let answer =
        answer.unwrap_or_else(|e| panic!("Bedrock refused a list or a failed tool result: {e}"));
    eprintln!("answer: {}", answer.text);
}
