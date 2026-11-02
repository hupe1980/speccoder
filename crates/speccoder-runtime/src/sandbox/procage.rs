//! speccoder's sandbox, confined by [procage](https://github.com/hupe1980/procage).
//!
//! A [`Profile`] becomes a procage policy ([`policy`]). Commands that run to
//! completion are spawned in this process ([`run`]); long-running servers
//! start through the helper (`speccoder __sandbox <profile> -- argv…`), which
//! confines itself and becomes the server ([`exec`]), so whatever kills the
//! helper kills the server.
//!
//! Where procage cannot enforce part of a profile on this host (Linux
//! without user namespaces, or with a kernel older than 6.12), [`fit`]
//! leaves exactly that part out and says so.
//!
//! [`run`] reports what the cage denies; after a failed command the record
//! becomes the outcome's denials. The helper's exec reports nothing (it
//! becomes the server, which has no record).

use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime};

use procage::native::{Cage, CageOptions, Grants};
use procage::policy::{
    AbsPath, Access, Base, EnvPolicy, Exec, FsRule, FsTarget, HardDenyClass, Lift, LowerOptions,
    Network as Net, Policy, Ports, Relax, Relaxation, Relaxed, Signals,
};

use super::{Network, Profile};
use crate::verify::process::{Captured, Invocation, Outcome, supervise};

fn abs(p: &Path) -> Result<AbsPath, String> {
    AbsPath::new(p.as_os_str().as_bytes()).map_err(|e| format!("{}: {e}", p.display()))
}

/// `profile` as a procage policy.
///
/// # Errors
///
/// A path procage cannot take (not absolute, not normalised).
pub fn policy(profile: &Profile) -> Result<Policy, String> {
    let mut p = Policy::default();
    // Version 3 runs developer tools (Xcode's shims, Homebrew), keeps them
    // on the private `TMPDIR`, and keeps the probes every program makes
    // (logging, DTrace, the terminal) out of the record of denials; it
    // allows nothing version 2 does not.
    p.base = Base::V3;
    p.process.exec = Exec::Any;
    p.process.fork = true;
    p.process.signals = if profile.unenforced.signal_scope {
        Signals::SameUser
    } else {
        Signals::SelfAndChildren
    };
    // Exactly the variables set on the command (speccoder scrubs them).
    p.env = EnvPolicy::Allowlist(Vec::new());
    // Reads as wide as before: all of the file system except hidden trees.
    p.fs.push(FsRule::new(
        Access::Read,
        FsTarget::Subpath(abs(Path::new("/"))?),
    ));
    // A rule on a path that does not exist fails the cage on Linux; missing
    // trees (caches not created yet) are skipped.
    let existing = |paths: &[std::path::PathBuf]| -> Result<Vec<AbsPath>, String> {
        paths
            .iter()
            .filter(|p| p.exists())
            .map(|p| abs(p))
            .collect()
    };
    let mut writable = profile.writable.clone();
    writable.push(profile.tmp.clone());
    let writable = existing(&writable)?;
    for w in &writable {
        p.fs.push(FsRule::new(Access::Write, FsTarget::Subpath(w.clone())));
    }
    for r in existing(&profile.read_only)? {
        p.fs.push(FsRule::new(Access::Protect, FsTarget::Subpath(r)));
    }
    for h in existing(&profile.hidden)? {
        p.fs.push(FsRule::new(Access::Hide, FsTarget::Subpath(h)));
    }
    // Test servers and databases in the writable trees talk over Unix
    // sockets; sockets elsewhere (a container engine's) stay out.
    if profile.network != Network::On && !profile.unenforced.workspace_sockets {
        p.ipc.unix_sockets = writable;
    }
    if profile.unenforced.trust_handoff {
        p.waive.insert(HardDenyClass::TrustHandoff);
    }
    p.net = match profile.network {
        Network::Off => Net::None,
        // On Linux loopback is no network: seccomp cannot tell addresses apart.
        Network::Loopback if cfg!(target_os = "linux") => Net::None,
        Network::Loopback => Net::Loopback(Ports::Any),
        Network::On => Net::All,
    };
    Ok(p)
}

/// The cage for `profile`. The approximations procage makes from the file
/// system as it is at spawn (masks on Linux with user namespaces, piecewise
/// hiding without them) are accepted, and see [`options`].
fn cage(profile: &Profile) -> Result<Cage, procage::native::SpawnError> {
    cage_at(profile, None)
}

