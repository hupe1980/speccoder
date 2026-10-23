//! `speccoder tui`: a live dashboard beside a running `speccoder run`.
//!
//! Three panes, refreshed every second from the same sources as `report`,
//! `status` and `approvals`: the feature's tasks with their evidence, the
//! live run (phase, recent tool calls, tokens), and the calls waiting for a
//! person — approve with `a`, reject with `r` and a reason. The model is
//! pure (`App`, `draw`, `App::key`) so it is tested without a terminal.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use speccoder_core::{ApprovalsReport, CommandData, CommandResult, CommandStatus, StatusReport};
use speccoder_runtime::approvals::{Verb, decide_with};
use speccoder_runtime::progress::{LiveState, read_live};

/// What the dashboard shows, loaded fresh each second.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The feature's tasks (`report`).
    pub report: Option<StatusReport>,
    /// The live run and whether its process is alive (`status`).
    pub live: Option<(LiveState, bool)>,
    /// Calls waiting for a person (`approvals`).
    pub approvals: Option<ApprovalsReport>,
    /// Why something could not be loaded.
    pub problem: Option<String>,
}

/// Which pane the arrow keys move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The task list.
    Tasks,
    /// The approval queue.
    Approvals,
}

/// What a key asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do outside the model.
    None,
    /// Leave.
    Quit,
    /// Approve the pending call of this task.
    Approve(String),
    /// Reject it with a reason.
    Reject(String, String),
}

/// The dashboard's state.
#[derive(Debug, Clone)]
pub struct App {
    /// The data.
    pub snap: Snapshot,
    /// Where the arrows go.
    pub focus: Focus,
    /// Selected task row.
    pub task: usize,
    /// Selected approval row.
    pub approval: usize,
    /// A rejection reason being typed, for the selected approval.
    pub reason: Option<String>,
    /// The last action's outcome.
    pub flash: Option<String>,
}

impl App {
    /// A dashboard on `snap`.
    #[must_use]
    pub fn new(snap: Snapshot) -> Self {
        Self {
            snap,
            focus: Focus::Tasks,
            task: 0,
            approval: 0,
            reason: None,
            flash: None,
        }
    }

    fn pending_task(&self) -> Option<String> {
        self.snap
            .approvals
            .as_ref()
            .and_then(|a| a.pending.get(self.approval))
            .map(|p| p.task.clone())
    }

    /// Handles one key.
    pub fn key(&mut self, code: KeyCode) -> Action {
        // Typing a rejection reason.
        if let Some(reason) = self.reason.as_mut() {
            match code {
                KeyCode::Esc => self.reason = None,
                KeyCode::Backspace => {
                    reason.pop();
                }
                KeyCode::Enter => {
                    let reason = self.reason.take().unwrap_or_default();
                    let reason = reason.trim().to_owned();
                    return match (self.pending_task(), reason.is_empty()) {
                        (Some(task), false) => Action::Reject(task, reason),
                        _ => {
                            self.flash = Some("a rejection needs a reason".to_owned());
                            Action::None
                        }
                    };
                }
                KeyCode::Char(c) => reason.push(c),
                _ => {}
            }
            return Action::None;
        }
        let rows = |n: usize| n.saturating_sub(1);
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Tasks => Focus::Approvals,
                    Focus::Approvals => Focus::Tasks,
                };
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.focus {
                    Focus::Tasks => {
                        let n = self.snap.report.as_ref().map_or(0, |r| r.tasks.len());
                        self.task = (self.task + 1).min(rows(n));
                    }
                    Focus::Approvals => {
                        let n = self.snap.approvals.as_ref().map_or(0, |a| a.pending.len());
                        self.approval = (self.approval + 1).min(rows(n));
                    }
                }
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.focus {
                    Focus::Tasks => self.task = self.task.saturating_sub(1),
                    Focus::Approvals => self.approval = self.approval.saturating_sub(1),
                }
                Action::None
            }
            KeyCode::Char('a') if self.focus == Focus::Approvals => {
                self.pending_task().map_or(Action::None, Action::Approve)
            }
            KeyCode::Char('r') if self.focus == Focus::Approvals => {
                if self.pending_task().is_some() {
                    self.reason = Some(String::new());
                }
                Action::None
            }
            _ => Action::None,
        }
    }
}

