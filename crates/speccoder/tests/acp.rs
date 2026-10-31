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

/// Words without a `/` are an Ask turn: reads as tool calls, the answer
/// streamed, each source a resource link, a proposal a permission request
/// — and "Not now" records nothing.
#[test]
fn a_question_is_an_ask_turn_and_only_record_records() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
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
    // The model: reads notes.txt, proposes a quick change, answers citing it.
    let mock_dir = tempfile::tempdir().unwrap();
    let propose = json!({
        "kind": "quick_change", "title": "Number the notes", "target": null,
        "request": "Number every line of notes.txt.", "checks": [], "note": null, "answer": null,
        "description": null, "symptom": null, "expected": null, "unchanged": null
    });
    std::fs::write(
        mock_dir.path().join("script.json"),
        json!([
            { "tool": "read_file", "args": { "path": "notes.txt" } },
            { "tool": "propose", "args": propose },
            { "content": "It counts to two [r1]." },
        ])
        .to_string(),
    )
    .unwrap();
    let mut mock = Command::new("python3")
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_mock.py"))
        .arg(mock_dir.path())
        .arg(mock_dir.path().join("script.json"))
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    while !mock_dir.path().join("port").exists() && start.elapsed().as_secs() < 20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let port = std::fs::read_to_string(mock_dir.path().join("port")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "one\ntwo\n").unwrap();
    std::fs::write(
        dir.path().join("speccoder.toml"),
        format!("[checks.ok]\nargv = [\"true\"]\n\n[model]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\n"),
    )
    .unwrap();
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
    next();
    send(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": { "cwd": dir.path(), "mcpServers": [] } }),
    );
    let session = next()["result"]["sessionId"].as_str().unwrap().to_owned();
    next(); // the commands
    send(
        json!({ "jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": { "sessionId": session, "prompt": [{ "type": "text", "text": "what is in notes.txt?" }] } }),
    );
    let (mut words, mut reads, mut links, mut asked) = (String::new(), 0, Vec::new(), 0);
    loop {
        let m = next();
        if m["id"] == 3 {
            assert_eq!(m["result"]["stopReason"], "end_turn", "{m}");
            break;
        }
        if m["method"] == "session/request_permission" {
            asked += 1;
            assert_eq!(m["params"]["options"][0]["optionId"], "record", "{m}");
            send(
                json!({ "jsonrpc": "2.0", "id": m["id"], "result": { "outcome": { "outcome": "selected", "optionId": "dismiss" } } }),
            );
            continue;
        }
        let u = &m["params"]["update"];
        match u["sessionUpdate"].as_str() {
            Some("tool_call") => reads += 1,
            Some("agent_message_chunk") if u["content"]["type"] == "resource_link" => {
                links.push(u["content"]["uri"].as_str().unwrap().to_owned());
            }
            Some("agent_message_chunk") => {
                words.push_str(u["content"]["text"].as_str().unwrap_or(""))
            }
            _ => {}
        }
    }
    assert!(words.contains("It counts to two"), "{words}");
    assert!(reads >= 1);
    assert!(
        links.len() == 1 && links[0].ends_with("notes.txt"),
        "{links:?}"
    );
    assert_eq!(asked, 1);
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap()
        .stdout;
    assert!(
        status.is_empty(),
        "nothing recorded: {}",
        String::from_utf8_lossy(&status)
    );
    drop(send);
    assert!(child.wait().unwrap().success());
    let _ = mock.kill();
    let _ = mock.wait();
}
