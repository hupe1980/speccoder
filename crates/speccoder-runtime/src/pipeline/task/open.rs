//! Phases 1–3 of a task run: the task opened — its unit and worktree
//! (a waiting or held attempt elsewhere refuses it; leftovers are set
//! aside), then the spec, configuration, policy, runner and a person's
//! decisions as committed in the worktree. Nothing it returns changes later
//! but what [`Carried`] holds.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::{CommandData, CommandResult, CommandStatus};

use super::Started;
use crate::pipeline::*;
use crate::{approvals, external, git, mutation, progress, resume, sandbox, substrate, tools, verify, worktree};

/// The opened task: what every later phase reads.
pub(super) struct Opened {
    pub(super) approver: Option<Arc<dyn approvals::Approver>>,
    pub(super) s: Setup,
    pub(super) native: Option<Arc<super::super::bug::NativeBug>>,
    pub(super) feature_name: String,
    pub(super) plane_dir: std::path::PathBuf,
    pub(super) live: Arc<dyn progress::Progress>,
    pub(super) wt_path: String,
    pub(super) project: std::path::PathBuf,
    pub(super) wt_root: std::path::PathBuf,
    pub(super) tmp: std::path::PathBuf,
    pub(super) elsewhere: approvals::Elsewhere,
    pub(super) marker_path: std::path::PathBuf,
    pub(super) autopilot: bool,
    pub(super) interrupted: Option<resume::Marker>,
    pub(super) resumed_from: Option<(Option<speccoder_core::stage::Stage>, Option<String>)>,
    pub(super) format: &'static dyn crate::format::Adapter,
    pub(super) snap: speccoder_core::snapshot::ProjectSnapshot,
    pub(super) checklists_overridden: bool,
    pub(super) parsed_task: Option<speccoder_core::snapshot::Task>,
    pub(super) tasks_file: Option<std::path::PathBuf>,
    pub(super) bundle: crate::context::Bundle,
    pub(super) acceptance_story: Option<String>,
    pub(super) story_task: Option<String>,
    pub(super) config_text: String,
    pub(super) run_config: substrate::run_config::RunConfig,
    pub(super) executor: Option<external::Executor>,
    pub(super) sandbox_config: sandbox::Config,
    pub(super) runner: sandbox::Runner,
    pub(super) sandbox_notice: Option<String>,
    pub(super) mutation_config: Option<mutation::Config>,
    pub(super) worktree_config: crate::prepare::Config,
    pub(super) check_argvs: Vec<Vec<String>>,
    pub(super) unit_dir: String,
    pub(super) planning: Option<String>,
    pub(super) policy: tools::policy::Policy,
    pub(super) contained: Option<&'static str>,
    pub(super) contained_notice: Option<String>,
    pub(super) wt_root_canonical: String,
    pub(super) this_run: RunId,
    pub(super) decisions: approvals::TaskDecisions,
    pub(super) task_grants: Vec<tools::policy::Object>,
}

