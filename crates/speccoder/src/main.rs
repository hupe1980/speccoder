//! The `speccoder` binary: parse, delegate to the runtime, render.

mod acp;
#[cfg(window)]
mod app;
mod attention;
mod cli;
#[cfg(test)]
mod contract;
mod doctor;
mod events;
mod explain;
mod follow;
mod github;
mod help;
mod human;
mod markdown;
mod output;
mod progress;
mod prompt;
mod schema;
mod style;
mod tui;

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
    .with_code("NO-STATE-DIR")
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

/// `speccoder ui`: serves the web interface on 127.0.0.1 until interrupted.
/// The address carries the token in its fragment, which no server sees.
fn ui(
    path: &std::path::Path,
    port: u16,
    no_open: bool,
    window: bool,
    json: bool,
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandData, CommandResult, CommandStatus, UiReport};
    let root = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if window && !cfg!(window) {
        return CommandResult::new(
            "ui",
            CommandStatus::UsageError,
            "this speccoder was built without its window: `cargo install speccoder --features app`, or use the browser (`speccoder ui`)",
        );
    }
    // The window must own the main thread: the host serves from another.
    #[cfg(window)]
    if window {
        let (tx, rx) = std::sync::mpsc::channel();
        let served_root = root.clone();
        std::thread::spawn(move || {
            let _ = speccoder_web::serve(&served_root, port, |s| {
                let _ = tx.send((s.url, s.events));
            });
        });
        return match rx.recv() {
            Ok((url, events)) => match app::run(&url, events, root.clone()) {
                Ok(()) => CommandResult::new("ui", CommandStatus::Ok, "the window was closed"),
                Err(e) => CommandResult::new(
                    "ui",
                    CommandStatus::Error,
                    &format!("the window failed: {e}"),
                ),
            },
            Err(_) => CommandResult::new("ui", CommandStatus::Error, "the host did not start"),
        };
    }
    // Under `--json` the result is printed once the host listens — the
    // address a tool needs — and is the only one: the host serves on.
    let announced = std::sync::atomic::AtomicBool::new(false);
    let served = speccoder_web::serve(&root, port, |s: speccoder_web::Started| {
        if json {
            let ready = CommandResult::new("ui", CommandStatus::Ok, "serving the web interface")
                .with_data(CommandData::Ui(Box::new(UiReport {
                    url: s.url.clone(),
                    port: s.port,
                    root: root.display().to_string(),
                })));
            if output::render(&ready, true).is_ok() {
                announced.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        eprintln!("speccoder is serving {} at", root.display());
        eprintln!("  {}", s.url);
        eprintln!(
            "  (127.0.0.1 only; the address holds this session's key — do not share it; ctrl-c stops)"
        );
        if !no_open {
            let opener = if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            };
            let _ = std::process::Command::new(opener)
                .arg(&s.url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    });
    if announced.load(std::sync::atomic::Ordering::SeqCst) && served.is_ok() {
        std::process::exit(0);
    }
    match served {
        Ok(()) => CommandResult::new("ui", CommandStatus::Ok, "stopped serving"),
        Err(e) => CommandResult::new("ui", CommandStatus::Error, &format!("could not serve: {e}")),
    }
}

/// `speccoder config [get|set|unset|commit]`.
fn config(
    path: &std::path::Path,
    action: Option<&cli::ConfigAction>,
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus, ConfigChange};
    use speccoder_runtime::settings;
    let change = |key: &str, value| {
        vec![ConfigChange {
            key: key.to_owned(),
            value,
            append: None,
        }]
    };
    match action {
        None => settings::show(path),
        Some(cli::ConfigAction::Get { key }) => {
            let report = settings::read(path);
            let (section, rest) = key.split_once('.').unwrap_or((key.as_str(), ""));
            let found = report
                .sections
                .iter()
                .find(|s| s.name == section)
                .and_then(|s| {
                    if s.named {
                        let (table, k) = rest.split_once('.')?;
                        s.tables
                            .iter()
                            .find(|t| t.name == table)?
                            .keys
                            .iter()
                            .find(|x| x.key == k)
                            .cloned()
                    } else {
                        s.keys.iter().find(|x| x.key == rest).cloned()
                    }
                });
            match found {
                Some(k) => {
                    let shown = match (&k.value, &k.default) {
                        (Some(v), _) => v.to_string(),
                        (None, Some(d)) => format!("{d} (default)"),
                        (None, None) => "unset".to_owned(),
                    };
                    CommandResult::new("config", CommandStatus::Ok, &format!("{key} = {shown}"))
                        .with_data(speccoder_core::CommandData::Config(Box::new(report)))
                }
                None => CommandResult::new(
                    "config",
                    CommandStatus::NotFound,
                    &format!("no key `{key}` in speccoder.toml's catalogue"),
                )
                .with_fix(vec!["speccoder config".to_owned()]),
            }
        }
        Some(cli::ConfigAction::Set { key, value }) => match settings::value_of(key, value) {
            Ok(v) => settings::apply(path, &change(key, Some(v))),
            Err(e) => CommandResult::new("config", CommandStatus::ConfigError, &e)
                .with_code("CONFIG-INVALID")
                .with_fix(vec![format!("speccoder config get {key}")]),
        },
        Some(cli::ConfigAction::Unset { key }) => settings::apply(path, &change(key, None)),
        Some(cli::ConfigAction::Commit) => settings::commit(path),
    }
}

/// Whether the planning skill `name` creates a feature (it needs none).
fn creates(path: &std::path::Path, name: &str) -> bool {
    let planning = speccoder_runtime::format::at(path).planning();
    let short = name.strip_prefix(planning.prefix).unwrap_or(name);
    planning.creates.contains(&short)
}

/// Asks which feature to work on when several exist and none is current,
/// and makes the answer current. Nothing happens otherwise; an empty answer
/// leaves the command to say what is missing.
fn pick_feature(path: &std::path::Path) {
    use std::io::Write as _;
    let format = speccoder_runtime::format::at(path);
    let Some(snap) = format.open(path, None) else {
        return;
    };
    if snap.feature.is_some() || snap.features.len() < 2 {
        return;
    }
    let listed = speccoder_runtime::current::list(path, None);
    let Some(speccoder_core::CommandData::Features(f)) = listed.data() else {
        return;
    };
    let unit = &f.unit;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "Which {unit}? None is current.");
    for line in human::features(f) {
        let _ = writeln!(err, "{line}");
    }
    loop {
        let _ = write!(err, "Number or name (enter: none) > ");
        let _ = err.flush();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            return;
        }
        let r = speccoder_runtime::current::switch(path, line.trim());
        let _ = writeln!(err, "{}", r.message());
        if r.status() == speccoder_core::CommandStatus::Ok {
            return;
        }
    }
}

