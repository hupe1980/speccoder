//! `speccoder new`: a new project from one line of intent. The plan is
//! printed with every command and whether it reaches the network, asked
//! about once (or `--yes`), then run exactly as shown; `--undo` takes back
//! a project speccoder created.

use std::io::Write;
use std::path::{Path, PathBuf};

use speccoder_core::new_project::{
    NewChoices, NewProjectReport, PlanStepState, ProjectMode, StepAccess,
};
use speccoder_core::{CommandResult, CommandStatus};

use crate::style::{Glyph, Token};

/// What `speccoder new` was given.
pub struct Args<'a> {
    /// The folder.
    pub path: Option<&'a Path>,
    /// `-C`.
    pub project: &'a Path,
    /// What is being built.
    pub idea: Option<&'a str>,
    /// The stack.
    pub stack: Option<speccoder_core::new_project::Stack>,
    /// The spec format (`--format`).
    pub format: Option<&'a str>,
    /// Spec Kit extensions.
    pub extensions: &'a [String],
    /// Leave out the speccoder preset.
    pub no_preset: bool,
    /// Print the plan only.
    pub dry_run: bool,
    /// Run without asking.
    pub yes: bool,
    /// Undo instead.
    pub undo: bool,
    /// Someone is at a terminal to ask.
    pub interactive: bool,
}

fn no_state() -> CommandResult {
    CommandResult::new(
        "new",
        CommandStatus::ConfigError,
        "no state directory (set SPECCODER_STATE_DIR or HOME)",
    )
    .with_code("NO-STATE-DIR")
}

/// One question, answered `y` or not: no means no.
fn confirm(question: &str) -> bool {
    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The command as run, with `--yes` added: the stop's fix.
fn again_with_yes() -> String {
    let words: Vec<String> = std::env::args().skip(1).collect();
    format!("speccoder {} --yes", shell(&words))
}

/// The folder `new` works on.
fn target(a: &Args<'_>) -> PathBuf {
    let base = if a.project.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        a.project.to_path_buf()
    };
    match (a.path, a.idea) {
        (Some(p), _) if p.is_absolute() => p.to_path_buf(),
        (Some(p), _) => base.join(p),
        (None, Some(idea)) if !idea.trim().is_empty() && !a.undo => {
            base.join(speccoder_core::new_project::slug(idea))
        }
        _ => base,
    }
}

/// `speccoder new …`.
#[must_use]
pub fn run(
    a: &Args<'_>,
    progress: Option<std::sync::Arc<dyn speccoder_runtime::progress::Progress>>,
) -> CommandResult {
    let Some(state) = speccoder_runtime::worktree::state_dir() else {
        return no_state();
    };
    let target = target(a);
    let options = speccoder_runtime::RunOptions {
        progress,
        ..speccoder_runtime::RunOptions::default()
    };
    if a.undo {
        return undo(a, &target, &state, &options);
    }
    let choices = NewChoices {
        idea: a.idea.unwrap_or_default().to_owned(),
        stack: a.stack,
        format: a.format.map(str::to_owned),
        extensions: a.extensions.to_vec(),
        no_preset: a.no_preset,
    };
    if a.dry_run {
        return speccoder_runtime::new_project::dry_run(&target, &choices, &state);
    }
    let plan = speccoder_runtime::new_project::plan(&target, &choices, &state);
    if plan.refusal.is_some() {
        return speccoder_runtime::new_project::dry_run(&target, &choices, &state);
    }
    if !a.yes {
        let commands = plan.steps.len();
        let hosts = plan.network_hosts();
        let network = plan
            .steps
            .iter()
            .filter(|s| s.access == StepAccess::Network)
            .count();
        if !a.interactive {
            let preview = speccoder_runtime::new_project::dry_run(&target, &choices, &state);
            let data = preview.data().cloned();
            let mut stop = CommandResult::new(
                "new",
                CommandStatus::NeedsHuman,
                &format!(
                    "the plan needs your confirmation: {commands} commands in {}; nothing ran",
                    speccoder_runtime::welcome::tilde(Path::new(&plan.target))
                ),
            )
            .with_code("NEW-CONFIRM")
            .with_fix(vec![again_with_yes()]);
            if let Some(d) = data {
                stop = stop.with_data(d);
            }
            return stop;
        }
        let mut err = Vec::new();
        let _ = plan_text(&mut err, &plan, &[]);
        eprint!("{}", String::from_utf8_lossy(&err));
        let reach = if network == 0 {
            String::new()
        } else {
            format!(
                " ({network} reach{} the network: {})",
                if network == 1 { "es" } else { "" },
                hosts.join(", ")
            )
        };
        if !confirm(&format!(
            "Run these {commands} commands in {}?{reach}",
            speccoder_runtime::welcome::tilde(Path::new(&plan.target))
        )) {
            return CommandResult::new("new", CommandStatus::Ok, "nothing ran");
        }
    }
    speccoder_runtime::new_project::perform(
        &target,
        &choices,
        &plan.digest,
        speccoder_core::actions::Via::Terminal,
        &state,
        &options,
    )
}

