//! The next step, taken: at a terminal, after a planning
//! step, a decision or the report, speccoder offers what comes next — commit
//! the planning files, the next planning skill, the run — one yes at a time.
//! Each offer shows where the feature is on its journey and the command the
//! step stands for. From the runtime's action list, so a later interface
//! offers the same steps.

use std::io::{BufRead, Write};
use std::path::Path;

use speccoder_core::actions::{Action, JourneyStep, Runnable, Via, primary};
use speccoder_core::{CommandResult, CommandStatus, ExitCode, Remedy};

/// At most this many steps in a row without a pause.
const STEPS: usize = 6;

/// Offers the next steps after `code`; returns the exit code of the last
/// step taken (or `code` when none was).
pub fn offer(
    path: &Path,
    feature: Option<&str>,
    verbosity: crate::progress::Verbosity,
    code: ExitCode,
) -> ExitCode {
    let mut code = code;
    let stdin = std::io::stdin();
    for _ in 0..STEPS {
        let actions = speccoder_runtime::actions::available(path, feature, None);
        let Some(next) = primary(&actions).filter(|a| a.input.is_none()).cloned() else {
            break;
        };
        // Decisions on waiting calls belong to their own prompt.
        if matches!(
            next.run,
            Some(Runnable::Approve { .. } | Runnable::Reject { .. })
        ) {
            break;
        }
        let steps = speccoder_runtime::actions::journey(path, feature);
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "{}", question(&next, &steps));
        let _ = err.flush();
        drop(err);
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if !matches!(line.trim().to_lowercase().as_str(), "" | "y" | "yes") {
            break;
        }
        let result = take(path, feature, &next, verbosity);
        let result = speccoder_runtime::next::attach(result, feature);
        code = match crate::output::render(&result, false) {
            Ok(()) => result.exit_code(),
            Err(_) => ExitCode::INTERNAL,
        };
        if result.status() != CommandStatus::Ok {
            break;
        }
    }
    code
}

/// Offers `remedy` as a card — what it does, the command it amounts to,
/// how to undo it — and applies it on a yes. A remedy that writes history
/// waits for an explicit yes (`[y/N]`). Whether it was applied.
pub fn remedy(path: &Path, feature: Option<&str>, remedy: &Remedy) -> bool {
    let mut err = std::io::stderr().lock();
    let _ = write!(err, "{}", remedy_question(remedy));
    let _ = err.flush();
    drop(err);
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
        return false;
    }
    let answer = line.trim().to_lowercase();
    let yes = matches!(answer.as_str(), "y" | "yes") || (!remedy.history && answer.is_empty());
    if !yes {
        return false;
    }
    let Some(result) = apply(path, feature, &remedy.run) else {
        return false;
    };
    let _ = crate::output::render(&result, false);
    result.status() == CommandStatus::Ok
}

/// Applies a fix the terminal applies; `None` for one it does not.
fn apply(path: &Path, feature: Option<&str>, run: &Runnable) -> Option<CommandResult> {
    use speccoder_runtime::actions::{Dispatch, Interface, perform_for};
    perform_for(
        Interface::CliRemedy,
        path,
        feature,
        Dispatch::new(run, Via::Terminal),
    )
}

/// The card [`remedy`] shows, ending in its question.
#[must_use]
pub fn remedy_question(r: &Remedy) -> String {
    use crate::style::Token;
    let look = crate::style::err();
    let prompt = if r.history { "[y/N]" } else { "[Y/n]" };
    let undo = r.undo.as_deref().map(|u| format!("undo: {u}"));
    if look.screen_reader {
        return format!(
            "\nfix: {} — {}.{}\nFix it and run again? yes or no: ",
            r.label,
            r.effect,
            undo.map(|u| format!(" {u}.")).unwrap_or_default()
        );
    }
    // The command it amounts to is the `fix:` line above.
    let mut lines = vec![
        look.paint(Token::Accent, &r.label),
        look.paint(Token::Muted, &r.effect),
    ];
    if let Some(u) = undo {
        lines.push(look.paint(Token::Muted, &u));
    }
    let text = format!(
        "\n{}  {} {} ",
        crate::human::card_in(&look, "Fix", &lines),
        look.paint(Token::Attention, "Fix it and run again?"),
        look.paint(Token::Muted, prompt)
    );
    if look.ascii {
        crate::style::asciify(&text)
    } else {
        text
    }
}