fn state_style(state: &str) -> (char, Style) {
    match state {
        "verified" => ('✓', Style::default().fg(Color::Green)),
        "regression_only" => ('~', Style::default().fg(Color::Yellow)),
        "attested" => ('A', Style::default().fg(Color::Cyan)),
        "ticked_without_evidence" => ('!', Style::default().fg(Color::Magenta)),
        "skipped" => ('–', Style::default().fg(Color::DarkGray)),
        "ready" => ('·', Style::default()),
        _ => ('⧗', Style::default().fg(Color::DarkGray)),
    }
}

fn focused(block: Block<'_>, on: bool) -> Block<'_> {
    if on {
        block.border_style(Style::default().fg(Color::Cyan))
    } else {
        block
    }
}

/// Draws the dashboard.
pub fn draw(f: &mut Frame<'_>, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(f.area());
    let feature = app.snap.report.as_ref().map_or("—", |r| r.feature.as_str());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" speccoder ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "{feature}   [tab] pane  [↑↓] select  [a] approve  [r] reject  [q] quit"
            )),
        ])),
        rows[0],
    );
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(rows[1]);
    tasks(f, app, cols[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(cols[1]);
    live(f, app, right[0]);
    approvals(f, app, right[1]);
    let footer = match (&app.reason, &app.flash, &app.snap.problem) {
        (Some(r), _, _) => format!(" reject — reason (the model is told; Enter to send, Esc to cancel): {r}▏"),
        (None, Some(m), _) => format!(" {m}"),
        (None, None, Some(p)) => format!(" {p}"),
        _ => app
            .snap
            .report
            .as_ref()
            .map(|r| {
                let t = &r.totals;
                format!(
                    " {} verified · {} regression only · {} attested · {} ready · {} blocked · {} skipped",
                    t.verified, t.regression_only, t.attested, t.ready, t.blocked, t.skipped
                )
            })
            .unwrap_or_default(),
    };
    f.render_widget(Paragraph::new(footer), rows[2]);
}

fn tasks(f: &mut Frame<'_>, app: &App, area: Rect) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(6)])
        .split(area);
    let Some(report) = &app.snap.report else {
        f.render_widget(
            Paragraph::new("no tasks")
                .block(Block::default().borders(Borders::ALL).title(" Tasks ")),
            area,
        );
        return;
    };
    let items: Vec<ListItem<'_>> = report
        .tasks
        .iter()
        .map(|t| {
            let (mark, style) = state_style(&t.state);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} {} ", t.id), style),
                Span::raw(t.title.clone()),
            ]))
        })
        .collect();
    let mut state = ListState::default()
        .with_selected(Some(app.task.min(report.tasks.len().saturating_sub(1))));
    f.render_stateful_widget(
        List::new(items)
            .block(focused(
                Block::default().borders(Borders::ALL).title(" Tasks "),
                app.focus == Focus::Tasks,
            ))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        parts[0],
        &mut state,
    );
    let detail = report.tasks.get(app.task).map_or_else(String::new, |t| {
        let mut d = format!("{} — {}", t.id, t.state.replace('_', " "));
        if !t.blocked_by.is_empty() {
            d.push_str(&format!(" by {}", t.blocked_by.join(", ")));
        }
        if let Some(n) = &t.note {
            d.push_str(&format!("\nreason: {n}"));
        }
        if let Some(a) = &t.last_attempt {
            d.push_str(&format!("\nlast attempt: {} — {}", a.outcome, a.reason));
        }
        if let Some(c) = &t.commit {
            d.push_str(&format!("\ncommit {}", c.get(..10).unwrap_or(c)));
        }
        d
    });
    f.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Task ")),
        parts[1],
    );
}

