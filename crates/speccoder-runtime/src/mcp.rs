//! MCP servers' tools, under speccoder's policy.
//!
//! `[mcp.<name>]` in `speccoder.toml` names a stdio server (`command`) and,
//! optionally, tools to `allow` without approval or to `deny`. For each run
//! speccoder starts the servers in the worktree with a scrubbed environment,
//! lists their tools and offers them to the model as `mcp__<server>__<tool>`
//! with strict schemas. speccoder cannot know what an MCP tool does, so a
//! call needs approval unless allowed — and whatever it writes in the
//! worktree is still seen by the diff, the integrity audit and the verifier.
//! The discovered tools ride in the attempt's journaled input.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use agentplane::tools::rmcp;
use agentplane::tools::rmcp::service::{ClientLifecycleMode, ClientServiceExt as _};
use agentplane::tools::{Destination, McpClient, ToolClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The prefix of an MCP tool's name as the model sees it.
pub const PREFIX: &str = "mcp__";

/// How long a server may take to start and list its tools.
const STARTUP: Duration = Duration::from_secs(30);

/// One configured server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// `[mcp.<name>]`; the tool server name in agentplane.
    pub name: String,
    /// Program and arguments, run without a shell in the worktree.
    pub command: Vec<String>,
    /// Tools called without approval.
    pub allow: Vec<String>,
    /// Tools never called.
    pub deny: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServer {
    command: Vec<String>,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
}

/// Reads `[mcp.<name>]` sections.
pub fn parse(text: &str) -> Result<Vec<ServerConfig>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(mcp) = table.get("mcp") else {
        return Ok(Vec::new());
    };
    let mcp = mcp
        .as_table()
        .ok_or_else(|| "`mcp` must be a table of [mcp.<name>] tables".to_owned())?;
    mcp.iter()
        .map(|(name, v)| {
            let valid = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !matches!(name.as_str(), "fs" | "proc" | "git");
            if !valid {
                return Err(format!(
                    "[mcp.{name}]: a server name is lowercase letters, digits and `-`, and not fs, proc or git"
                ));
            }
            let raw: RawServer = v
                .clone()
                .try_into()
                .map_err(|e| format!("[mcp.{name}]: {e}"))?;
            if raw.command.first().is_none_or(String::is_empty) {
                return Err(format!("[mcp.{name}]: `command` must name a program"));
            }
            Ok(ServerConfig {
                name: name.clone(),
                command: raw.command,
                allow: raw.allow,
                deny: raw.deny,
            })
        })
        .collect()
}

/// A tool as offered to the model (journaled with each attempt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTool {
    /// `mcp__<server>__<tool>`.
    pub name: String,
    /// The server.
    pub server: String,
    /// The tool's own name on the server.
    pub tool: String,
    /// What the server says it does.
    pub description: String,
    /// Its input schema, in the strict-calling subset.
    pub schema: Value,
    /// `allow`, `approval` or `deny`.
    pub decision: String,
    /// The argument fields the model authors (`/<property>`), for the grant.
    pub fields: Vec<String>,
    /// agentplane's digest over the tool's name, description, schema and
    /// annotations as discovered: what a person reviewed.
    pub digest: String,
}

/// Running servers and their tools.
#[derive(Debug, Default)]
pub struct Connected {
    /// The tools offered.
    pub tools: Vec<McpTool>,
    /// One tool client per server, by server name.
    pub clients: Vec<(String, Arc<dyn ToolClient>)>,
    /// What could not be used, and why.
    pub notices: Vec<String>,
}

