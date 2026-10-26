//! `speccoder acp` over stdio, as an editor drives it.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn an_editor_starts_a_session_and_runs_a_command() {
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
    for args in [
        &["init", "-q"][..],
        &["add", "-A"],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    let state = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_speccoder"))
        .arg("acp")
        .env("SPECCODER_STATE_DIR", state.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut send = move |v: Value| writeln!(stdin, "{v}").unwrap();
    let mut next = || -> Value { serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap() };

    send(
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": 1, "clientCapabilities": {} } }),
    );
    let init = next();
    assert_eq!(init["result"]["protocolVersion"], 1, "{init}");
    assert_eq!(init["result"]["agentInfo"]["name"], "speccoder");

    send(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": { "cwd": dir.path(), "mcpServers": [] } }),
    );
    let new = next();
    let session = new["result"]["sessionId"].as_str().unwrap().to_owned();
    let commands = next();
    assert_eq!(
        commands["params"]["update"]["sessionUpdate"],
        "available_commands_update"
    );
    let names: Vec<&str> = commands["params"]["update"]["availableCommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"run") && names.contains(&"specify"),
        "{names:?}"
    );

    send(
        json!({ "jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": { "sessionId": session, "prompt": [{ "type": "text", "text": "/report" }] } }),
    );
    let message = next();
    assert_eq!(message["method"], "session/update");
    let text = message["params"]["update"]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("T005"), "{text}");
    let done = next();
    assert_eq!(
        (done["id"].as_i64(), done["result"]["stopReason"].as_str()),
        (Some(3), Some("end_turn")),
        "{done}"
    );

    send(json!({ "jsonrpc": "2.0", "id": 4, "method": "no/such", "params": {} }));
    assert_eq!(next()["error"]["code"], -32601);
    drop(send);
    assert!(child.wait().unwrap().success());
}
