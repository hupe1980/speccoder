//! The web host's one way into the runtime: every entry point it uses,
//! re-exported or delegated without changing a result. A write the host
//! serves goes through [`actions::perform`], like every interface's; a
//! later feature adds read views here as delegates, never a second write
//! path. The layering test holds the host to this module.

use std::path::Path;

pub use crate::RunOptions;

/// The action list and the one dispatcher.
pub mod actions {
    pub use crate::actions::{Dispatch, Interface, Offer, available, list, offered, perform};
}

/// The person's own commands, run for them (a fix speccoder printed), and
/// a terminal at the project.
pub mod person {
    pub use crate::person::{Stream, TIMEOUT, open_terminal, run, sandboxed, shown, speccoder};
}

/// What went wrong, as a block to copy; the setup as a bug report.
pub mod copy {
    pub use crate::copy::block;
    pub use crate::doctor::bug_report;
}

/// Calls waiting for a person, and who answers them.
pub mod approvals {
    pub use crate::approvals::{Answer, Approver, Presentation, Who, list};
}

/// A run's progress, as it happens and as another process left it.
pub mod progress {
    pub use crate::progress::{Event, Progress, read_live};
}

/// Read views.
pub mod status {
    pub use crate::status::report;
}

/// The next steps a result names.
pub mod next {
    pub use crate::next::attach;
}

/// The person's review of the spec.
pub mod review {
    /// The comment tray and the last revision turn.
    pub use crate::comments::list as comments;
    pub use crate::review::{readiness, report};
}

/// Planning a new feature: the rung a description suggests, the
/// requirements it is close to (read-only, no model).
pub mod planning {
    pub use crate::planning_chain::{similar_command, similar_to, suggest};
}

/// A new project: its plan (read-only), the idea offered once.
pub mod new_project {
    pub use crate::new_project::{Creation, creation, first_feature, plan, undo_offer};
    pub use speccoder_core::new_project::{NewChoices, Stack, slug};
}

/// What a host outside a project shows.
pub mod welcome {
    pub use crate::welcome::{
        Recent, RecentView, default_parent, folders, forget, is_project, remember, tilde, views,
    };
}

/// The bundled sample.
pub mod sample {
    pub use crate::sample::{available, copy, home, missing_tool};
}

/// Needs You.
pub mod inbox {
    pub use crate::inbox::inbox;
}

/// A bug as speccoder keeps it (read-only; a bug's actions are kinds).
pub mod bug {
    pub use crate::bug::show;
}

/// A task's evidence.
pub mod evidence {
    pub use crate::evidence::{account_of, banner, rows, statements};
}

/// A task's evidence tour (`evidence_tour`), read as recorded.
pub mod tour {
    use std::path::Path;

    use speccoder_core::{CommandData, CommandResult, CommandStatus};

    /// The tour of `task` of `feature` (default: the selected feature) at
    /// `HEAD`: [`crate::tour::command`] without trust, so it shows what the
    /// statements and the commit say.
    #[must_use]
    pub fn of_task(root: &Path, feature: Option<&str>, task: &str) -> CommandResult {
        let selected = || match crate::inspect(root, None).data() {
            Some(CommandData::Inspect(snap)) => {
                snap.feature.as_ref().map(|f| f.feature.name.clone())
            }
            _ => None,
        };
        let Some(feature) = feature.map(str::to_owned).or_else(selected) else {
            return CommandResult::new(
                "tour",
                CommandStatus::SelectionNeeded,
                "no feature selected",
            );
        };
        crate::tour::command(
            root,
            &feature,
            task,
            &crate::attest::branch::Options::default(),
            "tour",
        )
    }
}

/// The feature's documents.
pub mod documents {
    pub use crate::documents::{of_feature, render};
}

/// The machine and the project, checked.
pub mod doctor {
    pub use crate::doctor::{agents, run};
}

/// `speccoder.toml`.
pub mod settings {
    /// What a prefix entry would newly allow (`config coverage`).
    pub use crate::approvals::coverage::show as coverage;
    pub use crate::settings::{apply, commit, show};
}

/// Tokens and money.
pub mod controls {
    pub use crate::controls::{SkillCost, cost, skill_cost};
}

/// The code view: the branch's change, its files.
pub mod code {
    pub use crate::code::{changes, file, tree, worktree_file, worktree_view};
}

