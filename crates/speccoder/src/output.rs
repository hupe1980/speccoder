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
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        serde_json::to_writer(&mut *out, result)?;
        return writeln!(out);
    }
    // Rendered whole, then written: ASCII mode translates every symbol of
    // every renderer in one place (F-055).
    let (mut body, mut block) = (Vec::new(), Vec::new());
    human_body(result, &mut body, &mut block)?;
    let shown = code_block(result, &mut block)?;
    let next: Vec<_> = result
        .next()
        .iter()
        .filter(|s| !shown.contains(&s.command))
        .cloned()
        .collect();
    human::next(&mut body, &next)?;
    let write = |w: &mut dyn Write, bytes: Vec<u8>, ascii: bool| -> std::io::Result<()> {
        let text = String::from_utf8_lossy(&bytes);
        if ascii {
            w.write_all(crate::style::asciify(&text).as_bytes())
        } else {
            w.write_all(text.as_bytes())
        }
    };
    write(err, block, crate::style::err().ascii)?;
    write(out, body, crate::style::out().ascii)
}

/// `error[CODE]: …` (or `stopped`, `warning`), why, `fix:` lines and
/// `docs:` — on stderr; the fixes shown, so `Next:` does not repeat them.
/// With a payload the report already said what happened, so the headline
/// is the code's title; without one it is the message.
fn code_block(result: &CommandResult, err: &mut dyn Write) -> std::io::Result<Vec<String>> {
    let Some(code) = result.code() else {
        return Ok(Vec::new());
    };
    use crate::style::Token;
    let look = crate::style::err();
    let fix = result.fix();
    let prefix = code.kind.prefix();
    let token = match code.kind {
        speccoder_core::codes::Kind::Error => Token::Danger,
        speccoder_core::codes::Kind::Stop => Token::Attention,
        speccoder_core::codes::Kind::Warning => Token::Regression,
    };
    let reported = !matches!(result.data(), None | Some(CommandData::InPlace(_)));
    let headline = if reported || result.message().is_empty() {
        code.title
    } else {
        result.message()
    };
    if look.screen_reader {
        writeln!(err, "{prefix}: {} — {headline}", code.name)?;
        if headline != code.title {
            writeln!(err, "why: {}", code.title)?;
        }
    } else {
        writeln!(
            err,
            "{}: {headline}",
            look.paint(token, &format!("{prefix}[{}]", code.name))
        )?;
        if headline != code.title {
            writeln!(err, "  {}", code.title)?;
        }
    }
    let indent = if look.screen_reader { "" } else { "  " };
    if let Some(CommandData::InPlace(p)) = result.data() {
        for r in &p.paths {
            writeln!(err, "{indent}{}  ({})", r.path, r.reason)?;
        }
    }
    for f in &fix {
        let shown = match f.strip_prefix("→ ") {
            Some(action) => action.to_owned(),
            None => look.paint(Token::Accent, f),
        };
        writeln!(err, "{indent}fix: {shown}")?;
    }
    writeln!(
        err,
        "{indent}docs: {}",
        look.paint(Token::Muted, &code.docs())
    )?;
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
            use crate::style::{Glyph, Token};
            let look = crate::style::out();
            writeln!(
                out,
                "Checks speccoder runs to decide \"nothing regressed\":"
            )?;
            for c in &i.checks {
                writeln!(
                    out,
                    "  {:<10} {:<5} {:<55} {}",
                    c.name,
                    c.tier,
                    c.argv.join(" "),
                    look.paint(Token::Muted, &c.why)
                )?;
            }
            if !i.ran.is_empty() {
                let shown: Vec<String> = i
                    .ran
                    .iter()
                    .map(|r| {
                        let g = if r.passed {
                            Glyph::Verified
                        } else {
                            Glyph::Failed
                        };
                        format!(
                            "{} {} {:.1}s",
                            r.name,
                            look.paint(g.token(), look.mark(g)),
                            r.duration_ms as f64 / 1000.0
                        )
                    })
                    .collect();
                writeln!(out, "On the untouched code  {}", shown.join(" · "))?;
                // Why a check fails before any change: the line that says it.
                for r in i.ran.iter().filter(|r| !r.passed) {
                    if let Some(why) = &r.reason {
                        writeln!(
                            out,
                            "  {} {}",
                            look.paint(Token::Danger, &format!("{}:", r.name)),
                            why
                        )?;
                    }
                }
            }
            let key = if i.key.set {
                format!("{} found", i.key.variable)
            } else {
                look.paint(Token::Attention, &format!("{} is not set", i.key.variable))
            };
            writeln!(out, "Model      {}  ({key})", i.model)?;
            if !i.instructions.is_empty() {
                writeln!(
                    out,
                    "Guidance   {} (given to every attempt)",
                    i.instructions.join(", ")
                )?;
            }
            if let Some(c) = &i.committed {
                writeln!(out, "Committed  {c} \"speccoder: config\"")?;
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Doctor(d))) => {
            use crate::style::{Glyph, Token};
            use speccoder_core::DoctorLevel;
            let look = crate::style::out();
            for c in &d.checks {
                let g = match c.level {
                    DoctorLevel::Pass => Glyph::Verified,
                    DoctorLevel::Info => Glyph::Ready,
                    DoctorLevel::Warn => Glyph::Regression,
                    DoctorLevel::Fail => Glyph::Failed,
                };
                if look.screen_reader {
                    writeln!(
                        out,
                        "check: {} — {} — {}",
                        c.check,
                        g.word_for_doctor(),
                        c.detail
                    )?;
                } else {
                    writeln!(out, "  {} {:<13} {}", look.cell(g), c.check, c.detail)?;
                }
                if matches!(c.level, DoctorLevel::Warn | DoctorLevel::Info) {
                    for f in &c.fix {
                        let f = f.strip_prefix("→ ").unwrap_or(f);
                        writeln!(
                            out,
                            "      {}",
                            look.paint(Token::Accent, &format!("fix: {f}"))
                        )?;
                    }
                }
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Control(_))) => {
            writeln!(&mut *out, "{}", result.message())
        }
        // `config get` says one key; `config` lists every one.
        (_, Some(CommandData::Config(_))) if result.message().contains(" = ") => {
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Config(c))) => {
            for line in crate::human::config(c) {
                writeln!(out, "{line}")?;
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Issues(r))) => {
            for i in &r.issues {
                let labels = if i.labels.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", i.labels.join(", "))
                };
                let started = i
                    .feature
                    .as_deref()
                    .map(|f| format!("  → {f}"))
                    .unwrap_or_default();
                let mine = if i.mine { "● " } else { "  " };
                writeln!(out, "{mine}#{:<5} {}{labels}{started}", i.number, i.title)?;
            }
            writeln!(out, "{}", result.message())
        }
        (_, Some(CommandData::Features(f))) => {
            for line in crate::human::features(f) {
                writeln!(out, "{line}")?;
            }
            writeln!(out, "{}", result.message())
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

#[cfg(test)]
mod look_tests {
    use super::*;
    use crate::style::{self, Look, Theme};
    use speccoder_core::{StatusReport, StatusTotals, TaskStatus};

    fn task(id: &str, state: &str, title: &str) -> TaskStatus {
        TaskStatus {
            id: id.to_owned(),
            title: title.to_owned(),
            phase: "Phase 2: User Story 1 - Login".to_owned(),
            state: state.to_owned(),
            intent_changed: false,
            blocked_by: if state == "blocked" {
                vec!["T001".to_owned()]
            } else {
                Vec::new()
            },
            commit: None,
            run_id: None,
            last_attempt: None,
            note: None,
            attestation: None,
            strength: None,
        }
    }

    fn report() -> CommandResult {
        let long = "Handle Unicode input in the slug generator, including combining marks and wide characters such as 日本語";
        let tasks = [
            "verified",
            "regression_only",
            "attested",
            "ticked_without_evidence",
            "evidence_without_tick",
            "skipped",
            "ready",
            "blocked",
        ]
        .iter()
        .enumerate()
        .map(|(i, s)| {
            task(
                &format!("T{:03}", i + 1),
                s,
                if i == 0 { long } else { "Add login" },
            )
        })
        .collect();
        let r = CommandResult::ok("report", "8 tasks").with_data(CommandData::Report(Box::new(
            StatusReport {
                issues: Vec::new(),
                feature: "001-x".to_owned(),
                branch: Some("speccoder/001-x".to_owned()),
                source: "branch".to_owned(),
                selected_by: String::new(),
                artifacts: Vec::new(),
                planning: Vec::new(),
                uncommitted: Vec::new(),
                journey: Vec::new(),
                unmerged_commits: 0,
                tasks,
                convergence: Vec::new(),
                trace: Vec::new(),
                totals: StatusTotals::default(),
            },
        )));
        speccoder_runtime::next::attach(r, None)
    }

    fn failure() -> CommandResult {
        speccoder_runtime::next::attach(
            CommandResult::new("run", CommandStatus::NotAProject, "no project here"),
            None,
        )
    }

    fn render(look: Look, result: &CommandResult) -> (String, String) {
        style::with(look, || {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            render_into(result, false, &mut out, &mut err).unwrap();
            (
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
            )
        })
    }

    #[test]
    fn colour_never_changes_the_text_in_any_theme() {
        for result in [report(), failure()] {
            let plain = render(Look::PLAIN, &result);
            assert!(!plain.0.contains('\u{1b}') && !plain.1.contains('\u{1b}'));
            for theme in Theme::ALL {
                let look = Look {
                    color: true,
                    theme,
                    ..Look::PLAIN
                };
                let (out, err) = render(look, &result);
                assert!(
                    out.contains('\u{1b}') || err.contains('\u{1b}'),
                    "{theme:?} painted nothing"
                );
                assert_eq!(style::strip(&out), plain.0, "{theme:?}");
                assert_eq!(style::strip(&err), plain.1, "{theme:?}");
            }
        }
    }

    #[test]
    fn every_line_fits_the_terminal() {
        for width in [60, 80, 160] {
            let (out, _) = render(
                Look {
                    width,
                    ..Look::PLAIN
                },
                &report(),
            );
            for line in out.lines().filter(|l| l.starts_with("  ")) {
                assert!(
                    style::width(line) <= width,
                    "{width}: {line:?} ({})",
                    style::width(line)
                );
            }
            if width == 160 {
                assert!(out.contains("wide characters such as 日本語"), "{out}");
            } else {
                assert!(out.contains('…'));
            }
        }
    }

    #[test]
    fn ascii_leaves_no_symbol() {
        let look = Look {
            ascii: true,
            ..Look::PLAIN
        };
        for result in [report(), failure()] {
            let (out, err) = render(look, &result);
            let symbols = |s: &str| {
                s.chars()
                    .filter(|c| "✓~A!?–·⧗◐✗△…→│─┌".contains(*c) && !c.is_ascii())
                    .count()
            };
            assert_eq!(symbols(&out) + symbols(&err), 0, "{out}{err}");
        }
        let (out, _) = render(look, &report());
        assert!(out.contains("[ok]") && out.contains("[..]"), "{out}");
    }

    #[test]
    fn a_screen_reader_hears_labelled_lines() {
        let look = Look {
            screen_reader: true,
            ascii: true,
            ..Look::PLAIN
        };
        let (out, _) = render(look, &report());
        let rows: Vec<&str> = out.lines().filter(|l| l.contains("T00")).collect();
        assert_eq!(rows.len(), 8, "{out}");
        assert!(rows.iter().all(|l| l.starts_with("task: ")), "{out}");
        assert!(out.lines().any(|l| l.starts_with("phase: ")));
        assert!(out.lines().any(|l| l.starts_with("next: ")), "{out}");
        let (_, err) = render(look, &failure());
        let lines: Vec<&str> = err.lines().collect();
        assert!(lines[0].starts_with("error: NO-PROJECT - "), "{err}");
        assert!(
            lines.iter().skip(1).all(|l| ["why: ", "fix: ", "docs: "]
                .iter()
                .any(|p| l.starts_with(p))),
            "{err}"
        );
    }

    #[test]
    fn progress_lines_for_a_screen_reader_start_with_a_label() {
        use speccoder_core::stage::{Stage, StageState};
        use speccoder_runtime::progress::Event;
        let events = [
            Event::TaskStarted {
                feature: "f".into(),
                task: "T1".into(),
                title: "t".into(),
            },
            Event::Stage {
                task: "T1".into(),
                stage: Stage::Attempt,
                state: StageState::Started,
                detail: Some("1/2".into()),
                elapsed_ms: None,
                tail: Vec::new(),
            },
            Event::ToolCall {
                task: "T1".into(),
                turn: 1,
                tool: "run_command".into(),
                target: "cargo test".into(),
                decision: "allow".into(),
                ok: Some(true),
                tail: Vec::new(),
            },
            Event::ApprovalRequested {
                task: "T1".into(),
                call: "c".into(),
                tool: "run_command".into(),
                what: "cargo add x".into(),
                class: String::new(),
            },
            Event::TaskFinished {
                task: "T1".into(),
                outcome: "verified".into(),
                message: "m".into(),
                run: None,
                input_tokens: 1,
                output_tokens: 2,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                usd: None,
            },
            Event::Notice { text: "n".into() },
        ];
        let labels = [
            "task: ",
            "stage: ",
            "tool: ",
            "approval required: ",
            "usage: ",
            "notice: ",
        ];
        for e in &events {
            let lines = crate::progress::reader_lines(e, crate::progress::Verbosity::Normal);
            assert!(!lines.is_empty(), "{e:?}");
            for l in lines {
                assert!(labels.iter().any(|p| l.starts_with(p)), "{l}");
                // Screen-reader mode writes ASCII (the printer translates).
                assert!(style::asciify(&l).is_ascii(), "{l}");
            }
        }
    }
}
