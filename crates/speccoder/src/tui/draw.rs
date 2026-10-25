//! Drawing the dashboard (F-058): the Run, Evidence and Report screens and
//! the approval, diff, help and quit overlays. Styles come only from the
//! person's look (`style::tui_style`), marks from its glyph table, and every
//! text passes through it, so ASCII mode is ASCII throughout.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use speccoder_core::stage::{Stage, StageState};

use super::{App, Ask, Focus, Overlay, Screen, Typing};
use crate::style::{self, Glyph, Look, Token};

/// The smallest terminal the dashboard draws in.
pub const MIN: (u16, u16) = (60, 15);

fn look() -> Look {
    style::out()
}

/// `text` as the look wants it: ASCII symbols when asked.
fn a(text: &str) -> String {
    if look().ascii {
        style::asciify(text)
    } else {
        text.to_owned()
    }
}

fn st(token: Token) -> Style {
    style::tui_style(&look(), token)
}

fn mark(g: Glyph) -> &'static str {
    look().mark(g)
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// A box, with ASCII borders when the look asks for them.
fn bordered<'a>(title: &str, focused: bool) -> Block<'a> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(a(title))
        .border_style(st(if focused {
            Token::BorderFocus
        } else {
            Token::Border
        }));
    if look().ascii {
        block.border_set(ratatui::symbols::border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        })
    } else {
        block
    }
}

/// `↑↓` or `up/down`.
fn arrows() -> &'static str {
    if look().ascii { "up/down" } else { "↑↓" }
}

