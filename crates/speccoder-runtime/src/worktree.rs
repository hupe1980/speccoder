//! Isolation and checkpoints.
//!
//! speccoder works in a git worktree on `speccoder/<feature>` under its own
//! state directory, so the user's checkout is never touched. A checkpoint is a
//! git tree object of the whole working tree (tracked and untracked, not
//! ignored), written through a temporary index: HEAD, the index and refs never
//! move. Restoring a checkpoint rewrites only the paths that differ.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::format::CommitPrecondition;
use crate::git::{Git, GitError, nul_fields};

/// Why speccoder will not work in a worktree here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// What is wrong, in one line.
    pub message: String,
    /// Offending paths (repository-relative) with a reason each.
    pub paths: Vec<(String, String)>,
    /// What to do.
    pub hint: String,
    /// The catalog code (`GIT`, `NO-REPOSITORY`, `UNCOMMITTED-INTENT`).
    pub code: &'static str,
}

impl Refusal {
    fn new(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code: "GIT",
            message: message.into(),
            paths: Vec::new(),
            hint: hint.into(),
        }
    }
}

impl From<GitError> for Refusal {
    fn from(e: GitError) -> Self {
        Self::new(
            e.0,
            "check that git is installed and the repository is healthy",
        )
    }
}

/// speccoder's state directory: `$SPECCODER_STATE_DIR`, else
/// `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`.
#[must_use]
pub fn state_dir() -> Option<PathBuf> {
    let var = |k| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("SPECCODER_STATE_DIR")
        .or_else(|| var("XDG_STATE_HOME").map(|p| p.join("speccoder")))
        .or_else(|| var("HOME").map(|h| h.join(".local/state/speccoder")))
}

/// A repository's directory under the state dir, keyed by its canonical
/// common git directory (shared by all of its worktrees).
pub fn repo_state_dir(state: &Path, repo: &Git) -> Result<PathBuf, GitError> {
    let common = repo.line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let canonical = fs::canonicalize(&common).unwrap_or_else(|_| PathBuf::from(&common));
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let id: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    Ok(state.join(id))
}

/// A project's directory under the state dir: per git repository when the
/// project is in one (shared by its worktrees), else per canonical path.
#[must_use]
pub fn project_state_dir(state: &Path, project: &Path) -> PathBuf {
    let git = Git::at(project);
    if let Ok(Some(_)) = git.toplevel()
        && let Ok(dir) = repo_state_dir(state, &git)
    {
        return dir;
    }
    let canonical = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let id: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    state.join(id)
}

/// A worktree speccoder works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Absolute path of the worktree.
    pub path: PathBuf,
    /// `speccoder/<feature>`.
    pub branch: String,
    /// Commit the branch points at.
    pub base: String,
    /// Created now (`false`: reused).
    pub created: bool,
}

/// A detached worktree of a checkout's `HEAD` for one check (the review's
/// red check): never the person's checkout, never the feature's
/// worktree. Removed when dropped — on every way out.
#[derive(Debug)]
pub struct Scratch {
    repo: PathBuf,
    /// The worktree's root.
    pub path: PathBuf,
}

/// A scratch worktree of `checkout`'s `HEAD` under `under`.
pub fn scratch(checkout: &Path, under: &Path) -> Result<Scratch, GitError> {
    let git = Git::at(checkout);
    let top = git
        .toplevel()?
        .ok_or_else(|| GitError("not a git repository".to_owned()))?;
    fs::create_dir_all(under).map_err(|e| GitError(format!("{}: {e}", under.display())))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = under.join(format!("scratch-{}-{nanos}", std::process::id()));
    let path_str = path.display().to_string();
    Git::at(&top).run(&["worktree", "add", "--quiet", "--detach", &path_str, "HEAD"])?;
    Ok(Scratch { repo: top, path })
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let path = self.path.display().to_string();
        let git = Git::at(&self.repo);
        let _ = git.run(&["worktree", "remove", "--force", &path]);
        let _ = fs::remove_dir_all(&self.path);
        let _ = git.run(&["worktree", "prune"]);
    }
}

