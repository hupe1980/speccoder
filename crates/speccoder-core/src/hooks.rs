//! Hooks: a person's own commands around a run. A gate can stop a tick or
//! a pull request — only exit 0 lets it through; an observer hears what
//! speccoder decided and never changes it. The pure parts live here: the
//! `[hooks.<name>]` grammar, the gate outcome rule, the two public events
//! and the sentence each carries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The longest gate reason kept, in characters.
pub const MAX_REASON: usize = 300;

/// The longest timeout a hook may ask for, in seconds.
pub const MAX_TIMEOUT: u64 = 600;

/// The events an observer can hear.
pub const EVENTS: &[&str] = &["task.ticked", "task.stopped"];

/// Where a gate stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Point {
    /// After the verdict, before the tick commit.
    Tick,
    /// After the person confirms a pull request, before anything is pushed.
    Pr,
}

impl Point {
    /// `tick` or `pr`, as `at` names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tick => "tick",
            Self::Pr => "pr",
        }
    }

    /// The point in words: `before tick`, `before pull request`.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Tick => "before tick",
            Self::Pr => "before pull request",
        }
    }

    /// The point `at = "<word>"` names.
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "tick" => Some(Self::Tick),
            "pr" => Some(Self::Pr),
            _ => None,
        }
    }
}

/// A gate or an observer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Stops a point unless it exits 0.
    Gate {
        /// Where it stands.
        at: Point,
        /// Whether it may reach the network.
        network: bool,
    },
    /// Hears events.
    Observer {
        /// The event names, or `*`.
        on: Vec<String>,
    },
}

/// What a hook runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runner {
    /// A command; an observer gets the event on stdin.
    Argv(Vec<String>),
    /// A webhook (observers only).
    Url {
        /// Where to post.
        url: String,
        /// The variable holding the signing key.
        secret_env: Option<String>,
    },
}

/// One `[hooks.<name>]` table.
#[derive(Debug, Clone, PartialEq)]
pub struct HookDef {
    /// The table's name.
    pub name: String,
    /// Gate or observer.
    pub role: Role,
    /// What it runs.
    pub runner: Runner,
    /// Seconds before it is stopped.
    pub timeout: u64,
    /// Variable names passed through the scrubbed environment.
    pub env: Vec<String>,
    /// More files it depends on (its configuration, scripts a shell line
    /// runs): digested with it, never writable by the model.
    pub files: Vec<String>,
    /// The table as canonical JSON: what a digest and a trust prompt cover.
    pub definition: Value,
    /// The extension it comes from, and that extension's folder relative
    /// to the project: its trust is the extension's, its `./` paths are
    /// inside that folder.
    pub extension: Option<(String, String)>,
}

impl HookDef {
    /// The point this gate stands at; `None` for an observer.
    #[must_use]
    pub const fn at(&self) -> Option<Point> {
        match self.role {
            Role::Gate { at, .. } => Some(at),
            Role::Observer { .. } => None,
        }
    }

    /// Whether this observer hears `event`.
    #[must_use]
    pub fn hears(&self, event: &str) -> bool {
        matches!(&self.role, Role::Observer { on } if on.iter().any(|e| e == "*" || e == event))
    }

    /// What it guards or hears, in words: `before tick`, `hears task.ticked, task.stopped`.
    #[must_use]
    pub fn listens(&self) -> String {
        match &self.role {
            Role::Gate { at, .. } => at.words().to_owned(),
            Role::Observer { on } if on.iter().any(|e| e == "*") => {
                format!("hears {}", EVENTS.join(", "))
            }
            Role::Observer { on } => format!("hears {}", on.join(", ")),
        }
    }

    /// The command it runs, if it runs one.
    #[must_use]
    pub fn argv(&self) -> Option<&[String]> {
        match &self.runner {
            Runner::Argv(a) => Some(a),
            Runner::Url { .. } => None,
        }
    }
}

