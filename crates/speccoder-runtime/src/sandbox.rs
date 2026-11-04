//! The OS sandbox for commands speccoder starts.
//!
//! Checks, the model's `run_command`, the formatter, acceptance tests and MCP
//! servers run code the model may have written. The policy decides *which*
//! commands run; the sandbox confines *what they do*:
//!
//! - writes only to the workspace, a private temporary directory and the
//!   package caches builds need (never the rest of the state directory —
//!   approvals live there — and never `~/.cargo/bin`);
//! - credentials unreadable (`~/.ssh`, `~/.aws`, `~/.cargo/credentials*`, …);
//! - no network beyond loopback, unless the command is allowed it
//!   (exactly, `[sandbox] network_allow`, or by a prefix in
//!   `[sandbox] network_allow_prefix`) or a person approved it.
//!
//! The [procage](https://github.com/hupe1980/procage) library confines
//! every command (module `sandbox::procage`): Seatbelt on macOS, Landlock,
//! seccomp and, where the kernel allows user namespaces, mounts on Linux.
//! Commands that run to completion are spawned in this process; long-running
//! servers start through the `speccoder` binary as a helper (`__sandbox`),
//! which confines itself and becomes the server. Where the host cannot
//! enforce part of the profile, that part is left out by name and the
//! runner says so.
//!
//! When a sandboxed command fails, its outcome carries what the sandbox
//! refused it (module `denials`): shown under a failed check, in the next
//! attempt's digest and in the model's `run_command` result. Linux lists
//! refused system calls only; macOS lists every denial it logged, which
//! under heavy load is not every denial.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::verify::process::{CommandRunner, Invocation, LocalRunner, Outcome};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod denials;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod procage;

/// What a sandboxed command may reach on the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    /// Nothing (on Linux also not loopback).
    Off,
    /// Loopback only: local test servers work, the outside does not. On
    /// Linux this is [`Network::Off`] (seccomp cannot tell addresses apart).
    Loopback,
    /// Everything.
    On,
}

/// Whether to sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Sandbox where the OS supports it; otherwise run unsandboxed and say so.
    Auto,
    /// Refuse to run commands unsandboxed.
    Required,
    /// Do not sandbox.
    Off,
}

/// `[sandbox]` in `speccoder.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Whether to sandbox.
    pub mode: Mode,
    /// The network for every command.
    pub network: Network,
    /// Exact commands that get the network (`[["cargo", "fetch"]]`).
    pub network_allow: Vec<Vec<String>>,
    /// Command prefixes (two elements or more) that get the network.
    pub network_allow_prefix: Vec<Vec<String>>,
    /// More writable paths (`~` is the home directory).
    pub writable: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            network: Network::Loopback,
            network_allow: Vec::new(),
            network_allow_prefix: Vec::new(),
            writable: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    mode: Option<String>,
    network: Option<String>,
    #[serde(default)]
    network_allow: Vec<Vec<String>>,
    #[serde(default)]
    network_allow_prefix: Vec<Vec<String>>,
    #[serde(default)]
    writable: Vec<String>,
}

/// Reads `[sandbox]`.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let Some(v) = table.get("sandbox") else {
        return Ok(Config::default());
    };
    let raw: Raw = v
        .clone()
        .try_into()
        .map_err(|e| format!("[sandbox]: {e}"))?;
    let mode = match raw.mode.as_deref() {
        None | Some("auto") => Mode::Auto,
        Some("required") => Mode::Required,
        Some("off") => Mode::Off,
        Some(o) => return Err(format!("[sandbox] mode = \"{o}\" (auto, required or off)")),
    };
    let network = match raw.network.as_deref() {
        None | Some("loopback") => Network::Loopback,
        Some("off") => Network::Off,
        Some("on") => Network::On,
        Some(o) => return Err(format!("[sandbox] network = \"{o}\" (off, loopback or on)")),
    };
    if let Some(why) = raw
        .network_allow_prefix
        .iter()
        .find_map(|p| crate::tools::policy::prefix_refusal(p))
    {
        return Err(format!("[sandbox] network_allow_prefix: {why}"));
    }
    Ok(Config {
        mode,
        network,
        network_allow: raw
            .network_allow
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect(),
        network_allow_prefix: raw.network_allow_prefix,
        writable: raw.writable,
    })
}

