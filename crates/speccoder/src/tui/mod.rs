//! The dashboard: one terminal app with three screens — Run,
//! Evidence, Report — and overlays for an approval, a diff, help and
//! quitting.
//!
//! It works two ways. Hosting (`speccoder run` at a terminal), the run
//! works on a thread of its own and its events and questions come here
//! ([`host`]); observing (`speccoder tui`, bare `speccoder`), it reads what
//! another process's run writes, as `report --live` does. Either way the
//! data is refreshed every second from the same sources as `report` and
//! `report --approvals`. The model is pure (`App`, `App::key`, `App::apply`,
//! [`draw::draw`]) so it is tested without a terminal.

pub mod diff;
pub mod draw;
pub mod evidence;
pub mod host;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Sender, TryRecvError};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind, KeyModifiers};
use speccoder_core::actions::Runnable;
use speccoder_core::stage::{Stage, StageState};
use speccoder_core::{ApprovalsReport, CommandData, CommandResult, CommandStatus, StatusReport};
use speccoder_runtime::approvals::{Answer, Presentation, Who};
use speccoder_runtime::progress::{Event, LiveState, RECENT, read_live};

use crate::prompt::Parsed;

/// What the dashboard shows, loaded fresh each second.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The feature's tasks (`report`).
    pub report: Option<StatusReport>,
    /// The live run and whether its process is alive (`report --live`).
    pub live: Option<(LiveState, bool)>,
    /// Calls waiting for a person (`report --approvals`).
    pub approvals: Option<ApprovalsReport>,
    /// Why something could not be loaded.
    pub problem: Option<String>,
    /// What to do next: a call waiting first, else the report's step.
    pub next: Option<speccoder_core::next::NextStep>,
    /// An in-place session: `in-place · <branch> (returns to <original>)`.
    pub in_place: Option<String>,
    /// The step `n` takes (at the terminal, after leaving).
    pub primary: Option<speccoder_core::actions::Action>,
}

/// A screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Tasks, the running task's pipeline, tool calls, what waits.
    Run,
    /// The selected task's verdict and what established it.
    Evidence,
    /// The feature at a glance.
    Report,
}

/// What is drawn over the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// Nothing.
    None,
    /// The keys.
    Help,
    /// A call or a question waiting for the person.
    Ask,
    /// A task commit's diff.
    Diff,
    /// Stop the run, or keep watching?
    Quit,
    /// Which feature to show (`f`).
    Features,
}

/// Which list the arrow keys move in on the Run screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The task list.
    Tasks,
    /// The approval queue.
    Approvals,
}

/// Text being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// A note for the model with the next approval (`tab`).
    Note,
    /// A rejection's reason (`n`): the model is told.
    Reason,
    /// The answer to a hosted skill's question.
    Answer,
    /// A rejection's reason for a call in the queue (`r`).
    QueueReason,
}

/// What waits for the person in a hosted run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// A call the policy sent to a person.
    Call(Box<Presentation>),
    /// A free-text question.
    Question(String),
}

/// One stage of the running task's pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow {
    /// Which.
    pub stage: Stage,
    /// Where it is.
    pub state: StageState,
    /// `1/2`, a short commit id, a reason.
    pub detail: Option<String>,
    /// How long it took, once it ended.
    pub elapsed_ms: Option<u64>,
}

/// A run the dashboard hosts.
#[derive(Debug, Clone, Default)]
pub struct Hosted {
    /// The running task: ID and title.
    pub task: Option<(String, String)>,
    /// Its pipeline so far.
    pub stages: Vec<StageRow>,
    /// The latest tool calls, oldest first.
    pub recent: Vec<String>,
    /// Tokens in and out, and money, of the current attempt.
    pub usage: (u64, u64, Option<f64>),
    /// Tasks finished in this run: ID and outcome.
    pub finished: Vec<(String, String)>,
    /// What waits for the person.
    pub ask: Option<Ask>,
    /// The person asked to stop after the current task.
    pub stopping: bool,
    /// The run's result, once it ended.
    pub done: Option<CommandResult>,
}