/// Creates or reuses the worktree for `feature`.
///
/// `repo_root` is the git working tree of the user's checkout; `committed`
/// says which repository-relative spec paths must be committed.
pub fn ensure_worktree(
    repo_root: &Path,
    state: &Path,
    feature: &str,
    committed: impl Into<CommitPrecondition>,
) -> Result<Worktree, Refusal> {
    let artifacts = &committed.into();
    let repo = Git::at(repo_root);
    let Some(_head) = repo.head()? else {
        return Err(Refusal {
            code: "NO-REPOSITORY",
            ..Refusal::new(
                "the repository has no commits yet; a worktree needs one",
                artifacts.no_commits_hint,
            )
        });
    };
    require_committed(&repo, artifacts)?;

    let branch = format!("speccoder/{feature}");
    let path = repo_state_dir(state, &repo)?
        .join("worktrees")
        .join(feature);

    // Forget worktrees whose directories were removed by hand.
    repo.run(&["worktree", "prune"])?;
    if let Some(existing) = worktree_for(&repo, &branch)? {
        bring_spec_changes(&repo, &existing, &branch, artifacts)?;
        let base = Git::at(&existing).line(&["rev-parse", "HEAD"])?;
        return Ok(Worktree {
            path: existing,
            branch,
            base,
            created: false,
        });
    }
    if path.exists() {
        return Err(Refusal::new(
            format!(
                "{} exists but is not a worktree of this repository",
                path.display()
            ),
            "move it away; speccoder does not delete directories it did not create",
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            Refusal::new(
                format!("{}: {e}", parent.display()),
                "check permissions of the state directory",
            )
        })?;
    }
    let path_str = path.to_string_lossy().into_owned();
    let branch_ref = format!("refs/heads/{branch}");
    if repo.succeeds(&["rev-parse", "--verify", "-q", &branch_ref])? {
        // The branch survived its worktree: re-attach, keeping its history.
        repo.run(&["worktree", "add", "--quiet", &path_str, &branch])?;
    } else {
        repo.run(&[
            "worktree", "add", "--quiet", "-b", &branch, &path_str, "HEAD",
        ])?;
    }
    let path = fs::canonicalize(&path).unwrap_or(path);
    let base = Git::at(&path).line(&["rev-parse", "HEAD"])?;
    Ok(Worktree {
        path,
        branch,
        base,
        created: true,
    })
}

/// When the person's checkout changed the spec artifacts after the
/// speccoder branch forked (a fixed example, a clarified requirement), the
/// worktree would work from the old spec: merge their `HEAD` in first. A
/// conflict is theirs to resolve; nothing is half-merged.
fn bring_spec_changes(
    repo: &Git,
    worktree: &Path,
    branch: &str,
    artifacts: &CommitPrecondition,
) -> Result<(), Refusal> {
    let head = repo.line(&["rev-parse", "HEAD"])?;
    let base = repo.line(&["merge-base", branch, &head])?;
    // What decides a run moves the branch too: the spec, and the setup the
    // checks run with (the configuration, the nextest profile, what is
    // ignored, the lockfiles).
    let mut paths = artifacts.paths();
    paths.extend(SETUP.iter().map(|p| (*p).to_owned()));
    let mut diff = vec!["diff", "--quiet", &base, &head, "--"];
    diff.extend(paths.iter().map(String::as_str));
    let moved = !repo.succeeds(&diff)?;
    if !moved {
        return Ok(());
    }
    let wt = Git::at(worktree);
    // A lockfile the checks wrote here, untracked, is their output: the one
    // the checkout committed takes its place.
    for lock in LOCKFILES {
        if worktree.join(lock).is_file()
            && wt
                .line(&["ls-files", "--error-unmatch", "--", lock])
                .is_err()
            && repo.succeeds(&["cat-file", "-e", &format!("{head}:{lock}")])?
        {
            let _ = fs::remove_file(worktree.join(lock));
        }
    }
    let merged = wt.run(&[
        "-c",
        "user.name=speccoder",
        "-c",
        "user.email=speccoder@users.noreply.invalid",
        "-c",
        "commit.gpgsign=false",
        "merge",
        "-q",
        "--no-edit",
        "-m",
        "speccoder: bring in the spec and setup changes from the checkout",
        &head,
    ]);
    if let Err(e) = merged {
        let _ = wt.run(&["merge", "--abort"]);
        return Err(Refusal::new(
            format!(
                "the spec changed in your checkout and could not be merged into {branch}: {}",
                e.0.lines().next().unwrap_or_default()
            ),
            format!(
                "merge it by hand in {} (git merge {head}), then run again",
                worktree.display()
            ),
        ));
    }
    Ok(())
}

