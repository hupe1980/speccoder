//! What a person can do now: one list, computed by the runtime and
//! rendered by every interface — the dashboard's `n`, `c` and `:` today, a
//! desktop app or another workbench later. Every action is a CLI command
//! too, so nothing an interface offers is out of reach of a script.

use serde::{Deserialize, Serialize};

/// How an interface runs an action itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Runnable {
    /// A planning skill (`speccoder spec <name> <arguments>`).
    Skill {
        /// The skill.
        name: String,
        /// Its arguments (the input, when one is asked for).
        arguments: String,
    },
    /// `speccoder run`, the feature or one task.
    Run {
        /// One task, or the feature's ready tasks.
        task: Option<String>,
    },
    /// `speccoder run --converge`: the ready tasks, then the assessment of
    /// the code against the spec as it is now, appending what remains.
    Converge,
    /// Commit exactly these planning files.
    Commit {
        /// Repository-relative paths.
        paths: Vec<String>,
    },
    /// Approve the task's waiting call: the one that was shown, refused
    /// when another waits now.
    Approve {
        /// The task.
        task: String,
        /// The call's id ([`crate::PendingCall::id`]).
        call: String,
        /// The digest of the call that was shown ([`crate::PendingCall::digest`]).
        digest: String,
    },
    /// Reject it (the input is the reason), bound the same way.
    Reject {
        /// The task.
        task: String,
        /// The call's id ([`crate::PendingCall::id`]).
        call: String,
        /// The digest of the call that was shown ([`crate::PendingCall::digest`]).
        digest: String,
    },
    /// Tick the task on the person's word (the input is the reason).
    Accept {
        /// The task.
        task: String,
    },
    /// Skip the task (the input is the reason).
    Skip {
        /// The task.
        task: String,
    },
    /// Leave a note for the task's next attempts (the input is the note).
    Nudge {
        /// The task.
        task: String,
    },
    /// Rule that a held attempt's tests are right (the input is the reason).
    Rule {
        /// The task.
        task: String,
    },
    /// Record the person's review of requirements (the input is the
    /// comment; required to request changes).
    Review {
        /// The requirement identifiers; empty: every one not yet approved.
        ids: Vec<String>,
        /// `approve`, `request` (changes), `clear`; `answer` (ids: the
        /// question) and `edit` / `edit-approve` (ids: the requirement) take
        /// the input as the words.
        verdict: String,
    },
    /// Tell a GitHub issue it is being specified as the current feature.
    IssueComment {
        /// The issue's number.
        issue: u64,
        /// Also assign it to the person.
        assign: bool,
    },
    /// Record a quick change and, when `run`, run it.
    QuickChange {
        /// The change, in the person's words.
        request: String,
        /// When it is done: its acceptance scenarios.
        checks: Vec<String>,
        /// Run it once recorded.
        run: bool,
    },
    /// Commit the changes set aside before the task's run onto the
    /// feature's branch, as the person's own.
    KeepSetAside {
        /// The task.
        task: String,
    },
    /// Discard them.
    DropSetAside {
        /// The task.
        task: String,
    },
    /// Commit the setup a run stops on (an uncommitted lockfile, build
    /// output not ignored) in the checkout.
    FixSetup,
    /// Pick the current feature.
    Feature,
    /// Push the feature's branch and open (or update) its draft pull request.
    PullRequest,
    /// Record a bug in the person's words (`speccoder bug`) and, when
    /// `run`, reproduce it.
    BugFix {
        /// What happens (the symptom).
        symptom: String,
        /// What should happen instead (one or more).
        expected: Vec<String>,
        /// What must keep working (zero or more).
        unchanged: Vec<String>,
        /// The GitHub issue it comes from.
        issue: Option<u64>,
        /// The person's own reproduction tests, one per expected statement
        /// (`PATH::NAME`).
        tests: Vec<String>,
        /// Someone else's fix to judge: a patch file or `pr:N`.
        candidate: Option<String>,
        /// Continue a bug the format's own bug step created.
        slug: Option<String>,
        /// Reproduce it once recorded.
        run: bool,
    },
    /// Confirm (or, with `reject`, reject: the input is the reason) a
    /// bug's reproductions, bound to the digest shown.
    BugConfirm {
        /// `<unit>/<task>`.
        task: String,
        /// The digest the person saw.
        digest: String,
        /// Reject instead.
        reject: bool,
    },
    /// Keep an already-satisfied bug's reproductions as a regression test:
    /// committed, the task attested (the input is the reason).
    BugKeep {
        /// `<unit>/<task>`.
        task: String,
    },
    /// Continue a bug: reproduce again, or run its confirmed fix.
    BugRun {
        /// A bug of the format's own bug structure, by slug.
        slug: Option<String>,
        /// Or `<unit>/<task>`.
        task: Option<String>,
    },
    /// Always Allow: approve the waiting command once and add exactly its
    /// argv to `[commands] allow` (or `[sandbox] network_allow`).
    AllowAlways {
        /// The task.
        task: String,
        /// The waiting call's id.
        call: String,
    },
    /// Add a prefix to a prefix list, as the confirmation of its coverage
    /// preview (refused when the preview changed or is incomplete).
    WidenPolicy {
        /// Which list.
        key: PolicyList,
        /// The prefix (two elements or more).
        prefix: Vec<String>,
        /// The digest of the preview the person saw.
        digest: String,
    },
    /// Decide the calls waiting in a run's batch together, each once.
    DecideBatch {
        /// The run.
        run: String,
        /// Approve or reject (the input is the reason).
        verdict: BatchVerdict,
        /// The listed calls decided (a non-empty subset).
        calls: Vec<String>,
        /// The digest of the list the person saw.
        digest: String,
        /// Start the run's asking budget again.
        resume_asking: bool,
    },
}

