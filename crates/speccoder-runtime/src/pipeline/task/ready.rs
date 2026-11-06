//! Phase 4 of a task run: the worktree made ready — files `[worktree]
//! include` names copied, dependencies prepared, the checks run on the
//! untouched code (a check that cannot run stops the task before any model
//! call), the MCP servers started and the plane opened.

use std::path::Path;
use std::sync::Arc;

use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{CommandResult, CommandStatus};

use super::open::Opened;
use super::{Carried, Started};
use crate::pipeline::*;
use crate::{git, mcp, progress, sandbox, substrate, tools, verify, worktree};

/// The ready worktree: what the tests and the attempt run with.
pub(super) struct Ready {
    /// The runtime the plane's calls are driven on.
    pub(super) rt: tokio::runtime::Runtime,
    /// The task's bundle as the skills read it.
    pub(super) bundle_value: serde_json::Value,
    /// The repository's instructions for agents.
    pub(super) instructions: Vec<crate::instructions::Instruction>,
    /// The project's MCP servers, connected.
    pub(super) connected: mcp::Connected,
    /// What a person is shown about the sandbox for every call they decide.
    pub(super) present: crate::approvals::PresentContext,
    /// The task's plane.
    pub(super) plane: substrate::Plane,
    /// A convergence task: its gap must be shown red before any change.
    pub(super) demand_red: bool,
    /// The tree the task started from: an attempt that does not pass is undone to it.
    pub(super) start: worktree::Checkpoint,
    /// Files copied from the checkout (an external agent is refused reading them).
    pub(super) copied_files: Vec<String>,
    /// Whether the worktree's dependencies were prepared for this task.
    pub(super) prepares: bool,
}

