//! Live progress lines on stderr. Plain text, one line per event,
//! so it reads the same in a terminal, a CI log or `2> run.log`; stdout
//! keeps the final report (or the single JSON object with `--json`, where
//! nothing is printed here).

use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use speccoder_core::stage::{Stage, StageState};
use speccoder_runtime::progress::{Event, Progress};

/// How much the printer says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Verbosity {
    /// Stops, finishes, resumes and what needs a person.
    Quiet,
    /// Every stage and tool call (the default).
    #[default]
    Normal,
    /// Also durations, skipped stages and the tails of what failed.
    Verbose,
}

/// Prints events to stderr.
#[derive(Debug, Default)]
pub struct Printer {
    started: Mutex<Option<Instant>>,
    verbosity: Verbosity,
}

impl Printer {
    /// A printer saying this much.
    #[must_use]
    pub fn new(verbosity: Verbosity) -> Self {
        Self {
            started: Mutex::new(None),
            verbosity,
        }
    }
}

fn tokens(n: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // display only
    if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn money(usd: Option<f64>) -> String {
    usd.map(|u| format!(" · ${u:.4}")).unwrap_or_default()
}

/// A stage as one line — the plain printer, `status`, the TUI and ACP all
/// use this; `None` when the verbosity hides it.
#[must_use]
pub fn stage_line(
    stage: Stage,
    state: StageState,
    detail: Option<&str>,
    elapsed_ms: Option<u64>,
    verbosity: Verbosity,
) -> Option<String> {
    let (mark, shown) = match state {
        StageState::Started => ("·", verbosity != Verbosity::Quiet),
        StageState::Passed => ("✓", verbosity != Verbosity::Quiet),
        StageState::Failed => ("✗", verbosity != Verbosity::Quiet),
        StageState::Skipped => ("–", verbosity == Verbosity::Verbose),
    };
    if !shown {
        return None;
    }
    let mut text = format!("  {mark} {}", stage.name());
    if let Some(d) = detail {
        if state == StageState::Skipped {
            text.push_str(&format!("  skipped ({d})"));
        } else {
            text.push(' ');
            text.push_str(d);
        }
    }
    if verbosity == Verbosity::Verbose
        && let Some(ms) = elapsed_ms
    {
        #[allow(clippy::cast_precision_loss)] // display only
        text.push_str(&format!("  {:.1}s", ms as f64 / 1000.0));
    }
    Some(text)
}

fn tail_lines(tail: &[String]) -> impl Iterator<Item = String> + '_ {
    tail.iter().map(|t| format!("      │ {t}"))
}

/// The lines for `event`, given the seconds since its task started.
#[must_use]
pub fn lines(event: &Event, seconds: Option<u64>, verbosity: Verbosity) -> Vec<String> {
    let quiet = verbosity == Verbosity::Quiet;
    let verbose = verbosity == Verbosity::Verbose;
    match event {
        Event::TaskStarted {
            feature,
            task,
            title,
        } if !quiet => vec![format!("▶ {task} {title}  ({feature})")],
        Event::Stage {
            stage,
            state,
            detail,
            elapsed_ms,
            tail,
            ..
        } => {
            let mut out: Vec<String> =
                stage_line(*stage, *state, detail.as_deref(), *elapsed_ms, verbosity)
                    .into_iter()
                    .collect();
            if verbose && *state == StageState::Failed {
                out.extend(tail_lines(tail));
            }
            out
        }
        Event::ToolCall {
            turn,
            tool,
            target,
            decision,
            ok,
            tail,
            ..
        } if !quiet => {
            let mut out = vec![format!(
                "    turn {turn:<3} {tool:<12} {target:<60} {decision}"
            )];
            if verbose && *ok == Some(false) {
                out.extend(tail_lines(tail));
            }
            out
        }
        Event::Resumed {
            task,
            stage,
            last_call,
        } => vec![format!(
            "Resuming — {task} was interrupted during {}{}; finished calls will not repeat.",
            stage.map_or("a run", Stage::name),
            last_call
                .as_ref()
                .map(|c| format!(" ({c})"))
                .unwrap_or_default()
        )],
        Event::ApprovalRequested { task, what, .. } => {
            vec![format!("△ {task} needs approval — {what}")]
        }
        Event::ApprovalDecided {
            task,
            decision,
            scope,
            by,
            ..
        } => vec![match (decision.as_str(), scope) {
            ("approve", Some(scope)) => format!("  {task} approved ({scope}) by {by}"),
            ("approve", None) => format!("  {task} approved by {by}"),
            _ => format!("  {task} rejected by {by}"),
        }],
        Event::TaskFinished {
            task,
            outcome,
            message,
            input_tokens,
            output_tokens,
            usd,
            ..
        } => {
            let mark = match outcome.as_str() {
                "verified" | "regression_only" | "completed" => "✓",
                "blocked" | "needs_approval" | "denied" | "not_demonstrated" | "incomplete"
                | "budget_exhausted" => "⏸",
                _ => "✗",
            };
            vec![format!(
                "{mark} {task} {outcome} · {} in / {} out{}{} — {message}",
                tokens(*input_tokens),
                tokens(*output_tokens),
                money(*usd),
                seconds.map(|s| format!(" · {s}s")).unwrap_or_default()
            )]
        }
        Event::Notice { text } if !quiet => vec![format!("» {text}")],
        // Running totals go to `speccoder status`; a line per turn is noise.
        _ => Vec::new(),
    }
}

/// The one line for `event` at the default verbosity.
#[cfg(test)]
#[must_use]
pub fn line(event: &Event, seconds: Option<u64>) -> Option<String> {
    lines(event, seconds, Verbosity::Normal).into_iter().next()
}

