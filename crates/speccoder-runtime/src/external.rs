//! External executors: another coding agent makes the attempt; speccoder
//! keeps everything around it.
//!
//! An ACP agent (`agent = "copilot"`, …, or `kind = "acp"` with `argv`)
//! is driven over the Agent Client Protocol ([`crate::acp`]): each
//! permission request it sends, and every file and command it runs through
//! speccoder, is decided by speccoder's policy and approvals. What it does
//! without asking stays outside speccoder. A `kind = "command"` program
//! gets the prompt on stdin and is judged from its diff alone.
//!
//! Either way speccoder keeps the isolated worktree, frozen and
//! spec-derived acceptance tests written before the attempt, the integrity
//! audit of the diff (protected files, deleted or weakened tests), the
//! checks and the verdict, retries with the verifier's digest, the gaming
//! monitor, ticking and committing.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::acp::agent::{self, Agent};

/// Which agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// An ACP agent: a preset, or `argv`.
    Acp(Agent),
    /// Any program: the prompt on stdin, the summary on stdout.
    Command(Vec<String>),
}

impl Kind {
    /// Its name in reports: the preset's id, `acp` or `command`.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Acp(a) => &a.id,
            Self::Command(_) => "command",
        }
    }
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
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    argv: Vec<String>,
    model: Option<String>,
    mode: Option<String>,
    timeout_secs: Option<u64>,
}