/// Phase 4. `Err`: the run ends here, its end recorded.
#[allow(clippy::too_many_lines)] // prepare, baseline and plane, in order
pub(super) fn make(
    o: &Opened,
    started: Started,
    task: &str,
    state: &Path,
    secret: Option<&str>,
    options: &RunOptions,
    provider: Arc<dyn agentplane::model::ModelProvider>,
) -> Result<(Ready, Carried), Box<CommandResult>> {
    let Opened {
        s,
        feature_name,
        plane_dir,
        live,
        project,
        wt_root,
        tmp,
        elsewhere,
        interrupted,
        resumed_from,
        format,
        snap,
        bundle,
        config_text,
        run_config,
        sandbox_config,
        runner,
        sandbox_notice,
        worktree_config,
        planning,
        policy,
        contained_notice,
        task_grants,
        ..
    } = o;
    let (format, live) = (*format, Arc::clone(live));
    let fail = |status: CommandStatus, m: String| super::ended(&*live, task, secret, status, m);
    // 4. Prepare: the git-ignored files `[worktree] include` names copied
    //    from the checkout, and the worktree's dependency folders (Node,
    //    uv) installed, once per lockfile. In place, the checkout has its own.
    let mut prepare_notes: Vec<String> = Vec::new();
    // Files copied from the checkout: an external agent's own read of one
    // cannot be masked, so it is refused one.
    let mut copied_files: Vec<String> = Vec::new();
    if options.session.is_none() {
        match crate::include::copy(
            wt_root,
            project,
            Path::new(&s.snapshot.root),
            &worktree_config.include,
        ) {
            Ok(c) => {
                copied_files.clone_from(&c.files);
                if !c.now.is_empty() {
                    prepare_notes.push(format!(
                        "copied from your checkout ([worktree] include): {}",
                        c.now.join(", ")
                    ));
                }
                prepare_notes.extend(
                    c.skipped
                        .iter()
                        .map(|n| format!("not copied ([worktree] include): {n}")),
                );
                for n in &prepare_notes {
                    live.event(&progress::Event::Notice { text: n.clone() });
                }
            }
            Err(e) => {
                return Err(Box::new(
                    fail(
                        CommandStatus::ConfigError,
                        format!("the files [worktree] include names could not be copied: {e}"),
                    )
                    .with_code("PREPARE-FAILED"),
                ));
            }
        }
    }
    let prepares =
        options.session.is_none() && worktree_config.prepare && crate::prepare::needed(project);
    if prepares {
        let began = std::time::Instant::now();
        stage(
            &*live,
            task,
            Stage::Prepare,
            StageState::Started,
            None,
            None,
        );
        let prepared = crate::prepare::prepare(&crate::prepare::Prepare {
            worktree: wt_root,
            project,
            config: worktree_config,
            runner,
            state: Some(state),
        });
        match prepared {
            Ok(None) => {}
            Ok(Some(p)) => {
                stage(
                    &*live,
                    task,
                    Stage::Prepare,
                    if p.done.is_empty() {
                        StageState::Skipped
                    } else {
                        StageState::Passed
                    },
                    Some(if p.done.is_empty() {
                        "reused: the lockfiles are unchanged".to_owned()
                    } else {
                        p.done.join(", ")
                    }),
                    Some(began),
                );
            }
            Err(f) => {
                stage(
                    &*live,
                    task,
                    Stage::Prepare,
                    StageState::Failed,
                    Some(f.message.clone()),
                    Some(began),
                );
                return Err(Box::new(
                    fail(CommandStatus::ConfigError, f.message)
                        .with_code("PREPARE-FAILED")
                        .with_fix(f.fix)
                        .with_commands(f.commands),
                ));
            }
        }
    } else if options.session.is_none() {
        // Nothing to prepare (any more): the next prepare starts over.
        crate::prepare::forget(wt_root);
    }

    // 4'. Baseline: no verdict without a judge.
    let began = std::time::Instant::now();
    stage(
        &*live,
        task,
        Stage::Baseline,
        StageState::Started,
        None,
        None,
    );
    let baseline = match verify::run_checks(project, Some(verify::config::Tier::Fast), runner) {
        Ok(b) => b,
        Err(e) => {
            stage(
                &*live,
                task,
                Stage::Baseline,
                StageState::Failed,
                None,
                Some(began),
            );
            return Err(Box::new(fail(
                CommandStatus::ConfigError,
                format!("baseline: {e}"),
            )));
        }
    };
    // A check that cannot run on the untouched code — it failed without
    // running a test: not found, a build or configuration error, a file the
    // worktree lacks because it was never committed — can never judge the
    // task. Stop before any model call rather than spend attempts on it.
    // A story's red oracle may stop the build until its tasks land.
    if !options.allow_failing_baseline
        && !super::story::open_oracle(wt_root, snap, format)
        && let Some((c, why)) = baseline
            .checks
            .iter()
            .find_map(|c| broken(c).map(|w| (c, w)))
    {
        stage(
            &*live,
            task,
            Stage::Baseline,
            StageState::Failed,
            Some(format!("`{}` cannot run", c.name)),
            Some(began),
        );
        // Configuration the checkout never committed is the likely cause,
        // and one speccoder can fix.
        let checkout = std::path::Path::new(&s.snapshot.root);
        if let Some(remedy) = crate::code::setup_remedy(checkout, Some(feature_name)) {
            return Err(Box::new(fail(
                CommandStatus::GitPrecondition,
                format!(
                    "check `{}` cannot run in the worktree: your checkout has not committed its setup — {why}",
                    c.name
                ),
            )
            .with_code("SETUP-INCOMPLETE")
            .with_fix(vec!["speccoder fix".to_owned(), remedy.command.clone()])
            .with_remedy(Some(remedy))));
        }
        return Err(Box::new(fail(
            CommandStatus::ConfigError,
            // A cause speccoder recognised comes with its fix; else the
            // general one.
            if why.contains("git add") || why.contains("--allow-failing-baseline") {
                format!(
                    "check `{}` cannot run on the untouched code, so it could never judge {task}: {why}",
                    c.name
                )
            } else {
                format!(
                    "check `{}` cannot run on the untouched code, so it could never judge {task}: {why} — fix the check (a file it needs may be uncommitted: `git status`), or run with --allow-failing-baseline",
                    c.name
                )
            },
        )
        .with_code("CHECK-BROKEN")));
    }
    // Failing tests at the baseline are reported, not blamed: the stage held.
    let failing = baseline
        .checks
        .iter()
        .filter(|c| c.status.is_failure())
        .count();
    stage(
        &*live,
        task,
        Stage::Baseline,
        StageState::Passed,
        (failing > 0).then(|| format!("{failing} check(s) already failing")),
        Some(began),
    );

    let rt = match tokio_runtime() {
        Ok(rt) => rt,
        Err(e) => return Err(Box::new(fail(CommandStatus::RunFailed, e))),
    };
    let bundle_value = match serde_json::to_value(bundle) {
        Ok(v) => v,
        Err(e) => return Err(Box::new(fail(CommandStatus::Error, e.to_string()))),
    };
    // The repository's instructions for agents, from the tree the
    // attempt starts in: for speccoder's own calls (the attempt, the test
    // authors); an external agent reads its own files.
    let instructions = crate::instructions::collect(
        wt_root,
        project,
        &speccoder_core::scope::named_paths(&bundle.task.title),
        &run_config.instructions,
    );

    // MCP servers, started in the worktree for this task's attempts.
    let servers = match mcp::parse(config_text) {
        Ok(s) => s,
        Err(e) => {
            return Err(Box::new(
                fail(CommandStatus::ConfigError, format!("speccoder.toml: {e}"))
                    .with_code("CONFIG-INVALID"),
            ));
        }
    };
    let mut connected = if servers.is_empty() {
        mcp::Connected::default()
    } else {
        live.event(&progress::Event::Notice {
            text: format!("starting {} MCP server(s)", servers.len()),
        });
        rt.block_on(mcp::connect(&servers, project, runner))
    };
    // An allowed tool its server changed since review needs approval again.
    let approved_tools: Vec<String> = task_grants
        .iter()
        .filter_map(|g| g.tool_name().map(str::to_owned))
        .collect();
    let pinned = mcp::pin_reviewed(&s.plane_dir, &mut connected.tools, &approved_tools);
    connected.notices.extend(pinned);

    // One plane for the whole task. Its tool server enforces denials; the
    // frozen-test approvals are the skill's, from its journaled input.
    let tools_server = match tools::WorkspaceTools::new(wt_root, policy.clone()) {
        Ok(t) => std::sync::Arc::new(t.with_runner(runner.clone())),
        Err(e) => return Err(Box::new(fail(CommandStatus::Error, e.to_string()))),
    };
    // Calls past their deadline are denied as expired before anything
    // resumes; a run that cannot tell does not start.
    let swept =
        match crate::approvals::expire_due(plane_dir, agentplane::core::Timestamp::now_utc()) {
            Ok(p) => p,
            Err(e) => {
                return Err(Box::new(fail(
                    CommandStatus::RunFailed,
                    format!("the calls past their deadline could not be denied: {e}"),
                )));
            }
        };
    // What a person is shown about the sandbox, for every call they decide.
    let present = crate::approvals::PresentContext {
        root: None,
        sandbox: runner.is_sandboxed().then(|| {
            if cfg!(target_os = "macos") {
                "Seatbelt".to_owned()
            } else {
                "Landlock+seccomp".to_owned()
            }
        }),
        network: match sandbox_config.network {
            sandbox::Network::Off => "none",
            sandbox::Network::Loopback => "loopback",
            sandbox::Network::On => "unrestricted",
        }
        .to_owned(),
        file_text: None,
    };
    let plane = match rt.block_on(async {
        substrate::Plane::open_with(
            plane_dir,
            &s.model,
            provider,
            Some(tools_server),
            substrate::Hooks {
                progress: Some(Arc::clone(&live)),
                mcp: connected.clients.clone(),
                mcp_tools: connected.tools.clone(),
                format: Some(format),
                present: present.clone(),
            },
        )
    }) {
        Ok(p) => p,
        Err(e) => return Err(Box::new(fail(CommandStatus::RunFailed, e.0))),
    };
    let notices: Vec<String> = elsewhere
        .replaced_journal
        .iter()
        .chain(swept.iter())
        .chain(plane.replaced_journal.iter())
        .map(|p| {
            format!(
                "the journal could not be read by this build and was moved to {}; past runs stay in git",
                p.display()
            )
        })
        .chain(connected.notices.iter().cloned())
        .chain(sandbox_notice.clone())
        .chain(contained_notice.clone())
        .chain(prepare_notes.iter().cloned())
        .chain(resumed_from.as_ref().map(|(stage, last_call)| {
            format!(
                "Resuming — {task} was interrupted during {}{}; finished calls will not repeat.",
                stage.map_or("a run", speccoder_core::stage::Stage::name),
                last_call
                    .as_ref()
                    .map(|c| format!(" ({c})"))
                    .unwrap_or_default()
            )
        }))
        .collect();

    // A Convergence task is the converge model's claim that a gap exists;
    // it changes no code until a failing test demonstrates the gap.
    let demand_red =
        run_config.acceptance && planning.is_none() && format.is_convergence(&bundle.task.phase);
    // Where this run started: an attempt that does not pass is undone to here.
    // Where an attempt that does not pass is undone to: the tree the task
    // started from. A resumed attempt (interrupted, or waiting for a
    // person) left its own writes in the tree, so its start is the branch
    // tip it began at — a task always starts from a clean tip.
    let start = match &interrupted {
        Some(_) => match git::Git::at(wt_root).line(&["rev-parse", "HEAD^{tree}"]) {
            Ok(tree) => worktree::Checkpoint(tree),
            Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
        },
        None => match worktree::checkpoint(wt_root, tmp) {
            Ok(c) => c,
            Err(e) => return Err(Box::new(fail(CommandStatus::GitPrecondition, e.0))),
        },
    };
    let Started {
        marker_guard,
        head_now,
        set_aside,
    } = started;
    Ok((
        Ready {
            rt,
            bundle_value,
            instructions,
            connected,
            present,
            plane,
            demand_red,
            start,
            copied_files,
            prepares,
        },
        Carried {
            marker_guard,
            head_now,
            set_aside,
            baseline,
            notices,
        },
    ))
}
