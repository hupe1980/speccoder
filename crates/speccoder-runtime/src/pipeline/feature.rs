//! `speccoder run` over a whole feature, and `converge`.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::pipeline::*;
use crate::{controls, convergence, hosting, progress, substrate, worktree};

/// `speccoder run` without `--task`: attempts ready tasks in document order
/// (read from the worktree) until the feature is done or an attempt does
/// not pass. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn run_feature(
    path: &Path,
    feature: Option<&str>,
    options: &RunOptions,
    converge: bool,
) -> CommandResult {
    with_driver("run", path, feature, |state, provider, key| {
        if converge {
            run_converged_with(path, feature, state, provider, Some(key), options)
        } else {
            run_feature_with(path, feature, state, provider, Some(key), options)
        }
    })
}

/// `speccoder converge`: assess the finished feature and append what remains
///. The provider comes from `[model]` and its key variable.
#[must_use]
pub fn converge(path: &Path, feature: Option<&str>) -> CommandResult {
    converge_in(path, feature, false)
}

/// [`converge`], in the person's checkout when `in_place`.
#[must_use]
pub fn converge_in(path: &Path, feature: Option<&str>, in_place: bool) -> CommandResult {
    let options = RunOptions {
        in_place,
        ..RunOptions::default()
    };
    with_driver("converge", path, feature, |state, provider, key| {
        convergence::converge_with_options(
            path,
            feature,
            state,
            Arc::clone(provider),
            Some(key),
            &options,
        )
    })
}