/// Draws the dashboard.
pub fn draw(f: &mut Frame<'_>, app: &App) {
    let area = f.area();
    if area.width < MIN.0 || area.height < MIN.1 {
        f.render_widget(
            Paragraph::new(format!(
                "make the terminal larger: at least {}x{} (now {}x{})",
                MIN.0, MIN.1, area.width, area.height
            ))
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);
    header(f, app, rows[0]);
    match app.screen {
        Screen::Run => run_screen(f, app, rows[1]),
        Screen::Evidence => evidence_screen(f, app, rows[1]),
        Screen::Report => report_screen(f, app, rows[1]),
    }
    footer(f, app, rows[2]);
    match app.overlay {
        Overlay::None => {}
        Overlay::Help => help(f, app, rows[1]),
        Overlay::Ask => ask(f, app, rows[1]),
        Overlay::Diff => diff(f, app, rows[1]),
        Overlay::Quit => quit(f, rows[1]),
        Overlay::Features => features(f, app, rows[1]),
    }
}

fn header(f: &mut Frame<'_>, app: &App, area: Rect) {
    let feature = app.snap.report.as_ref().map_or("-", |r| r.feature.as_str());
    let mut spans = vec![
        Span::styled(" speccoder ", bold()),
        Span::raw(format!("{feature}   ")),
    ];
    if let Some(m) = &app.snap.in_place {
        spans.push(Span::styled(a(&format!("{m}   ")), bold()));
    }
    for (n, screen, name) in [
        ('1', Screen::Run, "Run"),
        ('2', Screen::Evidence, "Evidence"),
        ('3', Screen::Report, "Report"),
    ] {
        let s = if app.screen == screen {
            st(Token::Accent).add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        spans.push(Span::styled(format!(" {n} {name} "), s));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        "  f feature  ? keys  q quit",
        st(Token::Muted),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn footer(f: &mut Frame<'_>, app: &App, area: Rect) {
    let waiting = app.hosted.as_ref().is_some_and(|h| h.ask.is_some());
    let line = match (&app.typing, &app.flash) {
        (Some((kind, text)), _) => {
            let what = match kind {
                Typing::Note => "note for the model (enter keeps it, esc drops it)",
                Typing::Reason | Typing::QueueReason => {
                    "reason (the model is told; enter sends, esc cancels)"
                }
                Typing::Answer => "answer (enter sends, empty skips, esc later)",
            };
            Line::from(vec![
                Span::styled(format!(" {what}: "), st(Token::Attention)),
                Span::raw(format!("{text}_")),
            ])
        }
        (None, _) if waiting && app.overlay != Overlay::Ask => Line::styled(
            a(&format!(
                " {} a call waits for you — enter to answer",
                mark(Glyph::Attention)
            )),
            st(Token::Attention),
        ),
        (None, Some(m)) => Line::raw(a(&format!(" {m}"))),
        (None, None) => match (&app.snap.problem, &app.snap.report) {
            (Some(p), _) => Line::styled(a(&format!(" {p}")), st(Token::Danger)),
            (None, Some(r)) => {
                let next = app
                    .snap
                    .next
                    .as_ref()
                    .map(|n| format!(" Next: {} — {}   |", n.command, n.why))
                    .unwrap_or_default();
                Line::raw(a(&format!("{next} {}", totals(&r.totals))))
            }
            _ => Line::raw(""),
        },
    };
    f.render_widget(Paragraph::new(line), area);
}

fn totals(t: &speccoder_core::StatusTotals) -> String {
    format!(
        "{} verified · {} regression only · {} attested · {} ready · {} blocked · {} skipped",
        t.verified, t.regression_only, t.attested, t.ready, t.blocked, t.skipped
    )
}

/// A task's mark: running when the hosted or watched run is on it.
fn task_glyph(app: &App, t: &speccoder_core::TaskStatus) -> Glyph {
    let running = match (&app.hosted, &app.snap.live) {
        (Some(h), _) => h.done.is_none() && h.task.as_ref().is_some_and(|(id, _)| *id == t.id),
        (None, Some((l, true))) => l.finished.is_none() && l.task == t.id,
        _ => false,
    };
    if running
        && !matches!(
            t.state.as_str(),
            "verified" | "regression_only" | "attested"
        )
    {
        Glyph::Running
    } else {
        Glyph::of_state(&t.state)
    }
}

fn task_list(f: &mut Frame<'_>, app: &App, area: Rect, focused: bool) {
    let Some(report) = &app.snap.report else {
        f.render_widget(
            Paragraph::new("no tasks").block(bordered(" Tasks ", focused)),
            area,
        );
        return;
    };
    let items: Vec<ListItem<'_>> = report
        .tasks
        .iter()
        .map(|t| {
            let g = task_glyph(app, t);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} {} ", mark(g), t.id), st(g.token())),
                Span::raw(a(&t.title)),
            ]))
        })
        .collect();
    let mut state = ListState::default()
        .with_selected(Some(app.task.min(report.tasks.len().saturating_sub(1))));
    f.render_stateful_widget(
        List::new(items)
            .block(bordered(
                &format!(" Tasks ({}) ", report.tasks.len()),
                focused,
            ))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

fn run_screen(f: &mut Frame<'_>, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(6)])
        .split(cols[0]);
    task_list(f, app, left[0], app.focus == Focus::Tasks);
    task_detail(f, app, left[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Percentage(35),
        ])
        .split(cols[1]);
    pipeline(f, app, right[0]);
    calls(f, app, right[1]);
    queue(f, app, right[2]);
}

fn task_detail(f: &mut Frame<'_>, app: &App, area: Rect) {
    let detail = app.selected().map_or_else(String::new, |t| {
        let mut d = format!("{} — {}", t.id, t.state.replace('_', " "));
        if !t.blocked_by.is_empty() {
            d.push_str(&format!(" by {}", t.blocked_by.join(", ")));
        }
        if let Some(n) = &t.note {
            d.push_str(&format!("\nreason: {n}"));
        }
        if let Some(l) = &t.last_attempt {
            d.push_str(&format!("\nlast attempt: {} — {}", l.outcome, l.reason));
        }
        if let Some(c) = &t.commit {
            d.push_str(&format!(
                "\ncommit {}   enter evidence · d diff",
                c.get(..10).unwrap_or(c)
            ));
        }
        d
    });
    f.render_widget(
        Paragraph::new(a(&detail))
            .wrap(Wrap { trim: true })
            .block(bordered(" Task ", false)),
        area,
    );
}

fn stage_glyph(state: Option<StageState>) -> Glyph {
    match state {
        Some(StageState::Started) => Glyph::Running,
        Some(StageState::Passed) => Glyph::Verified,
        Some(StageState::Failed) => Glyph::Failed,
        Some(StageState::Skipped) => Glyph::Skipped,
        None => Glyph::Ready,
    }
}

fn seconds(ms: u64) -> String {
    if ms < 10_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}s", ms / 1000)
    }
}

