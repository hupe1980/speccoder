//! Running a check: argv only, no stdin, scrubbed environment, a deadline for
//! the whole process group, capped output.
//!
//! [`CommandRunner`] is the seam the sandbox wraps and tests replace.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

/// Default cap per output stream.
pub const OUTPUT_CAP: usize = 1024 * 1024;

/// What to run.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// Program and arguments.
    pub argv: Vec<String>,
    /// Absolute working directory.
    pub cwd: PathBuf,
    /// The complete child environment.
    pub env: Vec<(String, String)>,
    /// Deadline for the process group.
    pub timeout: Duration,
    /// Bytes kept per stream.
    pub output_cap: usize,
}

/// Captured output of one stream.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    /// The first `output_cap` bytes.
    pub kept: Vec<u8>,
    /// Total bytes written.
    pub total: usize,
}

impl Captured {
    /// Whether output was cut off.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.total > self.kept.len()
    }
}

/// What happened.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Exit code, if the process exited normally.
    pub exit_code: Option<i32>,
    /// Killed at the deadline.
    pub timed_out: bool,
    /// The process could not be started.
    pub spawn_error: Option<String>,
    /// When it was started (for the stale-report rule).
    pub started: SystemTime,
    /// Wall-clock duration.
    pub duration: Duration,
    /// Standard output.
    pub stdout: Captured,
    /// Standard error.
    pub stderr: Captured,
}

/// Runs invocations. The verifier depends only on this trait.
pub trait CommandRunner: Send + Sync {
    /// Runs `inv` to completion or deadline.
    fn run(&self, inv: &Invocation) -> Outcome;
}

/// Runs processes on this machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalRunner;

impl CommandRunner for LocalRunner {
    fn run(&self, inv: &Invocation) -> Outcome {
        let started = SystemTime::now();
        let clock = Instant::now();
        let failed = |e: String| Outcome {
            exit_code: None,
            timed_out: false,
            spawn_error: Some(e),
            started,
            duration: clock.elapsed(),
            stdout: Captured::default(),
            stderr: Captured::default(),
        };
        let Some((program, args)) = inv.argv.split_first() else {
            return failed("empty argv".to_owned());
        };

        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&inv.cwd)
            .env_clear()
            .envs(inv.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return failed(format!("could not start `{program}`: {e}")),
        };
        let pgid = i32::try_from(child.id()).ok().map(Pid::from_raw);
        let cap = inv.output_cap;
        let out = child
            .stdout
            .take()
            .map(|s| thread::spawn(move || capture(s, cap)));
        let err = child
            .stderr
            .take()
            .map(|s| thread::spawn(move || capture(s, cap)));

        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if clock.elapsed() >= inv.timeout => {
                    timed_out = true;
                    if let Some(pgid) = pgid {
                        let _ = killpg(pgid, Signal::SIGKILL);
                    }
                    break child.wait().ok();
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break None,
            }
        };
        // Stragglers that outlived the leader (daemons started by tests) go too.
        if let Some(pgid) = pgid {
            let _ = killpg(pgid, Signal::SIGKILL);
        }
        let join = |h: Option<thread::JoinHandle<Captured>>| {
            h.and_then(|h| h.join().ok()).unwrap_or_default()
        };
        Outcome {
            exit_code: if timed_out {
                None
            } else {
                status.and_then(|s| s.code())
            },
            timed_out,
            spawn_error: None,
            started,
            duration: clock.elapsed(),
            stdout: join(out),
            stderr: join(err),
        }
    }
}

fn capture(mut stream: impl Read, cap: usize) -> Captured {
    let mut captured = Captured::default();
    let mut buf = [0u8; 8192];
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        captured.total += n;
        let room = cap.saturating_sub(captured.kept.len());
        captured.kept.extend_from_slice(&buf[..n.min(room)]);
    }
    captured
}

/// The child environment: everything except secret-looking variables, plus
/// `CI=1` and `GIT_TERMINAL_PROMPT=0`. Not a security boundary — that is
/// the OS sandbox (`crate::sandbox`).
#[must_use]
pub fn scrubbed_env(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vars
        .into_iter()
        .filter(|(k, _)| !is_secret(k))
        .filter(|(k, _)| !OURS.contains(&k.as_str()) && !FORCES_COLOUR.contains(&k.as_str()))
        .collect();
    env.push(("CI".to_owned(), "1".to_owned()));
    env.push(("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()));
    // Output is parsed, not shown: colour would hide a compile error's code
    // from the summary and turn a red test into an unexplained failure.
    env.push(("CARGO_TERM_COLOR".to_owned(), "never".to_owned()));
    env.push(("NO_COLOR".to_owned(), "1".to_owned()));
    env.sort();
    env
}

/// Set by [`scrubbed_env`] itself.
const OURS: &[&str] = &["CI", "GIT_TERMINAL_PROMPT", "CARGO_TERM_COLOR", "NO_COLOR"];

/// Variables that force colour on whatever the output goes to.
const FORCES_COLOUR: &[&str] = &["FORCE_COLOR", "CLICOLOR_FORCE", "PY_COLORS"];

/// Whether a variable name looks like a credential.
#[must_use]
pub fn is_secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.ends_with("_API_KEY")
        || n.ends_with("_TOKEN")
        || n.contains("SECRET")
        || n.contains("PASSWORD")
        || matches!(n.as_str(), "SSH_AUTH_SOCK" | "GIT_ASKPASS" | "SSH_ASKPASS")
        || n.starts_with("AWS_")
        || n.starts_with("ANTHROPIC_")
        || n.starts_with("OPENAI_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_secret_patterns_and_keeps_the_rest() {
        let vars = [
            ("PATH", "/bin"),
            ("HOME", "/h"),
            ("ANTHROPIC_API_KEY", "k"),
            ("GITHUB_TOKEN", "t"),
            ("NPM_TOKEN", "t"),
            ("DB_PASSWORD", "p"),
            ("MY_SECRET_THING", "s"),
            ("AWS_PROFILE", "x"),
            ("SSH_AUTH_SOCK", "/s"),
            ("CI", "false"),
            ("RUSTUP_HOME", "/r"),
            ("CARGO_TERM_COLOR", "always"),
            ("FORCE_COLOR", "1"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        let keys: Vec<_> = scrubbed_env(vars)
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        assert_eq!(
            keys,
            [
                "CARGO_TERM_COLOR=never",
                "CI=1",
                "GIT_TERMINAL_PROMPT=0",
                "HOME=/h",
                "NO_COLOR=1",
                "PATH=/bin",
                "RUSTUP_HOME=/r"
            ]
        );
    }
}
