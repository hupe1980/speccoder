//! `speccoder-verify`: checks every ticked task of a branch against its
//! signed attestation, with the caller's trust only. No model, no network,
//! nothing from the checked tree executed: it reads git objects and spawns
//! git and the signature tools the signatures name.
//!
//! The rules are `speccoder-runtime`'s (built without its `engine`), the
//! same code `speccoder attest verify --branch` runs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use speccoder_core::verify_branch::{Badge, BranchReport, TickOutcome};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::attest::branch::{self, AllowedSigners, Options};
use speccoder_runtime::attest::check;

/// The environment the verifier's children (git, the signature tools)
/// see: nothing else, so no token or configuration from the job reaches
/// them.
const KEPT_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "GNUPGHOME",
    "LANG",
    "TMPDIR",
    "GITHUB_STEP_SUMMARY",
];

/// Set in the re-executed process whose environment is already scrubbed.
const SCRUBBED: &str = "SPECCODER_VERIFY_SCRUBBED";

/// Checks every `[X]` of a branch against its signed attestation: no model
/// key, no network, nothing from the checked tree executed.
#[derive(Debug, Parser)]
#[command(
    name = "speccoder-verify",
    version,
    about,
    args_conflicts_with_subcommands = true,
    after_help = "Exit status: 0 every tick verifies, 1 a tick does not (or a refusal), 78 nothing to check (neutral)."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    check: CheckArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check every tick of the units the branch touches (the default).
    Check(CheckArgs),
    /// One task's evidence, step by step (payload `evidence_tour`).
    Tour(TourArgs),
    /// The share of ticks with verifying evidence, as a badge (every unit).
    Badge(BadgeArgs),
}

#[derive(Debug, Clone, Args)]
struct Where {
    /// The repository (any directory in it).
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The commit to check.
    #[arg(long, default_value = "HEAD")]
    head: String,
}

#[derive(Debug, Clone, Args)]
struct TrustArgs {
    /// An SSH allowed-signers file on this machine.
    #[arg(long, conflicts_with = "allowed_signers_rev")]
    allowed_signers: Option<PathBuf>,
    /// An allowed-signers file read from git at a revision,
    /// `<rev>:<path>` (in CI: the base branch's, never the head's).
    #[arg(long, value_name = "REV:PATH")]
    allowed_signers_rev: Option<String>,
    /// The Sigstore certificate identity to trust.
    #[arg(long, requires = "certificate_oidc_issuer")]
    certificate_identity: Option<String>,
    /// The Sigstore OIDC issuer to trust.
    #[arg(long, requires = "certificate_identity")]
    certificate_oidc_issuer: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct CheckArgs {
    #[command(flatten)]
    at: Where,
    /// The branch it is compared with: units touched since the merge base
    /// are checked. Without it, every unit.
    #[arg(long)]
    base: Option<String>,
    /// Every unit, whatever the base.
    #[arg(long)]
    all: bool,
    #[command(flatten)]
    trust: TrustArgs,
    /// Unsigned evidence is unverifiable (`false` accepts it, and says so).
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_name = "BOOL")]
    require_signed: bool,
    /// `regression_only` and `attested` ticks fail too.
    #[arg(long)]
    require_verified: bool,
    /// Print the result envelope (payload `branch_verify`).
    #[arg(long)]
    json: bool,
    /// Write the check as Markdown to this file.
    #[arg(long, value_name = "FILE")]
    markdown: Option<PathBuf>,
    /// Write the check-run request bodies (`{create, updates}`) to this file.
    #[arg(long, value_name = "FILE")]
    github_check: Option<PathBuf>,
    /// The check run's name.
    #[arg(long, default_value = check::NAME)]
    check_name: String,
    /// Publish a neutral outcome as `neutral` (GitHub counts it as passing);
    /// without it, as `failure`.
    #[arg(long)]
    allow_neutral: bool,
    /// Print workflow-command annotations and append the check to
    /// `$GITHUB_STEP_SUMMARY` (where the check cannot be published).
    #[arg(long)]
    github_annotations: bool,
}

