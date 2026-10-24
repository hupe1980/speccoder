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
        // A repository's own configuration never runs code through
        // speccoder's git: no fsmonitor, no hooks.
        cmd.args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(&self.dir)
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

/// Splits NUL-terminated git output into UTF-8 (lossy) fields.
#[must_use]
pub fn nul_fields(out: &[u8]) -> Vec<String> {
    out.split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect()
}