/// Whether `name` may name a hook: `[a-z0-9_-]+`.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}

/// Whether a webhook may go to `url`: https, or plain http only to this
/// machine.
///
/// # Errors
///
/// Why not, with the fix.
pub fn url_allowed(url: &str) -> Result<(), String> {
    if url.starts_with("https://") && url.len() > "https://".len() {
        return Ok(());
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return Err(format!("`{url}` is not an https URL"));
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split(']')
            .next()
            .unwrap_or_default()
            .trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or_default()
    };
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        Ok(())
    } else {
        Err(format!(
            "`{url}` is plain http to another machine: use https (http only to localhost)"
        ))
    }
}

/// The value with every object's keys sorted, so key order and comments in
/// the configuration never change a digest.
#[must_use]
pub fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), canonical(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        v => v.clone(),
    }
}

const KEYS: &[&str] = &[
    "at",
    "on",
    "argv",
    "url",
    "secret_env",
    "timeout",
    "env",
    "network",
    "files",
];

/// Parses `[hooks.<name>]` (as JSON).
///
/// # Errors
///
/// What is wrong, with the fix — a `HOOK-MISCONFIGURED` message.
#[allow(clippy::too_many_lines)] // one rule per key, each with its fix
pub fn parse(name: &str, table: &Value) -> Result<HookDef, String> {
    let at_hook = |m: String| format!("[hooks.{name}]: {m}");
    if !valid_name(name) {
        return Err(at_hook(
            "a hook's name is lowercase letters, digits, `-` and `_`".to_owned(),
        ));
    }
    let Some(t) = table.as_object() else {
        return Err(at_hook("must be a table".to_owned()));
    };
    if let Some(k) = t.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(at_hook(format!(
            "unknown key `{k}` (known: {})",
            KEYS.join(", ")
        )));
    }
    let strings = |key: &str| -> Result<Option<Vec<String>>, String> {
        match t.get(key) {
            None => Ok(None),
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .map(Some)
                .ok_or_else(|| at_hook(format!("`{key}` is a list of strings"))),
            Some(_) => Err(at_hook(format!("`{key}` is a list of strings"))),
        }
    };
    let string = |key: &str| -> Result<Option<String>, String> {
        match t.get(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(at_hook(format!("`{key}` is a string"))),
        }
    };
    let argv = strings("argv")?;
    let url = string("url")?;
    let secret_env = string("secret_env")?;
    let env = strings("env")?.unwrap_or_default();
    let files = strings("files")?.unwrap_or_default();
    if let Some(f) = files
        .iter()
        .find(|f| f.is_empty() || f.starts_with('/') || f.split('/').any(|s| s == ".."))
    {
        return Err(at_hook(format!(
            "`files`: `{f}` is not a path inside the repository"
        )));
    }
    let role = match (string("at")?, strings("on")?) {
        (Some(_), Some(_)) => {
            return Err(at_hook(
                "`at` makes a gate and `on` an observer: keep one".to_owned(),
            ));
        }
        (None, None) => {
            return Err(at_hook(
                "add `at = \"tick\"` (a gate) or `on = [\"task.ticked\"]` (an observer)".to_owned(),
            ));
        }
        (Some(at), None) => {
            let Some(at) = Point::named(&at) else {
                return Err(at_hook(format!(
                    "`at = \"{at}\"`: a gate stands at \"tick\" or \"pr\""
                )));
            };
            let network = match t.get("network") {
                None => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => return Err(at_hook("`network` is true or false".to_owned())),
            };
            Role::Gate { at, network }
        }
        (None, Some(on)) => {
            if on.is_empty() {
                return Err(at_hook(format!(
                    "`on` names the events to hear: {} or \"*\"",
                    EVENTS.join(", ")
                )));
            }
            if let Some(e) = on
                .iter()
                .find(|e| *e != "*" && !EVENTS.contains(&e.as_str()))
            {
                return Err(at_hook(format!(
                    "`{e}` is not an event an observer can hear ({} or \"*\")",
                    EVENTS.join(", ")
                )));
            }
            if t.contains_key("network") {
                return Err(at_hook(
                    "`network` is for gates; an observer always has the network".to_owned(),
                ));
            }
            Role::Observer { on }
        }
    };
    let runner = match (argv, url) {
        (Some(_), Some(_)) => {
            return Err(at_hook("give `argv` or `url`, not both".to_owned()));
        }
        (None, None) => return Err(at_hook("add `argv = [\"…\"]` to run".to_owned())),
        (Some(a), None) if a.is_empty() || a[0].trim().is_empty() => {
            return Err(at_hook("`argv` names the program first".to_owned()));
        }
        (Some(a), None) => {
            if secret_env.is_some() {
                return Err(at_hook("`secret_env` signs a `url`'s requests".to_owned()));
            }
            Runner::Argv(a)
        }
        (None, Some(url)) => {
            if matches!(role, Role::Gate { .. }) {
                return Err(at_hook(
                    "a gate runs `argv`; only an observer posts to a `url`".to_owned(),
                ));
            }
            url_allowed(&url).map_err(at_hook)?;
            Runner::Url { url, secret_env }
        }
    };
    let timeout = match t.get("timeout") {
        None if matches!(role, Role::Gate { .. }) => 60,
        None => 10,
        Some(v) => match v.as_u64() {
            Some(s) if (1..=MAX_TIMEOUT).contains(&s) => s,
            _ => {
                return Err(at_hook(format!("`timeout` is seconds, 1 to {MAX_TIMEOUT}")));
            }
        },
    };
    Ok(HookDef {
        name: name.to_owned(),
        role,
        runner,
        timeout,
        env,
        files,
        definition: canonical(table),
        extension: None,
    })
}

