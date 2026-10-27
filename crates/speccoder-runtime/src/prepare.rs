//! A run's worktree gets its dependency folders (`[worktree]`).
//!
//! A worktree holds only what is committed, and dependency folders never
//! are (`node_modules/`, `.venv/`): a check that needs them cannot start
//! there. Before the baseline each Node or uv root of the project gets its
//! folder by the toolchain's own locked, offline install, sandboxed — once
//! per lockfile: a record beside the worktree keeps the lockfile's digest,
//! and an attempt that changes the lockfile is installed again before its
//! checks. Nothing is removed or installed at a folder outside the worktree
//! or reached through a symlink.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::git::{Git, nul_fields};
use crate::sandbox::Runner;
use crate::verify::process::{CommandRunner as _, Invocation, OUTPUT_CAP, Outcome, scrubbed_env};

/// How long one install may take.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

/// `[worktree]` in `speccoder.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `prepare = "auto"` (the default): prepare Node and uv roots before
    /// the baseline; `"off"`: never.
    pub prepare: bool,
    /// Git-ignored files of the checkout copied into each worktree:
    /// `.gitignore`-style patterns (`.env`, `config/*.local.json`), joined
    /// with `.worktreeinclude`'s (see [`crate::include`]).
    pub include: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prepare: true,
            include: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    prepare: Option<String>,
    #[serde(default)]
    include: Vec<String>,
}

/// Reads `[worktree]`.
///
/// # Errors
///
/// An unknown key, a `prepare` other than `auto` or `off`, or an `include`
/// pattern that is empty or leaves the project.
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
    let mut include = Vec::new();
    for p in &raw.include {
        let p = p.trim();
        if p.is_empty() {
            return Err("[worktree] include: an empty pattern".to_owned());
        }
        if p.starts_with('~') || p.split('/').any(|c| c == "..") {
            return Err(format!("[worktree] include: `{p}` leaves the project"));
        }
        if p.split('/').any(|c| c == ".git") {
            return Err(format!("[worktree] include: `{p}` is git's own"));
        }
        if !include.iter().any(|q: &String| q == p) {
            include.push(p.to_owned());
        }
    }
    Ok(Config { prepare, include })
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
    // A workspace member: its lockfile is the workspace's, above it.
    for uv in [false, true] {
        let manifest = if uv { "pyproject.toml" } else { "package.json" };
        if project.join(manifest).is_file()
            && !roots
                .iter()
                .any(|r| r.dir.is_empty() && (r.toolchain == Toolchain::Uv) == uv)
            && let Some(r) = above(project, uv)
        {
            roots.push(r);
        }
    }
    roots.sort_by(|a, b| (&a.dir, a.toolchain.folder()).cmp(&(&b.dir, b.toolchain.folder())));
    roots
}

/// The nearest committed lockfile (Node, or uv's) above the project, up to
/// the repository's root, beside its manifest: the root that governs the
/// project's dependencies, its `dir` made of `..` (still inside the tree).
fn above(project: &Path, uv: bool) -> Option<Root> {
    let g = Git::at(project);
    let prefix = g.line(&["rev-parse", "--show-prefix"]).ok()?;
    let parts: Vec<&str> = prefix.split('/').filter(|c| !c.is_empty()).collect();
    for up in 1..=parts.len() {
        let top = parts[..parts.len() - up].join("/");
        let back = vec![".."; up].join("/");
        for (name, toolchain) in LOCKFILES
            .iter()
            .filter(|(_, t)| (*t == Toolchain::Uv) == uv)
        {
            let committed = g
                .run(&[
                    "ls-files",
                    "-z",
                    "--",
                    &format!(":(top){}", join(&top, name)),
                ])
                .is_ok_and(|o| !o.is_empty());
            if !committed || !project.join(&back).join(toolchain.manifest()).is_file() {
                continue;
            }
            let lockfile = join(&back, name);
            let toolchain = match toolchain {
                Toolchain::Yarn
                    if std::fs::read_to_string(project.join(&lockfile)).is_ok_and(|t| {
                        t.contains("\n__metadata:") || t.starts_with("__metadata:")
                    }) =>
                {
                    Toolchain::YarnBerry
                }
                t => *t,
            };
            return Some(Root {
                dir: back,
                toolchain,
                lockfile,
            });
        }
    }
    None
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
    // The project's own directory, or a workspace's above it.
    let at_root = |uv: bool| {
        roots.iter().any(|r| {
            r.dir.split('/').all(|c| c.is_empty() || c == "..")
                && (r.toolchain == Toolchain::Uv) == uv
        })
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

/// One root as prepared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The root, project-relative.
    pub dir: String,
    /// Its toolchain.
    pub toolchain: Toolchain,
    /// Its lockfile, project-relative.
    pub lockfile: String,
    /// sha256 of the lockfile it was installed for.
    pub lock_sha256: String,
    /// Why it was installed again, when an attempt changed its lockfile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// When (seconds since the epoch).
    pub at: u64,
}

impl Entry {
    /// The folder, project-relative.
    #[must_use]
    pub fn folder(&self) -> String {
        join(&self.dir, self.toolchain.folder())
    }

    fn root(&self) -> Root {
        Root {
            dir: self.dir.clone(),
            toolchain: self.toolchain,
            lockfile: self.lockfile.clone(),
        }
    }
}

/// What a worktree was prepared with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Per root.
    #[serde(default)]
    pub roots: Vec<Entry>,
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

fn store(worktree: &Path, record: &Record) -> Result<(), Failure> {
    let path = record_path(worktree);
    let tmp = path.with_extension("json.tmp");
    serde_json::to_string_pretty(record)
        .map_err(|e| e.to_string())
        .and_then(|text| {
            std::fs::write(&tmp, text)
                .and_then(|()| std::fs::rename(&tmp, &path))
                .map_err(|e| format!("{}: {e}", path.display()))
        })
        .map_err(|e| Failure {
            message: format!("the worktree's prepare record could not be written: {e}"),
            fix: vec!["speccoder doctor".to_owned()],
        })
}

/// Forgets what a worktree was prepared with (its folders stay, the
/// worktree's own): the next prepare installs again.
pub fn forget(worktree: &Path) {
    let _ = std::fs::remove_file(record_path(worktree));
}

/// sha256 of a file, hex; `None` when it cannot be read.
#[must_use]
pub fn sha256_file(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| crate::integrity::sha256_hex(&b))
}