fn pipeline(f: &mut Frame<'_>, app: &App, area: Rect) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    match (&app.hosted, &app.snap.live) {
        (Some(h), _) => {
            let head = match (&h.task, &h.done) {
                (_, Some(r)) => format!("run ended — {}", r.message()),
                (Some((id, title)), None) => {
                    let stop = if h.stopping {
                        "   (stopping after it)"
                    } else {
                        ""
                    };
                    format!("{id} {title}{stop}")
                }
                (None, None) => "starting".to_owned(),
            };
            lines.push(Line::styled(a(&head), bold()));
            let mut spans = Vec::new();
            for stage in Stage::ALL {
                let row = h.stages.iter().find(|s| s.stage == stage);
                let g = stage_glyph(row.map(|r| r.state));
                spans.push(Span::styled(
                    format!("{} {}", mark(g), stage.name()),
                    st(g.token()),
                ));
                spans.push(Span::raw("  "));
            }
            lines.push(Line::from(spans));
            let times: Vec<String> = h
                .stages
                .iter()
                .map(|s| match (s.state, s.elapsed_ms, &s.detail) {
                    (StageState::Started, _, Some(d)) => format!("{} {d}…", s.stage.name()),
                    (StageState::Started, _, None) => format!("{}…", s.stage.name()),
                    (_, Some(ms), _) => format!("{} {}", s.stage.name(), seconds(ms)),
                    (_, None, Some(d)) => format!("{} {d}", s.stage.name()),
                    (_, None, None) => s.stage.name().to_owned(),
                })
                .collect();
            lines.push(Line::styled(a(&times.join(" · ")), st(Token::Muted)));
        }
        (None, None) => lines.push(Line::raw("no run recorded yet")),
        (None, Some((l, running))) => {
            let head = match (&l.finished, running) {
                (None, true) => format!(
                    "running  {} {} — {}",
                    l.feature,
                    l.task,
                    speccoder_runtime::progress::stage_word(l)
                ),
                (None, false) => format!(
                    "interrupted  {} {} during {}",
                    l.feature,
                    l.task,
                    speccoder_runtime::progress::stage_word(l)
                ),
                (Some(o), _) => format!("last run  {} {} — {o}", l.feature, l.task),
            };
            lines.push(Line::styled(a(&head), bold()));
            let mut spans = Vec::new();
            let at = l
                .stage
                .and_then(|s| Stage::ALL.iter().position(|x| *x == s));
            for (i, stage) in Stage::ALL.iter().enumerate() {
                // Watching from outside, only the current stage is known.
                let g = match at {
                    Some(n) if i == n && l.finished.is_none() && *running => Glyph::Running,
                    _ => Glyph::Ready,
                };
                spans.push(Span::styled(
                    format!("{} {}", mark(g), stage.name()),
                    st(g.token()),
                ));
                spans.push(Span::raw("  "));
            }
            lines.push(Line::from(spans));
        }
    }
    f.render_widget(
        Paragraph::new(lines).block(bordered(" Pipeline ", false)),
        area,
    );
}

fn calls(f: &mut Frame<'_>, app: &App, area: Rect) {
    let (recent, usage): (&[String], (u64, u64, Option<f64>)) = match (&app.hosted, &app.snap.live)
    {
        (Some(h), _) => (&h.recent, h.usage),
        (None, Some((l, _))) => (&l.recent, (l.input_tokens, l.output_tokens, l.usd)),
        _ => (&[], (0, 0, None)),
    };
    let mut lines: Vec<Line<'_>> = vec![Line::styled(
        a(&format!(
            "{} in · {} out{}",
            usage.0,
            usage.1,
            usage.2.map(|u| format!(" · ${u:.4}")).unwrap_or_default()
        )),
        st(Token::Muted),
    )];
    // The newest at the bottom, as many as fit.
    let room = usize::from(area.height.saturating_sub(3));
    for c in recent.iter().skip(recent.len().saturating_sub(room)) {
        let token = if c.contains("(deny)") || c.contains(" failed") {
            Token::Danger
        } else if c.contains("(approval)") || c.contains("(stop)") {
            Token::Attention
        } else {
            Token::Ready
        };
        lines.push(Line::styled(a(c), st(token)));
    }
    f.render_widget(
        Paragraph::new(lines).block(bordered(" Tool calls ", false)),
        area,
    );
}

