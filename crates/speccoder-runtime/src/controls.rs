//! A person's controls over single tasks: `accept`, `skip`,
//! `nudge` — and `cost`, what the work used.
//!
//! What a person decides is recorded where speccoder's other durable facts
//! are: an acceptance is a commit on `speccoder/<feature>` (`Verdict:
//! attested` — a person's word, never shown as evidence); a skip is a ref
//! under `refs/speccoder/skipped/`; a nudge is a task note in the state
//! directory that rides in the attempt's journaled input.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use speccoder_core::{
    CommandData, CommandResult, CommandStatus, ControlReport, CostReport, TaskCost, Usage,
};

use crate::git::Git;

/// Where skips live.
pub const SKIPPED: &str = "refs/speccoder/skipped";

const FS: char = '\u{1f}';
const RS: char = '\u{1e}';

/// One line: trailers and subjects never span lines.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The person, as git knows them.
fn person(root: &Path) -> String {
    Git::at(root)
        .line(&["config", "user.name"])
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "a person".to_owned())
}

fn done(command: &str, report: ControlReport, message: &str) -> CommandResult {
    CommandResult::new(command, CommandStatus::Ok, message)
        .with_data(CommandData::Control(Box::new(report)))
}

/// The selected feature and the project root of the user's checkout.
fn locate(
    command: &str,
    path: &Path,
    feature: Option<&str>,
) -> Result<(String, PathBuf), Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::new(
            command,
            inspected.status(),
            inspected.message(),
        )));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    Ok((f.feature.name.clone(), PathBuf::from(&snapshot.root)))
}

/// The feature's open tasks as `report` sees them (branch first).
fn open_tasks(root: &Path, feature: &str) -> Vec<String> {
    match crate::status::report(root, Some(feature)).data() {
        Some(CommandData::Report(r)) => r
            .tasks
            .iter()
            .filter(|t| matches!(t.state.as_str(), "ready" | "blocked" | "skipped"))
            .map(|t| t.id.clone())
            .collect(),
        _ => Vec::new(),
    }
}

// ── accept ─────────────────────────────────────────────────────────────────

/// `speccoder accept --task T --reason R`: a person attests the task is
/// done. speccoder ticks it and commits `Verdict: attested`.
pub fn accept_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    reason: &str,
    state: &Path,
) -> CommandResult {
    let refuse = |status, m: String| CommandResult::new("accept", status, &m);
    let wt = crate::worktree_with(path, feature, state);
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return CommandResult::new("accept", wt.status(), wt.message());
    };
    if wt.status() != CommandStatus::Ok {
        return CommandResult::new("accept", wt.status(), wt.message());
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return refuse(CommandStatus::Error, "worktree without a path".to_owned());
    };
    let wt_root = PathBuf::from(&wt_path);
    if crate::worktree::require_clean(&wt_root).is_err() {
        return refuse(
            CommandStatus::GitPrecondition,
            format!(
                "the worktree {wt_path} has uncommitted changes; the next `speccoder run` sets them aside, or commit them yourself"
            ),
        );
    }
    let project = PathBuf::from(&project_dir);
    let Ok(snap) = speccoder_speckit::inspect(&project, Some(&w.feature)) else {
        return refuse(
            CommandStatus::NotAProject,
            "the worktree has no Spec Kit project".to_owned(),
        );
    };
    let Some(tasks) = snap.feature.as_ref().and_then(|f| f.tasks.as_ref()) else {
        return refuse(
            CommandStatus::NotFound,
            format!("{} has no tasks.md", w.feature),
        );
    };
    let Some(t) = tasks.tasks.iter().find(|t| t.id == task) else {
        return refuse(
            CommandStatus::NotFound,
            format!("no task {task} in {}", w.feature),
        );
    };
    if t.done {
        return refuse(
            CommandStatus::TaskNotReady,
            format!("{task} is already done"),
        );
    }
    if let Err(e) =
        speccoder_speckit::mark_task_complete(&project.join(&tasks.path), task, &t.line_hash)
    {
        return refuse(CommandStatus::Error, e);
    }
    let reason = one_line(reason);
    let by = person(&project);
    let message = format!(
        "speccoder: {task} accepted by {by}\n\n{}\n\n{reason}\n\nSpec-Task: {}/{task}\nVerdict: attested\nAccepted-By: {by}\nReason: {reason}\n",
        t.title, w.feature
    );
    match crate::commit_attempt(&wt_root, &message) {
        Ok(commit) => done(
            "accept",
            ControlReport {
                feature: w.feature.clone(),
                task: task.to_owned(),
                action: "accepted".to_owned(),
                text: Some(reason),
                record: Some(commit),
            },
            &format!("{task} ticked as attested by {by} (not verified); `report` shows it as such"),
        ),
        Err(e) => refuse(CommandStatus::Error, format!("commit failed: {e}")),
    }
}

