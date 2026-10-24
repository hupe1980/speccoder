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
    err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        serde_json::to_writer(&mut *out, result)?;
        return writeln!(out);
    }
    human_body(result, out, err)?;
    let shown = code_block(result, err)?;
    let next: Vec<_> = result
        .next()
        .iter()
        .filter(|s| !shown.contains(&s.command))
        .cloned()
        .collect();
    human::next(&mut out, &next)
}

/// `error[CODE]: …` (or `stopped`, `warning`), why, `fix:` lines and
/// `docs:` — on stderr; the fixes shown, so `Next:` does not repeat them.
/// With a payload the report already said what happened, so the headline
/// is the code's title; without one it is the message.
fn code_block(result: &CommandResult, err: &mut dyn Write) -> std::io::Result<Vec<String>> {
    let Some(code) = result.code() else {
        return Ok(Vec::new());
    };
    let fix = result.fix();
    let prefix = code.kind.prefix();
    let reported = !matches!(result.data(), None | Some(CommandData::InPlace(_)));
    if reported || result.message().is_empty() {
        writeln!(err, "{prefix}[{}]: {}", code.name, code.title)?;
    } else {
        writeln!(err, "{prefix}[{}]: {}", code.name, result.message())?;
        writeln!(err, "  {}", code.title)?;
    }
    if let Some(CommandData::InPlace(p)) = result.data() {
        for r in &p.paths {
            writeln!(err, "  {}  ({})", r.path, r.reason)?;
        }
    }
    for f in &fix {
        writeln!(err, "  fix: {f}")?;
    }
    writeln!(err, "  docs: {}", code.docs())?;
    Ok(fix)
}

/// The human rendering of `result`, before its `Next:` block.
fn human_body(
    result: &CommandResult,
    mut out: &mut dyn Write,
    mut err: &mut dyn Write,
) -> std::io::Result<()> {
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
        (_, Some(CommandData::Archive(a))) => {
            writeln!(
                out,
                "Change     {} ({})",
                a.change,
                a.stage.replace('_', " ")
            )?;
            if !a.unverified_tasks.is_empty() {
                writeln!(
                    out,
                    "Unverified {} (ticked without a speccoder verdict)",
                    a.unverified_tasks.join(", ")
                )?;
            }
            if a.stage == "awaiting_confirmation"
                && let Some(diff) = &a.diff
            {
                writeln!(out, "{}", diff.trim_end())?;
            }
            if let Some(c) = &a.commit {
                writeln!(out, "Commit     {c}")?;
            }
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
        (_, Some(CommandData::Explain(e))) => {
            for (i, c) in e.codes.iter().enumerate() {
                if e.codes.len() > 1 {
                    writeln!(out, "  {:<22} {}", c.code, c.title)?;
                } else {
                    if i > 0 {
                        writeln!(out)?;
                    }
                    crate::explain::entry(out, c)?;
                }
            }
            Ok(())
        }
        (_, Some(CommandData::Schema(k))) => match &k.schema {
            // The schema is the content: pretty JSON, also in human mode.
            Some(schema) => writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(schema).unwrap_or_default()
            ),
            None => {
                for n in &k.names {
                    writeln!(out, "  {:<12} {}", n.name, n.describes)?;
                }
                writeln!(out, "Published at {}", crate::schema::BASE)
            }
        },
        (_, Some(CommandData::Cost(c))) => human::cost(&mut out, c, result.message()),
        (_, Some(CommandData::Attest(a))) => human::attest(&mut out, a, result.message()),
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
        (_, Some(CommandData::Worktree(w))) => human::worktree_refused(&mut err, w),
        (CommandStatus::SelectionNeeded, Some(CommandData::Inspect(s))) => {
            human::selection_needed(&mut err, s)
        }
        (CommandStatus::Ok, None) => writeln!(&mut *out, "{}", result.message()),
        // The code block says it (`error[CODE]: message`).
        (_, None | Some(CommandData::InPlace(_))) => Ok(()),
        _ => writeln!(&mut *err, "{}", result.message()),
    }
}

#[cfg(test)]
mod next_tests {
    use super::*;

    #[test]
    fn the_human_next_block_and_data_next_list_the_same_commands() {
        let result = speccoder_runtime::next::attach(
            CommandResult::new("approve", CommandStatus::Ok, "approved"),
            Some("001-x"),
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        render_into(&result, false, &mut out, &mut err).unwrap_or_else(|e| panic!("{e}"));
        let human = String::from_utf8_lossy(&out).into_owned();
        let mut json_out = Vec::new();
        render_into(&result, true, &mut json_out, &mut err).unwrap_or_else(|e| panic!("{e}"));
        let json: serde_json::Value =
            serde_json::from_slice(&json_out).unwrap_or_else(|e| panic!("{e}"));
        let commands: Vec<&str> = json["next"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["command"].as_str())
            .collect();
        assert!(!commands.is_empty());
        for c in commands {
            assert!(human.contains(c), "{c} missing from: {human}");
        }
        assert!(human.contains("Next:  "), "{human}");
    }
}
