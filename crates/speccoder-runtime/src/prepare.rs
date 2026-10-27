//! A run's worktree gets its dependency folders (`[worktree]`).
//!
//! A worktree holds only what is committed, and dependency folders never
//! are (`node_modules/`, `.venv/`): a check that needs them cannot start
//! there. Before the baseline each Node or uv root of the project is
//! prepared once per lockfile — its folder linked from the person's
//! checkout (`[worktree] link`) where that is safe, else installed with the
//! toolchain's own locked, offline install, sandboxed. A record beside the
//! worktree makes it once per lockfile digest.
//!
//! A link is safe only when the folder is the checkout's own and ignored
//! there, the worktree's lockfile is byte-identical to the checkout's,
//! commands are sandboxed (the link's target lies outside the writable set,
//! so it is read-only to every command), and — for a `.venv` — the
//! project's package imports from the worktree. Linking writes one thing
//! outside the worktree: anchored entries in the repository's shared
//! `info/exclude`, the only exclude file git reads for a worktree, so the
//! link is never seen by git there. Nothing is ever deleted through a link.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::git::{Git, nul_fields};
use crate::sandbox::Runner;
use crate::verify::process::{CommandRunner as _, Invocation, OUTPUT_CAP, Outcome, scrubbed_env};

/// How long one install may take.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

/// How long the import probe may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The comment heading speccoder's entries in the shared `info/exclude`.
pub const EXCLUDE_MARK: &str = "# speccoder: [worktree] link";

/// `[worktree]` in `speccoder.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `prepare = "auto"` (the default): prepare Node and uv roots before
    /// the baseline; `"off"`: never.
    pub prepare: bool,
    /// Folders linked from the checkout instead of installed,
    /// project-relative (`node_modules`, `api/.venv`).
    pub link: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prepare: true,
            link: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    prepare: Option<String>,
    #[serde(default)]
    link: Vec<String>,
}

/// Reads `[worktree]`.
///
/// # Errors
///
/// An unknown key, a `prepare` other than `auto` or `off`, or a `link`
/// entry that is not a relative path inside the project.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(v) = table.get("worktree") else {
        return Ok(Config::default());
    };
    let raw: Raw = v
        .clone()
        .try_into()
        .map_err(|e| format!("[worktree]: {e}"))?;
    let prepare = match raw.prepare.as_deref() {
        None | Some("auto") => true,
        Some("off") => false,
        Some(o) => return Err(format!("[worktree] prepare = \"{o}\" (auto or off)")),
    };
    let mut link = Vec::new();
    for p in &raw.link {
        let p = link_path(p).map_err(|why| format!("[worktree] link: {why}"))?;
        if !link.contains(&p) {
            link.push(p);
        }
    }
    Ok(Config { prepare, link })
}

/// A `link` entry as stored (no trailing `/`), or why it is refused.
fn link_path(p: &str) -> Result<String, String> {
    let p = p.trim_end_matches('/');
    if p.is_empty() {
        return Err("an empty path".to_owned());
    }
    if p.starts_with('/') || p.starts_with('~') {
        return Err(format!("`{p}` is not relative to the project"));
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts.contains(&"..") {
        return Err(format!("`{p}` leaves the project"));
    }
    if parts.iter().any(|c| c.is_empty() || *c == ".") {
        return Err(format!("`{p}` is not a plain relative path"));
    }
    if parts.contains(&".git") {
        return Err(format!("`{p}` is git's own"));
    }
    Ok(p.to_owned())
}

/// A toolchain whose project needs a dependency folder in the tree its
/// checks run in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Toolchain {
    /// npm (`package-lock.json`).
    Npm,
    /// pnpm (`pnpm-lock.yaml`).
    Pnpm,
    /// Yarn 1 (`yarn.lock`).
    Yarn,
    /// Yarn 2+ (a `yarn.lock` with `__metadata`).
    YarnBerry,
    /// bun (`bun.lock`, `bun.lockb`).
    Bun,
    /// uv (`uv.lock`).
    Uv,
}

/// The lockfiles, in the order one is chosen when a directory has several.
const LOCKFILES: &[(&str, Toolchain)] = &[
    ("package-lock.json", Toolchain::Npm),
    ("pnpm-lock.yaml", Toolchain::Pnpm),
    ("yarn.lock", Toolchain::Yarn),
    ("bun.lock", Toolchain::Bun),
    ("bun.lockb", Toolchain::Bun),
    ("uv.lock", Toolchain::Uv),
];

impl Toolchain {
    /// The name in the record and the run log.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::YarnBerry => "yarn-berry",
            Self::Bun => "bun",
            Self::Uv => "uv",
        }
    }

    /// The folder it installs into, beside its lockfile.
    #[must_use]
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Uv => ".venv",
            _ => "node_modules",
        }
    }

    /// The manifest its lockfile sits beside.
    #[must_use]
    pub const fn manifest(self) -> &'static str {
        match self {
            Self::Uv => "pyproject.toml",
            _ => "package.json",
        }
    }

    /// Its locked install: offline from the toolchain's cache, or the
    /// online form (which `[sandbox] network_allow` may let reach the
    /// network). bun and Yarn 2+ have no offline flag: they read their
    /// cache first.
    #[must_use]
    pub fn install(self, offline: bool) -> Vec<String> {
        let w: &[&str] = match (self, offline) {
            (Self::Npm, true) => &["npm", "ci", "--offline"],
            (Self::Npm, false) => &["npm", "ci"],
            (Self::Pnpm, true) => &["pnpm", "install", "--frozen-lockfile", "--offline"],
            (Self::Pnpm, false) => &["pnpm", "install", "--frozen-lockfile"],
            (Self::Yarn, true) => &["yarn", "install", "--frozen-lockfile", "--offline"],
            (Self::Yarn, false) => &["yarn", "install", "--frozen-lockfile"],
            (Self::YarnBerry, _) => &["yarn", "install", "--immutable"],
            (Self::Bun, _) => &["bun", "install", "--frozen-lockfile"],
            (Self::Uv, true) => &["uv", "sync", "--locked", "--offline"],
            (Self::Uv, false) => &["uv", "sync", "--locked"],
        };
        w.iter().map(|s| (*s).to_owned()).collect()
    }

    /// What fills its cache, run once in the checkout.
    #[must_use]
    pub const fn fill(self) -> &'static str {
        match self {
            Self::Npm => "npm install",
            Self::Pnpm => "pnpm install",
            Self::Yarn | Self::YarnBerry => "yarn install",
            Self::Bun => "bun install",
            Self::Uv => "uv sync",
        }
    }

    /// The file each install writes inside its folder: what an install
    /// changed shows there first.
    const fn marker(self) -> &'static str {
        match self {
            Self::Npm => ".package-lock.json",
            Self::Pnpm => ".modules.yaml",
            Self::Yarn => ".yarn-integrity",
            Self::YarnBerry => ".yarn-state.yml",
            Self::Bun => ".bun-tag",
            Self::Uv => "pyvenv.cfg",
        }
    }
}