fn queue(f: &mut Frame<'_>, app: &App, area: Rect) {
    let pending = app
        .snap
        .approvals
        .as_ref()
        .map(|q| q.pending.as_slice())
        .unwrap_or_default();
    let mut items: Vec<ListItem<'_>> = pending
        .iter()
        .map(|p| {
            ListItem::new(a(&format!(
                "{} {} {} — {}",
                mark(Glyph::Attention),
                p.task,
                p.covers,
                p.reason
            )))
        })
        .collect();
    if let Some(Ask::Call(p)) = app.hosted.as_ref().and_then(|h| h.ask.as_ref()) {
        items.insert(
            0,
            ListItem::new(Line::styled(
                a(&format!(
                    "{} {} {} — now, enter to answer",
                    mark(Glyph::Attention),
                    p.request.task,
                    p.subject
                )),
                st(Token::Attention),
            )),
        );
    }
    if items.is_empty() {
        items.push(ListItem::new("nothing waits for a decision"));
    }
    // The selected call in detail, as the inline prompt shows it (F-054).
    if let Some(v) = pending
        .get(app.approval.min(pending.len().saturating_sub(1)))
        .and_then(|p| p.presentation.as_ref())
    {
        items.push(ListItem::new(""));
        items.push(ListItem::new(a(&format!(
            "  {}  {}   risk {}",
            v.title,
            v.subject,
            crate::prompt::risk_bar(&v.risk)
        ))));
        for line in v.body_lines.iter().take(6) {
            items.push(ListItem::new(a(&format!("    {line}"))));
        }
        if v.body_lines.len() > 6 {
            items.push(ListItem::new(a(&format!(
                "    … {} more lines (speccoder report --approvals)",
                v.body_lines.len() - 6
            ))));
        }
        items.push(ListItem::new(a(&format!("  Sandbox  {}", v.sandbox))));
        items.push(ListItem::new(a(&format!("  a → {}   r reject", v.scope))));
    }
    let decided = app.snap.approvals.as_ref().map_or(0, |q| q.decided.len());
    let mut state = ListState::default().with_selected(
        (!pending.is_empty() && app.focus == Focus::Approvals)
            .then_some(app.approval.min(pending.len().saturating_sub(1))),
    );
    f.render_stateful_widget(
        List::new(items)
            .block(bordered(
                &format!(" Approvals ({} waiting, {decided} decided) ", pending.len()),
                app.focus == Focus::Approvals,
            ))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

fn evidence_screen(f: &mut Frame<'_>, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);
    task_list(f, app, cols[0], false);
    let Some(t) = app.selected() else {
        f.render_widget(
            Paragraph::new("no task selected").block(bordered(" Evidence ", true)),
            cols[1],
        );
        return;
    };
    let (g, token, words) = super::evidence::banner(&t.state);
    let mut lines: Vec<Line<'_>> = vec![
        Line::styled(
            a(&format!(" {} {words} ", mark(g))),
            st(token).add_modifier(Modifier::BOLD | Modifier::REVERSED),
        ),
        Line::styled(a(&format!("{} {}", t.id, t.title)), bold()),
        Line::raw(""),
    ];
    if !t.blocked_by.is_empty() {
        lines.push(Line::raw(a(&format!(
            "  {} {:<15} {}",
            mark(Glyph::Blocked),
            "Waits for",
            t.blocked_by.join(", ")
        ))));
    }
    if let Some(n) = &t.note {
        lines.push(Line::raw(a(&format!(
            "  {} {:<15} {n}",
            mark(Glyph::Attested),
            "Note"
        ))));
    }
    let rows = app
        .evidence
        .as_ref()
        .map(|(_, rows)| rows.as_slice())
        .unwrap_or_default();
    for r in rows {
        // The model's words are a claim: muted, never styled as evidence.
        let text_style = if r.label == "Model says" {
            st(Token::Muted)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", mark(r.glyph)), st(r.glyph.token())),
            Span::styled(format!("{:<15} ", r.label), bold()),
            Span::styled(a(&r.text), text_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        a(&format!(
            "{} task · d diff · PgUp/PgDn scroll · esc back   attest verify checks the signatures",
            arrows()
        )),
        st(Token::Muted),
    ));
    let scroll = u16::try_from(app.scroll).unwrap_or(u16::MAX);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(bordered(" Evidence ", true)),
        cols[1],
    );
}

/// `[#####~A..]`: one cell per task, by state.
#[must_use]
pub fn verdict_bar(tasks: &[speccoder_core::TaskStatus], ascii: bool) -> String {
    let cells: String = tasks
        .iter()
        .map(|t| match (t.state.as_str(), ascii) {
            ("verified", true) => '#',
            ("verified", false) => '█',
            ("regression_only", _) => '~',
            ("attested", _) => 'A',
            ("ticked_without_evidence" | "evidence_without_tick", _) => '!',
            ("skipped", true) => '-',
            ("skipped", false) => '–',
            (_, true) => '.',
            (_, false) => '·',
        })
        .collect();
    format!("[{cells}]")
}

fn report_screen(f: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(r) = &app.snap.report else {
        f.render_widget(
            Paragraph::new(a(app.snap.problem.as_deref().unwrap_or("no report")))
                .block(bordered(" Report ", true)),
            area,
        );
        return;
    };
    let mut lines: Vec<Line<'_>> = vec![
        Line::from(vec![
            Span::styled(format!("{}  ", r.feature), bold()),
            Span::styled(verdict_bar(&r.tasks, look().ascii), st(Token::Verified)),
        ]),
        Line::raw(a(&totals(&r.totals))),
        Line::raw(a(&format!(
            "branch {} · {} commit(s) not merged · tasks read from the {}",
            r.branch.as_deref().unwrap_or("-"),
            r.unmerged_commits,
            r.source
        ))),
        Line::raw(""),
    ];
    let pending = app
        .snap
        .approvals
        .as_ref()
        .map(|q| q.pending.as_slice())
        .unwrap_or_default();
    lines.push(Line::styled(
        format!("Waiting for a person ({})", pending.len()),
        bold(),
    ));
    if pending.is_empty() {
        lines.push(Line::styled("  nothing", st(Token::Muted)));
    }
    for p in pending {
        lines.push(Line::styled(
            a(&format!(
                "  {} {} {} — {}",
                mark(Glyph::Attention),
                p.task,
                p.covers,
                p.reason
            )),
            st(Token::Attention),
        ));
    }
    let uncited: Vec<&str> = r
        .trace
        .iter()
        .filter(|t| t.task.is_none())
        .map(|t| t.requirement.as_str())
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("Requirements no task cites ({})", uncited.len()),
        bold(),
    ));
    lines.push(if uncited.is_empty() {
        Line::styled("  none", st(Token::Muted))
    } else {
        Line::styled(format!("  {}", uncited.join(", ")), st(Token::Regression))
    });
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("Convergence rounds ({})", r.convergence.len()),
        bold(),
    ));
    if r.convergence.is_empty() {
        lines.push(Line::styled("  none", st(Token::Muted)));
    }
    for c in &r.convergence {
        lines.push(Line::raw(a(&format!(
            "  round {} · {} · {} task(s): {}",
            c.round,
            c.commit.get(..10).unwrap_or(&c.commit),
            c.tasks.len(),
            c.tasks.join("; ")
        ))));
    }
    if let Some(n) = &app.snap.next {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("Next: ", bold()),
            Span::styled(n.command.clone(), st(Token::Accent)),
            Span::raw(a(&format!(" — {}", n.why))),
        ]));
    }
    let scroll = u16::try_from(app.scroll).unwrap_or(u16::MAX);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(bordered(" Report ", true)),
        area,
    );
}

