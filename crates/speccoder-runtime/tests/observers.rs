//! Observers end to end (spec 095): events read back from git, delivered
//! at least once, in order, signed; a receiver that fails changes nothing
//! but its own delivery.

// Test code: panicking on unexpected input is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use speccoder_runtime::hooks;

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with `hooks` and a unit branch, and a way to tick on it.
fn repo(hooks: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("speccoder.toml"),
        format!("[model]\nprovider = \"openai\"\nmodel = \"m\"\n{hooks}"),
    )
    .unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["add", "-A"]);
    git(d.path(), &["commit", "-q", "-m", "init"]);
    git(d.path(), &["branch", "speccoder/001-x"]);
    d
}

/// A tick commit for `task` on `speccoder/001-x`; its sha.
fn tick(repo: &Path, task: &str) -> String {
    git(repo, &["checkout", "-q", "speccoder/001-x"]);
    let msg = format!(
        "{task} Do it\n\nDo it in src/secret_place.rs\n\nSpec-Task: 001-x/{task}\nVerdict: verified\nScenarios: US1-1@aa\nRun: run_01\n"
    );
    git(repo, &["commit", "-q", "--allow-empty", "-m", &msg]);
    let sha = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["checkout", "-q", "main"]);
    sha
}

fn trust_all(repo: &Path, state: &Path) {
    let r = hooks::report::trust(repo, state, None, "terminal", &mut |_| true);
    assert_eq!(
        r.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        r.message()
    );
}

/// One request a receiver got: its headers and its body.
type Request = (Vec<(String, String)>, Vec<u8>);

/// A local receiver answering `status` to each request; what it got.
#[derive(Clone, Default)]
struct Got(Arc<Mutex<Vec<Request>>>);

fn receiver(status: &'static str) -> (String, Got) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/hook",
        listener.local_addr().unwrap().port()
    );
    let got = Got::default();
    let keep = got.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut headers = Vec::new();
            let mut len = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.trim_end().split_once(": ") {
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.parse().unwrap_or(0);
                    }
                    headers.push((k.to_ascii_lowercase(), v.to_owned()));
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            keep.0.lock().unwrap().push((headers, body));
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            );
        }
    });
    (url, got)
}

const SECRET: &str = "a-signing-key-of-at-least-24-bytes";

