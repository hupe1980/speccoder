//! Every key of `speccoder.toml`: its type, default and meaning.

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
    pub(super) const fn name(self) -> &'static str {
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

pub(super) const fn k(
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

pub(super) const fn req(key: &'static str, kind: Kind, about: &'static str) -> Key {
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
pub(super) const FORMATS: &[&str] = &[
    speccoder_core::spec::FormatKind::ALL[0].name(),
    speccoder_core::spec::FormatKind::ALL[1].name(),
    speccoder_core::spec::FormatKind::ALL[2].name(),
];

pub(super) const PRICE: Kind = Kind::Num(0.0, 100_000.0);

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
                Some("500000"),
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
        about: "How much an attempt may do, how often a task is tried, and what a run may spend.",
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
            k(
                "cap",
                Kind::Str,
                None,
                "What a run may spend: tokens (\"2M\", \"500k\") or dollars (\"$5\", at [model]'s prices); `run --cap` overrides it",
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
        about: "What runs without asking, how long a call waits for a person, and how often a run may ask.",
        named: false,
        keys: &[
            k(
                "contained",
                Kind::Choice(&["allow", "ask"]),
                Some("\"allow\""),
                "Commands the OS sandbox confines (writes only here, no network beyond loopback) run without asking; \"ask\" asks for each",
            ),
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
                "Derive acceptance tests from the spec: a story's oracle before its first code task, a convergence gap's own test",
            ),
            k(
                "held_out",
                Kind::Bool,
                Some("true"),
                "Also write held-out tests the implementer never sees; a story's last code task needs them to pass",
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
        name: "ask",
        title: "Ask",
        about: "The read-only conversation beside every view: its model and its limits.",
        named: false,
        keys: &[
            k(
                "model",
                Kind::Str,
                None,
                "Ask's model; [model]'s when unset (same provider)",
            ),
            k(
                "max_turn_tokens",
                Kind::Int(1_000, 10_000_000),
                Some("100000"),
                "Tokens one question may use, cache reads apart",
            ),
            k(
                "daily_tokens",
                Kind::Int(0, 1_000_000_000),
                Some("1000000"),
                "Tokens Ask may use in a UTC day; runs are never counted",
            ),
        ],
    },
    Section {
        name: "ladder",
        title: "Promotion",
        about: "When a quick change or bug that grew is offered as a spec of its own.",
        named: false,
        keys: &[k(
            "promote_files",
            Kind::Int(1, 10_000),
            Some("8"),
            "More files changed than this offers promotion",
        )],
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
        about: "Who makes the attempts: speccoder's own loop, or an external agent over ACP whose calls speccoder decides.",
        named: false,
        keys: &[
            k(
                "agent",
                Kind::Choice(&["copilot", "opencode", "claude", "codex", "gemini"]),
                None,
                "GitHub Copilot, OpenCode, Claude Code, Codex or Gemini CLI, over ACP",
            ),
            k(
                "kind",
                Kind::Choice(&["native", "acp", "command"]),
                None,
                "Without an agent: native, any ACP agent (acp) or any command, by argv",
            ),
            k(
                "model",
                Kind::Str,
                None,
                "The agent's model, where it offers a choice",
            ),
            k(
                "mode",
                Kind::Str,
                None,
                "The agent's own permission mode; unset, speccoder asks for the one that asks first",
            ),
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
                "For acp: the agent's command; for command: the program, which reads the task on stdin",
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
