//! A checkout's agent files (spec 126): the one table of what a checkout
//! tells agents — instructions, skills, each executor's project settings —
//! and a person's trust in each, bound to its bytes.
//!
//! Trust lives in the hooks' trust record (key `checkout:<path>`, the path
//! relative to the repository's top), never in the checkout. A record keeps
//! the text last trusted (up to [`TEXT_CAP`]), so a change shows as a diff
//! and speccoder's model can be given the trusted bytes while a changed
//! file waits, and every digest trusted for that path, so your checkout and
//! a feature's worktree may each hold a version you trusted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use speccoder_core::agent_files::{
    AgentFile, AgentFileKind, AgentFileState, Reader, TrustReport, TrustedBy, capped,
};

use crate::hooks::trust as store;
use crate::integrity::sha256_hex;

/// The most text kept per trusted file (larger ones are trusted by digest,
/// their diff cut).
pub const TEXT_CAP: usize = 256 * 1024;

/// The record's digest for a file trusted as gone.
const GONE: &str = "gone";

/// Earlier digests kept per path, besides the one in force.
const KEPT: usize = 16;

/// Folders never walked: they hold others' code, not this checkout's
/// instructions (no executor reads agent files from them).
const SKIP: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".tox",
    "vendor",
];

/// Where a file of the table is.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// This file name, in any directory.
    Named(&'static str),
    /// This repository-relative path.
    At(&'static str),
    /// Every file under this repository-relative directory.
    Under(&'static str),
}

use Place::{At, Named, Under};
use Reader::{Claude, Codex, Copilot, Gemini, Opencode, Speccoder};

/// The table (research R4): what each reader loads from a checkout itself.
/// speccoder's own instruction names come from `[context] instructions`.
/// The most specific row a path matches decides its kind.
const TABLE: &[(Place, AgentFileKind, &[Reader])] = &[
    (
        Named("AGENTS.md"),
        AgentFileKind::Instructions,
        &[Codex, Opencode, Copilot],
    ),
    (
        Named("AGENTS.override.md"),
        AgentFileKind::Instructions,
        &[Codex],
    ),
    (
        Named("CLAUDE.md"),
        AgentFileKind::Instructions,
        &[Claude, Opencode],
    ),
    (
        Named("CLAUDE.local.md"),
        AgentFileKind::Instructions,
        &[Claude],
    ),
    (Named("GEMINI.md"), AgentFileKind::Instructions, &[Gemini]),
    (
        At(".github/copilot-instructions.md"),
        AgentFileKind::Instructions,
        &[Copilot],
    ),
    (
        Under(".github/instructions"),
        AgentFileKind::Instructions,
        &[Copilot],
    ),
    (
        Under(".github/prompts"),
        AgentFileKind::Instructions,
        &[Copilot],
    ),
    (
        Under(".github/agents"),
        AgentFileKind::Instructions,
        &[Copilot],
    ),
    (Under(".github/hooks"), AgentFileKind::Settings, &[Copilot]),
    (Under(".claude"), AgentFileKind::Settings, &[Claude]),
    (
        Under(".claude/skills"),
        AgentFileKind::Skill,
        &[Speccoder, Claude, Opencode],
    ),
    (
        Under(".claude/agents"),
        AgentFileKind::Instructions,
        &[Claude],
    ),
    (
        Under(".claude/commands"),
        AgentFileKind::Instructions,
        &[Claude],
    ),
    (
        Under(".claude/rules"),
        AgentFileKind::Instructions,
        &[Claude],
    ),
    (
        Under(".claude/output-styles"),
        AgentFileKind::Instructions,
        &[Claude],
    ),
    (
        Under(".agents/skills"),
        AgentFileKind::Skill,
        &[Speccoder, Codex],
    ),
    (At(".mcp.json"), AgentFileKind::Settings, &[Claude]),
    (Under(".codex"), AgentFileKind::Settings, &[Codex]),
    (Under(".gemini"), AgentFileKind::Settings, &[Gemini]),
    (At("opencode.json"), AgentFileKind::Settings, &[Opencode]),
    (At("opencode.jsonc"), AgentFileKind::Settings, &[Opencode]),
    (Under(".opencode"), AgentFileKind::Settings, &[Opencode]),
];

/// The trust record's key for `path`.
fn key(path: &str) -> String {
    format!("checkout:{path}")
}

/// What a path is in the table.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    kind: AgentFileKind,
    readers: Vec<Reader>,
}

/// What `rel` is in the table (with `names`, speccoder's instruction
/// names): readers merged over every row it matches, the kind of the most
/// specific one.
fn classify(rel: &str, names: &[String]) -> Option<Found> {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let mut out: Option<(usize, Found)> = None;
    let mut add = |weight: usize, kind: AgentFileKind, readers: &[Reader]| {
        let (w, f) = out.get_or_insert((
            weight,
            Found {
                kind,
                readers: Vec::new(),
            },
        ));
        if weight >= *w {
            *w = weight;
            f.kind = kind;
        }
        for r in readers {
            if !f.readers.contains(r) {
                f.readers.push(*r);
            }
        }
    };
    if names.iter().any(|n| n == file) {
        add(1, AgentFileKind::Instructions, &[Speccoder]);
    }
    for (place, kind, readers) in TABLE {
        match place {
            Named(n) if *n == file => add(1, *kind, readers),
            At(p) if *p == rel => add(p.len() + 1, *kind, readers),
            Under(d) if rel.strip_prefix(d).is_some_and(|r| r.starts_with('/')) => {
                add(d.len(), *kind, readers);
            }
            _ => {}
        }
    }
    out.map(|(_, mut f)| {
        f.readers.sort();
        f
    })
}

/// Whether a link at `rel` reaches where the table looks: itself an agent
/// file's place, or a folder above one (`.claude` linked elsewhere).
fn agent_location(rel: &str, names: &[String]) -> bool {
    classify(rel, names).is_some()
        || TABLE.iter().any(|(p, _, _)| match p {
            At(x) | Under(x) => x.strip_prefix(rel).is_some_and(|r| r.starts_with('/')),
            Named(_) => false,
        })
}

/// What a file holds: its bytes, or where a link points (never followed).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Bytes(Vec<u8>),
    Link(String),
}