fn live(f: &mut Frame<'_>, app: &App, area: Rect) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    match &app.snap.live {
        None => lines.push(Line::raw("no run recorded yet")),
        Some((l, running)) => {
            let head = match (&l.finished, running) {
                (None, true) => format!("running  {} {} — {}", l.feature, l.task, l.phase),
                (None, false) => {
                    format!("interrupted  {} {} during {}", l.feature, l.task, l.phase)
                }
                (Some(o), _) => format!("last run  {} {} — {o}", l.feature, l.task),
            };
            lines.push(Line::styled(
                head,
                Style::default().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(format!(
                "{} in · {} out{}",
                l.input_tokens,
                l.output_tokens,
                l.usd.map(|u| format!(" · ${u:.4}")).unwrap_or_default()
            )));
            lines.push(Line::raw(""));
            for c in &l.recent {
                lines.push(Line::raw(c.clone()));
            }
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Live ")),
        area,
    );
}

fn approvals(f: &mut Frame<'_>, app: &App, area: Rect) {
    let pending = app
        .snap
        .approvals
        .as_ref()
        .map(|a| a.pending.as_slice())
        .unwrap_or_default();
    let mut items: Vec<ListItem<'_>> = pending
        .iter()
        .map(|p| ListItem::new(format!("? {} {} — {}", p.task, p.covers, p.reason)))
        .collect();
    if items.is_empty() {
        items.push(ListItem::new("nothing waits for a decision"));
    }
    let decided = app.snap.approvals.as_ref().map_or(0, |a| a.decided.len());
    let mut state = ListState::default().with_selected(
        (!pending.is_empty()).then_some(app.approval.min(pending.len().saturating_sub(1))),
    );
    f.render_stateful_widget(
        List::new(items)
            .block(focused(
                Block::default().borders(Borders::ALL).title(format!(
                    " Approvals ({} waiting, {decided} decided) ",
                    pending.len()
                )),
                app.focus == Focus::Approvals,
            ))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

/// Loads a fresh snapshot.
#[must_use]
pub fn load(path: &Path, feature: Option<&str>, state: &Path) -> Snapshot {
    let report_result = speccoder_runtime::status::report(path, feature);
    let report = match report_result.data() {
        Some(CommandData::Report(r)) => Some((**r).clone()),
        _ => None,
    };
    let approvals = match decide_with(path, feature, Verb::List, state).data() {
        Some(CommandData::Approvals(a)) => Some((**a).clone()),
        _ => None,
    };
    Snapshot {
        problem: report.is_none().then(|| report_result.message().to_owned()),
        report,
        live: read_live(path, state),
        approvals,
    }
}

/// `speccoder tui [PATH] [--feature]`.
pub fn run(path: &Path, feature: Option<&str>) -> CommandResult {
    let Some(state) = speccoder_runtime::worktree::state_dir() else {
        return CommandResult::new(
            "tui",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    let path: PathBuf = path.to_path_buf();
    let mut app = App::new(load(&path, feature, &state));
    if app.snap.report.is_none() {
        return CommandResult::new(
            "tui",
            CommandStatus::NotAProject,
            app.snap.problem.as_deref().unwrap_or("no feature"),
        );
    }
    let mut terminal = ratatui::init();
    let mut loaded = Instant::now();
    let outcome = loop {
        if let Err(e) = terminal.draw(|f| draw(f, &app)) {
            break Err(e.to_string());
        }
        match event::poll(Duration::from_millis(250)) {
            Ok(true) => {
                if let Ok(TermEvent::Key(k)) = event::read() {
                    if k.kind != KeyEventKind::Press {
                        continue;
                    }
                    let verb = match app.key(k.code) {
                        Action::Quit => break Ok(()),
                        Action::None => None,
                        Action::Approve(task) => Some((task, None)),
                        Action::Reject(task, reason) => Some((task, Some(reason))),
                    };
                    if let Some((task, reason)) = verb {
                        let r = match &reason {
                            Some(why) => {
                                decide_with(&path, feature, Verb::Reject(&task, why), &state)
                            }
                            None => decide_with(&path, feature, Verb::Approve(&task), &state),
                        };
                        app.flash = Some(r.message().to_owned());
                        app.snap = load(&path, feature, &state);
                        loaded = Instant::now();
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(e.to_string()),
        }
        if loaded.elapsed() >= Duration::from_secs(1) {
            app.snap = load(&path, feature, &state);
            loaded = Instant::now();
        }
    };
    ratatui::restore();
    match outcome {
        Ok(()) => CommandResult::new("tui", CommandStatus::Ok, ""),
        Err(e) => CommandResult::new("tui", CommandStatus::Error, &format!("terminal: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use speccoder_core::{PendingCall, StatusTotals, TaskStatus};

    fn task(id: &str, state: &str) -> TaskStatus {
        TaskStatus {
            id: id.to_owned(),
            title: format!("title of {id}"),
            phase: "Phase 2".to_owned(),
            state: state.to_owned(),
            blocked_by: Vec::new(),
            commit: None,
            run_id: None,
            last_attempt: None,
            note: None,
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            report: Some(StatusReport {
                feature: "001-x".to_owned(),
                branch: Some("speccoder/001-x".to_owned()),
                source: "branch".to_owned(),
                unmerged_commits: 1,
                tasks: vec![
                    task("T001", "verified"),
                    task("T002", "ready"),
                    task("T003", "blocked"),
                ],
                convergence: Vec::new(),
                totals: StatusTotals {
                    verified: 1,
                    ready: 1,
                    blocked: 1,
                    ..StatusTotals::default()
                },
            }),
            live: Some((
                LiveState {
                    feature: "001-x".to_owned(),
                    task: "T002".to_owned(),
                    phase: "attempt 1".to_owned(),
                    recent: vec!["turn 1  read_file src/lib.rs (allow)".to_owned()],
                    input_tokens: 1200,
                    ..LiveState::default()
                },
                true,
            )),
            approvals: Some(ApprovalsReport {
                feature: "001-x".to_owned(),
                pending: vec![PendingCall {
                    task: "T002".to_owned(),
                    covers: "write `package.json`".to_owned(),
                    tool: "write_file".to_owned(),
                    arguments: serde_json::json!({}),
                    reason: "a dependency manifest".to_owned(),
                    reference: "refs/x".to_owned(),
                }],
                decided: Vec::new(),
            }),
            problem: None,
        }
    }

    fn screen(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(140, 30)).unwrap_or_else(|e| panic!("{e}"));
        t.draw(|f| draw(f, app)).unwrap_or_else(|e| panic!("{e}"));
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_dashboard_shows_tasks_the_live_run_and_the_queue() {
        let s = screen(&App::new(snapshot()));
        for needle in [
            "✓ T001",
            "· T002",
            "⧗ T003",
            "running  001-x T002 — attempt 1",
            "turn 1  read_file src/lib.rs (allow)",
            "Approvals (1 waiting, 0 decided)",
            "? T002 write `package.json` — a dependency manifest",
            "1 verified",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
    }

    #[test]
    fn keys_select_approve_and_reject_with_a_reason() {
        let mut app = App::new(snapshot());
        assert_eq!(app.key(KeyCode::Down), Action::None);
        assert_eq!(app.task, 1);
        // `a` only acts in the approval pane.
        assert_eq!(app.key(KeyCode::Char('a')), Action::None);
        app.key(KeyCode::Tab);
        assert_eq!(
            app.key(KeyCode::Char('a')),
            Action::Approve("T002".to_owned())
        );
        // Reject: type a reason, Enter.
        app.key(KeyCode::Char('r'));
        for c in "no deps".chars() {
            app.key(KeyCode::Char(c));
        }
        assert!(screen(&app).contains("reason (the model is told"));
        assert_eq!(
            app.key(KeyCode::Enter),
            Action::Reject("T002".to_owned(), "no deps".to_owned())
        );
        // An empty reason is refused; Esc cancels typing; q quits.
        app.key(KeyCode::Char('r'));
        assert_eq!(app.key(KeyCode::Enter), Action::None);
        assert!(
            app.flash
                .as_deref()
                .unwrap_or_default()
                .contains("needs a reason")
        );
        app.key(KeyCode::Char('r'));
        app.key(KeyCode::Esc);
        assert!(app.reason.is_none());
        assert_eq!(app.key(KeyCode::Char('q')), Action::Quit);
    }
}
