//! `speccoder.toml` as settings: a catalogue of every key — its
//! type, default and meaning — the values a project sets, and edits that
//! keep the file's comments and are checked by every parser before they are
//! written. speccoder reads committed configuration only, so a change is
//! offered as a commit of exactly this file.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ConfigChange, ConfigKey, ConfigReport,
    ConfigSection, ConfigTable,
};

use crate::git::Git;
use crate::verify::config::FILE;

/// What a key holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// `true` or `false`.
    Bool,
    /// A whole number in a range.
    Int(i64, i64),
    /// A number in a range.
    Num(f64, f64),
    /// Text.
    Str,
    /// One of these.
    Choice(&'static [&'static str]),
    /// `30m`, `12h`, `7d`.
    Duration,
    /// A program and its arguments.
    Argv,
    /// A list of programs with arguments.
    ArgvList,
    /// A list of strings.
    StrList,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int(..) => "integer",
            Self::Num(..) => "number",
            Self::Str => "string",
            Self::Choice(_) => "choice",
            Self::Duration => "duration",
            Self::Argv => "argv",
            Self::ArgvList => "argv_list",
            Self::StrList => "string_list",
        }
    }
}

/// One key of the catalogue.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// The key, dotted below a sub-table (`sigstore.identity`).
    pub key: &'static str,
    /// What it holds.
    pub kind: Kind,
    /// Its default as JSON, when it has one.
    pub default: Option<&'static str>,
    /// The section is refused without it.
    pub required: bool,
    /// What it does.
    pub about: &'static str,
}

/// One section of the catalogue.
#[derive(Debug, Clone, Copy)]
pub struct Section {
    /// The table name.
    pub name: &'static str,
    /// For people.
    pub title: &'static str,
    /// What it decides.
    pub about: &'static str,
    /// `[name.<table>]` sections.
    pub named: bool,
    /// Its keys.
    pub keys: &'static [Key],
}

const fn k(
    key: &'static str,
    kind: Kind,
    default: Option<&'static str>,
    about: &'static str,
) -> Key {
    Key {
        key,
        kind,
        default,
        required: false,
        about,
    }
}

const fn req(key: &'static str, kind: Kind, about: &'static str) -> Key {
    Key {
        key,
        kind,
        default: None,
        required: true,
        about,
    }
}

/// The spec formats, by the names `[spec] format` takes — from the format
/// model, not spelled here.
const FORMATS: &[&str] = &[
    speccoder_core::spec::FormatKind::ALL[0].name(),
    speccoder_core::spec::FormatKind::ALL[1].name(),
    speccoder_core::spec::FormatKind::ALL[2].name(),
];

const PRICE: Kind = Kind::Num(0.0, 100_000.0);

