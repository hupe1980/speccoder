//! What a person can do now, and taking it. The list comes from the
//! project as it stands: the report, what waits for a decision, the
//! planning files not yet committed and the installed planning skills.
//! Interfaces render the list; [`perform`] is the one place an action's
//! [`Runnable`] becomes runtime calls, for every interface.
//!
//! # Adding a kind
//!
//! Every new person-initiated action — a model run or a plain write —
//! lands in one change with all of:
//!
//! 1. a `Runnable` variant with typed fields (`speccoder_core::actions`);
//! 2. its `ActionKind`, its `identity` arm and its subject rule (same file);
//! 3. exactly one arm in [`perform_in`] (input rules, `needs(..)`
//!    messages, the dispatch's `via` passed through);
//! 4. its result payload: an existing `CommandData` variant, or a new one
//!    with a unique name in `PAYLOADS`;
//! 5. that payload's published schema (`SPECCODER_BLESS=1`);
//! 6. its generated UI type (`crates/speccoder-web/ui/src/lib/schema.gen.ts`);
//! 7. a decision for every interface in [`offered`] (no wildcard: the
//!    build fails until each interface has one);
//! 8. a golden for its arm (`tests/fixtures/perform/`);
//! 9. if the web app shows it, its verb and icon in the UI's kind table
//!    (`ui/src/lib/actions.ts`).
//!
//! No feature adds a web job kind, a web route that writes, a terminal
//! prompt, an editor command or a dashboard key that maps a kind to runtime
//! calls itself, nor a marker on an existing variant meaning another
//! action: it adds a kind. Only the interfaces of [`Interface`] call
//! [`perform`] — never a model tool, a hook, a gate or the engine — so
//! nothing but a person's command approves, rejects, accepts, rules or
//! ticks through it. `specs/089-action-contract/quickstart.md` walks
//! through one such change.

use std::path::Path;

use speccoder_core::actions::{Action, ActionKind, Runnable, Via};
use speccoder_core::{ApprovalsReport, CommandData, CommandResult, CommandStatus, StatusReport};

use crate::RunOptions;

pub(crate) fn action(
    group: &str,
    label: impl Into<String>,
    why: impl Into<String>,
    command: impl Into<String>,
) -> Action {
    Action::new(group, label, why, command)
}

/// What can be done now in the project at `path` (`feature` as for every
/// command), with `selected` the task an interface has selected.
#[must_use]
pub fn available(path: &Path, feature: Option<&str>, selected: Option<&str>) -> Vec<Action> {
    available_with(path, feature, selected, None)
}

