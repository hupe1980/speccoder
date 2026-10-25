//! What the host runs for the page: one job at a time — a planning skill, a
//! run, a planning commit, a decision on a task. A job's progress, and the
//! calls and questions that need the person, go to the page as events; the
//! page's answers come back here and reach the waiting job. Nothing about a
//! run changes: it is journaled, decided and committed as from the CLI.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};
use speccoder_core::actions::Runnable;
use speccoder_core::{CommandResult, CommandStatus};
use speccoder_runtime::approvals::{Answer, Approver, Presentation, Who};
use speccoder_runtime::progress::{Event, Progress};
use tokio::sync::broadcast;

/// What a waiting job is answered with.
#[derive(Debug)]
pub enum Reply {
    /// A decision on a call.
    Answer(Answer),
    /// A question's answer (`None`: skipped).
    Text(Option<String>),
}

#[derive(Debug)]
struct Waiting {
    id: u64,
    reply: mpsc::Sender<Reply>,
    event: Value,
}

/// The job now, as the page sees it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JobView {
    /// Its number.
    pub id: u64,
    /// The command it stands for.
    pub command: String,
    /// Still working.
    pub running: bool,
    /// Its result envelope, once done.
    pub result: Option<Value>,
}

/// The host's jobs and the page's event stream.
#[derive(Debug)]
pub struct Hub {
    root: PathBuf,
    events: broadcast::Sender<String>,
    next: AtomicU64,
    job: Mutex<Option<JobView>>,
    waiting: Mutex<Option<Waiting>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl Hub {
    /// A hub for the project at `root`.
    #[must_use]
    pub fn new(root: &Path) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        Arc::new(Self {
            root: root.to_path_buf(),
            events,
            next: AtomicU64::new(1),
            job: Mutex::new(None),
            waiting: Mutex::new(None),
            cancel: Mutex::new(None),
        })
    }

    /// Every event from now on, as JSON text.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events.subscribe()
    }

    fn send(&self, v: &Value) {
        let _ = self.events.send(v.to_string());
    }

    /// The job, if one ran since the host started.
    #[must_use]
    pub fn job(&self) -> Option<JobView> {
        self.job.lock().ok().and_then(|j| j.clone())
    }

    /// What waits for the person now (an `ask` or `question` event).
    #[must_use]
    pub fn waiting(&self) -> Option<Value> {
        self.waiting
            .lock()
            .ok()
            .and_then(|w| w.as_ref().map(|w| w.event.clone()))
    }

    /// Answers what waits; `Err` when nothing does, or something else does.
    pub fn answer(&self, id: u64, reply: Reply) -> Result<(), String> {
        let w = self
            .waiting
            .lock()
            .map_err(|_| "the host is broken".to_owned())?
            .take();
        match w {
            Some(w) if w.id == id => {
                let _ = w.reply.send(reply);
                self.send(&json!({ "type": "answered", "id": id }));
                Ok(())
            }
            Some(w) => {
                let other = w.id;
                if let Ok(mut slot) = self.waiting.lock() {
                    *slot = Some(w);
                }
                Err(format!("{other} waits, not {id}"))
            }
            None => Err("nothing waits for an answer".to_owned()),
        }
    }

    /// Asks the running job to stop before its next task; a call waiting for
    /// the person is answered with "stop" (it keeps waiting, resumably).
    pub fn stop(&self) {
        if let Some(c) = self.cancel.lock().ok().and_then(|c| c.clone()) {
            c.store(true, Ordering::SeqCst);
        }
        let who = Who::local_in(&self.root, "web");
        if let Some(w) = self.waiting.lock().ok().and_then(|mut w| w.take()) {
            let _ = w.reply.send(match w.event["type"].as_str() {
                Some("question") => Reply::Text(None),
                _ => Reply::Answer(Answer::stop(Some(who))),
            });
        }
        self.send(&json!({ "type": "stopping" }));
    }

    fn wait_for(&self, mut event: Value) -> Option<Reply> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        event["id"] = json!(id);
        let (tx, rx) = mpsc::channel();
        if let Ok(mut w) = self.waiting.lock() {
            *w = Some(Waiting {
                id,
                reply: tx,
                event: event.clone(),
            });
        }
        self.send(&event);
        rx.recv().ok()
    }

    /// Runs `f` while no job runs and none can start — for a short change
    /// to the checkout (a commit): the check and the change are one step.
    ///
    /// # Errors
    ///
    /// A job is working.
    pub fn exclusive<T>(&self, f: impl FnOnce() -> T) -> Result<T, String> {
        let job = self
            .job
            .lock()
            .map_err(|_| "the host is broken".to_owned())?;
        if job.as_ref().is_some_and(|j| j.running) {
            return Err("a job is working; do this when it ends".to_owned());
        }
        let out = f();
        drop(job);
        Ok(out)
    }

    /// Starts `run` (with `input` for actions that take one) for `feature`;
    /// `Err` when a job is already working.
    pub fn start(
        self: &Arc<Self>,
        run: Runnable,
        input: Option<String>,
        feature: Option<String>,
        command: String,
    ) -> Result<u64, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        {
            let mut job = self
                .job
                .lock()
                .map_err(|_| "the host is broken".to_owned())?;
            if job.as_ref().is_some_and(|j| j.running) {
                return Err("a job is working; stop it or wait for it".to_owned());
            }
            *job = Some(JobView {
                id,
                command: command.clone(),
                running: true,
                result: None,
            });
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut c) = self.cancel.lock() {
            *c = Some(Arc::clone(&cancel));
        }
        self.send(&json!({ "type": "started", "id": id, "command": command }));
        let hub = Arc::clone(self);
        std::thread::spawn(move || {
            let peer: Arc<Peer> = Arc::new(Peer {
                hub: Arc::clone(&hub),
                who: Who::local_in(&hub.root, "web"),
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute(
                    &hub.root,
                    feature.as_deref(),
                    &run,
                    input.as_deref(),
                    &peer,
                    cancel,
                )
            }))
            .unwrap_or_else(|_| {
                CommandResult::new("job", CommandStatus::Error, "the job stopped unexpectedly")
            });
            let result = speccoder_runtime::next::attach(result, feature.as_deref());
            let value = serde_json::to_value(&result).unwrap_or(Value::Null);
            if let Ok(mut job) = hub.job.lock()
                && let Some(j) = job.as_mut()
            {
                j.running = false;
                j.result = Some(value.clone());
            }
            if let Ok(mut c) = hub.cancel.lock() {
                *c = None;
            }
            hub.send(&json!({ "type": "done", "id": id, "result": value }));
        });
        Ok(id)
    }
}

