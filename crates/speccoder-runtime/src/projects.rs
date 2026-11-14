//! Several projects, one view (spec 097): the recent projects rolled up
//! read-only, each on its own thread under a time limit; another project
//! opened only by starting its own host; a stop reported to another
//! project as a draft of its own bug sheet. Nothing here writes into
//! another project or holds its key.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use speccoder_core::projects::{
    Elsewhere, ItemAcross, LiveTask, Place, ProjectState, ProjectSummary, ProjectsReport, state_of,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::welcome;

/// How long one project's read may take.
pub const LIMIT: Duration = Duration::from_secs(5);
/// The longest bug text handed to another project.
pub const MAX_REPORT: usize = 2000;
/// How long another project's `bug` may take to record a report.
const RECORD_LIMIT: Duration = Duration::from_secs(30);
/// How long a host just started may take to say it listens.
const STARTING: u64 = 15;

/// Projects whose read has not finished: not read again until it does.
fn reading() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static R: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    R.get_or_init(Default::default)
}

/// The presence of a host serving a project: never its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    /// Its process.
    pub pid: u32,
    /// Its port.
    pub port: u16,
    /// Unix seconds it started.
    pub started: u64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One file per project folder (worktrees of one repository share a state
/// directory, never a presence).
fn presence_file(state: &Path, root: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let id: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    crate::worktree::project_state_dir(state, root).join(format!("host-{id}.json"))
}

fn write_presence(state: &Path, root: &Path, p: &Presence) {
    let f = presence_file(state, root);
    if let (Some(dir), Ok(text)) = (f.parent(), serde_json::to_string(p)) {
        let _ = std::fs::create_dir_all(dir);
        let tmp = f.with_extension(format!("{}.tmp", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }
}

/// Records that this process serves the project at `root`.
pub fn present(state: &Path, root: &Path, port: u16) {
    write_presence(
        state,
        root,
        &Presence {
            pid: std::process::id(),
            port,
            started: now(),
        },
    );
}

/// Removes this process's presence for `root` (another's is left alone).
pub fn absent(state: &Path, root: &Path) {
    let f = presence_file(state, root);
    if presence_of(state, root).is_some_and(|p| p.pid == std::process::id()) {
        let _ = std::fs::remove_file(f);
    }
}

/// The live host serving the project at `root`, if any: its process
/// alive and its port answering — or, just started (port 0), still within
/// the time a host takes to listen. Anything else is stale.
#[must_use]
pub fn presence_of(state: &Path, root: &Path) -> Option<Presence> {
    let text = std::fs::read_to_string(presence_file(state, root)).ok()?;
    let p: Presence = serde_json::from_str(&text).ok()?;
    if !crate::progress::alive(p.pid) {
        return None;
    }
    if p.port == 0 {
        return (now().saturating_sub(p.started) < STARTING).then_some(p);
    }
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], p.port));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300))
        .is_ok()
        .then_some(p)
}

/// One project's roll-up and its items, read on this thread.
fn summarise(
    state: &Path,
    recent: &welcome::Recent,
    here: Option<&Path>,
) -> (ProjectSummary, Vec<ItemAcross>) {
    let path = Path::new(&recent.path);
    let mut s = ProjectSummary {
        name: recent.name.clone(),
        path: recent.path.clone(),
        shown: welcome::tilde(path),
        state: ProjectState::Missing,
        current: here.is_some_and(|h| same(h, path)),
        feature: None,
        live: None,
        waiting: None,
        unknown: None,
        spend: None,
        elsewhere: None,
    };
    if !path.is_dir() {
        s.unknown = Some("folder not found".into());
        return (s, Vec::new());
    }
    if !welcome::is_project(path) {
        s.unknown = Some("not a project anymore".into());
        return (s, Vec::new());
    }
    let live = crate::progress::read_live(path, state).filter(|(l, _)| l.finished.is_none());
    let running = live.as_ref().is_some_and(|(_, r)| *r);
    s.live = live
        .as_ref()
        .filter(|(l, _)| !l.task.is_empty())
        .map(|(l, _)| LiveTask {
            task: l.task.clone(),
            stage: l
                .stage
                .as_ref()
                .and_then(|st| serde_json::to_value(st).ok())
                .and_then(|v| v.as_str().map(str::to_owned)),
        });
    if !s.current {
        s.elsewhere = presence_of(state, path).map(|p| Elsewhere {
            pid: p.pid,
            since: p.started,
        });
    }
    if let Some(CommandData::Cost(c)) = crate::controls::cost(path, None).data() {
        s.feature = Some(c.feature.clone());
        s.spend = Some(c.total.clone());
    }
    let mut items = Vec::new();
    let inbox = crate::inbox::inbox_with(path, None, Some(state));
    let (blocked, readable, other) = match inbox.data() {
        Some(CommandData::Inbox(i)) => {
            let unknown = i.items.iter().find(|x| x.kind == "unknown");
            if let Some(u) = unknown {
                s.unknown = Some(crate::approvals::present::escape(&crate::secrets::mask(
                    &u.detail,
                )));
            }
            for x in i.items.iter().filter(|x| x.kind != "unknown") {
                items.push(ItemAcross {
                    project: recent.name.clone(),
                    path: recent.path.clone(),
                    feature: x.feature.clone(),
                    task: x.task.clone(),
                    kind: x.kind.clone(),
                    group: x.group.clone(),
                    title: crate::approvals::present::escape(&crate::secrets::mask(&x.title)),
                    since: x.since,
                });
            }
            let blocked = items.iter().filter(|x| x.group == "blocked_on_you").count();
            (blocked, unknown.is_none(), items.len() - blocked)
        }
        // A repository speccoder was opened in but never set up: nothing
        // can wait in it yet.
        _ if matches!(inbox.status(), CommandStatus::NotAProject) => {
            s.unknown = Some("not set up yet".into());
            (0, true, 0)
        }
        _ => {
            s.unknown = Some("its Needs You could not be read".into());
            (0, false, 0)
        }
    };
    s.waiting = readable.then_some(blocked);
    s.state = state_of(blocked, running, readable, other);
    (s, items)
}

