//! Stable codes for every result that is not plainly `ok`:
//! what went wrong, why it matters, how to fix it and where it is
//! documented. A script branches on the code, a person reads the fix.
//!
//! Every non-ok status has a default code, so no result leaves without
//! one; a call site that knows the cause better names a specific code with
//! the same status.

use crate::command::CommandStatus;

/// How a code presents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Something failed: `error[CODE]`.
    Error,
    /// The run waits for a person: `stopped[CODE]`.
    Stop,
    /// It worked, with less than the full evidence: `warning[CODE]`.
    Warning,
}

impl Kind {
    /// `error`, `stopped` or `warning`: the human prefix.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Stop => "stopped",
            Self::Warning => "warning",
        }
    }
}

/// One code of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// `GIT-DIRTY`: upper-case words joined by hyphens.
    pub name: &'static str,
    /// Error, stop or warning.
    pub kind: Kind,
    /// The status results with this code have.
    pub status: CommandStatus,
    /// One line: why it matters (lowercase start, no period).
    pub title: &'static str,
    /// What it means and why it happens.
    pub explain: &'static str,
    /// How to resolve it: commands (`{task}`, `{feature}`, `{command}` are
    /// filled from the result), or `→ ` lines for a person.
    pub fix: &'static [&'static str],
}

/// Where the codes are documented.
pub const DOCS: &str = "https://hupe1980.github.io/speccoder/docs/codes/";

impl Entry {
    /// The code's section on the codes page.
    #[must_use]
    pub fn docs(&self) -> String {
        format!("{DOCS}#{}", self.name.to_ascii_lowercase())
    }
}

use CommandStatus as S;
use Kind::{Error as E, Stop as P, Warning as W};

macro_rules! entry {
    ($name:literal, $kind:expr, $status:expr, $title:literal, $explain:literal, [$($fix:literal),+ $(,)?]) => {
        Entry { name: $name, kind: $kind, status: $status, title: $title, explain: $explain, fix: &[$($fix),+] }
    };
}