/// A prefix list in `speccoder.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyList {
    /// `[commands] allow_prefix`.
    CommandsPrefix,
    /// `[sandbox] network_allow_prefix`.
    NetworkPrefix,
}

impl PolicyList {
    /// Its name as serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommandsPrefix => "commands_prefix",
            Self::NetworkPrefix => "network_prefix",
        }
    }

    /// The configuration key it is (`commands.allow_prefix`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::CommandsPrefix => "commands.allow_prefix",
            Self::NetworkPrefix => "sandbox.network_allow_prefix",
        }
    }

    /// The list for a configuration key.
    #[must_use]
    pub fn of_key(key: &str) -> Option<Self> {
        match key {
            "commands.allow_prefix" | "commands_prefix" => Some(Self::CommandsPrefix),
            "sandbox.network_allow_prefix" | "network_prefix" => Some(Self::NetworkPrefix),
            _ => None,
        }
    }
}

/// A batch decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchVerdict {
    /// Each listed call once.
    Approve,
    /// Each listed call, with one reason.
    Reject,
}

/// What kind of action it is: one per [`Runnable`] kind (a run split by
/// whether it names a task), plus [`ActionKind::Command`] for an action an
/// interface does not run itself. Exhaustive at any one version; a new
/// kind arrives with its `Runnable` variant (the *adding a kind* contract
/// of `speccoder_runtime::actions::perform`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// [`Runnable::Skill`].
    Skill,
    /// [`Runnable::Run`] with a task.
    RunTask,
    /// [`Runnable::Run`] without one: the feature.
    RunFeature,
    /// [`Runnable::Converge`].
    Converge,
    /// [`Runnable::Commit`].
    Commit,
    /// [`Runnable::Approve`].
    Approve,
    /// [`Runnable::Reject`].
    Reject,
    /// [`Runnable::Accept`].
    Accept,
    /// [`Runnable::Skip`].
    Skip,
    /// [`Runnable::Nudge`].
    Nudge,
    /// [`Runnable::Rule`].
    Rule,
    /// [`Runnable::Review`].
    Review,
    /// [`Runnable::IssueComment`].
    IssueComment,
    /// [`Runnable::QuickChange`].
    QuickChange,
    /// [`Runnable::KeepSetAside`].
    KeepSetAside,
    /// [`Runnable::DropSetAside`].
    DropSetAside,
    /// [`Runnable::FixSetup`].
    FixSetup,
    /// [`Runnable::Feature`].
    Feature,
    /// [`Runnable::PullRequest`].
    PullRequest,
    /// [`Runnable::BugFix`].
    BugFix,
    /// [`Runnable::BugConfirm`].
    BugConfirm,
    /// [`Runnable::BugRun`].
    BugRun,
    /// [`Runnable::BugKeep`].
    BugKeep,
    /// [`Runnable::AllowAlways`].
    AllowAlways,
    /// [`Runnable::WidenPolicy`].
    WidenPolicy,
    /// [`Runnable::DecideBatch`].
    DecideBatch,
    /// No runnable: the person runs the command.
    Command,
}