/// [`available`] with the waiting calls already listed (an interface that
/// lists them anyway): the journal is not opened again.
#[must_use]
pub fn available_with(
    path: &Path,
    feature: Option<&str>,
    selected: Option<&str>,
    listed: Option<&ApprovalsReport>,
) -> Vec<Action> {
    let report = crate::status::report(path, feature);
    let report: Option<&StatusReport> = match report.data() {
        Some(CommandData::Report(r)) => Some(r),
        _ => None,
    };
    let approvals = listed.cloned().or_else(|| {
        crate::worktree::state_dir().and_then(|state| {
            match crate::approvals::list_with(path, feature, &state).data() {
                Some(CommandData::Approvals(a)) => Some((**a).clone()),
                _ => None,
            }
        })
    });
    let uncommitted = crate::hosting::uncommitted(path, feature);
    let skills = match crate::hosting::list(path).data() {
        Some(CommandData::Skills(s)) => s.skills.clone(),
        _ => Vec::new(),
    };
    let format = crate::format::at(path);
    let planning = format.planning();
    let unit = format.messages().unit;
    let features = crate::current::list(path, None);
    let feature_count = match features.data() {
        Some(CommandData::Features(f)) => f.features.len(),
        _ => 0,
    };
    let short = |name: &str| {
        name.strip_prefix(planning.prefix)
            .unwrap_or(name)
            .to_owned()
    };
    let skill = |group: &str, name: &str, why: &str| {
        let creates = planning.creates.contains(&name);
        let mut a = action(
            group,
            format!("spec {name}"),
            why,
            if creates {
                format!("speccoder spec {name} \"…\"")
            } else {
                format!("speccoder spec {name}")
            },
        );
        a = a
            .with_run(Runnable::Skill {
                name: name.to_owned(),
                arguments: String::new(),
            })
            .with_role(planning.role_of(name));
        if creates {
            a.input = Some(format!("Describe the {unit}"));
        }
        a
    };
    let mut out = Vec::new();

    // ── next: what moves the work forward, most urgent first ──────────
    for p in approvals.iter().flat_map(|a: &ApprovalsReport| &a.pending) {
        let mut a = action(
            "next",
            format!("approve {}'s waiting call", p.task),
            format!("{} — {}", p.covers, p.reason),
            format!("speccoder task approve {}", p.task),
        );
        a = a.with_run(Runnable::Approve {
            task: p.task.clone(),
            call: p.id(),
            digest: p.digest.clone(),
        });
        out.push(a);
        let mut r = action(
            "next",
            format!("reject {}'s waiting call", p.task),
            "the model is told your reason",
            format!("speccoder task reject {} --reason \"…\"", p.task),
        );
        r = r.with_run(Runnable::Reject {
            task: p.task.clone(),
            call: p.id(),
            digest: p.digest.clone(),
        });
        r.input = Some("Why (the model is told)".to_owned());
        out.push(r);
    }
    let planning_next: Vec<String> = report.map(|r| r.planning.clone()).unwrap_or_default();
    for name in &planning_next {
        let why = match name.as_str() {
            "plan" => "write the plan from the spec",
            "tasks" => "break the plan into tasks",
            _ => "the next planning step",
        };
        out.push(skill("next", name, why));
    }
    if !uncommitted.is_empty() {
        let mut a = action(
            "next",
            format!("commit the planning files ({})", uncommitted.len()),
            "speccoder runs from committed files",
            format!(
                "git add {0} && git commit -m \"spec: …\" -- {0}",
                uncommitted.join(" ")
            ),
        );
        a = a.with_run(Runnable::Commit {
            paths: uncommitted.clone(),
        });
        out.push(a);
    }
    let ready = report.map_or(0, |r| r.totals.ready);
    let run_blocked = (!uncommitted.is_empty())
        .then(|| "commit the planning files first: speccoder runs from committed files".to_owned());
    if ready > 0 && planning_next.is_empty() {
        let mut a = action(
            "next",
            "run the feature",
            format!("{ready} ready task(s), each judged on evidence"),
            "speccoder run",
        );
        a = a.with_run(Runnable::Run { task: None });
        a.blocked = run_blocked.clone();
        out.push(a);
    }
    // The spec changed under ticks that cite it: their evidence is for the
    // earlier wording. Convergence assesses the code against it as it is.
    let stale: Vec<&str> = report
        .map(|r| {
            r.tasks
                .iter()
                .filter(|t| t.drift.as_ref().is_some_and(speccoder_core::Drift::stale))
                .map(|t| t.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    if !stale.is_empty() && planning_next.is_empty() {
        let mut a = action(
            "next",
            "re-check what the spec changed",
            format!(
                "{} was proven against an earlier wording; convergence assesses the code against the spec as it is now and adds tasks for what no longer holds",
                stale.join(", ")
            ),
            "speccoder run --converge",
        );
        a = a.with_run(Runnable::Converge);
        a.blocked = run_blocked.clone();
        out.push(a);
    }
    if report.is_none()
        && feature_count == 0
        && format.detect(path).is_some()
        && let Some(create) = planning.creates.first()
    {
        out.push(skill(
            "next",
            create,
            &format!("write the first {unit}'s spec"),
        ));
    }

    // ── plan: every hosted planning skill ─────────────────────────────
    for s in skills.iter().filter(|s| s.refused.is_none()) {
        let name = short(&s.name);
        if planning_next.contains(&name) {
            continue;
        }
        out.push(skill("plan", &name, &s.description));
    }

    // ── work ───────────────────────────────────────────────────────────
    if let Some(t) =
        selected.and_then(|id| report.and_then(|r| r.tasks.iter().find(|t| t.id == id)))
    {
        if t.state == "ready" {
            let mut a = action(
                "work",
                format!("run {}", t.id),
                t.title.clone(),
                format!("speccoder run --task {}", t.id),
            );
            a = a.with_run(Runnable::Run {
                task: Some(t.id.clone()),
            });
            a.blocked = run_blocked.clone();
            out.push(a);
        }
        // ── task: the selected one ────────────────────────────────────
        if !matches!(
            t.state.as_str(),
            "verified" | "regression_only" | "attested"
        ) {
            for (verb, why, prompt, run) in [
                (
                    "accept",
                    "tick it on your word (attested, not verified)",
                    "Why it is done",
                    Runnable::Accept { task: t.id.clone() },
                ),
                (
                    "skip",
                    "runs pass it by; what depends on it stays blocked",
                    "Why it is skipped",
                    Runnable::Skip { task: t.id.clone() },
                ),
                (
                    "nudge",
                    "a note its next attempts read",
                    "The note",
                    Runnable::Nudge { task: t.id.clone() },
                ),
            ] {
                let flag = if verb == "nudge" {
                    "--note"
                } else {
                    "--reason"
                };
                let mut a = action(
                    "task",
                    format!("{verb} {}", t.id),
                    why,
                    format!("speccoder task {verb} {} {flag} \"…\"", t.id),
                );
                a = a.with_run(run);
                a.input = Some(prompt.to_owned());
                out.push(a);
            }
        }
    }
    if ready > 0 && !planning_next.is_empty() {
        // Tasks exist and planning goes on: running is possible too.
        let mut a = action(
            "work",
            "run the feature",
            format!("{ready} ready task(s)"),
            "speccoder run",
        );
        a = a.with_run(Runnable::Run { task: None });
        a.blocked = run_blocked;
        out.push(a);
    }

    // ── feature ────────────────────────────────────────────────────────
    if feature_count > 1 {
        let mut a = action(
            "feature",
            format!("switch the {unit}"),
            format!("{feature_count} {unit}s"),
            "speccoder feature <name>",
        );
        a = a.with_run(Runnable::Feature);
        out.push(a);
    }

    // ── review: commands that leave the machine or are best read in full ─
    // It leaves the machine: an interface confirms it first.
    let mut pr = action(
        "review",
        "open a draft pull request",
        "push the branch and its evidence; open (or update) the draft",
        "speccoder pr --yes",
    );
    pr = pr.with_run(Runnable::PullRequest);
    pr.confirm = Some(
        "Push the feature's speccoder branch and its attestation notes to `origin`, and open (or update) its draft pull request on GitHub, as you (your token).".to_owned(),
    );
    out.push(pr);
    out.push(action(
        "review",
        "verify the attestations",
        "every tick against its commit and signature",
        "speccoder attest verify",
    ));
    out.push(action(
        "review",
        "check the setup",
        "git, the project, the config, the model key",
        "speccoder doctor",
    ));
    out
}

/// `speccoder actions`: what can be done now in the project at `path`
/// (with `selected` the task an interface has selected) and where the
/// feature is on its journey — the list every interface renders. Outside
/// a project: what `report` says there, with an empty list.
#[must_use]
pub fn list(path: &Path, feature: Option<&str>, selected: Option<&str>) -> CommandResult {
    let report = || speccoder_core::ActionsReport {
        actions: Vec::new(),
        journey: journey(path, feature),
    };
    if crate::format::at(path).detect(path).is_none() {
        let status = crate::status::report(path, feature);
        return CommandResult::carried("actions", &status)
            .with_data(CommandData::Actions(Box::new(report())));
    }
    let mut r = report();
    r.actions = available(path, feature, selected);
    let message = match speccoder_core::actions::primary(&r.actions) {
        Some(a) => format!("{} action(s); next: {}", r.actions.len(), a.label),
        None => format!("{} action(s)", r.actions.len()),
    };
    CommandResult::ok("actions", &message).with_data(CommandData::Actions(Box::new(r)))
}

/// Where the feature is on its journey (spec → plan → tasks → commit → run
/// → review); the report's, or the start of it.
#[must_use]
pub fn journey(path: &Path, feature: Option<&str>) -> Vec<speccoder_core::actions::JourneyStep> {
    match crate::status::report(path, feature).data() {
        Some(CommandData::Report(r)) => r.journey.clone(),
        _ => speccoder_core::actions::journey(speccoder_core::actions::JourneyFacts::default()),
    }
}

/// An interface that takes actions, and so decides which kinds it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    /// The web host's jobs (`POST /api/run`).
    WebJobs,
    /// The line CLI's offer of the next step.
    CliFollow,
    /// The line CLI's offer of a fix speccoder knows.
    CliRemedy,
    /// An editor's fix, over the Agent Client Protocol.
    AcpRemedy,
    /// The dashboard's own keys (its `n` hands off to [`Interface::CliFollow`]).
    Dashboard,
}

impl Interface {
    /// Every interface.
    pub const ALL: &'static [Self] = &[
        Self::WebJobs,
        Self::CliFollow,
        Self::CliRemedy,
        Self::AcpRemedy,
        Self::Dashboard,
    ];

    /// What it answers for a kind it does not offer.
    #[must_use]
    pub fn refusal(self) -> CommandResult {
        match self {
            Self::CliFollow => CommandResult::new(
                "next",
                CommandStatus::UsageError,
                "not a step speccoder takes here",
            ),
            Self::Dashboard => CommandResult::new(
                "tui",
                CommandStatus::UsageError,
                "not something the dashboard does",
            ),
            Self::WebJobs => CommandResult::new(
                "job",
                CommandStatus::UsageError,
                "not something the web app runs",
            ),
            Self::CliRemedy | Self::AcpRemedy => CommandResult::new(
                "fix",
                CommandStatus::UsageError,
                "not a fix speccoder applies here",
            ),
        }
    }
}

/// Whether an interface takes a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// It does.
    Take,
    /// It does not: it answers with [`Interface::refusal`], before anything runs.
    Refuse,
}