/// The model-facing name, if it is a valid tool name (≤ 64 of `[A-Za-z0-9_-]`).
fn model_name(server: &str, tool: &str) -> Option<String> {
    let name = format!("{PREFIX}{}__{tool}", server.replace('-', "_"));
    (name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then_some(name)
}

/// Starts every configured server in `cwd` and lists its tools; a server
/// that fails is reported, not fatal.
pub async fn connect(
    servers: &[ServerConfig],
    cwd: &Path,
    runner: &crate::sandbox::Runner,
) -> Connected {
    let mut out = Connected::default();
    for s in servers {
        match connect_one(s, cwd, runner).await {
            Ok((client, tools, notices)) => {
                out.clients.push((s.name.clone(), client));
                out.tools.extend(tools);
                out.notices.extend(notices);
            }
            Err(e) => out
                .notices
                .push(format!("MCP server `{}` is not used: {e}", s.name)),
        }
    }
    out
}

async fn connect_one(
    s: &ServerConfig,
    cwd: &Path,
    runner: &crate::sandbox::Runner,
) -> Result<(Arc<dyn ToolClient>, Vec<McpTool>, Vec<String>), String> {
    // Servers run confined like every other command.
    let (argv, tmp) = runner.wrap(&s.command)?;
    let mut command = tokio::process::Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(crate::verify::process::scrubbed_env(std::env::vars()))
        .kill_on_drop(true);
    if let Some(t) = tmp {
        command.env("TMPDIR", &t).env("TMP", &t).env("TEMP", &t);
    }
    let transport = rmcp::transport::TokioChildProcess::new(command)
        .map_err(|e| format!("cannot start `{}`: {e}", s.command.join(" ")))?;
    let service = tokio::time::timeout(
        STARTUP,
        McpClient::host_info().serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Auto {
                preferred_versions: vec![agentplane::tools::MCP_REVISION],
                legacy_version: Some(McpClient::LEGACY_REVISION),
            },
        ),
    )
    .await
    .map_err(|_| "it did not initialise within 30 s".to_owned())?
    .map_err(|e| format!("it did not initialise: {e}"))?;
    let service = Arc::new(service);
    let client = McpClient::new(s.name.clone(), Arc::clone(&service), Destination::Local)
        .map_err(|e| e.to_string())?;
    // Each tool's description, schema and a digest over all of it.
    let listed = tokio::time::timeout(STARTUP, client.discover())
        .await
        .map_err(|_| "it did not list its tools within 30 s".to_owned())?
        .map_err(|e| format!("tools/list failed: {e}"))?;

    let mut tools = Vec::new();
    let mut notices = Vec::new();
    for t in listed {
        let tool = t.id.tool.clone();
        let Some(name) = model_name(&s.name, &tool) else {
            notices.push(format!(
                "MCP tool `{}/{tool}` skipped: its name cannot be a tool name",
                s.name
            ));
            continue;
        };
        // The server's schema, its objects closed (that only narrows what
        // the model may send); optional properties are not made nullable,
        // which would change what the server is sent. One that is still
        // not strict-ready is sent non-strict.
        let schema = crate::tools::close_objects(t.input_schema.clone());
        if let Some(problem) = agentplane::model::strict_schema_problem(&schema) {
            notices.push(format!(
                "MCP tool `{}/{tool}` is sent without strict decoding: {problem}",
                s.name
            ));
        }
        let decision = if s.deny.contains(&tool) {
            "deny"
        } else if s.allow.contains(&tool) {
            "allow"
        } else {
            "approval"
        };
        let fields = schema["properties"]
            .as_object()
            .map(|p| p.keys().map(|k| format!("/{k}")).collect())
            .unwrap_or_default();
        tools.push(McpTool {
            name,
            server: s.name.clone(),
            tool,
            description: if t.description.is_empty() {
                "(no description)".to_owned()
            } else {
                t.description.clone()
            },
            schema,
            decision: decision.to_owned(),
            fields,
            digest: t.digest.to_hex(),
        });
    }
    Ok((Arc::new(client), tools, notices))
}

/// Where the digests of reviewed tools are kept, in the project's state directory.
pub const REVIEWED: &str = "mcp-reviewed.json";

fn load_reviewed(state: &Path) -> std::collections::BTreeMap<String, String> {
    std::fs::read_to_string(state.join(REVIEWED))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_reviewed(state: &Path, map: &std::collections::BTreeMap<String, String>) {
    let _ = std::fs::create_dir_all(state);
    if let Ok(text) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(state.join(REVIEWED), text);
    }
}

/// Holds allowed tools to what a person reviewed: the first time an
/// allowed tool is seen its digest is recorded; when a server later changes
/// it — description, schema or annotations — the tool needs approval again
/// and a notice says so. `approved` are the tools a person approved for
/// this task since (`speccoder approve`), which re-reviews them.
pub fn pin_reviewed(state: &Path, tools: &mut [McpTool], approved: &[String]) -> Vec<String> {
    let mut map = load_reviewed(state);
    let mut notices = Vec::new();
    for t in tools.iter_mut().filter(|t| t.decision == "allow") {
        let key = format!("{}/{}", t.server, t.tool);
        match map.get(&key) {
            None => {
                map.insert(key, t.digest.clone());
            }
            Some(seen) if *seen == t.digest => {}
            Some(_) if approved.contains(&t.name) => {
                map.insert(key, t.digest.clone());
            }
            Some(_) => {
                t.decision = "approval".to_owned();
                notices.push(format!(
                    "MCP tool `{key}` changed since it was allowed (description, schema or annotations); each call needs approval until a person approves one"
                ));
            }
        }
    }
    save_reviewed(state, &map);
    notices
}

/// Records that a person approved a call of `tool` (re-reviewed as it is now).
pub fn mark_reviewed(state: &Path, tools: &[McpTool], name: &str) {
    let Some(t) = tools.iter().find(|t| t.name == name) else {
        return;
    };
    let mut map = load_reviewed(state);
    map.insert(format!("{}/{}", t.server, t.tool), t.digest.clone());
    save_reviewed(state, &map);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_parse_and_bad_names_are_refused() {
        let s =
            parse("[mcp.notes]\ncommand = [\"python3\", \"notes.py\"]\nallow = [\"list_notes\"]\n")
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s[0].name, "notes");
        assert_eq!(s[0].allow, ["list_notes"]);
        assert!(parse("[mcp.fs]\ncommand = [\"x\"]\n").is_err());
        assert!(parse("[mcp.Bad]\ncommand = [\"x\"]\n").is_err());
        assert!(parse("[mcp.x]\ncommand = []\n").is_err());
        assert!(parse("").unwrap_or_default().is_empty());
        assert_eq!(
            model_name("my-db", "query").as_deref(),
            Some("mcp__my_db__query")
        );
        assert!(model_name("x", &"y".repeat(70)).is_none());
    }
}
