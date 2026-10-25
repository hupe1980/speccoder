//! Parallel lanes for `[P]` tasks (D-053).

use std::path::Path;

use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{git, sandbox, verify, worktree};

/// Lane worktrees are created one at a time (git's worktree bookkeeping).
pub(crate) static LANE_SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs each task of `group` in its own lane, at once. Lanes never ask
/// inline (prompts would interleave): a call that needs approval stops the
/// lane, and the person decides with `speccoder report --approvals`.
pub(crate) fn run_lanes(
    path: &Path,
    feature: &str,
    group: &[String],
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> Vec<(String, CommandResult)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = group
            .iter()
            .map(|task| {
                let lane_options = RunOptions {
                    max_tasks: None,
                    approver: None,
                    progress: options.progress.clone(),
                    parallel: 1,
                    lane: Some(task.clone()),
                    cancel: None,
                    ignore_checklists: options.ignore_checklists,
                    fresh_acceptance: false,
                    run_grants: std::sync::Arc::clone(&options.run_grants),
                    in_place: false,
                    session: None,
                };
                let provider = std::sync::Arc::clone(provider);
                scope.spawn(move || {
                    let res = run_task(
                        path,
                        Some(feature),
                        task,
                        state,
                        provider,
                        secret,
                        &lane_options,
                    );
                    (task.clone(), res)
                })
            })
            .collect();
        handles
            .into_iter()
            .zip(group)
            .map(|(h, task)| {
                h.join().unwrap_or_else(|_| {
                    (
                        task.clone(),
                        CommandResult::new("run", CommandStatus::Error, "the lane panicked"),
                    )
                })
            })
            .collect()
    })
}

/// What merging a parallel group did.
#[derive(Default)]
pub(crate) struct LanesMerged {
    pub(crate) reports: Vec<speccoder_core::RunReport>,
    pub(crate) lines: Vec<String>,
    pub(crate) one_at_a_time: Vec<String>,
    pub(crate) reverted: bool,
    pub(crate) stop: Option<(String, CommandStatus, String)>,
}

/// Merges each passed lane's commit into the feature branch, then runs the
/// checks on the merged whole: tasks that pass alone can fail together.
pub(crate) fn merge_lanes(
    path: &Path,
    feature_wt: &Path,
    project: &Path,
    feature: &str,
    lanes: Vec<(String, CommandResult)>,
) -> LanesMerged {
    let mut out = LanesMerged::default();
    let g = git::Git::at(feature_wt);
    let pre = g.line(&["rev-parse", "HEAD"]).unwrap_or_default();
    let repo_root = git::Git::at(path).toplevel().ok().flatten();
    let (runner, _) = sandbox::runner_at(feature_wt).unwrap_or((sandbox::Runner::Plain, None));
    let tier = Some(verify::config::Tier::Fast);
    let baseline = verify::run_checks(project, tier, &runner).ok();
    let mut merged: Vec<String> = Vec::new();
    // (task, the lane's commit, the commit on the feature branch).
    let mut landed: Vec<(String, String, String)> = Vec::new();
    for (task, res) in lanes {
        let report = match res.data() {
            Some(CommandData::Run(r)) => Some((**r).clone()),
            _ => None,
        };
        match (res.status(), report.as_ref().and_then(|r| r.commit.clone())) {
            (CommandStatus::Ok, Some(commit)) => {
                match merge_lane(feature_wt, project, feature, &task, &commit) {
                    Ok(head) => {
                        merged.push(task.clone());
                        landed.push((task.clone(), commit, head));
                    }
                    Err(e) => {
                        out.lines.push(format!(
                            "{task}: not merged ({e}); it runs again on its own"
                        ));
                        out.one_at_a_time.push(task.clone());
                    }
                }
            }
            (CommandStatus::Ok, None) => {}
            (status, _) => {
                if out.stop.is_none() {
                    out.stop = Some((task.clone(), status, res.message().to_owned()));
                }
            }
        }
        if let Some(r) = report {
            out.reports.push(r);
        }
        if let Some(root) = &repo_root {
            let _ = worktree::remove_lane(root, feature, &task);
        }
    }
    if merged.is_empty() {
        return out;
    }
    let after = verify::run_checks(project, tier, &runner).ok();
    let together = match (&baseline, &after) {
        (Some(b), Some(a)) => {
            speccoder_core::verify::passes(&a.checks, Some(&speccoder_core::verify::compare(b, a)))
        }
        _ => false,
    };
    if together {
        // The attestation follows the work onto the feature branch (F-061).
        let attest = crate::verify::find_config(project)
            .and_then(|d| std::fs::read_to_string(d.join(verify::config::FILE)).ok())
            .and_then(|c| crate::substrate::run_config::parse(&c).ok())
            .map(|c| c.attest)
            .filter(|a| a.enabled);
        if let Some(attest) = &attest {
            for (task, lane_commit, head) in &landed {
                let a = crate::attest::carry(feature_wt, lane_commit, head, attest);
                if let Some(r) = out.reports.iter_mut().find(|r| r.task == *task) {
                    r.commit = Some(head.clone());
                    r.attestation = Some(a);
                }
            }
        }
        out.lines.push(format!(
            "{} ran in parallel, merged and passed the checks together",
            merged.join(", ")
        ));
    } else {
        // The feature branch goes back to before the group; the lanes'
        // work stays in their reports.
        let _ = g.run(&["reset", "-q", "--hard", &pre]);
        out.reverted = true;
        out.one_at_a_time.extend(merged.iter().cloned());
        out.lines.push(format!(
            "{} passed alone but not together; the merge was undone and the remaining tasks run one at a time",
            merged.join(", ")
        ));
    }
    out
}