/// Which kinds each interface takes: every pair decided, no default, so a
/// new kind does not build until each interface has a decision.
#[must_use]
pub const fn offered(interface: Interface, kind: ActionKind) -> Offer {
    use ActionKind as K;
    match interface {
        Interface::WebJobs => match kind {
            K::Skill
            | K::RunTask
            | K::RunFeature
            | K::Converge
            | K::Commit
            | K::Approve
            | K::Reject
            | K::Accept
            | K::Skip
            | K::Nudge
            | K::Rule
            | K::Review
            | K::IssueComment
            | K::QuickChange
            | K::KeepSetAside
            | K::DropSetAside
            | K::FixSetup
            | K::Feature
            | K::PullRequest
            | K::BugFix
            | K::BugConfirm
            | K::BugRun
            | K::BugKeep
            | K::AllowAlways
            | K::WidenPolicy
            | K::DecideBatch
            | K::OracleTests
            | K::ApproveAndRun
            | K::Comment
            | K::CommentsSend
            | K::RevisionUndo
            | K::CodeNudge => Offer::Take,
            K::Command => Offer::Refuse,
        },
        // Decisions belong to their own prompt; what leaves the machine
        // or asks for words is the person's own command there.
        Interface::CliFollow => match kind {
            K::Skill | K::RunTask | K::RunFeature | K::Converge | K::Commit => Offer::Take,
            K::Approve
            | K::Reject
            | K::Accept
            | K::Skip
            | K::Nudge
            | K::Rule
            | K::Review
            | K::IssueComment
            | K::QuickChange
            | K::KeepSetAside
            | K::DropSetAside
            | K::FixSetup
            | K::Feature
            | K::PullRequest
            | K::BugFix
            | K::BugConfirm
            | K::BugRun
            | K::BugKeep
            | K::AllowAlways
            | K::WidenPolicy
            | K::DecideBatch
            | K::OracleTests
            | K::ApproveAndRun
            | K::Comment
            | K::CommentsSend
            | K::RevisionUndo
            | K::CodeNudge
            | K::Command => Offer::Refuse,
        },
        // Its keys decide the queue's calls and switch the feature; `n`
        // hands the next step to the line CLI's offer.
        Interface::Dashboard => match kind {
            K::Approve | K::Reject | K::Feature | K::AllowAlways | K::DecideBatch => Offer::Take,
            K::Skill
            | K::RunTask
            | K::RunFeature
            | K::Converge
            | K::Commit
            | K::Accept
            | K::Skip
            | K::Nudge
            | K::Rule
            | K::Review
            | K::IssueComment
            | K::QuickChange
            | K::KeepSetAside
            | K::DropSetAside
            | K::FixSetup
            | K::PullRequest
            | K::BugFix
            | K::BugConfirm
            | K::BugRun
            | K::BugKeep
            | K::WidenPolicy
            | K::OracleTests
            | K::ApproveAndRun
            | K::Comment
            | K::CommentsSend
            | K::RevisionUndo
            | K::CodeNudge
            | K::Command => Offer::Refuse,
        },
        Interface::CliRemedy | Interface::AcpRemedy => match kind {
            K::FixSetup => Offer::Take,
            K::Skill
            | K::RunTask
            | K::RunFeature
            | K::Converge
            | K::Commit
            | K::Approve
            | K::Reject
            | K::Accept
            | K::Skip
            | K::Nudge
            | K::Rule
            | K::Review
            | K::IssueComment
            | K::QuickChange
            | K::KeepSetAside
            | K::DropSetAside
            | K::Feature
            | K::PullRequest
            | K::BugFix
            | K::BugConfirm
            | K::BugRun
            | K::BugKeep
            | K::AllowAlways
            | K::WidenPolicy
            | K::DecideBatch
            | K::OracleTests
            | K::ApproveAndRun
            | K::Comment
            | K::CommentsSend
            | K::RevisionUndo
            | K::CodeNudge
            | K::Command => Offer::Refuse,
        },
    }
}

