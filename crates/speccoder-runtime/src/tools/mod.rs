//! speccoder's tools: what an attempt may do to
//! its worktree, served to agentplane as tool servers `fs`, `proc` and `git`.
//!
//! Domain failures (no such file, ambiguous edit, protected path) come back
//! as ordinary results `{"ok": false, "error": …}` so the model can see them
//! and recover; agentplane `ToolError`s are reserved for infrastructure.

pub mod paths;
pub mod policy;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentplane::model::ToolDeclaration;
use agentplane::tools::{Destination, ToolClient, ToolError, ToolId};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use self::paths::{Protected, resolve_existing, resolve_for_write};
use self::policy::{Decision, Policy};
use crate::git::{Git, nul_fields};
use crate::verify::process::{Invocation, OUTPUT_CAP, scrubbed_env};

/// `read_file` cap.
pub const READ_CAP: usize = 256 * 1024;
/// `list_files` cap.
pub const LIST_CAP: usize = 500;
/// `search_code` match cap.
pub const SEARCH_CAP: usize = 200;
/// Characters kept per search-result line.
pub const LINE_CAP: usize = 500;
/// Longest `run_command` timeout a model may ask for.
pub const MAX_COMMAND_SECS: u64 = 600;

// ── Inputs (their JSON schemas are what the model sees) ─────────────────────

/// Read a text file in the workspace, optionally a range of lines.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFile {
    /// Path relative to the workspace root.
    pub path: String,
    /// First line to return (1-based).
    pub start_line: Option<usize>,
    /// Last line to return (inclusive).
    pub end_line: Option<usize>,
}

/// List files (not ignored by .gitignore), optionally under a directory and matching a glob.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFiles {
    /// Directory relative to the workspace root (default: the root).
    pub path: Option<String>,
    /// Glob such as `**/*.rs`.
    pub glob: Option<String>,
}

/// Search file contents with a regular expression (respects .gitignore).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchCode {
    /// Rust regex syntax.
    pub pattern: String,
    /// Directory relative to the workspace root (default: the root).
    pub path: Option<String>,
    /// Only files matching this glob.
    pub glob: Option<String>,
}

/// Create or replace a whole file.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteFile {
    /// Path relative to the workspace root.
    pub path: String,
    /// The complete new contents.
    pub content: String,
}

/// Replace an exact string in a file: once (it must occur exactly once) or everywhere.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditFile {
    /// Path relative to the workspace root.
    pub path: String,
    /// Exact text to find, including whitespace.
    pub old_string: String,
    /// Replacement text.
    pub new_string: String,
    /// Replace every occurrence instead of exactly one (default: one).
    pub replace_all: Option<bool>,
}

/// Run a program (no shell) in the workspace with a timeout; stdin is closed.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunCommand {
    /// Program and arguments, e.g. ["cargo", "test", "-p", "core"].
    pub argv: Vec<String>,
    /// Working directory relative to the workspace root (default: the root).
    pub cwd: Option<String>,
    /// Seconds before the whole process group is killed (default 120, at most 600).
    pub timeout_secs: Option<u64>,
    /// Commands run without the network; `true` asks for it (a person decides).
    pub network: Option<bool>,
    /// Set by speccoder, not the model: the call's id, exported as
    /// `SPECCODER_CALL_ID`.
    #[schemars(skip)]
    #[serde(default)]
    pub call_id: Option<String>,
}

/// Show changed, added and untracked files.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitStatus {}

/// Show the diff of the working tree against HEAD, optionally for one path.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitDiff {
    /// Limit the diff to this path.
    pub path: Option<String>,
}

/// One tool as the model and agentplane see it.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Name offered to the model, e.g. `read_file`.
    pub name: &'static str,
    /// agentplane tool server: `fs`, `proc` or `git`.
    pub server: &'static str,
    /// Whether calling it can change the world.
    pub mutates: bool,
    /// Argument pointers the model authors (declared on mutating grants).
    pub model_fields: &'static [&'static str],
    /// What the model is told.
    pub description: &'static str,
    /// JSON schema of the arguments.
    pub parameters: fn() -> Value,
}

/// The schema of a tool's arguments, made strict-ready by agentplane: only
/// what serde enforces is declared — an optional argument is an `Option`,
/// so `null` is both allowed and readable (D-076).
fn schema<T: JsonSchema>() -> Value {
    agentplane::tools::strict_schema::<T>()
}

