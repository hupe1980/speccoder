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
    // `[YOU]` tasks the person gave a hint for: attempted after all.
    let mut hinted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
                // Who does what, said once: information, not a warning.
                if let Some(l) = crate::status::lanes(path, Some(&w.feature)) {
                    p.event(&crate::progress::Event::Notice { text: l.line() });
                }
                // The plan checks, said once per run start; they never
                // stop, delay or re-order it.
                let warnings = crate::plan_lint::lints(format, &snap, &project).len();
                if warnings > 0 {
                    p.event(&crate::progress::Event::Notice {
                        text: format!(
                            "plan: {warnings} warning{} — speccoder doctor --feature {}",
                            if warnings == 1 { "" } else { "s" },
                            w.feature
                        ),
                    });
                }
            }
        }
        // No task to work: the feature is not done, its tasks are not
        // written yet (no task list, or one without a task).
        if let Some(stop) = snap.feature.as_ref().and_then(|f| no_tasks(format, f)) {
            return finish(
                feature_name,
                attempts,
                CommandStatus::TaskNotReady,
                stop.message,
            )
            .with_code("NO-TASKS")
            .with_fix(stop.fix)
            .with_next(stop.next);
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
        // A `[YOU]` task is not attempted: the person is asked, or it waits
        // with its three actions.
        let parsed: Vec<&speccoder_core::snapshot::Task> = snap
            .feature
            .as_ref()
            .and_then(|f| f.tasks.as_ref())
            .map(|t| t.tasks.iter().collect())
            .unwrap_or_default();
        let yours = |id: &str| !hinted.contains(id) && parsed.iter().any(|t| t.id == id && t.yours);
        if yours(&next.id) {
            let id = next.id.clone();
            let title = parsed
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.title.clone())
                .unwrap_or_default();
            let stop = crate::approvals::StopRequest {
                task: format!("{}/{id}", w.feature),
                title: title.clone(),
                claim: None,
                dependents: speccoder_core::graph::dependents(&schedule, &id),
            };
            match ask_at_stop(path, feature_name.as_deref(), state, options, &stop) {
                Some((answer, done)) if done.status() == CommandStatus::Ok => {
                    if matches!(answer, crate::approvals::StopAnswer::Hint(_)) {
                        hinted.insert(id);
                    }
                    continue;
                }
                Some((_, failed)) => {
                    return finish(
                        feature_name,
                        attempts,
                        failed.status(),
                        format!("{id}: {}", failed.message()),
                    )
                    .caused_by(&failed);
                }
                None => {
                    let fix = speccoder_core::codes::lookup("TASK-YOURS")
                        .map(|e| {
                            speccoder_core::codes::fixes(
                                e,
                                "run",
                                Some(&id),
                                feature_name.as_deref(),
                            )
                        })
                        .unwrap_or_default();
                    return finish(
                        feature_name,
                        attempts,
                        CommandStatus::NeedsHuman,
                        format!("{id} is yours — {title}: a step only a person can take"),
                    )
                    .with_code("TASK-YOURS")
                    .with_fix(fix);
                }
            }
        }
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
            .filter(|t| {
                parallel_ids.contains(&t.id) && !one_at_a_time.contains(&t.id) && !yours(&t.id)
            })
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
        // An honest stop: the person is asked in place — I did it, a hint
        // and a new attempt, or skip — and the run goes on.
        let claim = match res.data() {
            Some(CommandData::Run(r)) if r.attempt_status == "blocked" => {
                Some(r.reason.clone().unwrap_or_default())
            }
            _ => None,
        };
        if let Some(claim) = claim.filter(|_| res.status() != CommandStatus::Ok) {
            let stop = crate::approvals::StopRequest {
                task: format!("{}/{task}", w.feature),
                title: parsed
                    .iter()
                    .find(|t| t.id == task)
                    .map(|t| t.title.clone())
                    .unwrap_or_default(),
                claim: Some(claim),
                dependents: speccoder_core::graph::dependents(&schedule, &task),
            };
            match ask_at_stop(path, feature_name.as_deref(), state, options, &stop) {
                Some((answer, done)) if done.status() == CommandStatus::Ok => {
                    if matches!(answer, crate::approvals::StopAnswer::Hint(_)) {
                        hinted.insert(task);
                    }
                    continue;
                }
                Some((_, failed)) => {
                    return finish(
                        feature_name,
                        attempts,
                        failed.status(),
                        format!("{task}: {}", failed.message()),
                    )
                    .caused_by(&failed);
                }
                None => {}
            }
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

/// Asks the person at a task that needs them — at a terminal, never in a
/// lane — and takes the answer — I did it, a hint, skip — through the one
/// dispatcher ([`crate::actions::answered_at_stop`]); after a hint the run
/// attempts the task again. `None`: nobody answered; the task waits in Needs You.
fn ask_at_stop(
    path: &Path,
    feature: Option<&str>,
    state: &Path,
    options: &RunOptions,
    stop: &crate::approvals::StopRequest,
) -> Option<(crate::approvals::StopAnswer, CommandResult)> {
    let approver = options
        .approver
        .as_ref()
        .filter(|_| options.lane.is_none())?;
    let answer = approver.at_stop(stop)?;
    let task = stop.task.rsplit('/').next().unwrap_or(&stop.task);
    let done =
        crate::actions::answered_at_stop(path, feature, task, &answer, approver.via(), state);
    Some((answer, done))
}

/// Why a unit has nothing to run: no task list, or one without a task.
#[derive(Debug, Clone)]
pub(crate) struct NoTasks {
    /// `no tasks yet: …`, naming the format's file and how to write it.
    pub message: String,
    /// The planning skills that write the tasks, as commands.
    pub fix: Vec<String>,
    /// The same as next steps, then the run.
    pub next: Vec<speccoder_core::next::NextStep>,
}

/// [`NoTasks`] for `feature`, unless it has a task: such a unit is never
/// done (code `NO-TASKS`).
pub(crate) fn no_tasks(
    format: &dyn crate::format::Adapter,
    feature: &speccoder_core::snapshot::FeatureSnapshot,
) -> Option<NoTasks> {
    if feature.tasks.as_ref().is_some_and(|t| !t.tasks.is_empty()) {
        return None;
    }
    let messages = format.messages();
    let skills: Vec<String> = (format.planning().toward_tasks)(feature.artifacts.plan)
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let name = &feature.feature.name;
    Some(NoTasks {
        message: format!(
            "no tasks yet: {name} {}; {}",
            if feature.tasks.is_some() {
                format!("has a {} without a task", messages.tasks_file)
            } else {
                format!("has no {}", messages.tasks_file)
            },
            messages.make_tasks
        ),
        fix: skills
            .iter()
            .map(|s| format!("speccoder spec {s}"))
            .collect(),
        next: speccoder_core::next::next(
            &speccoder_core::next::Outcome::Planning { skills },
            Some(name),
        ),
    })
}