/// The dashboard is for a person at a terminal: stdin, stdout and stderr
/// are terminals, and no machine output, `--plain` or screen reader.
fn dashboard(cli: &Cli) -> bool {
    use std::io::IsTerminal;
    !cli.json
        && !cli.events
        && !cli.plain
        && !style::out().screen_reader
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

/// What `speccoder run` was asked to do, owned so a hosted run can take it
/// to its thread.
struct RunJob {
    path: std::path::PathBuf,
    feature: Option<String>,
    task: Option<String>,
    max_tasks: Option<usize>,
    converge: bool,
    parallel: usize,
    ignore_checklists: bool,
    fresh_acceptance: bool,
    allow_failing_baseline: bool,
    in_place: bool,
}

impl RunJob {
    fn run(
        &self,
        approver: Option<std::sync::Arc<dyn speccoder_runtime::approvals::Approver>>,
        progress: Option<std::sync::Arc<dyn speccoder_runtime::progress::Progress>>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> speccoder_core::CommandResult {
        let (path, feature) = (self.path.as_path(), self.feature.as_deref());
        let options = speccoder_runtime::RunOptions {
            max_tasks: self.max_tasks,
            approver,
            progress,
            cancel,
            parallel: self.parallel,
            ignore_checklists: self.ignore_checklists,
            fresh_acceptance: self.fresh_acceptance,
            allow_failing_baseline: self.allow_failing_baseline,
            in_place: self.in_place,
            ..speccoder_runtime::RunOptions::default()
        };
        let result = match &self.task {
            Some(task) => speccoder_runtime::run(path, feature, task, &options),
            None => speccoder_runtime::run_feature(path, feature, &options, self.converge),
        };
        let result = speccoder_runtime::next::attach(result, feature);
        if self.in_place {
            speccoder_runtime::next::in_place(result)
        } else {
            result
        }
    }
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
    // The grouped help is built only when help is asked for: building it
    // costs a second pass over the CLI on every start (cold-start budget).
    let wants_help = args.len() == 1
        || args
            .iter()
            .skip(1)
            .any(|a| matches!(a.as_str(), "-h" | "--help" | "help"));
    let cmd = if wants_help {
        help::command()
    } else {
        <Cli as clap::CommandFactory>::command()
    };
    let parsed = cmd
        .try_get_matches_from(&args)
        .and_then(|m| <Cli as clap::FromArgMatches>::from_arg_matches(&m));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(e) => usage_error(&args, &e),
    };
    let (settings, warnings) = speccoder_runtime::user_config::load();
    let machine = cli.json || cli.events;
    if !machine {
        for w in &warnings {
            eprintln!("{w}");
        }
    }
    let (looks, reader_source) = resolve_looks(&cli, &settings.ui);
    style::install(looks);
    if let (Some(source), false) = (reader_source, machine) {
        eprintln!("screen reader mode: on ({source})");
    }
    let notify = settings.notify;
    let events = cli.events.then(|| {
        std::sync::Arc::new(events::Events::start(
            Box::new(std::io::stdout()),
            command_name(cli.command.as_ref()),
        ))
    });
    if let Some(f) = cli.format {
        speccoder_runtime::format::request(f.into());
    }
    let path = cli.project.as_path();
    let feature = cli.feature.as_deref();
    // Ask inline only when someone is at a terminal.
    let interactive = |no_prompt: bool| {
        use std::io::IsTerminal;
        !cli.json && !no_prompt && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    };
    let Some(command) = &cli.command else {
        // Bare `speccoder`: the dashboard at a terminal, else the report.
        let result = if dashboard(&cli) {
            tui::run(path, feature)
        } else {
            speccoder_runtime::status::report(path, feature)
        };
        finish(&cli, result, events.as_ref());
    };
    // Several features and none current: at a terminal, ask rather than fail.
    if feature.is_none() && !machine && interactive(false) {
        let needs = match command {
            Command::Run { return_: false, .. } => true,
            Command::Spec { name: Some(n), .. } => n != "archive" && !creates(path, n),
            _ => false,
        };
        if needs {
            pick_feature(path);
        }
    }
    let result = match command {
        Command::Help { topic } => {
            help::print(topic);
            return;
        }
        Command::Completion { shell } => {
            help::completion(*shell);
            return;
        }
        Command::Inspect => speccoder_runtime::inspect(path, feature),
        Command::Worktree => speccoder_runtime::worktree(path, feature),
        Command::Propose { task } => speccoder_runtime::propose(path, feature, task),
        Command::Doctor { bug_report } => {
            let report = speccoder_runtime::doctor::run(path, feature, Some(&look_line()));
            if *bug_report && !machine {
                print!("{}", doctor::bug_report(&report));
                return;
            }
            report
        }
        // `--return` (clap requires `--in-place`): no task runs.
        Command::Run { return_: true, .. } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::in_place::return_in_place(path, &state),
            None => no_state("run"),
        },
        Command::Run {
            task,
            max_tasks,
            converge,
            no_prompt,
            parallel,
            ignore_checklists,
            fresh_acceptance,
            allow_failing_baseline,
            in_place,
            return_: _,
        } => {
            let job = RunJob {
                path: path.to_path_buf(),
                feature: feature.map(str::to_owned),
                task: task.clone(),
                max_tasks: *max_tasks,
                converge: *converge,
                parallel: usize::from(*parallel),
                ignore_checklists: *ignore_checklists,
                fresh_acceptance: *fresh_acceptance,
                allow_failing_baseline: *allow_failing_baseline,
                in_place: *in_place,
            };
            if dashboard(&cli) {
                // The dashboard hosts the run: progress and questions go to it.
                let ask = interactive(*no_prompt);
                let notify = notify.clone();
                tui::host_run(path, feature, move |h| {
                    let layer = attention::Attention::new(
                        h.progress,
                        notify,
                        true,
                        Box::new(attention::Stderr),
                        &|k| std::env::var(k).ok(),
                    );
                    layer.watch_stalls(std::time::Duration::from_secs(1));
                    let approver = ask.then_some(h.approver);
                    let result = job.run(approver, Some(layer.clone() as _), Some(h.cancel));
                    layer.finish(&result);
                    result
                })
            } else {
                let approver: Option<std::sync::Arc<dyn speccoder_runtime::approvals::Approver>> =
                    interactive(*no_prompt).then(|| std::sync::Arc::new(prompt::Terminal) as _);
                // Live progress on stderr, unless stdout must stay one JSON object.
                let (progress, attention) = live_progress(&cli, &notify, events.as_ref());
                let result = job.run(approver, progress, None);
                if let Some(a) = &attention {
                    a.finish(&result);
                }
                result
            }
        }
        Command::Task { action } => task_action(action, path, feature),
        Command::Ui {
            port,
            no_open,
            window,
        } => ui(path, *port, *no_open, *window, cli.json),
        Command::Config { action } => config(path, action.as_ref()),
        Command::Inbox => speccoder_runtime::inbox::inbox(path, feature),
        Command::Issue {
            action: None | Some(cli::IssueAction::List),
        } => speccoder_runtime::github::list(&speccoder_runtime::github::gh(), path),
        Command::Issue {
            action: Some(cli::IssueAction::Start { issue, no_prompt }),
        } => {
            match speccoder_runtime::github::issue(&speccoder_runtime::github::gh(), path, issue) {
                Err(e) => speccoder_runtime::github::refused(&e),
                Ok(found) => {
                    // The format's creating skill (Spec Kit's specify), as `spec` runs it.
                    let planning = speccoder_runtime::format::at(path).planning();
                    match planning.creates.first() {
                        None => speccoder_core::CommandResult::new(
                            "issue",
                            speccoder_core::CommandStatus::UsageError,
                            "this spec format has no skill that creates a feature",
                        ),
                        Some(skill) => {
                            let (progress, _attention) =
                                live_progress(&cli, &notify, events.as_ref());
                            let options = speccoder_runtime::RunOptions {
                                max_tasks: None,
                                approver: interactive(*no_prompt)
                                    .then(|| std::sync::Arc::new(prompt::Terminal) as _),
                                progress,
                                ..speccoder_runtime::RunOptions::default()
                            };
                            let name = format!("{}{skill}", planning.prefix);
                            speccoder_runtime::hosting::run(
                                path,
                                feature,
                                &name,
                                &speccoder_runtime::github::description(&found),
                                &options,
                            )
                        }
                    }
                }
            }
        }
        Command::Feature { query, clear } => match (query, clear) {
            (_, true) => speccoder_runtime::current::clear(path),
            (Some(q), false) => speccoder_runtime::current::switch(path, q),
            (None, false) => speccoder_runtime::current::list(path, feature),
        },
        Command::Workflow {
            action: cli::WorkflowAction::Install { force },
        } => speccoder_runtime::workflow::install(path, *force),
        Command::Attest {
            action:
                cli::AttestAction::Verify {
                    rev,
                    allowed_signers,
                    certificate_identity,
                    certificate_oidc_issuer,
                    require_signed,
                },
        } => speccoder_runtime::attest::verify::verify_at(
            path,
            feature,
            rev.as_deref(),
            &speccoder_runtime::attest::verify::Trust {
                allowed_signers: allowed_signers.clone(),
                identity: certificate_identity.clone(),
                issuer: certificate_oidc_issuer.clone(),
            },
            *require_signed,
        ),
        Command::Attest {
            action: cli::AttestAction::Sign { rev },
        } => speccoder_runtime::attest::sign_at(path, feature, rev.as_deref()),
        Command::Spec {
            name,
            arguments,
            no_prompt,
            yes,
            allow_unverified,
        } => match name.as_deref() {
            Some("archive") => archive(&cli, path, arguments, *yes, allow_unverified),
            None => speccoder_runtime::hosting::list(path),
            Some(name) => {
                let (progress, _attention) = live_progress(&cli, &notify, events.as_ref());
                let options = speccoder_runtime::RunOptions {
                    max_tasks: None,
                    approver: interactive(*no_prompt)
                        .then(|| std::sync::Arc::new(prompt::Terminal) as _),
                    progress,
                    ..speccoder_runtime::RunOptions::default()
                };
                speccoder_runtime::hosting::run(path, feature, name, &arguments.join(" "), &options)
            }
        },
        Command::Init {
            provider,
            model,
            force,
            no_verify,
            yes,
        } => init(
            &cli,
            path,
            provider.as_deref(),
            model.as_deref(),
            *force,
            *no_verify,
            *yes,
        ),
        // A screen reader gets the report, not a redrawn screen (F-055).
        // Not at a terminal, `--plain` or a screen reader: the report (F-055).
        Command::Tui if !dashboard(&cli) => {
            if !machine {
                eprintln!("notice: no dashboard here; this is the report");
            }
            speccoder_runtime::status::report(path, feature)
        }
        Command::Tui => tui::run(path, feature),
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
            remote,
            base,
            yes,
            dry_run,
            force,
        } => {
            let gh = speccoder_runtime::github::gh();
            let ask = |o: &speccoder_runtime::handoff::PrOptions<'_>| {
                speccoder_runtime::handoff::pr(path, feature, o)
            };
            let mut options = speccoder_runtime::handoff::PrOptions {
                remote,
                base: base.as_deref(),
                gh: &gh,
                open: *yes,
                force: *force,
            };
            // Talking to a remote is a person's decision: show the
            // dry run, then ask, unless --yes; never without a terminal.
            use std::io::IsTerminal;
            if !*yes && !*dry_run && !machine && std::io::stdin().is_terminal() {
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
        Command::Report { live: true, .. } => speccoder_runtime::progress::status(path),
        Command::Report { cost: true, .. } => speccoder_runtime::controls::cost(path, feature),
        Command::Report {
            approvals: true, ..
        } => speccoder_runtime::approvals::decide(
            path,
            feature,
            speccoder_runtime::approvals::Verb::List,
        ),
        Command::Report { .. } => speccoder_runtime::status::report(path, feature),
        Command::Explain { code, list } => explain::command(code.as_deref(), *list),
        Command::Schema { name } => schema::command(name.as_deref()),
        Command::Converge { in_place } => speccoder_runtime::converge_in(path, feature, *in_place),
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
        Command::Replay { run } => speccoder_runtime::replay(path, run),
        Command::Verify { tier, against } => {
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
    finish(&cli, result, events.as_ref());
}

/// Next steps and codes, the in-place adjustment, rendering (human, JSON,
/// events, Markdown), GitHub Actions, and the exit status.
fn finish(
    cli: &Cli,
    result: speccoder_core::CommandResult,
    events: Option<&std::sync::Arc<events::Events>>,
) -> ! {
    let path = cli.project.as_path();
    let feature = cli.feature.as_deref();
    let result = speccoder_runtime::next::attach(result, feature);
    // While an in-place session holds the project, the way on is in place.
    let decision = matches!(
        cli.command,
        Some(Command::Task { .. } | Command::Report { .. }) | None
    );
    let result = match speccoder_runtime::worktree::state_dir() {
        Some(state)
            if decision && speccoder_runtime::in_place::session_at(path, &state).is_some() =>
        {
            speccoder_runtime::next::in_place(result)
        }
        _ => result,
    };
    let markdown = matches!(cli.command, Some(Command::Report { markdown: true, .. }));
    let rendered = match events {
        Some(stream) => {
            stream.result(&result);
            Ok(())
        }
        None if markdown && !cli.json => markdown::render(&result, &mut std::io::stdout().lock()),
        None => output::render(&result, cli.json),
    };
    // In GitHub Actions, a run's summary is the feature's report after it.
    let after = match &cli.command {
        Some(Command::Run { .. }) if std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true") => {
            Some(speccoder_runtime::status::report(path, feature))
        }
        _ => None,
    };
    github::report(&result, cli.json || cli.events, after.as_ref());
    let code = match rendered {
        Ok(()) => result.exit_code(),
        Err(_) => ExitCode::INTERNAL,
    };
    // At a terminal, the next step is offered rather than only printed.
    let code = if result.status() == speccoder_core::CommandStatus::Ok && offers_next(cli) {
        follow::offer(path, feature, verbosity(cli), code)
    } else {
        code
    };
    std::process::exit(i32::from(code.code()));
}

/// Whether the next step is offered after this command: a planning step, a
/// decision on a task, or the report — at a terminal, for a person.
fn offers_next(cli: &Cli) -> bool {
    use std::io::IsTerminal;
    if cli.json || cli.events || !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal()
    {
        return false;
    }
    match &cli.command {
        Some(Command::Spec {
            name: Some(n),
            no_prompt: false,
            ..
        }) => n != "archive",
        Some(Command::Task { action }) => !matches!(action, cli::TaskAction::Show { .. }),
        Some(Command::Report {
            live: false,
            cost: false,
            approvals: false,
            markdown: false,
        }) => true,
        // Bare `speccoder`: after the report, or when the dashboard handed off.
        None => !dashboard(cli) || tui::handed_off(),
        _ => false,
    }
}

/// A command line clap refused. Text for a person; with `--json` or
/// `--events` among the arguments, the result envelope (code USAGE, or
/// USAGE-CONFLICT for two exclusive flags) — a script never parses clap's
/// text. `--help` and `--version` stay text.
fn usage_error(args: &[String], e: &clap::Error) -> ! {
    use clap::error::ErrorKind;
    let machine = |flag: &str| args.iter().skip(1).any(|a| a == flag);
    let (json, stream) = (machine("--json"), machine("--events"));
    let text_only = matches!(
        e.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    if text_only {
        e.exit();
    }
    // The command the line names: a known subcommand, or a former one.
    let known: Vec<String> = help::command()
        .get_subcommands()
        .map(|c| c.get_name().to_owned())
        .collect();
    let named = args
        .iter()
        .skip(1)
        .find(|a| known.iter().any(|k| k == *a) || help::FORMER.iter().any(|(old, _)| old == a));
    let command = named.map_or("speccoder", String::as_str);
    let conflict = e.kind() == ErrorKind::ArgumentConflict && json && stream;
    let message = e
        .to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches("error: ")
        .to_owned();
    let former = help::FORMER
        .iter()
        .find(|(old, _)| Some(*old) == named.map(String::as_str));
    // A path or a task id where none is taken: say where it goes.
    let stray = (e.kind() == ErrorKind::UnknownArgument)
        .then(|| {
            args.iter()
                .skip(1)
                .find(|a| message.contains(&format!("'{a}'")))
                .cloned()
        })
        .flatten();
    let (message, fix): (String, Vec<String>) = match (former, &stray) {
        (Some((old, new)), _) => (
            format!("`speccoder {old}` is now `{new}`"),
            vec![(*new).to_owned()],
        ),
        (None, Some(a)) if std::path::Path::new(a).is_dir() => (
            format!("{message}; a project path is given with -C"),
            vec![format!("speccoder -C {a} {command}")],
        ),
        (None, Some(a)) if a.starts_with('T') && a[1..].chars().all(|c| c.is_ascii_digit()) => (
            format!("{message}; a task is given with --task, or as `task <verb> {a}`"),
            vec![
                format!("speccoder {command} --task {a}"),
                format!("speccoder task show {a}"),
            ],
        ),
        _ => (message, vec![format!("speccoder {command} --help")]),
    };
    let result = speccoder_core::CommandResult::new(
        command,
        speccoder_core::CommandStatus::UsageError,
        &message,
    )
    .with_code(if conflict { "USAGE-CONFLICT" } else { "USAGE" })
    .with_fix(if command == "speccoder" && former.is_none() {
        vec!["speccoder help".to_owned()]
    } else {
        fix
    });
    let result = speccoder_runtime::next::attach(result, None);
    if !(json || stream) && former.is_none() && stray.is_none() {
        // clap's own message, with its usage line and suggestions.
        e.exit();
    }
    if stream {
        events::Events::start(Box::new(std::io::stdout()), command).result(&result);
    } else {
        let _ = output::render(&result, json);
    }
    std::process::exit(i32::from(result.exit_code().code()));
}

/// The look, in one line, for `doctor`.
fn look_line() -> String {
    let l = style::out();
    format!(
        "colour {}, theme {:?}, {} marks, {} columns{}",
        if l.color { "on" } else { "off" },
        l.theme,
        if l.ascii { "ASCII" } else { "Unicode" },
        l.width,
        if l.screen_reader {
            ", screen reader"
        } else {
            ""
        }
    )
    .to_lowercase()
}

/// `speccoder init`: write the configuration, run its checks on the
/// untouched code, and at a terminal offer to commit it (F-057). Without a
/// terminal, with `--json` or `--yes`, nothing is committed; the fix line
/// says what to commit.
fn init(
    cli: &Cli,
    path: &std::path::Path,
    provider: Option<&str>,
    model: Option<&str>,
    force: bool,
    no_verify: bool,
    yes: bool,
) -> speccoder_core::CommandResult {
    use std::io::IsTerminal;
    if !no_verify && !cli.json && !cli.events {
        eprintln!("Writing speccoder.toml, then running its checks on the untouched code…");
    }
    let mut prepared =
        match speccoder_runtime::init::prepare(path, provider, model, force, no_verify) {
            Ok(p) => p,
            Err(refused) => return *refused,
        };
    let interactive = !yes
        && !cli.json
        && !cli.events
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal();
    if interactive {
        let default_yes = prepared.failed.is_empty();
        // What the answer rests on, before the question: each check on the
        // untouched code, and why one fails (the full list follows after).
        for r in &prepared.report.ran {
            match (&r.reason, r.passed) {
                (_, true) => eprintln!("  {:<10} passes on the untouched code", r.name),
                (Some(why), false) => {
                    eprintln!("  {:<10} FAILS on the untouched code: {why}", r.name)
                }
                (None, false) => eprintln!("  {:<10} FAILS on the untouched code", r.name),
            }
        }
        eprint!(
            "Commit {}? speccoder reads committed files only. [{}] ",
            prepared.to_commit.join(" and "),
            if default_yes { "Y/n" } else { "y/N" }
        );
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let answer = line.trim().to_lowercase();
        let commit = if answer.is_empty() {
            default_yes
        } else {
            answer.starts_with('y')
        };
        if commit {
            match speccoder_runtime::init::commit_written(&prepared.root, &prepared.to_commit) {
                Ok(id) => prepared.report.committed = Some(id),
                Err(e) => eprintln!("the commit failed: {e}"),
            }
        }
    }
    speccoder_runtime::init::finish(prepared)
}

/// The subcommand's name, as the envelope's `command` names it.
fn command_name(command: Option<&Command>) -> &'static str {
    use Command as C;
    match command {
        None => "report",
        Some(c) => match c {
            C::Init { .. } => "init",
            C::Doctor { .. } => "doctor",
            C::Spec { .. } => "spec",
            C::Feature { .. } => "feature",
            C::Ui { .. } => "ui",
            C::Config { .. } => "config",
            C::Issue { .. } => "issue",
            C::Inbox => "inbox",
            C::Run { .. } => "run",
            C::Task { action } => match action {
                cli::TaskAction::Approve { .. } => "task approve",
                cli::TaskAction::Reject { .. } => "task reject",
                cli::TaskAction::Accept { .. } => "task accept",
                cli::TaskAction::Skip { .. } => "task skip",
                cli::TaskAction::Nudge { .. } => "task nudge",
                cli::TaskAction::Rule { .. } => "task rule",
                cli::TaskAction::Show { .. } => "task show",
            },
            C::Report { .. } => "report",
            C::Pr { .. } => "pr",
            C::Attest { .. } => "attest",
            C::Help { .. } => "help",
            C::Verify { .. } => "verify",
            C::Converge { .. } => "converge",
            C::Inspect => "inspect",
            C::Replay { .. } => "replay",
            C::Worktree => "worktree",
            C::Acp => "acp",
            C::Workflow { .. } => "workflow",
            C::Eval { .. } => "eval",
            C::Bench { .. } => "bench",
            C::Propose { .. } => "propose",
            C::Explain { .. } => "explain",
            C::Schema { .. } => "schema",
            C::Completion { .. } => "completion",
            C::Tui => "tui",
        },
    }
}

/// `speccoder task <verb> TASK`.
fn task_action(
    action: &cli::TaskAction,
    path: &std::path::Path,
    feature: Option<&str>,
) -> speccoder_core::CommandResult {
    use cli::TaskAction as T;
    use speccoder_runtime::approvals::{Choice, Scope, Verb, decide};
    match action {
        T::Approve {
            task,
            call,
            all,
            scope,
            note,
        } => decide(
            path,
            feature,
            Verb::Decide(Choice {
                task: task.as_deref(),
                call: call.as_deref(),
                all: *all,
                approve: true,
                scope: (*scope).into(),
                note: note.as_deref(),
                reason: None,
            }),
        ),
        T::Reject {
            task,
            call,
            all,
            reason,
        } => decide(
            path,
            feature,
            Verb::Decide(Choice {
                task: task.as_deref(),
                call: call.as_deref(),
                all: *all,
                approve: false,
                scope: Scope::Task,
                note: None,
                reason: Some(reason),
            }),
        ),
        T::Accept {
            task,
            reason,
            in_place,
        } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::controls::accept_in(
                path, feature, task, reason, &state, *in_place,
            ),
            None => no_state("task accept"),
        },
        T::Skip { task, reason, .. } => {
            speccoder_runtime::controls::skip(path, feature, task, reason.as_deref())
        }
        T::Nudge { task, note, .. } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::controls::nudge_with(
                path,
                feature,
                task,
                note.as_deref(),
                &state,
            ),
            None => no_state("task nudge"),
        },
        T::Rule {
            task,
            reason,
            variant,
            ..
        } => match speccoder_runtime::worktree::state_dir() {
            Some(state) => speccoder_runtime::controls::rule(
                path,
                feature,
                task,
                reason,
                variant.as_deref(),
                &state,
            ),
            None => no_state("task rule"),
        },
        T::Show { task } => speccoder_runtime::status::show(path, feature, task),
    }
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

