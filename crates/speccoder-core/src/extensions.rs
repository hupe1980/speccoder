//! Extensions: a folder of someone else's work — gates, observers, views,
//! skills, MCP servers — that a person trusts by one digest. The pure
//! parts live here: names, the `[views.<name>]` grammar, what each
//! contribution can reach in words, what widened since the last trust, and
//! SARIF findings as speccoder shows them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hooks::{HookFile, TrustState};

/// The most files one extension may hold.
pub const MAX_FILES: usize = 2_000;
/// The most bytes one extension may hold.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// The most findings kept from one source.
pub const MAX_FINDINGS: usize = 200;
/// The largest SARIF file read.
pub const MAX_SARIF: usize = 1024 * 1024;
/// The longest finding message kept, in characters.
const MAX_MESSAGE: usize = 500;

/// Where an extension comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionSource {
    /// An Agent Plugins 1.0 folder in `.speccoder/extensions/`.
    AgentPlugins,
    /// A Spec Kit extension in `.specify/extensions/`.
    SpecKit,
}

/// Where `speccoder extensions add` copied an extension from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AddedFrom {
    /// The repository, or the folder.
    pub url: String,
    /// The commit, for a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// When (RFC 3339).
    pub added: String,
}

/// What kind of thing an extension contributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    /// A `[hooks]` gate.
    Gate,
    /// A `[hooks]` observer.
    Observer,
    /// A `[views]` view.
    View,
    /// A `skills/<name>/SKILL.md`.
    Skill,
    /// An `mcp.json` server.
    McpServer,
    /// A Spec Kit command.
    Command,
    /// A Spec Kit hook offered at its step.
    HookOffer,
}

/// One contribution and what it can reach, in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Contribution {
    /// Its kind.
    pub kind: ContributionKind,
    /// Its qualified name: `threatspec.check`, `/threatspec:model`.
    pub name: String,
    /// What it does and reaches: `gate before tick: runs ./check.sh — no network`.
    pub line: String,
    /// What it reaches that a narrower one would not: `network`,
    /// `host:hooks.example.com`, `event:task.stopped`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reaches: Vec<String>,
}

impl Contribution {
    /// Its kind in a word or two: `gate`, `MCP server`.
    #[must_use]
    pub const fn kind_words(&self) -> &'static str {
        match self.kind {
            ContributionKind::Gate => "gate",
            ContributionKind::Observer => "observer",
            ContributionKind::View => "view",
            ContributionKind::Skill => "skill",
            ContributionKind::McpServer => "MCP server",
            ContributionKind::Command => "command",
            ContributionKind::HookOffer => "hook",
        }
    }
}

/// What changed since an extension was trusted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExtensionChanges {
    /// Files added, changed or gone.
    pub files: Vec<String>,
    /// Contributions that are new or reach further, first in any listing.
    pub widened: Vec<String>,
    /// Contributions that went.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<String>,
}

/// One extension as `speccoder extensions` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Extension {
    /// Its name.
    pub name: String,
    /// Agent Plugins or Spec Kit.
    pub source: ExtensionSource,
    /// Its version, as it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// What it says it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Its folder, relative to the project.
    pub folder: String,
    /// Where it was added from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<AddedFrom>,
    /// `sha256:<hex>` over every file.
    pub digest: String,
    /// Every file and its digest.
    pub files: Vec<HookFile>,
    /// What it contributes.
    pub contributions: Vec<Contribution>,
    /// Whether it may run here.
    pub trust: TrustState,
    /// For a changed extension: what changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<ExtensionChanges>,
    /// The flag that trusts it in CI: `--trust-extension name=sha256:…`.
    pub trust_flag: String,
    /// Why it cannot be loaded; nothing of it is offered while any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

/// `speccoder extensions`: every extension, and what was just done.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExtensionsReport {
    /// Every extension, by name.
    pub extensions: Vec<Extension>,
    /// Extensions just trusted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted: Vec<String>,
    /// The extension just added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    /// The extension just removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<String>,
}

