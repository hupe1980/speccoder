//! The `speccoder` binary: parse, delegate to the runtime, render.

mod acp;
mod actions;
#[cfg(window)]
mod app;
#[cfg(test)]
mod approval_renderers;
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
#[cfg(test)]
mod typescript;

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

/// `speccoder attest verify --branch`: the branch verifier's check (the
/// same function as `speccoder-verify check`), with this machine's trust —
/// the flags, else git's `gpg.ssh.allowedSignersFile`.
#[allow(clippy::too_many_arguments)] // the command's flags, one each
fn attest_branch(
    path: &std::path::Path,
    head: Option<&str>,
    base: Option<&str>,
    allowed_signers: Option<&std::path::Path>,
    allowed_signers_rev: Option<&str>,
    identity: Option<&str>,
    issuer: Option<&str>,
    require_signed: bool,
) -> speccoder_core::CommandResult {
    use speccoder_runtime::attest::branch::{AllowedSigners, Options, verify};
    const COMMAND: &str = "attest verify --branch";
    let configured = || {
        speccoder_runtime::git::Git::at(path)
            .line(&["config", "gpg.ssh.allowedSignersFile"])
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
                (Some(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
                _ => std::path::PathBuf::from(p),
            })
    };
    let allowed = match (allowed_signers, allowed_signers_rev) {
        (Some(f), _) => Some(AllowedSigners::File(f.to_path_buf())),
        (None, Some(spec)) => match AllowedSigners::parse_rev(spec) {
            Ok(a) => Some(a),
            Err(e) => {
                return speccoder_core::CommandResult::new(
                    COMMAND,
                    speccoder_core::CommandStatus::UsageError,
                    &e,
                );
            }
        },
        (None, None) => configured().map(AllowedSigners::File),
    };
    let opts = Options {
        head: head.unwrap_or("HEAD").to_owned(),
        base: base.map(str::to_owned),
        allowed_signers: allowed,
        identity: identity.map(str::to_owned),
        issuer: issuer.map(str::to_owned),
        require_signed,
        ..Options::default()
    };
    verify(path, &opts, COMMAND)
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
            "this build has no window (on Linux, build with `--features app`); `speccoder ui` opens the browser",
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
            open_in_browser(&s.url);
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

/// `config set` on a prefix list: entries it drops are removed at once (that
/// narrows); each one it adds is shown with what it would newly allow and
/// written only as the confirmation of that preview (`WidenPolicy`).
fn widen(
    path: &std::path::Path,
    list: speccoder_core::actions::PolicyList,
    value: &serde_json::Value,
    yes: bool,
    interactive: bool,
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandData, CommandResult, CommandStatus, ConfigChange};
    use std::io::Write as _;
    let wanted: Vec<Vec<String>> = serde_json::from_value(value.clone()).unwrap_or_default();
    let current: Vec<Vec<String>> = speccoder_runtime::settings::read(path)
        .sections
        .iter()
        .flat_map(|s| {
            s.keys
                .iter()
                .map(move |k| (format!("{}.{}", s.name, k.key), k))
        })
        .find(|(name, _)| name == list.key())
        .and_then(|(_, k)| k.value.clone())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let kept: Vec<&Vec<String>> = current.iter().filter(|p| wanted.contains(p)).collect();
    let mut last = speccoder_runtime::settings::show(path);
    if kept.len() != current.len() {
        last = speccoder_runtime::settings::apply(
            path,
            &[ConfigChange {
                key: list.key().to_owned(),
                value: Some(serde_json::json!(kept)),
                append: None,
            }],
        );
        if last.status() != CommandStatus::Ok {
            return last;
        }
    }
    let state = speccoder_runtime::worktree::state_dir();
    for prefix in wanted.iter().filter(|p| !current.contains(p)) {
        let shown =
            speccoder_runtime::approvals::coverage::show(path, state.as_deref(), list, prefix);
        let Some(CommandData::PolicyCoverage(c)) = shown.data().cloned() else {
            return shown;
        };
        let confirmed = yes
            || (interactive && {
                eprintln!("{}", speccoder_runtime::approvals::coverage::summary(&c));
                for row in &c.rows {
                    let how: Vec<String> = row
                        .decided
                        .iter()
                        .map(|d| format!("{} ×{}", d.decision, d.count))
                        .collect();
                    eprintln!(
                        "  {}  ×{}  {}{}",
                        row.argv.join(" "),
                        row.count,
                        how.join(", "),
                        row.still_asks
                            .as_deref()
                            .map(|s| format!("  ({s})"))
                            .unwrap_or_default()
                    );
                }
                eprint!("Save this prefix? Type yes to confirm: ");
                let _ = std::io::stderr().flush();
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                line.trim() == "yes"
            });
        if !confirmed {
            return CommandResult::new(
                "config",
                CommandStatus::NeedsHuman,
                &format!(
                    "needs confirmation: {} — run again with --yes to save it",
                    speccoder_runtime::approvals::coverage::summary(&c)
                ),
            )
            .with_data(CommandData::PolicyCoverage(c));
        }
        let run = speccoder_core::actions::Runnable::WidenPolicy {
            key: list,
            prefix: prefix.clone(),
            digest: c.digest.clone(),
        };
        last = speccoder_runtime::actions::perform(
            path,
            None,
            speccoder_runtime::actions::Dispatch::new(&run, speccoder_core::actions::Via::Terminal),
        );
        if last.status() != CommandStatus::Ok {
            return last;
        }
    }
    last
}