/// Live progress for a run: the printer, and around it the attention layer
/// (notifications, title) — neither under `--json`.
fn live_progress(
    cli: &cli::Cli,
    notify: &speccoder_runtime::user_config::Notify,
    events: Option<&std::sync::Arc<events::Events>>,
) -> (
    Option<std::sync::Arc<dyn speccoder_runtime::progress::Progress>>,
    Option<std::sync::Arc<attention::Attention>>,
) {
    use std::io::IsTerminal;
    // `--events`: the stream is the progress; stderr stays quiet.
    if let Some(stream) = events {
        return (Some(std::sync::Arc::clone(stream) as _), None);
    }
    if cli.json {
        return (None, None);
    }
    let printer: std::sync::Arc<dyn speccoder_runtime::progress::Progress> =
        std::sync::Arc::new(progress::Printer::new(verbosity(cli)));
    let layer = attention::Attention::new(
        printer,
        notify.clone(),
        std::io::stderr().is_terminal(),
        Box::new(attention::Stderr),
        &|k| std::env::var(k).ok(),
    );
    layer.watch_stalls(std::time::Duration::from_secs(1));
    (Some(layer.clone() as _), Some(layer))
}

/// The looks of stdout and stderr (F-055): `--color`, then `[ui] color`,
/// then the environment and whether each is a terminal; never colour in
/// machine output. Also what turned screen-reader mode on, if anything.
fn resolve_looks(
    cli: &cli::Cli,
    ui: &speccoder_runtime::user_config::Ui,
) -> (style::Looks, Option<&'static str>) {
    use std::io::IsTerminal;
    let flag = match cli.color {
        cli::ColorArg::Auto => style::Choice::Auto,
        cli::ColorArg::Always => style::Choice::Always,
        cli::ColorArg::Never => style::Choice::Never,
    };
    let setting = match ui.color.as_deref() {
        Some("always") => style::Choice::Always,
        Some("never") => style::Choice::Never,
        _ => style::Choice::Auto,
    };
    let env = style::Env::current();
    let var = |k: &str| std::env::var(k).ok();
    let reader_env = var("SPECCODER_SCREEN_READER").filter(|v| !v.is_empty() && v != "0");
    let reader_source = if cli.screen_reader {
        Some("--screen-reader")
    } else if reader_env.is_some() {
        Some("SPECCODER_SCREEN_READER")
    } else if ui.screen_reader == Some(true) {
        Some("your settings")
    } else {
        None
    };
    let ascii = cli.ascii
        || ui.ascii.unwrap_or_else(|| {
            style::locale_is_ascii(
                var("LC_ALL").as_deref(),
                var("LC_CTYPE").as_deref(),
                var("LANG").as_deref(),
            )
        });
    let theme = ui
        .theme
        .as_deref()
        .and_then(style::Theme::parse)
        .unwrap_or_default();
    let machine = cli.json || cli.events;
    let look = |terminal: bool| style::Look {
        color: !machine && reader_source.is_none() && style::decide(flag, setting, &env, terminal),
        theme,
        ascii: ascii || reader_source.is_some(),
        width: style::columns(terminal, var("COLUMNS").as_deref()),
        screen_reader: reader_source.is_some(),
    };
    (
        style::Looks {
            out: look(std::io::stdout().is_terminal()),
            err: look(std::io::stderr().is_terminal()),
        },
        reader_source,
    )
}
