//! A feature's code as a person reads it: what its branch changed
//! since it forked — by commit, by file — and its files. The revision is
//! never the caller's: it is the feature's branch, or `HEAD` before
//! speccoder worked on it; a file is read only when the tree lists it.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git::Git;

/// One commit of the feature's branch.
#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    /// Full id.
    pub id: String,
    /// Subject line.
    pub subject: String,
    /// `Spec-Task` trailer (`feature/T005`), when a tick.
    pub task: Option<String>,
    /// `Verdict` trailer, when a tick.
    pub verdict: Option<String>,
    /// Author date, ISO 8601.
    pub date: String,
}

/// One changed file.
#[derive(Debug, Clone, Serialize)]
pub struct File {
    /// Path in the repository.
    pub path: String,
    /// Lines added (none for a binary file).
    pub add: Option<u64>,
    /// Lines removed.
    pub del: Option<u64>,
}

/// What the feature's branch changed since it forked.
#[derive(Debug, Clone, Serialize)]
pub struct Changes {
    /// The branch, when speccoder has worked on the feature.
    pub branch: Option<String>,
    /// Where it forked from the person's `HEAD`.
    pub base: Option<String>,
    /// Its commits, newest first.
    pub commits: Vec<Commit>,
    /// The files it changed.
    pub files: Vec<File>,
    /// The whole change as a patch (cut at `PATCH_LIMIT`).
    pub patch: String,
    /// Whether the patch was cut.
    pub cut: bool,
}

/// The largest patch sent whole.
pub const PATCH_LIMIT: usize = 2 * 1024 * 1024;
/// The largest file shown.
pub const FILE_LIMIT: usize = 512 * 1024;

/// The feature's branch, when it exists — by the branch convention, after a
/// snapshot (not the whole report: this runs on every file opened).
fn branch_of(root: &Path, feature: Option<&str>) -> Option<String> {
    let inspected = crate::inspect(root, feature);
    let Some(speccoder_core::CommandData::Inspect(snapshot)) = inspected.data() else {
        return None;
    };
    let branch = format!("speccoder/{}", snapshot.feature.as_ref()?.feature.name);
    Git::at(root)
        .succeeds(&[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ])
        .unwrap_or(false)
        .then_some(branch)
}