/// The branch of a lane: one task of `feature` run in parallel.
#[must_use]
pub fn lane_branch(feature: &str, lane: &str) -> String {
    format!("speccoder-lane/{feature}/{lane}")
}

/// A fresh lane worktree for `lane` of `feature`, started at `start` (the
/// feature branch's tip). A lane left behind is replaced: lanes are
/// speccoder's scratch space; their work reaches the feature branch only
/// by merging.
pub fn ensure_lane(
    repo_root: &Path,
    state: &Path,
    feature: &str,
    lane: &str,
    start: &str,
) -> Result<Worktree, Refusal> {
    let repo = Git::at(repo_root);
    let branch = lane_branch(feature, lane);
    let path = repo_state_dir(state, &repo)?
        .join("worktrees")
        .join(format!("{feature}@{lane}"));
    remove_lane(repo_root, feature, lane)?;
    if path.exists() {
        return Err(Refusal::new(
            format!(
                "{} exists but is not a worktree of this repository",
                path.display()
            ),
            "move it away; speccoder does not delete directories it did not create",
        ));
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let path_str = path.to_string_lossy().into_owned();
    repo.run(&[
        "worktree", "add", "--quiet", "-B", &branch, &path_str, start,
    ])?;
    let path = fs::canonicalize(&path).unwrap_or(path);
    Ok(Worktree {
        path,
        branch,
        base: start.to_owned(),
        created: true,
    })
}

/// Removes `lane`'s worktree and branch, if there are any.
pub fn remove_lane(repo_root: &Path, feature: &str, lane: &str) -> Result<(), GitError> {
    let repo = Git::at(repo_root);
    let branch = lane_branch(feature, lane);
    repo.run(&["worktree", "prune"])?;
    if let Some(existing) = worktree_for(&repo, &branch)? {
        repo.run(&["worktree", "remove", "--force", &existing.to_string_lossy()])?;
    }
    if repo.succeeds(&[
        "rev-parse",
        "--verify",
        "-q",
        &format!("refs/heads/{branch}"),
    ])? {
        repo.run(&["branch", "-q", "-D", &branch])?;
    }
    Ok(())
}

/// The worktree checked out on `branch`, if any.
pub fn worktree_for(repo: &Git, branch: &str) -> Result<Option<PathBuf>, GitError> {
    let fields = nul_fields(&repo.run(&["worktree", "list", "--porcelain", "-z"])?);
    let wanted = format!("branch refs/heads/{branch}");
    let mut current: Option<PathBuf> = None;
    for f in fields {
        if let Some(p) = f.strip_prefix("worktree ") {
            current = Some(PathBuf::from(p));
        } else if f == wanted {
            return Ok(current);
        }
    }
    Ok(None)
}

/// Spec Kit paths a worktree must contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifacts {
    /// Repository-relative `.specify` directory.
    pub specify_dir: String,
    /// Repository-relative feature directory, e.g. `specs/001-auth`.
    pub feature_dir: String,
}