/// Every tool, in a stable order (the order is part of the cached prompt prefix).
#[must_use]
pub fn catalog() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file",
            server: "fs",
            mutates: false,
            model_fields: &[],
            description: "Read a text file in the workspace, optionally a range of lines.",
            parameters: schema::<ReadFile>,
        },
        ToolSpec {
            name: "list_files",
            server: "fs",
            mutates: false,
            model_fields: &[],
            description: "List files not ignored by .gitignore, optionally under a directory and matching a glob.",
            parameters: schema::<ListFiles>,
        },
        ToolSpec {
            name: "search_code",
            server: "fs",
            mutates: false,
            model_fields: &[],
            description: "Search file contents with a regular expression; returns path:line: text.",
            parameters: schema::<SearchCode>,
        },
        ToolSpec {
            name: "write_file",
            server: "fs",
            mutates: true,
            model_fields: &["/path", "/content"],
            description: "Create or replace a whole file. Spec Kit files, CI and test configuration, speccoder.toml and .git are refused.",
            parameters: schema::<WriteFile>,
        },
        ToolSpec {
            name: "edit_file",
            server: "fs",
            mutates: true,
            model_fields: &["/path", "/old_string", "/new_string"],
            description: "Replace an exact string in a file (exactly one occurrence unless replace_all).",
            parameters: schema::<EditFile>,
        },
        ToolSpec {
            name: "run_command",
            server: "proc",
            mutates: true,
            model_fields: &["/argv"],
            description: "Run a program without a shell in the workspace; stdin is closed, output is capped, the timeout kills the process group.",
            parameters: schema::<RunCommand>,
        },
        ToolSpec {
            name: "git_status",
            server: "git",
            mutates: false,
            model_fields: &[],
            description: "Show modified, added, deleted and untracked files.",
            parameters: schema::<GitStatus>,
        },
        ToolSpec {
            name: "git_diff",
            server: "git",
            mutates: false,
            model_fields: &[],
            description: "Show the working tree's diff against HEAD.",
            parameters: schema::<GitDiff>,
        },
    ]
}

/// The model-facing declarations, in catalog order.
#[must_use]
pub fn declarations() -> Vec<ToolDeclaration> {
    catalog()
        .into_iter()
        .map(|t| ToolDeclaration::new(t.name, t.description, (t.parameters)()))
        .collect()
}

// ── The tool server ─────────────────────────────────────────────────────────

/// Tools bound to one worktree.
#[derive(Debug, Clone)]
pub struct WorkspaceTools {
    root: PathBuf,
    policy: Policy,
    runner: crate::sandbox::Runner,
}

