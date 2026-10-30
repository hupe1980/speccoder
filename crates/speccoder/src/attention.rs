//! Getting a person's attention during a long run: a
//! notification when a call waits for them, a task fails, the feature ends or
//! nothing happens for a while, and the terminal title following the run.
//! Settings are the person's (`~/.config/speccoder/config.toml`), never the
//! project's; nothing here runs under `--json` or `acp`.

use std::io::Write as _;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use speccoder_runtime::progress::{Event, Progress};
use speccoder_runtime::user_config::{Method, Notify, Signal};

/// Where escape sequences go.
pub trait TermWriter: Send + std::fmt::Debug {
    /// Writes `bytes` as they are.
    fn write(&mut self, bytes: &[u8]);
}

/// The terminal: stderr.
#[derive(Debug, Default)]
pub struct Stderr;

impl TermWriter for Stderr {
    fn write(&mut self, bytes: &[u8]) {
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    }
}

/// How an escape-based notification is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    /// `ESC ] 9 ; message BEL`.
    Osc9,
    /// `ESC ] 777 ; notify ; speccoder ; message BEL`.
    Osc777,
    /// BEL.
    Bel,
}

/// The escape `auto` resolves to from the environment (`None`: a dumb
/// terminal, nothing at all).
#[must_use]
pub fn auto(env: &dyn Fn(&str) -> Option<String>) -> Option<Escape> {
    let term = env("TERM").unwrap_or_default();
    if term == "dumb" {
        return None;
    }
    let program = env("TERM_PROGRAM").unwrap_or_default();
    if matches!(
        program.as_str(),
        "iTerm.app" | "WezTerm" | "ghostty" | "WarpTerminal"
    ) || term == "xterm-kitty"
        || env("KITTY_WINDOW_ID").is_some()
    {
        Some(Escape::Osc9)
    } else if term.starts_with("foot") || env("KONSOLE_VERSION").is_some() {
        Some(Escape::Osc777)
    } else {
        Some(Escape::Bel)
    }
}

/// `text` without control characters, at most 200 characters.
#[must_use]
pub fn sanitise(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// The bytes of a notification; inside tmux, wrapped so tmux passes it on.
#[must_use]
pub fn notification(escape: Escape, message: &str, tmux: bool) -> Vec<u8> {
    let message = sanitise(message);
    let osc = match escape {
        Escape::Bel => return b"\x07".to_vec(),
        Escape::Osc9 => format!("\x1b]9;{message}\x07"),
        Escape::Osc777 => format!("\x1b]777;notify;speccoder;{message}\x07"),
    };
    wrap(&osc, tmux)
}

/// The bytes that set the terminal title (empty: the terminal's default).
#[must_use]
pub fn title(text: &str, tmux: bool) -> Vec<u8> {
    wrap(&format!("\x1b]2;{}\x07", sanitise(text)), tmux)
}

fn wrap(osc: &str, tmux: bool) -> Vec<u8> {
    if tmux {
        format!("\x1bPtmux;{}\x1b\\", osc.replace('\x1b', "\x1b\x1b")).into_bytes()
    } else {
        osc.as_bytes().to_vec()
    }
}

/// How urgent a title is: the most urgent of parallel lanes wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Urgency {
    Done,
    Running,
    Failed,
    Approval,
}

#[derive(Debug)]
struct State {
    feature: String,
    task: String,
    /// The task's title, as the notifications name it.
    task_title: String,
    stage: String,
    title: Option<(Urgency, String)>,
    last_event: Instant,
    waiting: bool,
    stalled: bool,
    titled: bool,
}

/// A task as a notification names it: `T004 Handle Unicode` (the title of
/// the task the run started), else its id.
fn named(task: &str, s: &State) -> String {
    if s.task == task && !s.task_title.is_empty() {
        format!("{task} {}", s.task_title)
    } else {
        task.to_owned()
    }
}

/// The attention layer: forwards every event to the printer, and notifies.
#[derive(Debug)]
pub struct Attention {
    inner: Arc<dyn Progress>,
    settings: Notify,
    /// The escape to write, when the terminal gets one.
    escape: Option<Escape>,
    /// Whether titles and escapes go to the terminal at all (stderr is one).
    terminal: bool,
    tmux: bool,
    writer: Mutex<Box<dyn TermWriter>>,
    state: Mutex<State>,
}

