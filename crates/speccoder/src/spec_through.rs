//! `speccoder spec <skill> --through T`: plan through the format's own
//! skills, stopping where a person decides. At a terminal the review stop
//! asks once (`Approve … and plan? [y/N]`); `--yes` never answers it —
//! the approval is the person's, recorded by the review action.

use std::io::Write;
use std::path::Path;

use speccoder_core::actions::{FeatureSource, Runnable, Through, Via};
use speccoder_core::{ChainStop, CommandData, CommandResult, CommandStatus, PlanningChainReport};

/// Plan through from `name` (the creating skill with a description, or a
/// later skill of the chain to continue).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run(
    path: &Path,
    feature: Option<&str>,
    name: &str,
    arguments: &[String],
    through: Through,
    source: &FeatureSource,
    interactive: bool,
    options: &speccoder_runtime::RunOptions,
) -> CommandResult {
    let creates = match speccoder_runtime::planning_chain::takes_through(path, name) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult::new("spec", CommandStatus::UsageError, &e).with_code("USAGE");
        }
    };
    let first = if creates {
        let description = arguments.join(" ");
        if description.trim().is_empty() {
            return CommandResult::new(
                "spec",
                CommandStatus::UsageError,
                "a description is needed: speccoder spec specify \"what it should do\" --through tasks",
            )
            .with_code("USAGE");
        }
        speccoder_runtime::planning_chain::start(path, &description, source, through, options)
    } else {
        speccoder_runtime::planning_chain::resume(path, feature, through, options)
    };
    if !interactive {
        return first;
    }
    review_and_continue(path, first, through, options)
}

/// At a terminal: the review stop asked once; yes approves the
/// requirements shown through the review action, as the person, then
/// continues.
fn review_and_continue(
    path: &Path,
    result: CommandResult,
    through: Through,
    options: &speccoder_runtime::RunOptions,
) -> CommandResult {
    let Some(CommandData::PlanningChain(r)) = result.data() else {
        return result;
    };
    let ChainStop::Review { requirements } = &r.stop else {
        return result;
    };
    let Some(unit) = r.unit.clone() else {
        return result;
    };
    if requirements.is_empty() {
        return result;
    }
    // What is approved is the wording, so it is shown, as written now.
    let wording = |unit: &str| -> Vec<(String, String)> {
        speccoder_runtime::review::summary(path, Some(unit))
            .map(|r| {
                r.requirements
                    .iter()
                    .map(|i| (i.id.clone(), i.text.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let read = wording(&unit);
    let shown = match requirements.as_slice() {
        [one] => one.clone(),
        [a, .., z] => format!("{a} … {z}"),
        [] => String::new(),
    };
    eprintln!("{}", result.message());
    for line in approval_lines(&read) {
        eprintln!("{line}");
    }
    eprint!("Approve {shown} and plan? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return result;
    }
    // Changed while it was read: nothing is approved.
    if wording(&unit) != read {
        return CommandResult::new(
            "spec",
            CommandStatus::NeedsHuman,
            "the spec changed while you read it; nothing was approved — review it again",
        )
        .with_code("THROUGH-REVIEW");
    }
    let approve = Runnable::Review {
        ids: requirements.clone(),
        verdict: "approve".to_owned(),
    };
    let approved = speccoder_runtime::actions::perform(
        path,
        Some(&unit),
        speccoder_runtime::actions::Dispatch::new(&approve, Via::Terminal),
    );
    if approved.status() != CommandStatus::Ok {
        return approved;
    }
    speccoder_runtime::planning_chain::resume(path, Some(&unit), through, options)
}

/// The requirements a terminal approval is about, one line each, as
/// written now.
#[must_use]
pub fn approval_lines(requirements: &[(String, String)]) -> Vec<String> {
    requirements
        .iter()
        .map(|(id, text)| format!("  {id:<8} {text}"))
        .collect()
}

/// The human rendering of a `planning_chain` payload.
pub fn render(
    out: &mut dyn Write,
    r: &PlanningChainReport,
    result: &CommandResult,
) -> std::io::Result<()> {
    let look = crate::style::err();
    for m in &r.similar {
        writeln!(
            out,
            "{}: close to {} {} \"{}\" ({}) — shares: {}",
            look.paint(crate::style::Token::Regression, "warning[SIMILAR-FEATURE]"),
            m.unit,
            m.requirement,
            m.text,
            m.level.word(),
            m.shared.join(", ")
        )?;
    }
    for s in &r.ran {
        let mark = if s.status == CommandStatus::Ok {
            "✓"
        } else {
            "✗"
        };
        writeln!(out, "{mark} {}  {}", s.skill, s.message)?;
    }
    if let Some(rd) = &r.readiness {
        let mut parts = Vec::new();
        if rd.identifiers {
            parts.push(format!("{} requirements", rd.requirements));
        }
        parts.push(format!("{} scenarios", rd.scenarios));
        parts.push(format!(
            "{} open question{}",
            rd.open_questions,
            if rd.open_questions == 1 { "" } else { "s" }
        ));
        if !rd.untestable.is_empty() {
            parts.push(format!(
                "{} without a scenario ({} — can't be tested)",
                rd.untestable.len(),
                rd.untestable.join(", ")
            ));
        }
        writeln!(out, "  readiness  {}", parts.join(" · "))?;
    }
    if let ChainStop::Done { plan_checks } = &r.stop {
        for c in plan_checks {
            writeln!(out, "  warning[{}]: {}", c.code, c.detail)?;
        }
        writeln!(out, "ok: {}", result.message())?;
    }
    Ok(())
}

/// The human rendering of a `similar` payload.
pub fn render_similar(
    out: &mut dyn Write,
    r: &speccoder_core::similar::SimilarReport,
) -> std::io::Result<()> {
    if r.matches.is_empty() {
        writeln!(out, "no requirement is close")?;
    }
    for m in &r.matches {
        writeln!(
            out,
            "{} {} {} ({:.2}) \"{}\" — shares: {}",
            m.unit,
            m.requirement,
            m.level.word(),
            m.score,
            m.text,
            m.shared.join(", ")
        )?;
    }
    if r.limited {
        writeln!(
            out,
            "compared with the {} most recently changed features",
            speccoder_core::similar::MAX_UNITS
        )?;
    }
    Ok(())
}

/// The human rendering of a `rung_suggestion` payload.
pub fn render_suggestion(
    out: &mut dyn Write,
    s: &speccoder_core::ladder::RungSuggestion,
) -> std::io::Result<()> {
    use speccoder_core::ladder::RungPick;
    match s.rung {
        RungPick::None => writeln!(out, "{}", s.reason),
        RungPick::Ask => writeln!(out, "Just ask (not built yet) — {}", s.reason),
        r => writeln!(out, "{} — {}", r.label(), s.reason),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_approval_shows_the_wording_it_approves() {
        let lines = super::approval_lines(&[
            (
                "FR-001".to_owned(),
                "Users MUST export a report as CSV.".to_owned(),
            ),
            (
                "FR-002".to_owned(),
                "The export MUST keep the columns.".to_owned(),
            ),
        ]);
        assert_eq!(
            lines,
            [
                "  FR-001   Users MUST export a report as CSV.",
                "  FR-002   The export MUST keep the columns."
            ]
        );
    }
}
