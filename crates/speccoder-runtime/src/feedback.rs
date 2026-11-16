//! Feedback between projects. A project declares the projects
//! it depends on as `[upstream.<name>]` with an outbox file of open items;
//! the upstream lists what its downstream projects ask of it and keeps its
//! decisions in its own state; the downstream reads them for each item's
//! status. Nothing here writes into another project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::feedback::{
    FeedbackReport, Item, Request, Status, Tracked, Upstream, parse_outbox,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

/// One `[upstream.<name>]` of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// Its name.
    pub name: String,
    /// Its folder, resolved against the project.
    pub path: Option<PathBuf>,
    /// Its GitHub repository.
    pub repo: Option<String>,
    /// The outbox, project-relative.
    pub outbox: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    path: Option<String>,
    repo: Option<String>,
    outbox: Option<String>,
}

/// A project's upstreams, as `speccoder.toml` declares them.
///
/// # Errors
///
/// The section cannot be read.
pub fn upstreams(root: &Path) -> Result<Vec<Declared>, String> {
    let Some(dir) = crate::verify::find_config(root) else {
        return Ok(Vec::new());
    };
    let file = dir.join(crate::verify::config::FILE);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;
    let Some(ups) = table.get("upstream") else {
        return Ok(Vec::new());
    };
    let ups = ups
        .as_table()
        .ok_or("`upstream` must be a table of [upstream.<name>] tables")?;
    ups.iter()
        .map(|(name, v)| {
            let raw: Raw = v
                .clone()
                .try_into()
                .map_err(|e| format!("[upstream.{name}]: {e}"))?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                || name.starts_with('.')
            {
                return Err(format!(
                    "[upstream.{name}]: a name of letters, digits, `.`, `_`, `-`"
                ));
            }
            if let Some(o) = &raw.outbox
                && !inside(o)
            {
                return Err(format!(
                    "[upstream.{name}]: `outbox` is a path inside this project, not `{o}`"
                ));
            }
            if raw.path.is_none() && raw.repo.is_none() {
                return Err(format!("[upstream.{name}]: give `path`, `repo`, or both"));
            }
            if let Some(r) = &raw.repo
                && (r.split('/').count() != 2 || r.contains(char::is_whitespace))
            {
                return Err(format!(
                    "[upstream.{name}]: `repo` is owner/name, not `{r}`"
                ));
            }
            Ok(Declared {
                name: name.clone(),
                path: raw.path.map(|p| {
                    let p = root.join(p);
                    std::fs::canonicalize(&p).unwrap_or(p)
                }),
                repo: raw.repo,
                outbox: raw
                    .outbox
                    .unwrap_or_else(|| format!(".speccoder/outbox/{name}.md")),
            })
        })
        .collect()
}

/// A project-relative path that stays inside the project: no root, no `..`.
fn inside(p: &str) -> bool {
    let path = Path::new(p);
    !p.is_empty()
        && path.components().all(|c| {
            matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

/// The outbox file of `up` in `root`, checked to resolve inside `root`
/// (a link out of the project is refused) before it is written.
fn outbox_file(root: &Path, up: &Declared) -> Result<PathBuf, String> {
    let file = root.join(&up.outbox);
    let root_c = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    // The nearest folder that exists decides where it really is.
    while !dir.exists() {
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    let real = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    let link = std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink());
    if !real.starts_with(&root_c) || link {
        return Err(format!(
            "{} leads out of this project; it is not written",
            up.outbox
        ));
    }
    Ok(file)
}

/// What a project keeps about feedback, in its own state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    /// Its decisions on what others ask of it: `<source path>#<id>`.
    #[serde(default)]
    pub decisions: BTreeMap<String, Decision>,
    /// Issues it filed for its upstreams: `<upstream>#<id>`.
    #[serde(default)]
    pub issues: BTreeMap<String, Filed>,
    /// The last status read of each item it asks: `<upstream>#<id>`.
    #[serde(default)]
    pub seen: BTreeMap<String, (Status, u64)>,
}

/// A decision on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The item's digest when decided.
    pub digest: String,
    /// What was decided.
    pub status: Status,
    /// Unix seconds.
    pub at: u64,
}

/// An issue filed for an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filed {
    /// `owner/name`.
    pub repo: String,
    /// Its number.
    pub number: u64,
}

fn ledger_file(state: &Path, root: &Path) -> PathBuf {
    crate::worktree::project_state_dir(state, root).join("feedback.json")
}