/// Credentials a command never reads, relative to the home directory.
pub const HIDDEN: &[&str] = &[
    ".ssh",
    ".aws",
    ".azure",
    ".gnupg",
    ".netrc",
    ".docker",
    ".kube",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".password-store",
    ".config/gh",
    ".config/gcloud",
    ".config/hub",
    ".cargo/credentials",
    ".cargo/credentials.toml",
    ".local/share/keyrings",
    "Library/Keychains",
];

/// Package caches builds write to, relative to the home directory. Not
/// `~/.cargo` as a whole: `~/.cargo/bin` would let a test replace `cargo`.
pub const CACHES: &[&str] = &[
    ".cargo/registry",
    ".cargo/git",
    ".cargo/.package-cache",
    ".cargo/.package-cache-mutate",
    ".cargo/.global-cache",
    ".cache",
    ".npm",
    ".pnpm-store",
    ".yarn/berry/cache",
    "go/pkg/mod",
    "Library/Caches",
];

/// One command's confinement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Writable trees (the workspace first).
    pub writable: Vec<PathBuf>,
    /// Unreadable trees.
    pub hidden: Vec<PathBuf>,
    /// Trees inside the writable ones that stay read-only (in place: the
    /// checkout's `.git`).
    pub read_only: Vec<PathBuf>,
    /// The network.
    pub network: Network,
    /// The private temporary directory (`TMPDIR`), writable.
    pub tmp: PathBuf,
    /// What this host cannot enforce and the profile leaves out
    /// (`sandbox::procage::fit`).
    #[serde(default)]
    pub unenforced: Unenforced,
}

/// Parts of a profile left out: because the host cannot enforce them (Linux
/// without user namespaces or with an older kernel; the runner's notice
/// names each), or on purpose
/// (the person's own commands may write git hooks and agent configuration).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Unenforced {
    /// Git hooks and shell startup files in writable trees stay writable.
    pub trust_handoff: bool,
    /// Unix sockets in writable trees are not reachable either.
    pub workspace_sockets: bool,
    /// Commands can signal every process of the user, not only their own
    /// (Linux older than 6.12 cannot keep signals within the run).
    pub signal_scope: bool,
    /// In place: `.git` and ignored files could not be carved out, so
    /// commands can write the one and read the other.
    pub carve: bool,
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl Profile {
    /// The profile for commands working in `root`, for the home directory `home`.
    #[must_use]
    pub fn new(root: &Path, config: &Config, home: Option<&Path>) -> Self {
        let root = canonical(root);
        let expand = |p: &str| -> Option<PathBuf> {
            match (p.strip_prefix("~/"), home) {
                (Some(rest), Some(h)) => Some(h.join(rest)),
                (Some(_), None) => None,
                (None, _) => Some(PathBuf::from(p)),
            }
        };
        let home = home.map(canonical);
        let home = home.as_deref();
        let mut writable = vec![root.clone()];
        writable.extend(
            home.into_iter()
                .flat_map(|h| CACHES.iter().map(move |c| h.join(c))),
        );
        writable.extend(
            config
                .writable
                .iter()
                .filter_map(|p| expand(p))
                .map(|p| canonical(&p)),
        );
        let hidden = home
            .into_iter()
            .flat_map(|h| HIDDEN.iter().map(move |c| h.join(c)))
            .collect();
        let digest = {
            use sha2::{Digest as _, Sha256};
            let d = Sha256::digest(root.to_string_lossy().as_bytes());
            d.iter()
                .take(6)
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        // Short (Unix socket paths are limited to ~104 bytes) and per workspace.
        let tmp = canonical(&std::env::temp_dir()).join(format!("speccoder-{digest}"));
        Self {
            writable,
            hidden,
            read_only: Vec::new(),
            network: config.network,
            tmp,
            unenforced: Unenforced::default(),
        }
    }

    /// The same profile with `read_only` trees carved out of the writable
    /// ones, and `hidden` added to the unreadable ones.
    #[must_use]
    pub fn carve(mut self, read_only: &[PathBuf], hidden: &[PathBuf]) -> Self {
        self.read_only
            .extend(read_only.iter().map(|p| canonical(p)));
        self.hidden.extend(hidden.iter().map(|p| canonical(p)));
        self
    }

    /// The same profile with another network.
    #[must_use]
    pub fn with_network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }
}