/// Whether `name` may name an extension (Agent Plugins 1.0): 1–64 of
/// `a-z0-9-.`, a letter or digit at both ends, no `--` or `..`.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    (1..=64).contains(&b.len())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'.'))
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
        && !name.contains("--")
        && !name.contains("..")
}

/// The MCP server name of an extension's server: `<ext>_<server>`, every
/// run of `-`, `.` and `_` as one `_` (a tool reference's component:
/// lowercase letters, digits and single `_` inside).
#[must_use]
pub fn server_name(extension: &str, server: &str) -> String {
    let mut out = String::new();
    for c in format!("{extension}_{server}").to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        if !(c == '_' && (out.is_empty() || out.ends_with('_'))) {
            out.push(c);
        }
    }
    out.trim_end_matches('_').to_owned()
}

// ── views ──────────────────────────────────────────────────────────────

/// How a view shows its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewWidget {
    /// SARIF findings.
    Findings,
    /// Rows with columns.
    Table,
    /// Named numbers or short words.
    Metrics,
    /// A document.
    Markdown,
}

/// Where a view stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewPlace {
    /// On each feature's page, about that feature.
    #[default]
    Feature,
    /// On the project's overview.
    Project,
}

/// Where a view's data comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewSource {
    /// A command, run in the sandbox.
    Run(Vec<String>),
    /// A file, read as it is.
    File(String),
}

/// One `[views.<name>]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewDef {
    /// The table's name.
    pub name: String,
    /// Its heading.
    pub title: String,
    /// Its data.
    pub source: ViewSource,
    /// How it shows.
    pub widget: ViewWidget,
    /// A dotted path into JSON output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// A table's columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// Where it stands.
    pub place: ViewPlace,
    /// Seconds a command may run.
    pub timeout: u64,
    /// Exit codes whose output is still data.
    pub exit: Vec<i32>,
}

const VIEW_KEYS: &[&str] = &[
    "title", "run", "file", "widget", "path", "columns", "place", "timeout", "exit",
];

/// Parses `[views.<name>]` (as JSON).
///
/// # Errors
///
/// What is wrong, with the fix.
pub fn parse_view(name: &str, table: &Value) -> Result<ViewDef, String> {
    let at = |m: &str| format!("[views.{name}]: {m}");
    if !crate::hooks::valid_name(name) {
        return Err(at(
            "a view's name is lowercase letters, digits, `-` and `_`",
        ));
    }
    let Some(t) = table.as_object() else {
        return Err(at("must be a table"));
    };
    if let Some(k) = t.keys().find(|k| !VIEW_KEYS.contains(&k.as_str())) {
        return Err(at(&format!(
            "unknown key `{k}` (known: {})",
            VIEW_KEYS.join(", ")
        )));
    }
    let string = |k: &str| -> Result<Option<String>, String> {
        match t.get(k) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(at(&format!("`{k}` is a string"))),
        }
    };
    let strings = |k: &str| -> Result<Option<Vec<String>>, String> {
        match t.get(k) {
            None => Ok(None),
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .map(Some)
                .ok_or_else(|| at(&format!("`{k}` is a list of strings"))),
            Some(_) => Err(at(&format!("`{k}` is a list of strings"))),
        }
    };
    let title = string("title")?.unwrap_or_else(|| name.to_owned());
    if title.chars().count() > 60 {
        return Err(at("`title` is at most 60 characters"));
    }
    let source = match (strings("run")?, string("file")?) {
        (Some(_), Some(_)) => return Err(at("give `run` or `file`, not both")),
        (None, None) => return Err(at("add `run = [\"…\"]` or `file = \"…\"`")),
        (Some(a), None) if a.first().is_none_or(|p| p.trim().is_empty()) => {
            return Err(at("`run` names the program first"));
        }
        (Some(a), None) => ViewSource::Run(a),
        (None, Some(f)) => {
            if f.is_empty() || f.starts_with('/') || f.split(['/', '\\']).any(|s| s == "..") {
                return Err(at(&format!(
                    "`file`: `{f}` is not a path inside the project"
                )));
            }
            ViewSource::File(f)
        }
    };
    let widget = match string("widget")?.as_deref() {
        Some("findings") => ViewWidget::Findings,
        Some("table") => ViewWidget::Table,
        Some("metrics") => ViewWidget::Metrics,
        Some("markdown") => ViewWidget::Markdown,
        Some(w) => {
            return Err(at(&format!(
                "`widget = \"{w}\"` is not supported by this version (findings, table, metrics, markdown)"
            )));
        }
        None => return Err(at("add `widget` (findings, table, metrics, markdown)")),
    };
    let place = match string("place")?.as_deref() {
        None | Some("feature") => ViewPlace::Feature,
        Some("project") => ViewPlace::Project,
        Some(p) => return Err(at(&format!("`place = \"{p}\"`: feature or project"))),
    };
    let timeout = match t.get("timeout") {
        None => 60,
        Some(v) => match v.as_u64() {
            Some(s) if (1..=crate::hooks::MAX_TIMEOUT).contains(&s) => s,
            _ => {
                return Err(at(&format!(
                    "`timeout` is seconds, 1 to {}",
                    crate::hooks::MAX_TIMEOUT
                )));
            }
        },
    };
    let exit = match t.get("exit") {
        None => vec![0],
        Some(Value::Array(a)) if !a.is_empty() => a
            .iter()
            .map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| at("`exit` is a list of exit codes"))?,
        Some(_) => return Err(at("`exit` is a list of exit codes")),
    };
    if !matches!(source, ViewSource::Run(_)) && t.contains_key("exit") {
        return Err(at("`exit` is for a `run` source"));
    }
    Ok(ViewDef {
        name: name.to_owned(),
        title,
        source,
        widget,
        path: string("path")?,
        columns: strings("columns")?.unwrap_or_default(),
        place,
        timeout,
        exit,
    })
}