/// `speccoder approvals [batch …]`.
fn approvals(
    path: &std::path::Path,
    feature: Option<&str>,
    action: Option<&cli::ApprovalsAction>,
) -> speccoder_core::CommandResult {
    use speccoder_core::actions::{BatchVerdict, Runnable, Via};
    match action {
        None => speccoder_runtime::approvals::decide(
            path,
            feature,
            speccoder_runtime::approvals::Verb::List,
            Via::Terminal,
        ),
        Some(cli::ApprovalsAction::Batch {
            run,
            verdict,
            digest,
            calls,
            reason,
            resume_asking,
        }) => {
            // Every listed call by default.
            let calls = if calls.is_empty() {
                let listed = speccoder_runtime::approvals::decide(
                    path,
                    feature,
                    speccoder_runtime::approvals::Verb::List,
                    Via::Terminal,
                );
                match listed.data() {
                    Some(speccoder_core::CommandData::Approvals(a)) => a
                        .batch
                        .iter()
                        .filter(|b| b.run == *run)
                        .flat_map(|b| b.groups.iter().flat_map(|g| g.calls.clone()))
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                calls.clone()
            };
            let r = Runnable::DecideBatch {
                run: run.clone(),
                verdict: match verdict {
                    cli::VerdictArg::Approve => BatchVerdict::Approve,
                    cli::VerdictArg::Reject => BatchVerdict::Reject,
                },
                calls,
                digest: digest.clone(),
                resume_asking: *resume_asking,
            };
            let mut d = speccoder_runtime::actions::Dispatch::new(&r, Via::Terminal);
            d.input = reason.as_deref();
            speccoder_runtime::actions::perform(path, feature, d)
        }
    }
}

/// `speccoder config [get|set|unset|commit|coverage]`.
fn config(
    path: &std::path::Path,
    action: Option<&cli::ConfigAction>,
    interactive: bool,
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
        Some(cli::ConfigAction::Set { key, value, yes }) => match settings::value_of(key, value) {
            Ok(v) => match speccoder_core::actions::PolicyList::of_key(key) {
                Some(list) => widen(path, list, &v, *yes, interactive),
                None => settings::apply(path, &change(key, Some(v))),
            },
            Err(e) => CommandResult::new("config", CommandStatus::ConfigError, &e)
                .with_code("CONFIG-INVALID")
                .with_fix(vec![format!("speccoder config get {key}")]),
        },
        Some(cli::ConfigAction::Coverage { key, prefix }) => {
            match speccoder_core::actions::PolicyList::of_key(key) {
                Some(list) => speccoder_runtime::approvals::coverage::show(
                    path,
                    speccoder_runtime::worktree::state_dir().as_deref(),
                    list,
                    prefix,
                ),
                None => CommandResult::new(
                    "config",
                    CommandStatus::UsageError,
                    &format!(
                        "`{key}` is not a prefix list (commands.allow_prefix, sandbox.network_allow_prefix)"
                    ),
                ),
            }
        }
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
        Command::Fix { dry_run: true } => {
            match speccoder_runtime::code::setup_remedy(path, feature) {
                Some(r) => speccoder_core::CommandResult::new(
                    "fix",
                    speccoder_core::CommandStatus::Ok,
                    &format!("{} (dry run, nothing changed): {}", r.label, r.command),
                )
                .with_remedy(Some(r)),
                None => speccoder_core::CommandResult::new(
                    "fix",
                    speccoder_core::CommandStatus::Ok,
                    "nothing in the setup stops a run",
                ),
            }
        }
        Command::Fix { dry_run: false } => speccoder_runtime::code::fix_setup(path, feature),
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
        Command::Config { action } => config(
            path,
            action.as_ref(),
            !cli.json
                && std::io::IsTerminal::is_terminal(&std::io::stdin())
                && std::io::IsTerminal::is_terminal(&std::io::stderr()),
        ),
        Command::Approvals { action } => approvals(path, feature, action.as_ref()),
        Command::Do {
            request,
            checks,
            no_run,
            no_prompt,
        } => {
            let approver: Option<std::sync::Arc<dyn speccoder_runtime::approvals::Approver>> =
                interactive(*no_prompt).then(|| std::sync::Arc::new(prompt::Terminal) as _);
            let (progress, attention) = live_progress(&cli, &notify, events.as_ref());
            let options = speccoder_runtime::RunOptions {
                approver,
                progress,
                ..speccoder_runtime::RunOptions::default()
            };
            let result = speccoder_runtime::quick::change(
                path,
                request,
                checks,
                !*no_run,
                speccoder_core::actions::Via::Terminal,
                &options,
            );
            if let Some(a) = &attention {
                a.finish(&result);
            }
            result
        }
        Command::Bug {
            action,
            symptom,
            expected,
            unchanged,
            issue,
            tests,
            candidate,
            slug,
            no_run,
            no_prompt,
        } => {
            // A run without a terminal (CI) is recorded as such.
            let via = if std::env::var_os("CI").is_some() && !interactive(false) {
                speccoder_core::actions::Via::Ci
            } else {
                speccoder_core::actions::Via::Terminal
            };
            let ask = |no_prompt: bool| interactive(no_prompt) && !cli.events;
            let (progress, attention) = live_progress(&cli, &notify, events.as_ref());
            let options = |no_prompt: bool| speccoder_runtime::RunOptions {
                approver: ask(no_prompt).then(|| std::sync::Arc::new(prompt::Terminal) as _),
                progress: progress.clone(),
                ..speccoder_runtime::RunOptions::default()
            };
            let result = match action {
                Some(cli::BugAction::Confirm {
                    task,
                    digest,
                    reject,
                    reason,
                    no_run,
                }) => bug_confirm(
                    path,
                    task,
                    digest.as_deref(),
                    reject.then_some(reason.as_deref().unwrap_or_default()),
                    !*no_run,
                    via,
                    &options(false),
                    ask(false),
                ),
                Some(cli::BugAction::Keep { task, reason }) => {
                    speccoder_runtime::bug::keep(path, task, reason, None)
                }
                Some(cli::BugAction::Run {
                    slug,
                    task,
                    no_prompt,
                }) => speccoder_runtime::bug::run(
                    path,
                    slug.as_deref(),
                    task.as_deref(),
                    None,
                    &options(*no_prompt),
                ),
                None => {
                    let req = speccoder_runtime::bug::Request {
                        symptom: symptom.clone().unwrap_or_default(),
                        expected: expected.clone(),
                        unchanged: unchanged.clone(),
                        issue: *issue,
                        tests: tests.clone(),
                        candidate: candidate.clone(),
                        slug: slug.clone(),
                        run: !*no_run,
                        github: None,
                    };
                    speccoder_runtime::bug::bug(path, &req, via, None, &options(*no_prompt))
                }
            };
            if let Some(a) = &attention {
                a.finish(&result);
            }
            result
        }
        Command::Inbox => speccoder_runtime::inbox::inbox(path, feature),
        Command::Actions { task } => {
            speccoder_runtime::actions::list(path, feature, task.as_deref())
        }
        Command::Auth { action } => auth(path, action.as_ref()),
        Command::Review { action } => {
            use cli::ReviewAction as R;
            use speccoder_runtime::review;
            let ask = interactive(false) && !cli.events;
            let (progress, attention) = live_progress(&cli, &notify, events.as_ref());
            let options = speccoder_runtime::RunOptions {
                approver: ask.then(|| std::sync::Arc::new(prompt::Terminal) as _),
                progress: progress.clone(),
                ..speccoder_runtime::RunOptions::default()
            };
            let perform = |run: &speccoder_core::actions::Runnable, input: Option<&str>| {
                let mut d = speccoder_runtime::actions::Dispatch::new(
                    run,
                    speccoder_core::actions::Via::Terminal,
                );
                d.input = input;
                d.options = options.clone();
                speccoder_runtime::actions::perform(path, feature, d)
            };
            let result = match action {
                None => review::report(path, feature),
                Some(R::Approve {
                    run: Some(mode),
                    grant,
                    list_grants,
                    ..
                }) => {
                    let mode = match mode {
                        cli::RunModeArg::Ask => speccoder_core::oracle::RunMode::Ask,
                        cli::RunModeArg::Autopilot => speccoder_core::oracle::RunMode::Autopilot,
                    };
                    if *list_grants {
                        review_grants(path, feature)
                    } else {
                        perform(
                            &speccoder_core::actions::Runnable::ApproveAndRun {
                                mode,
                                grants: grant.clone(),
                                sheet: None,
                            },
                            None,
                        )
                    }
                }
                Some(R::Tests {
                    req,
                    write,
                    reject,
                    reason,
                    again,
                    comment,
                    edit,
                    from,
                    reading,
                }) => 'tests: {
                    use speccoder_core::oracle::{OracleOp, Reading};
                    let content = match from {
                        Some(f) => match std::fs::read_to_string(f) {
                            Ok(t) => Some(t),
                            Err(e) => {
                                break 'tests speccoder_core::CommandResult::new(
                                    "review",
                                    speccoder_core::CommandStatus::UsageError,
                                    &format!("{}: {e}", f.display()),
                                )
                                .with_code("USAGE");
                            }
                        },
                        None => None,
                    };
                    let (op, test, text, scenario, which) = if *write {
                        (OracleOp::Write, None, None, None, None)
                    } else if let Some(p) = reject {
                        (
                            OracleOp::Reject,
                            Some(p.clone()),
                            reason.clone(),
                            None,
                            None,
                        )
                    } else if let Some(p) = again {
                        (
                            OracleOp::Again,
                            Some(p.clone()),
                            comment.clone(),
                            None,
                            None,
                        )
                    } else if let Some(p) = edit {
                        (OracleOp::Edit, Some(p.clone()), content, None, None)
                    } else if let Some(r) = reading {
                        let which = match r.first().map(String::as_str) {
                            Some("A" | "a") => Reading::A,
                            Some("B" | "b") => Reading::B,
                            _ => {
                                break 'tests speccoder_core::CommandResult::new(
                                    "review",
                                    speccoder_core::CommandStatus::UsageError,
                                    "--reading takes A or B, then the scenario",
                                )
                                .with_code("USAGE");
                            }
                        };
                        (
                            OracleOp::Reading,
                            None,
                            None,
                            r.get(1).cloned(),
                            Some(which),
                        )
                    } else {
                        break 'tests review::report(path, feature);
                    };
                    perform(
                        &speccoder_core::actions::Runnable::OracleTests {
                            req: req.clone(),
                            op,
                            path: test,
                            scenario,
                            reading: which,
                        },
                        text.as_deref(),
                    )
                }
                Some(R::Comments { action }) => {
                    review_comments(path, feature, action.as_ref(), &perform)
                }
                Some(R::Approve { ids, comment, .. }) => review::decide(
                    path,
                    feature,
                    ids,
                    "approve",
                    comment.as_deref(),
                    speccoder_core::actions::Via::Terminal,
                ),
                Some(R::Request { ids, comment }) => review::decide(
                    path,
                    feature,
                    ids,
                    "request",
                    Some(comment),
                    speccoder_core::actions::Via::Terminal,
                ),
                Some(R::Clear { ids }) => review::decide(
                    path,
                    feature,
                    ids,
                    "clear",
                    None,
                    speccoder_core::actions::Via::Terminal,
                ),
                Some(R::Answer { question, answer }) => {
                    review::answer(path, feature, question, answer)
                }
                Some(R::Edit {
                    id,
                    wording,
                    approve,
                }) => review::edit(
                    path,
                    feature,
                    id,
                    wording,
                    *approve,
                    speccoder_core::actions::Via::Terminal,
                ),
            };
            if let Some(a) = &attention {
                a.finish(&result);
            }
            result
        }
        Command::Issue {
            action: None | Some(cli::IssueAction::List),
        } => speccoder_runtime::github::list(&speccoder_runtime::github::Ctx::from_env(), path),
        Command::Issue {
            action: Some(cli::IssueAction::Comment { issue, assign }),
        } => speccoder_runtime::github::comment(
            &speccoder_runtime::github::Ctx::from_env(),
            path,
            feature,
            issue,
            *assign,
        ),
        Command::Issue {
            action:
                Some(cli::IssueAction::Start {
                    issue,
                    no_prompt,
                    comment,
                    no_comment,
                    assign,
                }),
        } => {
            match speccoder_runtime::github::issue(
                &speccoder_runtime::github::Ctx::from_env(),
                path,
                issue,
            ) {
                Err(e) => speccoder_runtime::github::refused("issue", &e),
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
                            let made = speccoder_runtime::hosting::run(
                                path,
                                feature,
                                &name,
                                &speccoder_runtime::github::description(&found),
                                &options,
                            );
                            // The feature exists: tell the issue, when the
                            // person wants that (it leaves the machine).
                            let say = made.status() == speccoder_core::CommandStatus::Ok
                                && !*no_comment
                                && (*comment
                                    || (interactive(*no_prompt)
                                        && yes(&format!(
                                            "Comment on #{} that it is being specified{}? [Y/n] ",
                                            found.number,
                                            if *assign {
                                                ", and assign it to you"
                                            } else {
                                                ""
                                            }
                                        ))));
                            if !say {
                                made
                            } else {
                                let said = speccoder_runtime::github::comment(
                                    &speccoder_runtime::github::Ctx::from_env(),
                                    path,
                                    None,
                                    &found.number.to_string(),
                                    *assign,
                                );
                                let status = made.status();
                                let message = format!("{}; {}", made.message(), said.message());
                                made.reworded(status, &message)
                            }
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
            action:
                cli::WorkflowAction::Install {
                    force,
                    overlay: None,
                },
        } => speccoder_runtime::workflow::install(path, *force),
        Command::Workflow {
            action:
                cli::WorkflowAction::Install {
                    force,
                    overlay: Some(name),
                },
        } => speccoder_runtime::workflow::install_overlay(path, name, *force),
        Command::Workflow {
            action: cli::WorkflowAction::Uninstall { overlay },
        } => speccoder_runtime::workflow::uninstall_overlay(path, overlay),
        Command::Attest {
            action:
                cli::AttestAction::Verify {
                    rev,
                    allowed_signers,
                    certificate_identity,
                    certificate_oidc_issuer,
                    require_signed,
                    branch: true,
                    base,
                    allowed_signers_rev,
                },
        } => attest_branch(
            path,
            rev.as_deref(),
            base.as_deref(),
            allowed_signers.as_deref(),
            allowed_signers_rev.as_deref(),
            certificate_identity.as_deref(),
            certificate_oidc_issuer.as_deref(),
            *require_signed,
        ),
        Command::Attest {
            action:
                cli::AttestAction::Verify {
                    rev,
                    allowed_signers,
                    certificate_identity,
                    certificate_oidc_issuer,
                    require_signed,
                    ..
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
        // A screen reader gets the report, not a redrawn screen.
        // Not at a terminal, `--plain` or a screen reader: the report.
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
            let github = speccoder_runtime::github::Ctx::from_env();
            let ask = |o: &speccoder_runtime::handoff::PrOptions<'_>| {
                speccoder_runtime::handoff::pr(path, feature, o)
            };
            let mut options = speccoder_runtime::handoff::PrOptions {
                remote,
                base: base.as_deref(),
                open: *yes,
                force: *force,
                github: &github,
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
        } => speccoder_runtime::approvals::list(path, feature),
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
    // A fix speccoder knows is offered, at a terminal, then the command
    // runs again; never on its own, and never for a script.
    if let Some(remedy) = result.remedy()
        && offers_remedy(cli)
        && follow::remedy(path, feature, remedy)
    {
        let again = std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe)
                .args(std::env::args_os().skip(1))
                .status()
        });
        std::process::exit(again.ok().and_then(|s| s.code()).unwrap_or(1));
    }
    // At a terminal, the next step is offered rather than only printed.
    let code = if result.status() == speccoder_core::CommandStatus::Ok && offers_next(cli) {
        follow::offer(path, feature, verbosity(cli), code)
    } else {
        code
    };
    std::process::exit(i32::from(code.code()));
}