/// An action to take: what runs, the person's input, the interface it
/// came through and how that interface runs it (who it asks, where
/// progress goes, its cancel flag).
#[derive(Debug)]
pub struct Dispatch<'a> {
    /// What runs.
    pub run: &'a Runnable,
    /// The person's input (a reason, a note, a skill's arguments); blank
    /// is none.
    pub input: Option<&'a str>,
    /// The interface, as what the person decides is recorded.
    pub via: Via,
    /// The interface's approver, progress and cancellation.
    pub options: RunOptions,
}

impl<'a> Dispatch<'a> {
    /// `run` through `via`, without input, asking nobody.
    #[must_use]
    pub fn new(run: &'a Runnable, via: Via) -> Self {
        Self {
            run,
            input: None,
            via,
            options: RunOptions::default(),
        }
    }
}

/// [`perform`], if `interface` offers the action's kind; `None`, with
/// nothing run, if it does not.
#[must_use]
pub fn perform_for(
    interface: Interface,
    root: &Path,
    feature: Option<&str>,
    d: Dispatch<'_>,
) -> Option<CommandResult> {
    (offered(interface, d.run.kind()) == Offer::Take).then(|| perform(root, feature, d))
}

/// Takes the action: the only place a [`Runnable`] becomes runtime calls.
/// It never prompts and never confirms (an interface has, before); what
/// an interface does not offer it refuses before calling this
/// ([`offered`]).
#[must_use]
pub fn perform(root: &Path, feature: Option<&str>, d: Dispatch<'_>) -> CommandResult {
    perform_in(root, feature, d, crate::worktree::state_dir().as_deref())
}

