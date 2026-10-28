//! The person's own commands, run for them: a fix speccoder printed
//! (`npm install`, `git add … && git commit …`), started from an interface
//! that confirmed it. It runs as the person, in the project folder, with
//! their environment and outside the sandbox — what they would type —
//! time-limited and cancellable, its output masked and streamed line by
//! line. Which argvs may run is the interface's to prove (the web host
//! runs only the ones it issued); this module never decides that.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use speccoder_core::{CommandResult, CommandStatus};

/// How long one command may run.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// Which stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output.
    Out,
    /// Standard error.
    Err,
}

impl Stream {
    /// `stdout` or `stderr`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Out => "stdout",
            Self::Err => "stderr",
        }
    }
}

/// The argvs as a person would type them.
#[must_use]
pub fn shown(argvs: &[Vec<String>]) -> String {
    argvs
        .iter()
        .map(|a| {
            a.iter()
                .map(|w| {
                    if w.is_empty() || w.contains(|c: char| c.is_whitespace() || c == '"') {
                        format!("\"{}\"", w.replace('"', "\\\""))
                    } else {
                        w.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Runs `argvs` in `root`, one after the other while each succeeds, each
/// line of output handed to `line` (masked). `speccoder` is this binary.
/// Ends early when `cancel` turns true or after `timeout`.
#[must_use]
pub fn run(
    root: &Path,
    argvs: &[Vec<String>],
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
    line: &dyn Fn(Stream, &str),
) -> CommandResult {
    let what = shown(argvs);
    let failed = |message: String| {
        CommandResult::new("fix", CommandStatus::Error, &crate::secrets::mask(&message))
            .with_code("FIX-FAILED")
    };
    let until = Instant::now() + timeout;
    for argv in argvs {
        let Some((program, args)) = argv.split_first() else {
            return failed("an empty command".to_owned());
        };
        let program = if program == "speccoder" {
            std::env::current_exe().map_or_else(|_| program.into(), std::path::PathBuf::into)
        } else {
            std::ffi::OsString::from(program)
        };
        let mut child = match Command::new(&program)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return failed(format!(
                    "`{}` could not start: {e}",
                    shown(std::slice::from_ref(argv))
                ));
            }
        };
        let (tx, rx) = mpsc::channel::<(Stream, String)>();
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || {
                for l in BufReader::new(out).lines().map_while(Result::ok) {
                    let _ = tx.send((Stream::Out, l));
                }
            }));
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            readers.push(std::thread::spawn(move || {
                for l in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = tx.send((Stream::Err, l));
                }
            }));
        }
        drop(tx);
        // Lines until both streams close, then the exit; a stop or the
        // time limit ends it at any point.
        let mut open = true;
        let status = loop {
            if open {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((s, l)) => line(s, &crate::secrets::mask(&l)),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => open = false,
                }
            } else {
                match child.try_wait() {
                    Ok(Some(s)) => break Ok(s),
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(e) => break Err(e.to_string()),
                }
            }
            let why = if cancel.load(Ordering::SeqCst) {
                "was stopped"
            } else if Instant::now() >= until {
                "ran out of time"
            } else {
                continue;
            };
            let _ = child.kill();
            let _ = child.wait();
            break Err(why.to_owned());
        };
        for r in readers {
            let _ = r.join();
        }
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                let code = s
                    .code()
                    .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
                return failed(format!(
                    "`{}` exited with {code}",
                    shown(std::slice::from_ref(argv))
                ));
            }
            Err(why) => return failed(format!("`{what}` {why}")),
        }
    }
    CommandResult::ok("fix", &crate::secrets::mask(&format!("`{what}` finished")))
}

/// Opens a terminal of the person's own at `root` (macOS: Terminal;
/// elsewhere `$TERMINAL`, else `x-terminal-emulator`).
///
/// # Errors
///
/// No terminal could be started.
pub fn open_terminal(root: &Path) -> Result<(), String> {
    let started = if cfg!(target_os = "macos") {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(root)
            .status()
            .is_ok_and(|s| s.success())
    } else {
        let program = std::env::var("TERMINAL")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "x-terminal-emulator".to_owned());
        Command::new(program)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    };
    if started {
        Ok(())
    } else {
        Err("no terminal could be opened here".to_owned())
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn it_runs_in_the_folder_streams_and_stops_at_the_first_failure() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let lines = Mutex::new(Vec::new());
        let r = run(
            dir.path(),
            &[
                s(&["sh", "-c", "echo here; pwd; echo oops >&2"]),
                s(&["false"]),
                s(&["touch", "never"]),
            ],
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|stream, l| {
                lines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((stream, l.to_owned()))
            },
        );
        assert_eq!(r.status(), CommandStatus::Error);
        assert_eq!(r.code().map(|c| c.name), Some("FIX-FAILED"));
        assert_eq!(r.message(), "`false` exited with 1");
        assert!(!dir.path().join("never").exists());
        let lines = lines.into_inner().unwrap_or_default();
        assert!(
            lines.contains(&(Stream::Out, "here".to_owned())),
            "{lines:?}"
        );
        assert!(
            lines.contains(&(Stream::Err, "oops".to_owned())),
            "{lines:?}"
        );
        let canonical = std::fs::canonicalize(dir.path()).unwrap_or_default();
        assert!(
            lines
                .iter()
                .any(|(_, l)| Path::new(l) == canonical || Path::new(l) == dir.path()),
            "{lines:?}"
        );
        let ok = run(
            dir.path(),
            &[s(&["true"])],
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert_eq!(
            (ok.status(), ok.message()),
            (CommandStatus::Ok, "`true` finished")
        );
    }

    #[test]
    fn it_can_be_stopped_and_runs_out_of_time() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let r = run(
            dir.path(),
            &[s(&["sleep", "30"])],
            TIMEOUT,
            &cancel,
            &|_, _| {},
        );
        assert_eq!(r.message(), "`sleep 30` was stopped");
        assert!(started.elapsed() < Duration::from_secs(10));
        let r = run(
            dir.path(),
            &[s(&["sleep", "30"])],
            Duration::from_millis(300),
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert_eq!(r.message(), "`sleep 30` ran out of time");
    }

    #[test]
    fn its_output_is_masked() {
        crate::secrets::register("PERSON_TEST_TOKEN", "person-run-Qx7 value");
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let lines = Mutex::new(Vec::new());
        let _ = run(
            dir.path(),
            &[s(&["echo", "token person-run-Qx7 value"])],
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, l| {
                lines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(l.to_owned())
            },
        );
        let lines = lines.into_inner().unwrap_or_default();
        assert_eq!(lines, ["token [redacted:PERSON_TEST_TOKEN]"]);
    }

    #[test]
    fn what_it_shows_is_what_a_person_types() {
        assert_eq!(
            shown(&[
                s(&["git", "add", "speccoder.toml"]),
                s(&["git", "commit", "-m", "speccoder: config"])
            ]),
            "git add speccoder.toml && git commit -m \"speccoder: config\""
        );
    }
}