/// Whether a remedy is offered after this command: a run or a quick change
/// that may ask, at a terminal, for a person.
fn offers_remedy(cli: &Cli) -> bool {
    use std::io::IsTerminal;
    if cli.json || cli.events || !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal()
    {
        return false;
    }
    matches!(
        &cli.command,
        Some(
            Command::Run {
                no_prompt: false,
                ..
            } | Command::Do {
                no_prompt: false,
                ..
            } | Command::Bug {
                no_prompt: false,
                ..
            }
        )
    )
}

/// `speccoder review approve --run autopilot --list-grants`: the grants an
/// autopilot run may hold, each in full, by its grant id.
fn review_grants(path: &std::path::Path, feature: Option<&str>) -> speccoder_core::CommandResult {
    let r = speccoder_runtime::review::report(path, feature);
    let Some(speccoder_core::CommandData::Review(rv)) = r.data() else {
        return r;
    };
    let grants = rv
        .sheet
        .as_ref()
        .map(|s| s.grants.clone())
        .unwrap_or_default();
    let mut lines: Vec<String> = grants
        .iter()
        .map(|g| format!("{}  {}  {}", g.id, g.class, g.object))
        .collect();
    if lines.is_empty() {
        lines
            .push("no grants are offered for this feature: an autopilot run holds none".to_owned());
    }
    if let Some(why) = rv.sheet.as_ref().and_then(|s| s.autopilot_blocked.clone()) {
        lines.push(why);
    }
    let message = lines.join("\n");
    r.reworded(speccoder_core::CommandStatus::Ok, &message)
}

