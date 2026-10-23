//! External executors: another coding
//! agent — Claude Code (`claude -p`), Codex (`codex exec`) or any command —
//! makes the attempt; speccoder keeps everything around it.
//!
//! Guarantees are diff-level, not action-level: the agent edits the worktree
//! with its own tools and its own permission system, so speccoder cannot
//! classify each call. What it still does: the isolated worktree, frozen and
//! spec-derived acceptance tests written before the attempt, the integrity
//! audit of the diff (protected files, deleted or weakened tests), the
//! checks and the verdict, retries with the verifier's digest, the gaming
//! monitor, ticking and committing. Where the agent has a permission system,
//! speccoder's command allowances are passed to it.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

/// Which agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Claude Code: `claude -p --output-format json`.
    Claude,
    /// Codex: `codex exec --sandbox workspace-write`.
    Codex,
    /// Any program: the prompt on stdin, the summary on stdout.
    Command(Vec<String>),
}

/// `[executor]` in `speccoder.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executor {
    /// Which agent.
    pub kind: Kind,
    /// The agent's model, if set.
    pub model: Option<String>,
    /// How long one attempt may take.
    pub timeout: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    kind: String,
    #[serde(default)]
    argv: Vec<String>,
    model: Option<String>,
    timeout_secs: Option<u64>,
}

/// Reads `[executor]`; `None` (or `kind = "native"`) is speccoder's own loop.
pub fn parse(text: &str) -> Result<Option<Executor>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(v) = table.get("executor") else {
        return Ok(None);
    };
    let raw: Raw = v
        .clone()
        .try_into()
        .map_err(|e| format!("[executor]: {e}"))?;
    let kind = match raw.kind.as_str() {
        "native" => return Ok(None),
        "claude" => Kind::Claude,
        "codex" => Kind::Codex,
        "command" => {
            if raw.argv.first().is_none_or(String::is_empty) {
                return Err("[executor]: kind = \"command\" needs `argv`".to_owned());
            }
            Kind::Command(raw.argv)
        }
        other => {
            return Err(format!(
                "[executor]: kind = \"{other}\" (native, claude, codex or command)"
            ));
        }
    };
    Ok(Some(Executor {
        kind,
        model: raw.model,
        timeout: Duration::from_secs(raw.timeout_secs.unwrap_or(1800)),
    }))
}

/// What the external agent is told: the same context a native attempt gets,
/// the rules speccoder will check afterwards, and how to stop honestly.
#[must_use]
pub fn prompt(bundle: &Value, digest: Option<&Value>) -> String {
    prompt_in(crate::format::reference().wording(), bundle, digest)
}

/// [`prompt`] for a project whose format is worded `w`.
#[must_use]
pub fn prompt_in(w: &crate::format::Wording, bundle: &Value, digest: Option<&Value>) -> String {
    let s = |k: &str| bundle[k].as_str().unwrap_or_default();
    let task = &bundle["task"];
    let mut out = format!(
        "You are implementing exactly one task of a {product} {unit} in this git worktree.\n\n\
Rules — speccoder checks them after you finish, from the diff:\n\
- Change only what the task needs. Do not edit the spec ({intent}), the checks' configuration (`speccoder.toml`, CI files, test-runner config) or anything under `.git`.\n\
- Test files that exist now are frozen: add new tests in new files; never weaken, skip or delete tests.\n\
- Do not commit, branch or push; speccoder commits a task that passes.\n\
- Run the project's tests to see whether your change works; speccoder runs them again and decides.\n\
- If the task cannot be done as written (it contradicts the spec, a test is wrong, information is missing), stop and make your final message start with `BLOCKED:` and the reason.\n\n\
# Task {}\n\n{}\n\nPhase: {}\n",
        task["id"].as_str().unwrap_or_default(),
        task["title"].as_str().unwrap_or_default(),
        task["phase"].as_str().unwrap_or_default(),
        product = w.product,
        unit = w.unit,
        intent = w.intent_code,
    );
    for (title, key) in [
        (w.story, "story"),
        (w.plan_short, "plan"),
        (w.constitution, "constitution"),
    ] {
        if !s(key).trim().is_empty() {
            out.push_str(&format!("\n# {title}\n\n{}\n", s(key)));
        }
    }
    if let Some(d) = digest.filter(|d| !d.is_null()) {
        out.push_str(&format!(
            "\n# Previous attempt (failed; its changes were undone)\n\nThe verifier's facts:\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(d).unwrap_or_default()
        ));
    }
    out
}