/// This machine's sandbox: procage, which confines with Seatbelt on macOS
/// and Landlock and seccomp on Linux. Long-running servers start through the
/// `speccoder` binary at `helper`, when there is one (commands that run to
/// completion need none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    /// The `speccoder` binary that serves as the helper.
    pub helper: Option<PathBuf>,
}

impl Backend {
    /// The OS mechanism, for people and run records: `Seatbelt` or
    /// `Landlock+seccomp`.
    #[must_use]
    pub fn mechanism(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "Seatbelt"
        } else {
            "Landlock+seccomp"
        }
    }
}

/// The helper subcommand (`speccoder __sandbox <profile> -- argv…`).
pub const HELPER_ARG: &str = "__sandbox";

/// The sandbox this machine offers, or why there is none.
pub fn backend() -> Result<Backend, String> {
    static CACHED: OnceLock<Result<Backend, String>> = OnceLock::new();
    CACHED.get_or_init(detect).clone()
}

fn detect() -> Result<Backend, String> {
    available()?;
    Ok(Backend {
        helper: helper_binary(),
    })
}

/// Whether this machine can sandbox commands at all (whatever process asks).
///
/// # Errors
///
/// Why not.
pub fn available() -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    return procage::available();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Err(format!("no sandbox for {}", std::env::consts::OS))
}

/// The `speccoder` binary that serves as the helper: `SPECCODER_SANDBOX_HELPER`,
/// or this executable when it is `speccoder`.
fn helper_binary() -> Option<PathBuf> {
    std::env::var_os("SPECCODER_SANDBOX_HELPER")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .filter(|p| p.file_stem().is_some_and(|n| n == "speccoder"))
        })
}

/// The profile this machine enforces for `profile`, and a notice for what it
/// leaves out.
///
/// # Errors
///
/// Why this machine cannot confine the profile at all.
pub fn fit(profile: &Profile) -> Result<(Profile, Option<String>), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    return procage::fit(profile);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = profile;
        Err(format!("no sandbox for {}", std::env::consts::OS))
    }
}

/// `argv` confined by `p` through the helper on `backend`.
pub fn wrap(p: &Profile, backend: &Backend, argv: &[String]) -> Result<Vec<String>, String> {
    // Without the helper a server cannot be confined: never start it plain.
    let helper = backend.helper.as_ref().ok_or_else(|| {
        "servers start through the speccoder binary, which was not found (SPECCODER_SANDBOX_HELPER)"
            .to_owned()
    })?;
    let mut out = vec![
        helper.to_string_lossy().into_owned(),
        HELPER_ARG.to_owned(),
        serde_json::to_string(p).map_err(|e| e.to_string())?,
        "--".to_owned(),
    ];
    out.extend_from_slice(argv);
    Ok(out)
}

/// A runner that confines every command.
#[derive(Debug, Clone)]
pub struct Sandboxed {
    /// The confinement.
    pub profile: Profile,
    /// The OS mechanism.
    pub backend: Backend,
    /// Exact commands that get the network.
    pub network_allow: Vec<Vec<String>>,
    /// Prefixes (two elements or more) that get it.
    pub network_allow_prefix: Vec<Vec<String>>,
}

impl Sandboxed {
    /// The network `argv` gets: its profile's, or all of it for an allowed
    /// command (exactly, or extending an allowed prefix).
    #[must_use]
    pub fn network_for(&self, argv: &[String]) -> Network {
        if self.network_allow.iter().any(|p| p.as_slice() == argv)
            || self
                .network_allow_prefix
                .iter()
                .any(|p| crate::tools::policy::prefix_matches(p, argv))
        {
            Network::On
        } else {
            self.profile.network
        }
    }