/// How a gate's run ended.
#[derive(Debug, Clone, Default)]
pub struct GateExit<'a> {
    /// Its exit code; `None` when it was killed or never started.
    pub exit_code: Option<i32>,
    /// It ran past its timeout.
    pub timed_out: bool,
    /// It could not be started.
    pub spawn_error: Option<&'a str>,
    /// What it printed.
    pub stdout: &'a str,
    /// What it printed on stderr.
    pub stderr: &'a str,
    /// Files the tick would commit that it changed.
    pub changed: &'a [String],
    /// Its timeout, for the words.
    pub timeout: u64,
}

/// A gate's verdict on its point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// It exited 0 and changed nothing.
    Passed,
    /// Anything else, with why.
    Blocked(String),
}

impl GateOutcome {
    /// The rule: exit 0 and nothing changed passes; every other end blocks,
    /// with the gate's own last line when it printed one, else the cause.
    #[must_use]
    pub fn of(e: &GateExit<'_>) -> Self {
        if let Some(err) = e.spawn_error {
            return Self::Blocked(clean(&format!("could not start: {err}")));
        }
        if e.timed_out {
            return Self::Blocked(format!("timed out after {} s", e.timeout));
        }
        if !e.changed.is_empty() {
            let shown: Vec<String> = e.changed.iter().take(3).map(|p| format!("`{p}`")).collect();
            let more = e.changed.len().saturating_sub(3);
            let more = if more > 0 {
                format!(" (+{more})")
            } else {
                String::new()
            };
            return Self::Blocked(clean(&format!("changed {}{more}", shown.join(", "))));
        }
        match e.exit_code {
            Some(0) => Self::Passed,
            Some(code) => Self::Blocked(
                last_line(e.stderr)
                    .or_else(|| last_line(e.stdout))
                    .map_or_else(|| format!("exit {code}"), clean),
            ),
            None => Self::Blocked("was killed".to_owned()),
        }
    }
}

fn last_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).rfind(|l| !l.is_empty())
}