fn undo(
    a: &Args<'_>,
    target: &Path,
    state: &Path,
    options: &speccoder_runtime::RunOptions,
) -> CommandResult {
    let Some(c) = speccoder_runtime::new_project::creation(state, target) else {
        return CommandResult::new(
            "new",
            CommandStatus::GitPrecondition,
            &format!(
                "speccoder did not create {}; nothing was moved",
                speccoder_runtime::welcome::tilde(target)
            ),
        )
        .with_code("NEW-UNDO-REFUSED");
    };
    let commit = c.commit.clone().unwrap_or_default();
    let question = match c.mode {
        ProjectMode::NewFolder => format!(
            "Move {} to the Trash? It holds only what speccoder created.",
            speccoder_runtime::welcome::tilde(target)
        ),
        ProjectMode::ExistingRepository => format!(
            "Revert {} \"new project: {}\"?",
            commit.chars().take(7).collect::<String>(),
            c.name
        ),
    };
    if !a.yes {
        if !a.interactive {
            return CommandResult::new(
                "new",
                CommandStatus::NeedsHuman,
                &format!("{question} — nothing was moved"),
            )
            .with_code("NEW-CONFIRM")
            .with_fix(vec![again_with_yes()]);
        }
        if !confirm(&question) {
            return CommandResult::new("new", CommandStatus::Ok, "nothing was undone");
        }
    }
    speccoder_runtime::new_project::undo(
        target,
        &commit,
        speccoder_core::actions::Via::Terminal,
        state,
        options,
    )
}