/// The ledger of the project at `root` (empty when there is none or it
/// cannot be read; it is written only under its lock).
#[must_use]
pub fn ledger(state: &Path, root: &Path) -> Ledger {
    load(state, root).unwrap_or_default()
}

fn load(state: &Path, root: &Path) -> Result<Ledger, String> {
    match std::fs::read_to_string(ledger_file(state, root)) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| {
            format!(
                "{} cannot be read ({e}); it is left as it is",
                ledger_file(state, root).display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ledger::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Changes the ledger of `root` under one lock per process: read, change,
/// write whole — never over a ledger that cannot be read.
fn change<T>(
    state: &Path,
    root: &Path,
    f: impl FnOnce(&mut Ledger) -> Result<T, String>,
) -> Result<T, String> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = LOCK
        .lock()
        .map_err(|_| "the feedback ledger is busy".to_owned())?;
    let mut l = load(state, root)?;
    let out = f(&mut l)?;
    save(state, root, &l)?;
    Ok(out)
}

fn save(state: &Path, root: &Path, l: &Ledger) -> Result<(), String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let f = ledger_file(state, root);
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = f.with_extension(format!("{}.{n}.tmp", std::process::id()));
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(l).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn canonical(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

/// A request's key in the upstream's ledger.
fn key(source: &Path, id: &str) -> String {
    format!("{}#{id}", canonical(source))
}

/// The open items of `up`'s outbox in the project at `root`.
#[must_use]
pub fn items(root: &Path, up: &Declared) -> (bool, Vec<Item>) {
    match std::fs::read_to_string(root.join(&up.outbox)) {
        Ok(text) => (true, parse_outbox(&text)),
        Err(_) => (false, Vec::new()),
    }
}

/// What `root` asks of its upstreams, each item with its status read from
/// the upstream without writing to it (the last status read is kept in
/// `root`'s own ledger, for when the upstream cannot be read).
#[must_use]
pub fn asked(state: &Path, root: &Path) -> Vec<Upstream> {
    let Ok(declared) = upstreams(root) else {
        return Vec::new();
    };
    let mine = ledger(state, root);
    let mut seen: Vec<(String, Status)> = Vec::new();
    let out = declared
        .iter()
        .map(|up| {
            let (exists, list) = items(root, up);
            let theirs = up
                .path
                .as_ref()
                .filter(|p| p.is_dir())
                .map(|p| (p, ledger(state, p)));
            let tracked = list
                .into_iter()
                .map(|item| {
                    let mykey = format!("{}#{}", up.name, item.id);
                    let (status, changed) = match (&theirs, mine.issues.get(&mykey)) {
                        (Some((_, l)), _) => match l.decisions.get(&key(root, &item.id)) {
                            Some(d) => (d.status.clone(), d.digest != item.digest),
                            None => (Status::NotSeen, false),
                        },
                        (None, Some(f)) => (
                            Status::Filed {
                                repo: f.repo.clone(),
                                number: f.number,
                                issue_state: None,
                            },
                            false,
                        ),
                        (None, None) if up.path.is_some() => {
                            let why = match mine.seen.get(&mykey) {
                                Some((s, at)) => format!(
                                    "{} cannot be read; last seen {} ({} min ago)",
                                    up.name,
                                    word(s),
                                    now().saturating_sub(*at) / 60
                                ),
                                None => format!("{} cannot be read", up.name),
                            };
                            (Status::Unknown { why }, false)
                        }
                        (None, None) => (Status::NotSeen, false),
                    };
                    if !matches!(status, Status::Unknown { .. })
                        && mine.seen.get(&mykey).is_none_or(|(s, _)| *s != status)
                    {
                        seen.push((mykey, status.clone()));
                    }
                    Tracked {
                        item: clean(item),
                        status,
                        changed,
                    }
                })
                .collect();
            Upstream {
                name: up.name.clone(),
                path: up.path.as_ref().map(|p| p.display().to_string()),
                repo: up.repo.clone(),
                outbox: up.outbox.clone(),
                exists,
                items: tracked,
            }
        })
        .collect();
    if !seen.is_empty() {
        let _ = change(state, root, |l| {
            for (k, s) in seen {
                l.seen.insert(k, (s, now()));
            }
            Ok(())
        });
    }
    out
}

/// A status in a few words.
#[must_use]
pub fn word(s: &Status) -> String {
    match s {
        Status::NotSeen => "not seen yet".into(),
        Status::TakenUp { unit } if unit.is_empty() => "taken up as a spec".into(),
        Status::TakenUp { unit } => format!("taken up as {unit}"),
        Status::Bug { task } if task.is_empty() => "recorded as a bug".into(),
        Status::Bug { task } => format!("recorded as bug {task}"),
        Status::Filed {
            repo, number: 0, ..
        } => format!("being filed in {repo}"),
        Status::Fixed { commit, by_agent } => {
            format!(
                "fixed in {commit}{}",
                if *by_agent { " (by an agent)" } else { "" }
            )
        }
        Status::Declined { reason } => format!("declined — {reason}"),
        Status::Filed {
            repo,
            number,
            issue_state,
        } => format!(
            "filed as {repo}#{number}{}",
            issue_state
                .as_ref()
                .map(|s| format!(" · {s}"))
                .unwrap_or_default()
        ),
        Status::Unknown { why } => why.clone(),
    }
}

fn clean(mut i: Item) -> Item {
    let safe = |t: &str| crate::approvals::present::escape(&crate::secrets::mask(t));
    i.title = safe(&i.title);
    i.text = safe(&i.text);
    i
}

/// What the recent projects ask of the project at `here`.
#[must_use]
pub fn requests(state: &Path, here: &Path) -> Vec<Request> {
    let mine = ledger(state, here);
    let here_c = canonical(here);
    let mut out = Vec::new();
    for r in crate::welcome::recent(state) {
        let source = PathBuf::from(&r.path);
        if canonical(&source) == here_c || !source.is_dir() {
            continue;
        }
        let Ok(ups) = upstreams(&source) else {
            continue;
        };
        for up in ups
            .iter()
            .filter(|u| u.path.as_ref().is_some_and(|p| canonical(p) == here_c))
        {
            for item in items(&source, up).1 {
                let d = mine.decisions.get(&key(&source, &item.id));
                out.push(Request {
                    source: r.name.clone(),
                    path: r.path.clone(),
                    status: d.map_or(Status::NotSeen, |d| d.status.clone()),
                    changed: d.is_some_and(|d| d.digest != item.digest),
                    item: clean(item),
                });
            }
        }
    }
    out
}

/// Records this project's decision on `source`'s item `id`.
///
/// # Errors
///
/// No such request, or the ledger cannot be written.
pub fn decide(
    state: &Path,
    here: &Path,
    source: &str,
    id: &str,
    status: Status,
) -> Result<(), String> {
    let all: Vec<Request> = requests(state, here)
        .into_iter()
        .filter(|r| r.item.id == id)
        .collect();
    // A folder names one project; a name, only when no two share it.
    let by_path: Vec<&Request> = all
        .iter()
        .filter(|r| canonical(Path::new(&r.path)) == canonical(Path::new(source)))
        .collect();
    let by_name: Vec<&Request> = all.iter().filter(|r| r.source == source).collect();
    let req = match (by_path.as_slice(), by_name.as_slice()) {
        ([one], _) | ([], [one]) => *one,
        ([], []) => return Err(format!("{source} asks nothing called {id} of this project")),
        _ => {
            return Err(format!(
                "two projects called {source} ask {id}: name it by its folder"
            ));
        }
    };
    let k = key(Path::new(&req.path), id);
    let digest = req.item.digest.clone();
    change(state, here, |l| {
        l.decisions.insert(
            k,
            Decision {
                digest,
                status,
                at: now(),
            },
        );
        Ok(())
    })
}

/// An agent's word that `source`'s item `id` is fixed in `commit` of this
/// project: recorded only when the commit is in this project's history.
///
/// # Errors
///
/// No such request, or no such commit.
pub fn resolve(
    state: &Path,
    here: &Path,
    source: &str,
    id: &str,
    commit: &str,
) -> Result<String, String> {
    if commit.starts_with('-') || commit.len() < 7 || !commit.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!("`{commit}` is not a commit id (7–40 hex digits)"));
    }
    let full = crate::git::Git::at(here)
        .line(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{commit}^{{commit}}"),
        ])
        .map_err(|_| format!("{commit} is not a commit of this project"))?;
    // In this project's history: on what is checked out, not a stray one.
    crate::git::Git::at(here)
        .line(&["merge-base", "--is-ancestor", &full, "HEAD"])
        .map_err(|_| format!("{commit} is not in this project's history (HEAD)"))?;
    let short: String = full.chars().take(12).collect();
    decide(
        state,
        here,
        source,
        id,
        Status::Fixed {
            commit: short.clone(),
            by_agent: true,
        },
    )?;
    Ok(short)
}

