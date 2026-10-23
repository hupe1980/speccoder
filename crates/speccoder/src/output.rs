//! Rendering of command results.

use std::io::Write;

use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::human;

/// Writes `result` as one JSON line to stdout, or as human text.
///
/// Human mode: a snapshot report goes to stdout (statuses ok and
/// diagnostics_error); other outcomes are explained on stderr.
pub fn render(result: &CommandResult, json: bool) -> std::io::Result<()> {
    render_into(
        result,
        json,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// [`render`] into `out` and `err` (the ACP agent writes into its messages).
pub fn render_into(
    result: &CommandResult,
    json: bool,
    mut out: &mut dyn Write,
    mut err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        serde_json::to_writer(&mut *out, result)?;
        return writeln!(out);
    }
    match (result.status(), result.data()) {
        (CommandStatus::Ok | CommandStatus::DiagnosticsError, Some(CommandData::Inspect(s))) => {
            human::inspect(&mut out, s)
        }
        (_, Some(CommandData::Eval(e))) => human::eval(&mut out, e, result.message()),
        (_, Some(CommandData::Feature(f))) => {
            for r in &f.attempts {
                human::run(&mut out, r, "")?;
                writeln!(out)?;
            }
            for c in &f.converge {
                human::converge(&mut out, c, "")?;
                writeln!(out)?;
            }
            for h in &f.hooks {
                writeln!(out, "Hook       {h}")?;
            }
            for p in &f.parallel {
                writeln!(out, "Parallel   {p}")?;
            }
            writeln!(out, "Feature    {}: {}", f.feature, f.stop)
        }
        (_, Some(CommandData::Skills(k))) => {
            for s in &k.skills {
                let note = s
                    .refused
                    .as_deref()
                    .map(|r| format!("  (not hosted: {r})"))
                    .unwrap_or_default();
                writeln!(out, "  {:<24} {}{note}", s.name, s.description)?;
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Skill(k))) => {
            writeln!(out, "Skill      {} ({})", k.skill, k.status)?;
            if let Some(s) = &k.summary {
                writeln!(out, "Summary    {s}")?;
            }
            if k.changes.is_empty() {
                writeln!(out, "Changes    none")?;
            } else {
                writeln!(out, "Changes    (in your checkout, uncommitted)")?;
                for (p, kind) in &k.changes {
                    writeln!(out, "  {kind:<9} {p}")?;
                }
            }
            for q in &k.questions {
                writeln!(out, "  ? {q}")?;
            }
            for h in &k.hooks {
                writeln!(out, "Hook       {h}")?;
            }
            writeln!(out, "Run        {}", k.run_id)?;
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Init(i))) => {
            for c in &i.checks {
                writeln!(
                    out,
                    "  {:<10} {:<5} {:<55} {}",
                    c.name,
                    c.tier,
                    c.argv.join(" "),
                    c.why
                )?;
            }
            writeln!(out, "Model      {}", i.model)?;
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Control(_))) => {
            writeln!(&mut *out, "{}", result.message())
        }
        (_, Some(CommandData::Bench(b))) => {
            for i in &b.imported {
                writeln!(out, "  + {i}")?;
            }
            for s in &b.skipped {
                writeln!(out, "  - {s}")?;
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Pr(p))) => {
            if p.opened {
                writeln!(
                    out,
                    "Pull request {}",
                    p.url.as_deref().unwrap_or("opened (draft)")
                )?;
            } else {
                writeln!(out, "{}\n\n{}", p.title, p.body)?;
                for c in &p.commands {
                    writeln!(out, "  $ {c}")?;
                }
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Cost(c))) => human::cost(&mut out, c, result.message()),
        (_, Some(CommandData::Approvals(a))) => human::approvals(&mut out, a, result.message()),
        (_, Some(CommandData::Report(r))) => human::status(&mut out, r, result.message()),
        (_, Some(CommandData::Converge(c))) => human::converge(&mut out, c, result.message()),
        (_, Some(CommandData::Run(r))) => human::run(&mut out, r, result.message()),
        (_, Some(CommandData::Verify(r))) => human::verify(&mut out, r),
        (CommandStatus::Ok, Some(CommandData::Propose(p))) => human::propose(&mut out, p),
        (CommandStatus::Ok, Some(CommandData::Replay(r))) => {
            writeln!(
                &mut *out,
                "Run {}: {} — {}",
                r.run_id,
                r.result,
                result.message()
            )
        }
        (CommandStatus::Ok, Some(CommandData::Worktree(w))) => human::worktree(&mut out, w),
        (_, Some(CommandData::Worktree(w))) => {
            human::worktree_refused(&mut err, result.message(), w)
        }
        (CommandStatus::SelectionNeeded, Some(CommandData::Inspect(s))) => {
            human::selection_needed(&mut err, s)
        }
        (CommandStatus::Ok, None) => writeln!(&mut *out, "{}", result.message()),
        _ => writeln!(
            &mut *err,
            "speccoder {}: {}",
            result.command(),
            result.message()
        ),
    }
}