impl Progress for Printer {
    fn event(&self, event: &Event) {
        let seconds = match event {
            Event::TaskStarted { .. } => {
                if let Ok(mut s) = self.started.lock() {
                    *s = Some(Instant::now());
                }
                None
            }
            Event::TaskFinished { .. } => self
                .started
                .lock()
                .ok()
                .and_then(|s| s.map(|t| t.elapsed().as_secs())),
            _ => None,
        };
        let out = lines(event, seconds, self.verbosity);
        if !out.is_empty() {
            let mut err = std::io::stderr().lock();
            for l in out {
                let _ = writeln!(err, "{l}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_read_like_a_log() {
        let started = Event::TaskStarted {
            feature: "001-x".to_owned(),
            task: "T002".to_owned(),
            title: "Add login".to_owned(),
        };
        assert_eq!(
            line(&started, None).as_deref(),
            Some("▶ T002 Add login  (001-x)")
        );
        let finished = Event::TaskFinished {
            task: "T002".to_owned(),
            outcome: "verified".to_owned(),
            message: "verdict: verified".to_owned(),
            run: None,
            input_tokens: 12_345,
            output_tokens: 678,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            usd: Some(0.0123),
        };
        assert_eq!(
            line(&finished, Some(42)).as_deref(),
            Some("✓ T002 verified · 12.3k in / 678 out · $0.0123 · 42s — verdict: verified")
        );
        assert!(
            line(
                &Event::Usage {
                    task: "T002".to_owned(),
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    usd: None
                },
                None
            )
            .is_none()
        );
    }

    #[test]
    fn stage_lines_follow_state_and_verbosity() {
        use Verbosity::{Normal, Quiet, Verbose};
        let row =
            |stage, state, detail: Option<&str>, ms, v| stage_line(stage, state, detail, ms, v);
        assert_eq!(
            row(Stage::Baseline, StageState::Started, None, None, Normal).as_deref(),
            Some("  · baseline")
        );
        assert_eq!(
            row(
                Stage::Attempt,
                StageState::Started,
                Some("1/2 · sample 2/3"),
                None,
                Normal
            )
            .as_deref(),
            Some("  · attempt 1/2 · sample 2/3")
        );
        assert_eq!(
            row(Stage::Verify, StageState::Passed, None, Some(4100), Normal).as_deref(),
            Some("  ✓ verify")
        );
        assert_eq!(
            row(Stage::Verify, StageState::Passed, None, Some(4100), Verbose).as_deref(),
            Some("  ✓ verify  4.1s")
        );
        assert_eq!(
            row(
                Stage::Commit,
                StageState::Passed,
                Some("9c1e2ab"),
                None,
                Normal
            )
            .as_deref(),
            Some("  ✓ commit 9c1e2ab")
        );
        assert_eq!(
            row(Stage::Verify, StageState::Failed, None, None, Normal).as_deref(),
            Some("  ✗ verify")
        );
        assert_eq!(
            row(
                Stage::Mutation,
                StageState::Skipped,
                Some("not configured"),
                None,
                Normal
            ),
            None
        );
        assert_eq!(
            row(
                Stage::Mutation,
                StageState::Skipped,
                Some("not configured"),
                None,
                Verbose
            )
            .as_deref(),
            Some("  – mutation  skipped (not configured)")
        );
        for stage in Stage::ALL {
            for state in [
                StageState::Started,
                StageState::Passed,
                StageState::Failed,
                StageState::Skipped,
            ] {
                assert_eq!(
                    row(stage, state, None, None, Quiet),
                    None,
                    "{stage:?} {state:?}"
                );
            }
        }
    }

    #[test]
    fn resumes_and_stops_for_a_person_read_plainly() {
        let resumed = Event::Resumed {
            task: "T004".to_owned(),
            stage: Some(Stage::Attempt),
            last_call: Some("run_command cargo test (allow)".to_owned()),
        };
        assert_eq!(
            lines(&resumed, None, Verbosity::Quiet),
            [
                "Resuming — T004 was interrupted during attempt (run_command cargo test (allow)); finished calls will not repeat."
            ]
        );
        let bare = Event::Resumed {
            task: "T004".to_owned(),
            stage: None,
            last_call: None,
        };
        assert_eq!(
            lines(&bare, None, Verbosity::Normal),
            ["Resuming — T004 was interrupted during a run; finished calls will not repeat."]
        );
        let needs = Event::ApprovalRequested {
            task: "T004".to_owned(),
            call: "c1".to_owned(),
            tool: "run_command".to_owned(),
            class: String::new(),
            what: "run_command cargo add x".to_owned(),
        };
        assert_eq!(
            lines(&needs, None, Verbosity::Quiet),
            ["△ T004 needs approval — run_command cargo add x"]
        );
    }

    #[test]
    fn tails_of_failures_show_only_when_verbose() {
        let tail: Vec<String> = (4..9).map(|i| format!("line {i}")).collect();
        let call = Event::ToolCall {
            task: "T004".to_owned(),
            turn: 2,
            tool: "run_command".to_owned(),
            target: "cargo test".to_owned(),
            decision: "allow".to_owned(),
            ok: Some(false),
            tail: tail.clone(),
        };
        assert_eq!(lines(&call, None, Verbosity::Normal).len(), 1);
        let verbose = lines(&call, None, Verbosity::Verbose);
        assert_eq!(verbose.len(), 6);
        assert_eq!(verbose[5], "      │ line 8");
        assert!(lines(&call, None, Verbosity::Quiet).is_empty());
        // The runtime keeps the last five lines of what a command printed.
        let printed = (1..9)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(speccoder_runtime::progress::tail(&printed), tail);
    }
}