fn failure(error: impl Into<String>) -> Value {
    json!({ "ok": false, "error": error.into() })
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, Value> {
    serde_json::from_value(args.clone()).map_err(|e| failure(format!("invalid arguments: {e}")))
}

impl WorkspaceTools {
    /// Tools for the worktree at `root` under `policy`.
    pub fn new(root: &Path, policy: Policy) -> std::io::Result<Self> {
        Ok(Self {
            root: fs::canonicalize(root)?,
            policy,
            runner: crate::sandbox::Runner::Plain,
        })
    }

    /// Runs commands with `runner` (the sandbox).
    #[must_use]
    pub fn with_runner(mut self, runner: crate::sandbox::Runner) -> Self {
        self.runner = runner;
        self
    }

    fn protected(&self) -> &Protected {
        self.policy.protected()
    }

    /// Runs one tool by name; the result is what the model sees.
    pub async fn run(&self, name: &str, args: &Value) -> Value {
        // Denials are enforced here too, whatever the caller decided.
        if let Decision::Deny(reason) = self.policy.classify(name, args) {
            return failure(format!("denied: {reason}"));
        }
        let out = match name {
            "read_file" => parse(args).map(|a| self.read_file(&a)),
            "list_files" => parse(args).map(|a| self.list_files(&a)),
            "search_code" => parse(args).map(|a| self.search_code(&a)),
            "write_file" => parse(args).map(|a| self.write_file(&a)),
            "edit_file" => parse(args).map(|a| self.edit_file(&a)),
            "run_command" => match parse::<RunCommand>(args) {
                Ok(a) => Ok(self.run_command(a).await),
                Err(e) => Err(e),
            },
            "git_status" => parse::<GitStatus>(args).map(|_| self.git_status()),
            "git_diff" => parse(args).map(|a| self.git_diff(&a)),
            other => Err(failure(format!("unknown tool `{other}`"))),
        };
        out.unwrap_or_else(|e| e)
    }

    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn read_file(&self, a: &ReadFile) -> Value {
        let path = match resolve_existing(&self.root, &a.path) {
            Ok(p) => p,
            Err(e) => return failure(e.0),
        };
        if self.protected().is_secret(&self.rel(&path)) {
            return failure(format!("`{}` looks like a credential", a.path));
        }
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => return failure(format!("`{}`: {e}", a.path)),
        };
        let Ok(text) = String::from_utf8(bytes.clone()) else {
            return json!({ "ok": true, "path": a.path, "binary": true, "bytes": bytes.len() });
        };
        let lines: Vec<&str> = text.lines().collect();
        let start = a.start_line.unwrap_or(1).max(1);
        let end = a.end_line.unwrap_or(lines.len()).min(lines.len());
        let mut content = if start > end {
            String::new()
        } else {
            lines[start - 1..end].join("\n")
        };
        let truncated = content.len() > READ_CAP;
        if truncated {
            let mut cut = READ_CAP;
            while !content.is_char_boundary(cut) {
                cut -= 1;
            }
            content.truncate(cut);
            content.push_str("\n[… truncated; read a line range …]");
        }
        json!({ "ok": true, "path": a.path, "start_line": start, "end_line": end, "total_lines": lines.len(), "truncated": truncated, "content": content })
    }

    fn walk(&self, dir: Option<&str>, glob: Option<&str>) -> Result<Vec<PathBuf>, Value> {
        let base = match dir {
            Some(d) => resolve_existing(&self.root, d).map_err(|e| failure(e.0))?,
            None => self.root.clone(),
        };
        let matcher = glob
            .map(|g| globset::Glob::new(g).map(|g| g.compile_matcher()))
            .transpose()
            .map_err(|e| failure(format!("invalid glob: {e}")))?;
        let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(&base)
            .hidden(false)
            .filter_entry(|e| e.file_name() != ".git")
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(ignore::DirEntry::into_path)
            .filter(|p| {
                let rel = self.rel(p);
                !self.protected().is_secret(&rel)
                    && matcher.as_ref().is_none_or(|m| m.is_match(&rel))
            })
            .collect();
        files.sort();
        Ok(files)
    }

    fn list_files(&self, a: &ListFiles) -> Value {
        match self.walk(a.path.as_deref(), a.glob.as_deref()) {
            Ok(files) => {
                let total = files.len();
                let shown: Vec<String> = files.iter().take(LIST_CAP).map(|p| self.rel(p)).collect();
                json!({ "ok": true, "files": shown, "total": total, "truncated": total > LIST_CAP })
            }
            Err(e) => e,
        }
    }

    fn search_code(&self, a: &SearchCode) -> Value {
        let re = match regex::Regex::new(&a.pattern) {
            Ok(r) => r,
            Err(e) => return failure(format!("invalid pattern: {e}")),
        };
        let files = match self.walk(a.path.as_deref(), a.glob.as_deref()) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let mut matches = Vec::new();
        let mut total = 0usize;
        for f in files {
            let Ok(bytes) = fs::read(&f) else { continue };
            if bytes.len() > 1024 * 1024 || bytes.iter().take(8192).any(|b| *b == 0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    total += 1;
                    if matches.len() < SEARCH_CAP {
                        let shown: String = line.chars().take(LINE_CAP).collect();
                        matches.push(format!("{}:{}: {shown}", self.rel(&f), i + 1));
                    }
                }
            }
        }
        json!({ "ok": true, "matches": matches, "total": total, "truncated": total > SEARCH_CAP })
    }

    fn write_checked(&self, rel: &str) -> Result<PathBuf, Value> {
        let path = resolve_for_write(&self.root, rel).map_err(|e| failure(e.0))?;
        if let Some(class) = self.policy.write_protected(&self.rel(&path)) {
            return Err(failure(format!(
                "denied: `{rel}` is protected: {}",
                self.policy.protected().reason(class)
            )));
        }
        Ok(path)
    }

    fn write_file(&self, a: &WriteFile) -> Value {
        let path = match self.write_checked(&a.path) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if path.is_dir() {
            return failure(format!("`{}` is a directory", a.path));
        }
        let before = fs::read(&path)
            .ok()
            .map_or_else(|| "absent".to_owned(), |b| sha(&b));
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return failure(format!("`{}`: {e}", a.path));
        }
        if let Err(e) = fs::write(&path, &a.content) {
            return failure(format!("`{}`: {e}", a.path));
        }
        json!({ "ok": true, "path": a.path, "before": before, "after": sha(a.content.as_bytes()), "bytes": a.content.len() })
    }

    fn edit_file(&self, a: &EditFile) -> Value {
        let path = match self.write_checked(&a.path) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return failure(format!("`{}` does not exist or is not text", a.path));
        };
        if a.old_string.is_empty() {
            return failure("`old_string` is empty");
        }
        let count = text.matches(&a.old_string).count();
        let replace_all = a.replace_all.unwrap_or(false);
        let new = match (count, replace_all) {
            (0, _) => return failure(format!("`old_string` not found in `{}`", a.path)),
            (1, _) | (_, true) => text.replace(&a.old_string, &a.new_string),
            (n, false) => {
                return failure(format!(
                    "`old_string` occurs {n} times in `{}`; add context to make it unique or set replace_all",
                    a.path
                ));
            }
        };
        if let Err(e) = fs::write(&path, &new) {
            return failure(format!("`{}`: {e}", a.path));
        }
        json!({ "ok": true, "path": a.path, "replacements": if replace_all { count } else { 1 }, "before": sha(text.as_bytes()), "after": sha(new.as_bytes()) })
    }

    async fn run_command(&self, a: RunCommand) -> Value {
        // `.`, `./` and an empty string mean the workspace root (models pass them).
        let cwd = match a
            .cwd
            .as_deref()
            .filter(|d| !matches!(d.trim(), "" | "." | "./"))
        {
            None => self.root.clone(),
            Some(d) => match resolve_existing(&self.root, d) {
                Ok(p) if p.is_dir() => p,
                Ok(_) => return failure(format!("`{d}` is not a directory")),
                Err(e) => return failure(e.0),
            },
        };
        let secs = a.timeout_secs.unwrap_or(120).clamp(1, MAX_COMMAND_SECS);
        let mut env = scrubbed_env(std::env::vars());
        for (k, v) in self.policy.protected().rules().env {
            env.retain(|(name, _)| name != k);
            env.push(((*k).to_owned(), (*v).to_owned()));
        }
        if let Some(id) = &a.call_id {
            env.push(("SPECCODER_CALL_ID".to_owned(), id.clone()));
        }
        let inv = Invocation {
            argv: a.argv,
            cwd,
            env,
            timeout: Duration::from_secs(secs),
            output_cap: OUTPUT_CAP,
        };
        // Tests run model-written code: what a command changes among the
        // judge's inputs is found now, not at the end of the attempt (D-065).
        let judged_before = crate::integrity::JudgeSnapshot::take(&self.root, &self.policy);
        let runner = self.runner.clone();
        let network = a.network == Some(true);
        let outcome =
            match tokio::task::spawn_blocking(move || runner.run_with(&inv, network)).await {
                Ok(o) => o,
                Err(e) => return failure(format!("the command runner failed: {e}")),
            };
        if let Some(e) = outcome.spawn_error {
            return failure(e);
        }
        let tail = |b: &[u8]| {
            let start = b.len().saturating_sub(16 * 1024);
            String::from_utf8_lossy(&b[start..]).into_owned()
        };
        let drift = judged_before.drift(
            &crate::integrity::JudgeSnapshot::take(&self.root, &self.policy),
            &self.policy,
            &std::collections::BTreeSet::new(),
        );
        let mut result = json!({
            "ok": outcome.exit_code == Some(0),
            "exit_code": outcome.exit_code,
            "timed_out": outcome.timed_out,
            "duration_ms": u64::try_from(outcome.duration.as_millis()).unwrap_or(u64::MAX),
            "stdout": tail(&outcome.stdout.kept),
            "stderr": tail(&outcome.stderr.kept),
            "truncated": outcome.stdout.truncated() || outcome.stderr.truncated(),
        });
        if !drift.is_empty() {
            result["integrity"] = json!(drift);
        }
        result
    }

    fn git_status(&self) -> Value {
        match Git::at(&self.root).run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        {
            Ok(out) => {
                let entries: Vec<String> = nul_fields(&out);
                json!({ "ok": true, "entries": entries })
            }
            Err(e) => failure(e.0),
        }
    }

    fn git_diff(&self, a: &GitDiff) -> Value {
        let mut args = vec!["diff", "--no-color", "HEAD"];
        let path;
        if let Some(p) = &a.path {
            path = match paths::normalise(p) {
                Ok(p) => p,
                Err(e) => return failure(e.0),
            };
            args.extend(["--", path.as_str()]);
        }
        match Git::at(&self.root).run(&args) {
            Ok(out) => {
                let mut diff = String::from_utf8_lossy(&out).into_owned();
                let truncated = diff.len() > READ_CAP;
                if truncated {
                    let mut cut = READ_CAP;
                    while !diff.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    diff.truncate(cut);
                }
                json!({ "ok": true, "diff": diff, "truncated": truncated })
            }
            Err(e) => failure(e.0),
        }
    }
}

#[async_trait]
impl ToolClient for WorkspaceTools {
    async fn call(
        &self,
        tool: &ToolId,
        arguments: &Value,
        _provenance: Option<&agentplane::core::Provenance>,
    ) -> Result<Value, ToolError> {
        let known = catalog()
            .into_iter()
            .any(|t| t.server == tool.server && t.name == tool.tool);
        if !known {
            return Err(ToolError::Refused {
                tool: tool.clone(),
                detail: "not a speccoder tool".to_owned(),
            });
        }
        Ok(self.run(&tool.tool, arguments).await)
    }

    /// The tools touch only the local worktree. (A command the model runs
    /// may still use the network; that is the policy's and, in M6, the
    /// sandbox's concern, not a transport this client opens.)
    fn destination(&self, _tool: &ToolId) -> Destination {
        Destination::Local
    }
}
