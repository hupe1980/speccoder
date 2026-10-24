//! The `speccoder` binary: parse, delegate to the runtime, render.

mod acp;
mod attention;
mod cli;
mod human;
mod output;
mod progress;
mod prompt;
mod tui;

use clap::Parser;
use speccoder_core::ExitCode;

use speccoder_runtime::verify::config::Tier;
use speccoder_runtime::verify::verify;

use crate::cli::{Cli, Command, TierArg};

fn no_state(command: &str) -> speccoder_core::CommandResult {
    speccoder_core::CommandResult::new(
        command,
        speccoder_core::CommandStatus::ConfigError,
        "no state directory (set SPECCODER_STATE_DIR or HOME)",
    )
}

/// `speccoder spec archive [<change>] [--yes] [--allow-unverified TASK]…`:
/// flags given after the change are read from the trailing arguments too.
fn archive(
    cli: &Cli,
    path: &std::path::Path,
    arguments: &[String],
    yes: bool,
    allow_unverified: &[String],
) -> speccoder_core::CommandResult {
    use speccoder_runtime::format::openspec::archive;
    use std::io::IsTerminal;
    let mut change = None;
    let mut yes = yes;
    let mut allowed = allow_unverified.to_vec();
    let mut args = arguments.iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--yes" | "-y" => yes = true,
            "--allow-unverified" => allowed.extend(args.next().cloned()),
            other => {
                if let Some(v) = other.strip_prefix("--allow-unverified=") {
                    allowed.push(v.to_owned());
                } else if change.is_none() {
                    change = Some(other.to_owned());
                }
            }
        }
    }
    let ask = |r: &speccoder_core::ArchiveReport| {
        if let Some(diff) = &r.diff {
            eprintln!("{diff}");
        }
        eprint!(
            "Archive {} into the main specs and commit it? [y/N] ",
            r.change
        );
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        matches!(line.trim(), "y" | "Y" | "yes")
    };
    let interactive =
        !cli.json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    archive::archive(
        path,
        &archive::Options {
            change: change.as_deref(),
            yes,
            allow_unverified: &allowed,
            confirm: interactive.then_some(&ask as &dyn Fn(&speccoder_core::ArchiveReport) -> bool),
            program: None,
        },
    )
}