/// A view's data, ready for its widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "widget")]
pub enum ViewData {
    /// Findings.
    Findings {
        /// Each finding.
        findings: Vec<SarifFinding>,
    },
    /// Rows.
    Table {
        /// The columns.
        columns: Vec<String>,
        /// Each row's cells, as text.
        rows: Vec<Vec<String>>,
    },
    /// Named values.
    Metrics {
        /// `(name, value)`, values as text.
        metrics: Vec<(String, String)>,
    },
    /// A document, rendered (raw HTML shown as text).
    Markdown {
        /// The HTML.
        html: String,
    },
}

/// A view's last result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewResult {
    /// The digest of what it read.
    pub inputs: String,
    /// When it ran (RFC 3339).
    pub ran_at: String,
    /// How long.
    pub duration_ms: u64,
    /// Its data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ViewData>,
    /// Why there is no data, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One view as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewShown {
    /// The extension.
    pub extension: String,
    /// The view.
    pub view: ViewDef,
    /// Its result for the current inputs; none while it waits for trust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ViewResult>,
    /// The extension waits for trust (or changed since).
    pub waits_for_trust: bool,
}

/// The views placed on a feature (or the project).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ViewsReport {
    /// Every view, by extension then name.
    pub views: Vec<ViewShown>,
}

// ── what each contribution can reach ───────────────────────────────────

/// The host of an http(s) URL.
#[must_use]
pub fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.rsplit_once('@').map_or(host, |(_, h)| h).to_owned()
}

