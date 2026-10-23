//! Command-line arguments.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Spec-driven coding agent for GitHub Spec Kit projects.
#[derive(Debug, Parser)]
#[command(name = "speccoder", version, about, long_about = None)]
pub struct Cli {
    /// Print each command result as one JSON object on stdout.
    #[arg(long, global = true)]
    pub json: bool,

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
    /// Report the Spec Kit project's artifacts and tasks (read-only).
    Inspect {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature to inspect: directory name or numeric prefix.
        #[arg(long)]
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
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Assess a finished feature against its spec, plan and tasks (read-only) and append what remains.
    Converge {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Show a feature's tasks and the evidence behind each tick (read-only, no model).
    Report {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
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
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// List calls that stopped a task for approval, and the decisions made.
    Approvals {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Approve the call a task stopped for (its path, or its exact command) for that task.
    Approve {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Reject the call a task stopped for; the model is told the reason.
    Reject {
        /// Task ID, e.g. T005.
        #[arg(long)]
        task: String,
        /// Why — the model reads this.
        #[arg(long)]
        reason: String,
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
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
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
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
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
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
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Tokens (and money, if `[model]` has prices) per task, from the speccoder branch.
    Cost {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Push `speccoder/<feature>` and open a draft pull request with every task's evidence (asks first).
    Pr {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
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
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
    /// Create or reuse the git worktree speccoder works in for a feature.
    Worktree {
        /// A path inside the project (default: the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Feature: directory name or numeric prefix.
        #[arg(long)]
        feature: Option<String>,
    },
}

/// `speccoder bench` actions.
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
    /// Slower checks (linters, audits), run by `verify` only.
    Full,
}
