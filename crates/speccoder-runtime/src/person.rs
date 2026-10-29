//! Commands run for a person from an interface that confirmed them.
//!
//! - [`run`]: the person's own command, a fix speccoder printed (`npm
//!   install`, `git add … && git commit …`). It runs in the project folder,
//!   in the OS sandbox with the network on and without the variables
//!   speccoder treats as secret, for ten minutes at most.
//! - [`speccoder`]: one of speccoder's own commands that has no action of
//!   its own (`speccoder run --ignore-checklists`): this binary, with no
//!   time limit, its result read back as the job's.
//!
//! Each runs in a process group of its own: a stop ends it and everything
//! it started, and the job always ends. Output is masked and streamed line
//! by line. Which argvs may run is the interface's to prove (the web host
//! runs only the ones it issued); this module never decides that.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use speccoder_core::{CommandResult, CommandStatus};

/// How long the person's own command may run.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// How long a stopped group gets to end before it is killed.
const GRACE: Duration = Duration::from_secs(5);

/// How long output may still arrive once the command itself exited (a
/// background process it left holds the pipes open).
const DRAIN: Duration = Duration::from_millis(500);

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

/// The argvs as a shell reads them.
#[must_use]
pub fn shown(argvs: &[Vec<String>]) -> String {
    speccoder_core::step::render(argvs)
}

/// How a command ended.
enum End {
    Exited(ExitStatus),
    Stopped,
    OutOfTime,
    Broken(String),
}

/// Signals the child's process group.
fn signal(child: &Child, sig: nix::sys::signal::Signal) {
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), sig);
    }
}

/// Ends the group: `first` (a request to stop), then a kill after the grace.
fn end_group(child: &mut Child, first: nix::sys::signal::Signal) {
    signal(child, first);
    let until = Instant::now() + GRACE;
    while Instant::now() < until {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    signal(child, nix::sys::signal::Signal::SIGKILL);
    let _ = child.wait();
}

/// Starts `cmd` in a group of its own, its output piped.
fn spawn(mut cmd: Command, root: &Path) -> std::io::Result<Child> {
    cmd.current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn()
}

/// Follows `child` to its end: lines to `line`, a stop or `deadline` end
/// the group (`stop` the signal asked first). Readers are never waited
/// for past the end: what a background process holds open is let go.
fn follow(
    child: &mut Child,
    cancel: &Arc<AtomicBool>,
    deadline: Option<Instant>,
    stop: nix::sys::signal::Signal,
    line: &mut dyn FnMut(Stream, &str),
) -> End {
    let (tx, rx) = mpsc::channel::<(Stream, String)>();
    for (stream, pipe) in [
        (
            Stream::Out,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        ),
        (
            Stream::Err,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        ),
    ] {
        if let Some(pipe) = pipe {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send((stream, l)).is_err() {
                        break;
                    }
                }
            });
        }
    }
    drop(tx);
    let mut exited: Option<(ExitStatus, Instant)> = None;
    let mut open = true;
    loop {
        if open {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok((s, l)) => line(s, &crate::secrets::mask(&l)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => open = false,
            }
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
        // Every pipe closed and it exited: that is the end.
        if !open && let Some((s, _)) = exited {
            return End::Exited(s);
        }
        if exited.is_none() {
            match child.try_wait() {
                Ok(Some(s)) => exited = Some((s, Instant::now())),
                Ok(None) => {}
                Err(e) => {
                    end_group(child, nix::sys::signal::Signal::SIGKILL);
                    return End::Broken(e.to_string());
                }
            }
        }
        if let Some((s, at)) = exited
            && at.elapsed() >= DRAIN
        {
            // It exited; what it left running in its group goes with it.
            signal(child, nix::sys::signal::Signal::SIGKILL);
            return End::Exited(s);
        }
        if cancel.load(Ordering::SeqCst) {
            end_group(child, stop);
            return End::Stopped;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            end_group(child, nix::sys::signal::Signal::SIGTERM);
            return End::OutOfTime;
        }
    }
}

/// Environment variables, in order.
type Env = Vec<(String, String)>;