#[derive(Debug, Clone, Args)]
struct TourArgs {
    #[command(flatten)]
    at: Where,
    /// The unit.
    #[arg(long)]
    feature: String,
    /// The task ID.
    #[arg(long)]
    task: String,
    /// With trust, a tour of evidence that does not verify stops at the
    /// requirement; without, it shows what is recorded.
    #[command(flatten)]
    trust: TrustArgs,
    /// Print the result envelope (payload `evidence_tour`).
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Args)]
struct BadgeArgs {
    #[command(flatten)]
    at: Where,
    #[command(flatten)]
    trust: TrustArgs,
    /// Unsigned evidence is unverifiable.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, value_name = "BOOL")]
    require_signed: bool,
    /// Write the badge as SVG.
    #[arg(long, value_name = "FILE")]
    svg: Option<PathBuf>,
    /// Write the shields.io endpoint JSON.
    #[arg(long, value_name = "FILE")]
    endpoint: Option<PathBuf>,
    /// Print the result envelope (payload `branch_verify`).
    #[arg(long)]
    json: bool,
}

fn options(at: &Where, trust: &TrustArgs) -> Result<Options, String> {
    let allowed_signers = match (&trust.allowed_signers, &trust.allowed_signers_rev) {
        (Some(f), _) => Some(AllowedSigners::File(f.clone())),
        (None, Some(spec)) => Some(AllowedSigners::parse_rev(spec)?),
        (None, None) => None,
    };
    Ok(Options {
        head: at.head.clone(),
        allowed_signers,
        identity: trust.certificate_identity.clone(),
        issuer: trust.certificate_oidc_issuer.clone(),
        ..Options::default()
    })
}

fn print(result: &CommandResult, json: bool) {
    let mut out = std::io::stdout().lock();
    if json {
        let _ = serde_json::to_writer(&mut out, result);
        let _ = writeln!(out);
    } else {
        let _ = writeln!(out, "{}", result.message());
    }
}

/// A refusal: on stderr (or as the envelope with `--json`), exit 1.
fn refuse(command: &str, status: CommandStatus, message: &str, json: bool) -> ExitCode {
    if json {
        print(&CommandResult::new(command, status, message), true);
    } else {
        eprintln!("speccoder-verify: {message}");
    }
    ExitCode::FAILURE
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A person's view of the report: the title, then one line per tick.
fn human(report: &BranchReport) -> String {
    let mut out = format!("{}\n", report.title());
    for r in &report.ticks {
        let what = match (r.outcome, r.verdict.as_deref(), r.reason) {
            (TickOutcome::Evidenced, Some(v), _) => v.to_owned(),
            (TickOutcome::Stale, ..) => "stale".to_owned(),
            (TickOutcome::Regressed, ..) => "regressed".to_owned(),
            (_, _, Some(reason)) => format!("unverifiable: {}", reason.word()),
            _ => "unverifiable".to_owned(),
        };
        out.push_str(&format!(
            "  {}/{}  {what}{}\n",
            r.tick.feature,
            r.tick.task,
            r.detail
                .as_deref()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default()
        ));
    }
    for n in &report.not_evaluated {
        out.push_str(&format!("  not evaluated: {n}\n"));
    }
    if report.accepted_unsigned {
        out.push_str("  unsigned ticks were accepted (--require-signed=false)\n");
    }
    out
}

fn run_check(args: &CheckArgs) -> ExitCode {
    const COMMAND: &str = "speccoder-verify check";
    let mut opts = match options(&args.at, &args.trust) {
        Ok(o) => o,
        Err(e) => return refuse(COMMAND, CommandStatus::UsageError, &e, args.json),
    };
    opts.base.clone_from(&args.base);
    opts.all = args.all;
    opts.require_signed = args.require_signed;
    opts.require_verified = args.require_verified;
    let checked = match branch::check(&args.at.repo, &opts) {
        Ok(c) => c,
        Err(r) => return refuse(COMMAND, r.status, &r.message, args.json),
    };
    // Tours only where they are shown: each reads its commit's tree.
    let tours = if args.markdown.is_some() || args.github_check.is_some() || args.github_annotations
    {
        speccoder_runtime::tour::markdown_all(&checked)
    } else {
        Vec::new()
    };
    let report = &checked.report;
    let mut failed_writes = Vec::new();
    if let Some(path) = &args.markdown
        && let Err(e) = write(path, &check::markdown(report, &tours))
    {
        failed_writes.push(e);
    }
    if let Some(path) = &args.github_check {
        let body = check::request(report, &tours, &args.check_name, args.allow_neutral);
        if let Err(e) = write(path, &format!("{body}\n")) {
            failed_writes.push(e);
        }
    }
    if args.github_annotations {
        for line in check::workflow_commands(report) {
            println!("{line}");
        }
        if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
            let appended = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&summary)
                .and_then(|mut f| writeln!(f, "{}", check::markdown(report, &tours)));
            if let Err(e) = appended {
                failed_writes.push(format!("the step summary was not written: {e}"));
            }
        }
    }
    let code = report.conclusion.exit_code();
    let result = branch::result(COMMAND, checked.report);
    if args.json {
        print(&result, true);
    } else if let Some(CommandData::BranchVerify(r)) = result.data() {
        print!("{}", human(r));
    }
    for e in &failed_writes {
        eprintln!("speccoder-verify: {e}");
    }
    if failed_writes.is_empty() {
        ExitCode::from(code)
    } else {
        ExitCode::FAILURE
    }
}

