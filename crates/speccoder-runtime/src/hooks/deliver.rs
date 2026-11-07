//! Telling observers what speccoder decided, at least once. Events are read
//! from git (see [`super::events`]) past each observer's cursor, oldest
//! first; the cursor moves only after the observer took one (2xx, or exit
//! 0). Whichever speccoder process runs delivers; a run does it beside its
//! own work and never waits on an observer beyond a short grace at its end.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use speccoder_core::hooks::{HookDef, Runner, TrustState};

use super::HookRun;
use super::events::{self, Event};
use crate::git::Git;
use crate::verify::process::{Invocation, OUTPUT_CAP};

const CURSORS: &str = "cursors.json";
const LOCK: &str = "deliver.lock";

/// Attempts a process makes at one event before leaving it to the next.
const TRIES: u32 = 3;

/// How far an observer got: the tips it has heard up to, and the saved
/// attempts it heard.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Branch → the last commit delivered on it (or its tip when the
    /// observer started).
    pub tips: BTreeMap<String, String>,
    /// `attempt:<commit>` ids delivered (the last 2,000).
    pub attempts: Vec<String>,
}

fn cursors(dir: &Path) -> BTreeMap<String, Cursor> {
    std::fs::read_to_string(dir.join(CURSORS))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// A cursor at what exists now: an observer hears what happens from here.
#[must_use]
pub fn here(root: &Path) -> Cursor {
    let mut c = Cursor::default();
    for (r, sha) in events::tips(root) {
        if r.starts_with(crate::attempts::PREFIX) {
            c.attempts.push(format!("attempt:{sha}"));
        } else {
            c.tips.insert(r, sha);
        }
    }
    c
}

/// Whether `name` has a cursor yet.
#[must_use]
pub fn has_cursor(dir: &Path, name: &str) -> bool {
    cursors(dir).contains_key(name)
}

/// Starts `name`'s cursor at what exists now (a newly trusted observer).
pub fn start_here(dir: &Path, root: &Path, name: &str) {
    // Under the delivery lock: a pass running now would otherwise write
    // back the cursors it read before this one existed.
    let _ = std::fs::create_dir_all(dir);
    let lock = std::fs::File::create(dir.join(LOCK));
    let _held = lock.as_ref().map(std::fs::File::lock);
    let mut all = cursors(dir);
    all.insert(name.to_owned(), here(root));
    super::write_json(dir, CURSORS, &all);
}

/// The events past `cursor`, oldest first per branch, then the stops.
#[must_use]
pub fn pending(root: &Path, project: &str, cursor: &Cursor) -> Vec<(String, Event)> {
    let git = Git::at(root);
    // A tip that no longer exists (a branch reset, then collected) is
    // dropped: delivery goes on from what is there, inventing nothing.
    let seen: Vec<&str> = cursor
        .tips
        .values()
        .map(String::as_str)
        .filter(|sha| {
            git.succeeds(&["cat-file", "-e", &format!("{sha}^{{commit}}")])
                .unwrap_or(false)
        })
        .collect();
    let mut out = Vec::new();
    let tips = events::tips(root);
    for (r, sha) in tips
        .iter()
        .filter(|(r, _)| !r.starts_with(crate::attempts::PREFIX))
    {
        if cursor.tips.get(r) == Some(sha) {
            continue;
        }
        let mut args = vec![
            "log".to_owned(),
            "--reverse".to_owned(),
            "--format=%H".to_owned(),
            "--grep=^Verdict: ".to_owned(),
            sha.clone(),
        ];
        if !seen.is_empty() {
            args.push("--not".to_owned());
            args.extend(seen.iter().map(|s| (*s).to_owned()));
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let commits = git
            .run(&args)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        for c in commits.lines().filter(|c| !c.is_empty()) {
            if let Some(e) = events::of(&git, project, r, c) {
                out.push((r.clone(), e));
            }
        }
    }
    for (r, sha) in tips
        .iter()
        .filter(|(r, _)| r.starts_with(crate::attempts::PREFIX))
    {
        let id = format!("attempt:{sha}");
        if !cursor.attempts.contains(&id)
            && let Some(e) = events::of(&git, project, r, sha)
        {
            out.push((r.clone(), e));
        }
    }
    out
}

fn advance(cursor: &mut Cursor, reference: &str, event: &Event) {
    if let Some(commit) = event.id.strip_prefix("tick:") {
        cursor.tips.insert(reference.to_owned(), commit.to_owned());
    } else {
        cursor.attempts.push(event.id.clone());
    }
}

/// Sends `envelope` to one observer: `Ok` with what it answered, or why not.
///
/// # Errors
///
/// The observer refused it, failed, timed out or could not be reached.
pub fn send(def: &HookDef, envelope: &Value, root: &Path, state: &Path) -> Result<String, String> {
    match &def.runner {
        Runner::Url { url, secret_env } => post(def, url, secret_env.as_deref(), envelope),
        Runner::Argv(argv) => command(def, argv, envelope, root, state),
    }
}

fn post(
    def: &HookDef,
    url: &str,
    secret_env: Option<&str>,
    envelope: &Value,
) -> Result<String, String> {
    use agentplane::push::{
        Delivered, Destination, PushConfig, PushMessage, PushSender, PushTransport,
    };
    speccoder_core::hooks::url_allowed(url)?;
    let event = agentplane::core::CloudEvent::from_json(envelope.to_string().as_bytes())
        .map_err(|e| format!("the event is not a CloudEvent: {e}"))?;
    let mut destination = Destination::new(&def.name, url);
    if let Some(var) = secret_env {
        let key = std::env::var(var).map_err(|_| format!("${var} is not set"))?;
        destination = destination
            .try_signed_with(&agentplane::core::Secret::new(key))
            .map_err(|e| format!("${var}: {e}"))?;
    }
    let sender = PushSender::for_operator_destinations(std::slice::from_ref(&destination))
        .timeout(Duration::from_secs(def.timeout));
    let config = PushConfig {
        id: destination.registration_id(),
        task: agentplane::core::RunId::generate(),
        url: url.to_owned(),
        token: None,
        authentication: None,
    };
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let rt = crate::pipeline::tokio_runtime()?;
    match rt.block_on(sender.deliver(&config, &PushMessage::cloudevent(&event), at)) {
        Ok(Delivered::Accepted) => Ok("accepted".to_owned()),
        Ok(Delivered::Rejected { status, .. }) => Err(format!("answered {status}")),
        Ok(Delivered::Unreachable(why)) => Err(why),
        Err(e) => Err(e.to_string()),
    }
}

fn command(
    def: &HookDef,
    argv: &[String],
    envelope: &Value,
    root: &Path,
    state: &Path,
) -> Result<String, String> {
    let (runner, _) = crate::sandbox::runner_at(root)?;
    if !runner.is_sandboxed() {
        return Err("no sandbox is enforced here, and an observer runs only in one".to_owned());
    }
    // The checkout is the observer's to read, not to change.
    let (runner, _) = runner.carve(
        &[root.to_path_buf()],
        &super::gate::hidden_state(state, root),
    );
    if !runner.hides() {
        return Err("no sandbox is enforced here that hides speccoder's state, and an observer runs only in one".to_owned());
    }
    let scratch = runner
        .scratch()
        .ok_or_else(|| "the sandbox has no scratch directory".to_owned())?;
    let file = scratch.join(format!("event-{}.json", std::process::id()));
    std::fs::write(&file, format!("{envelope}\n")).map_err(|e| e.to_string())?;
    let mut env = crate::verify::process::scrubbed_env(std::env::vars());
    for name in &def.env {
        if let Ok(v) = std::env::var(name) {
            env.retain(|(k, _)| k != name);
            env.push((name.clone(), v));
        }
    }
    env.push((
        "SPECCODER_EVENT".to_owned(),
        envelope["type"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("dev.speccoder.")
            .to_owned(),
    ));
    // The event on stdin: the shell's redirect, then the hook's own argv.
    let mut full = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "exec \"$@\" < \"$0\"".to_owned(),
        file.to_string_lossy().into_owned(),
    ];
    full.extend(argv.iter().cloned());
    let inv = Invocation {
        argv: full,
        cwd: root.to_path_buf(),
        env,
        timeout: Duration::from_secs(def.timeout),
        output_cap: OUTPUT_CAP,
    };
    let ended = runner.run_with(&inv, true);
    let _ = std::fs::remove_file(&file);
    if let Some(e) = ended.spawn_error {
        return Err(format!("could not start: {e}"));
    }
    if ended.timed_out {
        return Err(format!("timed out after {} s", def.timeout));
    }
    match ended.exit_code {
        Some(0) => Ok("exit 0".to_owned()),
        code => {
            let stderr = String::from_utf8_lossy(&ended.stderr.kept);
            let line = stderr.lines().map(str::trim).rfind(|l| !l.is_empty());
            Err(line.map_or_else(
                || code.map_or_else(|| "was killed".to_owned(), |c| format!("exit {c}")),
                speccoder_core::hooks::clean,
            ))
        }
    }
}

/// What one delivery pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delivery {
    /// Events delivered.
    pub delivered: u32,
    /// Events still waiting (an observer failed on them).
    pub pending: u32,
    /// Observers not trusted, skipped.
    pub untrusted: Vec<String>,
    /// `(observer, why)` for each that failed.
    pub failed: Vec<(String, String)>,
    /// Another process was delivering: nothing was done.
    pub busy: bool,
}

/// Failures per `observer/event` in this process: after [`TRIES`] the
/// event waits for the next process.
fn tries() -> &'static Mutex<HashMap<String, u32>> {
    static TRIED: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    TRIED.get_or_init(Mutex::default)
}