/// What an external attempt produced, in the shape of a native attempt's output.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// An id for reports (`external_<agent>_<unix time>`); external runs are
    /// not journaled.
    pub run_id: String,
    /// `{status, report?, reason?, tool_calls, usage}` like a native attempt.
    pub output: Value,
}

/// The argv for an executor; `allow` are the commands speccoder allows
/// (checks and `[commands] allow`), passed to agents with a permission system.
#[must_use]
pub fn argv(
    exec: &Executor,
    worktree: &Path,
    last_message: &Path,
    allow: &[Vec<String>],
) -> Vec<String> {
    match &exec.kind {
        Kind::Claude => {
            let mut tools = vec!["Read", "Edit", "Write", "MultiEdit", "Glob", "Grep", "LS"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            tools.extend(allow.iter().map(|p| format!("Bash({}:*)", p.join(" "))));
            let mut a = vec![
                "claude".to_owned(),
                "-p".to_owned(),
                "--output-format".to_owned(),
                "json".to_owned(),
                "--permission-mode".to_owned(),
                "acceptEdits".to_owned(),
                "--allowedTools".to_owned(),
                tools.join(","),
                "--disallowedTools".to_owned(),
                "WebFetch,WebSearch".to_owned(),
            ];
            if let Some(m) = &exec.model {
                a.extend(["--model".to_owned(), m.clone()]);
            }
            a
        }
        Kind::Codex => {
            let mut a = vec![
                "codex".to_owned(),
                "exec".to_owned(),
                "--sandbox".to_owned(),
                "workspace-write".to_owned(),
                "--skip-git-repo-check".to_owned(),
                "-C".to_owned(),
                worktree.display().to_string(),
                "-o".to_owned(),
                last_message.display().to_string(),
            ];
            if let Some(m) = &exec.model {
                a.extend(["-m".to_owned(), m.clone()]);
            }
            a.push("-".to_owned());
            a
        }
        Kind::Command(argv) => argv.clone(),
    }
}

/// Reads an agent's final message and usage from its stdout (and, for
/// Codex, its last-message file).
#[must_use]
pub fn interpret(
    exec: &Executor,
    exit: Option<i32>,
    stdout: &str,
    last_message: Option<&str>,
) -> Value {
    let (message, usage) = match &exec.kind {
        Kind::Claude => match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(v) => {
                let u = &v["usage"];
                let n = |k: &str| u[k].as_u64().unwrap_or(0);
                let usage = json!({
                    "input_tokens": n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"),
                    "output_tokens": n("output_tokens"),
                    "cache_read_tokens": n("cache_read_input_tokens"),
                    "cache_write_tokens": n("cache_creation_input_tokens"),
                    "cost_usd": v["total_cost_usd"],
                });
                let failed = v["is_error"] == true;
                (
                    if failed {
                        format!(
                            "FAILED: {}",
                            v["result"]
                                .as_str()
                                .unwrap_or("the agent reported an error")
                        )
                    } else {
                        v["result"].as_str().unwrap_or_default().to_owned()
                    },
                    Some(usage),
                )
            }
            Err(_) => (stdout.to_owned(), None),
        },
        Kind::Codex => (last_message.unwrap_or(stdout).to_owned(), None),
        Kind::Command(_) => (stdout.to_owned(), None),
    };
    let message = message.trim().to_owned();
    let summary: String = message.chars().take(2000).collect();
    let (status, extra) = if exit != Some(0) {
        (
            "failed",
            json!({ "reason": format!("the agent exited {}: {}", exit.map_or("by signal".to_owned(), |c| c.to_string()), summary.chars().take(500).collect::<String>()) }),
        )
    } else if let Some(reason) = message.strip_prefix("BLOCKED:") {
        ("blocked", json!({ "reason": reason.trim() }))
    } else if message.starts_with("FAILED:") {
        ("failed", json!({ "reason": summary }))
    } else {
        (
            "completed",
            json!({ "report": { "summary": summary, "status": "done", "files_changed": [], "checks_run": [], "notes": ["external executor"] } }),
        )
    };
    let mut out =
        json!({ "status": status, "tool_calls": [], "usage": usage.unwrap_or(Value::Null) });
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    out
}