/// `speccoder review comments [add|delete|keep|send|undo]`.
fn review_comments(
    path: &std::path::Path,
    feature: Option<&str>,
    action: Option<&cli::CommentsAction>,
    perform: &dyn Fn(
        &speccoder_core::actions::Runnable,
        Option<&str>,
    ) -> speccoder_core::CommandResult,
) -> speccoder_core::CommandResult {
    use speccoder_core::actions::{CommentOp, Runnable};
    use speccoder_core::oracle::CommentAnchor;
    let usage = |m: &str| {
        speccoder_core::CommandResult::new("review", speccoder_core::CommandStatus::UsageError, m)
            .with_code("USAGE")
    };
    let keep = |op, id: &str| Runnable::Comment {
        op,
        anchor: None,
        id: Some(id.to_owned()),
    };
    match action {
        None => speccoder_runtime::comments::list(path, feature, None),
        Some(cli::CommentsAction::Delete { id }) => perform(&keep(CommentOp::Delete, id), None),
        Some(cli::CommentsAction::Keep { id }) => perform(&keep(CommentOp::Keep, id), None),
        Some(cli::CommentsAction::Send) => {
            perform(&Runnable::CommentsSend { ids: Vec::new() }, None)
        }
        Some(cli::CommentsAction::Undo) => perform(&Runnable::RevisionUndo { turn: None }, None),
        Some(cli::CommentsAction::Add {
            path: doc,
            lines,
            test,
            finding,
            text,
        }) => {
            let anchor = if let (Some(doc), Some(lines)) = (doc, lines) {
                let (a, b) = lines.split_once('-').unwrap_or((lines, lines));
                let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) else {
                    return usage("--lines is `40` or `40-42`");
                };
                // The quote is taken from the file, as it reads now.
                let root = speccoder_runtime::format::at(path)
                    .detect(path)
                    .unwrap_or_else(|| path.to_path_buf());
                let quote = std::fs::read_to_string(root.join(doc))
                    .unwrap_or_default()
                    .lines()
                    .skip(a.saturating_sub(1))
                    .take(b.saturating_sub(a) + 1)
                    .collect::<Vec<_>>()
                    .join("\n");
                CommentAnchor::Passage {
                    path: doc.clone(),
                    start_line: a,
                    end_line: b,
                    quote,
                    quote_sha256: String::new(),
                    section: String::new(),
                }
            } else if let Some(t) = test {
                let mut parts = t.splitn(3, ':');
                let (Some(req), Some(file), Some(line)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    return usage("--test is `US1:tests/a.rs:3`");
                };
                let Ok(line) = line.parse::<usize>() else {
                    return usage("--test is `US1:tests/a.rs:3`");
                };
                let shown = speccoder_runtime::review::report(path, feature);
                let quote = match shown.data() {
                    Some(speccoder_core::CommandData::Review(r)) => r
                        .oracle
                        .iter()
                        .find(|v| v.req == req)
                        .and_then(|v| v.tests.iter().find(|x| x.path == file))
                        .and_then(|x| x.content.lines().nth(line.saturating_sub(1)))
                        .unwrap_or_default()
                        .to_owned(),
                    _ => String::new(),
                };
                CommentAnchor::TestLine {
                    req: req.to_owned(),
                    path: file.to_owned(),
                    line,
                    quote,
                }
            } else if let Some(f) = finding {
                let Some((source, id)) = f.split_once(':') else {
                    return usage("--finding is `analyze:A1` or `review:FR-001`");
                };
                CommentAnchor::Finding {
                    source: source.to_owned(),
                    id: id.to_owned(),
                    text: text.clone(),
                }
            } else {
                return usage(
                    "say what the comment is about: --path P --lines 40-42, --test US1:PATH:LINE or --finding SRC:ID",
                );
            };
            perform(
                &Runnable::Comment {
                    op: CommentOp::Add,
                    anchor: Some(anchor),
                    id: None,
                },
                Some(text),
            )
        }
    }
}

