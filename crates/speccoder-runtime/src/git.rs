//! A thin wrapper over the git CLI — the user's git, with the user's
//! configuration, for anything that writes.
//!
//! Every call is argv-only, never prompts, takes no optional locks, and runs
//! with `LC_ALL=C` so output is parseable. Nothing here contacts a remote.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A failed git invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub String);

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// git, run in one directory.
#[derive(Debug, Clone)]
pub struct Git {
    dir: PathBuf,
}

/// A git invocation's result.
struct Output {
    ok: bool,
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

impl Git {
    /// git with `dir` as working directory.
    #[must_use]
    pub fn at(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// The working directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn exec(
        &self,
        args: &[&str],
        env: &[(&str, &OsStr)],
        stdin: Option<&[u8]>,
    ) -> Result<Output, GitError> {
        let mut cmd = Command::new("git");
        // What the environment says about the repository is not used: only
        // the directory, and what speccoder passes.
        for k in INHERITED {
            cmd.env_remove(k);
        }
        for (k, _) in std::env::vars_os() {
            let k = k.to_string_lossy();
            if k.starts_with("GIT_CONFIG_KEY_") || k.starts_with("GIT_CONFIG_VALUE_") {
                cmd.env_remove(k.as_ref());
            }
        }
        // A worktree's `.git` file is checked and its git directory pinned:
        // a command that rewrote it cannot make this git read another
        // repository's configuration (and run its filters).
        // `init` and `clone` make a repository: nothing above them is theirs.
        let making = matches!(subcommand(args), Some("init" | "clone"));
        if let Some((root, git_dir)) = if making { None } else { pinned(&self.dir)? } {
            cmd.env("GIT_DIR", git_dir).env("GIT_WORK_TREE", root);
        }
        // A repository's own configuration never runs code through
        // speccoder's git: no fsmonitor, no hooks, no external diff or
        // text conversion in what it reads.
        cmd.args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ]);
        match args.split_first() {
            Some((sub, rest)) if matches!(*sub, "diff" | "show" | "log") => {
                cmd.arg(sub)
                    .args(["--no-ext-diff", "--no-textconv"])
                    .args(rest);
            }
            _ => {
                cmd.args(args);
            }
        }
        cmd.current_dir(&self.dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| GitError(format!("cannot run git: {e}")))?;
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input)
                .map_err(|e| GitError(format!("git stdin: {e}")))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| GitError(format!("git: {e}")))?;
        Ok(Output {
            ok: out.status.success(),
            code: out.status.code(),
            stdout: out.stdout,
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }

    fn checked(
        &self,
        args: &[&str],
        env: &[(&str, &OsStr)],
        stdin: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitError> {
        let out = self.exec(args, env, stdin)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(GitError(format!("git {}: {}", args.join(" "), out.stderr)))
        }
    }

    /// Runs `git <args>`, returning stdout.
    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        self.checked(args, &[], None)
    }

    /// Runs `git <args>` with `env` set as well.
    pub fn run_with_env(&self, args: &[&str], env: &[(&str, &OsStr)]) -> Result<Vec<u8>, GitError> {
        self.checked(args, env, None)
    }

    /// Runs `git <args>` feeding `input` on stdin.
    pub fn run_with_stdin(&self, args: &[&str], input: &[u8]) -> Result<Vec<u8>, GitError> {
        self.checked(args, &[], Some(input))
    }

    /// Runs `git <args>` with a separate index file, optionally feeding stdin.
    pub fn run_with_index(
        &self,
        index: &Path,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitError> {
        self.checked(args, &[("GIT_INDEX_FILE", index.as_os_str())], stdin)
    }

    /// Runs `git <args>` and returns stdout as trimmed UTF-8.
    pub fn line(&self, args: &[&str]) -> Result<String, GitError> {
        let out = self.run(args)?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Whether `git <args>` succeeds (exit 0); errors only if git can't run.
    pub fn succeeds(&self, args: &[&str]) -> Result<bool, GitError> {
        Ok(self.exec(args, &[], None)?.ok)
    }

    /// The exit code of `git <args>` (`None`: ended by a signal); errors
    /// only if git can't run.
    pub fn code(&self, args: &[&str]) -> Result<Option<i32>, GitError> {
        Ok(self.exec(args, &[], None)?.code)
    }

    /// The exit code, stdout and trimmed stderr of `git <args>`, whatever
    /// the code (`merge-tree` says "conflicts" with 1); errors only if git
    /// can't run.
    pub fn outcome(&self, args: &[&str]) -> Result<(Option<i32>, Vec<u8>, String), GitError> {
        let out = self.exec(args, &[], None)?;
        Ok((out.code, out.stdout, out.stderr))
    }

    /// Runs `git <args>` with `env` set as well, returning stdout, or the
    /// trimmed stderr alone as the error.
    pub fn quiet(&self, args: &[&str], env: &[(&str, &OsStr)]) -> Result<Vec<u8>, GitError> {
        let out = self.exec(args, env, None)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(GitError(out.stderr))
        }
    }

    /// The working tree root containing `dir`, or `None` outside a repository.
    pub fn toplevel(&self) -> Result<Option<PathBuf>, GitError> {
        let out = self.exec(&["rev-parse", "--show-toplevel"], &[], None)?;
        if !out.ok {
            return Ok(None);
        }
        let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        Ok(Some(PathBuf::from(path)))
    }

    /// The commit HEAD points at, or `None` when HEAD is unborn.
    pub fn head(&self) -> Result<Option<String>, GitError> {
        let out = self.exec(&["rev-parse", "--verify", "-q", "HEAD^{commit}"], &[], None)?;
        match (out.ok, out.code) {
            (true, _) => Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())),
            (false, Some(1)) => Ok(None),
            (false, _) => Err(GitError(format!("git rev-parse HEAD: {}", out.stderr))),
        }
    }

    /// An absolute path git uses inside the repository (`--git-path`).
    pub fn git_path(&self, name: &str) -> Result<PathBuf, GitError> {
        let p = PathBuf::from(self.line(&["rev-parse", "--git-path", name])?);
        Ok(if p.is_absolute() { p } else { self.dir.join(p) })
    }
}