impl Content {
    fn digest(&self) -> String {
        match self {
            Self::Bytes(b) => format!("sha256:{}", sha256_hex(b)),
            Self::Link(to) => format!("link:{to}"),
        }
    }

    fn shown(&self) -> String {
        match self {
            Self::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Self::Link(to) => format!("a link to {to}\n"),
        }
    }
}

/// Every agent file in the tree at `top` (no ignore rule applies — an
/// agent reads an ignored file too; [`SKIP`] folders are not walked; a
/// link is never followed and counts where the table looks).
fn walk(top: &Path, names: &[String]) -> BTreeMap<String, (Found, Content)> {
    let mut out = BTreeMap::new();
    let walker = ignore::WalkBuilder::new(top)
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(|e| {
            !(e.depth() > 0
                && e.file_type().is_some_and(|t| t.is_dir())
                && SKIP.iter().any(|s| e.file_name() == *s))
        })
        .build();
    for entry in walker.flatten() {
        if entry.depth() == 0 {
            continue;
        }
        let Some(t) = entry.file_type() else {
            continue;
        };
        let Ok(rel) = entry.path().strip_prefix(top) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if t.is_symlink() {
            if agent_location(&rel, names) {
                let to = std::fs::read_link(entry.path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let found = classify(&rel, names).unwrap_or(Found {
                    kind: AgentFileKind::Settings,
                    readers: Reader::ALL.to_vec(),
                });
                out.insert(rel, (found, Content::Link(to)));
            }
            continue;
        }
        if !t.is_file() {
            continue;
        }
        if let Some(f) = classify(&rel, names)
            && let Ok(bytes) = std::fs::read(entry.path())
        {
            out.insert(rel, (f, Content::Bytes(bytes)));
        }
    }
    out
}

/// The instruction names `[context] instructions` sets at `root`.
#[must_use]
pub fn names_at(root: &Path) -> Vec<String> {
    let text = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    crate::substrate::run_config::parse(&text)
        .map(|c| c.instructions)
        .unwrap_or_else(|_| vec!["AGENTS.md".to_owned()])
}

/// The repository top containing `root` (keys are relative to it), else
/// `root` itself.
fn top_of(root: &Path) -> PathBuf {
    let top = crate::git::Git::at(root)
        .toplevel()
        .ok()
        .flatten()
        .unwrap_or_else(|| root.to_path_buf());
    std::fs::canonicalize(&top).unwrap_or(top)
}

/// The trust record's directory for the repository at `top`.
fn trust_dir(state: &Path, top: &Path) -> PathBuf {
    crate::hooks::dir(state, top)
}

/// What a path's record says: every digest trusted, the text last trusted,
/// and who trusted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trusted {
    /// Every digest trusted for the path; the one in force first.
    pub digests: Vec<String>,
    /// The text last trusted, if kept.
    pub text: Option<String>,
    by: TrustedBy,
    gone: bool,
}