    /// Runs `inv` with `network` instead of the profile's.
    #[must_use]
    pub fn run_with_network(&self, inv: &Invocation, network: Network) -> Outcome {
        let profile = self.profile.clone().with_network(network);
        let failed = |e: String| Outcome {
            exit_code: None,
            timed_out: false,
            spawn_error: Some(e),
            started: std::time::SystemTime::now(),
            duration: std::time::Duration::ZERO,
            stdout: crate::verify::process::Captured::default(),
            stderr: crate::verify::process::Captured::default(),
            sandbox: Default::default(),
        };
        // Through the wrapper a missing program would be the wrapper's
        // failure; report it as the plain runner does.
        if let Some(program) = inv.argv.first()
            && !resolvable(program, &inv.cwd, &inv.env)
        {
            return failed(format!(
                "could not start `{program}`: No such file or directory"
            ));
        }
        if let Err(e) = std::fs::create_dir_all(&profile.tmp) {
            return failed(format!("the sandbox's temporary directory: {e}"));
        }
        let tmp = profile.tmp.to_string_lossy().into_owned();
        let mut env: Vec<(String, String)> = inv
            .env
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "TMPDIR" | "TMP" | "TEMP"))
            .cloned()
            .collect();
        env.extend(["TMPDIR", "TMP", "TEMP"].map(|k| (k.to_owned(), tmp.clone())));
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return procage::run(&profile, &Invocation { env, ..inv.clone() });
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = env;
            failed(format!("no sandbox for {}", std::env::consts::OS))
        }
    }
}

/// The executable `program` runs as in `cwd` with `env`: a program with a
/// `/` against `cwd`, else the first file of that name on `PATH` (relative
/// `PATH` entries against `cwd` too) — canonicalised; `None` when there is
/// none. A file without an execute bit is skipped, as `execvp` skips it:
/// the executable named is the one that runs. What a command's approval
/// names, and what it is checked against again when it runs.
#[must_use]
pub fn resolve_executable(program: &str, cwd: &Path, env: &[(String, String)]) -> Option<PathBuf> {
    let candidate = |p: PathBuf| {
        executable_file(&p)
            .then(|| std::fs::canonicalize(&p).ok())
            .flatten()
    };
    if program.is_empty() {
        return None;
    }
    if program.contains('/') {
        return candidate(cwd.join(program));
    }
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.as_str());
    path.unwrap_or("/usr/bin:/bin")
        .split(':')
        .filter(|d| !d.is_empty())
        .find_map(|d| candidate(cwd.join(d).join(program)))
}