/// A rectangle `pw`% × `ph`% of `area`, centred.
fn centred(area: Rect, pw: u16, ph: u16) -> Rect {
    let w = area.width.saturating_mul(pw) / 100;
    let h = area.height.saturating_mul(ph) / 100;
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// The keys, per screen.
#[must_use]
pub fn keys(screen: Screen) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![
        ("1 2 3", "Run, Evidence, Report"),
        ("?", "these keys"),
        ("esc", "back (an overlay, then the screen); never quits"),
        ("q", "quit (asks while a run is working); ctrl-c too"),
        (
            "enter",
            "answer a waiting call; else the selected task's evidence",
        ),
        (
            "f",
            "pick the feature to show (made current for every tool)",
        ),
    ];
    match screen {
        Screen::Run => out.extend([
            ("up/down j k", "select a task (or a waiting call)"),
            ("tab", "tasks or the approval queue"),
            (
                "a / r",
                "approve / reject the selected waiting call (r asks why)",
            ),
            ("d", "the selected task's diff"),
        ]),
        Screen::Evidence => out.extend([
            ("up/down j k", "the previous or next task"),
            ("PgUp PgDn", "scroll"),
            ("d", "the task's diff"),
        ]),
        Screen::Report => out.push(("PgUp PgDn", "scroll")),
    }
    out.extend([
        ("approval: y t r", "yes once, for the task, for this run"),
        ("approval: n", "no, and tell the model why"),
        (
            "approval: s",
            "stop the task here; the call waits for `task approve`",
        ),
        ("approval: 1-5", "the same choices by number"),
        ("approval: tab", "add a note for the model"),
        ("approval: v", "view the whole body"),
        ("approval: esc", "later: the call keeps waiting"),
        ("diff: ] [", "next / previous hunk"),
        ("diff: n p", "next / previous file"),
    ]);
    out
}