/// What preparing needs.
pub struct Prepare<'a> {
    /// The worktree's root (the record is kept beside it).
    pub worktree: &'a Path,
    /// The project's directory inside it.
    pub project: &'a Path,
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
    /// One line per root installed now (`npm ci --offline (2.1s)`); empty:
    /// everything was reused.
    pub done: Vec<String>,
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

/// Prepares the worktree: each root's folder installed, unless the record
/// shows it installed for the same lockfile and still there. `None`: the
/// project has no root that needs it (Go, Rust).
///
/// # Errors
///
/// A lockfile is missing, a folder is not the worktree's own, or an
/// install failed: the task stops (`PREPARE-FAILED`), never a verdict.
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
        forget(p.worktree);
        return Ok(None);
    }
    let old = load(p.worktree);
    let mut out = Prepared::default();
    let done = prepare_roots(p, &roots, &old, &mut out).and_then(|()| {
        if out.record == old && record_path(p.worktree).is_file() {
            Ok(())
        } else {
            store(p.worktree, &out.record)
        }
    });
    if let Err(f) = done {
        // Half prepared: the next run starts over.
        forget(p.worktree);
        return Err(f);
    }
    Ok(Some(out))
}

fn prepare_roots(
    p: &Prepare<'_>,
    roots: &[Root],
    old: &Record,
    out: &mut Prepared,
) -> Result<(), Failure> {
    for root in roots {
        let folder = p.project.join(root.folder());
        let sha = sha256_file(&p.project.join(&root.lockfile)).unwrap_or_default();
        let prev = old.roots.iter().find(|e| {
            e.dir == root.dir && e.toolchain == root.toolchain && e.lockfile == root.lockfile
        });
        if let Some(e) = prev
            && e.lock_sha256 == sha
            && holds(p.worktree, &folder)
        {
            out.record.roots.push(e.clone());
            continue;
        }
        out.done.push(install_at(p, root, &folder)?);
        out.record.roots.push(Entry {
            dir: root.dir.clone(),
            toolchain: root.toolchain,
            lockfile: root.lockfile.clone(),
            lock_sha256: sha,
            why: None,
            at: now(),
        });
    }
    Ok(())
}

/// Whether an installed folder is still the worktree's own: a directory,
/// not a link, reached without one.
fn holds(worktree: &Path, folder: &Path) -> bool {
    confined(worktree, folder).is_ok()
        && std::fs::symlink_metadata(folder).is_ok_and(|m| m.is_dir())
}

/// `root`'s install at its `folder`, once the folder is known to be the
/// worktree's own: never at one outside it or reached through a symlink,
/// and a link there (an attempt can make one) goes first — itself, never
/// what it points to.
fn install_at(p: &Prepare<'_>, root: &Root, folder: &Path) -> Result<String, Failure> {
    confined(p.worktree, folder).map_err(|why| Failure {
        message: format!("the worktree's {} cannot be prepared: {why}", root.folder()),
        fix: vec!["speccoder doctor".to_owned()],
    })?;
    remove_link(folder)?;
    install(p, root)
}