/// What the feature's branch changed since it forked from `HEAD`.
#[must_use]
pub fn changes(root: &Path, feature: Option<&str>) -> Changes {
    let empty = |branch| Changes {
        branch,
        base: None,
        commits: Vec::new(),
        files: Vec::new(),
        patch: String::new(),
        cut: false,
    };
    let Some(branch) = branch_of(root, feature) else {
        return empty(None);
    };
    let g = Git::at(root);
    let Ok(head) = g.line(&["rev-parse", "HEAD"]) else {
        return empty(Some(branch));
    };
    let Ok(base) = g.line(&["merge-base", &branch, &head]) else {
        return empty(Some(branch));
    };
    let range = format!("{base}..{branch}");
    let commits = g
        .run(&[
            "log",
            "--no-color",
            "--format=%H%x1f%s%x1f%(trailers:key=Spec-Task,valueonly,separator=%x2c)%x1f%(trailers:key=Verdict,valueonly,separator=%x2c)%x1f%aI%x1e",
            &range,
        ])
        .map(|b| {
            String::from_utf8_lossy(&b)
                .split('\x1e')
                .filter_map(|rec| {
                    let f: Vec<&str> = rec.trim().split('\x1f').collect();
                    let [id, subject, task, verdict, date] = f.as_slice() else { return None };
                    let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
                    Some(Commit {
                        id: (*id).to_owned(),
                        subject: (*subject).to_owned(),
                        task: some(task),
                        verdict: some(verdict),
                        date: (*date).to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // `-z`: paths as they are (no quoting), and a rename as its new path —
    // without it a rename reads `src/{a.rs => b.rs}`.
    let files = g
        .run(&["diff", "--numstat", "-z", "-M", &base, &branch])
        .map(|b| numstat(&b))
        .unwrap_or_default();
    let mut patch = g
        .run(&["diff", "--no-color", "-M", &base, &branch])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let cut = patch.len() > PATCH_LIMIT;
    if cut {
        let mut at = PATCH_LIMIT;
        while !patch.is_char_boundary(at) {
            at -= 1;
        }
        patch.truncate(at);
    }
    Changes {
        branch: Some(branch),
        base: Some(base),
        commits,
        files,
        patch,
        cut,
    }
}

/// `git diff --numstat -z`: `add\tdel\tpath\0`, or for a rename
/// `add\tdel\t\0old\0new\0` (the new path is the file's).
fn numstat(out: &[u8]) -> Vec<File> {
    let text = String::from_utf8_lossy(out);
    let mut fields = text.split('\0');
    let mut files = Vec::new();
    while let Some(head) = fields.next() {
        let mut it = head.splitn(3, '\t');
        let (Some(a), Some(d), Some(p)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let path = if p.is_empty() {
            // A rename: the old path, then the new one.
            let _old = fields.next();
            fields.next().unwrap_or_default().to_owned()
        } else {
            p.to_owned()
        };
        files.push(File {
            path,
            add: a.parse().ok(),
            del: d.parse().ok(),
        });
    }
    files
}

/// The revision whose files are read: the feature's branch, else `HEAD`.
fn revision(root: &Path, feature: Option<&str>) -> String {
    branch_of(root, feature).unwrap_or_else(|| "HEAD".to_owned())
}

/// The files at the feature's revision (at most 20 000).
#[must_use]
pub fn tree(root: &Path, feature: Option<&str>) -> (String, Vec<String>) {
    let rev = revision(root, feature);
    let files = Git::at(root)
        .run(&["ls-tree", "-r", "--name-only", "-z", &rev])
        .map(|b| {
            crate::git::nul_fields(&b)
                .into_iter()
                .take(20_000)
                .collect()
        })
        .unwrap_or_default();
    (rev, files)
}

/// What git does not show at the feature's revision: the worktree's
/// untracked, ignored and modified paths, and what of it would stop a run.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WorktreeView {
    /// The feature's worktree, when there is one.
    pub path: Option<String>,
    /// `{path, state}`: `untracked`, `ignored` (a folder ends in `/`) or
    /// `modified`, at most 2000.
    pub entries: Vec<WorktreeEntry>,
    /// Untracked lockfiles to commit, build output to ignore: a run stops
    /// on them.
    pub blocking: Vec<String>,
    /// The command that fixes them, in the checkout.
    pub fix: Option<String>,
}

/// One path of a [`WorktreeView`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorktreeEntry {
    /// Worktree-relative.
    pub path: String,
    /// `untracked`, `ignored` or `modified`.
    pub state: &'static str,
}

/// The feature's worktree as it is on disk, beyond what git committed.
#[must_use]
pub fn worktree_view(root: &Path, feature: Option<&str>) -> WorktreeView {
    let Some(branch) = branch_of(root, feature) else {
        return WorktreeView::default();
    };
    let Ok(Some(wt)) = crate::worktree::worktree_for(&Git::at(root), &branch) else {
        return WorktreeView::default();
    };
    let out = Git::at(&wt)
        .run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
        ])
        .unwrap_or_default();
    let entries = crate::git::nul_fields(&out)
        .into_iter()
        .filter_map(|e| {
            let (code, path) = (e.get(..2)?, e.get(3..)?.to_owned());
            let state = match code {
                "??" => "untracked",
                "!!" => "ignored",
                _ => "modified",
            };
            Some(WorktreeEntry { path, state })
        })
        .take(2000)
        .collect();
    let s = setup(root, feature);
    WorktreeView {
        path: Some(wt.display().to_string()),
        entries,
        blocking: s
            .config
            .iter()
            .chain(&s.left.commit)
            .chain(&s.left.ignore)
            .cloned()
            .collect(),
        fix: (!s.is_empty()).then(|| s.command(root)),
    }
}

/// What of the project's setup stops a run: what the feature's worktree is
/// left with ([`crate::worktree::Leftovers`]), and the configuration a
/// check reads that the checkout has but never committed — a worktree,
/// checked out from a commit, lacks it.
struct Setup {
    worktree: Option<PathBuf>,
    left: crate::worktree::Leftovers,
    config: Vec<String>,
}

/// The files `init` writes that a run reads from its worktree.
const CONFIG: [&str; 2] = [crate::verify::config::FILE, ".config/nextest.toml"];

fn setup(root: &Path, feature: Option<&str>) -> Setup {
    let worktree = branch_of(root, feature).and_then(|b| {
        crate::worktree::worktree_for(&Git::at(root), &b)
            .ok()
            .flatten()
    });
    let left = worktree
        .as_deref()
        .map(crate::worktree::leftovers)
        .unwrap_or_default();
    let g = Git::at(root);
    let config = CONFIG
        .iter()
        .filter(|p| root.join(p).is_file())
        .filter(|p| {
            g.run(&["status", "--porcelain", "--", p])
                .is_ok_and(|o| !o.trim_ascii().is_empty())
        })
        .map(|p| (*p).to_owned())
        .collect();
    Setup {
        worktree,
        left,
        config,
    }
}

impl Setup {
    fn is_empty(&self) -> bool {
        self.left.is_empty() && self.config.is_empty()
    }

    /// Every file the fix commits.
    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .config
            .iter()
            .chain(&self.left.commit)
            .cloned()
            .collect();
        if !self.left.ignore.is_empty() {
            paths.push(".gitignore".to_owned());
        }
        paths
    }

    fn command(&self, root: &Path) -> String {
        let wt = self.worktree.as_deref().unwrap_or(root);
        self.left.fix(root, wt, &self.config)
    }
}

/// The fix of what in the setup stops a run, as an interface offers it:
/// `None` when nothing does.
#[must_use]
pub fn setup_remedy(root: &Path, feature: Option<&str>) -> Option<speccoder_core::Remedy> {
    let s = setup(root, feature);
    if s.is_empty() {
        return None;
    }
    let paths = s.paths();
    let ignore = if s.left.ignore.is_empty() {
        String::new()
    } else {
        format!(
            "{} added to .gitignore, ",
            speccoder_core::words::listed(&s.left.ignore)
        )
    };
    Some(speccoder_core::Remedy {
        label: format!("Commit {}", speccoder_core::words::listed(&paths)),
        effect: format!("{ignore}in your checkout as `speccoder: setup`; the next run picks it up"),
        command: s.command(root),
        undo: Some("git reset --soft HEAD~1".to_owned()),
        history: true,
        run: speccoder_core::actions::Runnable::FixSetup,
    })
}

/// Fixes the setup that stops a run, in the checkout at `root`, on the
/// person's command: a lockfile only the worktree's checks wrote is copied
/// over, build output is added to `.gitignore`, and exactly those files and
/// the uncommitted configuration are committed (`speccoder: setup`).
#[must_use]
pub fn fix_setup(root: &Path, feature: Option<&str>) -> speccoder_core::CommandResult {
    use speccoder_core::{CommandResult, CommandStatus};
    let refuse = |status, m: String| CommandResult::new("fix", status, &m);
    let s = setup(root, feature);
    if s.is_empty() {
        return CommandResult::new("fix", CommandStatus::Ok, "nothing in the setup stops a run");
    }
    if let Some(wt) = &s.worktree {
        for f in &s.left.commit {
            let to = root.join(f);
            if !to.exists()
                && let Err(e) = std::fs::copy(wt.join(f), &to)
            {
                return refuse(CommandStatus::Error, format!("{f}: {e}"));
            }
        }
    }
    if !s.left.ignore.is_empty() {
        let ignore = root.join(".gitignore");
        let existing = std::fs::read_to_string(&ignore).unwrap_or_default();
        let sep = if existing.is_empty() || existing.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let text = format!("{existing}{sep}{}\n", s.left.ignore.join("\n"));
        if let Err(e) = std::fs::write(&ignore, text) {
            return refuse(CommandStatus::Error, format!(".gitignore: {e}"));
        }
    }
    let paths = s.paths();
    match crate::init::commit_written_as(root, &paths, "speccoder: setup") {
        Ok(id) => CommandResult::new(
            "fix",
            CommandStatus::Ok,
            &format!(
                "committed {} as {id}; the next run takes it into the feature's branch (undo: git reset --soft HEAD~1)",
                speccoder_core::words::listed(&paths)
            ),
        ),
        Err(e) => refuse(CommandStatus::GitPrecondition, e),
    }
}

/// An untracked file of the feature's worktree, read from disk: its text,
/// or why not. Only what [`worktree_view`] lists as untracked.
///
/// # Errors
///
/// Why the file is not shown.
pub fn worktree_file(root: &Path, feature: Option<&str>, path: &str) -> Result<String, String> {
    let view = worktree_view(root, feature);
    if !view
        .entries
        .iter()
        .any(|e| e.path == path && e.state == "untracked")
    {
        return Err(format!("{path} is not an untracked file of the worktree"));
    }
    let wt = PathBuf::from(view.path.unwrap_or_default());
    let bytes = std::fs::read(wt.join(path)).map_err(|e| e.to_string())?;
    if bytes.len() > FILE_LIMIT {
        return Err(format!("{path} is larger than {} KB", FILE_LIMIT / 1024));
    }
    String::from_utf8(bytes).map_err(|_| format!("{path} is not text"))
}

/// One file at the feature's revision: its text, or why not (not in the
/// tree, binary, too large).
///
/// # Errors
///
/// Why the file is not shown.
pub fn file(root: &Path, feature: Option<&str>, path: &str) -> Result<String, String> {
    let (rev, files) = tree(root, feature);
    if !files.iter().any(|f| f == path) {
        return Err(format!("{path} is not in {rev}"));
    }
    let bytes = Git::at(root)
        // `./`: relative to this directory — a project may sit below the
        // repository's top level, and the tree's paths are relative to it.
        .run(&["show", &format!("{rev}:./{path}")])
        .map_err(|e| e.0)?;
    if bytes.len() > FILE_LIMIT {
        return Err(format!("{path} is larger than {} KB", FILE_LIMIT / 1024));
    }
    if bytes.contains(&0) {
        return Err(format!("{path} is binary"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn numstat_reads_renames_and_unusual_paths() {
        let out = b"3\t1\tsrc/lib.rs\x002\t0\t\x00src/a.rs\x00src/b.rs\x00-\t-\tlogo.png\x001\t0\tdocs/\xc3\xbcber.md\x00";
        let files = numstat(out);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["src/lib.rs", "src/b.rs", "logo.png", "docs/über.md"]
        );
        assert_eq!((files[2].add, files[2].del), (None, None));
        assert_eq!(files[1].add, Some(2));
    }

    /// A project below the repository's top level reads its own files, by
    /// its own paths; anything its tree does not list is refused.
    #[test]
    fn a_nested_project_reads_its_files_and_nothing_else() {
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let project = repo.path().join("apps/web");
        std::fs::create_dir_all(project.join("src")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(project.join("src/lib.rs"), "pub fn ok() {}\n")
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(repo.path().join("secret.txt"), "outside\n")
            .unwrap_or_else(|e| panic!("{e}"));
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["add", "-A"]);
        git(repo.path(), &["commit", "-qm", "init"]);
        let (rev, files) = tree(&project, None);
        assert_eq!(rev, "HEAD");
        assert_eq!(files, ["src/lib.rs"]);
        assert_eq!(
            file(&project, None, "src/lib.rs").as_deref(),
            Ok("pub fn ok() {}\n")
        );
        for bad in [
            "../../secret.txt",
            "secret.txt",
            "src/../../../secret.txt",
            "src",
        ] {
            assert!(file(&project, None, bad).is_err(), "{bad}");
        }
    }
}