fn run_tour(args: &TourArgs) -> ExitCode {
    const COMMAND: &str = "speccoder-verify tour";
    let opts = match options(&args.at, &args.trust) {
        Ok(o) => o,
        Err(e) => return refuse(COMMAND, CommandStatus::UsageError, &e, args.json),
    };
    let result =
        speccoder_runtime::tour::command(&args.at.repo, &args.feature, &args.task, &opts, COMMAND);
    if args.json {
        print(&result, true);
    } else if let Some(CommandData::EvidenceTour(t)) = result.data() {
        let words = result
            .message()
            .split_once(": ")
            .map_or("", |(_, w)| w)
            .to_owned();
        print!("{}", t.markdown(&words));
    } else {
        eprintln!("speccoder-verify: {}", result.message());
    }
    if result.status() == CommandStatus::Ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_badge(args: &BadgeArgs) -> ExitCode {
    const COMMAND: &str = "speccoder-verify badge";
    let mut opts = match options(&args.at, &args.trust) {
        Ok(o) => o,
        Err(e) => return refuse(COMMAND, CommandStatus::UsageError, &e, args.json),
    };
    opts.all = true;
    opts.require_signed = args.require_signed;
    let checked = match branch::check(&args.at.repo, &opts) {
        Ok(c) => c,
        Err(r) => return refuse(COMMAND, r.status, &r.message, args.json),
    };
    let badge = Badge::of(&checked.report);
    let mut failed = Vec::new();
    if let Some(path) = &args.svg
        && let Err(e) = write(path, &badge.svg())
    {
        failed.push(e);
    }
    if let Some(path) = &args.endpoint {
        let json = serde_json::to_string_pretty(&badge).unwrap_or_default();
        if let Err(e) = write(path, &format!("{json}\n")) {
            failed.push(e);
        }
    }
    let result = branch::result(COMMAND, checked.report);
    if args.json {
        print(&result, true);
    } else {
        println!("evidence: {} ({})", badge.message, badge.color);
    }
    for e in &failed {
        eprintln!("speccoder-verify: {e}");
    }
    // The badge is a measurement: it succeeds whenever it was written.
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Runs this binary again with only [`KEPT_ENV`], unless done already;
/// the child's exit status, or `None` when re-running is not possible
/// (then the work runs here, in the environment as it is).
fn scrubbed() -> Option<ExitCode> {
    if std::env::var_os(SCRUBBED).is_some() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let status = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env_clear()
        .envs(
            KEPT_ENV
                .iter()
                .filter_map(|k| std::env::var_os(k).map(|v| (*k, v))),
        )
        .env(SCRUBBED, "1")
        .status()
        .ok()?;
    Some(ExitCode::from(
        status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(1),
    ))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(code) = scrubbed() {
        return code;
    }
    match &cli.command {
        None => run_check(&cli.check),
        Some(Command::Check(args)) => run_check(args),
        Some(Command::Tour(args)) => run_tour(args),
        Some(Command::Badge(args)) => run_badge(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_check_with_no_subcommand_takes_the_check_flags() {
        let cli = Cli::try_parse_from([
            "speccoder-verify",
            "--base",
            "main",
            "--allowed-signers-rev",
            "main:.github/speccoder/allowed_signers",
            "--require-signed=false",
        ])
        .unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.check.base.as_deref(), Some("main"));
        assert!(!cli.check.require_signed);
        let cli =
            Cli::try_parse_from(["speccoder-verify", "tour", "--feature", "f", "--task", "T1"])
                .unwrap();
        assert!(matches!(cli.command, Some(Command::Tour(_))));
    }
}