/// After an attempt, before its checks: a root whose lockfile the attempt
/// changed is installed again for it, so the checks and the tick are about
/// the lockfile committed. The run log's lines; none when nothing changed.
///
/// # Errors
///
/// Its install failed: the task stops (`PREPARE-FAILED`).
pub fn recheck(p: &Prepare<'_>) -> Result<Vec<String>, Failure> {
    let mut record = load(p.worktree);
    let mut done = Vec::new();
    for e in &mut record.roots {
        let sha = sha256_file(&p.project.join(&e.lockfile)).unwrap_or_default();
        if sha == e.lock_sha256 {
            continue;
        }
        let folder = p.project.join(e.folder());
        match install_at(p, &e.root(), &folder) {
            Ok(ran) => {
                done.push(ran);
                e.lock_sha256 = sha;
                e.why = Some("the attempt changed its lockfile".to_owned());
                e.at = now();
            }
            Err(f) => {
                forget(p.worktree);
                return Err(f);
            }
        }
    }
    if !done.is_empty() {
        store(p.worktree, &record)?;
    }
    Ok(done)
}

/// Removes `folder` when it is a link — the link itself, never what it
/// points to.
fn remove_link(folder: &Path) -> Result<(), Failure> {
    if std::fs::symlink_metadata(folder).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::remove_file(folder).map_err(|e| Failure {
            message: format!(
                "the worktree's link {} could not be removed: {e}",
                folder.display()
            ),
            fix: vec!["speccoder doctor".to_owned()],
        })?;
    }
    Ok(())
}