fn main() {
    // The Linux sandbox helper: confine this process, then exec.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(speccoder_runtime::sandbox::HELPER_ARG) {
        eprintln!(
            "speccoder: {}",
            speccoder_runtime::sandbox::helper_main(&args[2..])
        );
        std::process::exit(126);
    }
    // The ACP agent: stdout is the protocol, so it never reaches
    // the renderer below.
    if args.get(1).map(String::as_str) == Some("acp") && args.len() == 2 {
        acp::serve(std::io::stdin().lock(), Box::new(std::io::stdout()));
        return;
    }
    // clap prints usage errors itself and exits with status 2 (ExitCode::USAGE).
    let cli = Cli::parse();
    if let Some(f) = cli.format {
        speccoder_runtime::format::request(f.into());
    }
    let result = match &cli.command {
        Command::Inspect { path, feature } => speccoder_runtime::inspect(path, feature.as_deref()),
        Command::Worktree { path, feature } => {
            speccoder_runtime::worktree(path, feature.as_deref())
        }
        Command::Propose {
            task,
            path,
            feature,
        } => speccoder_runtime::propose(path, feature.as_deref(), task),
        // `--return` (clap requires `--in-place`): no task runs.
        Command::Run {
            return_: true,
            path,
            ..
        } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::in_place::return_in_place(path, &state),
            None => speccoder_core::CommandResult::new(
                "run",
                speccoder_core::CommandStatus::ConfigError,
                "no state directory (set SPECCODER_STATE_DIR or HOME)",
            ),
        },
        Command::Run {
            task,
            max_tasks,
            converge,
            no_prompt,
            parallel,
            ignore_checklists,
            in_place,
            return_: _,
            path,
            feature,
        } => {
            // Ask inline only when someone is at a terminal.
            use std::io::IsTerminal;
            let approver: Option<std::sync::Arc<dyn speccoder_runtime::approvals::Approver>> =
                (!cli.json
                    && !*no_prompt
                    && std::io::stdin().is_terminal()
                    && std::io::stderr().is_terminal())
                .then(|| std::sync::Arc::new(prompt::Terminal) as _);
            // Live progress on stderr, unless stdout must stay one JSON object.
            let (progress, attention) = live_progress(&cli);
            let options = speccoder_runtime::RunOptions {
                max_tasks: *max_tasks,
                approver,
                progress,
                parallel: usize::from(*parallel),
                ignore_checklists: *ignore_checklists,
                in_place: *in_place,
                ..speccoder_runtime::RunOptions::default()
            };
            let result = match task {
                Some(task) => speccoder_runtime::run(path, feature.as_deref(), task, &options),
                None => {
                    speccoder_runtime::run_feature(path, feature.as_deref(), &options, *converge)
                }
            };
            let result = speccoder_runtime::next::attach(result, feature.as_deref());
            let result = if *in_place {
                speccoder_runtime::next::in_place(result)
            } else {
                result
            };
            if let Some(a) = &attention {
                a.finish(&result);
            }
            result
        }
        Command::Accept {
            task,
            reason,
            in_place,
            path,
            feature,
        } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::controls::accept_in(
                path,
                feature.as_deref(),
                task,
                reason,
                &state,
                *in_place,
            ),
            None => no_state("accept"),
        },
        Command::Skip {
            task,
            reason,
            path,
            feature,
            ..
        } => speccoder_runtime::controls::skip(path, feature.as_deref(), task, reason.as_deref()),
        Command::Nudge {
            task,
            note,
            path,
            feature,
            ..
        } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::controls::nudge_with(
                path,
                feature.as_deref(),
                task,
                note.as_deref(),
                &state,
            ),
            None => no_state("nudge"),
        },
        Command::Workflow {
            action: cli::WorkflowAction::Install { path, force },
        } => speccoder_runtime::workflow::install(path, *force),
        Command::Attest {
            action:
                cli::AttestAction::Verify {
                    rev,
                    path,
                    feature,
                    allowed_signers,
                    certificate_identity,
                    certificate_oidc_issuer,
                    require_signed,
                },
        } => speccoder_runtime::attest::verify::verify_at(
            path,
            feature.as_deref(),
            rev.as_deref(),
            &speccoder_runtime::attest::verify::Trust {
                allowed_signers: allowed_signers.clone(),
                identity: certificate_identity.clone(),
                issuer: certificate_oidc_issuer.clone(),
            },
            *require_signed,
        ),
        Command::Attest {
            action: cli::AttestAction::Sign { rev, path, feature },
        } => speccoder_runtime::attest::sign_at(path, feature.as_deref(), rev.as_deref()),
        Command::Skill {
            name,
            arguments,
            path,
            no_prompt,
        } => match name {
            None => speccoder_runtime::hosting::list(path),
            Some(name) => {
                use std::io::IsTerminal;
                let interactive = !cli.json
                    && !*no_prompt
                    && std::io::stdin().is_terminal()
                    && std::io::stderr().is_terminal();
                let (progress, _attention) = live_progress(&cli);
                let options = speccoder_runtime::RunOptions {
                    max_tasks: None,
                    approver: interactive.then(|| std::sync::Arc::new(prompt::Terminal) as _),
                    progress,
                    ..speccoder_runtime::RunOptions::default()
                };
                speccoder_runtime::hosting::run(path, name, &arguments.join(" "), &options)
            }
        },
        Command::Spec {
            name,
            arguments,
            path,
            no_prompt,
            yes,
            allow_unverified,
        } => match name.as_deref() {
            Some("archive") => archive(&cli, path, arguments, *yes, allow_unverified),
            None => speccoder_runtime::hosting::list(path),
            Some(name) => {
                use std::io::IsTerminal;
                let interactive = !cli.json
                    && !*no_prompt
                    && std::io::stdin().is_terminal()
                    && std::io::stderr().is_terminal();
                let (progress, _attention) = live_progress(&cli);
                let options = speccoder_runtime::RunOptions {
                    max_tasks: None,
                    approver: interactive.then(|| std::sync::Arc::new(prompt::Terminal) as _),
                    progress,
                    ..speccoder_runtime::RunOptions::default()
                };
                speccoder_runtime::hosting::run(path, name, &arguments.join(" "), &options)
            }
        },
        Command::Init {
            path,
            provider,
            model,
            force,
        } => speccoder_runtime::init::init(path, provider.as_deref(), model.as_deref(), *force),
        Command::Tui { path, feature } => tui::run(path, feature.as_deref()),
        Command::Status { path } => speccoder_runtime::progress::status(path),
        Command::Bench {
            action:
                cli::BenchAction::Import {
                    instances,
                    repos,
                    out,
                    limit,
                    test_cmd,
                },
        } => speccoder_runtime::bench::import(
            instances,
            &speccoder_runtime::bench::ImportOptions {
                repos,
                out,
                test_cmd: test_cmd
                    .as_ref()
                    .map(|c| c.split_whitespace().map(str::to_owned).collect()),
                limit: *limit,
            },
        ),
        Command::Acp => {
            acp::serve(std::io::stdin().lock(), Box::new(std::io::stdout()));
            return;
        }
        Command::Pr {
            path,
            feature,
            remote,
            base,
            yes,
            dry_run,
            force,
        } => {
            let ask = |o: &speccoder_runtime::handoff::PrOptions<'_>| {
                speccoder_runtime::handoff::pr(path, feature.as_deref(), o)
            };
            let mut options = speccoder_runtime::handoff::PrOptions {
                remote,
                base: base.as_deref(),
                gh: "gh",
                open: *yes,
                force: *force,
            };
            // Talking to a remote is a person's decision: show the
            // dry run, then ask, unless --yes; never without a terminal.
            use std::io::IsTerminal;
            if !*yes && !*dry_run && !cli.json && std::io::stdin().is_terminal() {
                let preview = ask(&options);
                if let Some(speccoder_core::CommandData::Pr(p)) = preview.data() {
                    eprintln!("{}\n\n{}\n", p.title, p.body);
                    for c in &p.commands {
                        eprintln!("  $ {c}");
                    }
                    eprint!(
                        "Push {} to {} and open a draft pull request against {}? [y/N] ",
                        p.branch, p.remote, p.base
                    );
                    let mut line = String::new();
                    let _ = std::io::stdin().read_line(&mut line);
                    options.open = matches!(line.trim(), "y" | "Y" | "yes");
                    if options.open { ask(&options) } else { preview }
                } else {
                    preview
                }
            } else {
                ask(&options)
            }
        }
        Command::Cost { path, feature } => {
            speccoder_runtime::controls::cost(path, feature.as_deref())
        }
        Command::Approvals { path, feature } => speccoder_runtime::approvals::decide(
            path,
            feature.as_deref(),
            speccoder_runtime::approvals::Verb::List,
        ),
        Command::Approve {
            task,
            call,
            all,
            scope,
            note,
            path,
            feature,
        } => speccoder_runtime::approvals::decide(
            path,
            feature.as_deref(),
            speccoder_runtime::approvals::Verb::Decide(speccoder_runtime::approvals::Choice {
                task: task.as_deref(),
                call: call.as_deref(),
                all: *all,
                approve: true,
                scope: (*scope).into(),
                note: note.as_deref(),
                reason: None,
            }),
        ),
        Command::Reject {
            task,
            call,
            all,
            reason,
            path,
            feature,
        } => speccoder_runtime::approvals::decide(
            path,
            feature.as_deref(),
            speccoder_runtime::approvals::Verb::Decide(speccoder_runtime::approvals::Choice {
                task: task.as_deref(),
                call: call.as_deref(),
                all: *all,
                approve: false,
                scope: speccoder_runtime::approvals::Scope::Task,
                note: None,
                reason: Some(reason),
            }),
        ),
        Command::Report { path, feature } => {
            speccoder_runtime::status::report(path, feature.as_deref())
        }
        Command::Converge {
            in_place,
            path,
            feature,
        } => speccoder_runtime::converge_in(path, feature.as_deref(), *in_place),
        Command::Eval {
            dir,
            case,
            keep,
            repeat,
            executor,
        } => speccoder_runtime::eval(
            dir,
            case.as_deref(),
            keep.as_deref(),
            usize::from(*repeat),
            executor.as_deref(),
        ),
        Command::Replay { run, path } => speccoder_runtime::replay(path, run),
        Command::Verify {
            path,
            tier,
            against,
        } => {
            let tier = tier.map(|t| match t {
                TierArg::Fast => Tier::Fast,
                TierArg::Full => Tier::Full,
            });
            // Checks run the project's code: confined like in a run.
            match speccoder_runtime::sandbox::runner_at(path) {
                Ok((runner, _)) => verify(path, tier, against.as_deref(), &runner),
                Err(e) => speccoder_core::CommandResult::new(
                    "verify",
                    speccoder_core::CommandStatus::ConfigError,
                    &e,
                ),
            }
        }
    };
    let result = speccoder_runtime::next::attach(result, feature_flag(&cli.command));
    // While an in-place session holds the project, the way on is in place.
    let result = match (
        decision_path(&cli.command),
        speccoder_runtime::worktree::state_dir(),
    ) {
        (Some(path), Some(state))
            if speccoder_runtime::in_place::session_at(path, &state).is_some() =>
        {
            speccoder_runtime::next::in_place(result)
        }
        _ => result,
    };
    let code = match output::render(&result, cli.json) {
        Ok(()) => result.exit_code(),
        Err(_) => ExitCode::INTERNAL,
    };
    std::process::exit(i32::from(code.code()));
}

