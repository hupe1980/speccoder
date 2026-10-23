//! The terminal approver: asks the person at the keyboard while an
//! attempt waits. Used only when stdin and stderr are terminals and `--json`
//! is off; otherwise the attempt stops and `speccoder approve` decides.

use std::io::{BufRead, Write};

use speccoder_runtime::approvals::{Answer, Approver, Request};

/// Lines of a write shown before asking.
const PREVIEW_LINES: usize = 20;

/// Asks on stderr, reads the answer from stdin.
#[derive(Debug, Default)]
pub struct Terminal;

/// What the person is shown for `request`.
#[must_use]
pub fn render(request: &Request) -> String {
    let mut out = format!(
        "\nspeccoder needs your approval — {} (risk: {})\n  {} — {}\n",
        request.task, request.risk, request.covers, request.reason
    );
    let text = |k: &str| request.arguments[k].as_str().unwrap_or_default().to_owned();
    let mut preview = |prefix: &str, body: &str| {
        let lines: Vec<&str> = body.lines().collect();
        for l in lines.iter().take(PREVIEW_LINES) {
            out.push_str(&format!("  │ {prefix}{l}\n"));
        }
        if lines.len() > PREVIEW_LINES {
            out.push_str(&format!(
                "  │ … {} more lines\n",
                lines.len() - PREVIEW_LINES
            ));
        }
    };
    match request.tool.as_str() {
        "write_file" => preview("", &text("content")),
        "edit_file" => {
            preview("- ", &text("old_string"));
            preview("+ ", &text("new_string"));
        }
        _ => {}
    }
    out
}

/// The answer a typed line means; `None` asks again.
#[must_use]
pub fn parse(line: &str) -> Option<(bool, bool)> {
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" => Some((true, false)),
        "t" | "task" => Some((true, true)),
        "n" | "no" => Some((false, false)),
        _ => None,
    }
}

impl Approver for Terminal {
    fn answer(&self, question: &str) -> Option<String> {
        let mut err = std::io::stderr().lock();
        let _ = write!(
            err,
            "\nspeccoder asks: {question}\n  answer (empty to skip) > "
        );
        let _ = err.flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(n) if n > 0 && !line.trim().is_empty() => Some(line.trim().to_owned()),
            _ => None,
        }
    }

    fn ask(&self, request: &Request) -> Answer {
        let stdin = std::io::stdin();
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "{}", render(request));
        let mut read = |prompt: &str| -> Option<String> {
            let _ = write!(err, "{prompt}");
            let _ = err.flush();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => None, // end of input: nobody is there
                Ok(_) => Some(line),
            }
        };
        loop {
            let Some(line) = read("  [y] once  [t] for this task  [n] reject  > ") else {
                return Answer {
                    by: Some(speccoder_runtime::approvals::Who::local("terminal")),
                    approved: false,
                    for_task: false,
                    reason: Some("no answer (input closed)".to_owned()),
                };
            };
            match parse(&line) {
                Some((true, for_task)) => {
                    return Answer {
                        by: Some(speccoder_runtime::approvals::Who::local("terminal")),
                        approved: true,
                        for_task,
                        reason: None,
                    };
                }
                Some((false, _)) => {
                    let reason = read("  Reason (the model is told): ")
                        .map(|r| r.trim().to_owned())
                        .filter(|r| !r.is_empty())
                        .unwrap_or_else(|| "rejected without a reason".to_owned());
                    return Answer {
                        by: Some(speccoder_runtime::approvals::Who::local("terminal")),
                        approved: false,
                        for_task: false,
                        reason: Some(reason),
                    };
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn answers_and_previews() {
        assert_eq!(parse(" Y\n"), Some((true, false)));
        assert_eq!(parse("t"), Some((true, true)));
        assert_eq!(parse("no"), Some((false, false)));
        assert_eq!(parse("maybe"), None);
        let r = Request {
            task: "001-x/T005".to_owned(),
            tool: "edit_file".to_owned(),
            arguments: json!({ "path": "Cargo.toml", "old_string": "[dependencies]", "new_string": "[dependencies]\nserde = \"1\"" }),
            reason: "`Cargo.toml` is a dependency manifest".to_owned(),
            covers: "write `Cargo.toml`".to_owned(),
            risk: "medium".to_owned(),
        };
        let shown = render(&r);
        assert!(shown.contains("001-x/T005 (risk: medium)"));
        assert!(
            shown.contains("  │ - [dependencies]\n  │ + [dependencies]\n  │ + serde = \"1\""),
            "{shown}"
        );
    }
}