/// Phases 1–3. `Err`: the run ends here, with what a person sees.
#[allow(clippy::too_many_lines)] // the checks before anything runs, in order
pub(super) fn task(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    state: &Path,
    secret: Option<&str>,
    options: &RunOptions,
) -> Result<(Opened, Started), Box<CommandResult>> {
    let approver = options.approver.clone();
    use speccoder_core::snapshot::TaskState;

    let refuse =
        |status: CommandStatus, m: String| CommandResult::new("run", status, &redact(&m, secret));

    // 1. The feature, as selected in the user's checkout.
    let s = setup(path, feature, state, "run")?;
    // A bug in the format's own bug structure: a unit no task list names.
    let native = options.bug_home.clone();
    let feature_name = match (&native, s.snapshot.feature.as_ref()) {
        (Some(n), _) => n.unit.clone(),
        (None, Some(f)) => f.feature.name.clone(),
        (None, None) => {
            return Err(Box::new(refuse(
                CommandStatus::SelectionNeeded,
                "no feature selected; pass --feature".to_owned(),
            )));
        }
    };
    // A lane keeps its own journal and live state; a person's
    // decisions are the feature's.
    let plane_dir = match &options.lane {
        Some(lane) => s.plane_dir.join("lanes").join(lane),
        None => s.plane_dir.clone(),
    };
    // Live progress: live.json for `speccoder report --live`, and the printer.
    let live: Arc<dyn progress::Progress> = Arc::new(progress::Live::new(
        &plane_dir,
        s.model.prices,
        options.progress.clone(),
    ));

    // 2. The worktree.
    let wt = match &native {
        Some(n) => crate::commands::worktree_of(&s.snapshot, &n.unit, &n.dir, state),
        None => workspace(path, Some(&feature_name), state, options),
    };
    let Some(CommandData::Worktree(w)) = wt.data() else {
        return Err(Box::new(CommandResult::carried("run", &wt)));
    };
    if wt.status() != CommandStatus::Ok {
        return Err(Box::new(CommandResult::carried("run", &wt).with_data(CommandData::Worktree(w.clone()))));
    }
    let (Some(wt_path), Some(project_dir)) = (w.path.clone(), w.project_dir.clone()) else {
        return Err(Box::new(refuse(CommandStatus::Error, "worktree without a path".to_owned())));
    };
    let (wt_path, project_dir) = match &options.lane {
        None => (wt_path, project_dir),
        Some(lane) => match lane_worktree(path, state, &feature_name, lane, &wt_path, &project_dir)
        {
            Ok(paths) => paths,
            Err(e) => return Err(Box::new(refuse(CommandStatus::GitPrecondition, e))),
        },
    };
    let project = std::path::PathBuf::from(&project_dir);
    let wt_root = std::path::PathBuf::from(&wt_path);
    let tmp = plane_dir.join("tmp");

    // 2a. A task waiting for a person holds the worktree with its attempt:
    //     another task starts once it is decided and has resumed.
    //     Read through the journal's writer, which repairs one a killed run
    //     left unclosed; calls that cannot be read refuse the run, never
    //     pass for none waiting.
    let elsewhere = if options.lane.is_none() {
        match crate::approvals::waiting_elsewhere(&project, &plane_dir, &feature_name, task) {
            Ok(e) => e,
            Err(e) => {
                return Err(Box::new(refuse(
                    CommandStatus::Error,
                    format!(
                        "the calls waiting for a person could not be read, so whether another task's attempt holds the worktree is unknown: {e}"
                    ),
                )));
            }
        }
    } else {
        crate::approvals::Elsewhere::default()
    };
    if let Some((other, covers)) = &elsewhere.waiting {
        return Err(Box::new(refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{other} waits for a person ({covers}) and its attempt holds the worktree: decide it (`speccoder task approve {other}` or `reject`), then `speccoder run --task {other}`"
            ),
        )));
    }
    // 2a'. So does an attempt held for a ruling on its tests.
    if options.lane.is_none()
        && let Some(other) = crate::strength::held_elsewhere(&plane_dir, &feature_name, task)
    {
        return Err(Box::new(refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{other}'s acceptance tests reject an implementation the spec appears to allow, and its attempt holds the worktree: `speccoder task rule {other} --tests-right --reason …` or `speccoder run --task {other} --fresh-acceptance`"
            ),
        )
        .with_code("TESTS-OVER-STRICT")));
    }
    // 2b. Leftovers of an interrupted run (or edits by hand) are saved under
    //     an attempt ref and set aside: a run starts from the branch tip
    //    . Without per-task commits, uncommitted work is expected.
    let commits_per_task = verify::find_config(&project)
        .and_then(|root| std::fs::read_to_string(root.join(verify::config::FILE)).ok())
        .and_then(|t| substrate::run_config::parse(&t).ok())
        .is_none_or(|c| c.commit_per_task);
    // an attempt a killed process left behind is resumed, not set
    // aside — the worktree holds what its finished tool calls wrote.
    let marker_path = resume::path(&plane_dir, &feature_name, task);
    // A fresh start for the task's tests: its held (or interrupted) attempt
    // is set aside below like any leftover, and new tests are written.
    if options.fresh_acceptance && options.lane.is_none() {
        let _ = std::fs::remove_file(&marker_path);
        crate::strength::release(&crate::strength::dir(&s.plane_dir, &feature_name, task));
    }
    let head_now = git::Git::at(&wt_root)
        .line(&["rev-parse", "HEAD"])
        .unwrap_or_default();
    // An autopilot run has exactly the grants ticked on its sheet:
    // it resumes no interrupted attempt — whose journal holds an earlier
    // run's grants and answers — but sets it aside like any leftover, and
    // adopts no answer kept for the task.
    let autopilot = options.mode == speccoder_core::oracle::RunMode::Autopilot;
    let interrupted = resume::load(&marker_path).filter(|m| {
        m.head == head_now && options.lane.is_none() && !head_now.is_empty() && !autopilot
    });
    // A resumed attempt continues its run: this invocation adopts the
    // run's id, its asking count and its grants.
    if let Some(m) = &interrupted {
        if let (Some(id), Ok(mut run)) = (&m.run_id, options.run_id.lock()) {
            *run = crate::pipeline::RunId(id.clone());
        }
        if let Ok(mut asked) = options.asked.lock() {
            *asked = (*asked).max(m.asked);
        }
        if let Ok(mut g) = options.run_grants.lock() {
            for o in &m.run_grants {
                if !g.contains(o) {
                    g.push(o.clone());
                }
            }
        }
    }
    let marker_guard = resume::Guard::new(marker_path.clone());
    // Where the interrupted attempt was, from the live record it left
    // (read before this run overwrites it).
    let resumed_from = interrupted.as_ref().map(|_| {
        std::fs::read_to_string(plane_dir.join(progress::FILE))
            .ok()
            .and_then(|t| serde_json::from_str::<progress::LiveState>(&t).ok())
            .filter(|l| l.task == task)
            .map_or((None, None), |l| (l.stage, l.last_call))
    });
    let mut set_aside = None;
    // In place, a dirty checkout is the person's: the session guard refuses it.
    if interrupted.is_none()
        && commits_per_task
        && options.session.is_none()
        && worktree::require_clean(&wt_root).is_err()
    {
        // What the checks leave behind — build output not ignored, a
        // lockfile never committed — is no one's edit: setting it aside
        // would store it in git on every run. The setup is fixed once.
        let left = worktree::leftovers(&wt_root);
        if !left.is_empty() {
            let mut what = Vec::new();
            if !left.commit.is_empty() {
                what.push(format!("{} is not committed", left.commit.join(", ")));
            }
            if !left.ignore.is_empty() {
                what.push(format!("{} is not git-ignored", left.ignore.join(", ")));
            }
            let checkout = std::path::Path::new(&s.snapshot.root);
            let remedy = crate::code::setup_remedy(checkout, Some(&feature_name));
            let command = remedy
                .as_ref()
                .map_or_else(|| left.fix(checkout, &wt_root, &[]), |r| r.command.clone());
            return Err(Box::new(refuse(
                CommandStatus::GitPrecondition,
                format!(
                    "{} — the checks write it on every run, so each run would find it in the worktree and set it aside",
                    what.join(" and "),
                ),
            )
            .with_code("SETUP-INCOMPLETE")
            .with_fix(vec!["speccoder fix".to_owned(), command])
            .with_remedy(remedy)));
        }
        match set_aside_leftovers(&wt_root, &tmp, &feature_name, task) {
            Ok(reference) => set_aside = Some(reference),
            Err(e) => return Err(Box::new(refuse(CommandStatus::GitPrecondition, e))),
        }
    }

    // 3. Spec, checks and policy as committed in the worktree.
    let format = crate::format::at(&project);
    let Some(snap) = format.inspect(
        &project,
        if native.is_some() {
            None
        } else {
            Some(&feature_name)
        },
    ) else {
        return Err(Box::new(refuse(
            CommandStatus::NotAProject,
            format.messages().no_project_in_worktree.to_owned(),
        )));
    };
    if native.is_some()
        && crate::bug::state::load(&s.plane_dir, &feature_name, task)
            .is_some_and(|b| b.outcome == speccoder_core::bug::Outcome::Fixed)
    {
        return Err(Box::new(refuse(
            CommandStatus::TaskNotReady,
            format!("{task} is already fixed"),
        )
        .with_code("TASK-DONE")));
    }
    // The task must exist and be ready in the worktree, where progress
    // (ticks committed on speccoder/<feature>) lives.
    if native.is_none() {
        let Some(st) = snap
            .feature
            .as_ref()
            .and_then(|f| f.schedule.as_ref())
            .and_then(|sch| sch.tasks.iter().find(|t| t.id == task))
        else {
            return Err(Box::new(refuse(
                CommandStatus::NotFound,
                format!("no task {task} in {feature_name}"),
            )));
        };
        match st.state {
            TaskState::Done => {
                return Err(Box::new(refuse(
                    CommandStatus::TaskNotReady,
                    format!("{task} is already done"),
                )
                .with_code("TASK-DONE")));
            }
            TaskState::Blocked => {
                return Err(Box::new(refuse(
                    CommandStatus::TaskNotReady,
                    format!("{task} is blocked by {}", st.blocked_by.join(", ")),
                )
                .with_code("TASK-BLOCKED")));
            }
            TaskState::Ready => {}
        }
    }
    // Unfinished checklists stop the task before any model call (the
    // committed checklists in the worktree are what counts).
    let checklists_overridden = if native.is_some() {
        false
    } else {
        match checklist_gate(&project, Some(&feature_name), options.ignore_checklists) {
            Ok(overridden) => overridden,
            Err(stop) => return Err(Box::new(*stop)),
        }
    };
    let parsed_task = snap
        .feature
        .as_ref()
        .and_then(|f| f.tasks.as_ref())
        .and_then(|t| t.tasks.iter().find(|t| t.id == task).cloned());
    let tasks_file = snap
        .feature
        .as_ref()
        .and_then(|f| f.tasks.as_ref())
        .map(|t| project.join(&t.path));

    let bundle = match native
        .as_ref()
        .map(|n| n.bundle(&project))
        .map_or_else(|| format.bundle(&snap, task), Ok)
    {
        Ok(b) => b,
        Err(_) => {
            return Err(Box::new(refuse(
                CommandStatus::NotFound,
                format!("no task {task} in the worktree's {feature_name}"),
            )));
        }
    };
    // A person's decision: this Setup or Foundational task writes
    // a story's acceptance tests — its author gets the story's scenarios.
    let acceptance_story = native
        .is_none()
        .then(|| crate::acceptance_at::story_at(&s.plane_dir, &feature_name, task))
        .flatten();
    let story_task: Option<String> = acceptance_story.as_ref().and_then(|st| {
        snap.feature
            .as_ref()
            .and_then(|f| f.tasks.as_ref())
            .and_then(|t| {
                t.tasks
                    .iter()
                    .find(|t| t.story.as_deref() == Some(st.as_str()))
            })
            .map(|t| t.id.clone())
    });
    let bundle = match (&acceptance_story, &story_task) {
        (Some(st), Some(other)) => {
            let mut b = bundle;
            if let Ok(of) = format.bundle(&snap, other) {
                b.story = of.story;
            }
            b.task.story.clone_from(st);
            b
        }
        _ => bundle,
    };
    let Some(config_root) = verify::find_config(&project) else {
        return Err(Box::new(refuse(
            CommandStatus::ConfigError,
            "the worktree has no speccoder.toml (is it committed?)".to_owned(),
        )
        .with_code("CONFIG-MISSING")));
    };
    let config_text =
        std::fs::read_to_string(config_root.join(verify::config::FILE)).unwrap_or_default();
    let run_config = match substrate::run_config::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID")));
        }
    };
    // `[review] required`: the spec as committed in the worktree must be
    // approved, every requirement on its current wording.
    if run_config.review_required
        && let Some(f) = snap.feature.as_ref()
        && let Some(t) = crate::review::totals_of(&project, format, &feature_name, &f.feature.dir)
        && !t.complete()
    {
        return Err(Box::new(refuse(
            CommandStatus::NeedsHuman,
            format!(
                "{feature_name}'s spec is not approved yet ({} approved; {} to review, {} with changes requested, {} changed since, {} open question(s); committed review only) and [review] required is on",
                t.approved, t.open, t.changes_requested, t.lapsed, t.questions
            ),
        )
        .with_code("REVIEW-PENDING")));
    }
    // Two test authors read a requirement two ways: the person says which
    // is meant before a gated run.
    if run_config.review_required {
        let ambiguous = match crate::oracle::open_ambiguities(
            &s.plane_dir,
            &feature_name,
            &format.oracle_requirements(&snap),
        ) {
            Ok(a) => a,
            Err(t) => {
                return Err(Box::new(refuse(
                    CommandStatus::NeedsHuman,
                    format!(
                        "{}: whether it is read two ways is unknown and [review] required is on",
                        t.0
                    ),
                )
                .with_code("ORACLE-TAMPERED")));
            }
        };
        if let Some((req, scenario)) = ambiguous.first() {
            return Err(Box::new(refuse(
                CommandStatus::NeedsHuman,
                format!(
                    "{req} is read two ways ({scenario}) and [review] required is on: choose a reading or reword it first"
                ),
            )
            .with_code("REVIEW-AMBIGUOUS")
            .with_fix(vec![
                format!("speccoder review tests {req} --reading A {scenario}"),
                format!("speccoder review tests {req} --reading B {scenario}"),
            ])));
        }
    }
    let executor = match external::parse(&config_text) {
        Ok(e) => e,
        Err(e) => {
            return Err(Box::new(refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID")));
        }
    };
    // `run --agent`: this run's agent, the configuration untouched.
    let executor = match options.agent.as_deref() {
        None => executor,
        Some("native") => None,
        Some(id) => match external::for_agent(id, executor.as_ref()) {
            Ok(e) => Some(e),
            Err(e) => return Err(Box::new(refuse(CommandStatus::UsageError, e))),
        },
    };
    // Every command this task starts runs confined to the worktree.
    let sandbox_config = match sandbox::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID")));
        }
    };
    let (runner, sandbox_notice) = match sandbox::runner(&wt_root, &sandbox_config) {
        Ok(r) => r,
        Err(e) => return Err(Box::new(refuse(CommandStatus::ConfigError, e))),
    };
    // Autopilot asks nobody: it runs only where the OS sandbox confines it.
    if options.mode == speccoder_core::oracle::RunMode::Autopilot && !runner.is_sandboxed() {
        return Err(Box::new(refuse(
            CommandStatus::ConfigError,
            "autopilot needs the OS sandbox, and commands here would run unsandboxed".to_owned(),
        )
        .with_code("SANDBOX-UNAVAILABLE")));
    }
    let mutation_config = match mutation::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID")));
        }
    };
    let worktree_config = match crate::prepare::parse(&config_text) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(refuse(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                .with_code("CONFIG-INVALID")));
        }
    };
    let check_argvs = verify::check_argvs(&config_root);
    // A task naming only its unit's design documents is planning work:
    // written with the planning-task profile — its unit's directory, never
    // a judged file — and judged by the checks alone.
    let unit_dir = snap
        .feature
        .as_ref()
        .map(|f| f.feature.dir.clone())
        .unwrap_or_default();
    let changed_code = crate::attempts::list(&wt_root, &feature_name)
        .iter()
        .any(|a| a.task == task && a.outcome == CHANGED_CODE);
    let planning: Option<String> = parsed_task
        .as_ref()
        .filter(|t| {
            native.is_none()
                && !changed_code
                && speccoder_core::lane::by_paths(&t.title, &unit_dir, &format.rules().judged)
                    == speccoder_core::lane::Lane::Planning
        })
        .map(|_| {
            let prefix = project
                .strip_prefix(&wt_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if prefix.is_empty() {
                unit_dir.trim_end_matches('/').to_owned()
            } else {
                format!("{prefix}/{}", unit_dir.trim_end_matches('/'))
            }
        });
    let policy = tools::policy::Policy::new(&check_argvs, &run_config.allow)
        .with_prefixes(run_config.allow_prefix.clone())
        .rules(format.rules());
    let policy = match &planning {
        Some(dir) => policy.planning(dir),
        None => policy,
    };
    // In place: the model's view of the checkout confined (git that moves
    // it denied, ignored files hidden, `.git` read-only to commands).
    let (policy, runner, carve_notice) = match options.session.as_ref().and_then(|s| s.lock().ok())
    {
        Some(s) => s.confine(policy, runner),
        None => (policy, runner, None),
    };
    let sandbox_notice = sandbox_notice.or(carve_notice);
    // Contained calls run without asking only where the sandbox confines
    // them; asked for, and not confined, the run says why once.
    let contained = runner.confines().filter(|_| run_config.contained);
    let contained_notice = (run_config.contained && contained.is_none()).then(|| {
        format!(
            "contained calls ask: {}",
            match &runner {
                sandbox::Runner::Plain => "no sandbox is enforced",
                sandbox::Runner::Sandboxed(s) if s.profile.network == sandbox::Network::On => {
                    "the sandbox gives the network"
                }
                sandbox::Runner::Sandboxed(_) =>
                    "the sandbox cannot confine signals or `.git` here",
            }
        )
    });
    // In place: the checkout as speccoder left it, the intent as the run
    // started with it — before the task, and again before the tick.
    if let Some(e) = options.session.as_ref().and_then(|s| {
        s.lock().ok().and_then(|s| {
            // A resumed attempt's writes are in the checkout already.
            if interrupted.is_some() {
                s.check_before_tick(&policy).err()
            } else {
                s.check(&policy).err()
            }
        })
    }) {
        return Err(Box::new(e.result("run")));
    }

    let wt_root_canonical = std::fs::canonicalize(&wt_root)
        .unwrap_or_else(|_| wt_root.clone())
        .display()
        .to_string();

    // A person's approvals (of this run only: a grant ends with its run)
    // and rejections for this task.
    let this_run = options.run();
    let (decisions, task_grants) = match approvals::Decisions::load(&s.plane_dir) {
        Ok(mut d) => {
            // Decisions on a call a lane stopped belong to the run that
            // now attempts the task (an autopilot run claims none).
            if !autopilot
                && d.claim(&feature_name, task, this_run.as_str())
                && let Err(e) = d.save(&s.plane_dir)
            {
                return Err(Box::new(refuse(CommandStatus::Error, e)));
            }
            (
                d.for_task(&feature_name, task),
                if autopilot {
                    Vec::new()
                } else {
                    d.for_run(&feature_name, task, this_run.as_str())
                },
            )
        }
        Err(e) => return Err(Box::new(refuse(CommandStatus::ConfigError, e))),
    };

    if let Some((stage, last_call)) = &resumed_from {
        live.event(&progress::Event::Resumed {
            task: task.to_owned(),
            stage: *stage,
            last_call: last_call.clone(),
        });
    }
    live.event(&progress::Event::TaskStarted {
        feature: feature_name.clone(),
        task: task.to_owned(),
        title: parsed_task
            .as_ref()
            .map(|t| t.title.clone())
            .unwrap_or_default(),
    });
    if planning.is_some()
        && let Some(t) = &parsed_task
    {
        live.event(&progress::Event::Notice {
            text: format!(
                "📐 planning · writes {}",
                speccoder_core::lane::task_paths(&t.title, &unit_dir).join(", ")
            ),
        });
    }
    Ok((
        Opened {
            approver,
            s,
            native,
            feature_name,
            plane_dir,
            live,
            wt_path,
            project,
            wt_root,
            tmp,
            elsewhere,
            marker_path,
            autopilot,
            interrupted,
            resumed_from,
            format,
            snap,
            checklists_overridden,
            parsed_task,
            tasks_file,
            bundle,
            acceptance_story,
            story_task,
            config_text,
            run_config,
            executor,
            sandbox_config,
            runner,
            sandbox_notice,
            mutation_config,
            worktree_config,
            check_argvs,
            unit_dir,
            planning,
            policy,
            contained,
            contained_notice,
            wt_root_canonical,
            this_run,
            decisions,
            task_grants,
        },
        Started {
            marker_guard,
            head_now,
            set_aside,
        },
    ))
}