/// The job's side: its progress sink and the person it asks.
#[derive(Debug)]
struct Peer {
    hub: Arc<Hub>,
    who: Who,
}

impl Progress for Peer {
    fn event(&self, event: &Event) {
        self.hub
            .send(&json!({ "type": "progress", "event": event }));
    }
}

impl Approver for Peer {
    fn ask(&self, p: &Presentation) -> Answer {
        match self
            .hub
            .wait_for(json!({ "type": "ask", "presentation": p }))
        {
            Some(Reply::Answer(mut a)) => {
                a.by = Some(self.who.clone());
                a
            }
            _ => Answer::stop(Some(self.who.clone())),
        }
    }

    fn answer(&self, question: &str) -> Option<String> {
        match self
            .hub
            .wait_for(json!({ "type": "question", "question": question }))
        {
            Some(Reply::Text(t)) => t,
            _ => None,
        }
    }

    fn via(&self) -> String {
        "web".to_owned()
    }
}

fn execute(
    root: &Path,
    feature: Option<&str>,
    run: &Runnable,
    input: Option<&str>,
    peer: &Arc<Peer>,
    cancel: Arc<AtomicBool>,
) -> CommandResult {
    use speccoder_runtime::approvals::{Choice, Verb, decide};
    let needs = |what: &str| {
        CommandResult::new(
            "job",
            CommandStatus::UsageError,
            &format!("{what} is needed"),
        )
    };
    let state = speccoder_runtime::worktree::state_dir();
    let options = speccoder_runtime::RunOptions {
        approver: Some(Arc::clone(peer) as Arc<dyn Approver>),
        progress: Some(Arc::clone(peer) as Arc<dyn Progress>),
        cancel: Some(cancel),
        ..speccoder_runtime::RunOptions::default()
    };
    let text = input.map(str::trim).filter(|t| !t.is_empty());
    match run {
        Runnable::Skill { name, arguments } => {
            let args = text.map_or_else(|| arguments.clone(), str::to_owned);
            speccoder_runtime::hosting::run(root, feature, name, &args, &options)
        }
        Runnable::Run { task: Some(t) } => speccoder_runtime::run(root, feature, t, &options),
        Runnable::Run { task: None } => {
            speccoder_runtime::run_feature(root, feature, &options, false)
        }
        Runnable::Commit { paths } => {
            speccoder_runtime::hosting::commit_planning(root, feature, paths)
        }
        Runnable::Approve { task } => {
            decide(root, feature, Verb::Decide(Choice::approve(task, text)))
        }
        Runnable::Reject { task } => match text {
            Some(why) => decide(root, feature, Verb::Decide(Choice::reject(task, why))),
            None => needs("a reason"),
        },
        Runnable::Accept { task } => match (text, &state) {
            (Some(why), Some(s)) => {
                speccoder_runtime::controls::accept_in(root, feature, task, why, s, false)
            }
            (None, _) => needs("a reason"),
            (_, None) => {
                CommandResult::new("job", CommandStatus::ConfigError, "no state directory")
            }
        },
        Runnable::Skip { task } => match text {
            Some(why) => speccoder_runtime::controls::skip(root, feature, task, Some(why)),
            None => needs("a reason"),
        },
        Runnable::Nudge { task } => match (text, &state) {
            (Some(note), Some(s)) => {
                speccoder_runtime::controls::nudge_with(root, feature, task, Some(note), s)
            }
            (None, _) => needs("a note"),
            (_, None) => {
                CommandResult::new("job", CommandStatus::ConfigError, "no state directory")
            }
        },
        Runnable::Feature => match text {
            Some(name) => speccoder_runtime::current::switch(root, name),
            None => needs("a feature"),
        },
        // The page has confirmed it (the action's `confirm`): push and open.
        Runnable::PullRequest => {
            let gh = speccoder_runtime::github::gh();
            speccoder_runtime::handoff::pr(
                root,
                feature,
                &speccoder_runtime::handoff::PrOptions {
                    remote: "origin",
                    base: None,
                    gh: &gh,
                    open: true,
                    force: false,
                },
            )
        }
    }
}