/// `p` with each `..` taken back and each `.` dropped, without asking the
/// file system.
fn lexical(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Where `folder` really lies, when that is inside the worktree and its
/// parent is reached without a symlink on the way; else why not. Nothing
/// is removed or linked at a folder that is not.
fn confined(worktree: &Path, folder: &Path) -> Result<PathBuf, String> {
    let root =
        std::fs::canonicalize(worktree).map_err(|e| format!("{}: {e}", worktree.display()))?;
    let (Some(parent), Some(name)) = (folder.parent(), folder.file_name()) else {
        return Err("it is not a folder of the worktree".to_owned());
    };
    let lex = lexical(parent);
    let rel = lex
        .strip_prefix(lexical(worktree))
        .or_else(|_| lex.strip_prefix(&root))
        .map_err(|_| "it lies outside the worktree".to_owned())?
        .to_path_buf();
    let real = std::fs::canonicalize(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    if real != root.join(&rel) {
        return Err(format!(
            "`{}` is reached through a symlink (to {}), so it is not the worktree's own",
            rel.display(),
            real.display()
        ));
    }
    Ok(real.join(name))
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
    fn the_section_parses_with_its_defaults_and_refuses_patterns_outside() {
        assert_eq!(parse("").unwrap(), Config::default());
        let c = parse("[worktree]\nprepare = \"off\"\ninclude = [\".env\", \"config/*.local.json\", \".env\"]\n").unwrap();
        assert!(!c.prepare);
        assert_eq!(c.include, [".env", "config/*.local.json"]);
        for bad in ["", "../up", "a/../b", "~/x", ".git/config"] {
            let e = parse(&format!("[worktree]\ninclude = [\"{bad}\"]\n")).unwrap_err();
            assert!(e.starts_with("[worktree] include: "), "{bad}: {e}");
        }
        assert!(parse("[worktree]\nprepare = \"always\"\n").is_err());
        assert!(parse("[worktree]\nlink = []\n").is_err());
    }

    #[test]
    fn the_record_is_kept_beside_the_worktree_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let wt = dir.path().join("001-x");
        std::fs::create_dir_all(&wt).unwrap();
        assert_eq!(record_path(&wt), dir.path().join("001-x.prepare.json"));
        assert_eq!(load(&wt), Record::default());
        let record = Record {
            roots: vec![Entry {
                dir: String::new(),
                toolchain: Toolchain::Uv,
                lockfile: "uv.lock".to_owned(),
                lock_sha256: "ab".to_owned(),
                why: None,
                at: 1,
            }],
        };
        store(&wt, &record).unwrap();
        assert_eq!(load(&wt), record);
        forget(&wt);
        assert!(!record_path(&wt).exists());
    }

    #[test]
    fn a_folder_reached_through_a_symlinked_parent_is_never_touched() {
        let checkout = tempfile::tempdir().unwrap();
        let api = checkout.path().join("api");
        std::fs::create_dir_all(api.join("node_modules/left-pad")).unwrap();
        std::fs::write(api.join("node_modules/left-pad/index.js"), "x").unwrap();
        std::fs::write(api.join("package.json"), "{}").unwrap();
        std::fs::write(api.join("package-lock.json"), "{}").unwrap();
        let wt = repo(&[("api/package.json", "{}"), ("api/package-lock.json", "{}")]);
        // The worktree's `api` replaced by a link to the checkout's.
        std::fs::remove_dir_all(wt.path().join("api")).unwrap();
        std::os::unix::fs::symlink(&api, wt.path().join("api")).unwrap();
        let entry = Entry {
            dir: "api".to_owned(),
            toolchain: Toolchain::Npm,
            lockfile: "api/package-lock.json".to_owned(),
            lock_sha256: "an older lockfile".to_owned(),
            why: None,
            at: 0,
        };
        let p = Prepare {
            worktree: wt.path(),
            project: wt.path(),
            config: &Config::default(),
            runner: &Runner::Plain,
            state: None,
        };
        for _ in 0..2 {
            store(
                wt.path(),
                &Record {
                    roots: vec![entry.clone()],
                },
            )
            .unwrap();
            let f = prepare(&p).unwrap_err();
            assert!(
                f.message.contains("reached through a symlink"),
                "{}",
                f.message
            );
            assert!(api.join("node_modules/left-pad/index.js").is_file());
            // Half prepared: forgotten, so the next run starts over.
            assert!(!record_path(wt.path()).exists());
            store(
                wt.path(),
                &Record {
                    roots: vec![entry.clone()],
                },
            )
            .unwrap();
            let f = recheck(&p).unwrap_err();
            assert!(
                f.message.contains("reached through a symlink"),
                "{}",
                f.message
            );
            assert!(api.join("node_modules/left-pad/index.js").is_file());
        }
        // Reused as recorded, it would not be the worktree's own either.
        store(
            wt.path(),
            &Record {
                roots: vec![Entry {
                    lock_sha256: sha256_file(&api.join("package-lock.json")).unwrap(),
                    ..entry
                }],
            },
        )
        .unwrap();
        assert!(prepare(&p).is_err());
        assert!(api.join("node_modules/left-pad/index.js").is_file());
    }

    #[test]
    fn a_link_where_a_folder_belongs_goes_itself_and_never_what_it_points_to() {
        let checkout = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(checkout.path().join("node_modules/a")).unwrap();
        std::fs::write(checkout.path().join("node_modules/a/index.js"), "x").unwrap();
        let wt = repo(&[("package.json", "{}"), ("package-lock.json", "{}")]);
        std::os::unix::fs::symlink(
            checkout.path().join("node_modules"),
            wt.path().join("node_modules"),
        )
        .unwrap();
        // The install itself fails here (no npm cache holds this), after the link went.
        let _ = install_at(
            &Prepare {
                worktree: wt.path(),
                project: wt.path(),
                config: &Config::default(),
                runner: &Runner::Plain,
                state: None,
            },
            &roots(wt.path())[0],
            &wt.path().join("node_modules"),
        );
        assert!(
            std::fs::symlink_metadata(wt.path().join("node_modules"))
                .map_or(true, |m| !m.file_type().is_symlink())
        );
        assert!(checkout.path().join("node_modules/a/index.js").is_file());
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
    fn a_workspace_members_lockfile_above_it_governs_it() {
        let r = repo(&[
            ("package.json", "{\"workspaces\": [\"app\"]}"),
            ("package-lock.json", "{}"),
            (
                "app/package.json",
                "{\"devDependencies\": {\"vitest\": \"^3\"}}",
            ),
            ("app/speccoder.toml", ""),
        ]);
        let app = r.path().join("app");
        let got = roots(&app);
        assert_eq!(
            got,
            [Root {
                dir: "..".to_owned(),
                toolchain: Toolchain::Npm,
                lockfile: "../package-lock.json".to_owned(),
            }]
        );
        assert_eq!(missing_lockfile(&app, &got), None);
        // Its folder is the repository root's, inside the tree.
        let folder = app.join(got[0].folder());
        assert_eq!(
            confined(r.path(), &folder).unwrap(),
            std::fs::canonicalize(r.path())
                .unwrap()
                .join("node_modules")
        );
        // A lockfile nobody committed governs nothing.
        let r = repo(&[(
            "app/package.json",
            "{\"devDependencies\": {\"vitest\": \"^3\"}}",
        )]);
        std::fs::write(r.path().join("package.json"), "{}").unwrap();
        std::fs::write(r.path().join("package-lock.json"), "{}").unwrap();
        let app = r.path().join("app");
        assert!(roots(&app).is_empty());
        assert!(missing_lockfile(&app, &[]).is_some());
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
