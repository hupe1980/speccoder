//! Command-line arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// A spec format (`--format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SpecFormat {
    /// GitHub Spec Kit.
    Speckit,
    /// OpenSpec.
    Openspec,
    /// Kiro and cc-sdd.
    Kiro,
}

impl From<SpecFormat> for speccoder_core::spec::FormatKind {
    fn from(f: SpecFormat) -> Self {
        match f {
            SpecFormat::Speckit => Self::Speckit,
            SpecFormat::Openspec => Self::Openspec,
            SpecFormat::Kiro => Self::Kiro,
        }
    }
}

/// `--color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorArg {
    /// A terminal gets colour, unless NO_COLOR says otherwise.
    Auto,
    /// Always, even piped.
    Always,
    /// Never.
    Never,
}

/// Spec-driven coding agent for GitHub Spec Kit and OpenSpec projects.
#[derive(Debug, Parser)]
#[command(
    name = "speccoder",
    version,
    about = "Spec-driven coding, decided by evidence",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// The project: a path inside it (default: the current directory).
    #[arg(
        short = 'C',
        long = "project",
        global = true,
        default_value = ".",
        value_name = "PATH"
    )]
    pub project: PathBuf,

    /// The feature: directory name or numeric prefix (an OpenSpec change: its id).
    #[arg(long, visible_alias = "change", global = true, value_name = "NAME")]
    pub feature: Option<String>,

    /// Print each command result as one JSON object on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Stream JSON Lines events on stdout, ending in the result object.
    #[arg(long, global = true, conflicts_with = "json")]
    pub events: bool,

    /// Colour: auto (a terminal, unless NO_COLOR), always or never.
    #[arg(long, global = true, value_enum, default_value_t = ColorArg::Auto, value_name = "WHEN")]
    pub color: ColorArg,

    /// ASCII marks and borders instead of Unicode symbols.
    #[arg(long, global = true)]
    pub ascii: bool,

    /// Labelled plain lines for screen readers: no symbols, no dashboard.
    #[arg(long, global = true)]
    pub screen_reader: bool,

    /// Lines instead of the dashboard: `run` prints its progress, bare
    /// `speccoder` prints the report.
    #[arg(long, global = true)]
    pub plain: bool,

    /// Print only stops, finishes, resumes and what needs you.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Also print durations, skipped stages and the tails of what failed.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// The spec format (speckit, openspec, kiro), instead of `[spec] format` or detection.
    #[arg(
        long,
        global = true,
        value_enum,
        env = "SPECCODER_FORMAT",
        value_name = "FORMAT"
    )]
    pub format: Option<SpecFormat>,

    /// Without a command: the dashboard at a terminal, else the report.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommands. The daily ones are grouped in the top-level help;