fn same(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

/// Every recent project rolled up, and what waits across them; `here` is
/// the project the asking page belongs to.
#[must_use]
pub fn list(state: &Path, here: Option<&Path>) -> ProjectsReport {
    let recents = welcome::recent(state);
    let unanswered = |r: &welcome::Recent, why: &str| ProjectSummary {
        name: r.name.clone(),
        path: r.path.clone(),
        shown: welcome::tilde(Path::new(&r.path)),
        state: ProjectState::CantTell,
        current: here.is_some_and(|h| same(h, Path::new(&r.path))),
        feature: None,
        live: None,
        waiting: None,
        unknown: Some(why.to_owned()),
        spend: None,
        elsewhere: None,
    };
    // Each project on its own thread; one whose last read still hangs is
    // not read again until it ends.
    let handles: Vec<_> = recents
        .iter()
        .map(|r| {
            let free = reading()
                .lock()
                .is_ok_and(|mut set| set.insert(r.path.clone()));
            if !free {
                return (r, None);
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let (state, r2, here) = (state.to_path_buf(), r.clone(), here.map(Path::to_path_buf));
            std::thread::spawn(move || {
                let out = summarise(&state, &r2, here.as_deref());
                if let Ok(mut set) = reading().lock() {
                    set.remove(&r2.path);
                }
                let _ = tx.send(out);
            });
            (r, Some(rx))
        })
        .collect();
    // One deadline for all: a hung project costs the list 5 s, not 5 s each.
    let deadline = Instant::now() + LIMIT;
    let mut projects = Vec::new();
    let mut items: Vec<(usize, ItemAcross)> = Vec::new();
    for (r, rx) in handles {
        let Some(rx) = rx else {
            projects.push(unanswered(r, "not answering"));
            continue;
        };
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((s, i)) => {
                projects.push(s);
                // Each project's own order is its rank in the merge.
                items.extend(i.into_iter().enumerate());
            }
            Err(_) => projects.push(unanswered(r, "not answering")),
        }
    }
    // By state, then as recently used (the recent list's order is kept).
    projects.sort_by_key(|p| p.state);
    let blocked = |i: &ItemAcross| i.group != "blocked_on_you";
    items.sort_by(|(ra, a), (rb, b)| {
        blocked(a)
            .cmp(&blocked(b))
            .then(ra.cmp(rb))
            .then(
                a.since
                    .unwrap_or(u64::MAX)
                    .cmp(&b.since.unwrap_or(u64::MAX)),
            )
            .then(a.project.cmp(&b.project))
    });
    ProjectsReport {
        projects,
        items: items.into_iter().map(|(_, i)| i).collect(),
    }
}

/// The recent project named `which` (its name or its folder).
fn find(state: &Path, which: &str) -> Option<welcome::Recent> {
    let recents = welcome::recent(state);
    recents
        .iter()
        .find(|r| same(Path::new(&r.path), Path::new(which)))
        .or_else(|| {
            let named: Vec<_> = recents.iter().filter(|r| r.name == which).collect();
            (named.len() == 1).then(|| named[0])
        })
        .cloned()
}

/// Removes `which` from the recent projects; nothing of it is touched.
pub fn forget(state: &Path, which: &str) -> Result<String, String> {
    let r = find(state, which).ok_or_else(|| format!("{which} is not a recent project"))?;
    welcome::forget(state, Path::new(&r.path));
    Ok(r.name)
}

/// What opening another project did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Opened {
    /// Its own host was started.
    Started {
        /// Its name.
        name: String,
    },
    /// Another speccoder serves it already: decide there.
    Elsewhere {
        /// Its name.
        name: String,
        /// That process.
        pid: u32,
        /// Since when.
        since: u64,
    },
}

