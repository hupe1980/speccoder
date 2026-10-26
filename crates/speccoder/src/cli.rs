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
    /// Fix what in the project's setup stops a run: commit it in your checkout.
    #[command(hide = true)]
    Fix {
        /// Show what it would do, and change nothing.
        #[arg(long)]
        dry_run: bool,
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
        /// Open it in speccoder's own window (on Linux, a build with `--features app`).
        #[arg(long)]
        window: bool,
    },
    /// A small change, judged like any other: `do "make the login button blue"` records and runs it.
    Do {
        /// The change, in your words.
        request: String,
        /// When it is done (repeatable): the acceptance scenarios its tests are written from.
        #[arg(long = "check", value_name = "WHEN")]
        checks: Vec<String>,
        /// Record it (a story, a requirement, a task — committed) without running it.
        #[arg(long)]
        no_run: bool,
        /// Never ask inline: approvals stop the run.
        #[arg(long)]
        no_prompt: bool,
    },
    /// A bug, fixed on evidence: `bug "<what happens>" --expected "<what should>"` reproduces it, asks you once, fixes it.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Bug {
        #[command(subcommand)]
        action: Option<BugAction>,
        /// What happens (the symptom), in your words; a symptom that reads
        /// `confirm` or `run` follows `--`.
        symptom: Option<String>,
        /// What should happen instead (repeatable: one per behaviour).
        #[arg(long, value_name = "WORDS")]
        expected: Vec<String>,
        /// What must keep working as it does (repeatable).
        #[arg(long, value_name = "WORDS")]
        unchanged: Vec<String>,
        /// The GitHub issue it comes from (its text is context, never the triad).
        #[arg(long, value_name = "N")]
        issue: Option<u64>,
        /// Your own failing test, one per --expected, in order (naming it confirms it).
        #[arg(long = "test", value_name = "PATH::NAME")]
        tests: Vec<String>,
        /// Someone else's fix to judge: a patch file or `pr:N` (no retries).
        #[arg(long, value_name = "PATCH|pr:N")]
        candidate: Option<String>,
        /// Continue a bug Spec Kit's `bug` extension created (`.specify/bugs/<slug>/`).
        #[arg(long, value_name = "SLUG")]
        slug: Option<String>,
        /// Record it (and commit exactly that) without reproducing it.
        #[arg(long)]
        no_run: bool,
        /// Never ask inline: the reproductions wait for `bug confirm`.
        #[arg(long)]
        no_prompt: bool,
    },
    /// What waits for your decision in every feature, the most urgent first, with the command for each.
    Inbox,
    /// What you can do now in the project: every action the interfaces offer, with its command.
    #[command(hide = true)]
    Actions {
        /// Also the actions of this task (as when an interface has it selected).
        #[arg(long)]
        task: Option<String>,
    },
    /// GitHub: sign in with a short code in your browser, see who speccoder is there, sign out.
    Auth {
        #[command(subcommand)]
        action: Option<AuthAction>,
    },
    /// Review the spec requirement by requirement: approve each wording, or request changes with a comment.
    Review {
        #[command(subcommand)]
        action: Option<ReviewAction>,
    },
    /// GitHub issues: list the open ones; `start 42` specifies a feature from one.
    Issue {
        #[command(subcommand)]
        action: Option<IssueAction>,
    },
    /// Calls waiting for a person, and the decisions made; `batch` decides a run's waiting calls together.
    #[command(hide = true)]
    Approvals {
        #[command(subcommand)]
        action: Option<ApprovalsAction>,
    },
    /// The project's configuration: every key of speccoder.toml, its value and default; set, unset, commit.
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
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
    #[command(hide = true)]
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
    /// Keep the changes set aside before the task's run: committed onto the feature's branch as yours.
    Restore {
        /// The task, e.g. T005.
        task: String,
    },
    /// Discard the changes set aside before the task's run.
    Drop {
        /// The task, e.g. T005.
        task: String,
    },
    /// The task's state and the evidence behind it.
    Show {
        /// The task, e.g. T005.
        task: String,
        /// Also what the model said in the task's last attempt, turn by turn (a claim).
        #[arg(long)]
        account: bool,
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
        /// Check every ticked task of the branch instead (what
        /// `speccoder-verify` checks): `REV` is the head (default `HEAD`).
        #[arg(long)]
        branch: bool,
        /// With `--branch`: only the units touched since the merge base
        /// with this revision.
        #[arg(long, value_name = "REV", requires = "branch")]
        base: Option<String>,
        /// With `--branch`: the allowed-signers file read from git at a
        /// revision, `<rev>:<path>` (the base's, never the head's).
        #[arg(
            long,
            value_name = "REV:PATH",
            requires = "branch",
            conflicts_with = "allowed_signers"
        )]
        allowed_signers_rev: Option<String>,
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
    /// Turn SWE-bench-format instances into cases for `speccoder eval`.
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