/// The words a refused, redirected worktree's error carries.
pub const REDIRECTED: &str = "speccoder's git refuses it";

/// Variables that would point speccoder's git at another repository,
/// index or configuration; cleared before every call (a caller's own env
/// is applied after).
const INHERITED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_EXEC_PATH",
    "GIT_ATTR_SOURCE",
    "GIT_NAMESPACE",
    "GIT_EXTERNAL_DIFF",
];

/// The worktree root above `dir` and its checked git directory, when the
/// root's `.git` is a file (a linked worktree or a submodule); `None` for
/// a `.git` directory or outside a repository.
///
/// A working tree speccoder made (anything under `<state>/<id>/`, `id`
/// naming the repository by a hash of its git directory) is held to that
/// repository: its `.git` is a file naming `<git dir>/worktrees/<name>`,
/// whose back-link names this `.git`, and whose git directory hashes to
/// `id` — nothing an attempt can forge. Elsewhere (the person's own linked
/// worktrees and submodules) the target must exist outside the tree and
/// outside every sandbox's private temporary directory, and a back-link,
/// when there is one, must name this `.git`.
///
/// # Errors
///
/// The `.git` points somewhere else: the refusal says where.
pub fn pinned(dir: &Path) -> Result<Option<(PathBuf, PathBuf)>, GitError> {
    let start = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let Some(root) = start
        .ancestors()
        .find(|a| std::fs::symlink_metadata(a.join(".git")).is_ok())
    else {
        return Ok(None);
    };
    let refuse = |to: &str| {
        GitError(format!(
            "the worktree's .git was changed to point at {to}; {REDIRECTED} — open the project again; if you did not move it, an attempt changed it"
        ))
    };
    let made = made_by_speccoder(root);
    let dot_git = root.join(".git");
    let Ok(meta) = std::fs::symlink_metadata(&dot_git) else {
        return Ok(None);
    };
    // A link is read as what it names, except in a tree speccoder made.
    let meta = if meta.file_type().is_symlink() {
        if made.is_some() {
            return Err(refuse("a link"));
        }
        match std::fs::metadata(&dot_git) {
            Ok(m) => m,
            Err(_) => return Err(refuse("a link to nothing")),
        }
    } else {
        meta
    };
    if meta.is_dir() {
        // speccoder's trees are linked worktrees: a `.git` directory there
        // is one an attempt made.
        return match made {
            Some(_) => Err(refuse("a git directory inside the worktree")),
            None => Ok(None),
        };
    }
    if !meta.is_file() {
        return Err(refuse("something that is not a file"));
    }
    let text = std::fs::read_to_string(&dot_git)
        .map_err(|e| GitError(format!("{}: {e}", dot_git.display())))?;
    let Some(target) = text.lines().next().and_then(|l| l.strip_prefix("gitdir:")) else {
        return Err(refuse("nothing git can read"));
    };
    let target = target.trim();
    let shown = target.to_owned();
    let Ok(target) = std::fs::canonicalize(root.join(target)) else {
        return Err(refuse(&shown));
    };
    let real_dot_git = std::fs::canonicalize(&dot_git).unwrap_or_else(|_| dot_git.clone());
    let back = std::fs::read_to_string(target.join("gitdir"))
        .ok()
        .map(|b| std::fs::canonicalize(target.join(b.trim())).is_ok_and(|b| b == real_dot_git));
    if let Some(id) = made {
        let common = target
            .parent()
            .filter(|w| w.file_name().is_some_and(|n| n == "worktrees"))
            .and_then(Path::parent);
        let ok = back == Some(true)
            && common.is_some_and(|c| {
                crate::integrity::sha256_hex(c.to_string_lossy().as_bytes()).get(..16)
                    == Some(id.as_str())
            });
        if !ok {
            return Err(refuse(&shown));
        }
        return Ok(Some((root.to_path_buf(), target)));
    }
    let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    // A sandbox's private tmp is `speccoder-` and 12 hex digits
    // (`sandbox::Profile::new`).
    let in_private_tmp = target.strip_prefix(&tmp).is_ok_and(|rest| {
        rest.components().next().is_some_and(|c| {
            c.as_os_str()
                .to_string_lossy()
                .strip_prefix("speccoder-")
                .is_some_and(|d| d.len() == 12 && d.bytes().all(|b| b.is_ascii_hexdigit()))
        })
    });
    if target.starts_with(root) || in_private_tmp || !target.is_dir() || back == Some(false) {
        return Err(refuse(&shown));
    }
    Ok(Some((root.to_path_buf(), target)))
}

/// The repository id of a working tree speccoder made: `root` lies under
/// `<state>/<id>/`, `id` 16 hex digits ([`crate::worktree::repo_state_dir`]).
fn made_by_speccoder(root: &Path) -> Option<String> {
    let state = crate::worktree::state_dir()?;
    let state = std::fs::canonicalize(&state).unwrap_or(state);
    let rest = root.strip_prefix(&state).ok()?;
    let mut parts = rest.components();
    let id = parts.next()?.as_os_str().to_string_lossy().into_owned();
    parts.next()?;
    (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
}

/// The subcommand of `args` (past `-c`/`-C` and their values).
fn subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "-c" | "-C" => {
                it.next();
            }
            a if a.starts_with('-') => {}
            a => return Some(a),
        }
    }
    None
}

/// Splits NUL-terminated git output into UTF-8 (lossy) fields.
#[must_use]
pub fn nul_fields(out: &[u8]) -> Vec<String> {
    out.split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect()
}