/// Every key of `speccoder.toml`, in the reference's order. A test holds it
/// to the parsers and to the configuration reference.
pub const CATALOGUE: &[Section] = &[
    Section {
        name: "model",
        title: "Model",
        about: "The model that makes the attempts. The key comes from ANTHROPIC_API_KEY or OPENAI_API_KEY, Bedrock's credentials from AWS — never from this file.",
        named: false,
        keys: &[
            req(
                "provider",
                Kind::Choice(&["anthropic", "openai", "openai-compatible", "bedrock"]),
                "Who serves the model; openai-compatible covers Ollama, vLLM, llama.cpp, LM Studio and others; bedrock is Amazon Bedrock, credentials from AWS",
            ),
            req(
                "model",
                Kind::Str,
                "The model, for example claude-sonnet-5-5",
            ),
            k(
                "region",
                Kind::Str,
                None,
                "Bedrock only: the AWS region calls go to (the journal records it)",
            ),
            k(
                "reasoning_summary",
                Kind::Bool,
                Some("true"),
                "Show the model's summary of its reasoning (a claim, not evidence)",
            ),
            k(
                "max_output_tokens",
                Kind::Int(1, 1_000_000),
                Some("4096"),
                "Output tokens per model turn",
            ),
            k(
                "max_run_tokens",
                Kind::Int(1, 100_000_000),
                Some("200000"),
                "Tokens per attempt",
            ),
            k(
                "cache_ttl",
                Kind::Choice(&["5m", "1h"]),
                Some("\"5m\""),
                "Anthropic's prompt-cache lifetime",
            ),
            k(
                "base_url",
                Kind::Str,
                None,
                "The server, required for openai-compatible (e.g. http://localhost:11434/v1)",
            ),
            k(
                "api_key_env",
                Kind::Str,
                None,
                "openai-compatible only: the variable holding the server's key",
            ),
            k(
                "structured",
                Kind::Choice(&["native", "tool"]),
                Some("\"native\""),
                "openai-compatible only: native structured output, or a forced tool call",
            ),
            k(
                "price_input_per_mtok",
                PRICE,
                None,
                "USD per million input tokens, for cost reports",
            ),
            k(
                "price_output_per_mtok",
                PRICE,
                None,
                "USD per million output tokens",
            ),
            k(
                "price_cache_read_per_mtok",
                PRICE,
                None,
                "USD per million tokens read from the cache",
            ),
            k(
                "price_cache_write_per_mtok",
                PRICE,
                None,
                "USD per million tokens written to the five-minute cache",
            ),
            k(
                "price_cache_write_1h_per_mtok",
                PRICE,
                None,
                "USD per million tokens written to the one-hour cache",
            ),
        ],
    },
    Section {
        name: "checks",
        title: "Checks",
        about: "The commands that decide whether nothing regressed. Each check's command up to its first option is also something the model may run.",
        named: true,
        keys: &[
            req(
                "argv",
                Kind::Argv,
                "Program and arguments, run without a shell",
            ),
            k(
                "cwd",
                Kind::Str,
                Some("\".\""),
                "Working directory, relative to the project root",
            ),
            k(
                "when_exists",
                Kind::Str,
                None,
                "Run only if this path exists; otherwise skipped, never passed",
            ),
            k(
                "tier",
                Kind::Choice(&["fast", "full"]),
                Some("\"fast\""),
                "fast judges every task; full runs once after the feature's last task",
            ),
            k(
                "timeout_secs",
                Kind::Int(1, 86_400),
                Some("600"),
                "Deadline for the whole process group",
            ),
            k(
                "junit",
                Kind::Str,
                None,
                "A JUnit XML report the check writes, for per-test results",
            ),
            k(
                "list",
                Kind::Argv,
                None,
                "A command that lists the tests the check would run (one test id or file path per line); speccoder runs it when it cannot otherwise tell",
            ),
        ],
    },
    Section {
        name: "budget",
        title: "Budget",
        about: "How much an attempt may do, and how often a task is tried.",
        named: false,
        keys: &[
            k(
                "max_turns",
                Kind::Int(1, 1000),
                Some("30"),
                "Model turns per attempt",
            ),
            k(
                "max_tool_calls",
                Kind::Int(1, 10_000),
                Some("100"),
                "Tool calls per attempt",
            ),
            k(
                "max_attempts",
                Kind::Int(1, 20),
                Some("2"),
                "Attempts per task run; a failed verdict is retried with the verifier's facts",
            ),
            k(
                "best_of",
                Kind::Int(1, 10),
                Some("1"),
                "Independent samples per task, the best by evidence kept",
            ),
        ],
    },
    Section {
        name: "commands",
        title: "Commands",
        about: "What the model may run without asking, besides the checks. `allow` is exact; a prefix in `allow_prefix` runs whatever follows it, so it is saved only from a preview of what it would newly allow.",
        named: false,
        keys: &[
            k(
                "allow",
                Kind::ArgvList,
                Some("[]"),
                "Exact commands that run without asking, e.g. [\"cargo\", \"check\"]; nothing after them",
            ),
            k(
                "allow_prefix",
                Kind::ArgvList,
                Some("[]"),
                "Command prefixes (two elements or more) whose every extension runs without asking; saved only from the coverage preview",
            ),
        ],
    },
    Section {
        name: "approvals",
        title: "Approvals",
        about: "How long a call waits for a person, and how often a run may ask.",
        named: false,
        keys: &[
            k(
                "wait",
                Kind::Duration,
                Some("\"7d\""),
                "How long a call waits before it is denied as expired (m, h or d)",
            ),
            k(
                "budget",
                Kind::Int(0, 100),
                Some("12"),
                "Prompts a run shows you; further calls wait together in Needs You",
            ),
        ],
    },
    Section {
        name: "tests",
        title: "Tests",
        about: "What the model may do with tests that existed before an attempt.",
        named: false,
        keys: &[k(
            "frozen",
            Kind::Choice(&["ask", "additions"]),
            Some("\"ask\""),
            "ask before any change, or allow new lines without asking",
        )],
    },
    Section {
        name: "spec",
        title: "Spec format",
        about: "Which spec format the project uses; detected when unset.",
        named: false,
        keys: &[k(
            "format",
            Kind::Choice(FORMATS),
            None,
            "The spec format; --format and SPECCODER_FORMAT override it",
        )],
    },
    Section {
        name: "context",
        title: "Instructions",
        about: "The repository's instructions for agents, given to every attempt.",
        named: false,
        keys: &[k(
            "instructions",
            Kind::StrList,
            Some("[\"AGENTS.md\"]"),
            "File names read per directory, the first that exists; none to read none",
        )],
    },
    Section {
        name: "acceptance",
        title: "Acceptance",
        about: "Tests derived from the spec, and how strong they are.",
        named: false,
        keys: &[
            k(
                "generate",
                Kind::Bool,
                Some("true"),
                "Derive acceptance tests from the spec before each attempt",
            ),
            k(
                "held_out",
                Kind::Bool,
                Some("true"),
                "Also write held-out tests the implementer never sees; verified needs them to pass",
            ),
            k(
                "strength",
                Kind::Bool,
                Some("true"),
                "Measure the acceptance strength of every verified task",
            ),
            k(
                "min_strength",
                Kind::Num(0.0, 1.0),
                None,
                "Below it the set is strengthened once, then the task stops for you",
            ),
            k(
                "near_miss",
                Kind::Int(0, 3),
                Some("2"),
                "Wrong variants to try",
            ),
            k(
                "permitted",
                Kind::Int(0, 2),
                Some("1"),
                "Spec-permitted alternatives to try; 0 turns the over-strict check off",
            ),
            k(
                "strength_timeout_secs",
                Kind::Int(1, 86_400),
                Some("900"),
                "Budget for the whole strength measurement",
            ),
        ],
    },
    Section {
        name: "bug",
        title: "Bug fixes",
        about: "`speccoder bug`: how hard reproduce tries before the bug goes to you as cannot reproduce. A project setting only: a value in your user config is ignored.",
        named: false,
        keys: &[
            k(
                "reproduce_tries",
                Kind::Int(1, 5),
                Some("3"),
                "Tries of the reproduction author before cannot reproduce (a rejection counts as one)",
            ),
            k(
                "test",
                Kind::Argv,
                None,
                "How one named test (`--test PATH::NAME`) runs, `{file}` and `{name}` filled in; default: the toolchain's own runner",
            ),
        ],
    },
    Section {
        name: "converge",
        title: "Convergence",
        about: "How often a feature may be brought back to its spec.",
        named: false,
        keys: &[k(
            "max_rounds",
            Kind::Int(1, 20),
            Some("3"),
            "Convergence phases a feature may get",
        )],
    },
    Section {
        name: "review",
        title: "Spec review",
        about: "Whether a run waits for every requirement to be approved, and how tests are written at review.",
        named: false,
        keys: &[
            k(
                "required",
                Kind::Bool,
                Some("false"),
                "A run waits until every requirement is approved on its current wording",
            ),
            k(
                "oracle_samples",
                Kind::Int(1, 2),
                Some("2"),
                "Independent author calls Write Tests makes; two are compared for ambiguity",
            ),
        ],
    },
    Section {
        name: "monitor",
        title: "Gaming monitor",
        about: "A second model that reviews every passing attempt for gaming.",
        named: false,
        keys: &[k(
            "enabled",
            Kind::Bool,
            Some("false"),
            "Review every passing attempt",
        )],
    },
    Section {
        name: "git",
        title: "Git",
        about: "How passed tasks are recorded.",
        named: false,
        keys: &[k(
            "commit",
            Kind::Choice(&["per-task", "none"]),
            Some("\"per-task\""),
            "Commit each passed task, or leave changes uncommitted",
        )],
    },
    Section {
        name: "format",
        title: "Formatter",
        about: "A formatter run after each attempt; its changes are part of the attempt and audited.",
        named: false,
        keys: &[k(
            "argv",
            Kind::Argv,
            None,
            "The formatter, e.g. [\"cargo\", \"fmt\"]",
        )],
    },
    Section {
        name: "sandbox",
        title: "Sandbox",
        about: "Where commands the model runs may write and connect.",
        named: false,
        keys: &[
            k(
                "mode",
                Kind::Choice(&["auto", "required", "off"]),
                Some("\"auto\""),
                "auto sandboxes where the OS can; required refuses to run unsandboxed",
            ),
            k(
                "network",
                Kind::Choice(&["off", "loopback", "on"]),
                Some("\"loopback\""),
                "Network for commands; loopback lets local test servers work",
            ),
            k(
                "network_allow",
                Kind::ArgvList,
                Some("[]"),
                "Exact commands that get the network, e.g. [\"cargo\", \"fetch\"]",
            ),
            k(
                "network_allow_prefix",
                Kind::ArgvList,
                Some("[]"),
                "Prefixes (two elements or more) whose extensions get the network; saved only from the coverage preview",
            ),
            k(
                "writable",
                Kind::StrList,
                Some("[]"),
                "Extra writable paths (~ is your home), e.g. ~/.m2",
            ),
        ],
    },
    Section {
        name: "worktree",
        title: "Worktree",
        about: "What a run's worktree gets that git never holds: dependency folders (node_modules, .venv) and git-ignored files such as .env.",
        named: false,
        keys: &[
            k(
                "prepare",
                Kind::Choice(&["auto", "off"]),
                Some("\"auto\""),
                "auto installs Node and uv dependencies (locked, offline) before the baseline; off never",
            ),
            k(
                "include",
                Kind::StrList,
                Some("[]"),
                "Git-ignored files copied from your checkout, .gitignore patterns, e.g. .env (with .worktreeinclude's); never committed, secret-named values masked",
            ),
        ],
    },
    Section {
        name: "attest",
        title: "Attestations",
        about: "Signed statements of every tick, as git notes.",
        named: false,
        keys: &[
            k(
                "enabled",
                Kind::Bool,
                Some("true"),
                "Write the statements on each tick commit",
            ),
            k(
                "signer",
                Kind::Choice(&["git", "sigstore", "none"]),
                Some("\"git\""),
                "Your git signing key, keyless sigstore, or unsigned",
            ),
            k(
                "sign_at",
                Kind::Choice(&["tick", "later"]),
                Some("\"tick\""),
                "Sign at each tick, or later with speccoder attest sign",
            ),
            k(
                "sigstore.identity",
                Kind::Str,
                None,
                "The identity you sign with, suggested to reviewers",
            ),
            k("sigstore.issuer", Kind::Str, None, "Its OIDC issuer"),
        ],
    },
    Section {
        name: "mutation",
        title: "Mutation testing",
        about: "Mutants run against the suite after a passing verdict.",
        named: false,
        keys: &[
            req(
                "tool",
                Kind::Choice(&["cargo-mutants", "command"]),
                "cargo-mutants, or command for any tool writing the mutation-testing report",
            ),
            k(
                "argv",
                Kind::Argv,
                None,
                "For command: the program; may use {diff}, {out}, {files}, {files_csv}",
            ),
            k("report", Kind::Str, None, "For command: the report file"),
            k(
                "mode",
                Kind::Choice(&["report", "require"]),
                Some("\"report\""),
                "report records; require fails the verdict when mutants survive",
            ),
            k(
                "timeout_secs",
                Kind::Int(1, 86_400),
                Some("900"),
                "Limit for the whole mutation run",
            ),
            k(
                "suite",
                Kind::Bool,
                Some("true"),
                "Run mutants against the whole suite; false uses them for strength only",
            ),
            k(
                "acceptance_argv",
                Kind::Argv,
                None,
                "For command: runs the mutants against the acceptance tests alone",
            ),
        ],
    },
    Section {
        name: "executor",
        title: "Executor",
        about: "Who makes the attempts: speccoder's own loop or an external agent.",
        named: false,
        keys: &[
            Key {
                key: "kind",
                kind: Kind::Choice(&["native", "claude", "codex", "command"]),
                default: None,
                // Without [executor] speccoder's own loop runs; with it, the
                // section must say which.
                required: true,
                about: "native, Claude Code, Codex, or any command",
            },
            k("model", Kind::Str, None, "The agent's model"),
            k(
                "timeout_secs",
                Kind::Int(1, 86_400),
                Some("1800"),
                "How long one attempt may take",
            ),
            k(
                "argv",
                Kind::Argv,
                None,
                "For command: the program, which reads the task on stdin",
            ),
        ],
    },
    Section {
        name: "mcp",
        title: "MCP servers",
        about: "Stdio MCP servers started in the worktree for each task; their tools appear as mcp__<server>__<tool>.",
        named: true,
        keys: &[
            req("command", Kind::Argv, "The server's program and arguments"),
            k(
                "allow",
                Kind::StrList,
                Some("[]"),
                "Tools called without approval",
            ),
            k("deny", Kind::StrList, Some("[]"), "Tools never offered"),
        ],
    },
];