/// How much progress `cli` asked for.
fn verbosity(cli: &cli::Cli) -> progress::Verbosity {
    if cli.quiet {
        progress::Verbosity::Quiet
    } else if cli.verbose {
        progress::Verbosity::Verbose
    } else {
        progress::Verbosity::Normal
    }
}

/// The `--feature` a command was given.
/// The project of a command whose next step is a run: decisions and
/// controls taken while an in-place session may hold the checkout.
fn decision_path(command: &cli::Command) -> Option<&std::path::Path> {
    use cli::Command as C;
    match command {
        C::Approvals { path, .. }
        | C::Approve { path, .. }
        | C::Reject { path, .. }
        | C::Skip { path, .. }
        | C::Nudge { path, .. }
        | C::Status { path } => Some(path.as_path()),
        _ => None,
    }
}

fn feature_flag(command: &cli::Command) -> Option<&str> {
    use cli::Command as C;
    match command {
        C::Run { feature, .. }
        | C::Converge { feature, .. }
        | C::Report { feature, .. }
        | C::Approvals { feature, .. }
        | C::Approve { feature, .. }
        | C::Reject { feature, .. }
        | C::Accept { feature, .. }
        | C::Skip { feature, .. }
        | C::Nudge { feature, .. } => feature.as_deref(),
        _ => None,
    }
}

/// Live progress for a run: the printer, and around it the attention layer
/// (notifications, title) — neither under `--json`.
fn live_progress(
    cli: &cli::Cli,
) -> (
    Option<std::sync::Arc<dyn speccoder_runtime::progress::Progress>>,
    Option<std::sync::Arc<attention::Attention>>,
) {
    use std::io::IsTerminal;
    if cli.json {
        return (None, None);
    }
    let printer: std::sync::Arc<dyn speccoder_runtime::progress::Progress> =
        std::sync::Arc::new(progress::Printer::new(verbosity(cli)));
    let (settings, warnings) = speccoder_runtime::user_config::load();
    for w in warnings {
        eprintln!("{w}");
    }
    let layer = attention::Attention::new(
        printer,
        settings,
        std::io::stderr().is_terminal(),
        Box::new(attention::Stderr),
        &|k| std::env::var(k).ok(),
    );
    layer.watch_stalls(std::time::Duration::from_secs(1));
    (Some(layer.clone() as _), Some(layer))
}