fn records(state: &Path, top: &Path) -> BTreeMap<String, Trusted> {
    store::read(&trust_dir(state, top))
        .into_iter()
        .filter_map(|(k, e)| {
            let path = k.strip_prefix("checkout:")?.to_owned();
            let d = &e.current.definition;
            let mut digests = vec![e.current.digest.clone()];
            digests.extend(
                d["digests"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(str::to_owned)),
            );
            Some((
                path,
                Trusted {
                    gone: e.current.digest == GONE,
                    digests,
                    text: d["text"].as_str().map(str::to_owned),
                    by: TrustedBy {
                        by: e.current.by.clone(),
                        via: e.current.via.clone(),
                        at: e.current.at.clone(),
                    },
                },
            ))
        })
        .collect()
}

/// What the record of `path` in the repository containing `root` says, if
/// any (`None`: never trusted, or trusted as gone).
#[must_use]
pub fn trusted(state: &Path, root: &Path, path: &str) -> Option<Trusted> {
    records(state, &top_of(root))
        .remove(path)
        .filter(|t| !t.gone)
}

/// A unified diff `before` → `now`.
fn unified(path: &str, before: &str, now: &str) -> String {
    similar::TextDiff::from_lines(before, now)
        .unified_diff()
        .context_radius(3)
        .header(&format!("{path} (trusted)"), &format!("{path} (now)"))
        .to_string()
}

/// The feature worktrees speccoder made for the repository at `top`, by
/// feature name.
fn worktrees(top: &Path) -> Vec<(String, PathBuf)> {
    let Some(state) = crate::worktree::state_dir() else {
        return Vec::new();
    };
    let state = std::fs::canonicalize(&state).unwrap_or(state);
    crate::git::Git::at(top)
        .run(&["worktree", "list", "--porcelain", "-z"])
        .map(|o| crate::git::nul_fields(&o))
        .unwrap_or_default()
        .iter()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|p| p.starts_with(&state) && p.is_dir())
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((name, p))
        })
        .collect()
}

fn state_of(t: Option<&Trusted>, digest: &str) -> AgentFileState {
    match t {
        None => AgentFileState::Untrusted,
        Some(t) if t.digests.iter().any(|d| d == digest) => AgentFileState::Trusted,
        Some(_) => AgentFileState::Changed,
    }
}

fn file(
    path: &str,
    found: &Found,
    content: &Content,
    t: Option<&Trusted>,
    tree: Option<String>,
    diffs: bool,
) -> AgentFile {
    let digest = content.digest();
    let state = state_of(t, &digest);
    let now = content.shown();
    let diff = (diffs && state.waits()).then(|| match (state, t.and_then(|t| t.text.as_ref())) {
        (AgentFileState::Changed, Some(before)) => capped(&unified(path, before, &now)),
        _ => capped(&now),
    });
    AgentFile {
        path: path.to_owned(),
        kind: found.kind,
        readers: found.readers.clone(),
        digest,
        bytes: match content {
            Content::Bytes(b) => b.len() as u64,
            Content::Link(_) => 0,
        },
        state,
        trusted: t.map(|t| t.by.clone()),
        tree,
        diff,
    }
}