/// Whether every parser accepts `text` — the same ones every command uses.
///
/// # Errors
///
/// The first parser's complaint.
pub fn validate(text: &str) -> Result<(), String> {
    crate::substrate::model_config::parse(text)?;
    validate_but_model(text)
}

/// [`validate`] without `[model]`.
fn validate_but_model(text: &str) -> Result<(), String> {
    crate::verify::config::parse(text)?;
    crate::substrate::run_config::parse(text)?;
    crate::sandbox::parse(text)?;
    crate::prepare::parse(text)?;
    crate::mcp::parse(text)?;
    crate::mutation::parse(text)?;
    crate::external::parse(text)?;
    crate::format::parse_config(text)?;
    Ok(())
}

/// The directory holding the configuration: the nearest with one, else the
/// repository's top level, else `root`.
fn home(root: &Path) -> PathBuf {
    // Where the file is, else where `init` would write it (the spec
    // project's root, which in a nested project is not the repository's).
    crate::verify::find_config(root).unwrap_or_else(|| crate::init::root_of(root))
}

fn lookup<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (last, parents) = path.split_last()?;
    let mut t = table;
    for p in parents {
        t = t.get(*p)?.as_table()?;
    }
    t.get(*last)
}

fn key_view(k: &Key, value: Option<Value>) -> ConfigKey {
    let (min, max) = match k.kind {
        #[allow(clippy::cast_precision_loss)]
        Kind::Int(a, b) => (Some(a as f64), Some(b as f64)),
        Kind::Num(a, b) => (Some(a), Some(b)),
        _ => (None, None),
    };
    ConfigKey {
        key: k.key.to_owned(),
        kind: k.kind.name().to_owned(),
        about: k.about.to_owned(),
        default: k.default.and_then(|d| serde_json::from_str(d).ok()),
        value,
        choices: match k.kind {
            Kind::Choice(c) => c.iter().map(|s| (*s).to_owned()).collect(),
            _ => Vec::new(),
        },
        min,
        max,
        required: k.required,
    }
}