/// [`perform`] with the state directory given (`None`: there is none).
#[must_use]
pub fn perform_in(
    root: &Path,
    feature: Option<&str>,
    d: Dispatch<'_>,
    state: Option<&Path>,
) -> CommandResult {
    use crate::approvals::{Choice, Verb};
    let needs = |what: &str| {
        CommandResult::new(
            "job",
            CommandStatus::UsageError,
            &format!("{what} is needed"),
        )
    };
    let no_state = || CommandResult::new("job", CommandStatus::ConfigError, "no state directory");
    let decide = |verb| crate::approvals::decide_in(root, feature, verb, state, d.via);
    let options = &d.options;
    let via = d.via;
    let text = d.input.map(str::trim).filter(|t| !t.is_empty());
    match d.run {
        Runnable::Skill { name, arguments } => skill(root, feature, name, text, arguments, options),
        Runnable::Run { task: Some(t) } => run_task(root, feature, t, options),
        Runnable::Run { task: None } => run_feature(root, feature, options, false),
        Runnable::Converge => run_feature(root, feature, options, true),
        Runnable::Commit { paths } => crate::hosting::commit_planning(root, feature, paths),
        Runnable::Approve { task, call, digest } => decide(Verb::Decide(
            Choice::approve(task, text).shown(call, digest),
        )),
        Runnable::Reject { task, call, digest } => match text {
            Some(why) => decide(Verb::Decide(Choice::reject(task, why).shown(call, digest))),
            None => needs("a reason"),
        },
        Runnable::Accept { task } => match (text, state) {
            (Some(why), Some(s)) => crate::controls::accept_in(root, feature, task, why, s, false),
            (None, _) => needs("a reason"),
            (_, None) => no_state(),
        },
        Runnable::Skip { task } => match text {
            Some(why) => crate::controls::skip(root, feature, task, Some(why)),
            None => needs("a reason"),
        },
        Runnable::Nudge { task } => match (text, state) {
            (Some(note), Some(s)) => {
                crate::controls::nudge_with(root, feature, task, Some(note), s)
            }
            (None, _) => needs("a note"),
            (_, None) => no_state(),
        },
        Runnable::Rule { task } => match (text, state) {
            (Some(why), Some(s)) => crate::controls::rule(root, feature, task, why, None, s),
            (None, _) => needs("a reason"),
            (_, None) => no_state(),
        },
        Runnable::Review { ids, verdict } => match (verdict.as_str(), ids.first(), text) {
            ("answer", Some(q), Some(t)) => crate::review::answer(root, feature, q, t),
            ("edit" | "edit-approve", Some(id), Some(t)) => {
                crate::review::edit(root, feature, id, t, verdict == "edit-approve", via)
            }
            ("answer" | "edit" | "edit-approve", _, _) => needs("the words"),
            _ => crate::review::decide(root, feature, ids, verdict, text, via),
        },
        Runnable::IssueComment { issue, assign } => crate::github::comment(
            &crate::github::Ctx::from_env(),
            root,
            feature,
            &issue.to_string(),
            *assign,
        ),
        Runnable::QuickChange {
            request,
            checks,
            run,
        } => quick_change(root, request, checks, *run, via, options),
        Runnable::KeepSetAside { task } => crate::controls::restore(root, feature, task),
        Runnable::DropSetAside { task } => crate::controls::drop_set_aside(root, feature, task),
        Runnable::FixSetup => crate::code::fix_setup(root, feature),
        Runnable::Feature => match text {
            Some(name) => crate::current::switch(root, name),
            None => needs("a feature"),
        },
        Runnable::BugFix {
            symptom,
            expected,
            unchanged,
            issue,
            tests,
            candidate,
            slug,
            run,
        } => bug_fix(
            root,
            &crate::bug::Request::of(
                symptom,
                expected,
                unchanged,
                *issue,
                tests,
                candidate.as_deref(),
                slug.as_deref(),
                *run,
            ),
            via,
            state,
            options,
        ),
        Runnable::BugConfirm {
            task,
            digest,
            reject,
        } => match (*reject, text) {
            (true, None) => needs("a reason"),
            (true, Some(why)) => bug_confirm(root, task, digest, Some(why), via, state, options),
            (false, _) => bug_confirm(root, task, digest, None, via, state, options),
        },
        Runnable::BugRun { slug, task } => {
            bug_run(root, slug.as_deref(), task.as_deref(), state, options)
        }
        Runnable::BugKeep { task } => match text {
            Some(why) => crate::bug::keep(root, task, why, state),
            None => needs("a reason"),
        },
        Runnable::AllowAlways { task, call } => {
            crate::approvals::allow_always(root, feature, task, call, state, via)
        }
        Runnable::WidenPolicy {
            key,
            prefix,
            digest,
        } => crate::approvals::coverage::widen(root, state, *key, prefix, digest, via),
        Runnable::DecideBatch {
            run,
            verdict,
            calls,
            digest,
            resume_asking,
        } => {
            let approve = *verdict == speccoder_core::actions::BatchVerdict::Approve;
            if !approve && text.is_none() {
                return needs("a reason");
            }
            crate::approvals::decide_batch(
                root,
                feature,
                crate::approvals::BatchChoice {
                    run,
                    approve,
                    calls,
                    digest,
                    resume_asking: *resume_asking,
                    reason: text,
                },
                state,
                via,
            )
        }
        Runnable::OracleTests {
            req,
            op,
            path,
            scenario,
            reading,
        } => crate::oracle::tests(
            root,
            feature,
            &crate::oracle::Request {
                req,
                op: *op,
                path: path.as_deref(),
                scenario: scenario.as_deref(),
                reading: *reading,
                text: d.input.filter(|t| !t.trim().is_empty()),
            },
            via,
            state,
            options,
        ),
        Runnable::ApproveAndRun {
            mode,
            grants,
            sheet,
        } => crate::review::approve_and_run(
            root,
            feature,
            *mode,
            grants,
            sheet.as_deref(),
            via,
            state,
            None,
            options,
        ),
        Runnable::Comment { op, anchor, id } => crate::comments::comment(
            root,
            feature,
            *op,
            anchor.as_ref(),
            id.as_deref(),
            text,
            via,
            state,
        ),
        Runnable::CommentsSend { ids } => {
            crate::comments::send(root, feature, ids, via, state, options)
        }
        Runnable::RevisionUndo { turn } => {
            crate::comments::undo(root, feature, turn.as_deref(), state)
        }
        Runnable::CodeNudge { task, comments } => {
            crate::comments::code_nudge(root, feature, task, comments, state)
        }
        // The interface has confirmed it (the action's `confirm`): push
        // and open.
        Runnable::PullRequest => crate::handoff::pr(
            root,
            feature,
            &crate::handoff::PrOptions {
                remote: "origin",
                base: None,
                open: true,
                force: false,
                github: &crate::github::Ctx::from_env(),
            },
        ),
    }
}