/// One line, control characters shown as `\u{…}`, at most [`MAX_REASON`]
/// characters.
#[must_use]
pub fn clean(reason: &str) -> String {
    let mut out = String::new();
    for c in reason.chars() {
        if c == '\n' || c == '\r' || c == '\t' {
            out.push(' ');
        } else if c.is_control() {
            out.push_str(&c.escape_unicode().to_string());
        } else {
            out.push(c);
        }
    }
    let out = out.trim();
    if out.chars().count() > MAX_REASON {
        let mut cut: String = out.chars().take(MAX_REASON - 1).collect();
        cut.push('…');
        cut
    } else {
        out.to_owned()
    }
}

/// How one gate ended at its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    /// It exited 0.
    Passed,
    /// It stopped the point.
    Blocked,
    /// An earlier gate at the point blocked first.
    NotRun,
}

impl GateState {
    /// `passed`, `blocked`, `not_run`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Blocked => "blocked",
            Self::NotRun => "not_run",
        }
    }
}

/// One gate's run at its point, as a report and the evidence carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateRecord {
    /// The hook's name.
    pub name: String,
    /// Where it stood.
    pub point: Point,
    /// How it ended.
    pub outcome: GateState,
    /// Why it blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `HOOK-BLOCKED`, or `HOOK-UNTRUSTED` for a gate that waits for trust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// How long it ran.
    pub duration_ms: u64,
    /// The findings it left (SARIF), as shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<crate::extensions::SarifFinding>,
}

impl GateRecord {
    /// The one line a blocked point shows: `Blocked by licence — <reason>`.
    #[must_use]
    pub fn blocked_line(&self) -> String {
        format!(
            "Blocked by {} — {}",
            self.name,
            self.reason.as_deref().unwrap_or("no reason given")
        )
    }
}

/// The first gate that blocked, if one did.
#[must_use]
pub fn first_block(gates: &[GateRecord]) -> Option<&GateRecord> {
    gates.iter().find(|g| g.outcome == GateState::Blocked)
}

/// A gate's mark on an event: its name and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateMark {
    /// The hook's name.
    pub name: String,
    /// How it ended.
    pub outcome: GateState,
}

/// `k` of `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Count {
    /// How many passed.
    pub passed: u32,
    /// Of how many.
    pub total: u32,
}

/// `task.ticked`: a tick commit on a unit's branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskTicked {
    /// The unit (feature) it belongs to.
    pub feature: String,
    /// The task id.
    pub task: String,
    /// `verified` or `regression_only`.
    pub verdict: String,
    /// The tick commit.
    pub commit: String,
    /// The scenarios its tests showed green (spec ids).
    pub scenarios: Vec<String>,
    /// The held-out tests that passed, when there were any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_out: Option<Count>,
    /// How strong its acceptance tests were (0–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_strength: Option<f64>,
    /// The gates it passed.
    pub gates: Vec<GateMark>,
    /// The external agent that did it, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    /// The run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// One sentence, ready for a chat message.
    pub summary: String,
}

/// `task.stopped`: an attempt saved without a tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskStopped {
    /// The unit (feature) it belongs to.
    pub feature: String,
    /// The task id.
    pub task: String,
    /// `verification_failed`, `needs_approval`, `blocked_by_gate`, …
    pub outcome: String,
    /// The reason's code, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The saved attempt.
    pub attempt_ref: String,
    /// The gates that ran.
    pub gates: Vec<GateMark>,
    /// The run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// One sentence, ready for a chat message.
    pub summary: String,
}

/// *T004 verified in 001-md2csv — 3 scenarios, held-out 2 of 2*.
#[must_use]
pub fn ticked_summary(t: &TaskTicked) -> String {
    let mut parts = Vec::new();
    match t.scenarios.len() {
        0 => {}
        1 => parts.push("1 scenario".to_owned()),
        n => parts.push(format!("{n} scenarios")),
    }
    if let Some(h) = t.held_out {
        parts.push(format!("held-out {} of {}", h.passed, h.total));
    }
    let verdict = t.verdict.replace('_', " ");
    let tail = if parts.is_empty() {
        String::new()
    } else {
        format!(" — {}", parts.join(", "))
    };
    format!("{} {verdict} in {}{tail}", t.task, t.feature)
}

