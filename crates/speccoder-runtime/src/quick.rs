//! `speccoder do "…"` (F-094): a small change without a planning ceremony,
//! and without skipping the evidence. The request becomes a story, a
//! requirement (approved: the person wrote it) and a task in the format's
//! standing unit for quick changes; those planning files are committed;
//! then the task runs like any other — worktree, failing acceptance tests
//! first, checks, a verdict, a commit with its evidence.

use std::path::Path;

use speccoder_core::{CommandResult, CommandStatus};

/// Records `request` (and its acceptance `checks`) and, when `run`, runs it.
#[must_use]
pub fn change(
    path: &Path,
    request: &str,
    checks: &[String],
    run: bool,
    via: &str,
    options: &crate::RunOptions,
) -> CommandResult {
    let format = crate::format::at(path);
    let Some(root) = format.detect(path) else {
        return CommandResult::new(
            "do",
            CommandStatus::NotAProject,
            &(format.messages().not_a_project)(path),
        )
        .with_code("NO-PROJECT");
    };
    let rec = match format.record_change(&root, request, checks) {
        Ok(r) => r,
        Err(e) => {
            return CommandResult::new("do", CommandStatus::UsageError, &e).with_code("USAGE");
        }
    };
    // The person's own words: approved as they wrote them.
    let approved = crate::review::decide(
        &root,
        Some(&rec.feature),
        std::slice::from_ref(&rec.requirement),
        "approve",
        None,
        via,
    );
    if approved.status() != CommandStatus::Ok {
        return CommandResult::carried("do", &approved);
    }
    // A run reads committed files: exactly this unit's planning files.
    let files = crate::hosting::uncommitted(&root, Some(&rec.feature));
    if !files.is_empty() {
        let committed = crate::hosting::commit_planning_as(
            &root,
            Some(&rec.feature),
            &files,
            Some(&format!("quick change: {}", rec.title)),
        );
        if committed.status() != CommandStatus::Ok {
            return CommandResult::carried("do", &committed);
        }
    }
    let said = format!(
        "recorded \"{}\" as {} and {} in {}",
        rec.title, rec.requirement, rec.task, rec.feature
    );
    if !run {
        return CommandResult::ok(
            "do",
            &format!(
                "{said}; run it: speccoder run --feature {} --task {}",
                rec.feature, rec.task
            ),
        )
        .with_next(vec![speccoder_core::next::NextStep {
            command: format!(
                "speccoder run --feature {} --task {}",
                rec.feature, rec.task
            ),
            why: "the change, judged on evidence".to_owned(),
        }]);
    }
    let result = crate::run(&root, Some(&rec.feature), &rec.task, options);
    let status = result.status();
    let message = format!("{said}; {}", result.message());
    result.reworded(status, &message)
}
