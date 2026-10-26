//! Hosting a run in the dashboard: the run works on a thread of its
//! own; its progress events and its questions to the person come to the
//! dashboard over one channel, and the person's answers go back to the
//! waiting call. Nothing about the run changes — it is journaled, decided
//! and committed exactly as a plain `run` — only where it is shown.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use speccoder_core::CommandResult;
use speccoder_runtime::approvals::{Answer, Approver, Presentation, Who};
use speccoder_runtime::progress::{Event, Progress};

/// What the run sends the dashboard.
#[derive(Debug)]
pub enum Message {
    /// A progress event.
    Event(Event),
    /// A call waits for the person; the answer goes back on the sender.
    Ask(Box<Presentation>, Sender<Answer>),
    /// A hosted skill asks a free-text question (`None`: skipped).
    Question(String, Sender<Option<String>>),
    /// The run ended.
    Done(Box<CommandResult>),
}

/// The run's side of the channel: its progress sink and its approver.
#[derive(Debug, Clone)]
pub struct Channel {
    tx: Arc<Mutex<Sender<Message>>>,
    who: Who,
}

impl Channel {
    fn send(&self, m: Message) -> bool {
        self.tx.lock().is_ok_and(|tx| tx.send(m).is_ok())
    }
}

impl Progress for Channel {
    fn event(&self, event: &Event) {
        self.send(Message::Event(event.clone()));
    }
}

impl Approver for Channel {
    fn ask(&self, p: &Presentation) -> Answer {
        let (tx, rx) = channel();
        // The dashboard is gone: nobody answers now, the call waits.
        if !self.send(Message::Ask(Box::new(p.clone()), tx)) {
            return Answer::stop(Some(self.who.clone()));
        }
        rx.recv()
            .unwrap_or_else(|_| Answer::stop(Some(self.who.clone())))
    }

    fn answer(&self, question: &str) -> Option<String> {
        let (tx, rx) = channel();
        if !self.send(Message::Question(question.to_owned(), tx)) {
            return None;
        }
        rx.recv().ok().flatten()
    }

    fn via(&self) -> speccoder_core::actions::Via {
        speccoder_core::actions::Via::Terminal
    }
}

/// What the work is given.
#[derive(Debug, Clone)]
pub struct Hosting {
    /// Where progress goes.
    pub progress: Arc<dyn Progress>,
    /// Who answers calls that need a person.
    pub approver: Arc<dyn Approver>,
    /// Set to stop a feature run before its next task.
    pub cancel: Arc<AtomicBool>,
}

/// Starts `work` on a thread; the dashboard reads the receiver.
pub fn start(
    path: &Path,
    work: impl FnOnce(Hosting) -> CommandResult + Send + 'static,
) -> (Receiver<Message>, Arc<AtomicBool>) {
    let (tx, rx) = channel();
    let ch = Channel {
        tx: Arc::new(Mutex::new(tx)),
        who: Who::local_in(path, speccoder_core::actions::Via::Terminal),
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let hosting = Hosting {
        progress: Arc::new(ch.clone()),
        approver: Arc::new(ch.clone()),
        cancel: Arc::clone(&cancel),
    };
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(hosting)))
            .unwrap_or_else(|p| {
                let why = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                CommandResult::new(
                    "run",
                    speccoder_core::CommandStatus::Error,
                    &format!("the run stopped unexpectedly: {why}"),
                )
            });
        ch.send(Message::Done(Box::new(result)));
    });
    (rx, cancel)
}

/// The dashboard's side, as a person answers: who they are.
#[must_use]
pub fn person(path: &Path) -> Who {
    Who::local_in(path, speccoder_core::actions::Via::Terminal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_questions_and_the_result_come_over_the_channel() {
        let (rx, _cancel) = start(Path::new("."), |h| {
            h.progress.event(&Event::Notice {
                text: "hello".to_owned(),
            });
            let said = h.approver.answer("which?");
            CommandResult::new(
                "run",
                speccoder_core::CommandStatus::Ok,
                &format!("said {said:?}"),
            )
        });
        assert!(matches!(
            rx.recv(),
            Ok(Message::Event(Event::Notice { .. }))
        ));
        match rx.recv() {
            Ok(Message::Question(q, tx)) => {
                assert_eq!(q, "which?");
                let _ = tx.send(Some("this".to_owned()));
            }
            other => panic!("{other:?}"),
        }
        match rx.recv() {
            Ok(Message::Done(r)) => assert_eq!(r.message(), "said Some(\"this\")"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_panicking_run_still_reports() {
        let (rx, _) = start(Path::new("."), |_| panic!("boom"));
        match rx.recv() {
            Ok(Message::Done(r)) => assert!(r.message().contains("boom")),
            other => panic!("{other:?}"),
        }
    }
}