/// What a key asks the loop to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing outside the model.
    None,
    /// Leave.
    Quit,
    /// Approve the waiting call of this task (the queue).
    Approve(String),
    /// Reject it with a reason (the queue).
    Reject(String, String),
    /// Approve every call of a run's batch once (`B` on a batched call).
    Batch(Runnable),
    /// Answer the hosted run's waiting call.
    Answer(Answer),
    /// Answer its question (`None`: skipped).
    Reply(Option<String>),
    /// Show this commit's diff.
    Diff(String),
    /// Stop the run before its next task.
    StopAfterTask,
    /// Stop the run now (it resumes on the next `run`).
    StopNow,
    /// List the features for the picker.
    Features,
    /// Make this feature current and show it.
    Switch(String),
    /// Leave, and take the next step at the terminal.
    Handoff,
}

/// The dashboard's state.
#[derive(Debug, Clone)]
pub struct App {
    /// The data.
    pub snap: Snapshot,
    /// The screen.
    pub screen: Screen,
    /// Over it.
    pub overlay: Overlay,
    /// Where the arrows go on the Run screen.
    pub focus: Focus,
    /// Selected task row.
    pub task: usize,
    /// Selected approval row.
    pub approval: usize,
    /// Text being typed.
    pub typing: Option<(Typing, String)>,
    /// The note for the next approval.
    pub note: Option<String>,
    /// The approval shows its whole body (`v`).
    pub view_all: bool,
    /// The approval body's or the evidence's first line shown.
    pub scroll: usize,
    /// The last action's outcome, or a hint.
    pub flash: Option<String>,
    /// The hosted run, when there is one.
    pub hosted: Option<Hosted>,
    /// Who answers here.
    pub who: Option<Who>,
    /// The selected task's evidence: for which task and commit, and rows.
    pub evidence: Option<(String, Vec<evidence::Row>)>,
    /// The open diff.
    pub diff: Option<diff::Diff>,
    /// The features, for the picker.
    pub features: Option<speccoder_core::FeaturesReport>,
    /// The picker's selected row.
    pub pick: usize,
}

impl App {
    /// A dashboard on `snap`.
    #[must_use]
    pub fn new(snap: Snapshot) -> Self {
        Self {
            snap,
            screen: Screen::Run,
            overlay: Overlay::None,
            focus: Focus::Tasks,
            task: 0,
            approval: 0,
            typing: None,
            note: None,
            view_all: false,
            scroll: 0,
            flash: None,
            hosted: None,
            who: None,
            evidence: None,
            diff: None,
            features: None,
            pick: 0,
        }
    }

    /// A dashboard hosting a run.
    #[must_use]
    pub fn hosting(snap: Snapshot, who: Who) -> Self {
        Self {
            hosted: Some(Hosted::default()),
            who: Some(who),
            ..Self::new(snap)
        }
    }

    /// A hosted run is still working.
    #[must_use]
    pub fn active(&self) -> bool {
        self.hosted.as_ref().is_some_and(|h| h.done.is_none())
    }

    /// The selected task.
    #[must_use]
    pub fn selected(&self) -> Option<&speccoder_core::TaskStatus> {
        self.snap
            .report
            .as_ref()
            .and_then(|r| r.tasks.get(self.task))
    }

    fn pending_task(&self) -> Option<String> {
        self.snap
            .approvals
            .as_ref()
            .and_then(|a| a.pending.get(self.approval))
            .map(|p| p.task.clone())
    }

    /// The batch decision for the selected call's run, if it waits in one.
    fn pending_batch(&self) -> Option<Runnable> {
        let a = self.snap.approvals.as_ref()?;
        let run = a.pending.get(self.approval)?.run.clone()?;
        let b = a.batch.iter().find(|b| b.run == run)?;
        Some(Runnable::DecideBatch {
            run,
            verdict: speccoder_core::actions::BatchVerdict::Approve,
            calls: b.groups.iter().flat_map(|g| g.calls.clone()).collect(),
            digest: b.digest.clone(),
            resume_asking: false,
        })
    }

    fn who(&self) -> Option<Who> {
        self.who.clone()
    }

    /// A call or a question waits for the person in the hosted run.
    pub fn ask(&mut self, ask: Ask) {
        if let Some(h) = self.hosted.as_mut() {
            h.ask = Some(ask.clone());
        }
        // It takes precedence over help and the diff.
        self.overlay = Overlay::Ask;
        self.view_all = false;
        self.scroll = 0;
        self.typing = matches!(ask, Ask::Question(_)).then(|| (Typing::Answer, String::new()));
    }