impl ActionKind {
    /// Every kind, in declaration order (a test holds it to the enum).
    pub const ALL: &'static [Self] = &[
        Self::Skill,
        Self::RunTask,
        Self::RunFeature,
        Self::Converge,
        Self::Commit,
        Self::Approve,
        Self::Reject,
        Self::Accept,
        Self::Skip,
        Self::Nudge,
        Self::Rule,
        Self::Review,
        Self::IssueComment,
        Self::QuickChange,
        Self::KeepSetAside,
        Self::DropSetAside,
        Self::FixSetup,
        Self::Feature,
        Self::PullRequest,
        Self::BugFix,
        Self::BugConfirm,
        Self::BugRun,
        Self::BugKeep,
        Self::AllowAlways,
        Self::WidenPolicy,
        Self::DecideBatch,
        Self::Command,
    ];

    /// Its name as serialized (`run_task`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::RunTask => "run_task",
            Self::RunFeature => "run_feature",
            Self::Converge => "converge",
            Self::Commit => "commit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Accept => "accept",
            Self::Skip => "skip",
            Self::Nudge => "nudge",
            Self::Rule => "rule",
            Self::Review => "review",
            Self::IssueComment => "issue_comment",
            Self::QuickChange => "quick_change",
            Self::KeepSetAside => "keep_set_aside",
            Self::DropSetAside => "drop_set_aside",
            Self::FixSetup => "fix_setup",
            Self::Feature => "feature",
            Self::PullRequest => "pull_request",
            Self::BugFix => "bug_fix",
            Self::BugConfirm => "bug_confirm",
            Self::BugRun => "bug_run",
            Self::BugKeep => "bug_keep",
            Self::AllowAlways => "allow_always",
            Self::WidenPolicy => "widen_policy",
            Self::DecideBatch => "decide_batch",
            Self::Command => "command",
        }
    }

    /// One runnable of this kind (`None` for [`ActionKind::Command`]): what
    /// tests and documentation enumerate the kinds by.
    #[must_use]
    pub fn example(self) -> Option<Runnable> {
        let task = || "T001".to_owned();
        Some(match self {
            Self::Skill => Runnable::Skill {
                name: "plan".to_owned(),
                arguments: String::new(),
            },
            Self::RunTask => Runnable::Run { task: Some(task()) },
            Self::RunFeature => Runnable::Run { task: None },
            Self::Converge => Runnable::Converge,
            Self::Commit => Runnable::Commit {
                paths: vec!["plan.md".to_owned()],
            },
            Self::Approve => Runnable::Approve {
                task: task(),
                call: "0f3c".to_owned(),
                digest: "d1".to_owned(),
            },
            Self::Reject => Runnable::Reject {
                task: task(),
                call: "0f3c".to_owned(),
                digest: "d1".to_owned(),
            },
            Self::Accept => Runnable::Accept { task: task() },
            Self::Skip => Runnable::Skip { task: task() },
            Self::Nudge => Runnable::Nudge { task: task() },
            Self::Rule => Runnable::Rule { task: task() },
            Self::Review => Runnable::Review {
                ids: vec!["FR-001".to_owned()],
                verdict: "approve".to_owned(),
            },
            Self::IssueComment => Runnable::IssueComment {
                issue: 42,
                assign: false,
            },
            Self::QuickChange => Runnable::QuickChange {
                request: "a change".to_owned(),
                checks: Vec::new(),
                run: false,
            },
            Self::KeepSetAside => Runnable::KeepSetAside { task: task() },
            Self::DropSetAside => Runnable::DropSetAside { task: task() },
            Self::FixSetup => Runnable::FixSetup,
            Self::Feature => Runnable::Feature,
            Self::PullRequest => Runnable::PullRequest,
            Self::BugFix => Runnable::BugFix {
                symptom: "saving an empty title crashes".to_owned(),
                expected: vec!["an empty title is refused".to_owned()],
                unchanged: Vec::new(),
                issue: None,
                tests: Vec::new(),
                candidate: None,
                slug: None,
                run: false,
            },
            Self::BugConfirm => Runnable::BugConfirm {
                task: "007-bugfixes/T001".to_owned(),
                digest: "sha256:0".to_owned(),
                reject: false,
            },
            Self::BugRun => Runnable::BugRun {
                slug: None,
                task: Some("007-bugfixes/T001".to_owned()),
            },
            Self::BugKeep => Runnable::BugKeep {
                task: "007-bugfixes/T001".to_owned(),
            },
            Self::AllowAlways => Runnable::AllowAlways {
                task: task(),
                call: "0f3c".to_owned(),
            },
            Self::WidenPolicy => Runnable::WidenPolicy {
                key: PolicyList::CommandsPrefix,
                prefix: vec!["npm".to_owned(), "run".to_owned()],
                digest: "d1".to_owned(),
            },
            Self::DecideBatch => Runnable::DecideBatch {
                run: "run-1-0a0b0c0d".to_owned(),
                verdict: BatchVerdict::Approve,
                calls: vec!["0f3c".to_owned()],
                digest: "d1".to_owned(),
                resume_asking: false,
            },
            Self::Command => return None,
        })
    }
}