/// Delivers what is pending for every trusted observer of the project at
/// `root`, until `deadline` (if given).
#[must_use]
pub fn pass(root: &Path, state: &Path, deadline: Option<Instant>) -> Delivery {
    let mut out = Delivery::default();
    let Ok(hooks) = super::load(root) else {
        return out;
    };
    let observers: Vec<&HookDef> = hooks.iter().filter(|h| h.at().is_none()).collect();
    if observers.is_empty() {
        return out;
    }
    let dir = super::dir(state, root);
    let _ = std::fs::create_dir_all(&dir);
    let Ok(lock) = std::fs::File::create(dir.join(LOCK)) else {
        return out;
    };
    if lock.try_lock().is_err() {
        out.busy = true;
        return out;
    }
    let project = super::project_id(state, root);
    let mut all = cursors(&dir);
    for def in observers {
        let digest = super::digest(def, root).digest;
        if super::trust::state(&dir, &def.name, &digest) != TrustState::Trusted {
            out.untrusted.push(def.name.clone());
            continue;
        }
        let Some(cursor) = all.get_mut(&def.name) else {
            // Trusted without a cursor (from CI's flag): from here on.
            all.insert(def.name.clone(), here(root));
            continue;
        };
        // Saved attempts whose refs are gone need no memory.
        let present: std::collections::HashSet<String> = events::tips(root)
            .into_iter()
            .filter(|(r, _)| r.starts_with(crate::attempts::PREFIX))
            .map(|(_, sha)| format!("attempt:{sha}"))
            .collect();
        cursor.attempts.retain(|id| present.contains(id));
        for (reference, event) in pending(root, &project, cursor) {
            if !def.hears(event.kind) {
                advance(cursor, &reference, &event);
                continue;
            }
            // In order: what follows waits behind an event not sent now.
            if deadline.is_some_and(|d| Instant::now() >= d) {
                out.pending += 1;
                break;
            }
            let key = format!("{}/{}", def.name, event.id);
            let tried = tries()
                .lock()
                .map_or(0, |t| t.get(&key).copied().unwrap_or(0));
            if tried >= TRIES {
                out.pending += 1;
                break;
            }
            let began = Instant::now();
            let sent = send(def, &event.envelope, root, state);
            let duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
            super::record_run(
                &dir,
                &def.name,
                HookRun {
                    at: crate::attest::now_rfc3339(),
                    what: event.id.clone(),
                    outcome: if sent.is_ok() { "delivered" } else { "failed" }.to_owned(),
                    duration_ms,
                    error: sent.as_ref().err().cloned(),
                },
            );
            match sent {
                Ok(_) => {
                    advance(cursor, &reference, &event);
                    out.delivered += 1;
                }
                Err(e) => {
                    if let Ok(mut t) = tries().lock() {
                        *t.entry(key).or_default() += 1;
                    }
                    out.failed.push((def.name.clone(), e));
                    out.pending += 1;
                    // In order: the rest wait behind the one that failed.
                    break;
                }
            }
        }
    }
    super::write_json(&dir, CURSORS, &all);
    out
}