/// A directory of the project with a committed lockfile beside its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    /// Project-relative; `""` is the project's own directory.
    pub dir: String,
    /// The toolchain its lockfile names.
    pub toolchain: Toolchain,
    /// The lockfile, project-relative.
    pub lockfile: String,
}

impl Root {
    /// Its dependency folder, project-relative.
    #[must_use]
    pub fn folder(&self) -> String {
        join(&self.dir, self.toolchain.folder())
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// The project's toolchain roots: each committed lockfile beside its
/// manifest (one per directory), never inside a dependency folder. Go and
/// Rust have none: their caches are shared already.
#[must_use]
pub fn roots(project: &Path) -> Vec<Root> {
    let listed = Git::at(project)
        .run(&["ls-files", "-z"])
        .map(|o| nul_fields(&o))
        .unwrap_or_default();
    let mut out: Vec<(usize, Root)> = Vec::new();
    for path in &listed {
        if path.split('/').any(|c| c == "node_modules" || c == ".venv") {
            continue;
        }
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let Some((rank, (_, toolchain))) =
            LOCKFILES.iter().enumerate().find(|(_, (n, _))| *n == name)
        else {
            continue;
        };
        if !project.join(dir).join(toolchain.manifest()).is_file() {
            continue;
        }
        let toolchain = match toolchain {
            // Yarn 2+ writes `__metadata` at the top of its lockfile.
            Toolchain::Yarn
                if std::fs::read_to_string(project.join(path))
                    .is_ok_and(|t| t.contains("\n__metadata:") || t.starts_with("__metadata:")) =>
            {
                Toolchain::YarnBerry
            }
            t => *t,
        };
        let root = Root {
            dir: dir.to_owned(),
            toolchain,
            lockfile: path.clone(),
        };
        let node = |t: Toolchain| t != Toolchain::Uv;
        match out
            .iter_mut()
            .find(|(_, r)| r.dir == root.dir && node(r.toolchain) == node(root.toolchain))
        {
            Some(seen) if seen.0 > rank => *seen = (rank, root),
            Some(_) => {}
            None => out.push((rank, root)),
        }
    }
    let mut roots: Vec<Root> = out.into_iter().map(|(_, r)| r).collect();
    roots.sort_by(|a, b| (&a.dir, a.toolchain.folder()).cmp(&(&b.dir, b.toolchain.folder())));
    roots
}

/// Whether the project has anything to prepare: a root, or a manifest
/// whose lockfile is missing.
#[must_use]
pub fn needed(project: &Path) -> bool {
    let roots = roots(project);
    !roots.is_empty() || missing_lockfile(project, &roots).is_some()
}

/// A manifest at the project's root that needs a lockfile none is
/// committed for: `(what is missing, the command that writes it)`.
#[must_use]
pub fn missing_lockfile(project: &Path, roots: &[Root]) -> Option<(String, String)> {
    let at_root = |uv: bool| {
        roots
            .iter()
            .any(|r| r.dir.is_empty() && (r.toolchain == Toolchain::Uv) == uv)
    };
    let package = std::fs::read_to_string(project.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    if let Some(p) = package
        && !at_root(false)
        && ["dependencies", "devDependencies", "optionalDependencies"]
            .iter()
            .any(|k| p[*k].as_object().is_some_and(|o| !o.is_empty()))
    {
        return Some((
            "package.json declares dependencies, but no lockfile is committed".to_owned(),
            "npm install (writes package-lock.json; commit it)".to_owned(),
        ));
    }
    let pyproject = std::fs::read_to_string(project.join("pyproject.toml")).unwrap_or_default();
    let uv = pyproject.contains("[tool.uv") || pyproject.contains("uv_build");
    if uv && !at_root(true) {
        return Some((
            "pyproject.toml is a uv project, but no uv.lock is committed".to_owned(),
            "uv lock (writes uv.lock; commit it)".to_owned(),
        ));
    }
    None
}

/// How a root's folder came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The toolchain's locked install, in the worktree's own folder.
    Installed,
    /// A symlink to the checkout's folder.
    Linked,
}

/// One root as prepared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The root, project-relative.
    pub dir: String,
    /// Its toolchain.
    pub toolchain: Toolchain,
    /// Its lockfile, project-relative.
    pub lockfile: String,
    /// sha256 of the lockfile it was prepared for.
    pub lock_sha256: String,
    /// Installed or linked.
    pub mode: Mode,
    /// `[worktree] link` names its folder.
    #[serde(default)]
    pub link_asked: bool,
    /// The link, in the worktree (absolute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// The checkout's folder it points to (absolute, resolved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// Why a folder `link` names was installed instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// A linked `.venv`: the worktree's source root, first on `PYTHONPATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pythonpath: Option<String>,
    /// A linked folder's install fingerprint (see [`fingerprint`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// When (seconds since the epoch).
    pub at: u64,
}

impl Entry {
    /// The folder, project-relative.
    #[must_use]
    pub fn folder(&self) -> String {
        join(&self.dir, self.toolchain.folder())
    }
}

/// What a worktree was prepared with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Per root.
    #[serde(default)]
    pub roots: Vec<Entry>,
}