/// [`cage`] as on a Linux kernel whose Landlock is at most ABI `cap` (tests).
fn cage_at(profile: &Profile, cap: Option<u8>) -> Result<Cage, procage::native::SpawnError> {
    cage_with(profile, &options(profile, cap))
}

/// The cage for `profile` with `options`.
fn cage_with(
    profile: &Profile,
    options: &CageOptions,
) -> Result<Cage, procage::native::SpawnError> {
    let policy = policy(profile).map_err(|e| {
        procage::native::SpawnError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
    })?;
    Cage::with_options(&policy, options)
}

/// The cage options: spawn-snapshot approximations and other processes'
/// arguments under `/proc` (Linux, no PID namespace) accepted, a protected
/// repository's object store out of the link check (a local `git clone`
/// hard-links it to the source's; objects are never rewritten in place), the
/// Landlock ABI capped at `cap` on Linux (tests).
fn options(profile: &Profile, cap: Option<u8>) -> CageOptions {
    let mut options = CageOptions::default().with_lower(
        LowerOptions::default()
            .accept_spawn_snapshot()
            .allow(procage::policy::Construct::ProcOtherProcesses),
    );
    for objects in profile
        .read_only
        .iter()
        .filter(|r| r.ends_with(".git"))
        .map(|r| r.join("objects"))
        .filter(|o| o.is_dir())
    {
        options = options.with_link_exemption(objects);
    }
    #[cfg(target_os = "linux")]
    let options = match cap {
        Some(abi) => options.with_landlock_abi(abi),
        None => options,
    };
    #[cfg(not(target_os = "linux"))]
    let _ = cap;
    options
}

/// The Landlock ABI `cage` enforces (Linux).
#[cfg(target_os = "linux")]
fn landlock_abi(cage: &Cage) -> Option<u8> {
    Some(cage.landlock_abi())
}

#[cfg(not(target_os = "linux"))]
fn landlock_abi(_: &Cage) -> Option<u8> {
    None
}

/// Whether procage can confine commands on this machine at all.
///
/// # Errors
///
/// Why not (an unsupported system, a nested sandbox, no Landlock).
pub fn available() -> Result<(), String> {
    let mut p = Policy::default();
    p.base = Base::V3;
    Cage::new(&p).map(drop).map_err(|e| e.to_string())
}

/// The profile procage enforces here, with a notice for what it leaves out.
///
/// Where the host cannot enforce part of a speccoder profile, procage's
/// [`Cage::fit`] gives up exactly that part, and only these:
///
/// - read-only or hidden trees inside a writable one (in place: `.git` and
///   git-ignored secrets) and the trust-handoff names in writable trees
///   (git hooks, shell startup files): Landlock cannot deny inside a tree
///   it allows, without user namespaces;
/// - Unix sockets in the writable trees: without user namespaces other
///   sockets cannot be kept out, so none is reachable;
/// - signals within the run: Linux older than 6.12 cannot keep them there.
///
/// The fitted profile records each of these, so the helper, which rebuilds
/// the cage from it, confines exactly the same way.
///
/// # Errors
///
/// Any other refusal: procage cannot confine this profile here.
pub fn fit(profile: &Profile) -> Result<(Profile, Option<String>), String> {
    fit_at(profile, None)
}

/// The parts of a profile speccoder gives up where the host cannot enforce
/// them.
fn relax() -> Relax {
    Relax::new()
        .leave_out_protect()
        .leave_out_hide()
        .waive(HardDenyClass::TrustHandoff)
        .no_unix_sockets()
        .signals_same_user()
}

/// [`fit`] as on a Linux kernel whose Landlock is at most ABI `cap` (tests).
fn fit_at(profile: &Profile, cap: Option<u8>) -> Result<(Profile, Option<String>), String> {
    let (cage, relaxed) = Cage::fit(&policy(profile)?, &options(profile, cap), &relax())
        .map_err(|e| e.to_string())?;
    let mut fitted = profile.clone();
    let mut notes = Vec::new();
    for r in &relaxed {
        let what = adopt(&mut fitted, r)?;
        note(&mut notes, cause(&r.lift), what);
    }
    if !relaxed.is_empty() {
        cage_at(&fitted, cap).map_err(|e| format!("the fitted profile does not hold: {e}"))?;
    }
    let abi = landlock_abi(&cage);
    if abi.is_some_and(|a| a < 2) {
        note(
            &mut notes,
            Cause::Kernel(2),
            "files cannot move between directories, so builds fail",
        );
    }
    if abi.is_some_and(|a| a < 3) {
        note(
            &mut notes,
            Cause::Kernel(3),
            "truncating a file by path fails",
        );
    }
    Ok((fitted, notice(&notes, abi)))
}