/// Every agent file of the repository containing `root` with its state and
/// what changed: your checkout's, then each feature worktree's that differs
/// and waits.
#[must_use]
pub fn report(root: &Path, state: &Path) -> TrustReport {
    report_with(&top_of(root), state, true)
}

/// [`report`] without the diffs, for Needs You and gates; kept for a
/// moment, as Needs You is read often.
#[must_use]
pub fn states(root: &Path, state: &Path) -> TrustReport {
    type Kept = (PathBuf, PathBuf, Instant, u64, TrustReport);
    static MEMO: Mutex<Vec<Kept>> = Mutex::new(Vec::new());
    let top = top_of(root);
    let fresh = Duration::from_secs(3);
    let generation = GENERATION.load(Ordering::Relaxed);
    let valid = |(t, s, at, g, _): &Kept| {
        *t == top && s == state && at.elapsed() < fresh && *g == generation
    };
    if let Ok(m) = MEMO.lock()
        && let Some((.., r)) = m.iter().find(|k| valid(k))
    {
        return r.clone();
    }
    let r = report_with(&top, state, false);
    if let Ok(mut m) = MEMO.lock() {
        m.retain(|(t, s, at, _, _)| !(*t == top && s == state) && at.elapsed() < fresh);
        m.push((
            top,
            state.to_path_buf(),
            Instant::now(),
            generation,
            r.clone(),
        ));
    }
    r
}

/// Bumped by every record: what [`states`] kept is read afresh.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn report_with(top: &Path, state: &Path, diffs: bool) -> TrustReport {
    let names = names_at(top);
    let recs = records(state, top);
    let here = walk(top, &names);
    let mut files: Vec<AgentFile> = here
        .iter()
        .map(|(p, (f, c))| file(p, f, c, recs.get(p), None, diffs))
        .collect();
    let mut seen: BTreeSet<(String, String)> = files
        .iter()
        .map(|f| (f.path.clone(), f.digest.clone()))
        .collect();
    for (name, wt) in worktrees(top) {
        for (p, (f, c)) in walk(&wt, &names) {
            let x = file(&p, &f, &c, recs.get(&p), Some(name.clone()), diffs);
            if x.state.waits() && seen.insert((x.path.clone(), x.digest.clone())) {
                files.push(x);
            }
        }
    }
    // Trusted once, gone now (and not trusted as gone).
    for (path, t) in &recs {
        if t.gone || here.contains_key(path) || top.join(path).exists() {
            continue;
        }
        let Some(f) = classify(path, &names) else {
            continue;
        };
        files.push(AgentFile {
            path: path.clone(),
            kind: f.kind,
            readers: f.readers,
            digest: GONE.to_owned(),
            bytes: 0,
            state: AgentFileState::Gone,
            trusted: Some(t.by.clone()),
            tree: None,
            diff: None,
        });
    }
    TrustReport::new(files)
}

/// The paths of the tree at `root` waiting for trust that `reader` loads
/// (`None`: any reader but speccoder — a command executor reads what it
/// likes). Only that tree: an attempt's gate.
#[must_use]
pub fn blocking(root: &Path, state: &Path, reader: Option<Reader>) -> Vec<String> {
    let top = top_of(root);
    let recs = records(state, &top);
    walk(&top, &names_at(&top))
        .into_iter()
        .filter(|(p, (_, c))| state_of(recs.get(p), &c.digest()).waits())
        .filter(|(_, (f, _))| match reader {
            Some(r) => f.readers.contains(&r),
            None => f.readers.iter().any(|r| *r != Reader::Speccoder),
        })
        .map(|(p, _)| p)
        .collect()
}