/// `speccoder bug confirm`: with the digest given, the decision; without
/// it, at a terminal, the reproductions are shown and the person answers.
#[allow(clippy::too_many_arguments)]
fn bug_confirm(
    path: &std::path::Path,
    task: &str,
    digest: Option<&str>,
    reject: Option<&str>,
    run: bool,
    via: speccoder_core::actions::Via,
    options: &speccoder_runtime::RunOptions,
    ask: bool,
) -> speccoder_core::CommandResult {
    use speccoder_runtime::approvals::{Approver, ConfirmAnswer};
    if let Some(d) = digest {
        return speccoder_runtime::bug::confirm(path, task, d, reject, run, via, None, options);
    }
    let shown = speccoder_runtime::worktree::state_dir().and_then(|state| {
        let root = speccoder_runtime::format::at(path).detect(path)?;
        speccoder_runtime::bug::shown(&state, &root, task)
    });
    let (Some(request), true) = (shown, ask) else {
        return speccoder_core::CommandResult::new(
            "bug",
            speccoder_core::CommandStatus::UsageError,
            &format!("pass --digest: the one shown with the reproductions (`speccoder inbox`, or `speccoder bug confirm {task}` at a terminal)"),
        )
        .with_code("USAGE");
    };
    match prompt::Terminal.confirm(&request) {
        Some(ConfirmAnswer::Confirm) => speccoder_runtime::bug::confirm(
            path,
            task,
            &request.digest,
            None,
            run,
            via,
            None,
            options,
        ),
        Some(ConfirmAnswer::Reject(why)) => speccoder_runtime::bug::confirm(
            path,
            task,
            &request.digest,
            Some(&why),
            run,
            via,
            None,
            options,
        ),
        None => speccoder_core::CommandResult::new(
            "bug",
            speccoder_core::CommandStatus::NeedsHuman,
            &format!("{task}'s reproductions still wait for you"),
        ),
    }
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
    // The command the line names, when it is a known subcommand.
    let known: Vec<String> = help::command()
        .get_subcommands()
        .map(|c| c.get_name().to_owned())
        .collect();
    let named = args.iter().skip(1).find(|a| known.iter().any(|k| k == *a));
    let command = named.map_or("speccoder", String::as_str);
    let conflict = e.kind() == ErrorKind::ArgumentConflict && json && stream;
    let message = e
        .to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches("error: ")
        .to_owned();
    // A path or a task id where none is taken: say where it goes.
    let stray = (e.kind() == ErrorKind::UnknownArgument)
        .then(|| {
            args.iter()
                .skip(1)
                .find(|a| message.contains(&format!("'{a}'")))
                .cloned()
        })
        .flatten();
    let (message, fix): (String, Vec<String>) = match &stray {
        Some(a) if std::path::Path::new(a).is_dir() => (
            format!("{message}; a project path is given with -C"),
            vec![format!("speccoder -C {a} {command}")],
        ),
        Some(a) if a.starts_with('T') && a[1..].chars().all(|c| c.is_ascii_digit()) => (
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
    .with_fix(if command == "speccoder" {
        vec!["speccoder help".to_owned()]
    } else {
        fix
    });
    let result = speccoder_runtime::next::attach(result, None);
    if !(json || stream) && stray.is_none() {
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
/// untouched code, and at a terminal offer to commit it. Without a
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
            speccoder_core::words::listed(&prepared.to_commit),
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
            C::Fix { .. } => "fix",
            C::Spec { .. } => "spec",
            C::Feature { .. } => "feature",
            C::Ui { .. } => "ui",
            C::Config { .. } => "config",
            C::Approvals { .. } => "approvals",
            C::Issue { .. } => "issue",
            C::Do { .. } => "do",
            C::Bug { .. } => "bug",
            C::Inbox => "inbox",
            C::Actions { .. } => "actions",
            C::Auth { .. } => "auth",
            C::Review { .. } => "review",
            C::Run { .. } => "run",
            C::Task { action } => match action {
                cli::TaskAction::Approve { .. } => "task approve",
                cli::TaskAction::Reject { .. } => "task reject",
                cli::TaskAction::Accept { .. } => "task accept",
                cli::TaskAction::Skip { .. } => "task skip",
                cli::TaskAction::Nudge { .. } => "task nudge",
                cli::TaskAction::Rule { .. } => "task rule",
                cli::TaskAction::Restore { .. } => "task restore",
                cli::TaskAction::Drop { .. } => "task drop",
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
                digest: None,
            }),
            speccoder_core::actions::Via::Terminal,
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
                digest: None,
            }),
            speccoder_core::actions::Via::Terminal,
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
        T::Restore { task } => speccoder_runtime::controls::restore(path, feature, task),
        T::Drop { task } => speccoder_runtime::controls::drop_set_aside(path, feature, task),
        T::Show { task, account } => {
            let shown = speccoder_runtime::status::show(path, feature, task);
            if !*account {
                return shown;
            }
            // The attempt the tick came from, else the last one tried.
            let run = match shown.data() {
                Some(speccoder_core::CommandData::Report(r)) => r.tasks.first().and_then(|t| {
                    t.run_id
                        .clone()
                        .or_else(|| t.last_attempt.as_ref().and_then(|a| a.run_id.clone()))
                }),
                _ => None,
            };
            let account = match run
                .as_deref()
                .map(|r| speccoder_runtime::evidence::account_of(path, r))
            {
                Some(Ok(turns)) if !turns.is_empty() => turns
                    .iter()
                    .map(|t| {
                        let block = |label: &str, text: &str| {
                            format!("    {label:<9} {}", text.replace('\n', "\n              "))
                        };
                        let mut lines = vec![format!("  turn {}", t.turn)];
                        lines.extend(t.reasoning.as_deref().map(|r| block("thinking", r)));
                        lines.extend(t.said.as_deref().map(|w| block("says", &format!("“{w}”"))));
                        lines.join("\n")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Some(Ok(_)) => {
                    "  (it neither reasoned aloud nor said anything beside its calls)".to_owned()
                }
                Some(Err(e)) => format!("  (the journal cannot be read: {e})"),
                None => "  (no attempt has run)".to_owned(),
            };
            let status = shown.status();
            let message = format!(
                "{}\nThe model's account — its reasoning and its words; a claim, not evidence:\n{account}",
                shown.message()
            );
            shown.reworded(status, &message)
        }
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

/// Opens `url` in the default browser; quietly does nothing without one.
pub(crate) fn open_in_browser(url: &str) {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        std::process::Command::new("explorer")
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = cmd
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
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

/// The looks of stdout and stderr: `--color`, then `[ui] color`,
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

/// `speccoder auth`: sign in to GitHub (a token from stdin, or the device
/// flow through speccoder's GitHub App), see who speccoder is, sign out.
fn auth(path: &std::path::Path, action: Option<&cli::AuthAction>) -> speccoder_core::CommandResult {
    use speccoder_runtime::github;
    let ctx = github::Ctx::from_env();
    let host_of = |h: Option<&String>| {
        h.cloned().unwrap_or_else(|| {
            github::repo(path, None).map_or_else(|_| "github.com".to_owned(), |r| r.host)
        })
    };
    match action {
        None | Some(cli::AuthAction::Status { .. }) => {
            let host = match action {
                Some(cli::AuthAction::Status { host }) => host.as_deref(),
                _ => None,
            };
            github::auth_status(&ctx, path, host)
        }
        Some(cli::AuthAction::Logout { host }) => github::logout(&host_of(host.as_ref())),
        Some(cli::AuthAction::Login { host, with_token }) => {
            let host = host_of(host.as_ref());
            if *with_token {
                let mut token = String::new();
                if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut token) {
                    return speccoder_core::CommandResult::new(
                        "auth",
                        speccoder_core::CommandStatus::UsageError,
                        &format!("stdin: {e}"),
                    );
                }
                return github::login_with_token(&ctx, &host, &token);
            }
            let Some(client) = github::auth::client_id(&host) else {
                return speccoder_core::CommandResult::new(
                    "auth",
                    speccoder_core::CommandStatus::UsageError,
                    &format!(
                        "{host} has no speccoder app: set SPECCODER_GITHUB_CLIENT_ID to your server's own GitHub App (with the device flow on), or sign in with a token — create one at https://{host}/settings/personal-access-tokens/new (Issues and Pull requests: read and write; Contents, Metadata, Checks, Commit statuses: read), then `speccoder auth login --with-token < token.txt`. In CI, set GH_ENTERPRISE_TOKEN."
                    ),
                )
                .with_code("GITHUB-AUTH");
            };
            let oauth = github::http::oauth(&host);
            let pending = match github::auth::start(&oauth, &client) {
                Ok(p) => p,
                Err(e) => return github::refused("auth", &e),
            };
            // The code, big enough to read, already on the clipboard; the
            // page already open.
            let copied = copy(&pending.user_code);
            eprintln!(
                "\n  Your code:  {}{}\n  Enter it at {} — opened in your browser.\n  Waiting for GitHub… (ctrl-c cancels)\n",
                pending.user_code,
                if copied { "   (copied)" } else { "" },
                pending.verification_uri
            );
            open_in_browser(&pending.verification_uri);
            match github::auth::wait(&oauth, &client, &pending) {
                Ok(token) => {
                    let signed = github::login_with_token(&ctx, &host, token.expose());
                    // Through the app: is it installed where this repository is?
                    match (signed.status(), github::reach(&ctx, path)) {
                        (speccoder_core::CommandStatus::Ok, Some(todo)) => {
                            let message = format!("{}; {todo}", signed.message());
                            signed.reworded(speccoder_core::CommandStatus::Ok, &message)
                        }
                        _ => signed,
                    }
                }
                Err(e) => github::refused("auth", &e),
            }
        }
    }
}

/// Asks a yes/no question at the terminal; Return is yes.
fn yes(question: &str) -> bool {
    use std::io::Write;
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes")
}

/// Puts `text` on the clipboard, where the system has one; whether it did.
fn copy(text: &str) -> bool {
    use std::io::Write;
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    tools.iter().any(|(tool, args)| {
        std::process::Command::new(tool)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()
            .and_then(|mut c| {
                c.stdin.take()?.write_all(text.as_bytes()).ok()?;
                c.wait().ok()
            })
            .is_some_and(|s| s.success())
    })
}