fn json_of(v: &toml::Value) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The configuration as settings: every key, set or default.
#[must_use]
pub fn read(root: &Path) -> ConfigReport {
    let dir = home(root);
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).ok();
    let table: toml::Table = text
        .as_deref()
        .and_then(|t| toml::from_str(t).ok())
        .unwrap_or_default();
    let sections = CATALOGUE
        .iter()
        .map(|s| {
            if s.named {
                let tables = table
                    .get(s.name)
                    .and_then(toml::Value::as_table)
                    .map(|named| {
                        named
                            .iter()
                            .map(|(name, t)| ConfigTable {
                                name: name.clone(),
                                keys: s
                                    .keys
                                    .iter()
                                    .map(|k| {
                                        let v =
                                            t.as_table().and_then(|t| t.get(k.key)).map(json_of);
                                        key_view(k, v)
                                    })
                                    .collect(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ConfigSection {
                    name: s.name.to_owned(),
                    title: s.title.to_owned(),
                    about: s.about.to_owned(),
                    named: true,
                    keys: s.keys.iter().map(|k| key_view(k, None)).collect(),
                    tables,
                }
            } else {
                ConfigSection {
                    name: s.name.to_owned(),
                    title: s.title.to_owned(),
                    about: s.about.to_owned(),
                    named: false,
                    keys: s
                        .keys
                        .iter()
                        .map(|k| {
                            let mut path = vec![s.name];
                            path.extend(k.key.split('.'));
                            let mut view = key_view(k, lookup(&table, &path).map(json_of));
                            // Bedrock's region is required with that provider.
                            view.required |= s.name == "model"
                                && k.key == "region"
                                && lookup(&table, &["model", "provider"])
                                    .and_then(toml::Value::as_str)
                                    == Some("bedrock");
                            view
                        })
                        .collect(),
                    tables: Vec::new(),
                }
            }
        })
        .collect();
    let error = text.as_deref().and_then(|t| validate(t).err());
    let (committed, diff) = state(&dir, text.is_some());
    ConfigReport {
        file: FILE.to_owned(),
        exists: text.is_some(),
        committed,
        valid: text.is_some() && error.is_none(),
        error,
        sections,
        diff,
    }
}

/// Whether the file on disk is the committed one, and the change if not.
fn state(dir: &Path, exists: bool) -> (bool, Option<String>) {
    let g = Git::at(dir);
    let tracked = g
        .succeeds(&["ls-files", "--error-unmatch", "--", FILE])
        .unwrap_or(false);
    if !tracked {
        let diff = exists.then(|| {
            std::fs::read_to_string(dir.join(FILE))
                .unwrap_or_default()
                .lines()
                .map(|l| format!("+{l}"))
                .collect::<Vec<_>>()
                .join("\n")
        });
        return (
            false,
            diff.map(|d| {
                format!("diff --git a/{FILE} b/{FILE}\nnew file mode 100644\n@@ -0,0 +1 @@\n{d}\n")
            }),
        );
    }
    let clean = g
        .succeeds(&["diff", "--quiet", "HEAD", "--", FILE])
        .unwrap_or(false);
    if clean {
        return (true, None);
    }
    let diff = g
        .run(&["diff", "--no-color", "HEAD", "--", FILE])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok();
    (false, diff)
}

/// Where a dotted key lives: the table path and the leaf, with its catalogue
/// entry.
struct Place {
    tables: Vec<String>,
    leaf: String,
    key: &'static Key,
}

fn place(dotted: &str) -> Result<Place, String> {
    let parts: Vec<&str> = dotted.split('.').collect();
    let Some(section) = parts
        .first()
        .and_then(|s| CATALOGUE.iter().find(|c| c.name == *s))
    else {
        if parts.first() == Some(&"ui") {
            return Err("[ui] is a person's setting, not the project's: it belongs in ~/.config/speccoder/config.toml".to_owned());
        }
        return Err(format!(
            "unknown section in `{dotted}` (known: {})",
            CATALOGUE
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let (tables, rest): (Vec<String>, &[&str]) = if section.named {
        let Some(name) = parts.get(1) else {
            return Err(format!(
                "`{dotted}`: name the table, e.g. {}.<name>.<key>",
                section.name
            ));
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(format!(
                "`{name}`: a table name is lowercase letters, digits, - and _"
            ));
        }
        (
            vec![section.name.to_owned(), (*name).to_owned()],
            &parts[2..],
        )
    } else {
        (vec![section.name.to_owned()], &parts[1..])
    };
    let wanted = rest.join(".");
    let key = section
        .keys
        .iter()
        .find(|k| k.key == wanted)
        .ok_or_else(|| {
            format!(
                "unknown key `{dotted}` (in [{}]: {})",
                section.name,
                section
                    .keys
                    .iter()
                    .map(|k| k.key)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut tables = tables;
    let mut leaf = wanted.clone();
    if let Some((sub, last)) = wanted.rsplit_once('.') {
        tables.extend(sub.split('.').map(str::to_owned));
        leaf = last.to_owned();
    }
    Ok(Place { tables, leaf, key })
}

/// Checks `value` against `kind` before any parser sees it, for a message
/// that names the key.
fn check(dotted: &str, kind: Kind, value: &Value) -> Result<(), String> {
    let strings = |v: &Value| v.as_array().is_some_and(|a| a.iter().all(Value::is_string));
    let ok = match kind {
        Kind::Bool => value.is_boolean(),
        Kind::Int(a, b) => value.as_i64().is_some_and(|n| (a..=b).contains(&n)),
        Kind::Num(a, b) => value.as_f64().is_some_and(|n| n >= a && n <= b),
        Kind::Str => value.as_str().is_some_and(|s| !s.is_empty()),
        Kind::Choice(c) => value.as_str().is_some_and(|s| c.contains(&s)),
        Kind::Duration => value.as_str().is_some_and(|s| {
            // The unit is the last character, which may be more than a byte.
            let (n, u) = s.split_at(s.char_indices().last().map_or(0, |(i, _)| i));
            n.parse::<u64>().is_ok_and(|n| n > 0) && ["m", "h", "d"].contains(&u)
        }),
        Kind::Argv => strings(value) && value.as_array().is_some_and(|a| !a.is_empty()),
        Kind::ArgvList => value.as_array().is_some_and(|a| {
            a.iter()
                .all(|x| strings(x) && x.as_array().is_some_and(|i| !i.is_empty()))
        }),
        Kind::StrList => strings(value),
    };
    if ok {
        return Ok(());
    }
    let want = match kind {
        Kind::Bool => "true or false".to_owned(),
        Kind::Int(a, b) => format!("a whole number from {a} to {b}"),
        Kind::Num(a, b) => format!("a number from {a} to {b}"),
        Kind::Str => "text".to_owned(),
        Kind::Choice(c) => format!("one of {}", c.join(", ")),
        Kind::Duration => "a duration like 30m, 12h or 7d".to_owned(),
        Kind::Argv => "a program and its arguments, e.g. [\"cargo\", \"fmt\"]".to_owned(),
        Kind::ArgvList => {
            "a list of programs with arguments, e.g. [[\"cargo\", \"check\"]]".to_owned()
        }
        Kind::StrList => "a list of strings".to_owned(),
    };
    Err(format!("`{dotted}` = {value}: expected {want}"))
}

fn toml_value(v: &Value) -> Option<toml_edit::Value> {
    Some(match v {
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml_edit::Value::from(i),
            None => toml_edit::Value::from(n.as_f64()?),
        },
        Value::String(s) => toml_edit::Value::from(s.as_str()),
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push(toml_value(x)?);
            }
            toml_edit::Value::Array(arr)
        }
        Value::Null | Value::Object(_) => return None,
    })
}

/// The table at `path` — a `[table]` or an inline `{ … }` — made when
/// missing and `create`; `None` when missing and not. The containers of
/// named tables (`[checks]`, `[mcp]`) are implicit: no header of their own.
fn table_mut<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    path: &[String],
    create: bool,
) -> Result<Option<&'a mut dyn toml_edit::TableLike>, String> {
    let mut t: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    let mut inline = false;
    for (i, p) in path.iter().enumerate() {
        if t.get(p).is_none() {
            if !create {
                return Ok(None);
            }
            let fresh = if inline {
                // Inside an inline table, what is made is inline too.
                toml_edit::Item::Value(toml_edit::Value::InlineTable(toml_edit::InlineTable::new()))
            } else {
                let mut table = toml_edit::Table::new();
                let container = i == 0 && CATALOGUE.iter().any(|c| c.named && c.name == p);
                table.set_implicit(container || i + 1 < path.len());
                toml_edit::Item::Table(table)
            };
            t.insert(p, fresh);
        }
        let Some(item) = t.get_mut(p) else {
            return Err(format!("`{}` could not be made", path[..=i].join(".")));
        };
        inline = inline || item.is_inline_table();
        t = item
            .as_table_like_mut()
            .ok_or_else(|| format!("`{}` is not a table", path[..=i].join(".")))?;
    }
    Ok(Some(t))
}

/// Removes empty tables (or inline tables) left behind by an unset,
/// innermost first.
fn prune(doc: &mut toml_edit::DocumentMut, path: &[String]) {
    for depth in (1..=path.len()).rev() {
        let (parent, last) = (&path[..depth - 1], &path[depth - 1]);
        let Ok(Some(t)) = table_mut(doc, parent, false) else {
            return;
        };
        let empty = t
            .get(last)
            .and_then(toml_edit::Item::as_table_like)
            .is_some_and(toml_edit::TableLike::is_empty);
        if empty {
            t.remove(last);
        }
    }
}

/// The new text with `changes` applied, or why not.
///
/// # Errors
///
/// An unknown key, a value of the wrong kind, or a result a parser refuses.
pub fn edit(text: &str, changes: &[ConfigChange]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    for c in changes {
        let at = place(&c.key)?;
        match (&c.value, &c.append) {
            (Some(v), None) => {
                check(&c.key, at.key.kind, v)?;
                let value = toml_value(v)
                    .ok_or_else(|| format!("`{}`: a value TOML cannot hold", c.key))?;
                let t = table_mut(&mut doc, &at.tables, true)?
                    .ok_or_else(|| format!("`{}` could not be made", c.key))?;
                match t.get_mut(&at.leaf).and_then(toml_edit::Item::as_value_mut) {
                    // Keep the key's own comment and spacing.
                    Some(old) => {
                        let decor = old.decor().clone();
                        *old = value;
                        *old.decor_mut() = decor;
                    }
                    None => {
                        t.insert(&at.leaf, toml_edit::value(value));
                    }
                }
            }
            (None, Some(item)) => {
                if !matches!(at.key.kind, Kind::ArgvList | Kind::StrList) {
                    return Err(format!("`{}` is not a list", c.key));
                }
                // The list as it is now, read back through the parser.
                let now: toml::Table = toml::from_str(&doc.to_string()).unwrap_or_default();
                let mut path: Vec<&str> = at.tables.iter().map(String::as_str).collect();
                path.push(&at.leaf);
                let mut list: Vec<Value> = lookup(&now, &path)
                    .map(json_of)
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                if !list.contains(item) {
                    list.push(item.clone());
                }
                let all = Value::Array(list);
                check(&c.key, at.key.kind, &all)?;
                let value = toml_value(&all)
                    .ok_or_else(|| format!("`{}`: a value TOML cannot hold", c.key))?;
                table_mut(&mut doc, &at.tables, true)?
                    .ok_or_else(|| format!("`{}` could not be made", c.key))?
                    .insert(&at.leaf, toml_edit::value(value));
            }
            (None, None) => {
                // Missing is already unset; anything else in the way is an error.
                if let Some(t) = table_mut(&mut doc, &at.tables, false)? {
                    t.remove(&at.leaf);
                }
                prune(&mut doc, &at.tables);
            }
            (Some(_), Some(_)) => {
                return Err(format!(
                    "`{}`: set a value or append to it, not both",
                    c.key
                ));
            }
        }
    }
    let out = doc.to_string();
    match crate::substrate::model_config::parse(&out) {
        // A step on the way to Bedrock: its provider before its region,
        // which `config set model.region` adds next (until then every
        // command, and `config commit`, refuses the file and says so).
        Err(e) if e.starts_with(crate::substrate::model_config::NEEDS_REGION) => {}
        other => {
            other?;
        }
    }
    validate_but_model(&out)?;
    Ok(out)
}

/// What a change to `[model]` leaves to do: Bedrock's region when it is
/// still missing (the step [`edit`] lets through), and a warning when a
/// change made the model a Bedrock one with an id Bedrock does not use.
fn model_notes(
    text: &str,
    changes: &[ConfigChange],
) -> (Vec<String>, Vec<speccoder_core::next::NextStep>) {
    use crate::substrate::model_config;
    let mut notes = Vec::new();
    let mut next = Vec::new();
    if model_config::parse(text).is_err_and(|e| e.starts_with(model_config::NEEDS_REGION)) {
        notes.push("provider bedrock needs model.region, set it next".to_owned());
        next.push(speccoder_core::next::NextStep {
            command: "speccoder config set model.region <region>".to_owned(),
            why: "provider bedrock needs the AWS region its calls go to".to_owned(),
        });
    }
    let touched = changes
        .iter()
        .any(|c| c.key == "model.provider" || c.key == "model.model");
    let model: toml::Table = toml::from_str(text).unwrap_or_default();
    let model = model.get("model").and_then(toml::Value::as_table);
    let field = |k: &str| model.and_then(|m| m.get(k)).and_then(toml::Value::as_str);
    if touched
        && field("provider") == Some("bedrock")
        && let Some(w) = field("model").and_then(model_config::bedrock_model_warning)
    {
        notes.push(format!("warning: {w}"));
    }
    (notes, next)
}

/// `speccoder config`: the settings.
#[must_use]
pub fn show(root: &Path) -> CommandResult {
    let report = read(root);
    let message = match (&report.exists, &report.error, report.committed) {
        (false, _, _) => "no speccoder.toml yet: every key has its default".to_owned(),
        (true, Some(e), _) => format!("speccoder.toml has an error: {e}"),
        (true, None, false) => {
            "speccoder.toml has changes that are not committed (not in force yet)".to_owned()
        }
        (true, None, true) => "speccoder.toml".to_owned(),
    };
    let result = CommandResult::new("config", CommandStatus::Ok, &message);
    let result = if report.committed || !report.exists {
        result
    } else {
        result.with_next(vec![speccoder_core::next::NextStep {
            command: "speccoder config commit".to_owned(),
            why: "speccoder reads committed configuration only".to_owned(),
        }])
    };
    result.with_data(CommandData::Config(Box::new(report)))
}

/// The keys only a coverage preview's confirmation writes.
pub const PREFIX_KEYS: [&str; 2] = ["commands.allow_prefix", "sandbox.network_allow_prefix"];

/// `speccoder config set|unset`, and the page's save: applies `changes` to
/// the file, all or nothing. A prefix key is refused: it is saved only as
/// the confirmation of its coverage preview (`WidenPolicy`); removing an
/// entry (`unset`) narrows and is allowed.
#[must_use]
pub fn apply(root: &Path, changes: &[ConfigChange]) -> CommandResult {
    if let Some(c) = changes.iter().find(|c| {
        PREFIX_KEYS.contains(&c.key.as_str()) && (c.value.is_some() || c.append.is_some())
    }) {
        // Setting the list to fewer entries than it has narrows it.
        let narrows = c.value.as_ref().is_some_and(|v| {
            let now = read(root);
            let current = now
                .sections
                .iter()
                .flat_map(|s| {
                    s.keys
                        .iter()
                        .map(move |k| (format!("{}.{}", s.name, k.key), k))
                })
                .find(|(name, _)| *name == c.key)
                .and_then(|(_, k)| k.value.clone())
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            v.as_array()
                .is_some_and(|new| new.iter().all(|e| current.contains(e)))
        });
        if !narrows {
            return CommandResult::new(
                "config",
                CommandStatus::UsageError,
                &format!(
                    "`{}` is saved only from its coverage preview: `speccoder config coverage {} <argv…>`, then confirm",
                    c.key, c.key
                ),
            )
            .with_code("CONFIG-INVALID");
        }
    }
    apply_confirmed(root, changes)
}

/// [`apply`] without the prefix guard: for the confirmation of a coverage
/// preview and for Always Allow's exact entry, which their actions checked.
#[must_use]
pub(crate) fn apply_confirmed(root: &Path, changes: &[ConfigChange]) -> CommandResult {
    let dir = home(root);
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match edit(&text, changes) {
        Err(e) => CommandResult::new("config", CommandStatus::ConfigError, &e)
            .with_code("CONFIG-INVALID")
            .with_fix(vec![
                crate::substrate::model_config::config_fix(&e).unwrap_or_else(|| {
                    format!(
                        "speccoder config get {}",
                        changes.first().map_or("", |c| c.key.as_str())
                    )
                }),
            ]),
        Ok(out) if out == text => show(root),
        Ok(out) => match std::fs::write(&path, &out) {
            Ok(()) => {
                let keys: Vec<&str> = changes.iter().map(|c| c.key.as_str()).collect();
                let r = show(root);
                let (notes, mut next) = model_notes(&out, changes);
                next.push(speccoder_core::next::NextStep {
                    command: "speccoder config commit".to_owned(),
                    why: "speccoder reads committed configuration only".to_owned(),
                });
                let mut message =
                    format!("changed {} — commit it to put it in force", keys.join(", "));
                for n in notes {
                    message.push('\n');
                    message.push_str(&n);
                }
                CommandResult::new("config", CommandStatus::Ok, &message)
                    .with_next(next)
                    .with_data(
                        r.data()
                            .cloned()
                            .unwrap_or(CommandData::Config(Box::new(read(root)))),
                    )
            }
            Err(e) => CommandResult::new(
                "config",
                CommandStatus::Error,
                &format!("{}: {e}", path.display()),
            ),
        },
    }
}

/// `speccoder config commit`: commits exactly `speccoder.toml`, nothing else
/// staged, once every parser accepts it.
#[must_use]
pub fn commit(root: &Path) -> CommandResult {
    let dir = home(root);
    let text = match std::fs::read_to_string(dir.join(FILE)) {
        Ok(t) => t,
        Err(_) => {
            return CommandResult::new(
                "config",
                CommandStatus::ConfigError,
                "there is no speccoder.toml to commit",
            )
            .with_code("CONFIG-MISSING");
        }
    };
    if let Err(e) = validate(&text) {
        return CommandResult::new("config", CommandStatus::ConfigError, &e)
            .with_code("CONFIG-INVALID");
    }
    let (committed, _) = state(&dir, true);
    if committed {
        return CommandResult::new(
            "config",
            CommandStatus::Ok,
            "speccoder.toml is committed; nothing to do",
        );
    }
    // Which sections changed, for the message.
    let g = Git::at(&dir);
    let before: toml::Table = g
        .run(&["show", &format!("HEAD:./{FILE}")])
        .ok()
        .and_then(|b| toml::from_str(&String::from_utf8_lossy(&b)).ok())
        .unwrap_or_default();
    let after: toml::Table = toml::from_str(&text).unwrap_or_default();
    let mut changed: Vec<&str> = CATALOGUE
        .iter()
        .map(|s| s.name)
        .filter(|s| before.get(*s) != after.get(*s))
        .collect();
    changed.dedup();
    let message = if changed.is_empty() {
        "speccoder: configuration".to_owned()
    } else {
        format!("speccoder: configuration ({})", changed.join(", "))
    };
    match g
        .run(&["add", "--", FILE])
        .and_then(|_| g.run(&["commit", "-q", "-m", &message, "--", FILE]))
    {
        Ok(_) => {
            let id = g
                .line(&["rev-parse", "--short", "HEAD"])
                .unwrap_or_default();
            CommandResult::new(
                "config",
                CommandStatus::Ok,
                &format!("committed speccoder.toml as {id} \"{message}\""),
            )
            .with_data(CommandData::Config(Box::new(read(root))))
        }
        Err(e) => CommandResult::new(
            "config",
            CommandStatus::GitPrecondition,
            &format!("commit failed: {}", e.0),
        ),
    }
}

/// A value for `key` from the words of `config set KEY WORDS…`: JSON when it
/// parses as the key's kind, else words read by the kind (`cargo fmt` for an
/// argv, `40` for a number, `on`/`off` for a switch).
///
/// # Errors
///
/// An unknown key, or words that are no value of its kind.
pub fn value_of(key: &str, words: &[String]) -> Result<Value, String> {
    let at = place(key)?;
    let joined = words.join(" ");
    if let Ok(v) = serde_json::from_str::<Value>(&joined)
        && check(key, at.key.kind, &v).is_ok()
    {
        return Ok(v);
    }
    let v = match at.key.kind {
        Kind::Bool => match joined.as_str() {
            "true" | "on" | "yes" => json!(true),
            "false" | "off" | "no" => json!(false),
            _ => json!(joined),
        },
        Kind::Int(..) => joined
            .parse::<i64>()
            .map_or_else(|_| json!(joined), |n| json!(n)),
        Kind::Num(..) => joined
            .parse::<f64>()
            .map_or_else(|_| json!(joined), |n| json!(n)),
        Kind::Argv | Kind::StrList => json!(words),
        Kind::ArgvList => json!([words]),
        Kind::Str | Kind::Choice(_) | Kind::Duration => json!(joined),
    };
    check(key, at.key.kind, &v).map(|()| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value of `kind` the parsers accept, for the drift test.
    fn sample(k: &Key) -> Value {
        if let Some(d) = k.default
            && let Ok(v) = serde_json::from_str::<Value>(d)
        {
            return v;
        }
        match k.kind {
            Kind::Bool => json!(true),
            Kind::Int(a, _) => json!(a.max(1)),
            Kind::Num(a, b) => json!(f64::midpoint(a, b).min(1.0)),
            Kind::Str => json!("x"),
            Kind::Choice(c) => json!(c[0]),
            Kind::Duration => json!("7d"),
            Kind::Argv => json!(["true"]),
            Kind::ArgvList => json!([["cargo", "check"]]),
            Kind::StrList => json!(["x"]),
        }
    }

    /// The smallest configuration that holds `section` with `key` set.
    fn with(section: &Section, key: &Key) -> Vec<ConfigChange> {
        let prefix = if section.named {
            format!("{}.t", section.name)
        } else {
            section.name.to_owned()
        };
        let mut changes: Vec<ConfigChange> = section
            .keys
            .iter()
            .filter(|r| r.required && r.key != key.key)
            .map(|r| ConfigChange {
                key: format!("{prefix}.{}", r.key),
                value: Some(sample(r)),
                append: None,
            })
            .collect();
        changes.push(ConfigChange {
            key: format!("{prefix}.{}", key.key),
            value: Some(sample(key)),
            append: None,
        });
        // `argv`, `report` and `acceptance_argv` belong to a command tool,
        // which needs its program and report; the rest to cargo-mutants.
        if section.name == "mutation" {
            let set = |key: &str, value: Value| ConfigChange {
                key: key.into(),
                value: Some(value),
                append: None,
            };
            if ["argv", "report", "acceptance_argv"].contains(&key.key) {
                changes.retain(|c| c.key != "mutation.tool");
                changes.insert(0, set("mutation.tool", json!("command")));
                for (k, v) in [
                    ("mutation.argv", json!(["mutants", "{out}"])),
                    ("mutation.report", json!("report.json")),
                ] {
                    if !changes.iter().any(|c| c.key == k) {
                        changes.insert(1, set(k, v));
                    }
                }
            } else {
                changes.retain(|c| c.key != "mutation.tool" || key.key == "tool");
                if key.key != "tool" {
                    changes.insert(0, set("mutation.tool", json!("cargo-mutants")));
                }
            }
        }
        if section.name == "executor" && key.key == "argv" {
            changes.retain(|c| c.key != "executor.kind");
            changes.insert(
                0,
                ConfigChange {
                    key: "executor.kind".into(),
                    value: Some(json!("command")),
                    append: None,
                },
            );
        }
        if section.name == "model" && key.key == "base_url" {
            changes.push(ConfigChange {
                key: "model.provider".into(),
                value: Some(json!("openai-compatible")),
                append: None,
            });
        }
        if section.name == "model" && key.key == "region" {
            changes.push(ConfigChange {
                key: "model.provider".into(),
                value: Some(json!("bedrock")),
                append: None,
            });
        }
        changes
    }

    #[test]
    fn every_catalogued_key_is_accepted_by_the_parsers() {
        for s in CATALOGUE {
            for k in s.keys {
                let changes = with(s, k);
                edit("", &changes).unwrap_or_else(|e| panic!("[{}] {}: {e}", s.name, k.key));
            }
        }
    }

    #[test]
    fn every_key_in_the_reference_is_catalogued() {
        let doc = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/content/docs/configuration.md"),
        )
        .unwrap_or_default();
        assert!(!doc.is_empty(), "the configuration reference is missing");
        let mut section: Option<String> = None;
        let mut missing = Vec::new();
        for line in doc.lines() {
            if line.starts_with("## ") {
                // `## \`[model]\`` sets the section; a heading naming several
                // (or none) leaves rows to name their own.
                let names: Vec<&str> = line.split('`').skip(1).step_by(2).collect();
                section = (names.len() == 1).then(|| {
                    names[0]
                        .trim_matches(['[', ']'])
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_owned()
                });
                if line.contains("Your settings") {
                    break;
                }
                continue;
            }
            let Some(cell) = line.strip_prefix("| ").and_then(|l| l.split(" |").next()) else {
                continue;
            };
            if cell.starts_with("Key") || cell.starts_with("---") {
                continue;
            }
            let mut prefix = section.clone().map(|s| vec![s]).unwrap_or_default();
            for token in cell.split('`').skip(1).step_by(2) {
                let key = match token.strip_prefix('[').and_then(|t| t.split_once("] ")) {
                    Some((table, key)) => {
                        prefix = table.split('.').map(str::to_owned).collect();
                        key.to_owned()
                    }
                    None => token.to_owned(),
                };
                let mut path = prefix.clone();
                if CATALOGUE
                    .iter()
                    .any(|c| c.named && Some(&c.name.to_owned()) == prefix.first())
                {
                    path.insert(1, "t".to_owned());
                }
                path.push(key);
                let dotted = path.join(".");
                if place(&dotted).is_err() {
                    missing.push(dotted);
                }
            }
        }
        assert!(
            missing.is_empty(),
            "documented but not catalogued: {missing:?}"
        );
    }

    #[test]
    fn bedrock_is_set_provider_first_then_region() {
        let set = |key: &str, v: Value| ConfigChange {
            key: key.into(),
            value: Some(v),
            append: None,
        };
        let text = "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n";
        // A region before the provider is refused: it is Bedrock's.
        assert!(edit(text, &[set("model.region", json!("us-west-2"))]).is_err());
        let step = edit(text, &[set("model.provider", json!("bedrock"))]).unwrap();
        assert!(validate(&step).is_err(), "not in force without a region");
        let done = edit(&step, &[set("model.region", json!("us-west-2"))]).unwrap();
        let m = crate::substrate::model_config::parse(&done)
            .unwrap()
            .unwrap();
        assert_eq!(
            (m.provider.as_str(), m.region.as_deref()),
            ("bedrock", Some("us-west-2"))
        );
        // Anything else wrong is still refused on the way.
        assert!(edit(&step, &[set("model.max_output_tokens", json!(0))]).is_err());
        let p = place("model.provider").unwrap();
        assert!(matches!(p.key.kind, Kind::Choice(c) if c.contains(&"bedrock")));
    }

    #[test]
    fn the_step_to_bedrock_is_let_through_only_when_the_region_is_all_that_is_missing() {
        let set = |key: &str, v: Value| ConfigChange {
            key: key.into(),
            value: Some(v),
            append: None,
        };
        // A key Bedrock does not use, left from the provider before.
        let compat = "[model]\nprovider = \"openai-compatible\"\nmodel = \"m.x\"\nbase_url = \"http://localhost:11434/v1\"\n";
        let e = edit(compat, &[set("model.provider", json!("bedrock"))]).unwrap_err();
        assert_eq!(
            e,
            "[model]: `base_url` does not apply to provider `bedrock` (its credentials and endpoint come from AWS) — speccoder config unset model.base_url"
        );
        // Its fix is the unset, from `config set` and from every command.
        assert_eq!(
            crate::substrate::model_config::config_fix(&e).as_deref(),
            Some("speccoder config unset model.base_url")
        );
        assert_eq!(
            crate::substrate::model_config::refused("run", &e).fix(),
            ["speccoder config unset model.base_url"]
        );
        // Any other `[model]` error on the way is still refused.
        let step = "[model]\nprovider = \"bedrock\"\nmodel = \"m.x\"\n";
        assert!(edit(step, &[set("model.model", json!(""))]).is_err());
        assert!(edit(step, &[set("model.structured", json!("tool"))]).is_err());
        assert!(edit(step, &[set("model.reasoning_summary", json!(false))]).is_ok());
    }

    #[test]
    fn switching_to_bedrock_says_to_set_the_region_and_warns_of_a_model_id_bedrock_does_not_use() {
        let dir = tempfile::tempdir().unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(ok.success());
        std::fs::write(
            dir.path().join(FILE),
            "[model]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n",
        )
        .unwrap();
        let r = apply(
            dir.path(),
            &[ConfigChange {
                key: "model.provider".into(),
                value: Some(json!("bedrock")),
                append: None,
            }],
        );
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
        assert!(
            r.message().contains(
                "warning: `claude-sonnet-5-5` is not a Bedrock model id; Bedrock names models like global.anthropic.claude-sonnet-5-5"
            ),
            "{}",
            r.message()
        );
        assert_eq!(
            r.next().first().map(|n| n.command.as_str()),
            Some("speccoder config set model.region <region>")
        );
        // The listing marks the region required for this provider.
        let region = read(dir.path())
            .sections
            .into_iter()
            .find(|s| s.name == "model")
            .and_then(|s| s.keys.into_iter().find(|k| k.key == "region"))
            .unwrap();
        assert!(region.required);
        // With the region set: nothing left to say about it.
        let r = apply(
            dir.path(),
            &[ConfigChange {
                key: "model.region".into(),
                value: Some(json!("eu-central-1")),
                append: None,
            }],
        );
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
        assert!(!r.message().contains("set it next"), "{}", r.message());
        assert_eq!(
            r.next().first().map(|n| n.command.as_str()),
            Some("speccoder config commit")
        );
        // An id Bedrock uses: no warning.
        assert_eq!(
            crate::substrate::model_config::bedrock_model_warning(
                "global.anthropic.claude-sonnet-5-5"
            ),
            None
        );
        assert_eq!(
            crate::substrate::model_config::bedrock_model_warning(
                "arn:aws:bedrock:eu-central-1:123456789012:inference-profile/x"
            ),
            None
        );
    }

    #[test]
    fn an_edit_keeps_comments_and_refuses_what_a_parser_refuses() {
        let text = "# The project's checks.\n[checks.test]\nargv = [\"cargo\", \"test\"] # all of them\n\n# How much an attempt may do.\n[budget]\nmax_turns = 30 # plenty\n";
        let out = edit(
            text,
            &[ConfigChange {
                key: "budget.max_turns".into(),
                value: Some(json!(40)),
                append: None,
            }],
        )
        .unwrap();
        assert!(out.contains("# The project's checks."), "{out}");
        assert!(out.contains("# How much an attempt may do."), "{out}");
        assert!(out.contains("max_turns = 40 # plenty"), "{out}");
        assert!(
            out.contains("argv = [\"cargo\", \"test\"] # all of them"),
            "{out}"
        );
        // Out of range, unknown, the wrong kind: nothing is written.
        for (key, value) in [
            ("budget.max_turns", json!(0)),
            ("budget.max_turn", json!(3)),
            ("sandbox.mode", json!("maybe")),
            ("ui.theme", json!("dark")),
        ] {
            assert!(
                edit(
                    text,
                    &[ConfigChange {
                        key: key.into(),
                        value: Some(value),
                        append: None
                    }]
                )
                .is_err(),
                "{key}"
            );
        }
        // Unset removes the key and an emptied table.
        let out = edit(
            text,
            &[ConfigChange {
                key: "budget.max_turns".into(),
                value: None,
                append: None,
            }],
        )
        .unwrap();
        assert!(!out.contains("[budget]"), "{out}");
        // Append adds once.
        let allow = ConfigChange {
            key: "commands.allow".into(),
            value: None,
            append: Some(json!(["cargo", "fetch"])),
        };
        let once = edit(text, std::slice::from_ref(&allow)).unwrap();
        let twice = edit(&once, &[allow]).unwrap();
        assert_eq!(once, twice);
        assert!(once.contains("allow = [[\"cargo\", \"fetch\"]]"), "{once}");
        // A named table is made under an implicit container.
        let out = edit(
            "",
            &[ConfigChange {
                key: "checks.lint.argv".into(),
                value: Some(json!(["cargo", "clippy"])),
                append: None,
            }],
        )
        .unwrap();
        assert!(
            out.contains("[checks.lint]") && !out.contains("[checks]\n"),
            "{out}"
        );
    }

    /// A unit of more than one byte is refused, never a panic (it aborted
    /// the web host).
    #[test]
    fn a_multibyte_unit_is_refused_not_a_panic() {
        let w = |s: &str| vec![s.to_owned()];
        for bad in ["7д", "1€", "д", "€"] {
            assert!(value_of("approvals.wait", &w(bad)).is_err(), "{bad}");
        }
        assert!(crate::substrate::run_config::parse("[approvals]\nwait = \"7д\"\n").is_err());
    }

    /// Inline tables are tables: a key in one is set and unset like any.
    #[test]
    fn inline_tables_are_edited_not_skipped() {
        let text = "[attest]\nsigstore = { identity = \"a@b\", issuer = \"https://x\" }\n\n[checks]\nlint = { argv = [\"cargo\", \"clippy\"] }\n";
        let unset = |k: &str| ConfigChange {
            key: k.into(),
            value: None,
            append: None,
        };
        let set = |k: &str, v: Value| ConfigChange {
            key: k.into(),
            value: Some(v),
            append: None,
        };
        let out =
            edit(text, &[unset("attest.sigstore.identity")]).unwrap_or_else(|e| panic!("{e}"));
        assert!(!out.contains("a@b") && out.contains("issuer"), "{out}");
        let out =
            edit(text, &[set("checks.lint.tier", json!("full"))]).unwrap_or_else(|e| panic!("{e}"));
        assert!(out.contains("tier = \"full\""), "{out}");
        let out = edit(
            text,
            &[
                unset("attest.sigstore.identity"),
                unset("attest.sigstore.issuer"),
            ],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(!out.contains("sigstore"), "{out}");
    }

    #[test]
    fn words_become_values_of_the_key_s_kind() {
        let w = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(value_of("budget.max_turns", &w("40")).unwrap(), json!(40));
        assert_eq!(value_of("monitor.enabled", &w("on")).unwrap(), json!(true));
        assert_eq!(
            value_of("format.argv", &w("cargo fmt")).unwrap(),
            json!(["cargo", "fmt"])
        );
        assert_eq!(
            value_of("commands.allow", &w("cargo check")).unwrap(),
            json!([["cargo", "check"]])
        );
        assert_eq!(value_of("approvals.wait", &w("2d")).unwrap(), json!("2d"));
        assert!(value_of("budget.max_turns", &w("lots")).is_err());
    }
}