impl Attention {
    /// A layer over `inner` with `settings`, writing to `writer` when
    /// `terminal` (stderr is a terminal); `env` reads the environment.
    pub fn new(
        inner: Arc<dyn Progress>,
        settings: Notify,
        terminal: bool,
        writer: Box<dyn TermWriter>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Arc<Self> {
        let escape = match &settings.method {
            Method::Auto => auto(env),
            Method::Osc9 => Some(Escape::Osc9),
            Method::Osc777 => Some(Escape::Osc777),
            Method::Bel => Some(Escape::Bel),
            Method::Command(_) | Method::Off => None,
        };
        Arc::new(Self {
            inner,
            escape,
            terminal,
            tmux: env("TMUX").is_some(),
            settings,
            writer: Mutex::new(writer),
            state: Mutex::new(State {
                feature: String::new(),
                task: String::new(),
                task_title: String::new(),
                stage: String::new(),
                title: None,
                last_event: Instant::now(),
                waiting: false,
                stalled: false,
                titled: false,
            }),
        })
    }

    /// Starts the stall timer: a background tick that notifies once after
    /// `stall_after` without an event (not while a person is being waited
    /// for); it ends with the layer.
    pub fn watch_stalls(self: &Arc<Self>, tick: Duration) {
        let weak: Weak<Self> = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(tick);
                let Some(this) = weak.upgrade() else { return };
                this.check_stall(Instant::now());
            }
        });
    }

    fn check_stall(&self, now: Instant) {
        let message = {
            let Ok(mut s) = self.state.lock() else { return };
            if s.waiting
                || s.stalled
                || s.task.is_empty()
                || now.duration_since(s.last_event) < self.settings.stall_after
            {
                return;
            }
            s.stalled = true;
            format!(
                "speccoder {}: {} no progress for {} (stage {})",
                s.feature,
                s.task,
                minutes(self.settings.stall_after),
                if s.stage.is_empty() { "—" } else { &s.stage }
            )
        };
        self.signal(Signal::Stalled, &message);
    }

    /// The end of a feature run: `done — …` or `stopped — <next step>`.
    pub fn finish(&self, result: &speccoder_core::CommandResult) {
        let Some(speccoder_core::CommandData::Feature(f)) = result.data() else {
            return;
        };
        let verified = f
            .attempts
            .iter()
            .filter(|a| a.verdict == Some(speccoder_core::verify::Verdict::Verified))
            .count();
        let regression = f
            .attempts
            .iter()
            .filter(|a| a.verdict == Some(speccoder_core::verify::Verdict::RegressionOnly))
            .count();
        let message = if result.status() == speccoder_core::CommandStatus::Ok {
            format!(
                "speccoder {}: done — {verified} verified, {regression} regression only of {}",
                f.feature,
                f.attempts.len()
            )
        } else {
            format!(
                "speccoder {}: stopped — {}",
                f.feature,
                result
                    .next()
                    .first()
                    .map_or("see the report", |s| s.command.as_str())
            )
        };
        // Only a run that finished is done; one that stopped (no tasks yet,
        // a call waiting) says so.
        let ended = if result.status() == speccoder_core::CommandStatus::Ok {
            "speccoder ✓ done"
        } else {
            "speccoder ■ stopped"
        };
        self.set_title(Urgency::Done, ended.to_owned(), true);
        self.signal(Signal::FeatureDone, &message);
    }

    fn signal(&self, signal: Signal, message: &str) {
        if !self.settings.wants(signal) {
            return;
        }
        match &self.settings.method {
            Method::Command(argv) => command(argv, &sanitise(message)),
            _ => {
                if let (true, Some(e)) = (self.terminal, self.escape)
                    && let Ok(mut w) = self.writer.lock()
                {
                    w.write(&notification(e, message, self.tmux));
                }
            }
        }
    }

    /// Sets the title when nothing more urgent holds it (`force`: a new
    /// state of the same run replaces it whatever it was).
    fn set_title(&self, urgency: Urgency, text: String, force: bool) {
        if !self.terminal {
            return;
        }
        let Ok(mut s) = self.state.lock() else { return };
        if !force && s.title.as_ref().is_some_and(|(u, _)| *u > urgency) {
            return;
        }
        s.title = Some((urgency, text.clone()));
        s.titled = true;
        drop(s);
        if let Ok(mut w) = self.writer.lock() {
            w.write(&title(&text, self.tmux));
        }
    }
}

impl Drop for Attention {
    fn drop(&mut self) {
        let titled = self.state.lock().map(|s| s.titled).unwrap_or(false);
        if titled && let Ok(mut w) = self.writer.lock() {
            w.write(&title("", self.tmux));
        }
    }
}