/// `speccoder auth` actions; without one, `status`.
#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Sign in with a short code in your browser; `--with-token` reads a token from stdin.
    Login {
        /// The GitHub host (default: the repository's remote's, else github.com).
        #[arg(long)]
        host: Option<String>,
        /// Read the token from stdin.
        #[arg(long)]
        with_token: bool,
    },
    /// Who speccoder is on the host, and where its token comes from.
    Status {
        /// The GitHub host (default: the repository's remote's, else github.com).
        #[arg(long)]
        host: Option<String>,
    },
    /// Forget the host's token (the keychain's; an environment variable stays yours).
    Logout {
        /// The GitHub host (default: the repository's remote's, else github.com).
        #[arg(long)]
        host: Option<String>,
    },
}

/// `speccoder review` actions; without one, every requirement and its review.
#[derive(Debug, Subcommand)]
pub enum ReviewAction {
    /// Approve requirements on their current wording (none named: every one not yet approved).
    Approve {
        /// `FR-001`, `SC-002`, …
        ids: Vec<String>,
        /// A note kept with the approval.
        #[arg(long)]
        comment: Option<String>,
    },
    /// Request changes to requirements: the comment says what they should say.
    Request {
        /// `FR-001`, `SC-002`, …
        #[arg(required = true)]
        ids: Vec<String>,
        /// What should change (required).
        #[arg(long, short)]
        comment: String,
    },
    /// Answer an open question (`Q1`): your words replace it where it stands.
    Answer {
        /// `Q1`, `Q2`, … as `speccoder review` lists them.
        question: String,
        /// The answer.
        answer: String,
    },
    /// Reword a requirement yourself; its approval lapses (or `--approve` the new words).
    Edit {
        /// `FR-001`, `SC-002`, …
        id: String,
        /// The new wording (the line keeps its identifier).
        wording: String,
        /// Approve the new wording as you write it.
        #[arg(long)]
        approve: bool,
    },
    /// Remove the decision on requirements.
    Clear {
        /// `FR-001`, `SC-002`, …
        #[arg(required = true)]
        ids: Vec<String>,
    },
}

/// `speccoder issue` actions.
#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// The repository's open issues (the default).
    List,
    /// Specify a feature from an issue: its title, text and address become the description.
    Start {
        /// The issue: `42`, `#42` or its address.
        issue: String,
        /// Never ask inline: questions are left as [NEEDS CLARIFICATION].
        #[arg(long)]
        no_prompt: bool,
        /// Comment on the issue that it is being specified (asked at a terminal otherwise).
        #[arg(long, conflicts_with = "no_comment")]
        comment: bool,
        /// Do not comment on the issue.
        #[arg(long)]
        no_comment: bool,
        /// Also assign the issue to you.
        #[arg(long)]
        assign: bool,
    },
    /// Comment on an issue that the current feature is being specified from it (once per feature).
    Comment {
        /// The issue: `42`, `#42` or its address.
        issue: String,
        /// Also assign the issue to you.
        #[arg(long)]
        assign: bool,
    },
}