// ── skip ───────────────────────────────────────────────────────────────────

/// Skipped tasks of `feature` with their reasons.
#[must_use]
pub fn skipped(root: &Path, feature: &str) -> BTreeMap<String, String> {
    let format = format!("--format=%(refname){FS}%(trailers:key=Reason,valueonly){RS}");
    let prefix = format!("{SKIPPED}/{feature}/");
    let out = Git::at(root)
        .run(&["for-each-ref", &format, &prefix])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    out.split(RS)
        .filter_map(|r| {
            let (name, reason) = r.trim_start_matches('\n').split_once(FS)?;
            Some((
                name.strip_prefix(&prefix)?.to_owned(),
                reason.trim().to_owned(),
            ))
        })
        .collect()
}

/// `speccoder skip --task T --reason R` (or `--undo`): the task is neither
/// done nor attempted; the feature loop passes it by, and what depends on it
/// stays blocked.
pub fn skip(path: &Path, feature: Option<&str>, task: &str, reason: Option<&str>) -> CommandResult {
    let (feature, root) = match locate("skip", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let git = Git::at(&root);
    let reference = format!("{SKIPPED}/{feature}/{task}");
    let report = |action: &str, text: Option<String>| ControlReport {
        feature: feature.clone(),
        task: task.to_owned(),
        action: action.to_owned(),
        text,
        record: Some(reference.clone()),
    };
    let Some(reason) = reason else {
        let _ = git.run(&["update-ref", "-d", &reference]);
        return done(
            "skip",
            report("unskipped", None),
            &format!("{task} is no longer skipped"),
        );
    };
    if !open_tasks(&root, &feature).iter().any(|t| t == task) {
        return CommandResult::new(
            "skip",
            CommandStatus::NotFound,
            &format!("{task} is not an open task of {feature}"),
        );
    }
    let reason = one_line(reason);
    let by = person(&root);
    let made = git
        .line(&["rev-parse", "HEAD^{tree}"])
        .and_then(|tree| {
            git.line(&[
                "-c",
                "user.name=speccoder",
                "-c",
                "user.email=speccoder@users.noreply.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit-tree",
                &tree,
                "-m",
                &format!("speccoder: skip {task}\n\n{reason}\n\nSpec-Task: {feature}/{task}\nSkipped-By: {by}\nReason: {reason}\n"),
            ])
        })
        .and_then(|commit| git.run(&["update-ref", &reference, &commit]));
    match made {
        Ok(_) => done(
            "skip",
            report("skipped", Some(reason)),
            &format!(
                "{task} skipped: runs pass it by, and tasks depending on it stay blocked (`skip --task {task} --undo` to take it back)"
            ),
        ),
        Err(e) => CommandResult::new("skip", CommandStatus::GitPrecondition, &e.0),
    }
}

// ── nudge ──────────────────────────────────────────────────────────────────

/// `speccoder nudge --task T "note"` (or `--clear`): a person's note for the
/// task's next attempts, journaled with them.
pub fn nudge_with(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    note: Option<&str>,
    state: &Path,
) -> CommandResult {
    let (feature, root) = match locate("nudge", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let dir = crate::worktree::project_state_dir(state, &root);
    let mut decisions = match crate::approvals::Decisions::load(&dir) {
        Ok(d) => d,
        Err(e) => return CommandResult::new("nudge", CommandStatus::ConfigError, &e),
    };
    let entry = decisions
        .tasks
        .entry(format!("{feature}/{task}"))
        .or_default();
    let (action, message) = match note {
        Some(n) => {
            entry.nudges.push(n.trim().to_owned());
            (
                "nudged",
                format!("{task}'s next attempts will read: {}", n.trim()),
            )
        }
        None => {
            entry.nudges.clear();
            ("nudges_cleared", format!("{task} has no notes any more"))
        }
    };
    if let Err(e) = decisions.save(&dir) {
        return CommandResult::new("nudge", CommandStatus::Error, &e);
    }
    done(
        "nudge",
        ControlReport {
            feature,
            task: task.to_owned(),
            action: action.to_owned(),
            text: note.map(|n| n.trim().to_owned()),
            record: None,
        },
        &message,
    )
}

// ── cost ───────────────────────────────────────────────────────────────────

fn add(total: &mut Usage, u: &serde_json::Value) {
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    total.input_tokens += n("input_tokens");
    total.output_tokens += n("output_tokens");
    total.cache_read_tokens += n("cache_read_tokens");
    total.cache_write_tokens += n("cache_write_tokens");
    if let Some(c) = u["cost_usd"].as_f64() {
        total.usd = Some(total.usd.unwrap_or(0.0) + c);
    }
}

/// Prices `u` from `[model]`, unless its cost was reported (an external
/// agent's own figure, which the configured prices do not describe).
fn price(u: &mut Usage, prices: Option<crate::substrate::model_config::Prices>) {
    #[allow(clippy::cast_precision_loss)] // token counts far below 2^52
    if let Some(p) = prices.filter(|_| u.usd.is_none()) {
        let fresh = u.input_tokens.saturating_sub(u.cache_read_tokens) as f64;
        u.usd = Some(
            (fresh * p.input
                + u.cache_read_tokens as f64 * p.cache_read
                + u.output_tokens as f64 * p.output)
                / 1_000_000.0,
        );
    }
}

/// `speccoder cost`: tokens per task — every attempt recorded in git, the
/// passing one and the saved ones — plus convergence, priced when `[model]`
/// states prices.
pub fn cost(path: &Path, feature: Option<&str>) -> CommandResult {
    let (feature, root) = match locate("cost", path, feature) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let model = crate::verify::find_config(&root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .and_then(|t| crate::substrate::model_config::parse(&t).ok().flatten());
    let prices = model.as_ref().and_then(|m| m.prices);

    let branch = format!("speccoder/{feature}");
    let format = format!(
        "--format=%s{FS}%(trailers:key=Spec-Task,valueonly){FS}%(trailers:key=Usage,valueonly){RS}"
    );
    let log = Git::at(&root)
        .run(&["log", &format, &branch])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let mut per_task: BTreeMap<String, (usize, Usage)> = BTreeMap::new();
    let mut convergence = Usage::default();
    for record in log.split(RS) {
        let f: Vec<&str> = record.trim_start_matches('\n').split(FS).collect();
        let [subject, task, usage] = f[..] else {
            continue;
        };
        let Some(u) = crate::attempts::parse_usage(usage.trim()) else {
            continue;
        };
        if let Some(t) = task.trim().strip_prefix(&format!("{feature}/")) {
            let e = per_task.entry(t.to_owned()).or_default();
            e.0 += 1;
            add(&mut e.1, &u);
        } else if subject.starts_with("speccoder: converge round") {
            add(&mut convergence, &u);
        }
    }
    for saved in crate::attempts::list(&root, &feature) {
        if let Some(u) = saved.usage.as_ref() {
            let e = per_task.entry(saved.task.clone()).or_default();
            e.0 += 1;
            add(&mut e.1, u);
        }
    }

    let mut total = Usage::default();
    let mut tasks = Vec::new();
    for (task, (attempts, mut usage)) in per_task {
        total.input_tokens += usage.input_tokens;
        total.output_tokens += usage.output_tokens;
        total.cache_read_tokens += usage.cache_read_tokens;
        total.cache_write_tokens += usage.cache_write_tokens;
        price(&mut usage, prices);
        tasks.push(TaskCost {
            task,
            attempts,
            usage,
        });
    }
    total.input_tokens += convergence.input_tokens;
    total.output_tokens += convergence.output_tokens;
    total.cache_read_tokens += convergence.cache_read_tokens;
    total.cache_write_tokens += convergence.cache_write_tokens;
    price(&mut convergence, prices);
    let priced: Vec<f64> = tasks
        .iter()
        .map(|t| &t.usage)
        .chain([&convergence])
        .filter_map(|u| u.usd)
        .collect();
    total.usd = (!priced.is_empty()).then(|| priced.iter().sum());
    let message = match total.usd {
        Some(usd) => format!(
            "{} tokens in, {} out ({} from the cache): ${usd:.4}",
            total.input_tokens, total.output_tokens, total.cache_read_tokens
        ),
        None => format!(
            "{} tokens in, {} out ({} from the cache); add price_input_per_mtok and price_output_per_mtok to [model] for money",
            total.input_tokens, total.output_tokens, total.cache_read_tokens
        ),
    };
    CommandResult::new("cost", CommandStatus::Ok, &message).with_data(CommandData::Cost(Box::new(
        CostReport {
            feature,
            tasks,
            convergence,
            total,
            priced_model: prices.and(model.map(|m| format!("{}/{}", m.provider, m.model))),
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_input_is_priced_at_the_cache_rate() {
        let mut u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 600_000,
            cache_write_tokens: 0,
            usd: None,
        };
        price(
            &mut u,
            Some(crate::substrate::model_config::Prices {
                input: 1.0,
                output: 4.0,
                cache_read: 0.1,
            }),
        );
        // 0.4M fresh × $1 + 0.6M cached × $0.1 + 0.1M out × $4
        assert!(
            (u.usd.unwrap_or_default() - 0.86).abs() < 1e-9,
            "{:?}",
            u.usd
        );
    }
}