fn help(f: &mut Frame<'_>, app: &App, area: Rect) {
    let box_ = centred(area, 80, 90);
    f.render_widget(Clear, box_);
    let lines: Vec<Line<'_>> = keys(app.screen)
        .into_iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("  {k:<16}"), st(Token::Accent)),
                Span::raw(a(what)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(bordered(" Keys (esc closes) ", true)),
        box_,
    );
}

fn features(f: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(r) = &app.features else {
        return;
    };
    let box_ = centred(area, 80, 70);
    f.render_widget(Clear, box_);
    let width = r
        .features
        .iter()
        .map(|e| e.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut items: Vec<ListItem<'_>> = r
        .features
        .iter()
        .map(|e| {
            let filled = (e.done * 10).checked_div(e.tasks).unwrap_or(0);
            let bar = a(&format!(
                "{}{}",
                "■".repeat(filled),
                "□".repeat(10 - filled)
            ));
            let progress = if e.tasks == 0 {
                "no tasks yet".to_owned()
            } else {
                format!("{}/{} tasks", e.done, e.tasks)
            };
            let mark = match (e.current, look().ascii) {
                (true, true) => ">",
                (true, false) => "▸",
                (false, _) => " ",
            };
            let done = e.tasks > 0 && e.done == e.tasks;
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{mark} {:>2}  {:<width$}  ", e.number, e.name),
                    if e.current { bold() } else { Style::default() },
                ),
                Span::styled(bar, st(if done { Token::Verified } else { Token::Accent })),
                Span::raw(format!("  {progress:<12} ")),
                Span::styled(a(&e.artifacts.join(" · ")), st(Token::Muted)),
            ]))
        })
        .collect();
    if items.is_empty() {
        items.push(ListItem::new(format!("no {}s yet", r.unit)));
    }
    let mut state = ListState::default().with_selected(Some(app.pick));
    let title = format!(
        " {} · {} · {} move · enter show · esc back ",
        if r.unit == "change" {
            "Changes"
        } else {
            "Features"
        },
        a(&r.reason_text),
        arrows()
    );
    f.render_stateful_widget(
        List::new(items)
            .block(bordered(&title, true))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        box_,
        &mut state,
    );
}

fn quit(f: &mut Frame<'_>, area: Rect) {
    let box_ = centred(area, 70, 50);
    f.render_widget(Clear, box_);
    let lines = vec![
        Line::styled("The run is still working.", bold()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  f  ", st(Token::Accent)),
            Span::raw("finish this task, then stop"),
        ]),
        Line::from(vec![
            Span::styled("  s  ", st(Token::Accent)),
            Span::raw(a(
                "stop now — the attempt is kept; the next `speccoder run` resumes it",
            )),
        ]),
        Line::from(vec![
            Span::styled("  esc", st(Token::Accent)),
            Span::raw("  keep watching"),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(bordered(" Quit? ", true)),
        box_,
    );
}

fn body_line(line: &str) -> Line<'static> {
    let token = if line.starts_with('+') && !line.starts_with("+++") {
        Some(Token::DiffAdd)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Some(Token::DiffDel)
    } else if line.starts_with("@@") {
        Some(Token::Accent)
    } else {
        None
    };
    match token {
        Some(t) => Line::styled(a(line), st(t)),
        None if line.starts_with("diff --git") => Line::styled(a(line), bold()),
        None => Line::raw(a(line)),
    }
}