/// The executor for `id` (a preset), for one run (`run --agent`).
///
/// # Errors
///
/// No such preset.
pub fn for_agent(id: &str, configured: Option<&Executor>) -> Result<Executor, String> {
    let p = agent::preset(id)
        .ok_or_else(|| format!("no agent `{id}` ({}, or `native`)", agent::ids()))?;
    let same = configured.filter(|e| e.kind.name() == id);
    Ok(Executor {
        kind: Kind::Acp(Agent::of(
            p,
            same.and_then(|e| match &e.kind {
                Kind::Acp(a) => a.mode.clone(),
                Kind::Command(_) => None,
            }),
        )),
        model: same.and_then(|e| e.model.clone()),
        timeout: configured.map_or(Duration::from_secs(1800), |e| e.timeout),
    })
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
    let kind = match (raw.kind.as_deref(), raw.agent.as_deref()) {
        (Some("native"), None) => return Ok(None),
        (Some(old @ ("claude" | "codex")), None) => {
            return Err(format!(
                "[executor]: kind = \"{old}\" is now agent = \"{old}\" (it runs over ACP)"
            ));
        }
        (None | Some("acp"), Some(id)) => {
            if !raw.argv.is_empty() {
                return Err(
                    "[executor]: `agent` and `argv` exclusive: an agent of your own is kind = \"acp\" with `argv`".to_owned(),
                );
            }
            let p = agent::preset(id)
                .ok_or_else(|| format!("[executor]: agent = \"{id}\" ({})", agent::ids()))?;
            Kind::Acp(Agent::of(p, raw.mode.clone()))
        }
        (Some(k @ ("acp" | "command")), None) => {
            if raw.argv.first().is_none_or(String::is_empty) {
                return Err(format!("[executor]: kind = \"{k}\" needs `argv`"));
            }
            if k == "acp" {
                Kind::Acp(Agent::custom(raw.argv, raw.mode.clone()))
            } else {
                Kind::Command(raw.argv)
            }
        }
        (None, None) => {
            return Err(format!(
                "[executor]: say which agent: agent = \"…\" ({}), or kind = \"acp\" / \"command\" with `argv`",
                agent::ids()
            ));
        }
        (Some(other), _) => {
            return Err(format!(
                "[executor]: kind = \"{other}\" (native, acp or command; an agent is agent = \"…\")"
            ));
        }
    };
    if raw.mode.is_some() && matches!(kind, Kind::Command(_)) {
        return Err("[executor]: `mode` is an ACP agent's permission mode".to_owned());
    }
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

/// Reads a command's final message from its stdout: `BLOCKED:` is an
/// honest stop, `FAILED:` a failure, anything else the agent's summary.
#[must_use]
pub fn interpret(exit: Option<i32>, stdout: &str) -> Value {
    let message = stdout.trim().to_owned();
    let summary: String = message.chars().take(2000).collect();
    let (status, extra) = if exit != Some(0) {
        (
            "failed",
            json!({ "reason": format!("the agent exited {}: {}", exit.map_or("by signal".to_owned(), |c| c.to_string()), summary.chars().take(500).collect::<String>()) }),
        )
    } else {
        return judged(&message);
    };
    let mut out = json!({ "status": status, "tool_calls": [], "usage": Value::Null });
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    out
}

/// An agent's final message after a turn it finished.
#[must_use]
pub fn judged(message: &str) -> Value {
    let message = message.trim();
    let summary: String = message.chars().take(2000).collect();
    let mut out = json!({ "tool_calls": [], "usage": Value::Null });
    if let Some(reason) = message.strip_prefix("BLOCKED:") {
        out["status"] = json!("blocked");
        out["reason"] = json!(reason.trim());
    } else if message.starts_with("FAILED:") {
        out["status"] = json!("failed");
        out["reason"] = json!(summary);
    } else {
        out["status"] = json!("completed");
        out["report"] = json!({ "summary": summary, "status": "done", "files_changed": [], "checks_run": [], "notes": ["external executor"] });
    }
    out
}

/// Runs one `kind = "command"` attempt with `argv` in `worktree`.
#[must_use]
pub fn run(
    argv: &[String],
    timeout: Duration,
    worktree: &Path,
    prompt: &str,
    tmp: &Path,
) -> Outcome {
    use crate::verify::process::{CommandRunner as _, Invocation, LocalRunner, OUTPUT_CAP};
    let _ = std::fs::create_dir_all(tmp);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let prompt_file = tmp.join(format!("external-{stamp}.prompt"));
    let _ = std::fs::write(&prompt_file, prompt);
    // The prompt arrives on stdin (a shell redirect keeps the runner's
    // closed-stdin rule for everything else); the agent keeps the person's
    // environment, because it needs its own credentials.
    let quoted: Vec<String> = argv
        .iter()
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
        timeout,
        output_cap: OUTPUT_CAP,
    });
    let stdout = String::from_utf8_lossy(&out.stdout.kept);
    let mut output = interpret(out.exit_code, &stdout);
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
            timeout.as_secs()
        ));
    }
    let _ = std::fs::remove_file(&prompt_file);
    Outcome {
        run_id: format!("external_command_{stamp}"),
        output,
    }
}

/// What an ACP attempt came to: the native-shaped outcome, and the
/// agent's own sign-in methods when it needs signing in.
#[derive(Debug, Clone)]
pub struct AcpOutcome {
    /// As a native attempt's.
    pub outcome: Outcome,
    /// The agent needs signing in: its ways (the attempt made nothing).
    pub sign_in: Option<Vec<crate::acp::session::SignIn>>,
    /// The agent's name, as it gave it.
    pub agent: String,
}

/// The permission tally of an attempt's log: what the agent asked,
/// what was allowed, what waited for a person, what was refused.
#[must_use]
pub fn tally(log: &[Value]) -> Value {
    let decided: Vec<&Value> = log.iter().filter(|e| e["decision"] != "reported").collect();
    let n = |f: &dyn Fn(&Value) -> bool| decided.iter().filter(|e| f(e)).count();
    json!({
        "asked": decided.len(),
        "allowed": n(&|e| e["decision"] == "allow"),
        "waited": n(&|e| e["by"] == "person" && !e["scope"].as_str().is_some_and(|s| s.ends_with("-grant")) || e["decision"] == "approval"),
        "refused": n(&|e| e["decision"] == "deny"),
        "unasked": log.iter().filter(|e| e["decision"] == "reported").count(),
    })
}