/// Opens the configured model driver (key from the environment only) and
/// hands it to `f` with the state directory.
pub(crate) fn with_driver(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    f: impl FnOnce(&Path, &Arc<dyn agentplane::model::ModelProvider>, &str) -> CommandResult,
) -> CommandResult {
    let Some(state) = worktree::state_dir() else {
        return CommandResult::new(
            command,
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR");
    };
    let model = match setup(path, feature, &state, command) {
        Ok(s) => s.model,
        Err(r) => return *r,
    };
    let (driver, key) = match substrate::model_config::open_driver(&model) {
        Ok((d, k)) => (Ok::<_, String>(d), k.unwrap_or_default()),
        Err(e) => return substrate::model_config::refused(command, &e),
    };
    match driver {
        Ok(provider) => f(&state, &provider, &key),
        Err(e) => CommandResult::new(command, CommandStatus::ConfigError, &redact(&e, Some(&key))),
    }
}

/// `run --converge`: work the feature through, converge, and work through
/// what convergence appended — until it converges, stops making progress,
/// reaches `[converge] max_rounds`, or an attempt needs a person.
pub fn run_converged_with(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: &Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    // In place: one session over every round.
    if options.in_place && options.session.is_none() {
        return in_session("run", path, feature, state, options, |o| {
            run_converged_with(path, feature, state, provider, secret, o)
        });
    }
    let mut attempts = Vec::new();
    let mut rounds = Vec::new();
    loop {
        let res = run_feature_with(path, feature, state, provider, secret, options);
        let Some(CommandData::Feature(fr)) = res.data() else {
            return res;
        };
        let mut fr = (**fr).clone();
        attempts.append(&mut fr.attempts);
        let finished = res.status() == CommandStatus::Ok && fr.stop.starts_with("feature done");
        if !finished {
            fr.attempts = attempts;
            fr.converge = rounds;
            return CommandResult::carried("run", &res)
                .with_data(CommandData::Feature(Box::new(fr)));
        }
        if let Some(p) = &options.progress {
            p.event(&progress::Event::Notice {
                text: format!("converge: assessing {} against its spec", fr.feature),
            });
        }
        let c = convergence::converge_with_options(
            path,
            feature,
            state,
            Arc::clone(provider),
            secret,
            options,
        );
        if let Some(p) = &options.progress {
            p.event(&progress::Event::Notice {
                text: format!("converge: {}", c.message()),
            });
        }
        let outcome = match c.data() {
            Some(CommandData::Converge(r)) => {
                let outcome = r.outcome.clone();
                rounds.push((**r).clone());
                outcome
            }
            _ => String::new(),
        };
        if outcome != "tasks_appended" {
            fr.attempts = attempts;
            fr.converge = rounds;
            fr.stop = format!("converge: {}", c.message());
            return CommandResult::new("run", c.status(), &fr.stop.clone())
                .with_data(CommandData::Feature(Box::new(fr)));
        }
    }
}

/// [`run_feature`] with an explicit state directory and model driver.
pub fn run_feature_with(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    provider: &std::sync::Arc<dyn agentplane::model::ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    // In place: one session around the whole feature.
    if options.in_place && options.session.is_none() {
        return in_session("run", path, feature, state, options, |o| {
            run_feature_with(path, feature, state, provider, secret, o)
        });
    }
    let max_tasks = options.max_tasks;
    use speccoder_core::FeatureReport;
    use speccoder_core::snapshot::TaskState;

    let mut attempts = Vec::new();
    let mut feature_name = feature.map(str::to_owned);
    let hook_lines: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let parallel_lines: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
    let full_tier: std::cell::RefCell<Option<speccoder_core::FullTier>> =
        std::cell::RefCell::new(None);
    let finish = |name: Option<String>,
                  attempts: Vec<speccoder_core::RunReport>,
                  status: CommandStatus,
                  stop: String| {
        // An autopilot run lists every call it refused instead of asking.
        let autopilot_denied: Vec<speccoder_core::oracle::AutopilotDenial> =
            if options.mode == speccoder_core::oracle::RunMode::Autopilot {
                attempts
                    .iter()
                    .flat_map(|a| crate::oracle::autopilot_denials(&a.task, &a.tool_calls))
                    .collect()
            } else {
                Vec::new()
            };
        if let Some(n) = name.as_deref() {
            crate::oracle::record_denials(
                &worktree::project_state_dir(state, path),
                n,
                &autopilot_denied,
            );
        }
        let stop = if autopilot_denied.is_empty() {
            stop
        } else {
            format!(
                "{stop} — autopilot refused {} call(s) instead of asking; they are listed for you (speccoder inbox)",
                autopilot_denied.len()
            )
        };
        CommandResult::new("run", status, &stop).with_data(CommandData::Feature(Box::new(
            FeatureReport {
                feature: name.unwrap_or_default(),
                attempts,
                stop,
                converge: Vec::new(),
                hooks: hook_lines.borrow().clone(),
                parallel: parallel_lines.borrow().clone(),
                full_tier: full_tier.borrow().clone(),
                mode: options.mode,
                autopilot_denied,
            },
        )))
    };
    // Tasks that must run on their own after a parallel group did not merge.
    let mut one_at_a_time: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut parallel_off = false;
    let mut budget = max_tasks.unwrap_or(usize::MAX);
    // Unfinished checklists stop the run before anything runs.
    if let Err(stop) = checklist_gate(path, feature, options.ignore_checklists) {
        return *stop;
    }
    // The format's `before_implement` hooks.
    match hosting::run_hooks(
        "before_implement",
        "",
        path,
        state,
        provider,
        secret,
        options,
    ) {
        Ok(lines) => hook_lines.borrow_mut().extend(lines),
        Err(stop) => {
            return finish(
                feature_name,
                attempts,
                stop.status(),
                stop.message().to_owned(),
            );
        }
    }
    let mut planned = false;
    while budget > 0 {
        if options
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst))
        {
            return finish(
                feature_name,
                attempts,
                CommandStatus::NeedsHuman,
                "cancelled by the person before the next task".to_owned(),
            );
        }
        // The next ready task, read from the worktree.
        let wt = workspace(path, feature_name.as_deref(), state, options);
        let Some(CommandData::Worktree(w)) = wt.data() else {
            return CommandResult::carried("run", &wt);
        };
        if wt.status() != CommandStatus::Ok {
            return CommandResult::carried("run", &wt).with_data(CommandData::Worktree(w.clone()));
        }
        feature_name = Some(w.feature.clone());
        let project = std::path::PathBuf::from(w.project_dir.clone().unwrap_or_default());
        let format = crate::format::at(&project);
        let Some(snap) = format.inspect(&project, Some(&w.feature)) else {
            return CommandResult::new(
                "run",
                CommandStatus::NotAProject,
                format.messages().no_project_in_worktree,
            );
        };
        let schedule = snap
            .feature
            .as_ref()
            .and_then(|f| f.schedule.clone())
            .unwrap_or_else(|| speccoder_core::snapshot::Schedule {
                tasks: Vec::new(),
                totals: speccoder_core::snapshot::ScheduleTotals::default(),
            });
        if !planned {
            planned = true;
            if let Some(p) = &options.progress {
                let skipped = controls::skipped(&project, &w.feature);
                let titles: std::collections::HashMap<&str, &str> = snap
                    .feature
                    .as_ref()
                    .and_then(|f| f.tasks.as_ref())
                    .map(|t| {
                        t.tasks
                            .iter()
                            .map(|x| (x.id.as_str(), x.title.as_str()))
                            .collect()
                    })
                    .unwrap_or_default();
                p.event(&crate::progress::Event::RunStarted {
                    feature: w.feature.clone(),
                    tasks: schedule
                        .tasks
                        .iter()
                        .map(|t| crate::progress::PlannedTask {
                            id: t.id.clone(),
                            title: titles
                                .get(t.id.as_str())
                                .copied()
                                .unwrap_or_default()
                                .to_owned(),
                            state: if skipped.contains_key(&t.id) {
                                "skipped".to_owned()
                            } else {
                                match t.state {
                                    TaskState::Done => "done",
                                    TaskState::Ready => "ready",
                                    _ => "blocked",
                                }
                                .to_owned()
                            },
                        })
                        .collect(),
                });
            }
        }
        // Skipped tasks are passed by; what depends on them stays blocked.
        let skipped = controls::skipped(&project, feature_name.as_deref().unwrap_or_default());
        let Some(next) = schedule
            .tasks
            .iter()
            .find(|t| t.state == TaskState::Ready && !skipped.contains_key(&t.id))
        else {
            let open: Vec<String> = schedule
                .tasks
                .iter()
                .filter(|t| t.state != TaskState::Done)
                .map(|t| {
                    if skipped.contains_key(&t.id) {
                        format!("{} (skipped)", t.id)
                    } else {
                        t.id.clone()
                    }
                })
                .collect();
            return if open.is_empty() {
                // The `full` tier, once, when this run ticked the last task:
                // a failure is reported and stops `pr`; the ticks stand.
                if attempts
                    .iter()
                    .any(|a: &speccoder_core::RunReport| a.ticked)
                {
                    let wt_root = std::path::PathBuf::from(w.path.clone().unwrap_or_default());
                    let tier = full::run_once(&project, &wt_root);
                    full::record(&wt_root, feature_name.as_deref().unwrap_or_default(), &tier);
                    let failed = (!tier.passed).then(|| tier.failed.join(", "));
                    *full_tier.borrow_mut() = Some(tier);
                    if let Some(failed) = failed {
                        return finish(
                            feature_name,
                            attempts,
                            CommandStatus::VerificationFailed,
                            format!(
                                "every task is ticked, but the full-tier checks failed: {failed}; fix them, or `speccoder pr --force`"
                            ),
                        );
                    }
                }
                // The held-out sets, now that no later task can read them.
                if let (Some(name), Ok(opened)) =
                    (feature_name.as_deref(), crate::format::open(path, feature))
                {
                    let wt_root = std::path::PathBuf::from(w.path.clone().unwrap_or_default());
                    let plane = crate::worktree::project_state_dir(
                        state,
                        std::path::Path::new(&opened.snapshot.root),
                    );
                    if let Err(e) =
                        crate::held_out::publish(&wt_root, &plane, name, &plane.join("tmp"))
                    {
                        hook_lines
                            .borrow_mut()
                            .push(format!("held-out sets not published: {e}"));
                    }
                }
                // The format's `after_implement` hooks.
                match hosting::run_hooks(
                    "after_implement",
                    "",
                    path,
                    state,
                    provider,
                    secret,
                    options,
                ) {
                    Ok(lines) => hook_lines.borrow_mut().extend(lines),
                    Err(stop) => {
                        return finish(
                            feature_name,
                            attempts,
                            stop.status(),
                            stop.message().to_owned(),
                        );
                    }
                }
                finish(
                    feature_name,
                    attempts,
                    CommandStatus::Ok,
                    "feature done: every task is ticked".to_owned(),
                )
            } else {
                finish(
                    feature_name,
                    attempts,
                    CommandStatus::TaskNotReady,
                    format!("no task can run; open: {}", open.join(", ")),
                )
            };
        };
        // ready [P] tasks run at once, each in its own lane.
        let parallel_ids: std::collections::BTreeSet<String> = snap
            .feature
            .as_ref()
            .and_then(|f| f.tasks.as_ref())
            .map(|t| {
                t.tasks
                    .iter()
                    .filter(|t| t.parallel)
                    .map(|t| t.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let group: Vec<String> = schedule
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Ready && !skipped.contains_key(&t.id))
            .filter(|t| parallel_ids.contains(&t.id) && !one_at_a_time.contains(&t.id))
            .map(|t| t.id.clone())
            .take(options.parallel.min(budget))
            .collect();
        if !parallel_off && group.len() >= 2 {
            budget = budget.saturating_sub(group.len());
            let feature = feature_name.clone().unwrap_or_default();
            let feature_wt = std::path::PathBuf::from(w.path.clone().unwrap_or_default());
            let lanes = run_lanes(path, &feature, &group, state, provider, secret, options);
            let outcome = merge_lanes(path, &feature_wt, &project, &feature, lanes);
            attempts.extend(outcome.reports);
            parallel_lines.borrow_mut().extend(outcome.lines);
            one_at_a_time.extend(outcome.one_at_a_time);
            parallel_off |= outcome.reverted;
            if let Some((task, status, message)) = outcome.stop {
                return finish(feature_name, attempts, status, format!("{task}: {message}"));
            }
            continue;
        }
        budget -= 1;
        let task = next.id.clone();
        let res = run_task(
            path,
            feature_name.as_deref(),
            &task,
            state,
            std::sync::Arc::clone(provider),
            secret,
            options,
        );
        if let Some(CommandData::Run(r)) = res.data() {
            attempts.push((**r).clone());
        }
        if res.status() != CommandStatus::Ok {
            return finish(
                feature_name,
                attempts,
                res.status(),
                format!("{task}: {}", res.message()),
            )
            .caused_by(&res);
        }
    }
    finish(
        feature_name,
        attempts,
        CommandStatus::Ok,
        "stopped at --max-tasks".to_owned(),
    )
}