fn ask(f: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(ask) = app.hosted.as_ref().and_then(|h| h.ask.as_ref()) else {
        return;
    };
    let box_ = centred(area, 92, 94);
    f.render_widget(Clear, box_);
    let p = match ask {
        Ask::Question(q) => {
            let lines = vec![
                Line::styled(a(q), bold()),
                Line::raw(""),
                Line::styled(
                    "type the answer below; enter sends, an empty answer skips",
                    st(Token::Muted),
                ),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(bordered(" speccoder asks ", true)),
                box_,
            );
            return;
        }
        Ask::Call(p) => p,
    };
    let r = &p.request;
    let mut lines: Vec<Line<'_>> = vec![
        Line::styled(
            a(&format!(
                "{} Approval needed — {}{}{}",
                mark(Glyph::Attention),
                r.task,
                if r.task_title.is_empty() {
                    String::new()
                } else {
                    format!(" {}", r.task_title)
                },
                if r.turn > 0 {
                    format!(" · turn {}", r.turn)
                } else {
                    String::new()
                }
            )),
            st(Token::Attention).add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled(a(&format!("{}  {}", p.title, p.subject)), bold()),
            Span::raw(a(&format!("   risk {}", crate::prompt::risk_bar(&r.risk)))),
        ]),
    ];
    let body = if app.view_all { &p.full } else { &p.body };
    // Room for the body: the box less the fixed lines around it.
    let fixed = 9 + p.options.len() + usize::from(r.claim.is_some());
    let room = usize::from(box_.height.saturating_sub(2))
        .saturating_sub(fixed)
        .max(1);
    let top = app.scroll.min(body.len().saturating_sub(1));
    for line in body.iter().skip(top).take(room) {
        lines.push(body_line(line));
    }
    let hidden = if app.view_all {
        body.len().saturating_sub(top + room)
    } else {
        p.full.len().saturating_sub(p.body.len().min(top + room))
    };
    lines.push(Line::styled(
        if hidden > 0 {
            a(&format!(
                "… {hidden} more line(s) — {} scroll{}",
                arrows(),
                if app.view_all { "" } else { ", v view all" }
            ))
        } else {
            String::new()
        },
        st(Token::Muted),
    ));
    lines.push(Line::raw(a(&format!("Why stopped  {}", r.reason))));
    lines.push(Line::raw(a(&format!("Sandbox      {}", p.sandbox))));
    if let Some(claim) = &r.claim {
        lines.push(Line::from(vec![
            Span::raw("Model says   "),
            Span::styled(a(&format!("\"{claim}\"   (a claim)")), st(Token::Muted)),
        ]));
    }
    lines.push(Line::raw(""));
    for o in &p.options {
        lines.push(Line::raw(a(&format!(
            "  {}. {}   ({})",
            o.number, o.label, o.key
        ))));
    }
    if let Some(n) = &app.note {
        lines.push(Line::styled(
            a(&format!("  note for the model: {n}")),
            st(Token::Accent),
        ));
    }
    let keys: Vec<String> = p.options.iter().map(|o| o.key.to_string()).collect();
    lines.push(Line::styled(
        a(&format!(
            "  {} or 1-5 · tab note · v view all · esc later",
            keys.join(" ")
        )),
        st(Token::Muted),
    ));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(bordered(" Approval ", true)),
        box_,
    );
}

fn diff(f: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(d) = &app.diff else {
        return;
    };
    f.render_widget(Clear, area);
    let room = usize::from(area.height.saturating_sub(2));
    let lines: Vec<Line<'_>> = d
        .lines
        .iter()
        .skip(d.top)
        .take(room)
        .map(|l| body_line(l))
        .collect();
    let title = format!(
        " Diff {} · ] [ hunk · n p file · esc back ",
        d.current_file().unwrap_or("")
    );
    f.render_widget(Paragraph::new(lines).block(bordered(&title, true)), area);
}