impl Progress for Attention {
    fn event(&self, event: &Event) {
        self.inner.event(event);
        if let Ok(mut s) = self.state.lock() {
            s.last_event = Instant::now();
            s.stalled = false;
            if !matches!(event, Event::ApprovalRequested { .. }) {
                s.waiting = false;
            }
        }
        match event {
            Event::TaskStarted {
                feature,
                task,
                title,
            } => {
                if let Ok(mut s) = self.state.lock() {
                    feature.clone_into(&mut s.feature);
                    task.clone_into(&mut s.task);
                    title.clone_into(&mut s.task_title);
                    s.stage.clear();
                }
                self.set_title(Urgency::Running, format!("speccoder ▸ {task}"), true);
            }
            Event::Stage { stage, .. } => {
                if let Ok(mut s) = self.state.lock() {
                    stage.name().clone_into(&mut s.stage);
                }
            }
            Event::ApprovalRequested { task, what, .. } => {
                let named = self.state.lock().map(|mut s| {
                    s.waiting = true;
                    named(task, &s)
                });
                self.set_title(
                    Urgency::Approval,
                    format!("speccoder △ approval {task}"),
                    false,
                );
                self.signal(
                    Signal::Approval,
                    &format!("speccoder: {} — {what}?", named.unwrap_or_default()),
                );
            }
            Event::TaskFinished {
                task,
                outcome,
                message,
                ..
            } if matches!(
                outcome.as_str(),
                "failed"
                    | "error"
                    | "verification_failed"
                    | "suspected_gaming"
                    | "blocked"
                    | "budget_exhausted"
                    | "incomplete"
                    | "not_demonstrated"
            ) =>
            {
                let named = self
                    .state
                    .lock()
                    .map(|s| named(task, &s))
                    .unwrap_or_default();
                self.set_title(Urgency::Failed, format!("speccoder ✗ {task} failed"), true);
                let why = if matches!(outcome.as_str(), "failed" | "error" | "verification_failed")
                {
                    message.lines().next().unwrap_or_default().to_owned()
                } else {
                    outcome.replace('_', " ")
                };
                self.signal(
                    Signal::TaskFailed,
                    &format!("speccoder: {named} did not pass — {why}"),
                );
            }
            _ => {}
        }
    }
}

fn minutes(d: Duration) -> String {
    let s = d.as_secs();
    if s.is_multiple_of(3600) {
        format!("{}h", s / 3600)
    } else if s.is_multiple_of(60) {
        format!("{}m", s / 60)
    } else {
        format!("{s}s")
    }
}