/// Every code speccoder uses.
pub const CATALOG: &[Entry] = &[
    // ── usage ─────────────────────────────────────────────────────────
    entry!(
        "USAGE",
        E,
        S::UsageError,
        "the arguments are not valid for this command",
        "The command line could not be parsed: an unknown flag, a missing value, or a value outside the allowed set.",
        ["speccoder {command} --help"]
    ),
    entry!(
        "USAGE-CONFLICT",
        E,
        S::UsageError,
        "two options were given that exclude each other",
        "`--json` prints one result object and `--events` prints a stream of events ending in that object; a command does one or the other.",
        ["speccoder {command} --events", "speccoder {command} --json"]
    ),
    entry!(
        "USAGE-UNKNOWN-CODE",
        E,
        S::UsageError,
        "there is no code by that name",
        "`speccoder explain` knows the codes speccoder itself prints; the closest names are listed with the error.",
        ["speccoder explain --list"]
    ),
    entry!(
        "CALL-CHANGED",
        E,
        S::UsageError,
        "the call waiting now is not the one that was shown",
        "An approval or rejection from an interface names the call it showed and that call's object. The call expired or was decided and another waits now, or the attempt stopped on another object; nothing was decided.",
        ["speccoder report --approvals"]
    ),
    // ── the person's own commands ─────────────────────────────────────
    entry!(
        "FIX-FAILED",
        E,
        S::Error,
        "your own command, run from the app, did not succeed",
        "A fix that is your own command (`npm install`, `git commit …`) was run from the app, as you, in the project folder: it exited with an error, was stopped, or ran past its ten minutes. Its output is in the app's panel. speccoder's own commands never end in this code.",
        [
            "→ read its output in the panel, fix what it names and run it again (or in your shell, to answer what it asks)"
        ]
    ),
    // ── internal ──────────────────────────────────────────────────────
    entry!(
        "INTERNAL",
        E,
        S::Error,
        "speccoder failed in a way it does not expect",
        "An unexpected failure inside speccoder: an IO error, a broken state file, a bug. The message says what failed.",
        ["→ re-run with -v and report the output at https://github.com/hupe1980/speccoder/issues"]
    ),
    // ── project ───────────────────────────────────────────────────────
    entry!(
        "NO-PROJECT",
        E,
        S::NotAProject,
        "speccoder works on a Spec Kit or OpenSpec project, and found none here",
        "No `.specify/` (Spec Kit) or `openspec/` (OpenSpec) directory was found here or above. Spec Kit makes one (`--integration claude` installs the skills speccoder runs); then `speccoder init` and `speccoder spec specify` take it from there.",
        ["specify init --here --integration claude", "openspec init"]
    ),
    entry!(
        "SPEC-DIAGNOSTICS",
        E,
        S::DiagnosticsError,
        "the spec artifacts have errors speccoder will not guess around",
        "The task list or spec could not be read as written: a dependency on a task that does not exist, a cycle, a malformed task line. `inspect` lists every diagnostic with its line.",
        ["speccoder inspect"]
    ),
    entry!(
        "FEATURE-SELECTION",
        E,
        S::SelectionNeeded,
        "more than one feature could be meant",
        "Several features exist and none is current: neither `--feature`, the format's variable nor its pointer (Spec Kit: `.specify/feature.json`) names one. Make one current, or name it for this command.",
        [
            "speccoder feature",
            "speccoder feature <name>",
            "speccoder {command} --feature <name>"
        ]
    ),
    entry!(
        "CHECK-BROKEN",
        E,
        S::ConfigError,
        "a check cannot run on the untouched code",
        "A fast check failed on the untouched worktree without running a test: a command or profile it names is missing, a file it needs was never committed, or it does not build. It could never judge the task, so nothing was attempted.",
        [
            "speccoder doctor",
            "git status",
            "speccoder run --allow-failing-baseline"
        ]
    ),
    entry!(
        "PREPARE-FAILED",
        E,
        S::ConfigError,
        "the worktree's dependencies could not be installed",
        "Before the baseline speccoder installs a Node or uv project's dependencies in the run's worktree with the toolchain's locked, offline install, sandboxed. It failed — a package the lockfile needs is not in the toolchain's cache, or no lockfile is committed — so no check could run and nothing was attempted. Never a failed verdict.",
        [
            "→ run the install once in your checkout to fill the cache (`npm install`, `uv sync`)",
            "→ or allow the network for the install's exact online form: [sandbox] network_allow = [[\"npm\", \"ci\"]]",
            "speccoder doctor"
        ]
    ),
    entry!(
        "SECRET-IN-DIFF",
        P,
        S::NeedsHuman,
        "the attempt's change holds a value copied from your secret files",
        "`[worktree] include` (or `.worktreeinclude`) copied a git-ignored secret file such as `.env` from your checkout into the worktree. The attempt's diff adds one of its values to a file git would commit, so it was undone: nothing was judged, ticked or committed. The variable is named, its value never.",
        [
            "→ the code should read the variable from the environment or the file, never copy its value",
            "→ run the task again; the attempt starts from the committed tree"
        ]
    ),
    entry!(
        "FEATURE-GONE",
        E,
        S::SelectionNeeded,
        "the current feature no longer exists",
        "The format's pointer (Spec Kit: `.specify/feature.json`) or its variable names a feature directory that is not there — renamed, deleted, or from another checkout.",
        [
            "speccoder feature",
            "speccoder feature <name>",
            "speccoder feature --clear"
        ]
    ),
    entry!(
        "REVIEW-PENDING",
        P,
        S::NeedsHuman,
        "the spec is not approved yet",
        "`[review] required` is on: a run waits until every requirement of the spec is approved on its current wording, in the review committed with it (`review.json`). A requirement reworded since its approval needs approving again.",
        ["speccoder review", "speccoder review approve", "git commit"]
    ),
    entry!(
        "REVIEW-AMBIGUOUS",
        P,
        S::NeedsHuman,
        "two readings of a requirement disagree",
        "Two independent test authors read a scenario two ways (the same given, different expected outcomes). Until you choose a reading or reword the requirement, it is not approved, its tests are not frozen, and a gated run (`[review] required`) waits.",
        [
            "speccoder review tests <REQ> --reading A <scenario>",
            "speccoder review tests <REQ> --reading B <scenario>",
            "→ or reword the scenario so it reads one way"
        ]
    ),
    entry!(
        "REVIEW-NO-SCENARIOS",
        E,
        S::UsageError,
        "the requirement has no scenarios to write tests from",
        "Tests are written at review per requirement from its acceptance scenarios. The requirement named has none (or the format yields no requirement by that key), so there is nothing to derive a test from.",
        [
            "speccoder review",
            "→ add acceptance scenarios to the requirement"
        ]
    ),
    entry!(
        "REVIEW-TESTS-OWN-PIPELINE",
        E,
        S::UsageError,
        "this unit's tests are written by its own pipeline",
        "A quick change writes its own tests and a bug's reproductions are confirmed by you; review writes no second set for them.",
        ["speccoder do \"…\"", "speccoder bug confirm <unit>/<task>"]
    ),
    entry!(
        "ACCEPTANCE-OFF",
        E,
        S::ConfigError,
        "acceptance tests are turned off",
        "`[acceptance] generate = false`: no acceptance tests are written, at review or at run time.",
        ["speccoder config set acceptance.generate true"]
    ),
    entry!(
        "ORACLE-AUTHOR-FAILED",
        E,
        S::RunFailed,
        "the test author could not write the tests",
        "The acceptance author call failed or the model provider refused it. Nothing was stored or frozen for the requirement; the message carries the provider's reason.",
        ["speccoder review tests <REQ> --write"]
    ),
    entry!(
        "ORACLE-TAMPERED",
        P,
        S::NeedsHuman,
        "a frozen test no longer matches what was frozen",
        "A test frozen at review is stored in the state directory with its digest. The stored file or its record no longer matches: reusing it would run tests nobody approved, so the task stops and nothing is ticked.",
        [
            "speccoder review tests <REQ>",
            "→ write and approve the tests again, or run without the frozen set"
        ]
    ),
    entry!(
        "ORACLE-FROZEN",
        E,
        S::UsageError,
        "the requirement's tests are frozen with the approval",
        "*Approve and Run* froze these tests and their held-out set, so they cannot be changed. Reword the requirement to lapse the freeze and write them again.",
        [
            "speccoder review",
            "→ reword the requirement to lapse the freeze, then write its tests again"
        ]
    ),
    entry!(
        "REVIEW-CHANGED",
        P,
        S::NeedsHuman,
        "what the approval would freeze changed since it was shown",
        "What the approval sheet showed changed before the click (another tab asked again, a *Write Tests* finished). Nothing was approved; look again and approve what you see.",
        [
            "speccoder review",
            "→ open the sheet again and approve what it shows"
        ]
    ),
    entry!(
        "REVIEW-UNREADABLE",
        P,
        S::NeedsHuman,
        "the review record cannot be read",
        "`review.json` (decisions, frozen tests, approval) or a record beside it is not valid. Reading it as empty would drop every decision and freeze, so speccoder stops until you look.",
        [
            "git diff -- <unit>/review.json",
            "→ restore it from git (`git checkout -- <unit>/review.json`) or fix the JSON"
        ]
    ),
    entry!(
        "RUN-ACTIVE",
        E,
        S::GitPrecondition,
        "a run is active for this project",
        "*Approve and Run* starts a feature run; one is running already (in a terminal or the web app). Approving now would start a second run beside it.",
        ["speccoder report", "→ wait for the run to end, or stop it"]
    ),
    entry!(
        "AUTOPILOT-NEEDS-GRANT-OBJECTS",
        E,
        S::ConfigError,
        "autopilot needs object-bound grants",
        "Autopilot holds only grants the person saw in full and ticked, each bound to one resolved object (a canonical path, an exact argv, an MCP tool digest). This build has no such grants, so autopilot is off.",
        ["speccoder review approve --run ask"]
    ),
    entry!(
        "FEATURE-UNKNOWN",
        E,
        S::SelectionNeeded,
        "no feature has that name or number",
        "`--feature` (or `--change`) names a feature by its directory name, a part of it, or its number in `speccoder feature`'s list; none matches what was given.",
        ["speccoder feature", "speccoder report --feature <name>"]
    ),
    // ── configuration ─────────────────────────────────────────────────
    entry!(
        "CONFIG",
        E,
        S::ConfigError,
        "the configuration or the environment is not usable",
        "`speccoder.toml` or the environment does not allow this command to run; the message names what is wrong.",
        ["speccoder init"]
    ),
    entry!(
        "NO-STATE-DIR",
        E,
        S::ConfigError,
        "speccoder has nowhere to keep its state",
        "Journals, worktrees and live progress live in a state directory: `$SPECCODER_STATE_DIR`, else `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`. None of these variables is set.",
        ["export SPECCODER_STATE_DIR=\"$PWD/.speccoder-state\""]
    ),
    entry!(
        "EXECUTOR-LOGIN",
        P,
        S::NeedsHuman,
        "the external agent needs signing in",
        "The `[executor]` agent answered over ACP that it needs signing in (or failed its session while it offered ways to sign in). Nothing was attempted and the task is unchanged. speccoder never signs in for you and stores no credential: the agent's own login runs in a terminal of yours.",
        [
            "→ run the agent's login in your terminal (the agent names it; for Copilot `copilot login`, for OpenCode `opencode auth login`)",
            "speccoder doctor"
        ]
    ),
    entry!(
        "SANDBOX-UNAVAILABLE",
        E,
        S::ConfigError,
        "the OS sandbox is not available and the project requires it",
        "No OS sandbox is available (Linux without Landlock, a container without user namespaces, another OS) while `[sandbox] mode = \"required\"` or an autopilot run needs one.",
        [
            "→ run on macOS or Linux 6.2+ (Landlock ABI 3)",
            "→ or set [sandbox] mode = \"auto\" to run unsandboxed and be told",
            "→ autopilot: choose Ask on Risky Calls, or enable the OS sandbox"
        ]
    ),
    entry!(
        "GITHUB-AUTH",
        E,
        S::Error,
        "not signed in to GitHub",
        "No GitHub token was found (GH_TOKEN, GITHUB_TOKEN, or the one `speccoder auth login` keeps in your keychain), or GitHub refused it.",
        [
            "speccoder auth login --with-token",
            "speccoder auth status",
            "→ or set GH_TOKEN (a fine-grained token: Issues read; Pull requests read and write; Contents and Metadata read)"
        ]
    ),
    entry!(
        "GITHUB-FAILED",
        E,
        S::Error,
        "the GitHub API could not do it",
        "GitHub answered, and not as asked: no such issue, no access for the token, a rate limit used up, or GitHub did not answer in time. The message is GitHub's, in words.",
        ["speccoder auth status", "git remote -v"]
    ),
    entry!(
        "CONFIG-EXISTS",
        E,
        S::ConfigError,
        "the project has a speccoder.toml already",
        "`init` writes a new one; change the existing one with `speccoder config` (or Settings), or start over with `--force`.",
        ["speccoder config", "speccoder init --force"]
    ),
    entry!(
        "CONFIG-MISSING",
        E,
        S::ConfigError,
        "the project has no speccoder.toml",
        "speccoder needs to know which checks decide \"nothing regressed\" and which model to use; `init` detects them and writes a commented file.",
        ["speccoder init"]
    ),
    entry!(
        "CONFIG-INVALID",
        E,
        S::ConfigError,
        "speccoder.toml has an error",
        "An unknown section or key, a wrong type, or a value outside the allowed set. Unknown keys are errors so a typo never goes unnoticed.",
        ["→ fix the line the message names in speccoder.toml, then commit it"]
    ),
    entry!(
        "KEY-MISSING",
        E,
        S::ConfigError,
        "the model provider's API key is not in the environment",
        "speccoder reads API keys from the environment only. The message names the variable the configured provider needs. For Amazon Bedrock, doctor and the app report it when they find neither AWS_BEARER_TOKEN_BEDROCK nor an AWS profile (aws login); a run still tries the AWS credential chain, which may find an instance role.",
        ["→ export the variable the message names, e.g. export ANTHROPIC_API_KEY=…"]
    ),
    entry!(
        "NO-CHECKS",
        E,
        S::ConfigError,
        "no check applies, so nothing could show that nothing regressed",
        "Every configured check was skipped (`when_exists` did not match) or none is configured. A task cannot be judged without at least one check: add your test command as one.",
        ["speccoder config set checks.test.argv -- <your test command>"]
    ),
    // ── git ───────────────────────────────────────────────────────────
    entry!(
        "GIT",
        E,
        S::GitPrecondition,
        "git is not in the state this command needs",
        "A git precondition does not hold; the message names it.",
        ["git status"]
    ),
    entry!(
        "SETUP-INCOMPLETE",
        E,
        S::GitPrecondition,
        "the project's setup is not committed, so every run would trip over it",
        "A worktree holds only commits: a file the checks need but the checkout never committed (`speccoder.toml`, `.config/nextest.toml`), a lockfile the checks write, or build output `.gitignore` does not list. `speccoder fix` commits that setup once.",
        ["speccoder fix"]
    ),
    entry!(
        "NO-REPOSITORY",
        E,
        S::GitPrecondition,
        "speccoder needs a git repository with at least one commit",
        "Evidence lives in commits and refs, and work happens on a branch of its own, so the project must be a git repository with a first commit.",
        ["git init && git add -A && git commit -m init"]
    ),
    entry!(
        "UNCOMMITTED-INTENT",
        E,
        S::GitPrecondition,
        "the spec has uncommitted changes the run would not see",
        "speccoder reads committed files only — a worktree is checked out from a commit — so an uncommitted edit to the spec, plan or tasks would silently not count.",
        ["git status", "speccoder run --in-place"]
    ),
    entry!(
        "NO-REMOTE",
        E,
        S::GitPrecondition,
        "there is no git remote to push to",
        "`pr` pushes `speccoder/<feature>` to a remote and opens the pull request there; `issue` reads the remote's GitHub issues. Add the repository's GitHub address as a remote, or name the one to use.",
        [
            "git remote add origin <url>",
            "git remote -v",
            "speccoder pr --remote <name>"
        ]
    ),
    entry!(
        "WORKTREE-DIRTY",
        E,
        S::GitPrecondition,
        "the feature's worktree has uncommitted changes",
        "Work left in the worktree outside a commit — a failed attempt's, or an edit by hand — would be mixed into what comes next.",
        ["speccoder worktree", "speccoder run --feature {feature}"]
    ),
    entry!(
        "NOTHING-TO-HAND-OFF",
        E,
        S::GitPrecondition,
        "the speccoder branch has nothing the base does not have",
        "`pr` hands off work on `speccoder/<feature>`; there is none yet, or it is already merged.",
        ["speccoder run --feature {feature}"]
    ),
    // in place
    entry!(
        "DIRTY-CHECKOUT",
        E,
        S::GitPrecondition,
        "an in-place run needs a clean checkout",
        "In place, the attempt runs in your checkout; uncommitted changes would be judged as the agent's.",
        ["git stash -u", "git commit -am wip"]
    ),
    entry!(
        "UNBORN-HEAD",
        E,
        S::GitPrecondition,
        "the checkout has no commit yet",
        "An in-place run branches from HEAD; there is no HEAD to branch from.",
        ["git add -A && git commit -m init"]
    ),
    entry!(
        "CONFIG-NOT-TRACKED",
        E,
        S::GitPrecondition,
        "speccoder.toml must be committed",
        "The judge's configuration is a reviewed input; an untracked or modified one could be changed without review.",
        ["git add speccoder.toml && git commit -m \"speccoder: config\""]
    ),
    entry!(
        "BRANCH-IN-WORKTREE",
        E,
        S::GitPrecondition,
        "the feature's branch is checked out in a worktree",
        "git refuses to check out a branch twice; an in-place run needs `speccoder/<feature>` free.",
        ["git worktree list", "git worktree remove <path>"]
    ),
    entry!(
        "IN-PLACE-ACTIVE",
        E,
        S::GitPrecondition,
        "an in-place run holds this checkout",
        "Another in-place session is active (or was killed); it owns the checkout until it ends or is left.",
        [
            "speccoder run --in-place",
            "speccoder run --in-place --return"
        ]
    ),
    entry!(
        "IN-PLACE-SESSION",
        E,
        S::GitPrecondition,
        "an in-place session is recorded for this checkout",
        "A worktree-mode command would not see what the in-place session holds.",
        [
            "speccoder run --in-place",
            "speccoder run --in-place --return"
        ]
    ),
    entry!(
        "IN-PLACE-PARALLEL",
        E,
        S::UsageError,
        "an in-place run works one task at a time",
        "Parallel lanes need worktrees of their own; in place there is only your checkout.",
        ["speccoder run --in-place", "speccoder run --parallel 2"]
    ),
    entry!(
        "MERGE-CONFLICT",
        E,
        S::GitPrecondition,
        "the checkout is in the middle of a merge or rebase",
        "An in-place run cannot start or return while git waits for a conflict to be resolved.",
        ["git status", "git merge --abort"]
    ),
    entry!(
        "INTENT-CHANGED",
        P,
        S::NeedsHuman,
        "the spec, plan or tasks changed during the run",
        "Intent files are hashed when an in-place run starts and compared before every task; a change means the work would be judged against a spec it was not given.",
        ["git diff", "speccoder run --in-place"]
    ),
    entry!(
        "JUDGE-CHANGED",
        P,
        S::NeedsHuman,
        "the judge's inputs changed during the run",
        "Checks, test configuration or agent configuration changed after the run started; a verdict under changed rules would mean nothing.",
        ["git diff", "speccoder run --in-place"]
    ),
    entry!(
        "CHECKOUT-CHANGED",
        P,
        S::NeedsHuman,
        "the checkout moved under the run",
        "HEAD, the branch or the tree is not where speccoder left it; someone or something else changed the checkout.",
        ["git status", "speccoder run --in-place --return"]
    ),
    entry!(
        "GIT-CHANGED",
        P,
        S::NeedsHuman,
        "git's configuration, hooks or refs changed during the run",
        "Where the sandbox cannot keep `.git` read-only, speccoder fingerprints it; a change stops the run before the next tick.",
        [
            "git config --list --show-origin",
            "speccoder run --in-place --return"
        ]
    ),
    entry!(
        "RETURN-BLOCKED",
        P,
        S::NeedsHuman,
        "the in-place run cannot switch back to your branch",
        "Switching back would overwrite changes or fail; the session stays until the checkout allows it.",
        ["git status", "speccoder run --in-place --return"]
    ),
    // ── tasks ─────────────────────────────────────────────────────────
    entry!(
        "NOT-FOUND",
        E,
        S::NotFound,
        "nothing by that name exists",
        "No task, run, call or object matches what was asked for; the message names it.",
        ["speccoder report"]
    ),
    entry!(
        "NOT-FOUND-SCHEMA",
        E,
        S::NotFound,
        "there is no schema by that name",
        "`speccoder schema` lists the schemas speccoder publishes.",
        ["speccoder schema"]
    ),
    entry!(
        "TASK-NOT-READY",
        E,
        S::TaskNotReady,
        "no task can run now",
        "The task is done, or waits for tasks it depends on.",
        ["speccoder report"]
    ),
    entry!(
        "NO-TASKS",
        E,
        S::TaskNotReady,
        "the feature has no tasks yet",
        "Its task list is not written yet, or lists no task: nothing ran, and the feature is not done. The spec format's planning skills write the tasks; `speccoder report` names the next one.",
        ["speccoder report"]
    ),
    entry!(
        "TASK-DONE",
        E,
        S::TaskNotReady,
        "the task is already ticked",
        "A ticked task is not run again; its evidence is in its commit.",
        ["speccoder report"]
    ),
    entry!(
        "TASK-BLOCKED",
        E,
        S::TaskNotReady,
        "the task waits for tasks it depends on",
        "Dependencies come from the task list's phases, `[P]` markers and explicit references; the message names the open ones.",
        [
            "speccoder run --feature {feature}",
            "speccoder task skip <dependency> --reason \"…\""
        ]
    ),
    // ── verdicts ──────────────────────────────────────────────────────
    entry!(
        "VERIFICATION-FAILED",
        E,
        S::VerificationFailed,
        "the checks or the acceptance tests did not pass",
        "After the retries, a check regressed or the spec-derived acceptance tests did not go from red to green. The attempt is saved under its ref.",
        [
            "speccoder report",
            "speccoder task nudge {task} --note \"…\""
        ]
    ),
    entry!(
        "REGRESSION-ONLY",
        W,
        S::Ok,
        "ticked on the regression checks alone, nothing task-specific was shown",
        "The checks still pass, but no acceptance test showed this task's behaviour. It is ticked `regression_only`, never `verified`, with one recorded reason: `no_story` (no user story to derive a test from), `no_scenarios` (its story states no acceptance scenario), `acceptance_off` (`[acceptance] generate = false`), `author_wrote_none` (the test author wrote no test), `author_failed` (its tests could not be used), `already_passing` (they passed before the attempt — with the task that implemented it), `not_red` (they did not fail on an assertion first), `tests_not_collected` (the command collected no test) or `planning_work` (a planning task: design documents, not code).",
        [
            "→ add an acceptance example to the task's story, then converge",
            "speccoder converge --feature {feature}"
        ]
    ),
    entry!(
        "ATTESTATION-FAILED",
        E,
        S::AttestationFailed,
        "an attestation is missing, unsigned where required, or does not hold",
        "`attest verify` found a tick whose signed statements are missing, malformed, edited, about another commit, or signed by no one you trust.",
        ["speccoder attest verify --feature {feature} --json"]
    ),
    entry!(
        "TESTS-NOT-COLLECTED",
        E,
        S::VerificationFailed,
        "a test file the attempt wrote never ran",
        "The attempt added or changed a test file, but the runner collected no test from it: the toolchain's rule says it is never built or picked up (a Rust file no target declares, a pytest file outside `python_files`, a vitest file outside `include`, a Go file under `testdata/`), or the run's report and listing hold none of its tests. A test that never ran is evidence of nothing, so the verdict fails and the retry is told the rule.",
        [
            "speccoder report",
            "→ move the tests where the runner collects them (the reason names the rule)"
        ]
    ),
    entry!(
        "PLAN-TESTS-UNCOLLECTED",
        W,
        S::Ok,
        "the plan puts tests where the runner won't collect them",
        "A task names a test path the project's runner would not collect (Cargo builds only `tests/*.rs` and `tests/<dir>/main.rs`), and no task creates the file that would declare it; or a test file in the tree is collected by nothing. Tests written there would never run. A warning: the run is not stopped.",
        [
            "speccoder doctor --feature {feature}",
            "speccoder spec tasks"
        ]
    ),
    entry!(
        "PLAN-BEHAVIOUR-EARLY",
        W,
        S::Ok,
        "a story's behaviour is planned before the story",
        "A Setup or Foundational task names a symbol a user story is about. If that task implements it, the story's acceptance tests already pass when the story's tasks run: they cannot go red, and the tasks tick regression-only. Write the story's acceptance tests at that task, or re-plan.",
        [
            "speccoder task acceptance-at {task} --story <US>",
            "speccoder spec tasks"
        ]
    ),
    entry!(
        "PLAN-TEST-ONLY-TASKS",
        W,
        S::Ok,
        "tasks that only write tests",
        "A story has tasks whose only outputs are test files next to the task that implements it. speccoder writes each story's acceptance tests itself and judges the implementing task by them; a test-only task has nothing of its own to show and ticks regression-only.",
        ["speccoder spec tasks"]
    ),
    entry!(
        "PRESET-NOT-SPECKIT",
        E,
        S::UsageError,
        "presets are a Spec Kit mechanism",
        "`speccoder preset add` installs the speccoder preset through Spec Kit's own `specify preset add`; this project is not a Spec Kit project, so there is nothing to add it to.",
        ["speccoder doctor"]
    ),
    entry!(
        "PRESET-SPECKIT-MISSING",
        E,
        S::ConfigError,
        "the Spec Kit tool `specify` is missing or too old",
        "Adding the preset runs `specify preset add`, which needs `specify` at the version the preset requires. It is not installed, or older.",
        [
            "→ install or upgrade Spec Kit (`uv tool install specify-cli`)",
            "speccoder doctor"
        ]
    ),
    entry!(
        "PRESET-FAILED",
        E,
        S::Error,
        "the preset could not be added: `specify preset add` failed",
        "Spec Kit refused or failed to install the preset; its own output is shown. Nothing in `specs/` was changed.",
        ["speccoder preset add", "speccoder doctor"]
    ),
    // ── a new project, the sample ─────────────────────────────────────
    entry!(
        "NEW-PLAN-CHANGED",
        E,
        S::UsageError,
        "the plan changed since you saw it; nothing ran",
        "Creating a project runs exactly the plan you confirmed, bound to its digest: the commands, their order and the folder as it was seen. A tool's version or the folder's contents changed meanwhile, so the plan computed now differs and nothing was run. The result carries the new plan.",
        ["→ review the new plan and confirm it again"]
    ),
    entry!(
        "NEW-TARGET-NOT-EMPTY",
        E,
        S::UsageError,
        "the folder is not empty and not a git repository",
        "A new project starts in an empty folder, or in a git repository at its top level with a clean tree. This folder holds files that are not under git, which the format's `--force` init could overwrite unseen.",
        [
            "→ choose an empty folder",
            "→ or run `git init` there and commit what it holds first"
        ]
    ),
    entry!(
        "NEW-DIRTY",
        E,
        S::GitPrecondition,
        "the repository has uncommitted changes",
        "speccoder commits exactly what the plan's steps produced. With changes of yours in the tree, the two could not be told apart, so the folder must be clean first.",
        ["→ commit or stash your changes, then try again"]
    ),
    entry!(
        "NEW-NESTED-REPOSITORY",
        E,
        S::GitPrecondition,
        "the folder is inside another repository",
        "A project is created at a repository's top level. This folder lies inside another repository's worktree; a second repository inside it, or a project in its subfolder, would split one history.",
        ["→ choose a folder outside it, or use the repository's top level"]
    ),
    entry!(
        "NEW-ALREADY-PROJECT",
        E,
        S::UsageError,
        "the folder already holds a spec project",
        "There is nothing to create: the folder is a project of a spec format already. Open it instead.",
        ["speccoder ui", "speccoder init"]
    ),
    entry!(
        "NEW-TOOL-MISSING",
        E,
        S::ConfigError,
        "a tool the plan needs is not installed",
        "The plan runs the ecosystems' own tools — the stack's scaffold and the spec format's init. One of them is not on the PATH; the message names it and how to install it.",
        [
            "→ install the tool, or choose another stack or format",
            "speccoder doctor"
        ]
    ),
    entry!(
        "NEW-STEP-FAILED",
        E,
        S::Error,
        "a step of the plan failed; later steps did not run",
        "A command of the plan exited with an error, timed out or was denied by the sandbox. The steps before it are kept; running the same plan again resumes at this step.",
        ["→ retry, or run the command yourself as shown"]
    ),
    entry!(
        "NEW-NO-IDENTITY",
        E,
        S::GitPrecondition,
        "git does not know who you are",
        "The new project's one commit is yours, made with your git identity; speccoder never invents one. `user.name` or `user.email` is not set.",
        [
            "→ git config --global user.name \"Your Name\"",
            "→ git config --global user.email you@example.com"
        ]
    ),
    entry!(
        "NEW-IN-PROGRESS",
        E,
        S::UsageError,
        "a new project is being created in this folder",
        "Another creation of a project in the same folder is running. Two at once would interleave their steps.",
        ["→ wait for it to finish"]
    ),
    entry!(
        "NEW-UNDO-REFUSED",
        E,
        S::GitPrecondition,
        "the project changed since speccoder created it; nothing was undone",
        "Undo moves a folder speccoder created to the Trash, or reverts speccoder's commit in an existing repository — only while nothing but that commit exists. Something changed since (the message lists what), or there is no Trash on this system; nothing was moved or deleted.",
        ["→ undo it yourself; nothing was moved"]
    ),
    entry!(
        "SAMPLE-DIVERGED",
        E,
        S::ReplayDiverged,
        "this build cannot reproduce the sample's recorded run",
        "The sample is a recorded run replayed strictly. This build replayed it to a different result, so it is not shown as if it had worked: the board shows the sample's committed state only.",
        ["speccoder doctor --bug-report"]
    ),
    entry!(
        "SAMPLE-MISSING",
        E,
        S::NotFound,
        "this build has no sample",
        "The sample's recording is not part of this build: release builds bundle it; a build from source has it only when `crates/speccoder-runtime/assets/sample/` holds one (`just record-sample`).",
        ["→ install a release build", "speccoder new"]
    ),
    entry!(
        "SIMILAR-FEATURE",
        W,
        S::Ok,
        "an existing requirement is close to this description",
        "Before the spec is written, the description is compared with every feature's requirements by a deterministic text similarity. A close match may mean the change belongs in that feature; it is a hint and never stops the skill.",
        [
            "speccoder feature similar \"…\"",
            "speccoder review comments"
        ]
    ),
    // ── stops ─────────────────────────────────────────────────────────
    entry!(
        "NEW-CONFIRM",
        P,
        S::NeedsHuman,
        "the plan needs your confirmation; nothing ran",
        "`speccoder new` runs commands in a folder; it runs them only after you saw them. Without a terminal to ask at, it printed the plan and stopped.",
        ["→ run the same command again with --yes"]
    ),
    entry!(
        "THROUGH-QUESTIONS",
        P,
        S::NeedsHuman,
        "the spec has open questions; planning waits for your answers",
        "The format's skill wrote the spec and left questions it could not decide. Planning on a guess would plan the wrong thing, so the chain stops until each is answered.",
        ["speccoder review", "speccoder review answer Q1 \"…\""]
    ),
    entry!(
        "THROUGH-REVIEW",
        P,
        S::NeedsHuman,
        "review the spec; planning waits for your approval",
        "Plan through stops after the spec is written: its requirements are what every task will be judged by, so they are planned only once you approved them. No flag passes this stop.",
        ["speccoder review", "speccoder review approve"]
    ),
    entry!(
        "NEEDS-HUMAN",
        P,
        S::NeedsHuman,
        "the run stopped for a person",
        "The run cannot go on without a decision; the message says what waits.",
        ["speccoder report"]
    ),
    entry!(
        "NEEDS-APPROVAL",
        P,
        S::NeedsHuman,
        "a call waits for a person's decision; nothing ran",
        "The model asked for something the policy does not allow on its own — a dependency change, the network, a protected file. The attempt waits; deciding continues it from that call.",
        [
            "speccoder report --approvals",
            "speccoder task approve {task}",
            "speccoder task reject {task} --reason \"…\""
        ]
    ),
    entry!(
        "BLOCKED",
        P,
        S::NeedsHuman,
        "the model says the task cannot be done as written",
        "The model used its honest exit: something in the spec or the code stops it — often a step only a person can take. Its reason is a claim, shown as \"Model says\". The task is yours now: say you did it, give a hint and retry, or skip it.",
        [
            "speccoder task done {task}",
            "speccoder task hint {task} \"…\"",
            "speccoder task skip {task} --reason \"…\""
        ]
    ),
    entry!(
        "TASK-YOURS",
        P,
        S::NeedsHuman,
        "the next task is a step only a person can take",
        "Its line carries `[YOU]`: the run does not attempt it. Do it, then say so; or give a hint and let the agent try; or skip it.",
        [
            "speccoder task done {task}",
            "speccoder task hint {task} \"…\"",
            "speccoder task skip {task} --reason \"…\""
        ]
    ),
    entry!(
        "ATTEMPT-INCOMPLETE",
        P,
        S::NeedsHuman,
        "the model says the task is not finished",
        "Its closing report says part of the task is left undone; so the attempt is not ticked. The stop quotes what the model says is missing (its claim); the work so far is saved under the attempt's ref.",
        [
            "speccoder task nudge {task} --note \"…\"",
            "speccoder run --task {task}"
        ]
    ),
    entry!(
        "BUDGET-EXHAUSTED",
        P,
        S::NeedsHuman,
        "the attempt ran out of turns, calls or tokens",
        "The attempt reached a limit of `[budget]` before finishing. The work so far is saved under the attempt's ref.",
        [
            "speccoder task nudge {task} --note \"…\"",
            "→ raise the limit in speccoder.toml [budget]"
        ]
    ),
    entry!(
        "NOT-DEMONSTRATED",
        P,
        S::NeedsHuman,
        "the evidence the task needs could not be produced",
        "The acceptance tests could not be made red before the attempt (they already pass, or fail for another reason), or a convergence gap shows in no failing test. Ticking would claim more than was shown.",
        [
            "speccoder report",
            "speccoder task accept {task} --reason \"…\""
        ]
    ),
    entry!(
        "TESTS-COLLECTION-UNKNOWN",
        P,
        S::NeedsHuman,
        "speccoder cannot tell whether the runner ran a test file",
        "The attempt changed a test file, and neither the toolchain's static rule, the check's JUnit report nor the runner's listing says whether a test from it ran (an unrecognised runner, a listing that failed or timed out, no report). A tick on tests that may not have run would claim more than was shown, so the task stops; the attempt is saved under its ref. It is not retried: the model cannot fix it.",
        [
            "→ declare a JUnit report for the check (`junit = \"…\"`)",
            "→ set the check's `list` command in speccoder.toml",
            "speccoder task accept {task} --reason \"…\""
        ]
    ),
    entry!(
        "ACCEPTANCE-UNWRITTEN",
        P,
        S::NeedsHuman,
        "the acceptance tests could not be written, so nothing was attempted",
        "The call that writes acceptance tests from the spec failed (no credits, a rejected key, a broken run). Without them the task would be judged on regression checks only, so it stops.",
        [
            "speccoder run --feature {feature}",
            "→ or set [acceptance] generate = false to rely on the checks alone"
        ]
    ),
    entry!(
        "SUSPECTED-GAMING",
        P,
        S::NeedsHuman,
        "the gaming monitor suspects a shortcut",
        "A separate review of the passing attempt found something like a hard-coded result or a weakened test. The task stays unticked until a person decides.",
        [
            "speccoder report",
            "speccoder task accept {task} --reason \"…\"",
            "speccoder task nudge {task} --note \"…\""
        ]
    ),
    entry!(
        "TESTS-OVER-STRICT",
        P,
        S::NeedsHuman,
        "an acceptance test rejects an implementation the spec appears to allow",
        "An alternative the spec allows (another order, wording, tie-break) failed the acceptance tests: they may encode a choice the spec did not make. The attempt is held until you rule; if the tests are right, the next run ticks it without a model call.",
        [
            "speccoder task rule {task} --tests-right --reason \"…\"",
            "speccoder run --task {task} --fresh-acceptance",
            "speccoder task accept {task} --reason \"…\""
        ]
    ),
    entry!(
        "ACCEPTANCE-WEAK",
        P,
        S::NeedsHuman,
        "the acceptance tests are too weak for [acceptance] min_strength",
        "Wrong implementations — mutants of the changed code and model-written near misses — passed the acceptance tests too often, and adding tests once did not fix it. The task stays unticked; the report names what got through.",
        [
            "speccoder report",
            "speccoder run --task {task}",
            "speccoder task accept {task} --reason \"…\""
        ]
    ),
    entry!(
        "INTERRUPTED",
        P,
        S::NeedsHuman,
        "the run was interrupted",
        "The process was stopped or killed; the journal holds what finished. The next run resumes without repeating finished calls.",
        ["speccoder run --feature {feature}"]
    ),
    entry!(
        "CHECKLISTS-OPEN",
        P,
        S::NeedsHuman,
        "the feature's checklists have unchecked items",
        "Spec Kit checklists gate implementation; nothing ran.",
        [
            "→ finish the checklists in the feature's checklists/ directory",
            "speccoder run --ignore-checklists"
        ]
    ),
    entry!(
        "ARCHIVE-AWAITING",
        P,
        S::NeedsHuman,
        "the OpenSpec archive waits for your yes",
        "The diff of the archive is shown; nothing is written until a person confirms.",
        ["speccoder spec archive --yes"]
    ),
    // ── runs ──────────────────────────────────────────────────────────
    entry!(
        "RUN-FAILED",
        E,
        S::RunFailed,
        "the model call or the run failed",
        "The provider returned an error, the journal could not be written, or the run broke; the message names it. Nothing was ticked.",
        ["speccoder run --feature {feature} -v"]
    ),
    entry!(
        "PROVIDER-REFUSED",
        E,
        S::RunFailed,
        "the model provider refused the request",
        "No credits, a rejected or revoked key, or an account limit. speccoder stops instead of judging the task on less evidence; for Amazon Bedrock, when access is denied (an invalid API key, say), the first fix renews the credential source.",
        [
            "→ check the provider account's credits and the API key",
            "speccoder run --feature {feature}"
        ]
    ),
    entry!(
        "REPLAY-DIVERGED",
        E,
        S::ReplayDiverged,
        "the replay did not reproduce the recorded run",
        "Strict replay re-derives a run from its journal without a model; a difference means the code's decisions changed.",
        ["speccoder replay <run> --json"]
    ),
    entry!(
        "NOT-REPLAYABLE",
        E,
        S::NotReplayable,
        "this build cannot re-derive the recorded run",
        "The run was recorded by a build whose skills or effects this one does not have.",
        ["→ replay with the speccoder version that recorded the run"]
    ),
    // ── bug fixes ─────────────────────────────────────────────────────
    entry!(
        "BUG-NO-UNIT",
        E,
        S::UsageError,
        "this spec format has no place for a bug",
        "A bug lives in the format's own bug structure (Spec Kit's `bug` extension) or in its standing bug-fix unit (Spec Kit's `NNN-bugfixes`, an OpenSpec standing change). Kiro projects are not supported: nothing is written.",
        [
            "→ write a spec for the change and run it",
            "→ a change without a reproduction: speccoder do \"…\""
        ]
    ),
    entry!(
        "BUG-EMPTY",
        E,
        S::UsageError,
        "the symptom and every statement need words",
        "`speccoder bug` records the person's own words: an empty symptom, `--expected` or `--unchanged` is refused before anything is written.",
        ["speccoder bug \"<what happens>\" --expected \"<what should>\""]
    ),
    entry!(
        "BUG-NO-EXPECTED",
        E,
        S::UsageError,
        "a bug needs at least one --expected statement",
        "A reproduction asserts what should happen; speccoder never infers it from the symptom or an issue. Give one `--expected` per behaviour.",
        ["speccoder bug \"<what happens>\" --expected \"<what should>\""]
    ),
    entry!(
        "BUG-ASSESSMENT-DRIFT",
        P,
        S::NeedsHuman,
        "the bug's home does not carry exactly your words",
        "The bug's file (`assessment.md`, or the standing unit) does not read back exactly your Current, Expected and Unchanged statements: one is missing, added, reworded, or changed by the file's markup. Reproductions are written from your words only, so nothing was reproduced.",
        [
            "→ edit the assessment so it carries each statement as a `Current:` / `Expected:` / `Unchanged:` line",
            "speccoder bug --slug <slug>"
        ]
    ),
    entry!(
        "BUG-ASSESSMENT-UNREADABLE",
        P,
        S::NeedsHuman,
        "the bug's assessment is not in a shape speccoder reads",
        "speccoder reads `assessment.md` in the layout of the `bug` extension version it was built against, and refuses rather than guesses on any other shape.",
        [
            "specify extension list --json",
            "→ add `Current:` and `Expected:` lines to the assessment's Report section"
        ]
    ),
    entry!(
        "BUG-TEST-NOT-FOUND",
        E,
        S::UsageError,
        "the test named with --test is not found",
        "`--test PATH::NAME` must name an existing test file and a test in it that the runner collects.",
        ["→ give the test as PATH::NAME, as the runner lists it"]
    ),
    entry!(
        "BUG-TEST-NOT-RED",
        E,
        S::UsageError,
        "the named test does not fail on an assertion",
        "A reproduction must fail on the untouched code by an assertion, twice. A build or collection error, a timeout, an absent symbol or a flaky failure shows nothing about the bug.",
        ["→ fix the test so it fails on an assertion, then run `speccoder bug` again"]
    ),
    entry!(
        "BUG-TEST-PASSES",
        E,
        S::UsageError,
        "the named test passes on the untouched code",
        "A test that passes before any fix cannot show the bug. Perhaps it is fixed already.",
        ["→ check the behaviour by hand; if it is fixed, there is nothing to record"]
    ),
    entry!(
        "BUG-TEST-COVERAGE",
        E,
        S::UsageError,
        "each --expected statement needs its own --test",
        "Each stated behaviour gets its own reproduction: repeat `--test` once per `--expected`, in the same order.",
        [
            "speccoder bug \"…\" --expected \"<e1>\" --test <file>::<test1> --expected \"<e2>\" --test <file>::<test2>"
        ]
    ),
    entry!(
        "BUG-CANNOT-REPRODUCE",
        P,
        S::NeedsHuman,
        "the bug could not be reproduced: nothing is fixed",
        "No try produced a test for each Expected statement that fails on its own assertion, or you rejected them (rejected tries count against `[bug] reproduce_tries`). A fix without a failing reproduction would be a guess, so the bug comes back to you with what each try showed.",
        [
            "speccoder bug run --task {feature}/{task}",
            "speccoder bug \"…\" --expected \"…\" --test <file>::<test>",
            "speccoder task drop {task}"
        ]
    ),
    entry!(
        "BUG-ALREADY-SATISFIED",
        W,
        S::Ok,
        "the reproductions already pass: nothing to fix",
        "Every reproduction passed on the untouched code on every try. Nothing is ticked; keep the reproductions as a regression test (committed, the task attested) or drop the task.",
        [
            "speccoder bug keep {feature}/{task} --reason \"kept as a regression test\"",
            "speccoder task drop {task}"
        ]
    ),
    entry!(
        "BUG-UNCHANGED-NOT-HOLDING",
        P,
        S::NeedsHuman,
        "an Unchanged statement does not hold today",
        "The test of an Unchanged statement fails on the untouched code, so the statement is not true now and cannot guard the fix. The bug stops before any fix.",
        ["→ reword or remove the statement, then `speccoder bug run`"]
    ),
    entry!(
        "BUG-NOT-DISCRIMINATING",
        E,
        S::VerificationFailed,
        "the reproduction does not tell the fix from no fix",
        "With the attempt's source changes undone (its tests kept), no reproduction failed on an assertion: passing them proves nothing about the fix. The retry is told which statement.",
        ["speccoder run --task {task}"]
    ),
    entry!(
        "BUG-REPRO-WEAK",
        P,
        S::NeedsHuman,
        "a fix that special-cases the reproduction's input passes",
        "A candidate fix that only recognises the reproduction's literal inputs passed the reproductions and the held-out variants, even after one variant was written again. The tests are too weak for a verified fix.",
        [
            "speccoder task rule {task} --tests-right --reason \"…\"",
            "speccoder bug \"…\" --expected \"…\" --test <file>::<test>"
        ]
    ),
    entry!(
        "BUG-CANDIDATE-APPLY",
        E,
        S::GitPrecondition,
        "the candidate patch is not applied",
        "`--candidate` applies the patch on the untouched code with `git apply --check` first; some hunks did not apply, git could not read it as a patch, or it carries a binary hunk or a symbolic link (neither is judged as a candidate).",
        ["→ rebase the patch on HEAD, as a text diff without binary hunks or links"]
    ),
    entry!(
        "BUG-CANDIDATE-JUDGE",
        E,
        S::UsageError,
        "the candidate patch touches what judges it",
        "A candidate must not change a frozen reproduction, a held-out path or another judge input (checks, configuration, the intent) — by an edit, a rename, a deletion or a mode change, as git itself reads the patch. It is refused before it is applied.",
        ["→ remove those hunks from the patch"]
    ),
    entry!(
        "EVAL-FAILED",
        E,
        S::EvalFailed,
        "the evaluation found a false completion or a cheat",
        "A case ended done although its hidden oracle fails, or the attempt gamed its tests.",
        ["speccoder eval --keep <dir>"]
    ),
];