/// Records `r` in `profile`, so the profile translates to the policy procage
/// fitted; returns what it means for commands.
fn adopt(profile: &mut Profile, r: &Relaxed) -> Result<&'static str, String> {
    Ok(match r.relaxation {
        Relaxation::LeaveOutProtect | Relaxation::LeaveOutHide => {
            let is =
                |p: &std::path::PathBuf| r.gap.subject.as_deref() == Some(&*p.to_string_lossy());
            let before = profile.read_only.len() + profile.hidden.len();
            profile.read_only.retain(|p| !is(p));
            profile.hidden.retain(|p| !is(p));
            if before == profile.read_only.len() + profile.hidden.len() {
                return Err(format!("{r}: not a path of the profile"));
            }
            "commands can write `.git` and read git-ignored secrets in place (the run stops if `.git` changes)"
        }
        Relaxation::Waive(HardDenyClass::TrustHandoff) => {
            profile.unenforced.trust_handoff = true;
            "git hooks and shell startup files in the workspace stay writable"
        }
        Relaxation::NoUnixSockets => {
            profile.unenforced.workspace_sockets = true;
            "Unix sockets in the workspace are unreachable"
        }
        Relaxation::SignalsSameUser => {
            profile.unenforced.signal_scope = true;
            "commands can signal your other processes"
        }
        _ => return Err(format!("{r}: speccoder does not accept this")),
    })
}

/// Why a part of the profile is left out, from what procage says would
/// lift it: each cause has its own fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// No unprivileged user namespaces (an AppArmor profile lifts it).
    UserNamespaces,
    /// The kernel's Landlock is too old; this ABI lifts it.
    Kernel(u8),
    /// Nothing procage knows lifts it.
    Unknown,
}

fn cause(lift: &Lift) -> Cause {
    match (lift.tier1, lift.landlock_abi) {
        (true, _) => Cause::UserNamespaces,
        (false, Some(abi)) => Cause::Kernel(abi),
        (false, None) => Cause::Unknown,
    }
}

/// The first mainline Linux with Landlock ABI `abi`.
fn linux_with(abi: u8) -> Option<&'static str> {
    Some(match abi {
        1 => "5.13",
        2 => "5.19",
        3 => "6.2",
        4 => "6.7",
        5 => "6.10",
        6 => "6.12",
        7 => "6.15",
        8 => "7.0",
        _ => return None,
    })
}

type Note = (Cause, String);

fn note(notes: &mut Vec<Note>, cause: Cause, what: &str) {
    let what = match cause {
        Cause::Kernel(abi) => match linux_with(abi) {
            Some(linux) => format!("{what} (Linux {linux} lifts this)"),
            None => format!("{what} (Landlock ABI {abi} lifts this)"),
        },
        _ => what.to_owned(),
    };
    if !notes.iter().any(|(_, w)| *w == what) {
        notes.push((cause, what));
    }
}