/// the advanced ones are hidden there and listed by `help advanced`.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Set up speccoder here: detect the checks, run them, offer to commit the config.
    Init {
        /// `anthropic` or `openai` (default: the one whose API key variable is set).
        #[arg(long)]
        provider: Option<String>,
        /// The model (default: the provider's speccoder default).
        #[arg(long)]
        model: Option<String>,
        /// Replace an existing `speccoder.toml`.
        #[arg(long)]
        force: bool,
        /// Do not run the checks on the untouched code.
        #[arg(long)]
        no_verify: bool,
        /// Ask nothing (and so commit nothing: the fix line says what to commit).
        #[arg(long)]
        yes: bool,
    },
    /// Check git, the sandbox, the model key, the spec tools and the config; changes nothing.
    Doctor {
        /// A redacted Markdown bundle to paste into an issue.
        #[arg(long)]
        bug_report: bool,
    },
    /// The web interface: the project in your browser (or a window), on 127.0.0.1 only.
    Ui {
        /// The port (default: a free one).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Print the address instead of opening the browser.
        #[arg(long)]
        no_open: bool,
        /// Open it in speccoder's own window (needs a build with `--features app`).
        #[arg(long)]
        window: bool,
    },
    /// Which feature commands work on: list them, or make one current (`feature 2`, `feature greet`).
    Feature {
        /// A name, a number (`2` is `002-…`), or a part of the name only one feature has.
        query: Option<String>,
        /// Forget the current feature.
        #[arg(long, conflicts_with = "query")]
        clear: bool,
    },
    /// The format's planning skills: `spec specify "…"`, `spec plan`, … (OpenSpec: propose, explore, archive).
    Spec {
        /// The skill, or `archive`; without it, list the installed skills.
        name: Option<String>,
        /// What the skill is asked ($ARGUMENTS); for `archive`, the change.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
        /// Never ask inline: questions are left as [NEEDS CLARIFICATION]; approvals stop the run.
        #[arg(long)]
        no_prompt: bool,
        /// archive: confirmed in advance (OpenSpec's checks still run first).
        #[arg(long)]
        yes: bool,
        /// archive: a task ticked without a speccoder verdict that you accept, by ID.
        #[arg(long, value_name = "TASK")]
        allow_unverified: Vec<String>,
    },
    /// Work through the feature's ready tasks, or one (`--task T005`); resumes where it stopped.
    Run {
        /// One task, e.g. T005; without it, ready tasks run in order until the feature is done.
        #[arg(long)]
        task: Option<String>,
        /// At most this many tasks in a feature run.
        #[arg(long, value_name = "N")]
        max_tasks: Option<usize>,
        /// After the last task, converge and work through what it appends, until converged.
        #[arg(long, conflicts_with = "task")]
        converge: bool,
        /// Never ask inline: a call that needs approval stops the attempt (`task approve`).
        #[arg(long)]
        no_prompt: bool,
        /// Run up to N ready `[P]` tasks at once, each in its own worktree, then merge and re-verify.
        #[arg(long, value_name = "N", default_value_t = 1, conflicts_with = "task", value_parser = clap::value_parser!(u8).range(1..=8))]
        parallel: u8,
        /// Run although the feature's checklists have unchecked items (recorded on every commit).
        #[arg(long)]
        ignore_checklists: bool,
        /// With --task: set the task's held attempt aside and write its acceptance tests anew.
        #[arg(long, requires = "task")]
        fresh_acceptance: bool,
        /// Attempt although a check cannot run on the untouched code (it can never judge the task).
        #[arg(long)]
        allow_failing_baseline: bool,
        /// Work in this checkout on branch `speccoder/<feature>` instead of a
        /// worktree (intent may be git-ignored); switches back at the end.
        #[arg(long)]
        in_place: bool,
        /// With --in-place: leave an interrupted in-place run — save its work
        /// under an attempt ref and switch back to the original branch.
        #[arg(long = "return", requires = "in_place", conflicts_with_all = ["task", "converge", "max_tasks"])]
        return_: bool,
    },
    /// Act on one task: approve, reject, accept, skip, nudge, rule or show it.
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Every task and the evidence behind its tick (--live, --cost, --approvals, --markdown).
    #[command(group(clap::ArgGroup::new("view").args(["live", "cost", "approvals", "markdown"]).multiple(false)))]
    Report {
        /// What the current (or last) run is doing, from another terminal.
        #[arg(long)]
        live: bool,
        /// Tokens (and money, if `[model]` has prices) per task.
        #[arg(long)]
        cost: bool,
        /// Calls waiting for a person, and the decisions made.
        #[arg(long)]
        approvals: bool,
        /// GitHub-flavoured Markdown (pull requests, CI summaries).
        #[arg(long)]
        markdown: bool,
    },
    /// Push `speccoder/<feature>` and open a draft pull request with every task's evidence (asks first).
    Pr {
        /// The git remote to push to.
        #[arg(long, default_value = "origin")]
        remote: String,
        /// The branch the pull request targets (default: the checkout's branch).
        #[arg(long)]
        base: Option<String>,
        /// Push and open without asking.
        #[arg(long, conflicts_with = "dry_run")]
        yes: bool,
        /// Only show the body and the commands; push nothing.
        #[arg(long)]
        dry_run: bool,
        /// Hand off although the full-tier checks failed on the branch (said in the body).
        #[arg(long)]
        force: bool,
    },
    /// Verify the signed evidence of the feature's ticks, or sign what is unsigned.
    Attest {
        #[command(subcommand)]
        action: AttestAction,
    },
    /// Help: `help advanced` for the rest, `help <command>` for one.
    Help {
        /// `advanced`, or a command.
        topic: Vec<String>,
    },
    // ── advanced (hidden from the top-level help) ─────────────────────
    /// Run the checks declared in speccoder.toml and report the evidence.
    #[command(hide = true)]
    Verify {
        /// Only checks of this tier.
        #[arg(long, value_enum)]
        tier: Option<TierArg>,
        /// Compare with a result saved from `speccoder --json verify`.
        #[arg(long, value_name = "FILE")]
        against: Option<PathBuf>,
    },
    /// Assess a finished feature against its spec, plan and tasks and append what remains.
    #[command(hide = true)]
    Converge {
        /// In this checkout on `speccoder/<feature>`, as `run --in-place`.
        #[arg(long)]
        in_place: bool,
    },
    /// The project's artifacts and tasks as speccoder reads them (read-only).
    #[command(hide = true)]
    Inspect,
    /// Strictly replay a recorded run from the journal (calls no model).
    #[command(hide = true)]
    Replay {
        /// A run id, as shown in run reports and commit trailers.
        run: String,
    },
    /// Create or reuse the git worktree speccoder works in for a feature.
    #[command(hide = true)]
    Worktree,
    /// Serve the Agent Client Protocol on stdio, for editors (Zed, JetBrains, …).
    #[command(hide = true)]
    Acp,
    /// Install speccoder as a Spec Kit workflow step type, with a workflow that uses it.
    #[command(hide = true)]
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Run the evaluation suite and judge each case with its hidden oracle.
    #[command(hide = true)]
    Eval {
        /// Suite directory (default: ./evals).
        #[arg(default_value = "evals")]
        dir: PathBuf,
        /// Only this case.
        #[arg(long)]
        case: Option<String>,
        /// Keep each case's repository and journal under this directory (for replay).
        #[arg(long, value_name = "DIR")]
        keep: Option<PathBuf>,
        /// Run the suite this many times (results vary between runs).
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..=100))]
        repeat: u16,
        /// Let another agent make the attempts (claude, codex; default: native).
        #[arg(long, value_name = "KIND", value_parser = ["native", "claude", "codex"])]
        executor: Option<String>,
    },
    /// Public benchmarks as evaluation cases.
    #[command(hide = true)]
    Bench {
        #[command(subcommand)]
        action: BenchAction,
    },
    /// Ask the configured model how it would implement one task (nothing is executed).
    #[command(hide = true)]
    Propose {
        /// Task ID, e.g. T005.
        task: String,
    },
    /// Explain a code speccoder prints (`error[CODE]`, `stopped[CODE]`), offline.
    #[command(hide = true)]
    Explain {
        /// The code, e.g. NEEDS-APPROVAL.
        code: Option<String>,
        /// List every code.
        #[arg(long)]
        list: bool,
    },
    /// The JSON Schema of an output: `result`, `event`, or a payload; without a name, the list.
    #[command(hide = true)]
    Schema {
        /// The schema's name.
        name: Option<String>,
    },
    /// Print a shell completion script.
    #[command(hide = true)]
    Completion {
        /// The shell.
        shell: clap_complete::Shell,
    },
    /// The dashboard: tasks with evidence, the live attempt, the approval queue.
    #[command(hide = true)]
    Tui,
}