/// The catalog entry named `name`.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.name == name)
}

/// The code a non-ok status gets when nothing more specific is known.
#[must_use]
pub fn default_for(status: CommandStatus) -> Option<&'static Entry> {
    let name = match status {
        S::Ok => return None,
        S::UsageError => "USAGE",
        S::Error => "INTERNAL",
        S::NotAProject => "NO-PROJECT",
        S::DiagnosticsError => "SPEC-DIAGNOSTICS",
        S::SelectionNeeded => "FEATURE-SELECTION",
        S::ConfigError => "CONFIG",
        S::VerificationFailed => "VERIFICATION-FAILED",
        S::GitPrecondition => "GIT",
        S::NotFound => "NOT-FOUND",
        S::RunFailed => "RUN-FAILED",
        S::ReplayDiverged => "REPLAY-DIVERGED",
        S::TaskNotReady => "TASK-NOT-READY",
        S::NeedsHuman => "NEEDS-HUMAN",
        S::EvalFailed => "EVAL-FAILED",
        S::NotReplayable => "NOT-REPLAYABLE",
        S::AttestationFailed => "ATTESTATION-FAILED",
    };
    lookup(name)
}

/// Up to `n` catalog names closest to `name` (case-insensitive edit distance).
#[must_use]
pub fn nearest(name: &str, n: usize) -> Vec<&'static str> {
    let wanted = name.to_ascii_uppercase();
    let mut scored: Vec<(usize, &'static str)> = CATALOG
        .iter()
        .map(|e| (distance(&wanted, e.name), e.name))
        .collect();
    scored.sort();
    scored
        .into_iter()
        .filter(|(d, _)| *d <= wanted.len().max(3) / 2 + 1)
        .take(n)
        .map(|(_, name)| name)
        .collect()
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// `entry`'s fixes with `{task}`, `{feature}` and `{command}` filled where
/// known; a fix whose placeholder is unknown keeps `<task>`-style blanks.
#[must_use]
pub fn fixes(
    entry: &Entry,
    command: &str,
    task: Option<&str>,
    feature: Option<&str>,
) -> Vec<String> {
    // Without a task, a fix about one is no step to take — unless every fix
    // is about one (then the placeholder shows what to fill in).
    let general = entry.fix.iter().any(|f| !f.contains("{task}"));
    entry
        .fix
        .iter()
        .filter(|f| task.is_some() || !general || !f.contains("{task}"))
        .map(|f| {
            let feature_arg = feature.map_or_else(String::new, |f| f.to_owned());
            let mut line = f
                .replace("{command}", command)
                .replace("{task}", task.unwrap_or("<task>"));
            line = if feature.is_some() {
                line.replace("{feature}", &feature_arg)
            } else {
                // Without a selected feature, the flag is not needed.
                line.replace(" --feature {feature}", "")
                    .replace("{feature}", "<feature>")
            };
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUSES: [CommandStatus; 17] = [
        S::Ok,
        S::UsageError,
        S::Error,
        S::NotAProject,
        S::DiagnosticsError,
        S::SelectionNeeded,
        S::ConfigError,
        S::VerificationFailed,
        S::GitPrecondition,
        S::NotFound,
        S::RunFailed,
        S::ReplayDiverged,
        S::TaskNotReady,
        S::NeedsHuman,
        S::EvalFailed,
        S::NotReplayable,
        S::AttestationFailed,
    ];

    #[test]
    fn names_are_unique_upper_case_words() {
        let mut seen = std::collections::BTreeSet::new();
        for e in CATALOG {
            assert!(seen.insert(e.name), "{} twice", e.name);
            assert!(
                e.name.split('-').all(|w| !w.is_empty()
                    && w.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())),
                "{}",
                e.name
            );
            assert!(!e.fix.is_empty() && !e.title.ends_with('.'), "{}", e.name);
            assert!(
                e.title.chars().next().is_some_and(char::is_lowercase),
                "{}",
                e.name
            );
        }
    }

    #[test]
    fn every_non_ok_status_has_a_default_and_kinds_agree_with_statuses() {
        for s in STATUSES {
            let d = default_for(s);
            assert_eq!(d.is_none(), s == S::Ok, "{s:?}");
            if let Some(d) = d {
                assert_eq!(d.status, s);
            }
        }
        for e in CATALOG {
            assert_eq!(
                e.kind == Kind::Stop,
                e.status == S::NeedsHuman,
                "{}",
                e.name
            );
            assert_eq!(e.kind == Kind::Warning, e.status == S::Ok, "{}", e.name);
        }
    }

    #[test]
    fn near_misses_are_suggested_and_placeholders_filled() {
        assert_eq!(nearest("uncommited-intent", 1), ["UNCOMMITTED-INTENT"]);
        assert_eq!(nearest("NEEDS-APROVAL", 1), ["NEEDS-APPROVAL"]);
        assert!(nearest("completely-unrelated-words", 3).is_empty());
        let e = lookup("NEEDS-APPROVAL").unwrap();
        assert_eq!(
            fixes(e, "run", Some("T004"), None)[1],
            "speccoder task approve T004"
        );
        let r = lookup("INTERRUPTED").unwrap();
        assert_eq!(fixes(r, "run", None, None), ["speccoder run"]);
        assert_eq!(
            fixes(r, "run", None, Some("001-x")),
            ["speccoder run --feature 001-x"]
        );
    }
}