/// `speccoder approvals` actions.
#[derive(Debug, Subcommand)]
pub enum ApprovalsAction {
    /// Decide the calls waiting in a run's batch together, each once:
    /// `approvals batch run-… approve --digest D`.
    Batch {
        /// The run (`speccoder approvals` lists it).
        run: String,
        /// `approve` or `reject`.
        verdict: VerdictArg,
        /// The digest of the list you saw (`speccoder approvals`).
        #[arg(long)]
        digest: String,
        /// Only these calls (default: every listed one).
        #[arg(long = "call")]
        calls: Vec<String>,
        /// A rejection's reason, which the model is told.
        #[arg(long)]
        reason: Option<String>,
        /// Start the run's asking budget again.
        #[arg(long)]
        resume_asking: bool,
    },
}

/// A batch decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum VerdictArg {
    /// Each listed call runs once.
    Approve,
    /// Each listed call is refused, with one reason.
    Reject,
}

/// `speccoder config` actions.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// One key's value (or its default), e.g. `config get budget.max_turns`.
    Get {
        /// The dotted key.
        key: String,
    },
    /// Set a key: `config set budget.max_turns 40`, `config set format.argv cargo fmt`,
    /// or JSON (`config set commands.allow '[["cargo","check"]]'`); words starting
    /// with `-` after `--` (`config set checks.lint.argv -- cargo clippy -- -D warnings`).
    /// A prefix list (`commands.allow_prefix`, `sandbox.network_allow_prefix`)
    /// shows what each new prefix would newly allow and is written only once
    /// you confirm (`--yes` without a terminal).
    Set {
        /// The dotted key.
        key: String,
        /// The value: words read by the key's kind, or JSON.
        #[arg(required = true, num_args = 1..)]
        value: Vec<String>,
        /// Confirm a prefix list's preview without being asked.
        #[arg(long)]
        yes: bool,
    },
    /// What a prefix would newly allow, from the project's journal:
    /// `config coverage commands.allow_prefix npm run`.
    Coverage {
        /// `commands.allow_prefix` or `sandbox.network_allow_prefix`.
        key: String,
        /// The prefix's program and arguments.
        #[arg(required = true, num_args = 1.., allow_hyphen_values = true)]
        prefix: Vec<String>,
    },
    /// Remove a key, so its default applies.
    Unset {
        /// The dotted key.
        key: String,
    },
    /// Commit speccoder.toml (alone): speccoder reads committed configuration only.
    Commit,
}

/// `speccoder bug` actions.
#[derive(Debug, Subcommand)]
pub enum BugAction {
    /// Confirm a bug's reproductions (bound to the digest shown), or reject them with a reason.
    Confirm {
        /// The bug, as `<unit>/<task>` (e.g. `007-bugfixes/T003`, `bugs/login-timeout/bug:login-timeout`).
        task: String,
        /// The digest you were shown (asked for at a terminal when left out).
        #[arg(long)]
        digest: Option<String>,
        /// Reject them: the author tries again, told your reason.
        #[arg(long, requires = "reason")]
        reject: bool,
        /// Why (with --reject).
        #[arg(long)]
        reason: Option<String>,
        /// Record the decision without running what follows.
        #[arg(long)]
        no_run: bool,
    },
    /// Continue a bug: reproduce it again, or run its confirmed fix.
    Run {
        /// A bug of Spec Kit's `bug` extension, by slug.
        #[arg(long, conflicts_with = "task")]
        slug: Option<String>,
        /// Or `<unit>/<task>`.
        #[arg(long)]
        task: Option<String>,
        /// Never ask inline.
        #[arg(long)]
        no_prompt: bool,
    },
}

/// `speccoder workflow` actions.
#[derive(Debug, Subcommand)]
pub enum WorkflowAction {
    /// Write `.specify/workflows/steps/speccoder/` and `.specify/workflows/speccoder/workflow.yml`
    /// — or, with `--overlay NAME`, an overlay of one of Spec Kit's workflows (`bugfix`).
    Install {
        /// Replace existing files.
        #[arg(long)]
        force: bool,
        /// An overlay instead: `bugfix` replaces the bug workflow's `test` step with speccoder's.
        #[arg(long, value_name = "NAME")]
        overlay: Option<String>,
    },
    /// Remove an overlay `install --overlay` wrote.
    Uninstall {
        /// The overlay (`bugfix`).
        #[arg(long, value_name = "NAME")]
        overlay: String,
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