/// The report's account of an ACP attempt, from its output.
#[must_use]
pub fn agent_attempt(output: &Value) -> Option<speccoder_core::AgentAttempt> {
    let a = output.get("acp")?;
    let n = |k: &str| {
        a["tally"][k]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    Some(speccoder_core::AgentAttempt {
        agent: a["agent"].as_str().unwrap_or_default().to_owned(),
        preset: a["preset"].as_str().unwrap_or_default().to_owned(),
        mode: a["mode"].is_object().then(|| {
            serde_json::from_value::<crate::acp::modes::Statement>(a["mode"].clone())
                .map(|s| s.sentence())
                .unwrap_or_default()
        }),
        decides_alone: a["decides_alone"] == true,
        tally: speccoder_core::PermissionTally {
            asked: n("asked"),
            allowed: n("allowed"),
            waited: n("waited"),
            refused: n("refused"),
            unasked: n("unasked"),
        },
        plan: a["plan"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| speccoder_core::AgentPlanStep {
                content: e["content"].as_str().unwrap_or_default().to_owned(),
                status: e["status"].as_str().unwrap_or("pending").to_owned(),
            })
            .collect(),
    })
}

/// Runs one ACP attempt: the agent in `worktree`, its calls decided by
/// `governor`.
#[must_use]
pub fn run_acp(
    exec: &Executor,
    agent: &Agent,
    worktree: &Path,
    prompt: &str,
    runner: &crate::sandbox::Runner,
    governor: std::sync::Arc<crate::acp::govern::Governor>,
    progress: Option<(String, std::sync::Arc<dyn crate::progress::Progress>)>,
) -> AcpOutcome {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let attempt = crate::acp::session::Attempt {
        agent: agent.clone(),
        argv: agent.command(agent::on_path),
        cwd: governor.root().to_path_buf(),
        prompt: prompt.to_owned(),
        model: exec.model.clone(),
        timeout: exec.timeout,
        runner: runner.clone(),
        progress,
    };
    let _ = worktree;
    let f = crate::acp::session::run(&attempt, std::sync::Arc::clone(&governor));
    let shown = f.agent_name.clone().unwrap_or_else(|| agent.name.clone());
    let log = governor.log();
    let pending = governor.pending();
    let mut output = if let Some(p) = &pending {
        json!({ "status": "needs_approval", "pending": p })
    } else if f.sign_in.is_some() {
        json!({ "status": "failed", "reason": f.error.clone().unwrap_or_default() })
    } else if f.timed_out {
        json!({ "status": "failed", "reason": format!("{shown} did not finish within {} s; its turn was cancelled", exec.timeout.as_secs()) })
    } else {
        match (f.stop_reason.as_deref(), &f.error) {
            (Some("end_turn"), _) => judged(&f.message),
            (Some("max_tokens" | "max_turn_requests"), _) => {
                let mut o = judged(&f.message);
                if o["status"] == "completed" {
                    o["status"] = json!("incomplete");
                    o["report"]["status"] = json!("incomplete");
                    o["report"]["notes"] = json!([format!(
                        "{shown} stopped at its own limit ({})",
                        f.stop_reason.as_deref().unwrap_or_default()
                    )]);
                }
                o
            }
            (Some("refusal"), _) => {
                json!({ "status": "failed", "reason": format!("{shown} refused the task: {}", f.message.chars().take(500).collect::<String>()) })
            }
            (Some(other), _) => {
                json!({ "status": "failed", "reason": format!("{shown} ended its turn: {other}") })
            }
            (None, Some(e)) => json!({ "status": "failed", "reason": e }),
            (None, None) => {
                json!({ "status": "failed", "reason": format!("{shown} ended without an answer") })
            }
        }
    };
    // A failure the agent explained only on stderr.
    if output["status"] == "failed"
        && let Some(line) = f.stderr.last()
        && !output["reason"]
            .as_str()
            .unwrap_or_default()
            .contains(line.as_str())
    {
        output["reason"] = json!(format!(
            "{} — {shown} said: {line}",
            output["reason"].as_str().unwrap_or_default()
        ));
    }
    output["tool_calls"] = json!(log);
    output["usage"] = f.usage.clone().unwrap_or(Value::Null);
    output["asked"] = json!(governor.asked());
    output["acp"] = json!({
        "agent": shown,
        "preset": agent.id,
        "mode": f.mode,
        "decides_alone": f.mode.as_ref().is_some_and(|m| m.asks() != Some(true)),
        "tally": tally(&log),
        "plan": f.plan,
        "notices": f.notices,
    });
    AcpOutcome {
        outcome: Outcome {
            run_id: format!("external_{}_{stamp}", agent.id),
            output,
        },
        sign_in: f.sign_in,
        agent: shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(text: &str) -> Result<Option<Kind>, String> {
        parse(text).map(|e| e.map(|e| e.kind))
    }

    #[test]
    fn an_executor_is_an_agent_an_acp_argv_or_a_command() {
        assert_eq!(kind(""), Ok(None));
        assert_eq!(kind("[executor]\nkind = \"native\"\n"), Ok(None));
        let Ok(Some(Kind::Acp(a))) = kind("[executor]\nagent = \"copilot\"\nmode = \"x\"\n") else {
            panic!("copilot")
        };
        assert_eq!((a.id.as_str(), a.mode.as_deref()), ("copilot", Some("x")));
        for id in ["claude", "codex", "gemini", "opencode"] {
            assert!(
                matches!(
                    kind(&format!("[executor]\nagent = \"{id}\"\n")),
                    Ok(Some(Kind::Acp(_)))
                ),
                "{id}"
            );
        }
        let Ok(Some(Kind::Acp(own))) =
            kind("[executor]\nkind = \"acp\"\nargv = [\"my-agent\", \"--acp\"]\n")
        else {
            panic!("argv")
        };
        assert_eq!((own.id.as_str(), own.argv.len()), ("acp", 2));
        assert!(matches!(
            kind("[executor]\nkind = \"command\"\nargv = [\"x\"]\n"),
            Ok(Some(Kind::Command(_)))
        ));
        assert!(kind("[executor]\nkind = \"command\"\n").is_err());
        assert!(kind("[executor]\nagent = \"gpt\"\n").is_err());
        assert!(kind("[executor]\nagent = \"copilot\"\nargv = [\"x\"]\n").is_err());
        assert!(kind("[executor]\nkind = \"command\"\nargv = [\"x\"]\nmode = \"y\"\n").is_err());
        assert!(kind("[executor]\n").is_err());
    }

    #[test]
    fn the_old_claude_and_codex_kinds_name_their_replacement() {
        for old in ["claude", "codex"] {
            let e = kind(&format!("[executor]\nkind = \"{old}\"\n"))
                .err()
                .unwrap_or_default();
            assert!(e.contains(&format!("agent = \"{old}\"")), "{e}");
        }
    }

    #[test]
    fn one_run_can_take_another_agent() {
        let configured =
            parse("[executor]\nagent = \"copilot\"\nmodel = \"m\"\ntimeout_secs = 60\n")
                .ok()
                .flatten();
        let e = for_agent("opencode", configured.as_ref()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (e.kind.name(), e.model, e.timeout.as_secs()),
            ("opencode", None, 60)
        );
        let same = for_agent("copilot", configured.as_ref()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(same.model.as_deref(), Some("m"));
        assert!(for_agent("nope", None).is_err());
    }

    #[test]
    fn final_messages_become_statuses() {
        assert_eq!(judged("Implemented add.")["status"], "completed");
        let blocked = judged("BLOCKED: the test asserts 2+2=5");
        assert_eq!(
            (blocked["status"].as_str(), blocked["reason"].as_str()),
            (Some("blocked"), Some("the test asserts 2+2=5"))
        );
        assert_eq!(interpret(Some(2), "boom")["status"], "failed");
    }
}