    fn answered(&mut self) {
        if let Some(h) = self.hosted.as_mut() {
            h.ask = None;
        }
        self.overlay = Overlay::None;
        self.typing = None;
        self.note = None;
        self.view_all = false;
        self.scroll = 0;
    }

    /// A hosted run's progress event.
    pub fn apply(&mut self, event: &Event) {
        let Some(h) = self.hosted.as_mut() else {
            return;
        };
        match event {
            Event::TaskStarted { task, title, .. } => {
                h.task = Some((task.clone(), title.clone()));
                h.stages.clear();
                h.usage = (0, 0, None);
                // The task list follows the running task.
                if let Some(i) = self
                    .snap
                    .report
                    .as_ref()
                    .and_then(|r| r.tasks.iter().position(|t| t.id == *task))
                {
                    self.task = i;
                }
            }
            Event::Stage {
                stage,
                state,
                detail,
                elapsed_ms,
                ..
            } => {
                let row = StageRow {
                    stage: *stage,
                    state: *state,
                    detail: detail.clone(),
                    elapsed_ms: *elapsed_ms,
                };
                match h.stages.iter_mut().find(|s| s.stage == *stage) {
                    Some(s) => *s = row,
                    None => h.stages.push(row),
                }
            }
            Event::Said { turn, text, .. } => {
                if let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) {
                    let cut: String = line.chars().take(120).collect();
                    h.recent.push(format!("turn {turn}  “{cut}”"));
                    if h.recent.len() > RECENT {
                        h.recent.remove(0);
                    }
                }
            }
            // The reasoning's conclusion, marked as its own.
            Event::Reasoning { turn, text, .. } => {
                if let Some(line) = text.lines().map(str::trim).rev().find(|l| !l.is_empty()) {
                    let cut: String = line.chars().take(120).collect();
                    h.recent.push(format!("turn {turn}  ∴ {cut}"));
                    if h.recent.len() > RECENT {
                        h.recent.remove(0);
                    }
                }
            }
            Event::ToolCall {
                turn,
                tool,
                target,
                decision,
                ok,
                ..
            } => {
                let mark = match ok {
                    Some(false) => " failed",
                    _ => "",
                };
                h.recent
                    .push(format!("turn {turn}  {tool} {target} ({decision}){mark}"));
                if h.recent.len() > RECENT {
                    h.recent.remove(0);
                }
            }
            Event::Usage {
                input_tokens,
                output_tokens,
                usd,
                ..
            } => h.usage = (*input_tokens, *output_tokens, *usd),
            Event::TaskFinished { task, outcome, .. } => {
                h.finished.push((task.clone(), outcome.clone()));
            }
            Event::ApprovalDecided { decision, by, .. } => {
                h.recent.push(format!("  {decision} by {by}"));
            }
            Event::Notice { text } => h.recent.push(text.clone()),
            _ => {}
        }
    }

    /// The hosted run ended.
    pub fn finish(&mut self, result: CommandResult) {
        self.flash = Some(format!("run ended: {} — q to leave", result.message()));
        if let Some(h) = self.hosted.as_mut() {
            h.done = Some(result);
            h.ask = None;
        }
        if self.overlay == Overlay::Ask || self.overlay == Overlay::Quit {
            self.overlay = Overlay::None;
            self.typing = None;
        }
    }

    /// Handles one key.
    pub fn key(&mut self, code: KeyCode) -> Action {
        if self.typing.is_some() {
            return self.type_key(code);
        }
        match self.overlay {
            Overlay::Ask => return self.ask_key(code),
            Overlay::Quit => return self.quit_key(code),
            Overlay::Help => {
                if matches!(code, KeyCode::Esc | KeyCode::Char('?' | 'q')) {
                    self.overlay = Overlay::None;
                }
                return Action::None;
            }
            Overlay::Diff => return self.diff_key(code),
            Overlay::Features => return self.features_key(code),
            Overlay::None => {}
        }
        self.screen_key(code)
    }

    fn type_key(&mut self, code: KeyCode) -> Action {
        let Some((kind, text)) = self.typing.as_mut() else {
            return Action::None;
        };
        let kind = *kind;
        match code {
            KeyCode::Char(c) => {
                text.push(c);
                return Action::None;
            }
            KeyCode::Backspace => {
                text.pop();
                return Action::None;
            }
            KeyCode::Esc => {
                self.typing = None;
                // Escaping a question's answer is "later", like the call's.
                if kind == Typing::Answer {
                    self.later();
                }
                return Action::None;
            }
            KeyCode::Enter => {}
            _ => return Action::None,
        }
        let text = self
            .typing
            .take()
            .map(|(_, t)| t.trim().to_owned())
            .unwrap_or_default();
        match kind {
            Typing::Note => {
                self.note = (!text.is_empty()).then_some(text);
                Action::None
            }
            Typing::Reason if text.is_empty() => {
                self.flash = Some("a rejection needs a reason — the model is told".to_owned());
                self.typing = Some((Typing::Reason, String::new()));
                Action::None
            }
            Typing::Reason => {
                let a = Answer::reject(text, self.who());
                self.answered();
                Action::Answer(a)
            }
            Typing::Answer => {
                self.answered();
                Action::Reply((!text.is_empty()).then_some(text))
            }
            Typing::QueueReason => match (self.pending_task(), text.is_empty()) {
                (Some(task), false) => Action::Reject(task, text),
                _ => {
                    self.flash = Some("a rejection needs a reason".to_owned());
                    Action::None
                }
            },
        }
    }

    /// `esc` on a waiting call: it keeps waiting, a banner offers it again.
    fn later(&mut self) {
        self.overlay = Overlay::None;
        self.flash = Some(
            "still waiting — enter to answer; s in the approval stops the task and keeps the call for `speccoder task approve`"
                .to_owned(),
        );
    }

    fn ask_key(&mut self, code: KeyCode) -> Action {
        let offered: Vec<String> = match self.hosted.as_ref().and_then(|h| h.ask.as_ref()) {
            Some(Ask::Call(p)) => p.request.options.clone(),
            Some(Ask::Question(_)) => {
                // Answering again after `esc`.
                self.typing = Some((Typing::Answer, String::new()));
                return Action::None;
            }
            None => {
                self.overlay = Overlay::None;
                return Action::None;
            }
        };
        match code {
            KeyCode::Esc => {
                self.later();
                Action::None
            }
            KeyCode::Tab => {
                self.typing = Some((Typing::Note, self.note.clone().unwrap_or_default()));
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10);
                Action::None
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Action::None
            }
            // The terminal prompt's answers, with the same meanings.
            KeyCode::Char(c) => match crate::prompt::parse(&c.to_string(), &offered) {
                Parsed::Approve { scope, .. } => {
                    let a = Answer::approve(scope, self.note.take(), self.who());
                    self.answered();
                    Action::Answer(a)
                }
                Parsed::Always => {
                    let a = Answer::always(self.who());
                    self.answered();
                    Action::Answer(a)
                }
                Parsed::Reject { .. } => {
                    self.typing = Some((Typing::Reason, String::new()));
                    Action::None
                }
                Parsed::Stop => {
                    let a = Answer::stop(self.who());
                    self.answered();
                    Action::Answer(a)
                }
                Parsed::View => {
                    self.view_all = !self.view_all;
                    self.scroll = 0;
                    Action::None
                }
                Parsed::Later => {
                    self.later();
                    Action::None
                }
                Parsed::NotOffered(why) => {
                    self.flash = Some(why.to_owned());
                    Action::None
                }
                Parsed::Unknown => Action::None,
            },
            _ => Action::None,
        }
    }

    fn quit_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('f') => {
                self.overlay = Overlay::None;
                if let Some(h) = self.hosted.as_mut() {
                    h.stopping = true;
                }
                self.flash = Some("stopping after this task".to_owned());
                Action::StopAfterTask
            }
            KeyCode::Char('s') => Action::StopNow,
            KeyCode::Esc | KeyCode::Char('k' | 'q') => {
                self.overlay = Overlay::None;
                Action::None
            }
            _ => Action::None,
        }
    }

    /// The picker opened with `features`, on the current one.
    pub fn show_features(&mut self, features: speccoder_core::FeaturesReport) {
        self.pick = features
            .features
            .iter()
            .position(|f| f.current)
            .unwrap_or(0);
        self.features = Some(features);
        self.overlay = Overlay::Features;
    }

    fn features_key(&mut self, code: KeyCode) -> Action {
        let n = self.features.as_ref().map_or(0, |f| f.features.len());
        match code {
            KeyCode::Esc | KeyCode::Char('q' | 'f') => self.overlay = Overlay::None,
            KeyCode::Down | KeyCode::Char('j') => {
                self.pick = (self.pick + 1).min(n.saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => self.pick = self.pick.saturating_sub(1),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let i = c.to_digit(10).map_or(0, |d| d as usize);
                if (1..=n).contains(&i) {
                    self.pick = i - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(f) = self
                    .features
                    .as_ref()
                    .and_then(|f| f.features.get(self.pick))
                {
                    let name = f.name.clone();
                    self.overlay = Overlay::None;
                    self.task = 0;
                    self.evidence = None;
                    return Action::Switch(name);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn diff_key(&mut self, code: KeyCode) -> Action {
        let Some(d) = self.diff.as_mut() else {
            self.overlay = Overlay::None;
            return Action::None;
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q' | 'd') => self.overlay = Overlay::None,
            KeyCode::Char(']') => d.next_hunk(),
            KeyCode::Char('[') => d.previous_hunk(),
            KeyCode::Char('n') => d.next_file(),
            KeyCode::Char('p') => d.previous_file(),
            KeyCode::Down | KeyCode::Char('j') => d.scroll(1),
            KeyCode::Up | KeyCode::Char('k') => d.scroll(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => d.scroll(20),
            KeyCode::PageUp => d.scroll(-20),
            KeyCode::Home | KeyCode::Char('g') => d.top = 0,
            _ => {}
        }
        Action::None
    }

    fn screen_key(&mut self, code: KeyCode) -> Action {
        let rows = |n: usize| n.saturating_sub(1);
        match code {
            KeyCode::Char('q') => {
                if self.active() {
                    self.overlay = Overlay::Quit;
                    Action::None
                } else {
                    Action::Quit
                }
            }
            // Back, never quit.
            KeyCode::Esc => {
                self.screen = Screen::Run;
                self.scroll = 0;
                Action::None
            }
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                Action::None
            }
            KeyCode::Char('n') => {
                if self.active() {
                    self.flash =
                        Some("the run is working; its next step comes when it ends".to_owned());
                    Action::None
                } else if self.snap.primary.is_some() {
                    Action::Handoff
                } else {
                    self.flash = Some("nothing to take next here".to_owned());
                    Action::None
                }
            }
            KeyCode::Char('f') => {
                if self.active() {
                    self.flash =
                        Some("the run works on this feature; switch once it has ended".to_owned());
                    Action::None
                } else {
                    Action::Features
                }
            }
            KeyCode::Char('1') => {
                self.screen = Screen::Run;
                Action::None
            }
            KeyCode::Char('2') => {
                self.screen = Screen::Evidence;
                self.scroll = 0;
                Action::None
            }
            KeyCode::Char('3') => {
                self.screen = Screen::Report;
                Action::None
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Tasks => Focus::Approvals,
                    Focus::Approvals => Focus::Tasks,
                };
                Action::None
            }
            KeyCode::Enter => {
                // A call waiting here comes first.
                if self.hosted.as_ref().is_some_and(|h| h.ask.is_some()) {
                    self.overlay = Overlay::Ask;
                    if matches!(
                        self.hosted.as_ref().and_then(|h| h.ask.as_ref()),
                        Some(Ask::Question(_))
                    ) {
                        self.typing = Some((Typing::Answer, String::new()));
                    }
                } else {
                    self.screen = Screen::Evidence;
                    self.scroll = 0;
                }
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match (self.screen, self.focus) {
                    (Screen::Run, Focus::Approvals) => {
                        let n = self.snap.approvals.as_ref().map_or(0, |a| a.pending.len());
                        self.approval = (self.approval + 1).min(rows(n));
                    }
                    _ => {
                        let n = self.snap.report.as_ref().map_or(0, |r| r.tasks.len());
                        self.task = (self.task + 1).min(rows(n));
                        self.scroll = 0;
                    }
                }
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match (self.screen, self.focus) {
                    (Screen::Run, Focus::Approvals) => {
                        self.approval = self.approval.saturating_sub(1);
                    }
                    _ => {
                        self.task = self.task.saturating_sub(1);
                        self.scroll = 0;
                    }
                }
                Action::None
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10);
                Action::None
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Action::None
            }
            KeyCode::Char('d') => match self.selected().and_then(|t| t.commit.clone()) {
                Some(c) => Action::Diff(c),
                None => {
                    self.flash = Some("no change to show: the task has no commit".to_owned());
                    Action::None
                }
            },
            KeyCode::Char('a') if self.focus == Focus::Approvals && self.screen == Screen::Run => {
                self.pending_task().map_or(Action::None, Action::Approve)
            }
            // A call waiting in its run's batch: the whole batch, each once.
            KeyCode::Char('B') if self.focus == Focus::Approvals && self.screen == Screen::Run => {
                match self.pending_batch() {
                    Some(run) => Action::Batch(run),
                    None => {
                        self.flash = Some("this call does not wait in a batch".to_owned());
                        Action::None
                    }
                }
            }
            KeyCode::Char('r') if self.focus == Focus::Approvals && self.screen == Screen::Run => {
                if self.pending_task().is_some() {
                    self.typing = Some((Typing::QueueReason, String::new()));
                }
                Action::None
            }
            _ => Action::None,
        }
    }
}

/// Set when the person pressed `n`: leave the dashboard and take the next
/// step at the terminal (the dashboard is a live view; steps are
/// taken by the CLI, and by the web UI later).
static HANDOFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the dashboard was left to take the next step.
#[must_use]
pub fn handed_off() -> bool {
    HANDOFF.load(Ordering::SeqCst)
}

/// Loads a fresh snapshot.
#[must_use]
pub fn load(path: &Path, feature: Option<&str>, state: &Path) -> Snapshot {
    let report_result = speccoder_runtime::status::report(path, feature);
    let report = match report_result.data() {
        Some(CommandData::Report(r)) => Some((**r).clone()),
        _ => None,
    };
    let approvals_result = speccoder_runtime::approvals::list_with(path, feature, state);
    let approvals = match approvals_result.data() {
        Some(CommandData::Approvals(a)) => Some((**a).clone()),
        _ => None,
    };
    // A call waiting for a person comes first; otherwise the report's step.
    let waiting = approvals.as_ref().is_some_and(|a| !a.pending.is_empty());
    let next = speccoder_runtime::next::attach(
        if waiting {
            approvals_result.clone()
        } else {
            report_result.clone()
        },
        feature,
    )
    .next()
    .first()
    .cloned();
    Snapshot {
        next,
        problem: report.is_none().then(|| report_result.message().to_owned()),
        report,
        live: read_live(path, state),
        approvals,
        primary: None,
        in_place: speccoder_runtime::in_place::session_at(path, state).map(|s| {
            format!(
                "in-place · {} (returns to {})",
                s.branch,
                s.original.describe()
            )
        }),
    }
}

/// The step `n` takes. Only while watching, and from the waiting calls the
/// snapshot already listed: the journal is not opened again.
fn with_primary(mut snap: Snapshot, path: &Path, feature: Option<&str>) -> Snapshot {
    snap.primary = speccoder_core::actions::primary(&speccoder_runtime::actions::available_with(
        path,
        feature,
        None,
        // Not listed (the journal was busy): none, rather than opening it again.
        Some(
            &snap
                .approvals
                .clone()
                .unwrap_or(speccoder_core::ApprovalsReport {
                    feature: String::new(),
                    pending: Vec::new(),
                    decided: Vec::new(),
                    unreadable: Some("the journal was busy".to_owned()),
                    batch: Vec::new(),
                }),
        ),
    ))
    .filter(|a| a.input.is_none())
    .cloned();
    snap
}

/// Reads the selected task's evidence when it changed.
fn refresh_evidence(app: &mut App, repo: &Path) {
    let Some(t) = app.selected() else {
        app.evidence = None;
        return;
    };
    let key = format!("{}@{}", t.id, t.commit.as_deref().unwrap_or_default());
    if app.evidence.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    let statements = t
        .commit
        .as_deref()
        .map(|c| evidence::statements(repo, c))
        .unwrap_or_default();
    let rows = evidence::rows(t, &statements, None);
    app.evidence = Some((key, rows));
}

fn no_state(command: &str) -> CommandResult {
    CommandResult::new(
        command,
        CommandStatus::ConfigError,
        "no state directory (set SPECCODER_STATE_DIR or HOME)",
    )
    .with_code("NO-STATE-DIR")
}

/// A decision or a switch, taken as every interface takes one.
fn take(path: &Path, feature: Option<&str>, run: &Runnable, input: Option<&str>) -> CommandResult {
    use speccoder_runtime::actions::{Dispatch, Interface, perform_for};
    let mut d = Dispatch::new(run, speccoder_core::actions::Via::Terminal);
    d.input = input;
    perform_for(Interface::Dashboard, path, feature, d)
        .unwrap_or_else(|| Interface::Dashboard.refusal())
}

/// What the loop does with an action that is not the model's.
fn perform(app: &mut App, action: Action, path: &Path, chosen: &mut Option<String>, state: &Path) {
    let feature = chosen.clone();
    let feature = feature.as_deref();
    match action {
        Action::Features => {
            let r = speccoder_runtime::current::list(path, feature);
            match r.data() {
                Some(CommandData::Features(f)) => app.show_features((**f).clone()),
                _ => app.flash = Some(r.message().to_owned()),
            }
        }
        Action::Switch(name) => {
            let r = take(path, feature, &Runnable::Feature, Some(&name));
            app.flash = Some(r.message().to_owned());
            if r.status() == CommandStatus::Ok {
                *chosen = Some(name);
                app.snap = load(path, chosen.as_deref(), state);
            }
        }
        Action::Approve(task) => {
            let r = take(path, feature, &Runnable::Approve { task }, None);
            app.flash = Some(r.message().to_owned());
            app.snap = load(path, feature, state);
        }
        Action::Reject(task, why) => {
            let r = take(path, feature, &Runnable::Reject { task }, Some(&why));
            app.flash = Some(r.message().to_owned());
            app.snap = load(path, feature, state);
        }
        Action::Batch(run) => {
            let r = take(path, feature, &run, None);
            app.flash = Some(r.message().to_owned());
            app.snap = load(path, feature, state);
        }
        Action::Diff(commit) => match diff::Diff::of_commit(path, &commit) {
            Some(d) => {
                app.diff = Some(d);
                app.overlay = Overlay::Diff;
            }
            None => app.flash = Some(format!("could not read commit {commit}")),
        },
        _ => {}
    }
}

/// The key of a terminal event, as the model reads it: `ctrl-c` is `q`.
fn key_of(e: &TermEvent) -> Option<KeyCode> {
    let TermEvent::Key(k) = e else {
        return None;
    };
    if k.kind != KeyEventKind::Press {
        return None;
    }
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        return Some(KeyCode::Char('q'));
    }
    Some(k.code)
}

/// `speccoder tui`: watches another process's run.
pub fn run(path: &Path, feature: Option<&str>) -> CommandResult {
    let Some(state) = speccoder_runtime::worktree::state_dir() else {
        return no_state("tui");
    };
    let path: PathBuf = path.to_path_buf();
    let mut chosen: Option<String> = feature.map(str::to_owned);
    let mut app = App::new(with_primary(load(&path, feature, &state), &path, feature));
    // Several features and none current: open on the picker.
    let features = speccoder_runtime::current::list(&path, feature);
    let listed = match features.data() {
        Some(CommandData::Features(f)) if !f.features.is_empty() => Some((**f).clone()),
        _ => None,
    };
    match (&app.snap.report, listed) {
        (None, Some(f)) if f.current.is_none() => app.show_features(f),
        // Nothing to show: the report's own answer, with its code and fix
        // (no project, no feature yet, several and none chosen).
        (None, _) => {
            return speccoder_runtime::next::attach(
                speccoder_runtime::status::report(&path, feature),
                feature,
            );
        }
        _ => {}
    }
    let mut terminal = ratatui::init();
    let mut loaded = Instant::now();
    let outcome = loop {
        if app.screen == Screen::Evidence {
            refresh_evidence(&mut app, &path);
        }
        if let Err(e) = terminal.draw(|f| draw::draw(f, &app)) {
            break Err(e.to_string());
        }
        match event::poll(Duration::from_millis(250)) {
            Ok(true) => {
                if let Some(code) = event::read().ok().as_ref().and_then(key_of) {
                    match app.key(code) {
                        Action::Quit => break Ok(()),
                        Action::Handoff => {
                            HANDOFF.store(true, Ordering::SeqCst);
                            break Ok(());
                        }
                        a => perform(&mut app, a, &path, &mut chosen, &state),
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(e.to_string()),
        }
        if loaded.elapsed() >= Duration::from_secs(1) {
            app.snap = with_primary(
                load(&path, chosen.as_deref(), &state),
                &path,
                chosen.as_deref(),
            );
            loaded = Instant::now();
        }
    };
    ratatui::restore();
    match outcome {
        Ok(()) => CommandResult::new("tui", CommandStatus::Ok, ""),
        Err(e) => CommandResult::new("tui", CommandStatus::Error, &format!("terminal: {e}")),
    }
}

/// Drains the run's messages into the model; the waiting replies are kept.
fn drain(
    app: &mut App,
    rx: &std::sync::mpsc::Receiver<host::Message>,
    call: &mut Option<Sender<Answer>>,
    question: &mut Option<Sender<Option<String>>>,
) {
    loop {
        match rx.try_recv() {
            Ok(host::Message::Event(e)) => app.apply(&e),
            Ok(host::Message::Ask(p, tx)) => {
                *call = Some(tx);
                app.ask(Ask::Call(p));
            }
            Ok(host::Message::Question(q, tx)) => {
                *question = Some(tx);
                app.ask(Ask::Question(q));
            }
            Ok(host::Message::Done(r)) => app.finish(*r),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
        }
    }
}

/// `speccoder run` at a terminal: hosts `work` (the run) in the dashboard
/// and returns its result once the person leaves.
pub fn host_run(
    path: &Path,
    feature: Option<&str>,
    work: impl FnOnce(host::Hosting) -> CommandResult + Send + 'static,
) -> CommandResult {
    let Some(state) = speccoder_runtime::worktree::state_dir() else {
        return no_state("run");
    };
    let path: PathBuf = path.to_path_buf();
    let (rx, cancel) = host::start(&path, work);
    let mut chosen: Option<String> = feature.map(str::to_owned);
    let mut app = App::hosting(load(&path, feature, &state), host::person(&path));
    let mut call: Option<Sender<Answer>> = None;
    let mut question: Option<Sender<Option<String>>> = None;
    let mut terminal = ratatui::init();
    // ratatui's panic hook restores the terminal; only the dashboard's own
    // panic should. The run's is caught on its thread and shown as its result.
    let ui = std::thread::current().id();
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == ui {
            restore(info);
        }
    }));
    let mut loaded = Instant::now();
    let outcome = loop {
        drain(&mut app, &rx, &mut call, &mut question);
        if app.screen == Screen::Evidence {
            refresh_evidence(&mut app, &path);
        }
        if let Err(e) = terminal.draw(|f| draw::draw(f, &app)) {
            break Err(e.to_string());
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {
                if let Some(code) = event::read().ok().as_ref().and_then(key_of) {
                    match app.key(code) {
                        Action::Quit => break Ok(false),
                        Action::StopNow => break Ok(true),
                        Action::StopAfterTask => cancel.store(true, Ordering::SeqCst),
                        Action::Answer(a) => {
                            if let Some(tx) = call.take() {
                                let _ = tx.send(a);
                            }
                        }
                        Action::Reply(text) => {
                            if let Some(tx) = question.take() {
                                let _ = tx.send(text);
                            }
                        }
                        a => perform(&mut app, a, &path, &mut chosen, &state),
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
    let done = app.hosted.and_then(|h| h.done);
    match (outcome, done) {
        (Ok(_), Some(result)) => result,
        (Ok(_), None) => CommandResult::new(
            "run",
            CommandStatus::NeedsHuman,
            "stopped from the dashboard; the next `speccoder run` resumes without repeating finished calls",
        )
        .with_code("INTERRUPTED"),
        (Err(e), _) => CommandResult::new("run", CommandStatus::Error, &format!("terminal: {e}")),
    }
}

#[cfg(test)]
mod tests;