/// What the person is asked: where the feature is, then the step as a
/// card — what, why, the files a commit takes, the command it stands for —
/// then one question.
#[must_use]
pub fn question(a: &Action, steps: &[JourneyStep]) -> String {
    use crate::style::Token;
    let look = crate::style::err();
    let verb = match &a.run {
        Some(Runnable::Commit { .. }) => "Commit them?",
        Some(Runnable::Run { .. } | Runnable::Converge) => "Start it now?",
        _ => "Run it now?",
    };
    if look.screen_reader {
        let files = match &a.run {
            Some(Runnable::Commit { paths }) => format!(" Files: {}.", paths.join(", ")),
            _ => String::new(),
        };
        return format!(
            "\n{}\nnext step: {} — {}.{files} Command: {}\n{verb} yes or no: ",
            crate::human::journey_in(&look, steps),
            a.label,
            a.why,
            a.command
        );
    }
    // The headline says what happens; a skill's label only repeats its
    // command, which is shown below.
    let capital = |t: &str| {
        let mut c = t.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    };
    let mut lines = match &a.run {
        Some(Runnable::Skill { .. }) => vec![look.paint(Token::Accent, &capital(&a.why))],
        _ => vec![
            look.paint(Token::Accent, &capital(&a.label)),
            look.paint(Token::Muted, &a.why),
        ],
    };
    if let Some(Runnable::Commit { paths }) = &a.run {
        lines.push(String::new());
        for p in paths {
            lines.push(look.paint(Token::DiffAdd, &format!("+ {p}")));
        }
        lines.push(look.paint(
            Token::Muted,
            "only these are committed; `git diff` shows them",
        ));
    }
    lines.push(String::new());
    lines.push(format!("{} {}", look.paint(Token::Muted, "$"), a.command));
    let text = format!(
        "\n  {}\n\n{}  {} {} ",
        crate::human::journey_in(&look, steps),
        crate::human::card_in(&look, "Next", &lines),
        look.paint(Token::Attention, verb),
        look.paint(Token::Muted, "[Y/n]")
    );
    if look.ascii {
        crate::style::asciify(&text)
    } else {
        text
    }
}

/// Runs `a` here, at the terminal: inline approvals, live progress, the
/// arguments it carries — if it is a step the terminal takes.
fn take(
    path: &Path,
    feature: Option<&str>,
    a: &Action,
    verbosity: crate::progress::Verbosity,
) -> CommandResult {
    use speccoder_runtime::actions::{Dispatch, Interface, perform_for};
    let refused = || Interface::CliFollow.refusal();
    let Some(run) = &a.run else {
        return refused();
    };
    let approver: std::sync::Arc<dyn speccoder_runtime::approvals::Approver> =
        std::sync::Arc::new(crate::prompt::Terminal);
    let progress: std::sync::Arc<dyn speccoder_runtime::progress::Progress> =
        std::sync::Arc::new(crate::progress::Printer::new(verbosity));
    let mut step = Dispatch::new(run, Via::Terminal);
    step.options.approver = Some(approver);
    step.options.progress = Some(progress);
    perform_for(Interface::CliFollow, path, feature, step).unwrap_or_else(refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit() -> Action {
        Action::new(
            "next",
            "commit the planning files (2)",
            "speccoder runs from committed files",
            "git add … && git commit -m \"spec: …\" -- …",
        )
        .with_run(Runnable::Commit {
            paths: vec![
                "specs/001-x/plan.md".to_owned(),
                "specs/001-x/tasks.md".to_owned(),
            ],
        })
    }

    fn steps() -> Vec<JourneyStep> {
        speccoder_core::actions::journey(speccoder_core::actions::JourneyFacts {
            spec: true,
            plan: true,
            tasks: true,
            uncommitted: true,
            total: 2,
            finished: 0,
        })
    }

    #[test]
    fn a_step_the_terminal_does_not_take_is_refused_before_it_runs() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut a = commit();
        a.run = Some(Runnable::PullRequest);
        let r = take(dir.path(), None, &a, crate::progress::Verbosity::Normal);
        assert_eq!(
            (r.command(), r.status(), r.message()),
            (
                "next",
                CommandStatus::UsageError,
                "not a step speccoder takes here"
            )
        );
    }

    #[test]
    fn a_fix_the_terminal_does_not_apply_is_not_applied() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        assert!(apply(dir.path(), None, &Runnable::PullRequest).is_none());
        assert!(apply(dir.path(), None, &Runnable::FixSetup).is_some());
    }

    #[test]
    fn the_offer_shows_the_journey_the_card_and_one_question() {
        let q = crate::style::with(crate::style::Look::PLAIN, || question(&commit(), &steps()));
        for needle in [
            "✓ spec ── ✓ plan ── ✓ tasks ── ● commit ── · run ── · review",
            "┌─ Next ─",
            "│ Commit the planning files (2)",
            "│ + specs/001-x/plan.md",
            "│ $ git add",
            "Commit them? [Y/n] ",
        ] {
            assert!(q.contains(needle), "missing {needle:?} in\n{q}");
        }
        // Every card line has the same width.
        let widths: Vec<usize> = q
            .lines()
            .filter(|l| l.trim_start().starts_with(['│', '┌', '└']))
            .map(crate::style::width)
            .collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}\n{q}");
    }

    #[test]
    fn ascii_and_screen_readers_get_their_own_form() {
        let ascii = crate::style::Look {
            ascii: true,
            ..crate::style::Look::PLAIN
        };
        let q = crate::style::with(ascii, || question(&commit(), &steps()));
        assert!(q.is_ascii(), "{q}");
        assert!(
            q.contains("[x] spec - [x] plan - [x] tasks - [>] commit"),
            "{q}"
        );
        let reader = crate::style::Look {
            screen_reader: true,
            ..crate::style::Look::PLAIN
        };
        let q = crate::style::with(reader, || question(&commit(), &steps()));
        assert!(
            q.contains("journey: spec done, plan done, tasks done, commit next, run to do"),
            "{q}"
        );
        assert!(
            q.contains("Files: specs/001-x/plan.md, specs/001-x/tasks.md."),
            "{q}"
        );
        assert!(q.ends_with("Commit them? yes or no: "), "{q}");
    }
}