impl Runnable {
    /// One runnable of every kind ([`ActionKind::example`]).
    #[must_use]
    pub fn examples() -> Vec<Self> {
        ActionKind::ALL.iter().filter_map(|k| k.example()).collect()
    }

    /// Its kind.
    #[must_use]
    pub const fn kind(&self) -> ActionKind {
        match self {
            Self::Skill { .. } => ActionKind::Skill,
            Self::Run { task: Some(_) } => ActionKind::RunTask,
            Self::Run { task: None } => ActionKind::RunFeature,
            Self::Converge => ActionKind::Converge,
            Self::Commit { .. } => ActionKind::Commit,
            Self::Approve { .. } => ActionKind::Approve,
            Self::Reject { .. } => ActionKind::Reject,
            Self::Accept { .. } => ActionKind::Accept,
            Self::Skip { .. } => ActionKind::Skip,
            Self::Nudge { .. } => ActionKind::Nudge,
            Self::Rule { .. } => ActionKind::Rule,
            Self::Review { .. } => ActionKind::Review,
            Self::IssueComment { .. } => ActionKind::IssueComment,
            Self::QuickChange { .. } => ActionKind::QuickChange,
            Self::KeepSetAside { .. } => ActionKind::KeepSetAside,
            Self::DropSetAside { .. } => ActionKind::DropSetAside,
            Self::FixSetup => ActionKind::FixSetup,
            Self::Feature => ActionKind::Feature,
            Self::PullRequest => ActionKind::PullRequest,
            Self::BugFix { .. } => ActionKind::BugFix,
            Self::BugConfirm { .. } => ActionKind::BugConfirm,
            Self::BugRun { .. } => ActionKind::BugRun,
            Self::BugKeep { .. } => ActionKind::BugKeep,
            Self::AllowAlways { .. } => ActionKind::AllowAlways,
            Self::WidenPolicy { .. } => ActionKind::WidenPolicy,
            Self::DecideBatch { .. } => ActionKind::DecideBatch,
        }
    }
}

/// What an action is, independent of its wording: its kind and what it
/// is about (the task, the skill, the issue). An interface finds an action
/// by this, never by its label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionId {
    /// Its kind.
    pub kind: ActionKind,
    /// What it is about: a task's id, a skill's name, an issue's number,
    /// a review's `verdict` (`verdict:first id`), a command's words.
    pub subject: Option<String>,
}

/// The identity of the action that runs `run` (or, without one, stands
/// for `command`): derived, so it cannot disagree with what runs.
#[must_use]
pub fn identity(run: Option<&Runnable>, command: &str) -> ActionId {
    let Some(run) = run else {
        // The command's words: `speccoder attest verify` → `attest verify`.
        let words: Vec<&str> = command
            .split_whitespace()
            .skip_while(|w| *w == "speccoder")
            .take_while(|w| {
                w.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                    && w.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            })
            .collect();
        return ActionId {
            kind: ActionKind::Command,
            subject: (!words.is_empty()).then(|| words.join(" ")),
        };
    };
    let subject = match run {
        Runnable::Skill { name, .. } => Some(name.clone()),
        Runnable::Run { task } => task.clone(),
        Runnable::Approve { task, .. }
        | Runnable::Reject { task, .. }
        | Runnable::Accept { task }
        | Runnable::Skip { task }
        | Runnable::Nudge { task }
        | Runnable::Rule { task }
        | Runnable::KeepSetAside { task }
        | Runnable::DropSetAside { task }
        | Runnable::AllowAlways { task, .. } => Some(task.clone()),
        Runnable::WidenPolicy { key, .. } => Some(key.as_str().to_owned()),
        Runnable::DecideBatch { run, .. } => Some(run.clone()),
        Runnable::IssueComment { issue, .. } => Some(issue.to_string()),
        Runnable::BugConfirm { task, .. } | Runnable::BugKeep { task } => Some(task.clone()),
        Runnable::BugRun { slug, task } => task.clone().or_else(|| slug.clone()),
        Runnable::BugFix { slug, issue, .. } => {
            slug.clone().or_else(|| issue.map(|n| format!("#{n}")))
        }
        Runnable::Review { ids, verdict } => Some(match ids.first() {
            Some(id) => format!("{verdict}:{id}"),
            None => verdict.clone(),
        }),
        Runnable::Converge
        | Runnable::Commit { .. }
        | Runnable::QuickChange { .. }
        | Runnable::FixSetup
        | Runnable::Feature
        | Runnable::PullRequest => None,
    };
    ActionId {
        kind: run.kind(),
        subject,
    }
}