impl From<&Artifacts> for CommitPrecondition {
    fn from(a: &Artifacts) -> Self {
        crate::format::speckit::precondition(&a.specify_dir, &a.feature_dir)
    }
}

/// Refuses unless the spec artifacts are committed as they are on disk:
/// a worktree contains only commits, so anything else would be an agent
/// working from a different spec than the user's.
fn require_committed(repo: &Git, a: &CommitPrecondition) -> Result<(), Refusal> {
    let mut paths: Vec<(String, String)> = Vec::new();

    for dir in &a.strict {
        let out = repo.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored=traditional",
            "--",
            dir,
        ])?;
        paths.extend(status_entries(&out));
    }
    for dir in &a.tracked {
        let out = repo.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            dir,
        ])?;
        paths.extend(
            status_entries(&out)
                .into_iter()
                .filter(|(p, _)| !(a.runtime)(p)),
        );
    }

    // And the essentials must be tracked at all (a fully ignored directory
    // has no status entries of its own here).
    for p in &a.essentials {
        let tracked = !repo.run(&["ls-files", "--", p])?.is_empty();
        if !tracked && repo.dir().join(p).exists() && !paths.iter().any(|(q, _)| q == p) {
            let reason = if repo.succeeds(&["check-ignore", "-q", "--", p])? {
                "ignored"
            } else {
                "untracked"
            };
            paths.push((p.clone(), reason.to_owned()));
        }
    }

    if paths.is_empty() {
        return Ok(());
    }
    paths.sort();
    paths.dedup();
    Err(Refusal {
        code: "UNCOMMITTED-INTENT",
        message: a.message.to_owned(),
        paths,
        hint: a.hint.to_owned(),
    })
}

/// `git status --porcelain=v1 -z` entries as (path, reason).
fn status_entries(out: &[u8]) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut fields = nul_fields(out).into_iter();
    while let Some(f) = fields.next() {
        if f.len() < 4 {
            continue;
        }
        let (code, path) = f.split_at(3);
        let code = code.trim_end();
        let reason = match code {
            "??" => "untracked",
            "!!" => "ignored",
            _ => "modified",
        };
        // Renames carry the original path as a second field.
        if code.starts_with('R') || code.starts_with('C') {
            fields.next();
        }
        entries.push((path.to_owned(), reason.to_owned()));
    }
    entries
}

/// The build output a check's toolchain writes into the tree it runs in.
#[must_use]
pub fn build_output(argv: &[String]) -> &'static [&'static str] {
    let program = argv
        .first()
        .map(|a| a.rsplit('/').next().unwrap_or(a))
        .unwrap_or_default();
    match program {
        "cargo" => &["target/"],
        "npm" | "npx" | "pnpm" | "yarn" | "bun" => &["node_modules/"],
        "pytest" | "python" | "python3" | "uv" => &[".pytest_cache/", "__pycache__/"],
        _ => &[],
    }
}

/// What the checks run with besides the spec: a change in the checkout
/// reaches the feature's branch before a run.
pub const SETUP: &[&str] = &[
    "speccoder.toml",
    ".config/nextest.toml",
    ".gitignore",
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "go.sum",
];

/// Lockfiles a toolchain writes when the repository has none committed.
pub const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
    "go.sum",
];

/// What checks leave behind in `dir` that git does not know: build output
/// `.gitignore` does not list (`target/`, `node_modules/`, caches), and
/// lockfiles never committed. No one's edits — the setup's gaps.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Leftovers {
    /// Directories to ignore.
    pub ignore: Vec<String>,
    /// Lockfiles to commit.
    pub commit: Vec<String>,
}

