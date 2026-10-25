//! What a person can do now (D-087), from the project as it stands: the
//! report, what waits for a decision, the planning files not yet committed
//! and the installed planning skills. Interfaces render the list; the
//! dashboard runs what it can and shows the command for the rest.

use std::path::Path;

use speccoder_core::actions::{Action, Runnable};
use speccoder_core::{ApprovalsReport, CommandData, StatusReport};

fn action(
    group: &str,
    label: impl Into<String>,
    why: impl Into<String>,
    command: impl Into<String>,
) -> Action {
    Action {
        group: group.to_owned(),
        label: label.into(),
        why: why.into(),
        command: command.into(),
        run: None,
        blocked: None,
        input: None,
        confirm: None,
    }
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
            match crate::approvals::decide_with(path, feature, crate::approvals::Verb::List, &state)
                .data()
            {
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
        a.run = Some(Runnable::Skill {
            name: name.to_owned(),
            arguments: String::new(),
        });
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
        a.run = Some(Runnable::Approve {
            task: p.task.clone(),
        });
        out.push(a);
        let mut r = action(
            "next",
            format!("reject {}'s waiting call", p.task),
            "the model is told your reason",
            format!("speccoder task reject {} --reason \"…\"", p.task),
        );
        r.run = Some(Runnable::Reject {
            task: p.task.clone(),
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
        a.run = Some(Runnable::Commit {
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
        a.run = Some(Runnable::Run { task: None });
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
            a.run = Some(Runnable::Run {
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
                a.run = Some(run);
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
        a.run = Some(Runnable::Run { task: None });
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
        a.run = Some(Runnable::Feature);
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
    pr.run = Some(Runnable::PullRequest);
    pr.confirm = Some(
        "Push the feature's speccoder branch and its attestation notes to `origin`, and open (or update) its draft pull request on GitHub through your `gh` login.".to_owned(),
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

/// Where the feature is on its journey (spec → plan → tasks → commit → run
/// → review); the report's, or the start of it.
#[must_use]
pub fn journey(path: &Path, feature: Option<&str>) -> Vec<speccoder_core::actions::JourneyStep> {
    match crate::status::report(path, feature).data() {
        Some(CommandData::Report(r)) => r.journey.clone(),
        _ => speccoder_core::actions::journey(speccoder_core::actions::JourneyFacts::default()),
    }
}

#[cfg(test)]
mod tests {
    use speccoder_core::actions::primary;

    use super::*;

    #[test]
    fn a_project_outside_git_still_gets_a_list() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let list = available(dir.path(), None, None);
        assert!(list.iter().any(|a| a.command == "speccoder doctor"));
        assert!(primary(&list).is_none());
    }
}
