//! Stable codes for every result that is not plainly `ok` (F-056, D-062):
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
        "No `.specify/` (Spec Kit) or `openspec/` (OpenSpec) directory was found here or above. speccoder implements specs; it does not write them from nothing.",
        ["specify init --here", "openspec init"]
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
        "SANDBOX-UNAVAILABLE",
        E,
        S::ConfigError,
        "the OS sandbox is not available and the project requires it",
        "`[sandbox] mode = \"required\"` refuses to run commands unsandboxed, and this OS offers no sandbox (Linux without Landlock, a container without user namespaces, another OS).",
        [
            "→ run on macOS or a Linux with Landlock (5.13+)",
            "→ or set [sandbox] mode = \"auto\" to run unsandboxed and be told"
        ]
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
        "speccoder reads API keys from the environment only. The message names the variable the configured provider needs.",
        ["→ export the variable the message names, e.g. export ANTHROPIC_API_KEY=…"]
    ),
    entry!(
        "NO-CHECKS",
        E,
        S::ConfigError,
        "no check applies, so nothing could show that nothing regressed",
        "Every configured check was skipped (`when_exists` did not match) or none is configured. A task cannot be judged without at least one check.",
        ["speccoder init --force"]
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
        "`pr` pushes `speccoder/<feature>` to a remote and opens the pull request there.",
        ["git remote -v", "speccoder pr --remote <name>"]
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
    // in place (F-048)
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
        "The checks still pass, but no acceptance test showed this task's behaviour (the spec has no example for it, or the tests could not be made red first). It is ticked `regression_only`, never `verified`.",
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
    // ── stops ─────────────────────────────────────────────────────────
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
        "The model used its honest exit: something in the spec or the code stops it. Its reason is a claim, shown as \"Model says\".",
        [
            "speccoder task nudge {task} --note \"…\"",
            "speccoder task skip {task} --reason \"…\""
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
        "ACCEPTANCE-UNWRITTEN",
        P,
        S::NeedsHuman,
        "the acceptance tests could not be written, so nothing was attempted",
        "The call that derives acceptance tests from the spec failed — the provider refused it (no credits, a rejected key) or the run broke. Without those tests the task could only be judged on the regression checks, which would look like a spec without examples.",
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
        "After the task passed, an alternative the spec leaves open (another order, wording, tie-break) was run against the acceptance tests, and they failed it on an assertion: the tests may encode a choice the spec did not make. The attempt is held — its tree kept — until a person rules. A ruling that the tests are right ticks the held attempt on the next run without a model call.",
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
        "No credits, a rejected or revoked key, or an account limit. speccoder stops instead of judging the task on less evidence.",
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
    entry
        .fix
        .iter()
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