/// The program `name` names: the first executable of that name in an
/// absolute `PATH` entry outside the project — never one the repository
/// could bring (`.`, `node_modules/.bin`, a folder of the project).
fn program(
    name: &str,
    root: &Path,
    env: &[(String, String)],
) -> Result<std::path::PathBuf, String> {
    if name.contains('/') {
        return Err(format!("`{name}` is a path, not a program on PATH"));
    }
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map_or("/usr/bin:/bin", |(_, v)| v.as_str());
    path.split(':')
        .map(Path::new)
        .filter(|d| d.is_absolute())
        .filter_map(|d| std::fs::canonicalize(d).ok())
        .filter(|d| !d.starts_with(&root))
        .find_map(|d| {
            crate::sandbox::resolve_executable(
                name,
                &d,
                &[("PATH".to_owned(), d.display().to_string())],
            )
        })
        .filter(|p| !p.starts_with(&root))
        .ok_or_else(|| format!("`{name}` is not on PATH outside the project"))
}

/// How one argv starts: confined by speccoder's OS sandbox — writes to the
/// project (its `.git` among it), the package caches and a private
/// temporary folder, credentials unreadable, the network on (a fix is an
/// install, most often) — unless it is one of the few fixes that write
/// outside the project by their nature, or there is no sandbox here. The
/// person's environment, without the variables speccoder treats as secret.
fn confined(
    argv: &[String],
    root: &Path,
    env: &[(String, String)],
) -> Result<(Vec<String>, Env), String> {
    let (name, args) = argv.split_first().ok_or("an empty command")?;
    let mut full = vec![program(name, root, env)?.display().to_string()];
    full.extend(args.iter().cloned());
    let mut env: Vec<(String, String)> = env
        .iter()
        .filter(|(k, _)| !crate::verify::process::is_secret(k))
        .cloned()
        .collect();
    if speccoder_core::step::outside(argv).is_some() {
        return Ok((full, env));
    }
    let Ok(backend) = crate::sandbox::backend() else {
        return Ok((full, env));
    };
    let home = env
        .iter()
        .find(|(k, _)| k == "HOME")
        .map(|(_, v)| std::path::PathBuf::from(v));
    let profile =
        crate::sandbox::Profile::new(root, &crate::sandbox::Config::default(), home.as_deref())
            .with_network(crate::sandbox::Network::On);
    std::fs::create_dir_all(&profile.tmp)
        .map_err(|e| format!("the sandbox's temporary folder: {e}"))?;
    let tmp = profile.tmp.display().to_string();
    env.retain(|(k, _)| !matches!(k.as_str(), "TMPDIR" | "TMP" | "TEMP"));
    env.extend(["TMPDIR", "TMP", "TEMP"].map(|k| (k.to_owned(), tmp.clone())));
    Ok((crate::sandbox::wrap(&profile, &backend, &full)?, env))
}

/// Whether the person's commands run in an OS sandbox here.
#[must_use]
pub fn sandboxed() -> bool {
    crate::sandbox::backend().is_ok()
}

/// Runs the person's own `argvs` in `root`, one after the other while each
/// succeeds, each line of output handed to `line` (masked). Ends early, with
/// everything it started, when `cancel` turns true or after `timeout`.
#[must_use]
pub fn run(
    root: &Path,
    argvs: &[Vec<String>],
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
    line: &dyn Fn(Stream, &str),
) -> CommandResult {
    let env: Vec<(String, String)> = std::env::vars().collect();
    run_in(root, argvs, &env, timeout, cancel, line)
}

fn run_in(
    root: &Path,
    argvs: &[Vec<String>],
    env: &[(String, String)],
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
    line: &dyn Fn(Stream, &str),
) -> CommandResult {
    let what = shown(argvs);
    let failed = |message: String| {
        CommandResult::new("fix", CommandStatus::Error, &crate::secrets::mask(&message))
            .with_code("FIX-FAILED")
    };
    let deadline = Instant::now() + timeout;
    for argv in argvs {
        let one = shown(std::slice::from_ref(argv));
        let (start, vars) = match confined(argv, root, env) {
            Ok(x) => x,
            Err(e) => return failed(format!("`{one}` could not start: {e}")),
        };
        let Some((program, args)) = start.split_first() else {
            return failed("an empty command".to_owned());
        };
        let mut cmd = Command::new(program);
        cmd.args(args).env_clear().envs(vars);
        let mut child = match spawn(cmd, root) {
            Ok(c) => c,
            Err(e) => return failed(format!("`{one}` could not start: {e}")),
        };
        match follow(
            &mut child,
            cancel,
            Some(deadline),
            nix::sys::signal::Signal::SIGTERM,
            &mut |s, l| line(s, l),
        ) {
            End::Exited(s) if s.success() => {}
            End::Exited(s) => {
                let code = s
                    .code()
                    .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
                return failed(format!("`{one}` exited with {code}"));
            }
            End::Stopped => return failed(format!("`{what}` was stopped")),
            End::OutOfTime => return failed(format!("`{what}` ran out of time")),
            End::Broken(e) => return failed(format!("`{what}`: {e}")),
        }
    }
    CommandResult::ok("fix", &crate::secrets::mask(&format!("`{what}` finished")))
}