/// The calls a person answered Always Allow inline: each one's exact argv
/// joins the allow list through [`perform`] with the interface it was
/// answered through. What each did, as notices.
#[must_use]
pub fn allow_always_inline(
    root: &Path,
    feature: &str,
    always: &[crate::substrate::Always],
    state: &Path,
) -> Vec<String> {
    always
        .iter()
        .map(|a| {
            let task = a.task.rsplit('/').next().unwrap_or(&a.task).to_owned();
            let run = Runnable::AllowAlways {
                task,
                call: a.call.clone(),
            };
            perform_in(root, Some(feature), Dispatch::new(&run, a.via), Some(state))
                .message()
                .to_owned()
        })
        .collect()
}

// The arms that run the model, one function each, so a build without the
// engine can leave them out without a second dispatcher.

/// A planning skill: the person's input, else the arguments it carries.
fn skill(
    root: &Path,
    feature: Option<&str>,
    name: &str,
    input: Option<&str>,
    carried: &str,
    options: &RunOptions,
) -> CommandResult {
    crate::hosting::run(root, feature, name, input.unwrap_or(carried), options)
}

fn run_task(root: &Path, feature: Option<&str>, task: &str, options: &RunOptions) -> CommandResult {
    crate::run(root, feature, task, options)
}

fn run_feature(
    root: &Path,
    feature: Option<&str>,
    options: &RunOptions,
    converge: bool,
) -> CommandResult {
    crate::run_feature(root, feature, options, converge)
}