impl Leftovers {
    /// Nothing left behind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ignore.is_empty() && self.commit.is_empty()
    }

    /// The fix, as one command line run in `checkout`, committing `also`
    /// with it; a lockfile only the worktree's checks wrote is copied from
    /// `worktree`.
    #[must_use]
    pub fn fix(&self, checkout: &Path, worktree: &Path, also: &[String]) -> String {
        let mut steps = Vec::new();
        for f in &self.commit {
            if !checkout.join(f).exists() {
                steps.push(format!("cp '{}' .", worktree.join(f).display()));
            }
        }
        if !self.ignore.is_empty() {
            steps.push(format!(
                "printf '{}\\n' >> .gitignore",
                self.ignore.join("\\n")
            ));
        }
        let mut add: Vec<&str> = also
            .iter()
            .chain(&self.commit)
            .map(String::as_str)
            .collect();
        if !self.ignore.is_empty() {
            add.push(".gitignore");
        }
        steps.push(format!(
            "git add {} && git commit -m \"speccoder: setup\"",
            add.join(" ")
        ));
        steps.join(" && ")
    }
}

/// [`Leftovers`] in `dir`.
#[must_use]
pub fn leftovers(dir: &Path) -> Leftovers {
    const BUILD: [&str; 4] = ["target", "node_modules", ".pytest_cache", "__pycache__"];
    let Ok(out) = Git::at(dir).run(&["status", "--porcelain=v1", "--untracked-files=normal"])
    else {
        return Leftovers::default();
    };
    let mut found = Leftovers::default();
    for p in String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| l.strip_prefix("?? "))
    {
        let p = p.trim_end_matches('/');
        let first = p.split('/').next().unwrap_or(p);
        if BUILD.contains(&first) {
            found.ignore.push(format!("{first}/"));
        } else if LOCKFILES.contains(&p) {
            found.commit.push(p.to_owned());
        }
    }
    found.ignore.sort();
    found.ignore.dedup();
    found
}

/// Refuses when the working tree has any uncommitted change (for in-place mode).
pub fn require_clean(dir: &Path) -> Result<(), Refusal> {
    let out = Git::at(dir).run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    let paths = status_entries(&out);
    if paths.is_empty() {
        Ok(())
    } else {
        Err(Refusal {
            code: "DIRTY-CHECKOUT",
            message: "the working tree has uncommitted changes".to_owned(),
            paths,
            hint: "commit or stash them first".to_owned(),
        })
    }
}

// ── Checkpoints ─────────────────────────────────────────────────────────────

/// A snapshot of a working tree: a git tree object id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint(pub String);

/// What happened to a path between two checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeKind {
    /// Present only in the later checkpoint.
    Added,
    /// Content, mode or type differs.
    Modified,
    /// Present only in the earlier checkpoint.
    Deleted,
}

/// One changed path (repository-relative).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The path.
    pub path: String,
    /// What happened.
    pub kind: ChangeKind,
}

/// A temporary index file that is removed when dropped.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new(tmp: &Path) -> Result<Self, GitError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(tmp).map_err(|e| GitError(format!("{}: {e}", tmp.display())))?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(Self(
            tmp.join(format!("index-{}-{nanos}-{n}", std::process::id())),
        ))
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        // Our own temporary file; nothing else is ever removed here.
        let _ = fs::remove_file(&self.0);
    }
}

/// Snapshots the working tree of `dir`. `tmp` holds the temporary
/// index (inside speccoder's state directory).
pub fn checkpoint(dir: &Path, tmp: &Path) -> Result<Checkpoint, GitError> {
    let git = Git::at(dir);
    let index = TempIndex::new(tmp)?;
    // Start from the real index so unchanged files are not rehashed.
    let real = git.git_path("index")?;
    if real.exists() {
        fs::copy(&real, &index.0).map_err(|e| GitError(format!("copy index: {e}")))?;
    } else if git.head()?.is_some() {
        git.run_with_index(&index.0, &["read-tree", "HEAD"], None)?;
    } else {
        git.run_with_index(&index.0, &["read-tree", "--empty"], None)?;
    }
    git.run_with_index(&index.0, &["add", "-A", "--", ":/"], None)?;
    let tree = git.run_with_index(&index.0, &["write-tree"], None)?;
    Ok(Checkpoint(String::from_utf8_lossy(&tree).trim().to_owned()))
}