impl Record {
    /// The variables checks in the worktree need: a linked `.venv`'s source
    /// root first on `PYTHONPATH` (tests import the worktree's code, never
    /// the checkout's editable install), and `UV_NO_SYNC` (uv would
    /// otherwise re-install the project into the checkout's `.venv`).
    #[must_use]
    pub fn env(&self) -> Vec<(String, String)> {
        let paths: Vec<&str> = self
            .roots
            .iter()
            .filter(|e| e.mode == Mode::Linked)
            .filter_map(|e| e.pythonpath.as_deref())
            .collect();
        if paths.is_empty() {
            return Vec::new();
        }
        vec![
            ("PYTHONPATH".to_owned(), paths.join(":")),
            ("UV_NO_SYNC".to_owned(), "1".to_owned()),
        ]
    }

    /// The linked folders, for the tick's evidence.
    #[must_use]
    pub fn linked(&self) -> Vec<speccoder_core::predicate::LinkedDependency> {
        self.roots
            .iter()
            .filter(|e| e.mode == Mode::Linked)
            .map(|e| speccoder_core::predicate::LinkedDependency {
                path: e.folder(),
                lockfile: e.lockfile.clone(),
                sha256: e.lock_sha256.clone(),
            })
            .collect()
    }
}

/// Where a worktree's record is kept: beside it (`<name>.prepare.json`),
/// never inside it, where git would see it.
#[must_use]
pub fn record_path(worktree: &Path) -> PathBuf {
    let name = worktree
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    worktree.with_file_name(format!("{name}.prepare.json"))
}

/// The worktree's record (none: nothing prepared yet).
#[must_use]
pub fn load(worktree: &Path) -> Record {
    std::fs::read_to_string(record_path(worktree))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn store(worktree: &Path, record: &Record) -> Result<(), String> {
    let path = record_path(worktree);
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The variables a worktree's record gives its commands ([`Record::env`]);
/// none for a tree never prepared (the checkout, a fresh scratch tree).
#[must_use]
pub fn recorded_env(worktree: &Path) -> Vec<(String, String)> {
    let path = record_path(worktree);
    if !path.is_file() {
        return Vec::new();
    }
    load(worktree).env()
}

/// Removes a worktree's links — each only after `symlink_metadata` shows a
/// link, so nothing is ever removed through one — and its record: before
/// the worktree itself is removed.
pub fn unlink(worktree: &Path) {
    let path = record_path(worktree);
    if !path.is_file() {
        return;
    }
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let root = canonical(worktree);
    for e in load(worktree).roots {
        if let Some(link) = e.link.as_deref().map(Path::new)
            && link
                .parent()
                .is_some_and(|d| canonical(d).starts_with(&root))
            && std::fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink())
        {
            let _ = std::fs::remove_file(link);
        }
    }
    let _ = std::fs::remove_file(path);
}

/// sha256 of a file, hex; `None` when it cannot be read.
#[must_use]
pub fn sha256_file(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| crate::integrity::sha256_hex(&b))
}

/// What an install left in `folder` (read through a link): the marker
/// file its toolchain writes on every install and the folder's top-level
/// entries (one level into `@scope` folders) for Node; for a `.venv`, the
/// names of its installed distributions and the contents of its `.pth`
/// files (where an editable install points). Cheap: no tree walk.
#[must_use]
pub fn fingerprint(folder: &Path, toolchain: Toolchain) -> String {
    let mut h = Sha256::new();
    let names = |dir: &Path| -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    };
    if let Ok(b) = std::fs::read(folder.join(toolchain.marker())) {
        h.update(&b);
    }
    if toolchain == Toolchain::Uv {
        for py in names(&folder.join("lib"))
            .into_iter()
            .filter(|n| n.starts_with("python"))
        {
            let site = folder.join("lib").join(&py).join("site-packages");
            for n in names(&site) {
                if n.ends_with(".dist-info") {
                    h.update(n.as_bytes());
                    h.update(b"\n");
                } else if n.ends_with(".pth") {
                    h.update(n.as_bytes());
                    h.update(std::fs::read(site.join(&n)).unwrap_or_default());
                    h.update(b"\n");
                }
            }
        }
    } else {
        for n in names(folder) {
            h.update(n.as_bytes());
            h.update(b"\n");
            if n.starts_with('@') {
                for m in names(&folder.join(&n)) {
                    h.update(m.as_bytes());
                    h.update(b"\n");
                }
            }
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What preparing needs.
pub struct Prepare<'a> {
    /// The worktree's root (the record is kept beside it).
    pub worktree: &'a Path,
    /// The project's directory inside it.
    pub project: &'a Path,
    /// The project's directory in the person's checkout.
    pub checkout: &'a Path,
    /// `[worktree]`.
    pub config: &'a Config,
    /// The worktree's (sandboxed) runner.
    pub runner: &'a Runner,
    /// speccoder's state directory: the cache `speccoder new`'s steps
    /// filled is tried after the person's own.
    pub state: Option<&'a Path>,
}

/// What preparing did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    /// One line per root prepared now (`npm ci --offline`, `linked
    /// node_modules`); empty: everything was reused.
    pub done: Vec<String>,
    /// Why a folder `link` names was installed instead, one line each.
    pub notes: Vec<String>,
    /// The record as stored.
    pub record: Record,
}

/// Why preparing stopped the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What failed, with the toolchain's own reason.
    pub message: String,
    /// How to resolve it.
    pub fix: Vec<String>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Prepares the worktree: each root's folder linked or installed, unless