/// `speccoder feedback`: with each filed issue's state from GitHub when
/// signed in (left unknown otherwise).
#[must_use]
pub fn report(state: &Path, root: &Path) -> CommandResult {
    let mut upstreams = asked(state, root);
    let ctx = crate::github::Ctx::from_env();
    for i in upstreams.iter_mut().flat_map(|u| u.items.iter_mut()) {
        if let Status::Filed {
            repo,
            number,
            issue_state,
        } = &mut i.status
            && *number > 0
        {
            *issue_state = crate::github::issue_state(&ctx, repo, *number).ok();
        }
    }
    let r = FeedbackReport {
        upstreams,
        requests: requests(state, root),
    };
    let open: usize = r.upstreams.iter().map(|u| u.items.len()).sum();
    let msg = format!(
        "{open} item(s) asked of {} upstream(s); {} asked of this project",
        r.upstreams.len(),
        r.requests.len()
    );
    CommandResult::new("feedback", CommandStatus::Ok, &msg)
        .with_data(CommandData::Feedback(Box::new(r)))
}

/// One writer of an outbox at a time in this process.
static OUTBOX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The version of `name` the project at `root` pins, as its manifest says
/// (`Cargo.toml`, `package.json`).
fn pinned(root: &Path, name: &str) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml"))
        && let Ok(t) = toml::from_str::<toml::Table>(&text)
    {
        let tables = ["dependencies", "dev-dependencies"]
            .iter()
            .filter_map(|k| t.get(*k))
            .chain(t.get("workspace").and_then(|w| w.get("dependencies")));
        for deps in tables {
            match deps.get(name) {
                Some(toml::Value::String(v)) => return Some(v.clone()),
                Some(toml::Value::Table(d)) => {
                    if let Some(v) = d.get("version").and_then(toml::Value::as_str) {
                        return Some(v.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    let pkg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("package.json")).ok()?).ok()?;
    ["dependencies", "devDependencies"]
        .iter()
        .find_map(|k| pkg[*k][name].as_str().map(str::to_owned))
}

/// What the project at `root` asks its upstream `upstream`, appended to its
/// outbox by an agent: the next id of the file's series, the text masked,
/// and the evidence line speccoder writes itself. A title already open is
/// not appended again.
///
/// # Errors
///
/// An undeclared upstream, an empty title, a duplicate, or a file that
/// cannot be written.
pub fn report_item(root: &Path, upstream: &str, title: &str, text: &str) -> Result<String, String> {
    let _held = OUTBOX.lock().map_err(|_| "the outbox is busy".to_owned())?;
    let ups = upstreams(root)?;
    let Some(up) = ups.iter().find(|u| u.name == upstream) else {
        let names: Vec<&str> = ups.iter().map(|u| u.name.as_str()).collect();
        return Err(format!(
            "{upstream} is not an upstream of this project (declared: {}); declare it as [upstream.{upstream}] with `path` or `repo` in speccoder.toml",
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(", ")
            }
        ));
    };
    let title = crate::secrets::mask_out(title.trim()).replace('\n', " ");
    if title.is_empty() || title.chars().count() > 120 {
        return Err("give a title of one line, at most 120 characters".into());
    }
    let file = outbox_file(root, up)?;
    let current = std::fs::read_to_string(&file).unwrap_or_default();
    if let Some(open) = parse_outbox(&current)
        .into_iter()
        .find(|i| i.title.eq_ignore_ascii_case(&title))
    {
        return Err(format!("already asked as {} — {}", open.id, open.title));
    }
    let id = speccoder_core::feedback::next_id(&current);
    let project = root
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let commit = crate::git::Git::at(root)
        .line(&["rev-parse", "--short=7", "HEAD"])
        .unwrap_or_default();
    let mut evidence = format!("Reported by an agent · {project}@{commit}");
    if let Some(v) = pinned(root, upstream) {
        evidence.push_str(&format!(" · pins {upstream} {v}"));
    }
    let body = crate::secrets::mask_out(text.trim());
    let out = speccoder_core::feedback::append(&current, &id, &title, &body, &evidence);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    write_outbox(&file, &out)?;
    Ok(id)
}

/// Removes item `id` from the outbox for `upstream`.
///
/// # Errors
///
/// An undeclared upstream, no such item, or a file that cannot be written.
pub fn withdraw(root: &Path, upstream: &str, id: &str) -> Result<(), String> {
    let _held = OUTBOX.lock().map_err(|_| "the outbox is busy".to_owned())?;
    let up = upstreams(root)?
        .into_iter()
        .find(|u| u.name == upstream)
        .ok_or_else(|| format!("{upstream} is not an upstream of this project"))?;
    let file = outbox_file(root, &up)?;
    let current = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let out = speccoder_core::feedback::remove(&current, id)
        .ok_or_else(|| format!("{id} is not in {}", up.outbox))?;
    write_outbox(&file, &out)
}

/// The outbox written whole (a temporary file, then renamed): a reader
/// never sees half of it.
fn write_outbox(file: &Path, text: &str) -> Result<(), String> {
    let tmp = file.with_extension(format!("md.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", file.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("{}: {e}", file.display()))
}

/// An item prepared as a GitHub issue for its upstream's repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prepared {
    /// `owner/name`.
    pub repo: String,
    /// The issue's title.
    pub title: String,
    /// Its body: the item's text and where it comes from.
    pub body: String,
    /// GitHub's new-issue page with both filled in (no sign-in needed).
    pub url: String,
    /// Filed already: its number.
    pub filed: Option<u64>,
}

/// The longest address a browser is handed; a longer body is shortened.
const MAX_URL: usize = 7000;

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Item `id` of the outbox for `upstream`, prepared as an issue.
///
/// # Errors
///
/// No such upstream or item, or the upstream has no repository.
pub fn prepared(state: &Path, root: &Path, upstream: &str, id: &str) -> Result<Prepared, String> {
    let up = upstreams(root)?
        .into_iter()
        .find(|u| u.name == upstream)
        .ok_or_else(|| format!("{upstream} is not an upstream of this project"))?;
    let repo = up
        .repo
        .clone()
        .ok_or_else(|| format!("[upstream.{upstream}] names no `repo` (owner/name)"))?;
    let item = items(root, &up)
        .1
        .into_iter()
        .find(|i| i.id == id)
        .map(clean)
        .ok_or_else(|| format!("{id} is not open in {}", up.outbox))?;
    let project = root
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let commit = crate::git::Git::at(root)
        .line(&["rev-parse", "--short=7", "HEAD"])
        .unwrap_or_default();
    let mut footer = format!("From {project} ({id}) at {commit}");
    if let Some(v) = pinned(root, upstream) {
        footer.push_str(&format!(", which pins {upstream} {v}"));
    }
    let body = format!("{}\n\n---\n_{footer}. Prepared with speccoder._", item.text);
    let base = format!(
        "https://github.com/{repo}/issues/new?title={}&body=",
        encode(&item.title)
    );
    let mut url = format!("{base}{}", encode(&body));
    if url.len() > MAX_URL {
        // Shortened for the address; File Issue sends it whole.
        let note = "\n\n… (shortened: the whole text is in the outbox)";
        let mut cut = body.clone();
        while format!("{base}{}", encode(&format!("{cut}{note}"))).len() > MAX_URL
            && !cut.is_empty()
        {
            let keep = cut.chars().count() * 9 / 10;
            cut = cut.chars().take(keep).collect();
        }
        url = format!("{base}{}", encode(&format!("{cut}{note}")));
    }
    let filed = ledger(state, root)
        .issues
        .get(&format!("{upstream}#{id}"))
        .map(|f| f.number);
    Ok(Prepared {
        repo,
        title: item.title,
        body,
        url,
        filed,
    })
}

/// Files item `id` as an issue with the person's GitHub sign-in, once.
///
/// # Errors
///
/// Filed already, not signed in, or GitHub's refusal.
pub fn file_issue(
    ctx: &crate::github::Ctx,
    state: &Path,
    root: &Path,
    upstream: &str,
    id: &str,
) -> Result<(u64, String), String> {
    let p = prepared(state, root, upstream, id)?;
    let k = format!("{upstream}#{id}");
    // Claimed before it is sent (number 0: being filed), so a second click
    // or tab is refused; released when GitHub refuses.
    change(state, root, |l| match l.issues.get(&k) {
        Some(f) if f.number == 0 => Err(format!("{id} is being filed")),
        Some(f) => Err(format!("{id} is filed already as {}#{}", f.repo, f.number)),
        None => {
            l.issues.insert(
                k.clone(),
                Filed {
                    repo: p.repo.clone(),
                    number: 0,
                },
            );
            Ok(())
        }
    })?;
    match crate::github::create_issue(ctx, &p.repo, &p.title, &p.body) {
        Ok((n, url)) => {
            change(state, root, |l| {
                l.issues.insert(
                    k.clone(),
                    Filed {
                        repo: p.repo.clone(),
                        number: n,
                    },
                );
                Ok(())
            })?;
            Ok((n, url))
        }
        Err(e) => {
            let _ = change(state, root, |l| {
                l.issues.remove(&k);
                Ok(())
            });
            Err(e)
        }
    }
}
