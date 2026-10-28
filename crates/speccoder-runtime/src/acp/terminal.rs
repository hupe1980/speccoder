//! `terminal/*`: the commands an agent runs through speccoder, started by
//! the attempt's runner — the OS sandbox, the scrubbed environment — after
//! the policy allowed them, their output masked before the agent reads it.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

/// The most of a terminal's output kept (the agent may ask for less).
pub const KEPT: usize = 1024 * 1024;

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exit {
    /// Its exit code, if it exited.
    pub code: Option<u32>,
    /// The signal that ended it, if one did.
    pub signal: Option<String>,
}

#[derive(Debug, Default)]
struct Shared {
    output: Vec<u8>,
    total: usize,
    exit: Option<Exit>,
}

#[derive(Debug)]
struct Term {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    pgid: Option<Pid>,
    limit: Option<usize>,
}

/// The terminals of one attempt.
#[derive(Debug, Default)]
pub struct Terminals {
    next: Mutex<u64>,
    all: Mutex<HashMap<String, Term>>,
}

fn kill(pgid: Option<Pid>) {
    if let Some(p) = pgid {
        let _ = killpg(p, Signal::SIGKILL);
    }
}

impl Terminals {
    /// Starts `argv` (already decided) in `cwd` with `env` through `runner`,
    /// killed after `deadline`.
    ///
    /// # Errors
    ///
    /// It could not be started.
    pub fn create(
        &self,
        runner: &crate::sandbox::Runner,
        argv: &[String],
        cwd: PathBuf,
        mut env: Vec<(String, String)>,
        limit: Option<usize>,
        deadline: Duration,
    ) -> Result<String, String> {
        let (wrapped, tmp) = runner.wrap(argv)?;
        if let Some(tmp) = tmp {
            env.retain(|(k, _)| !matches!(k.as_str(), "TMPDIR" | "TMP" | "TEMP"));
            let t = tmp.to_string_lossy().into_owned();
            env.extend(["TMPDIR", "TMP", "TEMP"].map(|k| (k.to_owned(), t.clone())));
        }
        let Some((program, args)) = wrapped.split_first() else {
            return Err("empty command".to_owned());
        };
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&cwd)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start `{}`: {e}", argv.join(" ")))?;
        let pgid = i32::try_from(child.id()).ok().map(Pid::from_raw);
        let shared: Arc<(Mutex<Shared>, Condvar)> = Arc::default();
        let readers: Vec<std::thread::JoinHandle<()>> = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|mut stream| {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut s) = shared.0.lock() {
                        s.total += n;
                        s.output.extend_from_slice(&buf[..n]);
                        // The end is kept: what a command printed last.
                        let keep = KEPT + crate::secrets::longest();
                        if s.output.len() > keep {
                            let cut = s.output.len() - keep;
                            s.output.drain(..cut);
                        }
                    }
                }
            })
        })
        .collect();
        let waiter = Arc::clone(&shared);
        std::thread::spawn(move || {
            let began = Instant::now();
            let status = loop {
                match child.try_wait() {
                    Ok(Some(s)) => break Some(s),
                    Ok(None) if began.elapsed() >= deadline => {
                        kill(pgid);
                        break child.wait().ok();
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break None,
                }
            };
            // Stragglers that outlived the leader go too.
            kill(pgid);
            for r in readers {
                let _ = r.join();
            }
            let exit = {
                #[cfg(unix)]
                let signal = status
                    .and_then(|s| std::os::unix::process::ExitStatusExt::signal(&s))
                    .map(|n| {
                        Signal::try_from(n)
                            .map_or_else(|_| n.to_string(), |s| s.as_str().to_owned())
                    });
                #[cfg(not(unix))]
                let signal: Option<String> = None;
                Exit {
                    code: status
                        .and_then(|s| s.code())
                        .and_then(|c| u32::try_from(c).ok()),
                    signal,
                }
            };
            if let Ok(mut s) = waiter.0.lock() {
                s.exit = Some(exit);
            }
            waiter.1.notify_all();
        });
        let id = {
            let mut n = self.next.lock().map_err(|e| e.to_string())?;
            *n += 1;
            format!("term-{n}")
        };
        self.all.lock().map_err(|e| e.to_string())?.insert(
            id.clone(),
            Term {
                shared,
                pgid,
                limit,
            },
        );
        Ok(id)
    }

    fn with<T>(&self, id: &str, f: impl FnOnce(&Term) -> T) -> Result<T, String> {
        let all = self.all.lock().map_err(|e| e.to_string())?;
        all.get(id)
            .map(f)
            .ok_or_else(|| format!("no terminal `{id}`"))
    }

    /// What it printed so far (masked, the end kept within its limit),
    /// whether that was cut, and how it ended if it has.
    ///
    /// # Errors
    ///
    /// No such terminal.
    pub fn output(&self, id: &str) -> Result<(String, bool, Option<Exit>), String> {
        self.with(id, |t| {
            let Ok(s) = t.shared.0.lock() else {
                return (String::new(), false, None);
            };
            let masked = crate::secrets::mask(&String::from_utf8_lossy(&s.output));
            let limit = t.limit.unwrap_or(KEPT).min(KEPT);
            let mut cut = masked.len().saturating_sub(limit);
            while !masked.is_char_boundary(cut) {
                cut += 1;
            }
            let truncated = cut > 0 || s.total > s.output.len();
            (masked[cut..].to_owned(), truncated, s.exit.clone())
        })
    }

    /// Waits for it to end.
    ///
    /// # Errors
    ///
    /// No such terminal.
    pub fn wait(&self, id: &str) -> Result<Exit, String> {
        let shared = self.with(id, |t| Arc::clone(&t.shared))?;
        let (lock, cv) = &*shared;
        let mut s = lock.lock().map_err(|e| e.to_string())?;
        loop {
            if let Some(e) = &s.exit {
                return Ok(e.clone());
            }
            s = cv.wait(s).map_err(|e| e.to_string())?;
        }
    }

    /// Kills it; it stays readable.
    ///
    /// # Errors
    ///
    /// No such terminal.
    pub fn kill(&self, id: &str) -> Result<(), String> {
        self.with(id, |t| kill(t.pgid))
    }

    /// Kills it and forgets it.
    ///
    /// # Errors
    ///
    /// No such terminal.
    pub fn release(&self, id: &str) -> Result<(), String> {
        let t = self
            .all
            .lock()
            .map_err(|e| e.to_string())?
            .remove(id)
            .ok_or_else(|| format!("no terminal `{id}`"))?;
        kill(t.pgid);
        Ok(())
    }

    /// Kills every terminal still running (the attempt ended).
    pub fn kill_all(&self) {
        if let Ok(all) = self.all.lock() {
            for t in all.values() {
                if t.shared.0.lock().is_ok_and(|s| s.exit.is_none()) {
                    kill(t.pgid);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_runs_and_its_output_is_kept_from_the_end() {
        let t = Terminals::default();
        let dir = std::env::temp_dir();
        let env: Vec<(String, String)> = std::env::vars().collect();
        let argv = ["sh", "-c", "printf 'hello world'; exit 3"].map(str::to_owned);
        let id = t
            .create(
                &crate::sandbox::Runner::Plain,
                &argv,
                dir.clone(),
                env.clone(),
                Some(5),
                Duration::from_secs(10),
            )
            .unwrap_or_default();
        let exit = t.wait(&id).unwrap_or(Exit {
            code: None,
            signal: None,
        });
        assert_eq!(exit.code, Some(3));
        let (out, truncated, ended) = t.output(&id).unwrap_or_default();
        assert_eq!(
            (out.as_str(), truncated, ended.is_some()),
            ("world", true, true)
        );
        // Killed at its deadline.
        let argv = ["sleep", "5"].map(str::to_owned);
        let id = t
            .create(
                &crate::sandbox::Runner::Plain,
                &argv,
                dir,
                env,
                None,
                Duration::from_millis(100),
            )
            .unwrap_or_default();
        let exit = t.wait(&id).unwrap_or(Exit {
            code: Some(0),
            signal: None,
        });
        assert_eq!(exit.code, None);
        assert!(t.release(&id).is_ok());
        assert!(t.output(&id).is_err());
    }
}
