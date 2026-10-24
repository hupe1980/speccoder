//! The terminal approver: asks the person at the keyboard while an
//! attempt waits. Used only when stdin and stderr are terminals and `--json`
//! is off; otherwise the attempt stops and `speccoder task approve` decides.
//!
//! One question, its full scope (F-054): what the call does (a diff, the
//! whole command), why it stopped, what it can reach, what the model
//! claimed, and what each answer covers.

use std::io::{BufRead, Write};

use speccoder_runtime::approvals::{Answer, Approver, Presentation, Scope, Who};

/// Asks on stderr, reads the answer from stdin.
#[derive(Debug, Default)]
pub struct Terminal;

/// `■■□ medium`.
#[must_use]
pub fn risk_bar(risk: &str) -> String {
    let filled = match risk {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    };
    format!("{}{} {risk}", "■".repeat(filled), "□".repeat(3 - filled))
}

/// The title line, the body, why, the sandbox and the model's claim:
/// what every view of a pending call shows.
#[must_use]
pub fn describe(p: &Presentation, body: &[String]) -> String {
    let head = format!("  {}  {}", p.title, p.subject);
    let width = head.chars().count();
    let pad = 70usize.saturating_sub(width).max(3);
    let mut out = format!(
        "{head}{}risk {}\n",
        " ".repeat(pad),
        risk_bar(&p.request.risk)
    );
    let look = crate::style::err();
    for line in body {
        // A diff's added and removed lines, in their colours.
        let painted = if line.starts_with('+') && !line.starts_with("+++") {
            look.paint(crate::style::Token::DiffAdd, line)
        } else if line.starts_with('-') && !line.starts_with("---") {
            look.paint(crate::style::Token::DiffDel, line)
        } else {
            line.clone()
        };
        out.push_str(&format!("  {painted}\n"));
    }
    out.push_str(&format!("  Why stopped  {}\n", p.request.reason));
    out.push_str(&format!("  Sandbox      {}\n", p.sandbox));
    if let Some(claim) = &p.request.claim {
        out.push_str(&format!("  Model says   \"{claim}\"   (a claim)\n"));
    }
    out
}

fn options(p: &Presentation) -> String {
    let reader = crate::style::err().screen_reader;
    let mut out = String::new();
    for o in &p.options {
        if reader {
            out.push_str(&format!(
                "choice {}: {} (or type {})\n",
                o.number, o.label, o.key
            ));
        } else {
            out.push_str(&format!("  {}. {:<66} ({})\n", o.number, o.label, o.key));
        }
    }
    out
}

/// `text` as the stderr look wants it: ASCII symbols when asked.
fn looked(text: String) -> String {
    if crate::style::err().ascii {
        crate::style::asciify(&text)
    } else {
        text
    }
}

fn footer(p: &Presentation) -> String {
    let keys: Vec<String> = p.options.iter().map(|o| o.key.to_string()).collect();
    format!(
        "  {} or 1-5 · add a note: \"y <note>\" · v view all · esc later  > ",
        keys.join(" ")
    )
}

/// What the person is shown for a call that needs them.
#[must_use]
pub fn render(p: &Presentation) -> String {
    let r = &p.request;
    let title = if r.task_title.is_empty() {
        String::new()
    } else {
        format!(" {}", r.task_title)
    };
    let turn = if r.turn > 0 {
        format!(" · turn {}", r.turn)
    } else {
        String::new()
    };
    let look = crate::style::err();
    if look.screen_reader {
        return looked(format!(
            "approval required: {}{title}{turn}\n{}risk: {}\n{}",
            r.task,
            describe(p, &p.body).replace("  ", " "),
            r.risk,
            options(p)
        ));
    }
    let head = look.paint(
        crate::style::Token::Attention,
        &format!("△ Approval needed — {}{title}{turn}", r.task),
    );
    looked(format!("\n{head}\n{}{}", describe(p, &p.body), options(p)))
}

/// The whole body (`v`), then the choices again.
#[must_use]
pub fn render_full(p: &Presentation) -> String {
    looked(format!("\n{}{}", describe(p, &p.full), options(p)))
}

/// What a typed line means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Run it, this far, with an optional note to the model.
    Approve {
        /// How far.
        scope: Scope,
        /// The note.
        note: Option<String>,
    },
    /// Refuse it; the reason, when typed on the same line.
    Reject {
        /// The reason.
        reason: Option<String>,
    },
    /// End the task here; the call waits.
    Stop,
    /// Decide later: nothing changes.
    Later,
    /// Show the whole body.
    View,
    /// A choice not offered for this call.
    NotOffered(&'static str),
    /// Not an answer.
    Unknown,
}

