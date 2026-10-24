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
#[command(name = "speccoder", version, about, long_about = None)]
pub struct Cli {
    /// Print each command result as one JSON object on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Stream JSON Lines events on stdout, ending in the result object.
    #[arg(long, global = true, conflicts_with = "json")]
    pub events: bool,

    /// Colour: auto (a terminal, unless NO_COLOR), always or never.
    #[arg(long, global = true, value_enum, default_value_t = ColorArg::Auto)]
    pub color: ColorArg,

    /// ASCII marks and borders instead of Unicode symbols.
    #[arg(long, global = true)]
    pub ascii: bool,

    /// Labelled plain lines for screen readers: no symbols, no dashboard.
    #[arg(long, global = true)]
    pub screen_reader: bool,

    /// Print only stops, finishes, resumes and what needs you.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Also print durations, skipped stages and the tails of what failed.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// The project's spec format, instead of `[spec] format` or detection.
    #[arg(long, global = true, value_enum, env = "SPECCODER_FORMAT")]
    pub format: Option<SpecFormat>,

    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Detect the project's checks and write `speccoder.toml` (review, then commit it).
    Init {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// `anthropic` or `openai` (default: the one whose API key variable is set).
        #[arg(long)]
        provider: Option<String>,
        /// The model (default: the provider's speccoder default).
        #[arg(long)]
        model: Option<String>,
        /// Replace an existing `speccoder.toml`.
        #[arg(long)]
        force: bool,
    },
    /// Run an installed Agent Skill (e.g. speckit-specify, -plan, -tasks), or list them.
    Skill {
        /// The skill (`speckit-plan`, or just `plan`); without it, list the installed skills.
        name: Option<String>,
        /// What the skill is asked ($ARGUMENTS), e.g. a feature description.
        #[arg(trailing_var_arg = true)]
        arguments: Vec<String>,
        /// A path inside the project (default: the current directory).
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Never ask inline: questions are left as [NEEDS CLARIFICATION]; approvals stop the run.
        #[arg(long)]
        no_prompt: bool,
    },
    /// Planning in the project's spec format: run one of its skills (Spec
    /// Kit: specify, clarify, plan, tasks, …; OpenSpec: propose, explore,
    /// update-change), list them, or archive an OpenSpec change.
    Spec {
        /// The skill, or `archive`; without it, list the installed skills.
        name: Option<String>,
        /// What the skill is asked ($ARGUMENTS); for `archive`, the change.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
        /// A path inside the project (default: the current directory).
        #[arg(long, default_value = ".")]
        path: PathBuf,
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
    /// Report the project's artifacts and tasks (read-only).
    Inspect {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature to inspect: directory name or numeric prefix (an OpenSpec
        /// change: its id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Run the checks declared in speccoder.toml and report the evidence.
    Verify {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Only checks of this tier.
        #[arg(long, value_enum)]
        tier: Option<TierArg>,
        /// Compare with a result saved from `speccoder --json verify`.
        #[arg(long, value_name = "FILE")]
        against: Option<PathBuf>,
    },
    /// Run one attempt at a task (`--task`), or work through the feature's ready tasks.
    Run {
        /// Task ID, e.g. T005; without it, ready tasks run in order until the feature is done.
        #[arg(long)]
        task: Option<String>,
        /// At most this many tasks in a feature run.
        #[arg(long, value_name = "N")]
        max_tasks: Option<usize>,
        /// After the last task, converge and work through what it appends, until converged.
        #[arg(long, conflicts_with = "task")]
        converge: bool,
        /// Never ask inline: a call that needs approval stops the attempt (see `approvals`).
        #[arg(long)]
        no_prompt: bool,
        /// Run up to N ready `[P]` tasks at once, each in its own worktree, then merge and re-verify.
        #[arg(long, value_name = "N", default_value_t = 1, conflicts_with = "task", value_parser = clap::value_parser!(u8).range(1..=8))]
        parallel: u8,
        /// Run although the feature's checklists have unchecked items (recorded on every commit).
        #[arg(long)]
        ignore_checklists: bool,
        /// Work in this checkout on branch `speccoder/<feature>` instead of a
        /// worktree (intent may be git-ignored); switches back at the end.
        #[arg(long)]
        in_place: bool,
        /// With --in-place: leave an interrupted in-place run — save its work
        /// under an attempt ref and switch back to the original branch.
        #[arg(long = "return", requires = "in_place", conflicts_with_all = ["task", "converge", "max_tasks"])]
        return_: bool,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Assess a finished feature against its spec, plan and tasks (read-only) and append what remains.
    Converge {
        /// In this checkout on `speccoder/<feature>`, as `run --in-place`.
        #[arg(long)]
        in_place: bool,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Show a feature's tasks and the evidence behind each tick (read-only, no model).
    Report {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
        /// The report as GitHub-flavoured Markdown (pull requests, CI summaries).
        #[arg(long)]
        markdown: bool,
    },
    /// What the project's current (or last) run is doing, from another terminal.
    Status {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// A live dashboard: tasks and their evidence, the running attempt, the approval queue.
    Tui {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// List calls that stopped a task for approval, and the decisions made.
    Approvals {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Approve the call a task stopped for (its path, or its exact command) for that task.
    Approve {
        /// Task ID, e.g. T005.
        #[arg(long, required_unless_present = "all")]
        task: Option<String>,
        /// One waiting call, by its id or a prefix of it (`speccoder approvals`).
        #[arg(long)]
        call: Option<String>,
        /// Every waiting call (of `--task`, or of the feature), each recorded on its own.
        #[arg(long)]
        all: bool,
        /// How far the approval reaches: this call, the task, or the run.
        #[arg(long, value_enum, default_value_t = ScopeArg::Task)]
        scope: ScopeArg,
        /// A note the attempt is told with the approval.
        #[arg(long)]
        note: Option<String>,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Reject the call a task stopped for; the model is told the reason.
    Reject {
        /// Task ID, e.g. T005.
        #[arg(long, required_unless_present = "all")]
        task: Option<String>,
        /// One waiting call, by its id or a prefix of it (`speccoder approvals`).
        #[arg(long)]
        call: Option<String>,
        /// Every waiting call (of `--task`, or of the feature), each recorded on its own.
        #[arg(long)]
        all: bool,
        /// Why — the model reads this.
        #[arg(long)]
        reason: String,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Tick a task as done on a person's word (`attested`, never shown as verified).
    Accept {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// Why it is done.
        #[arg(long)]
        reason: String,
        /// In this checkout on `speccoder/<feature>`, as `run --in-place`.
        #[arg(long)]
        in_place: bool,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Skip a task: runs pass it by, and what depends on it stays blocked.
    Skip {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// Why it is skipped.
        #[arg(long, required_unless_present = "undo")]
        reason: Option<String>,
        /// Take a skip back.
        #[arg(long, conflicts_with = "reason")]
        undo: bool,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Leave a note for a task's next attempts (the model reads it).
    Nudge {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// The note.
        #[arg(long, required_unless_present = "clear")]
        note: Option<String>,
        /// Remove the task's notes.
        #[arg(long, conflicts_with = "note")]
        clear: bool,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Tokens (and money, if `[model]` has prices) per task, from the speccoder branch.
    Cost {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Push `speccoder/<feature>` and open a draft pull request with every task's evidence (asks first).
    Pr {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
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
    /// Serve the Agent Client Protocol on stdio, for editors (Zed, JetBrains, …).
    Acp,
    /// Install speccoder as a Spec Kit workflow step type, with a workflow that uses it.
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Run the evaluation suite and judge each case with its hidden oracle.
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
    /// Signed evidence per ticked task: verify it, or sign what is unsigned.
    Attest {
        #[command(subcommand)]
        action: AttestAction,
    },
    /// Public benchmarks as evaluation cases.
    Bench {
        #[command(subcommand)]
        action: BenchAction,
    },
    /// Strictly replay a recorded run from the journal (calls no model).
    Replay {
        /// A run id, as shown in run reports and commit trailers.
        run: String,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Ask the configured model how it would implement one task (nothing is executed).
    Propose {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Create or reuse the git worktree speccoder works in for a feature.
    Worktree {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix (an OpenSpec change: its
        /// id, also as --change).
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
    },
    /// Explain a code speccoder prints (`error[CODE]`, `stopped[CODE]`), offline.
    Explain {
        /// The code, e.g. NEEDS-APPROVAL.
        code: Option<String>,
        /// List every code.
        #[arg(long)]
        list: bool,
    },
    /// Print the JSON Schema of speccoder's output: `result`, `event`, or a
    /// command's payload; without a name, list them.
    Schema {
        /// The schema's name.
        name: Option<String>,
    },
}

/// `speccoder bench` actions.
#[derive(Debug, Subcommand)]
pub enum AttestAction {
    /// Re-check every task's attestation against its commit, signatures
    /// against your trust (exit 26 when one does not hold).
    Verify {
        /// A commit or a range (`main..speccoder/001-x`); default: the
        /// feature's task commits.
        rev: Option<String>,
        /// A path inside the project.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
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
        /// A path inside the project.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long, visible_alias = "change")]
        feature: Option<String>,
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
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
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