/// Setting up a project.
pub mod init {
    pub use crate::init::{commit_written, finish, prepare, preview, root_of};
}

/// The features.
pub mod current {
    pub use crate::current::list;
}

/// The pull request, previewed.
pub mod handoff {
    pub use crate::handoff::{PrOptions, pr};
}

/// Where speccoder keeps its state.
pub mod worktree {
    pub use crate::worktree::state_dir;
}

/// GitHub: sign-in, issues, the branch's pull request.
pub mod github {
    pub use crate::github::{
        Ctx, RepoEntry, description, issue, issues, login_with_token, pr_for, reach, refused, repo,
        repositories, started, status, valid_slug,
    };

    /// Signing in on a device.
    pub mod auth {
        pub use crate::github::auth::{Pending, client_id, start, wait};
    }

    /// GitHub's addresses.
    pub mod http {
        pub use crate::github::http::oauth;
    }
}

/// How the project's format words things, for the page's header.
#[derive(Debug, Clone, Copy)]
pub struct FormatWords {
    /// The format (`Spec Kit`).
    pub name: &'static str,
    /// What it calls a unit of work (`feature`, `change`).
    pub unit: &'static str,
}

/// The words of the format of the project at `root`.
#[must_use]
pub fn format_words(root: &Path) -> FormatWords {
    let format = crate::format::at(root);
    FormatWords {
        name: format.wording().name,
        unit: format.messages().unit,
    }
}

/// The format's planning chain, by installed skill name, for the New
/// Feature sheet: what creates a unit, what leads to the task list, what
/// checks it and what revises a written spec.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanningWords {
    /// The creating skill (`speckit-specify`).
    pub creates: Option<String>,
    /// The skills toward the task list after it (`speckit-plan`, `speckit-tasks`).
    pub toward: Vec<String>,
    /// The checking skill (`speckit-analyze`).
    pub checks: Option<String>,
    /// The revising skill (`speckit-clarify`).
    pub revises: Option<String>,
}

/// [`PlanningWords`] of the project at `root`.
#[must_use]
pub fn planning_words(root: &Path) -> PlanningWords {
    let p = crate::format::at(root).planning();
    let full = |s: &str| format!("{}{s}", p.prefix);
    let creates = p.creates.first().map(|c| full(c));
    PlanningWords {
        toward: (p.toward_tasks)(false)
            .iter()
            .map(|s| full(s))
            .filter(|s| Some(s) != creates.as_ref())
            .collect(),
        creates,
        checks: p.checks.map(full),
        revises: p.revises.map(full),
    }
}

/// The installed name of the format's skill that creates a unit, if it has one.
#[must_use]
pub fn create_skill(root: &Path) -> Option<String> {
    let planning = crate::format::at(root).planning();
    planning
        .creates
        .first()
        .map(|c| format!("{}{c}", planning.prefix))
}

/// One commit's patch, if `commit` names a commit (a blob or a tree id
/// from elsewhere in the repository is never shown); empty otherwise.
#[must_use]
pub fn commit_patch(root: &Path, commit: &str) -> String {
    commit_patch_of(root, commit)
}

/// Whether `id` is reached from `HEAD`, a branch or a saved attempt.
fn reachable(git: &crate::git::Git, id: &str) -> bool {
    if git
        .succeeds(&["merge-base", "--is-ancestor", id, "HEAD"])
        .unwrap_or(false)
    {
        return true;
    }
    git.line(&[
        "for-each-ref",
        "--count=1",
        "--format=%(refname)",
        "--contains",
        id,
        "refs/heads",
        "refs/speccoder/attempts",
    ])
    .is_ok_and(|l| !l.trim().is_empty())
}

fn commit_patch_of(root: &Path, commit: &str) -> String {
    let git = crate::git::Git::at(root);
    // `^{commit}` peels or fails.
    let Ok(id) = git.line(&[
        "rev-parse",
        "--verify",
        "-q",
        &format!("{commit}^{{commit}}"),
    ]) else {
        return String::new();
    };
    // Only what a branch, `HEAD` or a saved attempt reaches: a commit the
    // page could only name by its id (a sealed or unreachable object) shows
    // nothing.
    if !reachable(&git, &id) {
        return String::new();
    }
    git.run(&["show", "--no-color", "--patch", "--format=%h %s%n%n%b", &id])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}