/// *T006 blocked by licence in 001-md2csv* · *T005 stopped in 001-md2csv:
/// verification failed (CHECK-FAILED)*.
#[must_use]
pub fn stopped_summary(s: &TaskStopped) -> String {
    if s.outcome == "blocked_by_gate"
        && let Some(g) = s.gates.iter().find(|g| g.outcome == GateState::Blocked)
    {
        return format!("{} blocked by {} in {}", s.task, g.name, s.feature);
    }
    let code = s
        .code
        .as_deref()
        .map(|c| format!(" ({c})"))
        .unwrap_or_default();
    format!(
        "{} stopped in {}: {}{code}",
        s.task,
        s.feature,
        s.outcome.replace('_', " ")
    )
}

/// Whether a hook may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    /// A person trusted these exact bytes.
    Trusted,
    /// Never trusted here.
    Untrusted,
    /// Trusted once; the definition or a file it runs changed since.
    Changed,
}

impl TrustState {
    /// `trusted`, `untrusted`, `changed`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Untrusted => "untrusted",
            Self::Changed => "changed",
        }
    }
}

/// A file a hook runs, and its content's digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HookFile {
    /// Its path in the repository.
    pub path: String,
    /// `sha256:<hex>` of its content.
    pub digest: String,
}

/// One run of a hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HookRun {
    /// When it started (RFC 3339).
    pub at: String,
    /// The event id, `tick T004`, `pr` or `test`.
    pub what: String,
    /// `delivered`, `failed`, `passed`, `blocked`, `not_run`, `skipped_untrusted`.
    pub outcome: String,
    /// How long it ran.
    pub duration_ms: u64,
    /// Why it failed or blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The findings a gate left (SARIF), as shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<crate::extensions::SarifFinding>,
    /// Why the findings it left could not be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings_error: Option<String>,
}

/// One hook as `speccoder hooks` shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HookEntry {
    /// The table's name.
    pub name: String,
    /// `gate` or `observer`.
    pub role: String,
    /// What it guards or hears, in words.
    pub listens: String,
    /// Where a gate stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
    /// The events an observer hears.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<String>,
    /// The command it runs, or the URL it posts to.
    pub runs: String,
    /// Whether it may run here.
    pub state: TrustState,
    /// `sha256:<hex>` over its definition and files.
    pub digest: String,
    /// The repository files it runs.
    pub files: Vec<HookFile>,
    /// Programs it names outside the repository.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside: Vec<String>,
    /// Its definition as TOML, as a trust prompt shows it.
    pub definition: String,
    /// The definition last trusted, for a changed hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_definition: Option<String>,
    /// Its recent runs, oldest first.
    pub recent: Vec<HookRun>,
    /// The flag that trusts it in CI: `--trust-hook name=sha256:…`.
    pub trust_flag: String,
    /// What keeps it from running here, with the fix (a missing program
    /// or signing variable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// How `speccoder hooks test` went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HookTested {
    /// The hook.
    pub name: String,
    /// `passed` or `blocked` (a gate), `delivered` or `failed` (an observer).
    pub outcome: String,
    /// Why it blocked or failed, or what the receiver answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// How long it took.
    pub duration_ms: u64,
}