/// Runs one attempt with `exec` in `worktree`.
#[must_use]
pub fn run(
    exec: &Executor,
    worktree: &Path,
    prompt: &str,
    allow: &[Vec<String>],
    tmp: &Path,
) -> Outcome {
    use crate::verify::process::{CommandRunner as _, Invocation, LocalRunner, OUTPUT_CAP};
    let _ = std::fs::create_dir_all(tmp);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let last = tmp.join(format!("external-{stamp}.txt"));
    let prompt_file = tmp.join(format!("external-{stamp}.prompt"));
    let _ = std::fs::write(&prompt_file, prompt);
    let agent = match &exec.kind {
        Kind::Claude => "claude",
        Kind::Codex => "codex",
        Kind::Command(_) => "command",
    };
    // The prompt arrives on stdin (a shell redirect keeps the runner's
    // closed-stdin rule for everything else); the agent keeps the person's
    // environment, because it needs its own credentials.
    let mut inner = argv(exec, worktree, &last, allow);
    let quoted: Vec<String> = inner
        .drain(..)
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect();
    let script = format!(
        "exec {} < '{}'",
        quoted.join(" "),
        prompt_file.display().to_string().replace('\'', "'\\''")
    );
    let out = LocalRunner.run(&Invocation {
        argv: vec!["sh".to_owned(), "-c".to_owned(), script],
        cwd: worktree.to_path_buf(),
        env: std::env::vars().collect(),
        timeout: exec.timeout,
        output_cap: OUTPUT_CAP,
    });
    let last_message = std::fs::read_to_string(&last).ok();
    let stdout = String::from_utf8_lossy(&out.stdout.kept);
    let mut output = interpret(exec, out.exit_code, &stdout, last_message.as_deref());
    // A failing agent often explains itself only on stderr.
    if out.exit_code != Some(0) && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr.kept);
        let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = lines[lines.len().saturating_sub(5)..].join(" | ");
        output["reason"] = json!(format!(
            "the agent exited {}: {}",
            out.exit_code
                .map_or("by signal".to_owned(), |c| c.to_string()),
            tail.chars().take(500).collect::<String>()
        ));
    }
    if let Some(e) = &out.spawn_error {
        output["status"] = json!("failed");
        output["reason"] = json!(format!("the agent could not be started: {e}"));
    }
    if out.timed_out {
        output["status"] = json!("failed");
        output["reason"] = json!(format!(
            "the agent did not finish within {} s",
            exec.timeout.as_secs()
        ));
    }
    let _ = std::fs::remove_file(&prompt_file);
    let _ = std::fs::remove_file(&last);
    Outcome {
        run_id: format!("external_{agent}_{stamp}"),
        output,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executors_parse_and_claude_gets_the_allowances() {
        assert!(parse("").unwrap_or_default().is_none());
        assert!(
            parse("[executor]\nkind = \"native\"\n")
                .unwrap_or_default()
                .is_none()
        );
        let c = parse("[executor]\nkind = \"claude\"\nmodel = \"sonnet\"\n")
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("claude"));
        let a = argv(
            &c,
            Path::new("/w"),
            Path::new("/t/last"),
            &[vec!["cargo".to_owned(), "test".to_owned()]],
        );
        assert!(
            a.contains(&"Read,Edit,Write,MultiEdit,Glob,Grep,LS,Bash(cargo test:*)".to_owned()),
            "{a:?}"
        );
        assert!(a.ends_with(&["--model".to_owned(), "sonnet".to_owned()]));
        let x = parse("[executor]\nkind = \"codex\"\n")
            .unwrap_or_default()
            .unwrap_or_else(|| panic!("codex"));
        let a = argv(&x, Path::new("/w"), Path::new("/t/last"), &[]);
        assert_eq!(&a[..4], ["codex", "exec", "--sandbox", "workspace-write"]);
        assert!(parse("[executor]\nkind = \"command\"\n").is_err());
        assert!(parse("[executor]\nkind = \"gpt\"\n").is_err());
    }

    #[test]
    fn final_messages_become_statuses() {
        let c = Executor {
            kind: Kind::Claude,
            model: None,
            timeout: Duration::from_secs(1),
        };
        let ok = interpret(
            &c,
            Some(0),
            r#"{"type":"result","is_error":false,"result":"Implemented add.","total_cost_usd":0.01,"usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":5}}"#,
            None,
        );
        assert_eq!(ok["status"], "completed");
        assert_eq!(ok["usage"]["input_tokens"], 100);
        let blocked = interpret(
            &c,
            Some(0),
            r#"{"is_error":false,"result":"BLOCKED: the test asserts 2+2=5"}"#,
            None,
        );
        assert_eq!(
            (blocked["status"].as_str(), blocked["reason"].as_str()),
            (Some("blocked"), Some("the test asserts 2+2=5"))
        );
        let cmd = Executor {
            kind: Kind::Command(vec!["x".to_owned()]),
            model: None,
            timeout: Duration::from_secs(1),
        };
        assert_eq!(interpret(&cmd, Some(2), "boom", None)["status"], "failed");
    }
}