/// the record shows it prepared for the same lockfile and still there.
/// `None`: the project has no root that needs it (Go, Rust).
///
/// # Errors
///
/// A lockfile is missing, or an install failed: the task stops
/// (`PREPARE-FAILED`), never a verdict.
pub fn prepare(p: &Prepare<'_>) -> Result<Option<Prepared>, Failure> {
    let roots = roots(p.project);
    if let Some((what, writes)) = missing_lockfile(p.project, &roots) {
        return Err(Failure {
            message: format!("{what}: the worktree cannot get the dependencies it locks"),
            fix: vec![
                format!("→ in your checkout: {writes}"),
                "→ or, when the checks need none of it: [worktree] prepare = \"off\"".to_owned(),
            ],
        });
    }
    if roots.is_empty() {
        return Ok(None);
    }
    let old = load(p.worktree);
    let mut out = Prepared::default();
    for root in &roots {
        let folder = p.project.join(root.folder());
        let sha = sha256_file(&p.project.join(&root.lockfile)).unwrap_or_default();
        let asked = p.config.link.contains(&root.folder());
        let prev = old.roots.iter().find(|e| {
            e.dir == root.dir && e.toolchain == root.toolchain && e.lockfile == root.lockfile
        });
        if let Some(e) = prev
            && e.lock_sha256 == sha
            && e.link_asked == asked
            && holds(e, &folder, p.checkout)
        {
            out.record.roots.push(e.clone());
            continue;
        }
        // A link made earlier goes first — itself, never what it points to.
        if std::fs::symlink_metadata(&folder).is_ok_and(|m| m.file_type().is_symlink()) {
            std::fs::remove_file(&folder).map_err(|e| Failure {
                message: format!(
                    "the worktree's link {} could not be removed: {e}",
                    root.folder()
                ),
                fix: vec!["speccoder doctor".to_owned()],
            })?;
        }
        let why = if asked {
            match link(p, root, &folder, &sha) {
                Ok(entry) => {
                    out.done.push(format!("linked {}", root.folder()));
                    out.record.roots.push(entry);
                    continue;
                }
                Err(why) => {
                    out.notes.push(format!(
                        "{} not linked: {why}; installed in the worktree instead",
                        root.folder()
                    ));
                    Some(why)
                }
            }
        } else {
            None
        };
        let ran = install(p, root)?;
        out.done.push(ran);
        out.record.roots.push(Entry {
            dir: root.dir.clone(),
            toolchain: root.toolchain,
            lockfile: root.lockfile.clone(),
            lock_sha256: sha,
            mode: Mode::Installed,
            link_asked: asked,
            link: None,
            link_target: None,
            why,
            pythonpath: None,
            fingerprint: None,
            at: now(),
        });
    }
    if out.record != old || !record_path(p.worktree).is_file() {
        store(p.worktree, &out.record).map_err(|e| Failure {
            message: format!("the worktree's prepare record could not be written: {e}"),
            fix: vec!["speccoder doctor".to_owned()],
        })?;
    }
    Ok(Some(out))
}

/// Whether a recorded root is still as recorded: an installed folder is
/// the worktree's own; a link still points where it did, and the
/// checkout's lockfile is still the one it was checked against (a person's
/// `npm install` there changes what the link holds).
fn holds(e: &Entry, folder: &Path, checkout: &Path) -> bool {
    let meta = std::fs::symlink_metadata(folder);
    match e.mode {
        Mode::Installed => meta.is_ok_and(|m| m.is_dir()),
        Mode::Linked => {
            meta.is_ok_and(|m| m.file_type().is_symlink())
                && std::fs::read_link(folder)
                    .ok()
                    .is_some_and(|t| Some(t.display().to_string()) == e.link_target)
                && folder.is_dir()
                && sha256_file(&checkout.join(&e.lockfile)).as_deref() == Some(&e.lock_sha256)
        }
    }
}

/// Serialises edits of the shared `info/exclude` (lanes prepare at once).
static EXCLUDE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Links `root`'s folder from the checkout, or says why not.
fn link(p: &Prepare<'_>, root: &Root, folder: &Path, sha: &str) -> Result<Entry, String> {
    let rel = root.folder();
    if !p.runner.is_sandboxed() {
        return Err(
            "commands run unsandboxed here, so a linked folder would be writable".to_owned(),
        );
    }
    let state = link_state(p.checkout, root, Some(sha))?;
    // The worktree's own install of it, from before: replaced.
    if std::fs::symlink_metadata(folder).is_ok_and(|m| m.is_dir()) {
        std::fs::remove_dir_all(folder)
            .map_err(|e| format!("the worktree's own {rel} could not be replaced: {e}"))?;
    }
    std::os::unix::fs::symlink(&state.target, folder)
        .map_err(|e| format!("the link could not be made: {e}"))?;
    let undo = |why: String| {
        let _ = std::fs::remove_file(folder);
        why
    };
    let pythonpath = if root.toolchain == Toolchain::Uv {
        Some(probe(p, root, folder).map_err(undo)?)
    } else {
        None
    };
    exclude(p.project, &rel).map_err(undo)?;
    Ok(Entry {
        dir: root.dir.clone(),
        toolchain: root.toolchain,
        lockfile: root.lockfile.clone(),
        lock_sha256: sha.to_owned(),
        mode: Mode::Linked,
        link_asked: true,
        link: Some(folder.display().to_string()),
        link_target: Some(state.target.display().to_string()),
        why: None,
        pythonpath,
        fingerprint: Some(fingerprint(folder, root.toolchain)),
        at: now(),
    })
}

/// A checkout folder that may be linked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linkable {
    /// The folder, resolved.
    pub target: PathBuf,
}

