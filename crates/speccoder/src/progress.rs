//! Live progress lines on stderr. Plain text, one line per event,
//! so it reads the same in a terminal, a CI log or `2> run.log`; stdout
//! keeps the final report (or the single JSON object with `--json`, where
//! nothing is printed here).

use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use speccoder_runtime::progress::{Event, Progress};

/// Prints events to stderr.
#[derive(Debug, Default)]
pub struct Printer {
    started: Mutex<Option<Instant>>,
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

/// The line for `event`, given the seconds since its task started.
#[must_use]
pub fn line(event: &Event, seconds: Option<u64>) -> Option<String> {
    Some(match event {
        Event::TaskStarted {
            feature,
            task,
            title,
        } => format!("▶ {task} {title}  ({feature})"),
        Event::Phase { phase, .. } => format!("  · {phase}"),
        Event::ToolCall {
            turn,
            tool,
            target,
            decision,
            ..
        } => format!("    turn {turn:<3} {tool:<12} {target:<60} {decision}"),
        // Running totals go to `speccoder status`; a line per turn is noise.
        Event::Usage { .. } => return None,
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
            format!(
                "{mark} {task} {outcome} · {} in / {} out{}{} — {message}",
                tokens(*input_tokens),
                tokens(*output_tokens),
                money(*usd),
                seconds.map(|s| format!(" · {s}s")).unwrap_or_default()
            )
        }
        Event::Notice { text } => format!("» {text}"),
    })
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
        if let Some(l) = line(event, seconds) {
            let _ = writeln!(std::io::stderr().lock(), "{l}");
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
            input_tokens: 12_345,
            output_tokens: 678,
            cache_read_tokens: 0,
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
                    usd: None
                },
                None
            )
            .is_none()
        );
    }
}