/// Runs `argv` with `message` as its last argument: no shell, no stdio, at
/// most five seconds, on its own thread; a failure is one warning.
fn command(argv: &[String], message: &str) {
    let Some((program, args)) = argv.split_first() else {
        return;
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .arg(message)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let program = program.clone();
    std::thread::spawn(move || {
        let child = cmd.spawn();
        let Ok(mut child) = child else {
            let _ = writeln!(
                std::io::stderr(),
                "warning: the notification command `{program}` could not start"
            );
            return;
        };
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => {
                    let _ = child.kill();
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default)]
    struct Recording(Arc<Mutex<Vec<u8>>>);

    impl TermWriter for Recording {
        fn write(&mut self, bytes: &[u8]) {
            if let Ok(mut b) = self.0.lock() {
                b.extend_from_slice(bytes);
            }
        }
    }

    #[derive(Debug)]
    struct Silent;
    impl Progress for Silent {
        fn event(&self, _: &Event) {}
    }

    fn layer(settings: Notify, env: &[(&str, &str)]) -> (Arc<Attention>, Recording) {
        let rec = Recording::default();
        let env: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let get = move |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let a = Attention::new(
            Arc::new(Silent),
            settings,
            true,
            Box::new(rec.clone()),
            &get,
        );
        (a, rec)
    }

    fn bytes(r: &Recording) -> String {
        String::from_utf8_lossy(&r.0.lock().map(|b| b.clone()).unwrap_or_default()).into_owned()
    }

    fn started() -> Event {
        Event::TaskStarted {
            feature: "001-x".to_owned(),
            task: "T004".to_owned(),
            title: "Handle Unicode".to_owned(),
        }
    }

    fn needs() -> Event {
        Event::ApprovalRequested {
            task: "T004".to_owned(),
            call: "c1".to_owned(),
            tool: "run_command".to_owned(),
            class: String::new(),
            object: String::new(),
            what: "cargo add x".to_owned(),
        }
    }

    #[test]
    fn escapes_are_exact() {
        assert_eq!(notification(Escape::Osc9, "hi", false), b"\x1b]9;hi\x07");
        assert_eq!(
            notification(Escape::Osc777, "hi", false),
            b"\x1b]777;notify;speccoder;hi\x07"
        );
        assert_eq!(notification(Escape::Bel, "hi", false), b"\x07");
        assert_eq!(
            notification(Escape::Osc9, "hi", true),
            b"\x1bPtmux;\x1b\x1b]9;hi\x07\x1b\\"
        );
        // Model text cannot smuggle its own escapes.
        assert_eq!(
            notification(Escape::Osc9, "a\x1b]0;x\x07b", false),
            b"\x1b]9;a]0;xb\x07"
        );
    }

    #[test]
    fn auto_follows_the_terminal() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        assert_eq!(
            auto(&env(&[("TERM_PROGRAM", "ghostty")])),
            Some(Escape::Osc9)
        );
        assert_eq!(auto(&env(&[("TERM", "foot")])), Some(Escape::Osc777));
        assert_eq!(auto(&env(&[("TERM", "xterm-256color")])), Some(Escape::Bel));
        assert_eq!(auto(&env(&[("TERM", "dumb")])), None);
    }

    #[test]
    fn a_run_sets_titles_notifies_and_restores_the_title() {
        let (a, rec) = layer(Notify::default(), &[("TERM_PROGRAM", "iTerm.app")]);
        a.event(&started());
        a.event(&needs());
        drop(a);
        let out = bytes(&rec);
        assert!(out.contains("\x1b]2;speccoder ▸ T004\x07"), "{out:?}");
        assert!(out.contains("\x1b]2;speccoder △ approval T004\x07"));
        // The task by its title and the decision, never a branch.
        assert!(
            out.contains("\x1b]9;speccoder: T004 Handle Unicode — cargo add x?\x07"),
            "{out:?}"
        );
        assert!(out.ends_with("\x1b]2;\x07"), "{out:?}");
    }

    #[test]
    fn a_run_that_stopped_without_a_task_is_never_titled_done() {
        let (a, rec) = layer(
            Notify {
                method: Method::Osc9,
                ..Notify::default()
            },
            &[],
        );
        let stopped = speccoder_core::CommandResult::new(
            "run",
            speccoder_core::CommandStatus::TaskNotReady,
            "no tasks yet: 001-x has no tasks.md",
        )
        .with_data(speccoder_core::CommandData::Feature(Box::new(
            speccoder_core::FeatureReport {
                feature: "001-x".to_owned(),
                attempts: Vec::new(),
                stop: "no tasks yet: 001-x has no tasks.md".to_owned(),
                converge: Vec::new(),
                hooks: Vec::new(),
                parallel: Vec::new(),
                full_tier: None,
                mode: speccoder_core::oracle::RunMode::default(),
                autopilot_denied: Vec::new(),
            },
        )));
        a.finish(&stopped);
        let out = bytes(&rec);
        assert!(out.contains("speccoder ■ stopped"), "{out:?}");
        assert!(!out.contains("done"), "{out:?}");
    }

    #[test]
    fn only_the_signals_asked_for_notify() {
        let settings = Notify {
            on: vec![Signal::FeatureDone],
            method: Method::Bel,
            ..Notify::default()
        };
        let (a, rec) = layer(settings, &[]);
        a.event(&started());
        a.event(&needs());
        // Two titles (running, approval) and no BEL of its own.
        assert_eq!(bytes(&rec).matches('\x07').count(), 2, "{:?}", bytes(&rec));
    }

    #[test]
    fn a_stall_notifies_once_and_not_while_waiting() {
        let settings = Notify {
            on: vec![Signal::Approval, Signal::Stalled],
            method: Method::Osc9,
            stall_after: Duration::from_secs(60),
            ..Notify::default()
        };
        let (a, rec) = layer(settings, &[]);
        a.event(&started());
        let later = Instant::now() + Duration::from_secs(120);
        a.check_stall(later);
        a.check_stall(later);
        assert_eq!(bytes(&rec).matches("no progress for 1m").count(), 1);
        a.event(&needs());
        a.check_stall(Instant::now() + Duration::from_secs(600));
        assert_eq!(bytes(&rec).matches("no progress").count(), 1);
    }

    #[test]
    fn by_default_only_what_blocks_work_notifies() {
        let settings = Notify {
            method: Method::Osc9,
            stall_after: Duration::from_secs(60),
            ..Notify::default()
        };
        let (a, rec) = layer(settings, &[]);
        a.event(&started());
        a.check_stall(Instant::now() + Duration::from_secs(120));
        assert!(
            !bytes(&rec).contains("no progress"),
            "a stall is not a decision"
        );
        a.event(&needs());
        assert_eq!(
            bytes(&rec).matches("\x1b]9;").count(),
            1,
            "{:?}",
            bytes(&rec)
        );
    }

    #[test]
    fn a_command_gets_the_message_last() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let out = dir.path().join("got");
        let settings = Notify {
            method: Method::Command(vec![
                "sh".to_owned(),
                "-c".to_owned(),
                format!("printf '%s' \"$0\" > {}", out.display()),
            ]),
            ..Notify::default()
        };
        let (a, _) = layer(settings, &[]);
        a.event(&started());
        a.event(&needs());
        // The file exists before printf has written it: wait for the text.
        let mut got = String::new();
        for _ in 0..100 {
            got = std::fs::read_to_string(&out).unwrap_or_default();
            if !got.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(got, "speccoder: T004 Handle Unicode — cargo add x?");
    }
}