/// `speccoder hooks`: every hook, and what a `trust`, `test` or `deliver`
/// just did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HooksReport {
    /// Every hook, by name.
    pub hooks: Vec<HookEntry>,
    /// Why the hooks could not be read (`HOOK-MISCONFIGURED`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The hooks just trusted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted: Vec<String>,
    /// A test just run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested: Option<HookTested>,
    /// Events just delivered, per observer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered: Option<u32>,
    /// Events still waiting for an observer that failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hook(v: Value) -> Result<HookDef, String> {
        parse("licence", &v)
    }

    #[test]
    fn a_table_with_at_is_a_gate_and_one_with_on_an_observer() {
        let g = hook(json!({"at": "tick", "argv": ["cargo", "deny", "check"]})).unwrap();
        assert_eq!(g.at(), Some(Point::Tick));
        assert_eq!(g.timeout, 60);
        assert_eq!(g.listens(), "before tick");
        let o =
            hook(json!({"on": ["*"], "url": "https://example.com/h", "secret_env": "S"})).unwrap();
        assert!(o.hears("task.stopped"));
        assert_eq!(o.timeout, 10);
        assert_eq!(o.listens(), "hears task.ticked, task.stopped");
        let only = hook(json!({"on": ["task.ticked"], "argv": ["./n.sh"]})).unwrap();
        assert!(only.hears("task.ticked") && !only.hears("task.stopped"));
    }

    #[test]
    fn every_misconfiguration_names_its_fix() {
        for (v, words) in [
            (
                json!({"at": "tick", "on": ["*"], "argv": ["x"]}),
                "keep one",
            ),
            (json!({"argv": ["x"]}), "add `at"),
            (json!({"at": "commit", "argv": ["x"]}), "\"tick\" or \"pr\""),
            (json!({"on": ["task.done"], "argv": ["x"]}), "not an event"),
            (json!({"on": [], "argv": ["x"]}), "names the events"),
            (
                json!({"at": "tick", "url": "https://x.dev"}),
                "only an observer",
            ),
            (
                json!({"on": ["*"], "url": "http://example.com"}),
                "plain http",
            ),
            (json!({"at": "tick"}), "add `argv"),
            (json!({"at": "tick", "argv": []}), "program first"),
            (
                json!({"at": "tick", "argv": ["x"], "tiemout": 5}),
                "unknown key `tiemout`",
            ),
            (
                json!({"at": "tick", "argv": ["x"], "timeout": 0}),
                "1 to 600",
            ),
            (
                json!({"at": "tick", "argv": ["x"], "timeout": 601}),
                "1 to 600",
            ),
            (
                json!({"on": ["*"], "argv": ["x"], "network": true}),
                "for gates",
            ),
            (
                json!({"on": ["*"], "argv": ["x"], "secret_env": "S"}),
                "signs a `url`",
            ),
            (
                json!({"at": "tick", "argv": "cargo deny"}),
                "list of strings",
            ),
        ] {
            let e = hook(v.clone()).unwrap_err();
            assert!(e.starts_with("[hooks.licence]: "), "{e}");
            assert!(e.contains(words), "{v} → {e}");
        }
        assert!(parse("Licence", &json!({"at": "tick", "argv": ["x"]})).is_err());
    }

    #[test]
    fn plain_http_only_to_this_machine() {
        assert!(url_allowed("https://hooks.slack.com/x").is_ok());
        for ok in [
            "http://localhost:8080/h",
            "http://127.0.0.1/h",
            "http://[::1]:9/h",
        ] {
            assert!(url_allowed(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://example.com",
            "http://localhost.evil.com/",
            "http://user@10.0.0.1/",
            "ftp://x",
            "https://",
        ] {
            assert!(url_allowed(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn key_order_does_not_change_the_definition() {
        let a = hook(json!({"at": "tick", "argv": ["x"], "timeout": 5})).unwrap();
        let b = hook(json!({"timeout": 5, "argv": ["x"], "at": "tick"})).unwrap();
        assert_eq!(a.definition.to_string(), b.definition.to_string());
    }

    fn exit(code: Option<i32>) -> GateExit<'static> {
        GateExit {
            exit_code: code,
            timeout: 60,
            ..GateExit::default()
        }
    }

    #[test]
    fn only_exit_zero_passes() {
        assert_eq!(GateOutcome::of(&exit(Some(0))), GateOutcome::Passed);
        let blocked = |e: &GateExit<'_>| match GateOutcome::of(e) {
            GateOutcome::Blocked(r) => r,
            GateOutcome::Passed => panic!("{e:?} passed"),
        };
        assert_eq!(blocked(&exit(Some(1))), "exit 1");
        assert_eq!(blocked(&exit(Some(2))), "exit 2");
        assert_eq!(blocked(&exit(None)), "was killed");
        let words = GateExit {
            stderr: "checking…\nerror: GPL-3.0 in `foo`\n\n",
            ..exit(Some(1))
        };
        assert_eq!(blocked(&words), "error: GPL-3.0 in `foo`");
        let stdout = GateExit {
            stdout: "2 problems",
            ..exit(Some(3))
        };
        assert_eq!(blocked(&stdout), "2 problems");
        let late = GateExit {
            timed_out: true,
            ..exit(None)
        };
        assert_eq!(blocked(&late), "timed out after 60 s");
        let missing = GateExit {
            spawn_error: Some("`cargo-deny` not found"),
            ..exit(None)
        };
        assert_eq!(blocked(&missing), "could not start: `cargo-deny` not found");
        let changed = ["Cargo.lock".to_owned()];
        let wrote = GateExit {
            changed: &changed,
            ..exit(Some(0))
        };
        assert_eq!(blocked(&wrote), "changed `Cargo.lock`");
    }

    #[test]
    fn no_end_but_a_clean_exit_zero_passes() {
        let changed = ["a".to_owned()];
        for code in [
            None,
            Some(-1),
            Some(0),
            Some(1),
            Some(2),
            Some(127),
            Some(255),
        ] {
            for timed_out in [false, true] {
                for spawn in [None, Some("x")] {
                    for ch in [&[][..], &changed[..]] {
                        let e = GateExit {
                            exit_code: code,
                            timed_out,
                            spawn_error: spawn,
                            changed: ch,
                            timeout: 1,
                            ..GateExit::default()
                        };
                        let clean_zero =
                            code == Some(0) && !timed_out && spawn.is_none() && ch.is_empty();
                        assert_eq!(
                            GateOutcome::of(&e) == GateOutcome::Passed,
                            clean_zero,
                            "{e:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_reason_is_one_short_line() {
        assert_eq!(clean("a\tb\u{1b}[31m"), "a b\\u{1b}[31m");
        let long = "x".repeat(400);
        let c = clean(&long);
        assert_eq!(c.chars().count(), MAX_REASON);
        assert!(c.ends_with('…'));
    }

    #[test]
    fn the_summaries_read_as_sentences() {
        let t = TaskTicked {
            feature: "001-md2csv".into(),
            task: "T004".into(),
            verdict: "verified".into(),
            commit: "4f1c".into(),
            scenarios: vec!["US1-1".into(), "US1-2".into(), "US1-3".into()],
            held_out: Some(Count {
                passed: 2,
                total: 2,
            }),
            acceptance_strength: None,
            gates: vec![],
            executor: None,
            run: None,
            summary: String::new(),
        };
        assert_eq!(
            ticked_summary(&t),
            "T004 verified in 001-md2csv — 3 scenarios, held-out 2 of 2"
        );
        let mut s = TaskStopped {
            feature: "001-md2csv".into(),
            task: "T005".into(),
            outcome: "verification_failed".into(),
            code: Some("CHECK-FAILED".into()),
            attempt_ref: "refs/x".into(),
            gates: vec![],
            run: None,
            summary: String::new(),
        };
        assert_eq!(
            stopped_summary(&s),
            "T005 stopped in 001-md2csv: verification failed (CHECK-FAILED)"
        );
        s.outcome = "blocked_by_gate".into();
        s.gates = vec![GateMark {
            name: "licence".into(),
            outcome: GateState::Blocked,
        }];
        assert_eq!(stopped_summary(&s), "T005 blocked by licence in 001-md2csv");
    }
}