/// A run's deliverer: delivers beside the run; dropped, it gives what is
/// pending at most [`GRACE`]. One per process — a feature run's tasks share
/// their run's.
#[derive(Debug)]
pub struct Deliverer {
    stop: mpsc::Sender<()>,
    done: mpsc::Receiver<()>,
}

/// How long a run waits for its last deliveries.
pub const GRACE: Duration = Duration::from_secs(5);

static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Deliverer {
    /// Starts delivering for the project at `root`, when it has observers
    /// and no deliverer runs in this process yet.
    #[must_use]
    pub fn start(root: &Path, state: &Path) -> Option<Self> {
        use std::sync::atomic::Ordering;
        let has_observers = super::load(root)
            .map(|h| h.iter().any(|d| d.at().is_none()))
            .unwrap_or(false);
        if !has_observers || ACTIVE.swap(true, Ordering::SeqCst) {
            return None;
        }
        let (stop, stopped) = mpsc::channel::<()>();
        let (finished, done) = mpsc::channel::<()>();
        let (root, state): (PathBuf, PathBuf) = (root.to_path_buf(), state.to_path_buf());
        std::thread::spawn(move || {
            loop {
                let _ = pass(&root, &state, None);
                match stopped.recv_timeout(Duration::from_secs(1)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            let _ = pass(&root, &state, Some(Instant::now() + GRACE));
            let _ = finished.send(());
        });
        Some(Self { stop, done })
    }
}

impl Drop for Deliverer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        let _ = self.done.recv_timeout(GRACE + Duration::from_secs(1));
        ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_moves_per_event() {
        let mut c = Cursor::default();
        let tick = Event {
            id: "tick:abc".into(),
            kind: "task.ticked",
            envelope: Value::Null,
        };
        advance(&mut c, "refs/heads/speccoder/x", &tick);
        assert_eq!(c.tips["refs/heads/speccoder/x"], "abc");
        let stop = Event {
            id: "attempt:def".into(),
            kind: "task.stopped",
            envelope: Value::Null,
        };
        advance(&mut c, "refs/speccoder/attempts/x/T1/r", &stop);
        assert_eq!(c.attempts, ["attempt:def"]);
    }
}