/// Whether `root`'s folder in `checkout` may be linked into a worktree
/// whose lockfile has digest `sha` (`None`: the checkout's committed one):
/// it exists, resolves inside the checkout, is ignored there, its lockfile
/// is the worktree's byte for byte, and — for Node — no workspace package
/// is linked into it from the checkout. Creates nothing.
///
/// # Errors
///
/// Why not, in a phrase.
pub fn link_state(checkout: &Path, root: &Root, sha: Option<&str>) -> Result<Linkable, String> {
    let rel = root.folder();
    let source = checkout.join(&rel);
    if !source.is_dir() {
        return Err("missing in your checkout".to_owned());
    }
    let home = std::fs::canonicalize(checkout).map_err(|e| e.to_string())?;
    let target = std::fs::canonicalize(&source).map_err(|e| e.to_string())?;
    let Ok(inside) = target.strip_prefix(&home) else {
        return Err("it resolves outside your checkout".to_owned());
    };
    let inside = inside.to_string_lossy().into_owned();
    if !Git::at(checkout)
        .succeeds(&["check-ignore", "-q", "--", &inside])
        .unwrap_or(false)
    {
        return Err("not git-ignored in your checkout".to_owned());
    }
    let theirs = sha256_file(&checkout.join(&root.lockfile));
    let ours = match sha {
        Some(s) => Some(s.to_owned()),
        None => Git::at(checkout)
            .run(&["show", &format!("HEAD:./{}", root.lockfile)])
            .ok()
            .map(|b| crate::integrity::sha256_hex(&b)),
    };
    if theirs.is_none() || theirs != ours {
        return Err(format!("{} differs from your checkout's", root.lockfile));
    }
    if root.toolchain != Toolchain::Uv {
        let manifest = std::fs::read_to_string(checkout.join(&root.dir).join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        if manifest.is_some_and(|m| !m["workspaces"].is_null())
            || checkout
                .join(&root.dir)
                .join("pnpm-workspace.yaml")
                .is_file()
        {
            return Err(
                "its workspace packages are linked into it from your checkout, so the checks would run your checkout's code"
                    .to_owned(),
            );
        }
    }
    Ok(Linkable { target })
}

/// A uv root's package and the directory it is imported from:
/// `[tool.uv.build-backend] module-name` (else `[project] name`, `-` as
/// `_`) and `module-root` (else `src/` when the package is there, else the
/// root itself).
#[must_use]
pub fn python_package(root_dir: &Path) -> Option<(String, PathBuf)> {
    let text = std::fs::read_to_string(root_dir.join("pyproject.toml")).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    let backend = table
        .get("tool")
        .and_then(|t| t.get("uv"))
        .and_then(|t| t.get("build-backend"));
    let name = backend
        .and_then(|b| b.get("module-name"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            table
                .get("project")
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
                .map(|n| n.to_lowercase().replace(['-', '.'], "_"))
        })?;
    let source = match backend
        .and_then(|b| b.get("module-root"))
        .and_then(toml::Value::as_str)
    {
        Some(r) => root_dir.join(r),
        None if root_dir.join("src").join(&name).exists() => root_dir.join("src"),
        None => root_dir.to_path_buf(),
    };
    Some((name, source))
}

/// The import probe of a linked `.venv`: the project's package, imported
/// by the linked interpreter with the worktree's source root first on
/// `PYTHONPATH`, sandboxed, must resolve inside the worktree. The source
/// root, or why not.
fn probe(p: &Prepare<'_>, root: &Root, folder: &Path) -> Result<String, String> {
    let dir = p.project.join(&root.dir);
    let Some((module, source)) = python_package(&dir) else {
        return Err("pyproject.toml names no package to check the import of".to_owned());
    };
    let source = source.display().to_string();
    let mut env = scrubbed_env(std::env::vars());
    env.retain(|(k, _)| k != "PYTHONPATH");
    env.push(("PYTHONPATH".to_owned(), source.clone()));
    let python = folder.join("bin").join("python").display().to_string();
    let out = p.runner.run(&Invocation {
        argv: vec![
            python,
            "-c".to_owned(),
            "import importlib, sys; print(importlib.import_module(sys.argv[1]).__file__)"
                .to_owned(),
            module.clone(),
        ],
        cwd: dir,
        env,
        timeout: PROBE_TIMEOUT,
        output_cap: OUTPUT_CAP,
    });
    let printed = String::from_utf8_lossy(&out.stdout.kept).trim().to_owned();
    let worktree = std::fs::canonicalize(p.worktree).unwrap_or_else(|_| p.worktree.to_path_buf());
    let resolved = std::fs::canonicalize(&printed).ok();
    match resolved {
        Some(r) if out.exit_code == Some(0) && r.starts_with(&worktree) => Ok(source),
        Some(_) => Err(format!(
            "`import {module}` resolves outside the worktree (to your checkout's install), so the tests would judge the wrong code"
        )),
        None => Err(format!(
            "`import {module}` fails through the linked .venv: {}",
            reason(&out)
        )),
    }
}

/// Adds the folder's anchored entry under [`EXCLUDE_MARK`] in the
/// repository's shared `info/exclude`, once.
fn exclude(project: &Path, rel: &str) -> Result<(), String> {
    let g = Git::at(project);
    let common = g
        .line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map_err(|e| e.0)?;
    let prefix = g.line(&["rev-parse", "--show-prefix"]).map_err(|e| e.0)?;
    let entry = format!("/{prefix}{rel}");
    let file = Path::new(&common).join("info").join("exclude");
    let _one = EXCLUDE.lock();
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let Some(next) = with_entry(&text, &entry) else {
        return Ok(());
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = file.with_extension("speccoder.tmp");
    std::fs::write(&tmp, next)
        .and_then(|()| std::fs::rename(&tmp, &file))
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// `text` with `entry` in speccoder's block (the block added at the end
/// when there is none); `None` when it is there already.
fn with_entry(text: &str, entry: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(mark) = lines.iter().position(|l| l.trim() == EXCLUDE_MARK) else {
        let sep = if text.is_empty() || text.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        return Some(format!("{text}{sep}{EXCLUDE_MARK}\n{entry}\n"));
    };
    let end = lines[mark + 1..]
        .iter()
        .position(|l| !l.starts_with('/'))
        .map_or(lines.len(), |n| mark + 1 + n);
    if lines[mark + 1..end].contains(&entry) {
        return None;
    }
    let mut out: Vec<&str> = lines[..end].to_vec();
    out.push(entry);
    out.extend_from_slice(&lines[end..]);
    Some(format!("{}\n", out.join("\n")))
}

/// Runs `root`'s install in its directory, sandboxed: the offline form,
/// or the online one when `[sandbox] network_allow` names it exactly;
/// once more from `speccoder new`'s own cache when the person's lacks a
/// package. The line for the run log.
fn install(p: &Prepare<'_>, root: &Root) -> Result<String, Failure> {
    let online = root.toolchain.install(false);
    let argv = if p.runner.allows_network(&online) {
        online.clone()
    } else {
        root.toolchain.install(true)
    };
    let cwd = p.project.join(&root.dir);
    let mut env = scrubbed_env(std::env::vars());
    env.push(("npm_config_update_notifier".to_owned(), "false".to_owned()));
    let run = |runner: &Runner, env: Vec<(String, String)>| {
        runner.run(&Invocation {
            argv: argv.clone(),
            cwd: cwd.clone(),
            env,
            timeout: INSTALL_TIMEOUT,
            output_cap: OUTPUT_CAP,
        })
    };
    let ok = |o: &Outcome| o.exit_code == Some(0) && !o.timed_out && o.spawn_error.is_none();
    let began = std::time::Instant::now();
    let mut out = run(p.runner, env.clone());
    if !ok(&out)
        && out.spawn_error.is_none()
        && let Some((cache, vars)) = p.state.and_then(|s| {
            crate::new_project::steps::filled_cache(s, argv.first().map_or("", String::as_str))
        })
    {
        let mut env = env;
        for (k, v) in vars {
            env.retain(|(key, _)| *key != k);
            env.push((k, v));
        }
        out = run(&p.runner.clone().with_writable(&cache), env);
    }
    let shown = argv.join(" ");
    let at = if root.dir.is_empty() {
        String::new()
    } else {
        format!(" in {}", root.dir)
    };
    if ok(&out) {
        return Ok(format!(
            "{shown}{at} ({:.1}s)",
            began.elapsed().as_secs_f64()
        ));
    }
    let mut fix = vec![format!(
        "→ run `{}` once in your checkout{at} to fill {}'s cache",
        root.toolchain.fill(),
        argv.first().map_or("the toolchain", String::as_str)
    )];
    if online != argv {
        fix.push(format!(
            "→ or let `{}` reach the network: [sandbox] network_allow = [[{}]]",
            online.join(" "),
            online
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    fix.push("speccoder doctor".to_owned());
    Err(Failure {
        message: format!("`{shown}` failed in the worktree{at}: {}", reason(&out)),
        fix,
    })
}

/// The toolchain's own reason, in a line: its first error line (and the
/// cause uv gives on the next), else the last line it printed.
fn reason(out: &Outcome) -> String {
    if let Some(e) = &out.spawn_error {
        return e.clone();
    }
    if out.timed_out {
        return format!("timed out after {}s", out.duration.as_secs());
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stderr.kept),
        String::from_utf8_lossy(&out.stdout.kept)
    );
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let noise = |l: &str| {
        l.contains("complete log of this run")
            || l.contains("Log files were not written")
            || (l.starts_with("npm error code ") && !l["npm error code ".len()..].contains(' '))
    };
    let first = lines.iter().position(|l| {
        let low = l.to_lowercase();
        (low.starts_with("error") || low.starts_with("npm error") || low.starts_with("npm err!"))
            && !noise(l)
    });
    match first {
        Some(i) => {
            let line = lines[i]
                .trim_start_matches("npm error ")
                .trim_start_matches("npm ERR! ");
            match lines.get(i + 1).filter(|n| n.starts_with("cause:")) {
                Some(cause) => format!("{line} — {cause}"),
                None => line.to_owned(),
            }
        }
        None => lines
            .last()
            .map_or_else(|| format!("exit {:?}", out.exit_code), |l| (*l).to_owned()),
    }
}

/// The note a failed command's output gets when it names a linked folder
/// and was refused a write: why, and the setting.
#[must_use]
pub fn linked_note(output: &str, linked: &[String]) -> Option<String> {
    let refused = [
        "Operation not permitted",
        "EPERM",
        "EACCES",
        "Permission denied",
        "Read-only file system",
        "EROFS",
    ]
    .iter()
    .any(|w| output.contains(w));
    if !refused {
        return None;
    }
    let named: Vec<&str> = linked
        .iter()
        .filter(|l| output.contains(l.as_str()))
        .map(String::as_str)
        .collect();
    (!named.is_empty()).then(|| {
        format!(
            "{} is linked from the checkout, read-only ([worktree] link)",
            named
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git {args:?}");
    }

    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let f = dir.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        }
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn the_section_parses_with_its_defaults_and_refuses_paths_outside() {
        assert_eq!(parse("").unwrap(), Config::default());
        let c = parse("[worktree]\nprepare = \"off\"\nlink = [\"node_modules/\", \"api/.venv\", \"node_modules\"]\n").unwrap();
        assert!(!c.prepare);
        assert_eq!(c.link, ["node_modules", "api/.venv"]);
        for bad in [
            "/abs", "~/x", "../up", "a/../b", "./a", ".git", "a/.git/x", "",
        ] {
            let e = parse(&format!("[worktree]\nlink = [\"{bad}\"]\n")).unwrap_err();
            assert!(e.starts_with("[worktree] link: "), "{bad}: {e}");
        }
        assert!(parse("[worktree]\nprepare = \"always\"\n").is_err());
        assert!(parse("[worktree]\nlinks = []\n").is_err());
    }

    #[test]
    fn roots_are_the_committed_lockfiles_beside_their_manifests() {
        let berry =
            "# This file is generated by running \"yarn install\"\n\n__metadata:\n  version: 8\n";
        type Files<'a> = &'a [(&'a str, &'a str)];
        type Want<'a> = &'a [(&'a str, Toolchain, &'a str)];
        let cases: &[(Files<'_>, Want<'_>)] = &[
            (&[("Cargo.toml", ""), ("Cargo.lock", "")], &[]),
            (&[("go.mod", ""), ("go.sum", "")], &[]),
            (
                &[("package.json", "{}"), ("package-lock.json", "{}")],
                &[("", Toolchain::Npm, "package-lock.json")],
            ),
            (
                &[("package.json", "{}"), ("pnpm-lock.yaml", "")],
                &[("", Toolchain::Pnpm, "pnpm-lock.yaml")],
            ),
            (
                &[
                    ("package.json", "{}"),
                    ("yarn.lock", "# yarn lockfile v1\n"),
                ],
                &[("", Toolchain::Yarn, "yarn.lock")],
            ),
            (
                &[("package.json", "{}"), ("yarn.lock", berry)],
                &[("", Toolchain::YarnBerry, "yarn.lock")],
            ),
            (
                &[("package.json", "{}"), ("bun.lock", "")],
                &[("", Toolchain::Bun, "bun.lock")],
            ),
            (
                &[("package.json", "{}"), ("bun.lockb", "")],
                &[("", Toolchain::Bun, "bun.lockb")],
            ),
            // Two Node lockfiles: npm's is chosen.
            (
                &[
                    ("package.json", "{}"),
                    ("yarn.lock", ""),
                    ("package-lock.json", "{}"),
                ],
                &[("", Toolchain::Npm, "package-lock.json")],
            ),
            (
                &[("pyproject.toml", ""), ("uv.lock", "")],
                &[("", Toolchain::Uv, "uv.lock")],
            ),
            // A workspace: each lockfile's directory on its own.
            (
                &[
                    ("package.json", "{}"),
                    ("package-lock.json", "{}"),
                    ("api/pyproject.toml", ""),
                    ("api/uv.lock", ""),
                ],
                &[
                    ("", Toolchain::Npm, "package-lock.json"),
                    ("api", Toolchain::Uv, "api/uv.lock"),
                ],
            ),
            // A lockfile without its manifest is no root.
            (&[("uv.lock", "")], &[]),
        ];
        for (files, want) in cases {
            let r = repo(files);
            let got: Vec<(String, Toolchain, String)> = roots(r.path())
                .into_iter()
                .map(|x| (x.dir, x.toolchain, x.lockfile))
                .collect();
            let want: Vec<(String, Toolchain, String)> = want
                .iter()
                .map(|(d, t, l)| ((*d).to_owned(), *t, (*l).to_owned()))
                .collect();
            assert_eq!(got, want, "{files:?}");
        }
        // Uncommitted lockfiles do not count.
        let r = repo(&[("package.json", "{}")]);
        std::fs::write(r.path().join("package-lock.json"), "{}").unwrap();
        assert!(roots(r.path()).is_empty());
    }

    #[test]
    fn each_toolchain_installs_locked_and_offline() {
        let shown = |t: Toolchain, offline| t.install(offline).join(" ");
        assert_eq!(shown(Toolchain::Npm, true), "npm ci --offline");
        assert_eq!(shown(Toolchain::Npm, false), "npm ci");
        assert_eq!(
            shown(Toolchain::Pnpm, true),
            "pnpm install --frozen-lockfile --offline"
        );
        assert_eq!(
            shown(Toolchain::Yarn, true),
            "yarn install --frozen-lockfile --offline"
        );
        assert_eq!(
            shown(Toolchain::YarnBerry, true),
            "yarn install --immutable"
        );
        assert_eq!(shown(Toolchain::Bun, true), "bun install --frozen-lockfile");
        assert_eq!(shown(Toolchain::Uv, true), "uv sync --locked --offline");
        assert_eq!(shown(Toolchain::Uv, false), "uv sync --locked");
    }

    #[test]
    fn a_manifest_with_dependencies_and_no_lockfile_is_named() {
        let r = repo(&[(
            "package.json",
            "{\"devDependencies\": {\"vitest\": \"^3\"}}",
        )]);
        let (what, writes) = missing_lockfile(r.path(), &roots(r.path())).unwrap();
        assert!(what.contains("no lockfile is committed"), "{what}");
        assert!(writes.starts_with("npm install"), "{writes}");
        // No dependencies: nothing to lock.
        let r = repo(&[("package.json", "{\"scripts\": {\"test\": \"node t.js\"}}")]);
        assert_eq!(missing_lockfile(r.path(), &roots(r.path())), None);
        let r = repo(&[(
            "pyproject.toml",
            "[project]\nname = \"x\"\n[build-system]\nrequires = [\"uv_build\"]\n",
        )]);
        let (what, writes) = missing_lockfile(r.path(), &roots(r.path())).unwrap();
        assert!(what.contains("uv.lock"), "{what}");
        assert!(writes.starts_with("uv lock"), "{writes}");
        let r = repo(&[("pyproject.toml", "[project]\nname = \"x\"\n")]);
        assert_eq!(missing_lockfile(r.path(), &roots(r.path())), None);
    }

    #[test]
    fn the_record_is_kept_beside_the_worktree_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("001-x");
        std::fs::create_dir_all(&wt).unwrap();
        assert_eq!(record_path(&wt), dir.path().join("001-x.prepare.json"));
        assert_eq!(load(&wt), Record::default());
        assert!(recorded_env(&wt).is_empty());
        let record = Record {
            roots: vec![Entry {
                dir: String::new(),
                toolchain: Toolchain::Uv,
                lockfile: "uv.lock".to_owned(),
                lock_sha256: "ab".to_owned(),
                mode: Mode::Linked,
                link_asked: true,
                link: Some(wt.join(".venv").display().to_string()),
                link_target: Some("/c/.venv".to_owned()),
                why: None,
                pythonpath: Some(wt.join("src").display().to_string()),
                fingerprint: None,
                at: 1,
            }],
        };
        store(&wt, &record).unwrap();
        assert_eq!(load(&wt), record);
        assert_eq!(
            recorded_env(&wt),
            [
                (
                    "PYTHONPATH".to_owned(),
                    wt.join("src").display().to_string()
                ),
                ("UV_NO_SYNC".to_owned(), "1".to_owned())
            ]
        );
        assert_eq!(
            record.linked(),
            [speccoder_core::predicate::LinkedDependency {
                path: ".venv".to_owned(),
                lockfile: "uv.lock".to_owned(),
                sha256: "ab".to_owned()
            }]
        );
    }

    #[test]
    fn unlinking_removes_only_links_never_what_they_point_to() {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("checkout");
        std::fs::create_dir_all(checkout.join("node_modules/a")).unwrap();
        std::fs::write(checkout.join("node_modules/a/index.js"), "x").unwrap();
        let wt = dir.path().join("wt");
        std::fs::create_dir_all(wt.join("real")).unwrap();
        std::os::unix::fs::symlink(checkout.join("node_modules"), wt.join("node_modules")).unwrap();
        let entry = |link: PathBuf| Entry {
            dir: String::new(),
            toolchain: Toolchain::Npm,
            lockfile: "package-lock.json".to_owned(),
            lock_sha256: String::new(),
            mode: Mode::Linked,
            link_asked: true,
            link: Some(link.display().to_string()),
            link_target: None,
            why: None,
            pythonpath: None,
            fingerprint: None,
            at: 0,
        };
        // A recorded "link" that is a real directory is left alone.
        store(
            &wt,
            &Record {
                roots: vec![entry(wt.join("node_modules")), entry(wt.join("real"))],
            },
        )
        .unwrap();
        unlink(&wt);
        assert!(std::fs::symlink_metadata(wt.join("node_modules")).is_err());
        assert!(wt.join("real").is_dir());
        assert!(checkout.join("node_modules/a/index.js").is_file());
        assert!(!record_path(&wt).exists());
    }

    #[test]
    fn the_exclude_block_is_written_once_and_grows_in_place() {
        let once = with_entry("", "/node_modules").unwrap();
        assert_eq!(once, format!("{EXCLUDE_MARK}\n/node_modules\n"));
        assert_eq!(with_entry(&once, "/node_modules"), None);
        let person = "# git ls-files --others --exclude-from=.git/info/exclude\n*.log";
        let both = with_entry(person, "/node_modules").unwrap();
        assert_eq!(both, format!("{person}\n{EXCLUDE_MARK}\n/node_modules\n"));
        let grown = with_entry(&format!("{both}# mine\nfoo\n"), "/api/.venv").unwrap();
        assert_eq!(
            grown,
            format!("{person}\n{EXCLUDE_MARK}\n/node_modules\n/api/.venv\n# mine\nfoo\n")
        );
    }

    #[test]
    fn a_python_package_is_found_with_its_source_root() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        std::fs::write(r.join("pyproject.toml"), "[project]\nname = \"my-pkg\"\n").unwrap();
        assert_eq!(
            python_package(r),
            Some(("my_pkg".to_owned(), r.to_path_buf()))
        );
        std::fs::create_dir_all(r.join("src/my_pkg")).unwrap();
        assert_eq!(
            python_package(r),
            Some(("my_pkg".to_owned(), r.join("src")))
        );
        std::fs::write(
            r.join("pyproject.toml"),
            "[project]\nname = \"my-pkg\"\n[tool.uv.build-backend]\nmodule-name = \"other\"\nmodule-root = \"lib\"\n",
        )
        .unwrap();
        assert_eq!(python_package(r), Some(("other".to_owned(), r.join("lib"))));
        std::fs::write(r.join("pyproject.toml"), "[tool.uv]\n").unwrap();
        assert_eq!(python_package(r), None);
    }

    #[test]
    fn a_fingerprint_changes_with_what_an_install_writes() {
        let dir = tempfile::tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        std::fs::create_dir_all(nm.join("@scope/a")).unwrap();
        std::fs::write(nm.join(".package-lock.json"), "{}").unwrap();
        let before = fingerprint(&nm, Toolchain::Npm);
        assert_eq!(before, fingerprint(&nm, Toolchain::Npm));
        std::fs::create_dir_all(nm.join("@scope/b")).unwrap();
        let added = fingerprint(&nm, Toolchain::Npm);
        assert_ne!(added, before);
        std::fs::write(nm.join(".package-lock.json"), "{\"x\":1}").unwrap();
        assert_ne!(fingerprint(&nm, Toolchain::Npm), added);

        let venv = dir.path().join(".venv");
        let site = venv.join("lib/python3.13/site-packages");
        std::fs::create_dir_all(site.join("pytest-9.0.dist-info")).unwrap();
        std::fs::write(site.join("_pkg.pth"), "/checkout/src\n").unwrap();
        let before = fingerprint(&venv, Toolchain::Uv);
        std::fs::write(site.join("_pkg.pth"), "/worktree/src\n").unwrap();
        assert_ne!(fingerprint(&venv, Toolchain::Uv), before);
    }

    #[test]
    fn a_refused_write_into_a_linked_folder_is_explained() {
        let linked = vec!["node_modules".to_owned()];
        assert_eq!(
            linked_note(
                "npm error EPERM: operation not permitted, mkdir '/w/node_modules/left-pad'",
                &linked
            )
            .as_deref(),
            Some("`node_modules` is linked from the checkout, read-only ([worktree] link)")
        );
        assert_eq!(
            linked_note("Error: cannot find module node_modules/x", &linked),
            None
        );
        assert_eq!(linked_note("EPERM: /w/dist", &linked), None);
    }

    #[test]
    fn the_reason_is_the_toolchains_own_line() {
        let out = |stderr: &str, stdout: &str| Outcome {
            exit_code: Some(1),
            timed_out: false,
            spawn_error: None,
            started: std::time::SystemTime::now(),
            duration: Duration::ZERO,
            stdout: crate::verify::process::Captured {
                kept: stdout.as_bytes().to_vec(),
                total: stdout.len(),
            },
            stderr: crate::verify::process::Captured {
                kept: stderr.as_bytes().to_vec(),
                total: stderr.len(),
            },
        };
        assert_eq!(
            reason(&out(
                "npm error code ENOTCACHED\nnpm error request to https://registry.npmjs.org/x.tgz failed: cache mode is 'only-if-cached' but no cached response is available.\nnpm error A complete log of this run can be found in: /h/.npm/_logs/x.log\n",
                ""
            )),
            "request to https://registry.npmjs.org/x.tgz failed: cache mode is 'only-if-cached' but no cached response is available."
        );
        assert_eq!(
            reason(&out(
                "Resolved 7 packages in 3ms\nerror: Failed to download `pygments==2.21.0`\n  cause: Network connectivity is disabled, but the requested data wasn't found in the cache\n\nhint: `pygments` was included because …\n",
                ""
            )),
            "error: Failed to download `pygments==2.21.0` — cause: Network connectivity is disabled, but the requested data wasn't found in the cache"
        );
        assert_eq!(reason(&out("", "something odd\n")), "something odd");
    }
}