/// One clause per cause, naming the kernel's Landlock ABI where it is known.
fn notice(notes: &[Note], abi: Option<u8>) -> Option<String> {
    let items = |pick: fn(&Cause) -> bool| {
        notes
            .iter()
            .filter(|(c, _)| pick(c))
            .map(|(_, w)| w.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mut clauses = Vec::new();
    let userns = items(|c| *c == Cause::UserNamespaces);
    if !userns.is_empty() {
        clauses.push(format!(
            "this machine does not allow user namespaces (on Ubuntu 23.10+: AppArmor), so {userns}"
        ));
    }
    let kernel = items(|c| matches!(c, Cause::Kernel(_)));
    if !kernel.is_empty() {
        let which = abi.map_or_else(String::new, |a| format!(" (Landlock ABI {a})"));
        clauses.push(format!("this kernel is too old{which}, so {kernel}"));
    }
    let unknown = items(|c| *c == Cause::Unknown);
    if !unknown.is_empty() {
        clauses.push(format!(
            "this machine cannot enforce every part, so {unknown}"
        ));
    }
    (!clauses.is_empty()).then(|| clauses.join("; and "))
}

/// The caged command for `argv` in `cwd` with exactly `env`.
fn command(argv: &[String], cwd: &Path, env: &[(String, String)]) -> Option<Command> {
    let (program, args) = argv.split_first()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .envs(foreground_git(
            env.iter()
                .find(|(k, _)| k == "GIT_CONFIG_COUNT")
                .map(|(_, v)| v.as_str()),
        ));
    Some(cmd)
}

/// `gc.autoDetach=false` after the `count` git settings the environment
/// already passes: git's automatic `gc` detaches with `setsid`, which the
/// cage refuses (a run's processes end with it), and would print "setsid
/// failed".
fn foreground_git(count: Option<&str>) -> [(String, String); 3] {
    let n: usize = count.and_then(|c| c.parse().ok()).unwrap_or(0);
    [
        (format!("GIT_CONFIG_KEY_{n}"), "gc.autoDetach".to_owned()),
        (format!("GIT_CONFIG_VALUE_{n}"), "false".to_owned()),
        ("GIT_CONFIG_COUNT".to_owned(), (n + 1).to_string()),
    ]
}

/// Runs `inv` caged by `profile` to completion or deadline. `inv.env`
/// already holds the profile's `TMPDIR`.
#[must_use]
pub fn run(profile: &Profile, inv: &Invocation) -> Outcome {
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
        sandbox: Default::default(),
    };
    // A cage that cannot be built now is this command's failure, never an
    // unconfined run. Reporting what it denies changes no rule; where it
    // cannot start, the command runs confined all the same.
    let cage = match cage_with(profile, &options(profile, None).with_violations()) {
        Ok(c) => c,
        Err(e) => return failed(format!("the sandbox could not confine the command: {e}")),
    };
    let Some(mut cmd) = command(&inv.argv, &inv.cwd, &inv.env) else {
        return failed("empty argv".to_owned());
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = &inv.argv[0];
    let child = match cage.spawn(&mut cmd, &Grants::new()) {
        Ok(c) => c,
        Err(e) => return failed(format!("could not start `{program}` in the sandbox: {e}")),
    };
    let mut caged = Caged(child);
    let mut outcome = supervise(&mut caged, inv, started, clock);
    // Read only after a failure: `supervise` ended the run, which completed
    // the record, so reading it costs nothing more.
    if outcome.exit_code != Some(0) {
        outcome.sandbox = super::denials::from_record(&caged.0.violations());
    }
    outcome
}

/// A caged child: procage's handle kills every process the command started
/// when the run ends, wherever it went.
struct Caged(procage::native::Child);

impl crate::verify::process::Supervised for Caged {
    fn child(&mut self) -> &mut std::process::Child {
        &mut self.0
    }

    fn kill(&mut self) {
        let _ = self.0.kill_all();
    }
}

/// The helper (`speccoder __sandbox <profile> -- argv…`): confines this
/// process with `profile` and becomes the command, keeping this process's
/// pid, standard streams and environment, leading a session of its own
/// that the command's processes cannot leave (the caller ends it with
/// [`crate::session::end`]); returns only on failure, with the reason.
pub fn exec(profile: &Profile, argv: &[String]) -> String {
    let cage = match cage(profile) {
        Ok(c) => c,
        Err(e) => return format!("the sandbox could not confine the command: {e}"),
    };
    let Some((program, args)) = argv.split_first() else {
        return "nothing to run".to_owned();
    };
    let mut cmd = Command::new(program);
    // The environment speccoder gave the helper, which it already scrubbed.
    cmd.args(args)
        .envs(std::env::vars_os())
        .envs(foreground_git(
            std::env::var("GIT_CONFIG_COUNT").ok().as_deref(),
        ));
    let err = cage.exec_session(&mut cmd, &Grants::new());
    format!("could not start `{program}` in the sandbox: {err}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use procage::policy::{CapabilityGap, GapKind, Target, Tier};

    fn relaxed(relaxation: Relaxation, subject: Option<&str>, lift: Lift) -> Relaxed {
        Relaxed {
            gap: CapabilityGap {
                construct: relaxation.construct(),
                target: Target::Linux {
                    tier: Tier::T2,
                    landlock_abi: 5,
                },
                kind: GapKind::Unsupported,
                subject: subject.map(str::to_owned),
            },
            relaxation,
            lift,
        }
    }

    #[test]
    fn caged_git_configuration_keeps_what_the_environment_set() {
        assert_eq!(
            foreground_git(Some("2")),
            [
                ("GIT_CONFIG_KEY_2".to_owned(), "gc.autoDetach".to_owned()),
                ("GIT_CONFIG_VALUE_2".to_owned(), "false".to_owned()),
                ("GIT_CONFIG_COUNT".to_owned(), "3".to_owned()),
            ]
        );
        assert_eq!(foreground_git(None)[2].1, "1");
    }

    /// Run, the helper and fit share one policy, on the base that keeps
    /// benign probes out of the record and allows nothing more than V2.
    #[test]
    fn the_policy_is_on_base_v3() {
        let p = Profile::new(Path::new("/w"), &super::super::Config::default(), None);
        assert_eq!(policy(&p).unwrap().base, Base::V3);
    }

    const T1: Lift = Lift {
        tier1: true,
        landlock_abi: None,
    };

    fn profile() -> Profile {
        let mut p = Profile::new(Path::new("/w"), &super::super::Config::default(), None);
        p.read_only.push("/w/.git".into());
        p.hidden.push("/w/.env".into());
        p
    }

    /// Each relaxation procage reports is recorded in the profile, so the
    /// helper's cage is the fitted one.
    #[test]
    fn each_relaxation_is_recorded_in_the_profile() {
        let mut p = profile();
        let protect = relaxed(Relaxation::LeaveOutProtect, Some("/w/.git"), T1);
        assert!(adopt(&mut p, &protect).unwrap().contains("`.git`"));
        assert!(p.read_only.is_empty() && p.hidden == [Path::new("/w/.env")]);
        adopt(
            &mut p,
            &relaxed(Relaxation::LeaveOutHide, Some("/w/.env"), T1),
        )
        .unwrap();
        assert!(p.hidden.is_empty());
        let handoff = Relaxation::Waive(HardDenyClass::TrustHandoff);
        assert!(
            adopt(&mut p, &relaxed(handoff, None, T1))
                .unwrap()
                .contains("git hooks")
        );
        adopt(&mut p, &relaxed(Relaxation::NoUnixSockets, None, T1)).unwrap();
        adopt(
            &mut p,
            &relaxed(Relaxation::SignalsSameUser, None, Lift::default()),
        )
        .unwrap();
        assert!(p.unenforced.trust_handoff && p.unenforced.workspace_sockets);
        assert!(p.unenforced.signal_scope);
        // The translation follows the profile.
        let policy = policy(&p).unwrap();
        assert!(policy.waive.contains(&HardDenyClass::TrustHandoff));
        assert!(policy.ipc.unix_sockets.is_empty());
        assert_eq!(policy.process.signals, Signals::SameUser);
        assert!(
            !policy
                .fs
                .iter()
                .any(|r| matches!(r.access, Access::Protect | Access::Hide))
        );
    }

    /// What speccoder did not accept, or a path it does not know, fails.
    #[test]
    fn other_relaxations_fail() {
        let mut p = profile();
        for r in [
            relaxed(Relaxation::Waive(HardDenyClass::Secrets), None, T1),
            relaxed(Relaxation::NoLoopback, None, T1),
            relaxed(Relaxation::LeaveOutHide, Some("/w/other"), T1),
        ] {
            assert!(adopt(&mut p, &r).is_err(), "{r}");
        }
        assert_eq!(p, profile());
        assert_eq!(relax().accepted().count(), 5);
    }

    /// The cause is what procage says would lift it; each cause gets its own
    /// clause, so each fix is clear.
    #[test]
    fn the_notice_names_each_cause() {
        assert_eq!(cause(&T1), Cause::UserNamespaces);
        let abi6 = Lift {
            tier1: false,
            landlock_abi: Some(6),
        };
        assert_eq!(cause(&abi6), Cause::Kernel(6));
        assert_eq!(cause(&Lift::default()), Cause::Unknown);
        let mut notes = Vec::new();
        note(&mut notes, Cause::UserNamespaces, "sockets");
        assert_eq!(
            notice(&notes, Some(8)).unwrap(),
            "this machine does not allow user namespaces (on Ubuntu 23.10+: AppArmor), so sockets"
        );
        let mut kernel = Vec::new();
        note(&mut kernel, Cause::Kernel(6), "signals");
        let n = notice(&kernel, Some(5)).unwrap();
        assert_eq!(
            n,
            "this kernel is too old (Landlock ABI 5), so signals (Linux 6.12 lifts this)"
        );
        notes.extend(kernel);
        note(&mut notes, Cause::Unknown, "other");
        let all = notice(&notes, Some(5)).unwrap();
        assert!(
            all.starts_with("this machine does not allow user namespaces")
                && all.contains("; and this kernel is too old (Landlock ABI 5), so signals")
                && all.ends_with("; and this machine cannot enforce every part, so other"),
            "{all}"
        );
        let mut new = Vec::new();
        note(&mut new, Cause::Kernel(12), "x");
        assert!(
            notice(&new, None)
                .unwrap()
                .ends_with("x (Landlock ABI 12 lifts this)")
        );
        assert_eq!(notice(&[], Some(5)), None);
    }

    /// On older kernels every command is still caged; only the signal scope
    /// (and below ABI 3, truncation by path) is given up, and the notice
    /// says so. At the kernel's own ABI the scope holds.
    #[cfg(target_os = "linux")]
    #[test]
    fn older_kernels_keep_commands_caged() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let (root, home, outside) = (base.join("w"), base.join("home"), base.join("o"));
        for d in [&root, &home.join(".ssh"), &outside] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(home.join(".ssh/id"), "secret").unwrap();
        let mut sleeper = Command::new("sleep").arg("30").spawn().unwrap();
        let profile = Profile::new(&root, &super::super::Config::default(), Some(&home));
        let kernel = procage::linux::probe()
            .landlock_abi
            .min(procage::linux::BACKEND_MAX_ABI);
        let ok = |p: &Profile, cap: Option<u8>, script: &str| -> bool {
            let cage = cage_at(p, cap).unwrap();
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", script])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = cage.spawn(&mut cmd, &Grants::new()).unwrap();
            child.wait().unwrap().success()
        };
        let signal = format!("kill -0 {}", sleeper.id());
        // `os.rename`, not `mv`: `mv` copies where a rename fails.
        let moves = format!(
            "cd {} && mkdir -p a b && echo x > a/x && python3 -I -c \
             'import os; os.rename(\"a/x\", \"b/x\"); os.rename(\"b/x\", \"a/x\")'",
            root.display()
        );
        for cap in [1, 2, 5].into_iter().filter(|&c| c < kernel) {
            let (fitted, notice) = fit_at(&profile, Some(cap)).unwrap();
            assert!(fitted.unenforced.signal_scope, "ABI {cap}");
            let notice = notice.unwrap();
            assert!(
                notice.contains(&format!("this kernel is too old (Landlock ABI {cap})"))
                    && notice.contains("commands can signal your other processes"),
                "{notice}"
            );
            assert_eq!(
                notice.contains("truncating a file by path"),
                cap < 3,
                "{notice}"
            );
            assert_eq!(
                notice.contains("files cannot move between directories"),
                cap < 2,
                "{notice}"
            );
            let at = Some(cap);
            assert_eq!(ok(&fitted, at, &moves), cap >= 2, "ABI {cap}");
            assert!(
                ok(&fitted, at, &format!("echo x > {}/f", root.display())),
                "ABI {cap}"
            );
            assert!(
                !ok(&fitted, at, &format!("echo x > {}/f", outside.display())),
                "ABI {cap}"
            );
            assert!(
                !ok(&fitted, at, &format!("cat {}/.ssh/id", home.display())),
                "ABI {cap}"
            );
            assert!(ok(&fitted, at, &signal), "ABI {cap}");
        }
        if kernel >= 6 {
            let (fitted, _) = fit_at(&profile, None).unwrap();
            assert!(!fitted.unenforced.signal_scope);
            assert!(!ok(&fitted, None, &signal));
        }
        assert!(!outside.join("f").exists());
        let _ = sleeper.kill();
        let _ = sleeper.wait();
    }

    #[test]
    fn the_workspace_sockets_follow_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Profile::new(dir.path(), &super::super::Config::default(), None);
        assert!(!policy(&p).unwrap().ipc.unix_sockets.is_empty());
        p.network = Network::On;
        assert!(policy(&p).unwrap().ipc.unix_sockets.is_empty());
    }
}