/// Paths that differ from `from` to `to`.
pub fn diff(dir: &Path, from: &Checkpoint, to: &Checkpoint) -> Result<Vec<Change>, GitError> {
    let out = Git::at(dir).run(&[
        "diff-tree",
        "-r",
        "-z",
        "--no-renames",
        "--name-status",
        &from.0,
        &to.0,
    ])?;
    let fields = nul_fields(&out);
    let mut changes = Vec::new();
    for pair in fields.chunks(2) {
        let [status, path] = pair else { continue };
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('D') => ChangeKind::Deleted,
            _ => ChangeKind::Modified,
        };
        changes.push(Change {
            path: path.clone(),
            kind,
        });
    }
    Ok(changes)
}

/// Every file path in a checkpoint.
pub fn files(dir: &Path, checkpoint: &Checkpoint) -> Result<Vec<String>, GitError> {
    let out = Git::at(dir).run(&["ls-tree", "-r", "--name-only", "-z", &checkpoint.0])?;
    Ok(nul_fields(&out))
}

/// The textual diff between two checkpoints, without context lines.
pub fn patch(dir: &Path, from: &Checkpoint, to: &Checkpoint) -> Result<String, GitError> {
    let out = Git::at(dir).run(&["diff", "--no-color", "--no-renames", "-U0", &from.0, &to.0])?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Makes the working tree of `dir` equal to `target`: differing
/// paths are rewritten from the checkpoint, paths the checkpoint lacks are
/// removed. HEAD, the index and refs are not touched. Returns what changed.
pub fn restore(dir: &Path, tmp: &Path, target: &Checkpoint) -> Result<Vec<Change>, GitError> {
    let git = Git::at(dir);
    let root = git
        .toplevel()?
        .ok_or_else(|| GitError(format!("{} is not in a git repository", dir.display())))?;
    let current = checkpoint(&root, tmp)?;
    let changes = diff(&root, target, &current)?;
    if changes.is_empty() {
        return Ok(changes);
    }

    // Changed relative to target: `Added` = only in current (remove);
    // `Deleted`/`Modified` = take the target's version.
    let mut write: Vec<u8> = Vec::new();
    let mut remove: Vec<&str> = Vec::new();
    for c in &changes {
        if !is_plain_relative(&c.path) {
            return Err(GitError(format!(
                "refusing to restore unsafe path {}",
                c.path
            )));
        }
        match c.kind {
            ChangeKind::Added => remove.push(&c.path),
            ChangeKind::Deleted | ChangeKind::Modified => {
                write.extend_from_slice(c.path.as_bytes());
                write.push(0);
            }
        }
    }
    let rgit = Git::at(&root);
    for path in &remove {
        let full = root.join(path);
        // A path being replaced by a directory/file of another type is handled
        // by checkout-index below; remove the current entry without following links.
        if fs::symlink_metadata(&full).is_ok() {
            fs::remove_file(&full).map_err(|e| GitError(format!("remove {path}: {e}")))?;
            prune_empty_parents(&root, &full);
        }
    }
    if !write.is_empty() {
        let index = TempIndex::new(tmp)?;
        rgit.run_with_index(&index.0, &["read-tree", &target.0], None)?;
        rgit.run_with_index(
            &index.0,
            &["checkout-index", "-f", "-z", "--stdin"],
            Some(&write),
        )?;
    }
    Ok(changes)
}

fn is_plain_relative(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Removes directories left empty by a removal, up to (not including) `root`.
fn prune_empty_parents(root: &Path, removed: &Path) {
    let mut dir = removed.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}