/// For each repository skill folder (`.claude/skills/x/`, relative to the
/// top), whether it may be hosted: no link on its way, and every file in it
/// trusted as it is now. One walk, read now — never from a kept report.
#[must_use]
pub fn skills_trusted(root: &Path, state: &Path, folders: &[&str]) -> Vec<bool> {
    let top = top_of(root);
    let recs = records(state, &top);
    let all = walk(&top, &names_at(&top));
    folders
        .iter()
        .map(|folder| {
            let dir = folder.trim_end_matches('/');
            let link = Path::new(dir)
                .ancestors()
                .filter(|a| !a.as_os_str().is_empty())
                .any(|a| {
                    std::fs::symlink_metadata(top.join(a)).is_ok_and(|m| m.file_type().is_symlink())
                });
            let inside: Vec<(&String, &Content)> = all
                .iter()
                .filter(|(p, _)| p.strip_prefix(dir).is_some_and(|r| r.starts_with('/')))
                .map(|(p, (_, c))| (p, c))
                .collect();
            !link
                && !inside.is_empty()
                && inside
                    .iter()
                    .all(|(p, c)| state_of(recs.get(*p), &c.digest()) == AgentFileState::Trusted)
        })
        .collect()
}

/// Records trust in `path` as `content` (read once, by the caller), keeping
/// every digest trusted before.
fn record(top: &Path, state: &Path, path: &str, content: Option<&Content>, by: &str, via: &str) {
    let dir = trust_dir(state, top);
    let before = records(state, top).remove(path);
    let mut digests: Vec<String> = before
        .map(|t| t.digests.into_iter().filter(|d| d != GONE).collect())
        .unwrap_or_default();
    let (digest, mut definition) = match content {
        Some(Content::Bytes(bytes)) => {
            let text = String::from_utf8_lossy(bytes);
            let mut end = text.len().min(TEXT_CAP);
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            (
                Content::Bytes(bytes.clone()).digest(),
                serde_json::json!({ "text": &text[..end], "bytes": bytes.len(), "cut": text.len() > TEXT_CAP }),
            )
        }
        Some(c @ Content::Link(to)) => (c.digest(), serde_json::json!({ "link": to })),
        None => (GONE.to_owned(), serde_json::json!({})),
    };
    digests.retain(|d| *d != digest);
    digests.truncate(KEPT);
    if digest != GONE {
        definition["digests"] = serde_json::json!(digests);
    }
    store::record_as(&dir, &key(path), &digest, definition, by, via);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Why trusting refused: the files that changed since they were shown,
/// or paths that are not agent files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// These changed after the person saw them.
    Changed(Vec<String>),
    /// These are not agent files of this checkout.
    Unknown(Vec<String>),
}

/// Every version of every agent file now, across your checkout and the
/// feature worktrees, by path and digest — read once.
fn versions(
    top: &Path,
) -> (
    BTreeMap<String, Content>,
    BTreeMap<(String, String), Content>,
) {
    let names = names_at(top);
    let here: BTreeMap<String, Content> = walk(top, &names)
        .into_iter()
        .map(|(p, (_, c))| (p, c))
        .collect();
    let mut all: BTreeMap<(String, String), Content> = here
        .iter()
        .map(|(p, c)| ((p.clone(), c.digest()), c.clone()))
        .collect();
    for (_, wt) in worktrees(top) {
        for (p, (_, c)) in walk(&wt, &names) {
            all.entry((p, c.digest())).or_insert(c);
        }
    }
    (here, all)
}