#[test]
fn a_webhook_gets_each_tick_once_signed_and_schema_shaped() {
    let (url, got) = receiver("200 OK");
    let r = repo(&format!(
        "\n[hooks.chat]\non = [\"task.ticked\"]\nurl = \"{url}\"\nsecret_env = \"SPECCODER_TEST_HOOK_SECRET\"\n"
    ));
    let state = tempfile::tempdir().unwrap();
    // The secret is named, never written in the configuration; a child
    // process gets it, this test runs `deliver` in-process.
    if std::env::var("SPECCODER_TEST_HOOK_SECRET").as_deref() != Ok(SECRET) {
        // Re-run this test in a child with the variable set (no unsafe
        // set_var in-process).
        let me = std::env::current_exe().unwrap();
        let out = Command::new(me)
            .args([
                "--exact",
                "a_webhook_gets_each_tick_once_signed_and_schema_shaped",
                "--nocapture",
            ])
            .env("SPECCODER_TEST_HOOK_SECRET", SECRET)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    // Ticks before the observer was trusted are not its to hear.
    tick(r.path(), "T001");
    trust_all(r.path(), state.path());
    let t2 = tick(r.path(), "T002");
    let done = hooks::report::deliver(r.path(), state.path());
    assert_eq!(
        done.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        done.message()
    );
    let first = got.0.lock().unwrap().clone();
    assert_eq!(first.len(), 1, "{first:?}");
    let (headers, body) = &first[0];
    let event: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(event["id"], format!("tick:{t2}"));
    assert_eq!(event["type"], "dev.speccoder.task.ticked");
    assert_eq!(
        event["data"]["summary"],
        "T002 verified in 001-x — 1 scenario"
    );
    assert!(!String::from_utf8_lossy(body).contains("secret_place"));
    let h = |k: &str| {
        headers
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap()
    };
    assert_eq!(h("webhook-id"), format!("tick:{t2}"));
    let verifier =
        agentplane::push::WebhookVerifier::new(&agentplane::core::Secret::new(SECRET)).unwrap();
    verifier
        .verify_parts(
            h("webhook-id"),
            h("webhook-timestamp"),
            h("webhook-signature"),
            body,
            agentplane::core::Timestamp::now_utc(),
        )
        .unwrap();
    // Delivered: not again.
    let again = hooks::report::deliver(r.path(), state.path());
    assert!(
        again.message().starts_with("delivered 0 events"),
        "{}",
        again.message()
    );
    assert_eq!(got.0.lock().unwrap().len(), 1);
}

/// SC-005 and FR-019: a receiver that fails keeps the event pending; the
/// next process delivers it with the same id.
#[test]
fn an_event_a_receiver_refused_waits_and_goes_out_later_with_its_id() {
    let (bad, refused) = receiver("500 Internal Server Error");
    let r = repo(&format!("\n[hooks.chat]\non = [\"*\"]\nurl = \"{bad}\"\n"));
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    let t1 = tick(r.path(), "T001");
    let first = hooks::report::deliver(r.path(), state.path());
    assert_eq!(
        first.status(),
        speccoder_core::CommandStatus::Error,
        "{}",
        first.message()
    );
    assert!(
        first.message().contains("answered 500"),
        "{}",
        first.message()
    );
    assert_eq!(refused.0.lock().unwrap().len(), 1);
    // The receiver is fixed (the same hook, a healthy endpoint): a new
    // definition is a new digest, trusted again; the cursor stays.
    let (good, took) = receiver("204 No Content");
    std::fs::write(
        r.path().join("speccoder.toml"),
        format!("[model]\nprovider = \"openai\"\nmodel = \"m\"\n\n[hooks.chat]\non = [\"*\"]\nurl = \"{good}\"\n"),
    )
    .unwrap();
    trust_all(r.path(), state.path());
    let later = hooks::report::deliver(r.path(), state.path());
    assert_eq!(
        later.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        later.message()
    );
    let got = took.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    let event: Value = serde_json::from_slice(&got[0].1).unwrap();
    assert_eq!(event["id"], format!("tick:{t1}"));
}

#[test]
fn a_command_observer_reads_the_event_on_stdin() {
    if speccoder_runtime::sandbox::backend().is_err() {
        return;
    }
    let r = repo(
        "\n[hooks.log]\non = [\"task.ticked\"]\nargv = [\"sh\", \"-c\", \"read line; case \\\"$line\\\" in *dev.speccoder.task.ticked*T001*) exit 0;; *) echo \\\"got: $line\\\" >&2; exit 3;; esac\"]\n",
    );
    let state = tempfile::tempdir().unwrap();
    trust_all(r.path(), state.path());
    tick(r.path(), "T001");
    let done = hooks::report::deliver(r.path(), state.path());
    assert_eq!(
        done.status(),
        speccoder_core::CommandStatus::Ok,
        "{}",
        done.message()
    );
    assert!(
        done.message().starts_with("delivered 1 event"),
        "{}",
        done.message()
    );
}

#[test]
fn an_untrusted_observer_hears_nothing_and_says_so() {
    let (url, got) = receiver("200 OK");
    let r = repo(&format!("\n[hooks.chat]\non = [\"*\"]\nurl = \"{url}\"\n"));
    let state = tempfile::tempdir().unwrap();
    tick(r.path(), "T001");
    let done = hooks::report::deliver(r.path(), state.path());
    assert!(
        done.message().contains("skipped (not trusted): chat"),
        "{}",
        done.message()
    );
    assert!(got.0.lock().unwrap().is_empty());
    assert!(
        hooks::report::untrusted_notice(r.path(), state.path())
            .unwrap()
            .starts_with("chat not trusted")
    );
}