/// `argv` as a shell would read it: words with spaces or quotes quoted.
#[must_use]
pub fn shell(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty() || a.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                speccoder_core::ladder::quoted(a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The plan as text, with each step's outcome when it ran.
pub fn plan_text(
    out: &mut dyn Write,
    plan: &speccoder_core::new_project::ProjectPlan,
    results: &[speccoder_core::new_project::PlanStepResult],
) -> std::io::Result<()> {
    let look = crate::style::err();
    let place = speccoder_runtime::welcome::tilde(Path::new(&plan.target));
    let mode = match plan.mode {
        ProjectMode::NewFolder => "new folder",
        ProjectMode::ExistingRepository => "existing repository",
    };
    writeln!(out, "New project {} in {place} ({mode})", plan.name)?;
    writeln!(
        out,
        "  stack   {} — {}",
        plan.stack.id.label(),
        plan.stack.reason
    )?;
    let format = plan
        .formats
        .iter()
        .find(|f| f.id == plan.format)
        .map_or_else(
            || plan.format.clone(),
            |f| {
                format!(
                    "{}{}",
                    f.label,
                    f.version
                        .as_deref()
                        .map(|v| format!(" {v}"))
                        .unwrap_or_default()
                )
            },
        );
    writeln!(
        out,
        "  specs   {format}{}",
        if plan.preset {
            " with the speccoder preset"
        } else {
            ""
        }
    )?;
    match &plan.sandbox.backend {
        Some(b) => writeln!(
            out,
            "  sandbox {b} (writes: this folder and speccoder's cache)"
        )?,
        None => writeln!(
            out,
            "  sandbox {}",
            look.paint(
                Token::Attention,
                "none — these commands run without an OS sandbox on this system"
            )
        )?,
    }
    writeln!(out)?;
    let width = plan
        .steps
        .iter()
        .map(|s| shell(&s.argv).chars().count())
        .max()
        .unwrap_or(0)
        .min(72);
    for (i, s) in plan.steps.iter().enumerate() {
        let mut command = shell(&s.argv);
        if s.id == "commit"
            && let Some(who) = &plan.identity
        {
            command = format!("{command}   as {} <{}>", who.name, who.email);
        }
        let badge = match s.access {
            StepAccess::Local => "local".to_owned(),
            StepAccess::Network => look.paint(
                Token::Attention,
                &format!("⇅ network: {}", s.hosts.join(", ")),
            ),
        };
        let mark = match results.get(i).map(|r| r.state) {
            Some(PlanStepState::Done) => format!(
                "{} ",
                look.paint(Token::Verified, look.mark(Glyph::Verified))
            ),
            Some(PlanStepState::Failed) => {
                format!("{} ", look.paint(Token::Danger, look.mark(Glyph::Failed)))
            }
            Some(PlanStepState::Skipped) => "= ".to_owned(),
            Some(PlanStepState::Running) => format!("{} ", look.mark(Glyph::Running)),
            Some(PlanStepState::Waiting) | None => "  ".to_owned(),
        };
        writeln!(out, "{mark}{:>2}  {command:<width$}  {badge}", i + 1)?;
        if let Some(n) = &s.note {
            writeln!(out, "        {}", look.paint(Token::Muted, n))?;
        }
        if let Some(r) = results.get(i).filter(|r| r.state == PlanStepState::Failed) {
            for l in &r.output_tail {
                writeln!(out, "        │ {l}")?;
            }
        }
    }
    writeln!(
        out,
        "plan {}",
        plan.digest
            .trim_start_matches("sha256:")
            .chars()
            .take(12)
            .collect::<String>()
    )
}

/// The human rendering of a `new_project` payload.
pub fn render(
    out: &mut dyn Write,
    err: &mut dyn Write,
    r: &NewProjectReport,
    result: &CommandResult,
) -> std::io::Result<()> {
    if r.dry_run || r.plan.refusal.is_some() || result.status() == CommandStatus::NeedsHuman {
        plan_text(out, &r.plan, &r.results)?;
        if let Some(why) = &r.plan.refusal {
            writeln!(err, "{}", why.message)?;
        }
        return Ok(());
    }
    // The steps were shown as they ran; a failed one again, with its output.
    if r.results.iter().any(|x| x.state == PlanStepState::Failed) {
        plan_text(err, &r.plan, &r.results)?;
    }
    if let Some(i) = &r.init {
        for run in &i.ran {
            let word = if run.status == "no_tests" {
                "no tests yet".to_owned()
            } else if run.passed {
                "passes".to_owned()
            } else {
                format!(
                    "fails before any change{}",
                    run.reason
                        .as_deref()
                        .map(|w| format!(": {w}"))
                        .unwrap_or_default()
                )
            };
            writeln!(err, "  check {:<10} {word}", run.name)?;
        }
        if i.ran.iter().any(|r| !r.passed && r.status != "no_tests") {
            writeln!(
                err,
                "  A check that fails before any change makes every task's baseline red: fix it first."
            )?;
        }
    }
    if result.status() == CommandStatus::Ok {
        writeln!(out, "ok: {}", result.message())?;
    }
    Ok(())
}