/// A command as a person reads it: its words, quoted where needed.
#[must_use]
pub fn shown(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty() || a.contains([' ', '"', '\'']) {
                format!("{a:?}")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A hook's contribution, `name` qualified (`threatspec.check`).
#[must_use]
pub fn hook_contribution(name: &str, def: &crate::hooks::HookDef) -> Contribution {
    use crate::hooks::{Role, Runner};
    match (&def.role, &def.runner) {
        (Role::Gate { at, network }, runner) => {
            let runs = match runner {
                Runner::Argv(a) => shown(a),
                Runner::Url { url, .. } => url.clone(),
            };
            let net = if *network { "network on" } else { "no network" };
            Contribution {
                kind: ContributionKind::Gate,
                name: name.to_owned(),
                line: format!("gate {}: runs {runs} — {net}", at.words()),
                reaches: if *network {
                    vec!["network".to_owned()]
                } else {
                    Vec::new()
                },
            }
        }
        (Role::Observer { on }, runner) => {
            let mut reaches: Vec<String> = on.iter().map(|e| format!("event:{e}")).collect();
            let does = match runner {
                Runner::Argv(a) => {
                    reaches.push("network".to_owned());
                    format!("runs {} (network on)", shown(a))
                }
                Runner::Url { url, .. } => {
                    let h = host(url);
                    reaches.push(format!("host:{h}"));
                    format!("posts to {h}")
                }
            };
            Contribution {
                kind: ContributionKind::Observer,
                name: name.to_owned(),
                line: format!("observer, {}: {does}", def.listens()),
                reaches,
            }
        }
    }
}

/// A view's contribution, `name` qualified.
#[must_use]
pub fn view_contribution(name: &str, v: &ViewDef) -> Contribution {
    let place = match v.place {
        ViewPlace::Feature => "on each feature",
        ViewPlace::Project => "on the project",
    };
    let from = match &v.source {
        ViewSource::Run(a) => format!("runs {} read-only, no network", shown(a)),
        ViewSource::File(f) => format!("reads {f}"),
    };
    Contribution {
        kind: ContributionKind::View,
        name: name.to_owned(),
        line: format!("view “{}” {place}: {from}", v.title),
        reaches: Vec::new(),
    }
}

/// What changed between the contributions trusted (`before`) and now.
#[must_use]
pub fn widened(before: &[Contribution], now: &[Contribution]) -> (Vec<String>, Vec<String>) {
    let mut widened = Vec::new();
    for c in now {
        match before.iter().find(|b| b.name == c.name && b.kind == c.kind) {
            None => widened.push(format!("new — {}", c.line)),
            Some(b) => {
                let more: Vec<&String> = c
                    .reaches
                    .iter()
                    .filter(|r| !b.reaches.contains(r))
                    .collect();
                if !more.is_empty() {
                    let more: Vec<&str> = more.iter().map(|s| s.as_str()).collect();
                    widened.push(format!("reaches {} — {}", more.join(", "), c.line));
                }
            }
        }
    }
    let gone = before
        .iter()
        .filter(|b| !now.iter().any(|c| c.name == b.name && c.kind == b.kind))
        .map(|b| b.line.clone())
        .collect();
    (widened, gone)
}

// ── findings ───────────────────────────────────────────────────────────

/// One finding, as speccoder shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SarifFinding {
    /// The rule's id.
    pub rule: String,
    /// `error`, `warning`, `note` or `none`.
    pub level: String,
    /// What it says.
    pub message: String,
    /// The file, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
}

/// The findings of a SARIF 2.1.0 log: at most [`MAX_FINDINGS`], each
/// message cut to 500 characters, each location made relative to the
/// project by `relative` (`None`: outside it).
///
/// # Errors
///
/// Why the log cannot be shown: too large, not JSON, not SARIF, or a
/// location outside the project.
pub fn sarif(
    text: &str,
    relative: impl Fn(&str) -> Option<String>,
) -> Result<Vec<SarifFinding>, String> {
    if text.len() > MAX_SARIF {
        return Err(format!(
            "the findings are {} KB; at most {} KB are read",
            text.len() / 1024,
            MAX_SARIF / 1024
        ));
    }
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let Some(runs) = v.get("runs").and_then(Value::as_array) else {
        return Err("not SARIF: no `runs`".to_owned());
    };
    let mut out = Vec::new();
    for r in runs.iter().flat_map(|r| {
        r.get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }) {
        if out.len() == MAX_FINDINGS {
            break;
        }
        let loc = &r["locations"][0]["physicalLocation"];
        let file = match loc["artifactLocation"]["uri"].as_str() {
            None => None,
            Some(u) => {
                let u = u.trim_start_matches("file://").trim_start_matches("./");
                Some(
                    relative(u)
                        .ok_or_else(|| format!("a finding names `{u}`, outside the project"))?,
                )
            }
        };
        let message: String = r["message"]["text"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(MAX_MESSAGE)
            .collect();
        out.push(SarifFinding {
            rule: r["ruleId"].as_str().unwrap_or("finding").to_owned(),
            level: match r["level"].as_str() {
                Some(l @ ("error" | "warning" | "note" | "none")) => l.to_owned(),
                _ => "warning".to_owned(),
            },
            message,
            file,
            line: loc["region"]["startLine"].as_u64(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_follow_agent_plugins() {
        for ok in ["threatspec", "a", "my.tool-2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "-a", "a-", "A", "a--b", "a..b", "a_b", &"x".repeat(65)] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert_eq!(server_name("my.tool", "echo_1"), "my_tool_echo_1");
        assert_eq!(server_name("plain-plugin", "notes"), "plain_plugin_notes");
    }

    #[test]
    fn a_view_has_one_source_and_a_widget() {
        let v = parse_view(
            "findings",
            &json!({"run": ["./check.sh", "--sarif"], "widget": "findings", "exit": [0, 1, 2]}),
        )
        .unwrap();
        assert_eq!(v.exit, vec![0, 1, 2]);
        assert_eq!(v.timeout, 60);
        assert_eq!(v.place, ViewPlace::Feature);
        assert!(parse_view("x", &json!({"widget": "table"})).is_err());
        assert!(parse_view("x", &json!({"run": ["a"], "file": "b", "widget": "table"})).is_err());
        let e = parse_view("x", &json!({"file": "a.md", "widget": "chart"})).unwrap_err();
        assert!(e.contains("not supported by this version"), "{e}");
        assert!(parse_view("x", &json!({"file": "../a.md", "widget": "markdown"})).is_err());
        assert!(
            parse_view(
                "x",
                &json!({"file": "a.md", "widget": "markdown", "colour": 1})
            )
            .is_err()
        );
        assert!(
            parse_view(
                "x",
                &json!({"file": "a.md", "widget": "markdown", "exit": [1]})
            )
            .is_err()
        );
    }

    #[test]
    fn a_new_host_or_the_network_widens() {
        let gate = |net: bool| {
            let def = crate::hooks::parse(
                "check",
                &json!({"at": "tick", "argv": ["./check.sh"], "network": net}),
            )
            .unwrap();
            hook_contribution("x.check", &def)
        };
        let off = gate(false);
        assert!(
            off.line
                .contains("gate before tick: runs ./check.sh — no network"),
            "{}",
            off.line
        );
        let (w, gone) = widened(std::slice::from_ref(&off), &[gate(true)]);
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("reaches network"), "{w:?}");
        assert!(gone.is_empty());
        let obs = crate::hooks::parse(
            "notify",
            &json!({"on": ["task.stopped"], "url": "https://hooks.example.com/x"}),
        )
        .unwrap();
        let (w, _) = widened(&[off], &[hook_contribution("x.notify", &obs)]);
        assert!(w[0].starts_with("new — observer"), "{w:?}");
        assert!(w[0].contains("posts to hooks.example.com"), "{w:?}");
    }

    #[test]
    fn sarif_is_read_within_bounds() {
        let log = json!({"version": "2.1.0", "runs": [{"results": [
            {"ruleId": "TS-1", "level": "error", "message": {"text": "unmitigated"},
             "locations": [{"physicalLocation": {"artifactLocation": {"uri": "specs/a/spec.md"},
                                                 "region": {"startLine": 3}}}]}]}]});
        let rel = |p: &str| (!p.starts_with('/')).then(|| p.to_owned());
        let f = sarif(&log.to_string(), rel).unwrap();
        assert_eq!(f[0].rule, "TS-1");
        assert_eq!(f[0].line, Some(3));
        let outside = log.to_string().replace("specs/a/spec.md", "/etc/passwd");
        assert!(sarif(&outside, rel).is_err());
        assert!(sarif("{}", rel).is_err());
        assert!(sarif(&"x".repeat(MAX_SARIF + 1), rel).is_err());
    }
}