/// `speccoder task` verbs; the task comes first after the verb.
#[derive(Debug, Subcommand)]
pub enum TaskAction {
    /// Approve the call the task stopped for (its class for the task, by default).
    Approve {
        /// The task, e.g. T005.
        #[arg(required_unless_present = "all")]
        task: Option<String>,
        /// One waiting call, by its id or a prefix of it (`report --approvals`).
        #[arg(long)]
        call: Option<String>,
        /// Every waiting call (of the task, or of the feature), each recorded on its own.
        #[arg(long)]
        all: bool,
        /// How far the approval reaches: this call, the task, or the run.
        #[arg(long, value_enum, default_value_t = ScopeArg::Task)]
        scope: ScopeArg,
        /// A note the attempt is told with the approval.
        #[arg(long)]
        note: Option<String>,
    },
    /// Reject the call the task stopped for; the model is told the reason.
    Reject {
        /// The task, e.g. T005.
        #[arg(required_unless_present = "all")]
        task: Option<String>,
        /// One waiting call, by its id or a prefix of it.
        #[arg(long)]
        call: Option<String>,
        /// Every waiting call (of the task, or of the feature), each recorded on its own.
        #[arg(long)]
        all: bool,
        /// Why — the model reads this.
        #[arg(long)]
        reason: String,
    },
    /// Tick the task on your word (`attested`, never shown as verified).
    Accept {
        /// The task, e.g. T005.
        task: String,
        /// Why it is done.
        #[arg(long)]
        reason: String,
        /// In this checkout on `speccoder/<feature>`, as `run --in-place`.
        #[arg(long)]
        in_place: bool,
    },
    /// Skip the task: runs pass it by, and what depends on it stays blocked.
    Skip {
        /// The task, e.g. T005.
        task: String,
        /// Why it is skipped.
        #[arg(long, required_unless_present = "undo")]
        reason: Option<String>,
        /// Take a skip back.
        #[arg(long, conflicts_with = "reason")]
        undo: bool,
    },
    /// Leave a note for the task's next attempts (the model reads it).
    Nudge {
        /// The task, e.g. T005.
        task: String,
        /// The note.
        #[arg(long, required_unless_present = "clear")]
        note: Option<String>,
        /// Remove the task's notes.
        #[arg(long, conflicts_with = "note")]
        clear: bool,
    },
    /// Rule on a held attempt whose tests reject an alternative: the tests are right.
    Rule {
        /// The task, e.g. T005.
        task: String,
        /// The tests are right: the flagged alternative is not what the spec allows.
        #[arg(long, required = true)]
        tests_right: bool,
        /// Why (recorded with the tick).
        #[arg(long)]
        reason: String,
        /// One finding by its id (`permitted-1`); default: every finding held.
        #[arg(long, value_name = "ID")]
        variant: Option<String>,
    },
    /// The task's state and the evidence behind it.
    Show {
        /// The task, e.g. T005.
        task: String,
    },
}