/// Opens the recent project `which` in its own host — its own process and
/// key — at `at`, its bug sheet prefilled with `bug`; `window`: in
/// speccoder's window rather than the browser.
pub fn open(
    state: &Path,
    which: &str,
    at: Option<&Place>,
    bug: Option<&str>,
    window: bool,
) -> Result<Opened, String> {
    let r = find(state, which).ok_or_else(|| format!("{which} is not a recent project"))?;
    let path = PathBuf::from(&r.path);
    if !path.is_dir() {
        return Err(format!("{}: folder not found", r.name));
    }
    if let Some(p) = presence_of(state, &path) {
        return Ok(Opened::Elsewhere {
            name: r.name,
            pid: p.pid,
            since: p.started,
        });
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("ui").arg("-C").arg(&path);
    if window {
        cmd.arg("--window");
    }
    if let Some(at) = at {
        cmd.args(["--at", &at.text()]);
    }
    if let Some(text) = bug {
        cmd.args(["--bug-from", &truncate(text)]);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own session: it outlives this host.
        cmd.process_group(0);
    }
    let child = cmd.spawn().map_err(|e| format!("{}: {e}", r.name))?;
    // Starting: a second click finds it before it listens (port 0 until
    // the host says where).
    write_presence(
        state,
        &path,
        &Presence {
            pid: child.id(),
            port: 0,
            started: now(),
        },
    );
    Ok(Opened::Started { name: r.name })
}

fn truncate(text: &str) -> String {
    text.chars().take(MAX_REPORT).collect()
}

/// The draft of a bug report about `feature/task`'s stop in the project at
/// `root`, for another project: where it came from, how it stopped, which
/// checks failed — masked and escaped; never a test body or held-out
/// material (the saved attempt's reason names checks, not their output).
pub fn report_text(root: &Path, feature: &str, task: &str) -> Result<String, String> {
    let name = root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let saved = crate::attempts::list(root, feature)
        .into_iter()
        // Newest first: the last stop.
        .find(|s| s.task == task)
        .ok_or_else(|| format!("{task} has no stopped attempt to report"))?;
    let reason = saved.reason.lines().next().unwrap_or_default();
    let text = format!(
        "From {name} · {feature} · {task}: {} — {reason}",
        saved.outcome.replace('_', " ")
    );
    Ok(truncate(&crate::approvals::present::escape(
        &crate::secrets::mask(&text),
    )))
}

/// Records `feature/task`'s stop here (at `here`) as a bug in the recent
/// project `which`, through that project's own `bug --no-run` in its own
/// process — one commit of its spec files, nothing run; `expected` is
/// what should happen instead, the person's words. Returns its message.
pub fn file_report(
    state: &Path,
    here: &Path,
    which: &str,
    feature: &str,
    task: &str,
    expected: &str,
) -> Result<String, String> {
    let expected = expected.trim();
    if expected.is_empty() || expected.contains('\n') || expected.chars().count() > 500 {
        return Err("say in one line what should happen instead".into());
    }
    let r = find(state, which).ok_or_else(|| format!("{which} is not a recent project"))?;
    if same(Path::new(&r.path), here) {
        return Err("that is this project: report the bug here".into());
    }
    // Its own run must not see its checkout change under it.
    if let Some((live, true)) = crate::progress::read_live(Path::new(&r.path), state)
        && live.finished.is_none()
    {
        return Err(format!(
            "{} is running {} now — report it when that is done, or Edit There First",
            r.name,
            if live.task.is_empty() {
                "a task"
            } else {
                live.task.as_str()
            }
        ));
    }
    let text = report_text(here, feature, task)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe)
        .arg("-C")
        .arg(&r.path)
        .args(["--json", "bug", "--no-run", "--no-prompt"])
        // One word: what should happen may start with `-`.
        .arg(format!("--expected={expected}"))
        .arg("--")
        .arg(&text)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", r.name))?;
    // Its commit may wait on a hook or a signing prompt: not forever.
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if began.elapsed() < RECORD_LIMIT => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not record it within {} s (a commit hook or a signing prompt?) — Edit There First",
                    r.name,
                    RECORD_LIMIT.as_secs()
                ));
            }
        }
    }
    let mut stdout = Vec::new();
    if let Some(mut o) = child.stdout.take() {
        use std::io::Read;
        let _ = o.read_to_end(&mut stdout);
    }
    let v: serde_json::Value = serde_json::from_slice(&stdout)
        .map_err(|_| format!("{} did not answer as speccoder does", r.name))?;
    let message = v["message"].as_str().unwrap_or_default().to_owned();
    if v["status"] == "ok" {
        // Where it landed, in a few words: `api · 002-bugfixes/T001`.
        Ok(
            match (v["data"]["unit"].as_str(), v["data"]["task"].as_str()) {
                (Some(u), Some(t)) => format!("{} · {u}/{t}", r.name),
                _ => format!("{}: {message}", r.name),
            },
        )
    } else {
        Err(format!("{}: {message}", r.name))
    }
}

/// `speccoder projects`: the roll-up as a result.
#[must_use]
pub fn report(state: &Path, here: Option<&Path>) -> CommandResult {
    let r = list(state, here);
    let msg = format!(
        "{} project(s), {} item(s) waiting",
        r.projects.len(),
        r.items
            .iter()
            .filter(|i| i.group == "blocked_on_you")
            .count()
    );
    CommandResult::new("projects", CommandStatus::Ok, &msg)
        .with_data(CommandData::Projects(Box::new(r)))
}