/// The part a planning skill plays in its format's planning, from the
/// format's adapter: what an interface finds "the create step" or "the
/// analysis step" by, without naming a format's skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillRole {
    /// It creates a unit (Spec Kit's `specify`).
    Create,
    /// It leads toward the task list (`plan`, `tasks`).
    TowardTasks,
    /// It revises a written spec (`clarify`).
    Revise,
    /// It checks the planning files for consistency (`analyze`).
    Check,
    /// Anything else.
    Other,
}

/// The interface a person acted through, as a write records it (`via`):
/// one vocabulary for every record. Records written before it named
/// `cli`, `tui` or `dashboard` (read as [`Via::Terminal`]) and
/// `editor <name>` (read as [`Via::Acp`], the name a separate client).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// A terminal: the line CLI, its prompts, the dashboard.
    #[serde(alias = "cli", alias = "tui", alias = "dashboard")]
    Terminal,
    /// The web app, in a browser tab or the desktop window.
    Web,
    /// The desktop window, once it can tell itself from a tab.
    Window,
    /// An editor over the Agent Client Protocol.
    Acp,
    /// A phone.
    Phone,
    /// Continuous integration.
    Ci,
    /// A client over the Model Context Protocol.
    Mcp,
}

impl Via {
    /// Its recorded value (`terminal`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Web => "web",
            Self::Window => "window",
            Self::Acp => "acp",
            Self::Phone => "phone",
            Self::Ci => "ci",
            Self::Mcp => "mcp",
        }
    }

    /// A recorded value, read back with the client it names (`editor Zed`
    /// → `acp`, `Zed`); `None` for a value no interface ever recorded.
    #[must_use]
    pub fn read(recorded: &str) -> Option<(Self, Option<String>)> {
        if let Some(client) = recorded.strip_prefix("editor ") {
            return Some((Self::Acp, Some(client.to_owned())));
        }
        let via = match recorded {
            "terminal" | "cli" | "tui" | "dashboard" => Self::Terminal,
            "web" => Self::Web,
            "window" => Self::Window,
            "acp" | "editor" => Self::Acp,
            "phone" => Self::Phone,
            "ci" => Self::Ci,
            "mcp" => Self::Mcp,
            _ => return None,
        };
        Some((via, None))
    }
}

impl std::fmt::Display for Via {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing a person can do now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Action {
    /// `next`, `plan`, `work`, `task`, `feature` or `review`.
    pub group: String,
    /// What it does, in a few words.
    pub label: String,
    /// Why, or what follows (shown beside it).
    pub why: String,
    /// The equivalent command; `"…"` marks the input.
    pub command: String,
    /// How an interface runs it; `None`: run the command yourself (it leaves
    /// the machine, or it is not something an interface hosts).
    pub run: Option<Runnable>,
    /// Why it cannot run now; `None`: it can.
    pub blocked: Option<String>,
    /// What to ask for before it runs (`Describe the feature`).
    pub input: Option<String>,
    /// It leaves the machine (a push, a pull request) or cannot be undone
    /// (a discard): what an interface says in a confirmation before it runs it.
    pub confirm: Option<String>,
    /// What it is, independent of its wording ([`identity`]).
    pub id: ActionId,
    /// A planning skill's part in its format's planning; `None` for
    /// everything else.
    pub role: Option<SkillRole>,
}

impl Action {
    /// An action that stands for `command`, which the person runs.
    #[must_use]
    pub fn new(
        group: &str,
        label: impl Into<String>,
        why: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        let command = command.into();
        Self {
            group: group.to_owned(),
            label: label.into(),
            why: why.into(),
            id: identity(None, &command),
            command,
            run: None,
            blocked: None,
            input: None,
            confirm: None,
            role: None,
        }
    }