/// Runs speccoder itself with `args` in `root`, as a job: no time limit,
/// a stop asks it to stop (as ctrl-c would) before it is killed. Its
/// standard error goes to `line`; its result, read from `--json`, is the
/// job's. `Err`: it gave none (it could not start, or was stopped first).
///
/// # Errors
///
/// No result came back.
pub fn speccoder(
    root: &Path,
    args: &[String],
    cancel: &Arc<AtomicBool>,
    line: &dyn Fn(Stream, &str),
) -> Result<serde_json::Value, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("speccoder could not find itself: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(args).arg("--json");
    let mut child = spawn(cmd, root).map_err(|e| format!("speccoder could not start: {e}"))?;
    let mut out = String::new();
    let end = follow(
        &mut child,
        cancel,
        None,
        nix::sys::signal::Signal::SIGINT,
        &mut |s, l| match s {
            Stream::Out => {
                out.push_str(l);
                out.push('\n');
            }
            Stream::Err => line(s, l),
        },
    );
    let result = serde_json::from_str::<serde_json::Value>(out.trim()).ok();
    match (end, result) {
        (_, Some(v)) if v.is_object() => Ok(v),
        (End::Stopped, _) => Err("it was stopped".to_owned()),
        (End::Broken(e), _) => Err(e),
        _ => Err("it gave no result".to_owned()),
    }
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
            "git add speccoder.toml && git commit -m 'speccoder: config'"
        );
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|s| s.success())
    }

    #[test]
    fn a_stop_ends_everything_it_started_and_the_job_ends() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            flag.store(true, Ordering::SeqCst);
        });
        let pids = Mutex::new(Vec::new());
        let started = Instant::now();
        let r = run(
            dir.path(),
            &[s(&["sh", "-c", "sleep 30 & echo $!; sleep 30"])],
            TIMEOUT,
            &cancel,
            &|_, l| {
                pids.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(l.to_owned())
            },
        );
        assert!(r.message().ends_with("was stopped"), "{}", r.message());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        let pids = pids.into_inner().unwrap_or_default();
        let background = pids.first().cloned().unwrap_or_default();
        assert!(!background.is_empty());
        std::thread::sleep(Duration::from_millis(200));
        assert!(!alive(&background), "{background} survived the stop");
    }

    #[test]
    fn it_runs_confined_to_the_project_where_there_is_a_sandbox() {
        if !sandboxed() {
            return;
        }
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let escaped = outside.path().join("escaped");
        let r = run(
            dir.path(),
            &[s(&[
                "sh",
                "-c",
                &format!("touch inside && touch '{}'", escaped.display()),
            ])],
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert_eq!(r.status(), CommandStatus::Error, "{}", r.message());
        assert!(dir.path().join("inside").exists());
        assert!(!escaped.exists(), "a write outside the project got through");
    }

    #[test]
    fn its_program_is_never_one_the_project_brings() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let bin = dir.path().join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap_or_else(|e| panic!("{e}"));
        let fake = bin.join("true");
        std::fs::write(&fake, "#!/bin/sh\ntouch planted\n").unwrap_or_else(|e| panic!("{e}"));
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::copy(&fake, dir.path().join("true")).unwrap_or_else(|e| panic!("{e}"));
        let env = vec![(
            "PATH".to_owned(),
            format!(".:node_modules/.bin:{}:/usr/bin:/bin", bin.display()),
        )];
        let r = run_in(
            dir.path(),
            &[s(&["true"])],
            &env,
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
        assert!(!dir.path().join("planted").exists());
        // Only the project's own: refused.
        let only = vec![("PATH".to_owned(), format!(".:{}", bin.display()))];
        let r = run_in(
            dir.path(),
            &[s(&["true"])],
            &only,
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert!(
            r.message().contains("not on PATH outside the project"),
            "{}",
            r.message()
        );
        assert!(!dir.path().join("planted").exists());
    }

    #[test]
    fn what_it_leaves_running_does_not_hold_the_job() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let started = Instant::now();
        let r = run(
            dir.path(),
            &[s(&["sh", "-c", "sleep 30 & echo started"])],
            TIMEOUT,
            &Arc::new(AtomicBool::new(false)),
            &|_, _| {},
        );
        assert_eq!(r.status(), CommandStatus::Ok, "{}", r.message());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }
}