/// Applies a lane's task commit on the feature branch: its changes, and
/// the feature's `tasks.md` with this task ticked (lanes tick neighbouring
/// lines, which git would call a conflict).
pub(crate) fn merge_lane(
    feature_wt: &Path,
    project: &Path,
    feature: &str,
    task: &str,
    commit: &str,
) -> Result<String, String> {
    let g = git::Git::at(feature_wt);
    let ident = [
        "-c",
        "user.name=speccoder",
        "-c",
        "user.email=speccoder@users.noreply.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    let format = crate::format::at(project);
    let tasks_file = format
        .inspect(project, Some(feature))
        .and_then(|s| s.feature)
        .and_then(|f| f.tasks)
        .map(|t| project.join(t.path))
        .ok_or_else(|| format!("the feature has no {}", format.messages().tasks_file))?;
    let tasks_arg = tasks_file.to_string_lossy().into_owned();
    // Untracked files the commit adds were generated here by speccoder's own
    // check runs (a `Cargo.lock`, say); they would stop the cherry-pick.
    let added = g
        .run(&[
            "diff",
            "--name-only",
            "--diff-filter=A",
            &format!("{commit}^"),
            commit,
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let untracked = g
        .run(&["ls-files", "--others", "--exclude-standard"])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    for f in added.lines().filter(|f| untracked.lines().any(|u| u == *f)) {
        let _ = std::fs::remove_file(feature_wt.join(f));
    }
    let mut pick = ident.to_vec();
    pick.extend(["cherry-pick", "-n", commit]);
    let picked = g.run(&pick);
    let conflicted = g
        .run(&["diff", "--name-only", "--diff-filter=U"])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let tasks_rel = tasks_file
        .strip_prefix(feature_wt)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A failure that left no conflict applied nothing (an empty merge would
    // tick a task whose work is missing).
    if let Err(e) = &picked
        && conflicted.trim().is_empty()
    {
        let _ = g.run(&["reset", "-q", "--hard", "HEAD"]);
        return Err(e
            .0
            .lines()
            .next()
            .unwrap_or("cherry-pick failed")
            .to_owned());
    }
    let others: Vec<&str> = conflicted.lines().filter(|f| *f != tasks_rel).collect();
    if !others.is_empty() {
        let _ = g.run(&["reset", "-q", "--hard", "HEAD"]);
        return Err(format!("conflicts in {}", others.join(", ")));
    }
    g.run(&["checkout", "HEAD", "--", &tasks_arg])
        .map_err(|e| e.0)?;
    let line_hash = format
        .inspect(project, Some(feature))
        .and_then(|s| s.feature)
        .and_then(|f| f.tasks)
        .and_then(|t| t.tasks.into_iter().find(|t| t.id == task))
        .map(|t| t.line_hash)
        .ok_or_else(|| {
            format!(
                "{task} is not in the feature's {}",
                format.messages().tasks_file
            )
        })?;
    format
        .tick(&tasks_file, task, &line_hash)
        .map_err(|e| e.to_string())?;
    let mut add = ident.to_vec();
    add.extend(["add", "-A"]);
    g.run(&add).map_err(|e| e.0)?;
    let mut c = ident.to_vec();
    c.extend(["commit", "-q", "--no-verify", "-C", commit]);
    g.run(&c).map_err(|e| e.0)?;
    g.line(&["rev-parse", "HEAD"]).map_err(|e| e.0)
}

/// The worktree of `lane` for `feature`, started where the feature branch
/// (`wt_path`) is now: `(lane root, lane project dir)`.
pub(crate) fn lane_worktree(
    path: &Path,
    state: &Path,
    feature: &str,
    lane: &str,
    wt_path: &str,
    project_dir: &str,
) -> Result<(String, String), String> {
    let start = git::Git::at(Path::new(wt_path))
        .line(&["rev-parse", "HEAD"])
        .map_err(|e| e.0)?;
    let repo_root = match git::Git::at(path).toplevel() {
        Ok(Some(r)) => r,
        _ => return Err("not a git repository".to_owned()),
    };
    let guard = LANE_SETUP.lock();
    let made = worktree::ensure_lane(&repo_root, state, feature, lane, &start);
    drop(guard);
    let l = made.map_err(|e| e.message)?;
    let rel = Path::new(project_dir)
        .strip_prefix(wt_path)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    Ok((
        l.path.display().to_string(),
        l.path.join(rel).display().to_string(),
    ))
}