/// `speccoder attest` actions.
#[derive(Debug, Subcommand)]
pub enum AttestAction {
    /// Re-check every task's attestation against its commit, signatures
    /// against your trust (exit 26 when one does not hold).
    Verify {
        /// A commit or a range (`main..speccoder/001-x`); default: the
        /// feature's task commits.
        rev: Option<String>,
        /// SSH allowed-signers file (default: git's gpg.ssh.allowedSignersFile).
        #[arg(long, value_name = "FILE")]
        allowed_signers: Option<PathBuf>,
        /// The Sigstore certificate identity to expect.
        #[arg(long)]
        certificate_identity: Option<String>,
        /// The Sigstore OIDC issuer to expect.
        #[arg(long)]
        certificate_oidc_issuer: Option<String>,
        /// Fail on unsigned statements too.
        #[arg(long)]
        require_signed: bool,
    },
    /// Sign the unsigned statements of the feature's task commits (for
    /// `[attest] sign_at = "later"`, or a key that was not set up).
    Sign {
        /// A commit or a range; default: the feature's task commits.
        rev: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum BenchAction {
    /// Turn SWE-bench-format instances (JSON Lines) into cases for `speccoder eval`.
    Import {
        /// The instances (JSON Lines or a JSON array).
        instances: PathBuf,
        /// One local clone per repository, named owner__name.
        #[arg(long, value_name = "DIR")]
        repos: PathBuf,
        /// The suite directory to write.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
        /// At most this many instances.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// The test command, when the dataset has none (default: detected).
        #[arg(long, value_name = "CMD")]
        test_cmd: Option<String>,
    },
}

/// `speccoder workflow` actions.
#[derive(Debug, Subcommand)]
pub enum WorkflowAction {
    /// Write `.specify/workflows/steps/speccoder/` and `.specify/workflows/speccoder/workflow.yml`.
    Install {
        /// Replace existing files.
        #[arg(long)]
        force: bool,
    },
}

/// Check tiers.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TierArg {
    /// Checks run for every task.
    Fast,
    /// Slower checks (linters, audits): once after a run ticks the last task, and by `verify`.
    Full,
}

/// How far an approval reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ScopeArg {
    /// This call only.
    Once,
    /// This call's class for the rest of the task.
    Task,
    /// This call's class for every task of the run.
    Run,
}

impl From<ScopeArg> for speccoder_runtime::approvals::Scope {
    fn from(s: ScopeArg) -> Self {
        match s {
            ScopeArg::Once => Self::Once,
            ScopeArg::Task => Self::Task,
            ScopeArg::Run => Self::Run,
        }
    }
}