    /// Run by an interface as `run`; its identity follows.
    #[must_use]
    pub fn with_run(mut self, run: Runnable) -> Self {
        self.id = identity(Some(&run), &self.command);
        self.run = Some(run);
        self
    }

    /// A planning skill playing `role`.
    #[must_use]
    pub const fn with_role(mut self, role: SkillRole) -> Self {
        self.role = Some(role);
        self
    }

    /// It can be run from an interface, now.
    #[must_use]
    pub const fn runnable(&self) -> bool {
        self.run.is_some() && self.blocked.is_none()
    }
}

/// The primary action: the first runnable one of the `next` group.
#[must_use]
pub fn primary(actions: &[Action]) -> Option<&Action> {
    actions.iter().find(|a| a.group == "next" && a.runnable())
}

/// Where a step of the journey is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    /// Done.
    Done,
    /// The one to take now.
    Now,
    /// Later.
    Todo,
    /// Passed without its file: a task list written without a spec or a
    /// plan. Not a step left to take.
    Skipped,
}

/// One step of a feature's journey: `spec`, `plan`, `tasks`, `commit`,
/// `run`, `review`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct JourneyStep {
    /// Its name.
    pub name: String,
    /// Where it is.
    pub state: StepState,
}

/// Facts the journey is derived from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JourneyFacts {
    /// `spec.md` exists.
    pub spec: bool,
    /// `plan.md` exists.
    pub plan: bool,
    /// The task list exists.
    pub tasks: bool,
    /// Planning files are not committed.
    pub uncommitted: bool,
    /// Tasks in the list.
    pub total: usize,
    /// Of them ticked or skipped.
    pub finished: usize,
}