/// Trusts each `(path, digest)` the person saw, only if that version is
/// still there (in your checkout or a feature's worktree); an empty digest
/// trusts your checkout's file as it is now, `gone` its absence. Each file
/// is read once and those bytes recorded.
///
/// # Errors
///
/// A file changed since it was shown, or a path is not an agent file:
/// nothing is recorded.
pub fn trust(
    root: &Path,
    state: &Path,
    files: &[(String, String)],
    by: &str,
    via: &str,
) -> Result<Vec<String>, Refused> {
    let top = top_of(root);
    let (here, all) = versions(&top);
    let recs = records(state, &top);
    let known = |p: &str| {
        here.contains_key(p)
            || all.keys().any(|(x, _)| x == p)
            || recs.get(p).is_some_and(|t| !t.gone)
    };
    let unknown: Vec<String> = files
        .iter()
        .filter(|(p, _)| !known(p))
        .map(|(p, _)| p.clone())
        .collect();
    if !unknown.is_empty() {
        return Err(Refused::Unknown(unknown));
    }
    let mut chosen: Vec<(String, Option<Content>)> = Vec::new();
    let mut changed = Vec::new();
    for (p, d) in files {
        let pick = match d.as_str() {
            "" => Some(here.get(p).cloned()),
            GONE => (!here.contains_key(p)).then_some(None),
            d => all.get(&(p.clone(), d.to_owned())).cloned().map(Some),
        };
        match pick {
            Some(c) => chosen.push((p.clone(), c)),
            None => changed.push(p.clone()),
        }
    }
    if !changed.is_empty() {
        return Err(Refused::Changed(changed));
    }
    for (p, c) in &chosen {
        record(&top, state, p, c.as_ref(), by, via);
    }
    Ok(chosen.into_iter().map(|(p, _)| p).collect())
}

/// [`trust`] by the person speccoder knows in `root` (git's user).
///
/// # Errors
///
/// As [`trust`].
pub fn trust_as_person(
    root: &Path,
    state: &Path,
    files: &[(String, String)],
    via: &str,
) -> Result<Vec<String>, Refused> {
    trust(root, state, files, &crate::approvals::who_in(root), via)
}

/// Trusts every waiting agent file as it is now, in your checkout and the
/// feature worktrees.
#[must_use]
pub fn trust_all(root: &Path, state: &Path, by: &str, via: &str) -> Vec<String> {
    let waiting: Vec<(String, String)> = report_with(&top_of(root), state, false)
        .files
        .into_iter()
        .filter(|f| f.state.waits())
        .map(|f| (f.path, f.digest))
        .collect();
    trust(root, state, &waiting, by, via).unwrap_or_default()
}

/// Records the agent files speccoder itself wrote in `root` (a new
/// project, the sample) as trusted *via speccoder*. Paths that are not
/// agent files are skipped.
pub fn record_written(root: &Path, state: &Path, paths: &[&str]) {
    let top = top_of(root);
    let all = walk(&top, &names_at(&top));
    for p in paths {
        if let Some((_, c)) = all.get(*p) {
            record(&top, state, p, Some(c), "speccoder", "speccoder");
        }
    }
}

/// Records every agent file now in `root` as written by speccoder: for a
/// project speccoder just created, where nothing came from elsewhere.
pub fn record_all_written(root: &Path, state: &Path) {
    let top = top_of(root);
    for (p, (_, c)) in walk(&top, &names_at(&top)) {
        record(&top, state, &p, Some(&c), "speccoder", "speccoder");
    }
}

/// `p` (relative to `base`, the command's folder) relative to `top`.
fn relative_to_top(base: &Path, top: &Path, p: &str) -> String {
    let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let mut out = PathBuf::new();
    for c in base.join(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out.strip_prefix(top)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| p.to_owned())
}