fn bug_fix(
    root: &Path,
    req: &crate::bug::Request,
    via: Via,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    crate::bug::bug(root, req, via, state, options)
}

fn bug_confirm(
    root: &Path,
    task: &str,
    digest: &str,
    reject: Option<&str>,
    via: Via,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    crate::bug::confirm(root, task, digest, reject, true, via, state, options)
}

fn bug_run(
    root: &Path,
    slug: Option<&str>,
    task: Option<&str>,
    state: Option<&Path>,
    options: &RunOptions,
) -> CommandResult {
    crate::bug::run(root, slug, task, state, options)
}

fn quick_change(
    root: &Path,
    request: &str,
    checks: &[String],
    run: bool,
    via: Via,
    options: &RunOptions,
) -> CommandResult {
    crate::quick::change(root, request, checks, run, via, options)
}

#[cfg(test)]
mod tests {
    use speccoder_core::actions::primary;

    use super::*;

    #[test]
    fn every_interface_decides_every_kind() {
        let mut taken = 0;
        for i in Interface::ALL {
            for k in ActionKind::ALL {
                if offered(*i, *k) == Offer::Take {
                    taken += 1;
                } else {
                    assert_ne!(i.refusal().status(), CommandStatus::Ok, "{i:?}");
                }
            }
        }
        // The web host runs every runnable kind; the terminal the steps
        // it offers; the remedies only the setup's fix.
        let runnable = Runnable::examples().len();
        // The dashboard also decides Always Allow and batches.
        assert_eq!(taken, runnable + 5 + 5 + 2);
        assert_eq!(
            offered(Interface::CliFollow, ActionKind::PullRequest),
            Offer::Refuse
        );
        assert_eq!(
            offered(Interface::AcpRemedy, ActionKind::FixSetup),
            Offer::Take
        );
    }

    #[test]
    fn blank_input_is_no_input() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let run = Runnable::Reject {
            task: "T001".to_owned(),
            call: "attempt:T001".to_owned(),
            digest: String::new(),
        };
        let r = perform_in(
            dir.path(),
            None,
            Dispatch {
                run: &run,
                input: Some("  "),
                via: Via::Web,
                options: RunOptions::default(),
            },
            None,
        );
        assert_eq!(
            (r.command(), r.status(), r.message()),
            ("job", CommandStatus::UsageError, "a reason is needed")
        );
    }

    #[test]
    fn a_project_outside_git_still_gets_a_list() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let list = available(dir.path(), None, None);
        assert!(list.iter().any(|a| a.command == "speccoder doctor"));
        assert!(primary(&list).is_none());
    }
}