/// The journey from a spec to a reviewed branch, and where the feature is
/// on it: everything before the first open step is done (or skipped, when a
/// later planning file exists without it), that one is now.
#[must_use]
pub fn journey(f: JourneyFacts) -> Vec<JourneyStep> {
    let skipped = [!f.spec && (f.plan || f.tasks), !f.plan && f.tasks];
    let done = [
        ("spec", f.spec),
        ("plan", f.plan),
        ("tasks", f.tasks),
        ("commit", f.tasks && !f.uncommitted),
        ("run", f.total > 0 && f.finished == f.total),
        ("review", false),
    ];
    let passed = |i: usize| skipped.get(i).copied().unwrap_or(false);
    let now = done
        .iter()
        .enumerate()
        .position(|(i, (_, d))| !d && !passed(i))
        .unwrap_or(done.len());
    done.iter()
        .enumerate()
        .map(|(i, (name, d))| JourneyStep {
            name: (*name).to_owned(),
            state: if *d && i < now {
                StepState::Done
            } else if passed(i) && i < now {
                StepState::Skipped
            } else if i == now {
                StepState::Now
            } else {
                StepState::Todo
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_listed_once_and_every_runnable_has_its_own() {
        // The schema lists every variant the compiler knows.
        let schema = serde_json::to_value(schemars::schema_for!(ActionKind))
            .unwrap_or_else(|e| panic!("{e}"));
        let mut named: Vec<String> = schema["oneOf"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["const"].as_str().map(str::to_owned))
            .collect();
        named.sort();
        let mut listed: Vec<String> = ActionKind::ALL
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect();
        listed.sort();
        assert_eq!(named, listed, "ActionKind::ALL is not the enum");
        for k in ActionKind::ALL {
            assert_eq!(
                serde_json::to_value(k).unwrap_or_default(),
                k.as_str(),
                "{k:?}"
            );
            if let Some(run) = k.example() {
                assert_eq!(run.kind(), *k);
                assert_eq!(identity(Some(&run), "").kind, *k);
            }
        }
        assert_eq!(
            Runnable::examples().len(),
            ActionKind::ALL.len() - 1,
            "every kind but `command` has a runnable"
        );
    }

    #[test]
    fn identity_is_what_runs_never_how_it_is_worded() {
        let a = Action::new(
            "task",
            "accept T004",
            "tick it",
            "speccoder task accept T004",
        )
        .with_run(Runnable::Accept {
            task: "T004".to_owned(),
        });
        let mut b = a.clone();
        b.label = "x tick it anyway".to_owned();
        b.why = "x".to_owned();
        assert_eq!(a.id, b.id);
        assert_eq!(
            a.id,
            ActionId {
                kind: ActionKind::Accept,
                subject: Some("T004".to_owned())
            }
        );
        let id = |c: &str| identity(None, c).subject;
        assert_eq!(
            id("speccoder attest verify").as_deref(),
            Some("attest verify")
        );
        assert_eq!(id("speccoder doctor").as_deref(), Some("doctor"));
        assert_eq!(id("speccoder pr --yes").as_deref(), Some("pr"));
        assert_eq!(
            id("speccoder spec specify \"…\"").as_deref(),
            Some("spec specify")
        );
        let review = |ids: &[&str], v: &str| {
            identity(
                Some(&Runnable::Review {
                    ids: ids.iter().map(|s| (*s).to_owned()).collect(),
                    verdict: v.to_owned(),
                }),
                "",
            )
            .subject
        };
        assert_eq!(review(&[], "approve").as_deref(), Some("approve"));
        assert_eq!(review(&["Q1"], "answer").as_deref(), Some("answer:Q1"));
    }

    #[test]
    fn origins_read_back_from_every_value_ever_recorded() {
        for (old, via, client) in [
            ("cli", Via::Terminal, None),
            ("tui", Via::Terminal, None),
            ("dashboard", Via::Terminal, None),
            ("terminal", Via::Terminal, None),
            ("web", Via::Web, None),
            ("editor Zed", Via::Acp, Some("Zed")),
        ] {
            assert_eq!(
                Via::read(old),
                Some((via, client.map(str::to_owned))),
                "{old}"
            );
        }
        let parsed: Via = serde_json::from_str("\"dashboard\"").unwrap_or(Via::Web);
        assert_eq!(parsed, Via::Terminal);
        assert_eq!(Via::Acp.as_str(), "acp");
        assert_eq!(Via::read("nobody"), None);
    }

    fn states(f: JourneyFacts) -> Vec<(String, StepState)> {
        journey(f).into_iter().map(|s| (s.name, s.state)).collect()
    }

    #[test]
    fn the_journey_marks_what_is_done_and_what_is_now() {
        use StepState::{Done, Now, Todo};
        let names = ["spec", "plan", "tasks", "commit", "run", "review"].map(String::from);
        let at = |v: [StepState; 6]| names.iter().cloned().zip(v).collect::<Vec<_>>();
        assert_eq!(
            states(JourneyFacts::default()),
            at([Now, Todo, Todo, Todo, Todo, Todo])
        );
        let planned = JourneyFacts {
            spec: true,
            plan: true,
            ..JourneyFacts::default()
        };
        assert_eq!(states(planned), at([Done, Done, Now, Todo, Todo, Todo]));
        let written = JourneyFacts {
            tasks: true,
            uncommitted: true,
            total: 3,
            ..planned
        };
        assert_eq!(states(written), at([Done, Done, Done, Now, Todo, Todo]));
        let running = JourneyFacts {
            uncommitted: false,
            finished: 1,
            ..written
        };
        assert_eq!(states(running), at([Done, Done, Done, Done, Now, Todo]));
        let finished = JourneyFacts {
            finished: 3,
            ..running
        };
        assert_eq!(states(finished), at([Done, Done, Done, Done, Done, Now]));
    }

    #[test]
    fn a_task_list_without_a_spec_skips_to_the_run() {
        use StepState::{Now, Skipped, Todo};
        let names = ["spec", "plan", "tasks", "commit", "run", "review"].map(String::from);
        let at = |v: [StepState; 6]| names.iter().cloned().zip(v).collect::<Vec<_>>();
        let only_tasks = JourneyFacts {
            tasks: true,
            total: 4,
            finished: 1,
            ..JourneyFacts::default()
        };
        assert_eq!(
            states(only_tasks),
            at([
                Skipped,
                Skipped,
                StepState::Done,
                StepState::Done,
                Now,
                Todo
            ])
        );
        // A plan without a spec: the spec is skipped, the tasks are next.
        let planned = JourneyFacts {
            plan: true,
            ..JourneyFacts::default()
        };
        assert_eq!(
            states(planned),
            at([Skipped, StepState::Done, Now, Todo, Todo, Todo])
        );
    }
}