/// `speccoder trust [--all | PATH…]`: lists the agent files of the project
/// at `path`, or trusts them as they are now (a PATH relative to `path`).
#[must_use]
pub fn command(
    path: &Path,
    state: &Path,
    all: bool,
    paths: &[String],
) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandData, CommandResult, CommandStatus};
    let root = top_of(path);
    let by = crate::approvals::who_in(&root);
    let done: Result<Vec<String>, Refused> = if all {
        Ok(trust_all(&root, state, &by, "terminal"))
    } else if paths.is_empty() {
        Ok(Vec::new())
    } else {
        let files: Vec<(String, String)> = paths
            .iter()
            .map(|p| (relative_to_top(path, &root, p), String::new()))
            .collect();
        trust(&root, state, &files, &by, "terminal")
    };
    let report = states(&root, state);
    let data = CommandData::Trust(Box::new(report.clone()));
    match done {
        Err(Refused::Unknown(p)) => CommandResult::new(
            "trust",
            CommandStatus::UsageError,
            &format!("not an agent file of this checkout: {}", p.join(", ")),
        )
        .with_data(data),
        Err(Refused::Changed(p)) => CommandResult::new(
            "trust",
            CommandStatus::NeedsHuman,
            &format!("changed since shown: {}", p.join(", ")),
        )
        .with_data(data),
        Ok(done) if !done.is_empty() => CommandResult::new(
            "trust",
            CommandStatus::Ok,
            &format!("trusted {} as {} now", done.join(", "), if done.len() == 1 { "it is" } else { "they are" }),
        )
        .with_data(data),
        Ok(_) if report.files.is_empty() => CommandResult::new(
            "trust",
            CommandStatus::Ok,
            "no agent files in this checkout",
        )
        .with_data(data),
        Ok(_) if report.waiting == 0 => CommandResult::new(
            "trust",
            CommandStatus::Ok,
            &format!(
                "{} agent {}, every one trusted as it is now",
                report.files.len(),
                if report.files.len() == 1 { "file" } else { "files" }
            ),
        )
        .with_data(data),
        Ok(_) => CommandResult::new(
            "trust",
            CommandStatus::Ok,
            &format!(
                "{} — agents read only what you trusted; review the changes in the app (Needs You), or trust them as they are now with `speccoder trust --all`",
                report.title().unwrap_or_default()
            ),
        )
        .with_data(data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_merges_readers_and_the_most_specific_row_decides_the_kind() {
        let names = vec!["AGENTS.md".to_owned()];
        let a = classify("AGENTS.md", &names).unwrap_or_else(|| panic!("AGENTS.md"));
        assert_eq!(a.kind, AgentFileKind::Instructions);
        assert_eq!(a.readers, vec![Speccoder, Codex, Opencode, Copilot]);
        let s = classify(".claude/skills/x/SKILL.md", &names).unwrap_or_else(|| panic!("skill"));
        assert_eq!(s.kind, AgentFileKind::Skill);
        assert_eq!(s.readers, vec![Speccoder, Claude, Opencode]);
        let r = classify(".claude/rules/x.md", &names).unwrap_or_else(|| panic!("rule"));
        assert_eq!(r.kind, AgentFileKind::Instructions);
        let st = classify(".claude/settings.json", &names).unwrap_or_else(|| panic!("settings"));
        assert_eq!(st.kind, AgentFileKind::Settings);
        let nested = classify("src/CLAUDE.md", &names).unwrap_or_else(|| panic!("nested"));
        assert_eq!(nested.readers, vec![Claude, Opencode]);
        assert!(classify("README.md", &names).is_none());
        assert!(classify(".claude-skills/x", &names).is_none());
        let custom =
            classify("docs/RULES.md", &["RULES.md".to_owned()]).unwrap_or_else(|| panic!("custom"));
        assert_eq!(custom.readers, vec![Speccoder]);
        assert!(agent_location(".claude", &names));
        assert!(agent_location(".github", &names));
        assert!(!agent_location("docs", &names));
    }

    #[test]
    fn a_path_is_made_relative_to_the_top_from_where_the_command_runs() {
        let t = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let top = std::fs::canonicalize(t.path()).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir_all(top.join("docs")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            relative_to_top(&top.join("docs"), &top, "CLAUDE.md"),
            "docs/CLAUDE.md"
        );
        assert_eq!(
            relative_to_top(&top.join("docs"), &top, "../AGENTS.md"),
            "AGENTS.md"
        );
        assert_eq!(relative_to_top(&top, &top, "./AGENTS.md"), "AGENTS.md");
    }
}