/// A regular file (through symlinks) that someone may execute.
fn executable_file(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Whether `program` names an executable file, directly or on `PATH`.
fn resolvable(program: &str, cwd: &Path, env: &[(String, String)]) -> bool {
    resolve_executable(program, cwd, env).is_some()
}

impl CommandRunner for Sandboxed {
    fn run(&self, inv: &Invocation) -> Outcome {
        self.run_with_network(inv, self.network_for(&inv.argv))
    }
}

/// The runner for commands working in `root`: sandboxed when configured and
/// possible; otherwise the plain runner and a notice (or, with `required`,
/// an error).
pub fn runner(root: &Path, config: &Config) -> Result<(Runner, Option<String>), String> {
    if config.mode == Mode::Off {
        return Ok((
            Runner::Plain,
            Some("commands run unsandboxed ([sandbox] mode = \"off\")".to_owned()),
        ));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let profile = Profile::new(root, config, home.as_deref());
    // What this machine cannot enforce is left out by name; anything else it
    // cannot confine means no sandbox, and `mode` decides.
    match backend().and_then(|backend| Ok((backend, fit(&profile)?))) {
        Ok((backend, (profile, notice))) => Ok((
            Runner::Sandboxed(Sandboxed {
                profile,
                backend,
                network_allow: config.network_allow.clone(),
                network_allow_prefix: config.network_allow_prefix.clone(),
            }),
            notice,
        )),
        Err(e) if config.mode == Mode::Required => Err(format!(
            "[sandbox] mode = \"required\", but there is no sandbox: {e}"
        )),
        Err(e) => Ok((
            Runner::Plain,
            Some(format!("commands run unsandboxed: {e}")),
        )),
    }
}

/// What `[approvals] contained` comes to at `root`, in one line: the
/// commands the sandbox confines run without asking, or why every command
/// that is not a check asks — what `init`, `new` and `doctor` say.
#[must_use]
pub fn contained_at(root: &Path) -> (bool, String) {
    let text = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let allowed = crate::substrate::run_config::parse(&text).is_ok_and(|c| c.contained);
    if !allowed {
        return (
            true,
            "every command that is not a check asks ([approvals] contained = \"ask\")".to_owned(),
        );
    }
    match runner_at(root).map(|(r, _)| r.confines()) {
        Ok(Some(m)) => (
            true,
            format!(
                "commands the sandbox confines run without asking ({m}) — [approvals] contained = \"ask\" to be asked"
            ),
        ),
        Ok(None) | Err(_) => (
            false,
            "every command that is not a check asks: no sandbox confines them here".to_owned(),
        ),
    }
}

/// [`runner`] with `[sandbox]` read from the `speccoder.toml` that governs `root`.
pub fn runner_at(root: &Path) -> Result<(Runner, Option<String>), String> {
    let text = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let config = parse(&text).map_err(|e| format!("speccoder.toml: {e}"))?;
    runner(root, &config)
}

impl Runner {
    /// In place: `read_only` and `hidden` carved out of the profile, and
    /// what the person must know when this machine cannot. A plain runner
    /// confines nothing, so there is nothing to carve.
    #[must_use]
    pub fn carve(self, read_only: &[PathBuf], hidden: &[PathBuf]) -> (Self, Option<String>) {
        match self {
            Self::Sandboxed(mut s) => {
                let carved = s.profile.clone().carve(read_only, hidden);
                match fit(&carved) {
                    Ok((fitted, notice)) => {
                        s.profile = fitted;
                        (Self::Sandboxed(s), notice)
                    }
                    Err(e) => (
                        Self::Sandboxed({
                            s.profile.unenforced.carve = true;
                            s
                        }),
                        Some(format!(
                            "the sandbox cannot carve here ({e}): commands can write `.git` and read ignored secrets in place; the run stops if `.git` changes"
                        )),
                    ),
                }
            }
            Self::Plain => (Self::Plain, None),
        }
    }
}

/// A sandboxed or a plain runner.
#[derive(Debug, Clone)]
pub enum Runner {
    /// Confined.
    Sandboxed(Sandboxed),
    /// Not confined.
    Plain,
}

impl Runner {
    /// Whether commands are confined (the network rule then matters).
    #[must_use]
    pub fn is_sandboxed(&self) -> bool {
        matches!(self, Self::Sandboxed(_))
    }

    /// The mechanism that confines a command to the worktree — writes
    /// there, no network beyond loopback, no signal to other processes,
    /// `.git` carved out in place — when this runner does: a call it
    /// confines needs nobody's approval (`[approvals] contained`).
    #[must_use]
    pub fn confines(&self) -> Option<&'static str> {
        match self {
            Self::Sandboxed(s)
                if s.profile.network != Network::On
                    && !s.profile.unenforced.signal_scope
                    && !s.profile.unenforced.carve =>
            {
                Some(s.backend.mechanism())
            }
            _ => None,
        }
    }

    /// The same runner, with git hooks, git config and agent configuration
    /// (the trust-handoff names) writable in its trees: for the tools that
    /// create a project (`git init`, a format's init), never for the model's
    /// commands.
    #[must_use]
    pub fn with_trust_handoff_writable(self) -> Self {
        match self {
            Self::Sandboxed(mut s) => {
                s.profile.unenforced.trust_handoff = true;
                Self::Sandboxed(s)
            }
            Self::Plain => Self::Plain,
        }
    }

    /// The same runner, `dir` writable too.
    #[must_use]
    pub fn with_writable(self, dir: &Path) -> Self {
        match self {
            Self::Sandboxed(mut s) => {
                s.profile.writable.push(canonical(dir));
                Self::Sandboxed(s)
            }
            Self::Plain => Self::Plain,
        }
    }

    /// Whether `argv` gets the network by `[sandbox] network_allow` (or a
    /// prefix): never for a plain runner, which asks nobody.
    #[must_use]
    pub fn allows_network(&self, argv: &[String]) -> bool {
        matches!(self, Self::Sandboxed(s) if s.network_for(argv) == Network::On)
    }

    /// Runs `inv`; `network` (a person's approval) widens the network.
    #[must_use]
    pub fn run_with(&self, inv: &Invocation, network: bool) -> Outcome {
        match self {
            Self::Plain => LocalRunner.run(inv),
            Self::Sandboxed(s) if network => s.run_with_network(inv, Network::On),
            Self::Sandboxed(s) => s.run(inv),
        }
    }

    /// Where commands may write scratch files: the sandbox's private
    /// temporary directory (created), or `None` unsandboxed.
    #[must_use]
    pub fn scratch(&self) -> Option<PathBuf> {
        match self {
            Self::Plain => None,
            Self::Sandboxed(s) => std::fs::create_dir_all(&s.profile.tmp)
                .ok()
                .map(|()| s.profile.tmp.clone()),
        }
    }

    /// `argv` as it will be started (for long-running servers started elsewhere).
    pub fn wrap(&self, argv: &[String]) -> Result<(Vec<String>, Option<PathBuf>), String> {
        match self {
            Self::Plain => Ok((argv.to_vec(), None)),
            Self::Sandboxed(s) => {
                let profile = s.profile.clone().with_network(s.network_for(argv));
                std::fs::create_dir_all(&profile.tmp).map_err(|e| e.to_string())?;
                Ok((wrap(&profile, &s.backend, argv)?, Some(profile.tmp)))
            }
        }
    }
}

impl CommandRunner for Runner {
    fn run(&self, inv: &Invocation) -> Outcome {
        self.run_with(inv, false)
    }
}

/// The helper's entry (`speccoder __sandbox <profile> -- argv…`): confines
/// this process and becomes the command; returns only on failure.
pub fn helper_main(args: &[String]) -> String {
    let (Some(profile), Some("--")) = (args.first(), args.get(1).map(String::as_str)) else {
        return format!("usage: speccoder {HELPER_ARG} <profile> -- <argv…>");
    };
    let profile: Profile = match serde_json::from_str(profile) {
        Ok(p) => p,
        Err(e) => return format!("the sandbox profile: {e}"),
    };
    let argv = &args[2..];
    if argv.is_empty() {
        return "nothing to run".to_owned();
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    return procage::exec(&profile, argv);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    format!("no sandbox for {}", std::env::consts::OS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_and_defaults_to_loopback() {
        assert_eq!(parse("").unwrap_or_default(), Config::default());
        let c = parse("[sandbox]\nmode = \"required\"\nnetwork = \"off\"\nnetwork_allow = [[\"cargo\", \"fetch\"]]\nwritable = [\"~/.m2\"]\n")
            .unwrap_or_default();
        assert_eq!((c.mode, c.network), (Mode::Required, Network::Off));
        assert!(parse("[sandbox]\nnetwork = \"some\"\n").is_err());
    }

    #[test]
    fn the_profile_keeps_the_state_directory_and_cargo_bin_out() {
        let home = Path::new("/home/u");
        let p = Profile::new(
            Path::new("/s/proj/worktrees/001"),
            &Config::default(),
            Some(home),
        );
        assert_eq!(p.writable[0], Path::new("/s/proj/worktrees/001"));
        assert!(
            !p.writable
                .iter()
                .any(|w| w == Path::new("/s/proj") || w == &home.join(".cargo"))
        );
        assert!(p.hidden.contains(&home.join(".ssh")));
        assert!(p.writable.contains(&home.join(".cargo/registry")));
        assert_eq!(p.unenforced, Unenforced::default());
    }

    /// A profile written before the signal scope could be left out
    /// enforces it; a fitted one keeps the choice through the helper.
    #[test]
    fn the_signal_scope_is_enforced_unless_the_profile_says_otherwise() {
        let p = Profile::new(Path::new("/w"), &Config::default(), None);
        let mut json: serde_json::Value = serde_json::to_value(&p).unwrap();
        json["unenforced"] =
            serde_json::json!({"trust_handoff": false, "workspace_sockets": false});
        let old: Profile = serde_json::from_value(json).unwrap();
        assert!(!old.unenforced.signal_scope);
        let mut fitted = p.clone();
        fitted.unenforced.signal_scope = true;
        let back: Profile = serde_json::from_str(&serde_json::to_string(&fitted).unwrap()).unwrap();
        assert!(back.unenforced.signal_scope);
    }

    #[test]
    fn the_executable_is_the_first_on_the_path_canonicalised() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        for d in ["bin", "usr"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
            let f = root.join(d).join("make");
            std::fs::write(&f, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let env = |path: &str| vec![("PATH".to_owned(), path.to_owned())];
        let usr = format!("{}", root.join("usr").display());
        assert_eq!(
            resolve_executable("make", &root, &env(&usr)),
            Some(root.join("usr/make"))
        );
        // A worktree directory earlier on the path shadows it.
        assert_eq!(
            resolve_executable("make", &root, &env(&format!("bin:{usr}"))),
            Some(root.join("bin/make"))
        );
        assert_eq!(
            resolve_executable("./bin/make", &root, &env("")),
            Some(root.join("bin/make"))
        );
        assert_eq!(resolve_executable("nope", &root, &env(&usr)), None);
        assert!(!resolvable("nope", &root, &env(&usr)));
        // A file without an execute bit is skipped, as execvp skips it.
        std::fs::set_permissions(
            root.join("bin/make"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(
            resolve_executable("make", &root, &env(&format!("bin:{usr}"))),
            Some(root.join("usr/make"))
        );
        assert_eq!(resolve_executable("./bin/make", &root, &env("")), None);
    }

    /// The experimental `engine` key is gone: procage is the only engine.
    #[test]
    fn the_engine_key_is_refused_by_name() {
        let e = parse("[sandbox]\nengine = \"procage\"\n").unwrap_err();
        assert!(e.contains("engine"), "{e}");
    }

    /// A profile this machine cannot confine at all (here: a writable path
    /// with a control character, which Seatbelt cannot express) means no
    /// sandbox: `auto` runs commands plain and says why, `required` refuses.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_unconfinable_profile_follows_the_mode() {
        if backend().is_err() {
            return; // no sandbox on this machine at all
        }
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join("procage\u{1}odd");
        std::fs::create_dir_all(&odd).unwrap();
        let mut config = Config {
            writable: vec![odd.to_string_lossy().into_owned()],
            ..Config::default()
        };
        let (plain, notice) = runner(dir.path(), &config).unwrap();
        assert!(!plain.is_sandboxed());
        assert!(
            notice.is_some_and(|n| n.starts_with("commands run unsandboxed: ")),
            "{plain:?}"
        );
        config.mode = Mode::Required;
        let e = runner(dir.path(), &config).unwrap_err();
        assert!(e.contains("mode = \"required\""), "{e}");
    }

    #[test]
    fn a_confinable_profile_is_sandboxed() {
        let dir = tempfile::tempdir().unwrap();
        let Ok((Runner::Sandboxed(s), _)) = runner(dir.path(), &Config::default()) else {
            assert!(
                backend().is_err(),
                "a sandbox exists but the runner is plain"
            );
            return;
        };
        assert_eq!(s.backend, backend().unwrap());
    }

    #[test]
    fn network_allow_is_exact_and_a_prefix_needs_two_elements() {
        let c = parse("[sandbox]\nnetwork_allow = [[\"cargo\", \"fetch\"]]\nnetwork_allow_prefix = [[\"npm\", \"ci\"]]\n").unwrap();
        let s = Sandboxed {
            profile: Profile::new(Path::new("/w"), &c, None),
            backend: Backend { helper: None },
            network_allow: c.network_allow.clone(),
            network_allow_prefix: c.network_allow_prefix.clone(),
        };
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(s.network_for(&argv("cargo fetch")), Network::On);
        assert_ne!(s.network_for(&argv("cargo fetch --x")), Network::On);
        assert_eq!(s.network_for(&argv("npm ci --silent")), Network::On);
        assert!(parse("[sandbox]\nnetwork_allow_prefix = [[\"npm\"]]\n").is_err());
    }
}
