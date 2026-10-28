//! What the host runs for the page: one job at a time — a planning skill, a
//! run, a planning commit, a decision on a task. A job's progress, and the
//! calls and questions that need the person, go to the page as events; the
//! page's answers come back here and reach the waiting job. Nothing about a
//! run changes: it is journaled, decided and committed as from the CLI.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};
use speccoder_core::actions::{Runnable, Via};
use speccoder_core::{CommandResult, CommandStatus};
use speccoder_runtime::service::approvals::{Answer, Approver, Presentation, Who};
use speccoder_runtime::service::progress::{Event, Progress};
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
    root: crate::Root,
    /// Signs the person's own commands this host sends, and checks them.
    pub issuer: crate::fixes::Issuer,
    events: broadcast::Sender<String>,
    next: AtomicU64,
    job: Mutex<Option<JobView>>,
    waiting: Mutex<Option<Waiting>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl Hub {
    /// A hub for the project `root` is (or will be, once bound), signing
    /// with `issuer`.
    #[must_use]
    pub fn new(root: crate::Root, issuer: crate::fixes::Issuer) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        Arc::new(Self {
            root,
            issuer,
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
        let who = Who::local_in(&self.root.get(), Via::Web);
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

    /// Takes the job slot for `command`: its number and cancel flag;
    /// `Err` when a job is already working.
    fn begin(&self, command: &str) -> Result<(u64, Arc<AtomicBool>), String> {
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
                command: command.to_owned(),
                running: true,
                result: None,
            });
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut c) = self.cancel.lock() {
            *c = Some(Arc::clone(&cancel));
        }
        self.send(&json!({ "type": "started", "id": id, "command": command }));
        Ok((id, cancel))
    }

    /// Ends job `id` with `result`, its command steps signed for `root`.
    fn finish(&self, id: u64, root: &Path, result: &CommandResult) {
        let mut value = serde_json::to_value(result).unwrap_or(Value::Null);
        self.issuer.sign(root, &mut value);
        if let Ok(mut job) = self.job.lock()
            && let Some(j) = job.as_mut()
        {
            j.running = false;
            j.result = Some(value.clone());
        }
        if let Ok(mut c) = self.cancel.lock() {
            *c = None;
        }
        self.send(&json!({ "type": "done", "id": id, "result": value }));
    }

    /// Starts `run` (with `input` for actions that take one) for `feature`,
    /// in `root` (the bound project's, or a folder of a host not bound yet);
    /// `Err` when a job is already working.
    pub fn start(
        self: &Arc<Self>,
        run: Runnable,
        input: Option<String>,
        feature: Option<String>,
        command: String,
        root: PathBuf,
    ) -> Result<u64, String> {
        let (id, cancel) = self.begin(&command)?;
        let hub = Arc::clone(self);
        std::thread::spawn(move || {
            let peer: Arc<Peer> = Arc::new(Peer {
                hub: Arc::clone(&hub),
                who: Who::local_in(&root, Via::Web),
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute(
                    &root,
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
            let result = speccoder_runtime::service::next::attach(result, feature.as_deref());
            // A host started outside a project binds to the one this job
            // made, cloned or opened: once, then the page reloads into it.
            if let Some(bound) = crate::welcome::bind_after(&hub.root, &run, &result) {
                hub.send(&json!({ "type": "bound", "root": bound.display().to_string() }));
            }
            hub.finish(id, &hub.root.get(), &result);
        });
        Ok(id)
    }

    /// Runs the person's own command — argvs this host issued, checked by
    /// the caller — in `root`, as a job: its output lines go to the page
    /// (`output` events), Stop ends it, and it ends in its result.
    pub fn start_fix(
        self: &Arc<Self>,
        argvs: Vec<Vec<String>>,
        root: PathBuf,
    ) -> Result<u64, String> {
        use speccoder_runtime::service::person;
        let (id, cancel) = self.begin(&person::shown(&argvs))?;
        let hub = Arc::clone(self);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                person::run(&root, &argvs, person::TIMEOUT, &cancel, &|stream, line| {
                    hub.send(&json!({ "type": "output", "id": id, "stream": stream.as_str(), "line": line }));
                })
            }))
            .unwrap_or_else(|_| {
                CommandResult::new("fix", CommandStatus::Error, "the command stopped unexpectedly")
            });
            hub.finish(id, &root, &result);
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

    fn via(&self) -> Via {
        Via::Web
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
    use speccoder_runtime::service::actions::{Dispatch, Interface, Offer, offered, perform};
    if offered(Interface::WebJobs, run.kind()) == Offer::Refuse {
        return Interface::WebJobs.refusal();
    }
    let options = speccoder_runtime::service::RunOptions {
        approver: Some(Arc::clone(peer) as Arc<dyn Approver>),
        progress: Some(Arc::clone(peer) as Arc<dyn Progress>),
        cancel: Some(cancel),
        ..speccoder_runtime::service::RunOptions::default()
    };
    perform(
        root,
        feature,
        Dispatch {
            run,
            input,
            via: Via::Web,
            options,
        },
    )
}