/// The answer `line` means, given the choices `offered`.
#[must_use]
pub fn parse(line: &str, offered: &[String]) -> Parsed {
    if line.starts_with('\x1b') {
        return Parsed::Later;
    }
    let line = line.trim();
    let (head, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    let text = (!rest.is_empty()).then(|| rest.to_owned());
    let has = |c: &str| offered.iter().any(|o| o == c);
    match head.to_lowercase().as_str() {
        "esc" | "later" | "l" => Parsed::Later,
        "v" | "view" => Parsed::View,
        "s" | "stop" | "5" => Parsed::Stop,
        "y" | "yes" | "1" => Parsed::Approve {
            scope: Scope::Once,
            note: text,
        },
        "t" | "task" | "2" if has("task") => Parsed::Approve {
            scope: Scope::Task,
            note: text,
        },
        "t" | "task" | "2" => Parsed::NotOffered(
            "\"for the task\" is not offered: the call's class is not shown in full",
        ),
        "r" | "run" | "3" if has("run") => Parsed::Approve {
            scope: Scope::Run,
            note: text,
        },
        "r" | "run" | "3" => {
            Parsed::NotOffered("\"for this run\" is not offered for high-risk calls")
        }
        "n" | "no" | "4" => Parsed::Reject { reason: text },
        _ => Parsed::Unknown,
    }
}

/// Asks about `p` on `out`, reading answers from `input`; end of input
/// rejects ("no answer (input closed)").
pub fn ask_with(input: &mut dyn BufRead, out: &mut dyn Write, p: &Presentation) -> Answer {
    let by = || Some(Who::local("terminal"));
    let closed = || Answer::reject("no answer (input closed)", Some(Who::local("terminal")));
    let mut read = |out: &mut dyn Write, prompt: &str| -> Option<String> {
        let _ = write!(out, "{prompt}");
        let _ = out.flush();
        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => None, // end of input: nobody is there
            Ok(_) => Some(line),
        }
    };
    let _ = write!(out, "{}", render(p));
    loop {
        let Some(line) = read(out, &footer(p)) else {
            return closed();
        };
        match parse(&line, &p.request.options) {
            Parsed::Approve { scope, note } => return Answer::approve(scope, note, by()),
            Parsed::Reject { reason: Some(r) } => return Answer::reject(r, by()),
            Parsed::Reject { reason: None } => loop {
                let Some(r) = read(out, "  Reason (the model is told): ") else {
                    return closed();
                };
                if !r.trim().is_empty() {
                    return Answer::reject(r.trim(), by());
                }
            },
            Parsed::Stop => return Answer::stop(by()),
            Parsed::Later => {
                let _ = writeln!(
                    out,
                    "  still waiting — answer when ready (s stops the task and keeps the call for `speccoder task approve`)"
                );
            }
            Parsed::View => {
                let _ = write!(out, "{}", render_full(p));
            }
            Parsed::NotOffered(why) => {
                let _ = writeln!(out, "  {why}");
            }
            Parsed::Unknown => {}
        }
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

    fn ask(&self, p: &Presentation) -> Answer {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut err = std::io::stderr().lock();
        ask_with(&mut input, &mut err, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use speccoder_runtime::approvals::{Decision, PresentContext, Request, options, present};

    fn edit() -> Presentation {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        present(
            &Request {
                task: "001-slugify/T004".to_owned(),
                task_title: "Handle Unicode input".to_owned(),
                turn: 8,
                tool: "edit_file".to_owned(),
                arguments: json!({ "path": "Cargo.toml", "old_string": "serde = \"1\"\n", "new_string": "serde = \"1\"\nunicode-normalization = \"0.1\"\n" }),
                kind: "edit".to_owned(),
                reason: "`Cargo.toml` is a dependency manifest".to_owned(),
                covers: "write `Cargo.toml`".to_owned(),
                risk: "medium".to_owned(),
                claim: Some(
                    "NFKD normalisation is needed for the Unicode acceptance test".to_owned(),
                ),
                options: options("medium", true),
            },
            &PresentContext {
                root: Some(dir.path().to_path_buf()),
                sandbox: Some("Seatbelt".to_owned()),
                network: "none".to_owned(),
                file_text: None,
            },
        )
    }

    fn run(risk: &str) -> Presentation {
        present(
            &Request {
                task: "001-x/T002".to_owned(),
                task_title: String::new(),
                turn: 3,
                tool: "run_command".to_owned(),
                arguments: json!({ "argv": ["cargo", "add", "serde"] }),
                kind: "run".to_owned(),
                reason: "`cargo add` changes dependencies".to_owned(),
                covers: "run `cargo add serde`".to_owned(),
                risk: risk.to_owned(),
                claim: None,
                options: options(risk, true),
            },
            &PresentContext {
                root: None,
                sandbox: Some("Seatbelt".to_owned()),
                network: "none".to_owned(),
                file_text: None,
            },
        )
    }

    #[test]
    fn an_edit_shows_the_diff_why_the_sandbox_the_claim_and_each_scope() {
        let shown = render(&edit());
        assert_eq!(
            shown,
            "\n△ Approval needed — 001-slugify/T004 Handle Unicode input · turn 8
  Edit this file?  Cargo.toml                                         risk ■■□ medium
      3     3
      4     4   [dependencies]
      5     5   serde = \"1\"
            6 + unicode-normalization = \"0.1\"
  Why stopped  `Cargo.toml` is a dependency manifest
  Sandbox      a file write — nothing is executed
  Model says   \"NFKD normalisation is needed for the Unicode acceptance test\"   (a claim)
  1. Yes, once                                                          (y)
  2. Yes, for the rest of T004: this path, any content                  (t)
  3. Yes, for this run: this path, any content, in any task             (r)
  4. No, and tell the model why …                                       (n)
  5. Stop this task here (it waits; decide later)                       (s)
"
        );
    }

    #[test]
    fn a_high_risk_command_offers_no_run_scope() {
        let shown = render(&run("high"));
        assert!(
            shown.contains("  Run this command?  cargo add serde"),
            "{shown}"
        );
        assert!(
            shown.contains("  $ cargo add serde\n  cwd: . · network: none\n"),
            "{shown}"
        );
        assert!(shown.contains("sandbox on (Seatbelt) · network: none · writes: worktree"));
        assert!(!shown.contains("3. "), "{shown}");
        assert!(shown.contains("  4. No, and tell the model why"));
    }

    #[test]
    fn answers_parse_old_and_new_forms() {
        let all = options("medium", true);
        let high = options("high", true);
        let ap = |scope, note: Option<&str>| Parsed::Approve {
            scope,
            note: note.map(str::to_owned),
        };
        assert_eq!(parse(" Y\n", &all), ap(Scope::Once, None));
        assert_eq!(parse("yes", &all), ap(Scope::Once, None));
        assert_eq!(parse("1", &all), ap(Scope::Once, None));
        assert_eq!(parse("t", &all), ap(Scope::Task, None));
        assert_eq!(
            parse("task keep it small", &all),
            ap(Scope::Task, Some("keep it small"))
        );
        assert_eq!(parse("r", &all), ap(Scope::Run, None));
        assert!(matches!(parse("r", &high), Parsed::NotOffered(_)));
        assert!(matches!(parse("3", &high), Parsed::NotOffered(_)));
        assert_eq!(parse("y use 0.1", &all), ap(Scope::Once, Some("use 0.1")));
        assert_eq!(parse("no", &all), Parsed::Reject { reason: None });
        assert_eq!(
            parse("n not that crate", &all),
            Parsed::Reject {
                reason: Some("not that crate".to_owned())
            }
        );
        assert_eq!(parse("s", &all), Parsed::Stop);
        assert_eq!(parse("\x1b\n", &all), Parsed::Later);
        assert_eq!(parse("later", &all), Parsed::Later);
        assert_eq!(parse("v", &all), Parsed::View);
        assert_eq!(parse("maybe", &all), Parsed::Unknown);
    }

    fn answer(p: &Presentation, typed: &str) -> (Answer, String) {
        let mut input = std::io::Cursor::new(typed.as_bytes().to_vec());
        let mut out = Vec::new();
        let a = ask_with(&mut input, &mut out, p);
        (a, String::from_utf8(out).unwrap())
    }

    #[test]
    fn the_prompt_loop_steers_rejects_stops_and_waits() {
        let p = edit();
        let (a, _) = answer(&p, "t use the 0.1 series\n");
        assert_eq!((a.decision, a.scope), (Decision::Approve, Scope::Task));
        assert_eq!(a.note.as_deref(), Some("use the 0.1 series"));
        // A rejection needs a reason; an empty one asks again.
        let (a, out) = answer(&p, "n\n\nno new dependencies\n");
        assert_eq!(a.reason.as_deref(), Some("no new dependencies"));
        assert_eq!(out.matches("Reason (the model is told)").count(), 2);
        // Later decides nothing.
        let (a, out) = answer(&p, "later\ny\n");
        assert_eq!(a.decision, Decision::Approve);
        assert!(out.contains("still waiting"));
        let (a, _) = answer(&p, "s\n");
        assert_eq!(a.decision, Decision::Stop);
        // End of input is no answer.
        let (a, _) = answer(&p, "");
        assert_eq!(a.reason.as_deref(), Some("no answer (input closed)"));
        let (a, _) = answer(&p, "n\n");
        assert_eq!(a.reason.as_deref(), Some("no answer (input closed)"));
        // `v` shows everything, then asks again.
        let (_, out) = answer(&p, "v\ny\n");
        assert_eq!(out.matches("1. Yes, once").count(), 2);
        // A choice not offered says so.
        let (_, out) = answer(&run("high"), "r\ny\n");
        assert!(out.contains("not offered for high-risk calls"));
    }
}
