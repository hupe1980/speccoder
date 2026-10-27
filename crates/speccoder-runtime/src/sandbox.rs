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
//! macOS: Seatbelt (`sandbox-exec`) with a generated profile. Linux: the
//! `speccoder` binary re-executes itself as a helper (`__sandbox`) that
//! applies Landlock and a seccomp filter to itself, sets `no_new_privs` and
//! executes the command — so the workspace keeps `unsafe_code` forbidden.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::verify::process::{CommandRunner, Invocation, LocalRunner, Outcome};

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

/// How this machine sandboxes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    /// macOS Seatbelt.
    Seatbelt,
    /// Linux Landlock + seccomp through the `speccoder` helper at this path.
    Landlock(PathBuf),
}

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// The helper subcommand (`speccoder __sandbox <profile> -- argv…`).
pub const HELPER_ARG: &str = "__sandbox";

/// The sandbox this machine offers, or why there is none.
pub fn backend() -> Result<Backend, String> {
    static CACHED: OnceLock<Result<Backend, String>> = OnceLock::new();
    CACHED.get_or_init(detect).clone()
}

fn detect() -> Result<Backend, String> {
    if cfg!(target_os = "macos") {
        return if Path::new(SANDBOX_EXEC).exists() {
            Ok(Backend::Seatbelt)
        } else {
            Err(format!("{SANDBOX_EXEC} is missing"))
        };
    }
    if cfg!(target_os = "linux") {
        let helper = std::env::var_os("SPECCODER_SANDBOX_HELPER")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .filter(|p| p.file_stem().is_some_and(|n| n == "speccoder"))
            })
            .ok_or_else(|| "the speccoder helper binary was not found".to_owned())?;
        return with_helper(helper);
    }
    Err(format!("no sandbox for {}", std::env::consts::OS))
}

/// The Linux backend with the helper binary at `helper`, if the kernel
/// supports Landlock.
pub fn with_helper(helper: PathBuf) -> Result<Backend, String> {
    linux_supported()?;
    Ok(Backend::Landlock(helper))
}

#[cfg(target_os = "linux")]
fn linux_supported() -> Result<(), String> {
    linux::probe()
}

#[cfg(not(target_os = "linux"))]
fn linux_supported() -> Result<(), String> {
    Err("not Linux".to_owned())
}

fn quote(p: &Path) -> String {
    let s = p.to_string_lossy();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `text` as a literal in a Seatbelt (POSIX extended) regular expression.
fn regex_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\^$.|?*+()[]{}\"".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The Seatbelt profile for `p`. Later rules win.
#[must_use]
pub fn seatbelt(p: &Profile) -> String {
    let mut out = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
    out.push_str("(allow file-write*");
    for w in p.writable.iter().chain([&p.tmp]) {
        out.push_str(&format!(" (subpath {})", quote(w)));
    }
    // macOS's `mktemp` ignores `TMPDIR` and creates `tmp.XXXXXXXX` in the
    // per-user temporary directory (the private one's parent): only those
    // names, and what is inside them, are writable there.
    if let Some(user_tmp) = p.tmp.parent() {
        out.push_str(&format!(
            " (regex #\"^{}/tmp\\.[A-Za-z0-9]+(/|$)\")",
            regex_escape(&user_tmp.to_string_lossy())
        ));
    }
    out.push_str(
        " (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\") (regex #\"^/dev/fd/\") (regex #\"^/dev/ttys\"))\n",
    );
    if !p.read_only.is_empty() {
        out.push_str("(deny file-write*");
        for r in &p.read_only {
            out.push_str(&format!(" (subpath {})", quote(r)));
        }
        out.push_str(")\n");
    }
    if !p.hidden.is_empty() {
        out.push_str("(deny file-read* file-write*");
        for h in &p.hidden {
            out.push_str(&format!(" (subpath {})", quote(h)));
        }
        out.push_str(")\n");
    }
    // Opening applications (and Apple events) would run code outside the sandbox.
    out.push_str("(deny appleevent-send)\n");
    out.push_str("(deny mach-lookup (global-name-regex #\"^com\\.apple\\.lsd\\.\") (global-name \"com.apple.coreservices.launchservicesd\") (global-name \"com.apple.coreservices.appleevents\"))\n");
    match p.network {
        Network::On => {}
        Network::Off | Network::Loopback => {
            out.push_str("(deny network*)\n");
            if p.network == Network::Loopback {
                // Not `(local ip "localhost:*")` for all operations: it also
                // matches outbound connections to anywhere.
                out.push_str("(allow network-outbound (remote ip \"localhost:*\"))\n(allow network-bind network-inbound (local ip \"localhost:*\"))\n");
            }
            // Unix sockets inside the writable trees (not the Docker socket).
            out.push_str("(allow network-outbound network-bind network-inbound");
            for w in p.writable.iter().chain([&p.tmp]) {
                out.push_str(&format!(
                    " (remote unix-socket (subpath {q})) (local unix-socket (subpath {q}))",
                    q = quote(w)
                ));
            }
            out.push_str(")\n");
        }
    }
    out
}

/// `argv` confined by `p` on `backend`.
pub fn wrap(p: &Profile, backend: &Backend, argv: &[String]) -> Result<Vec<String>, String> {
    match backend {
        Backend::Seatbelt => {
            let mut out = vec![SANDBOX_EXEC.to_owned(), "-p".to_owned(), seatbelt(p)];
            out.extend_from_slice(argv);
            Ok(out)
        }
        Backend::Landlock(helper) => {
            let mut out = vec![
                helper.to_string_lossy().into_owned(),
                HELPER_ARG.to_owned(),
                serde_json::to_string(p).map_err(|e| e.to_string())?,
                "--".to_owned(),
            ];
            out.extend_from_slice(argv);
            Ok(out)
        }
    }
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
    /// Variables every command gets: a worktree's linked `.venv` sets
    /// `PYTHONPATH` (put before the command's own) and `UV_NO_SYNC`.
    pub env: Vec<(String, String)>,
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
        let argv = match wrap(&profile, &self.backend, &inv.argv) {
            Ok(a) => a,
            Err(e) => return failed(e),
        };
        let tmp = profile.tmp.to_string_lossy().into_owned();
        let mut env: Vec<(String, String)> = inv
            .env
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "TMPDIR" | "TMP" | "TEMP"))
            .cloned()
            .collect();
        env.extend(["TMPDIR", "TMP", "TEMP"].map(|k| (k.to_owned(), tmp.clone())));
        for (k, v) in &self.env {
            let own = env
                .iter()
                .position(|(key, _)| key == k)
                .map(|i| env.remove(i).1);
            let value = match own {
                Some(o) if k == "PYTHONPATH" && !o.is_empty() => format!("{v}:{o}"),
                _ => v.clone(),
            };
            env.push((k.clone(), value));
        }
        LocalRunner.run(&Invocation {
            argv,
            env,
            ..inv.clone()
        })
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
    match backend() {
        Ok(backend) => {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            Ok((
                Runner::Sandboxed(Sandboxed {
                    profile: Profile::new(root, config, home.as_deref()),
                    backend,
                    network_allow: config.network_allow.clone(),
                    network_allow_prefix: config.network_allow_prefix.clone(),
                    // A worktree prepared with a linked `.venv`: its checks
                    // import the worktree's code, wherever they start.
                    env: crate::prepare::recorded_env(root),
                }),
                None,
            ))
        }
        Err(e) if config.mode == Mode::Required => Err(format!(
            "[sandbox] mode = \"required\", but there is no sandbox: {e}"
        )),
        Err(e) => Ok((
            Runner::Plain,
            Some(format!("commands run unsandboxed: {e}")),
        )),
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
                // Linux carves with mounts in a user namespace, which some
                // hosts do not allow (container seccomp, AppArmor).
                if let Backend::Landlock(helper) = &s.backend
                    && !mounts_supported(helper)
                {
                    return (
                        Self::Sandboxed(s),
                        Some("this machine does not allow user namespaces (on Ubuntu 23.10+: AppArmor), so commands can write `.git`, read ignored secrets in place and reach daemon sockets such as Docker's; the run stops if `.git` changes".to_owned()),
                    );
                }
                s.profile = s.profile.carve(read_only, hidden);
                (Self::Sandboxed(s), None)
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

    /// The same runner, its commands also given `env` (see
    /// [`Sandboxed::env`]); a plain runner is unchanged — nothing is ever
    /// linked into a worktree whose commands run unsandboxed.
    #[must_use]
    pub fn with_env(self, env: Vec<(String, String)>) -> Self {
        match self {
            Self::Sandboxed(mut s) => {
                s.env = env;
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
/// this process and executes the command; returns only on failure.
pub fn helper_main(args: &[String]) -> String {
    if args.first().map(String::as_str) == Some(PROBE_MOUNTS) {
        return match probe_mounts() {
            Ok(()) => std::process::exit(0),
            Err(e) => e,
        };
    }
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
    enter(&profile, argv)
}

#[cfg(target_os = "linux")]
fn enter(profile: &Profile, argv: &[String]) -> String {
    linux::enter(profile, argv)
}

/// The helper's argument that only checks the mount carve-outs work.
const PROBE_MOUNTS: &str = "--probe-mounts";

/// Whether the Linux helper can carve with mounts here (checked once, in a
/// child: the namespace never touches this process).
fn mounts_supported(helper: &Path) -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::process::Command::new(helper)
            .args([HELPER_ARG, PROBE_MOUNTS])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

#[cfg(target_os = "linux")]
fn probe_mounts() -> Result<(), String> {
    linux::probe_mounts()
}

#[cfg(not(target_os = "linux"))]
fn probe_mounts() -> Result<(), String> {
    Err("mount carve-outs are for Linux".to_owned())
}

#[cfg(not(target_os = "linux"))]
fn enter(_profile: &Profile, _argv: &[String]) -> String {
    format!("the {HELPER_ARG} helper is for Linux")
}

#[cfg(target_os = "linux")]
mod linux {
    //! Landlock (filesystem) and seccomp (network) for this process, then exec.

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use landlock::{
        ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus,
    };
    use seccompiler::{
        SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
        SeccompRule, TargetArch,
    };

    use super::{Network, Profile};

    const ABI_LEVEL: ABI = ABI::V3;

    pub(super) fn probe() -> Result<(), String> {
        Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::from_all(ABI_LEVEL))
            .and_then(|r| r.create())
            .map(|_| ())
            .map_err(|e| format!("Landlock is not available: {e}"))
    }

    /// Landlock cannot take back what a rule grants beneath it, so the
    /// carve-outs inside a writable tree are mounts in a namespace of this
    /// process alone: `read_only` trees bound read-only, `hidden` ones
    /// inside a writable tree covered (a file by `/dev/null`, a directory
    /// by an empty read-only tmpfs).
    fn mount_carve(profile: &Profile) -> Result<(), String> {
        use nix::mount::{MsFlags, mount};
        let inside = |p: &PathBuf| profile.writable.iter().any(|w| p.starts_with(w));
        let hidden: Vec<&PathBuf> = profile
            .hidden
            .iter()
            .filter(|h| inside(h) && h.exists())
            .collect();
        let sockets: Vec<PathBuf> = daemon_sockets()
            .into_iter()
            .filter(|p| !inside(p))
            .collect();
        let carved = !profile.read_only.is_empty() || !hidden.is_empty();
        if !carved && sockets.is_empty() {
            return Ok(());
        }
        // Without user namespaces (AppArmor on Ubuntu 23.10+) the runner has
        // already said so and asked for no carve-outs: the sockets stay
        // reachable rather than every command failing.
        if let Err(e) = private_namespace() {
            return if carved { Err(e) } else { Ok(()) };
        }
        let none: Option<&str> = None;
        for p in profile.read_only.iter().filter(|p| p.exists()) {
            mount(Some(p), p, none, MsFlags::MS_BIND | MsFlags::MS_REC, none)
                .map_err(|e| format!("bind {}: {e}", p.display()))?;
            mount(
                none,
                p,
                none,
                MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY | locked(p),
                none,
            )
            .map_err(|e| format!("read-only {}: {e}", p.display()))?;
        }
        for h in hidden {
            if h.is_dir() {
                mount(
                    Some("tmpfs"),
                    h,
                    Some("tmpfs"),
                    MsFlags::MS_RDONLY | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
                    Some("size=4k"),
                )
            } else {
                mount(Some("/dev/null"), h, none, MsFlags::MS_BIND, none)
            }
            .map_err(|e| format!("hide {}: {e}", h.display()))?;
        }
        // Landlock does not govern connecting to a Unix socket and seccomp
        // cannot see its path: a daemon's socket (Docker's is root on the
        // host) is covered instead, so connecting to it fails.
        for p in &sockets {
            mount(Some("/dev/null"), p, none, MsFlags::MS_BIND, none)
                .map_err(|e| format!("cover {}: {e}", p.display()))?;
        }
        Ok(())
    }

    /// The Unix sockets in the places daemons keep them (`/run`, `/var/run`,
    /// `/tmp`, `$XDG_RUNTIME_DIR`, three levels deep), each once.
    fn daemon_sockets() -> Vec<PathBuf> {
        use std::os::unix::fs::FileTypeExt as _;
        fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let Ok(t) = e.file_type() else { continue };
                if t.is_socket() {
                    out.push(e.path());
                } else if t.is_dir() && depth > 0 {
                    walk(&e.path(), depth - 1, out);
                }
            }
        }
        let mut roots = vec![
            PathBuf::from("/run"),
            PathBuf::from("/var/run"),
            PathBuf::from("/tmp"),
        ];
        if let Some(x) = std::env::var_os("XDG_RUNTIME_DIR") {
            roots.push(PathBuf::from(x));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for root in roots {
            // `/var/run` is usually a link to `/run`: walk each tree once.
            let Ok(real) = root.canonicalize() else {
                continue;
            };
            if !seen.insert(real.clone()) {
                continue;
            }
            walk(&real, 3, &mut out);
        }
        out.sort();
        out.dedup();
        out
    }

    /// A user and mount namespace for this process, mapped to its own ids,
    /// with no mount propagating out.
    fn private_namespace() -> Result<(), String> {
        use nix::mount::{MsFlags, mount};
        use nix::sched::{CloneFlags, unshare};
        let (uid, gid) = (nix::unistd::getuid(), nix::unistd::getgid());
        unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)
            .map_err(|e| format!("user namespace: {e}"))?;
        let write = |file: &str, text: String| {
            std::fs::write(file, text).map_err(|e| format!("{file}: {e}"))
        };
        write("/proc/self/setgroups", "deny".to_owned())?;
        write("/proc/self/uid_map", format!("{uid} {uid} 1"))?;
        write("/proc/self/gid_map", format!("{gid} {gid} 1"))?;
        let none: Option<&str> = None;
        mount(none, "/", none, MsFlags::MS_REC | MsFlags::MS_PRIVATE, none)
            .map_err(|e| format!("private mounts: {e}"))
    }

    /// The flags a bind mount inherits and may not drop in a user namespace.
    fn locked(p: &Path) -> nix::mount::MsFlags {
        use nix::mount::MsFlags;
        use nix::sys::statvfs::{FsFlags, statvfs};
        let Ok(st) = statvfs(p) else {
            return MsFlags::empty();
        };
        let f = st.flags();
        let mut out = MsFlags::empty();
        for (fs, ms) in [
            (FsFlags::ST_NOSUID, MsFlags::MS_NOSUID),
            (FsFlags::ST_NODEV, MsFlags::MS_NODEV),
            (FsFlags::ST_NOEXEC, MsFlags::MS_NOEXEC),
            (FsFlags::ST_NOATIME, MsFlags::MS_NOATIME),
            (FsFlags::ST_NODIRATIME, MsFlags::MS_NODIRATIME),
            (FsFlags::ST_RELATIME, MsFlags::MS_RELATIME),
        ] {
            if f.contains(fs) {
                out |= ms;
            }
        }
        out
    }

    /// Whether [`mount_carve`] works here: a read-only bind of a temporary
    /// directory in a namespace of this (probe) process.
    pub(super) fn probe_mounts() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("speccoder-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let profile = Profile {
            writable: Vec::new(),
            hidden: Vec::new(),
            read_only: vec![dir.clone()],
            network: Network::On,
            tmp: dir.clone(),
        };
        let carved = mount_carve(&profile);
        let refused = carved.is_ok() && std::fs::write(dir.join("x"), "x").is_err();
        let _ = std::fs::remove_dir_all(&dir);
        carved.and_then(|()| {
            refused
                .then_some(())
                .ok_or_else(|| "the read-only bind did not hold".to_owned())
        })
    }

    /// Readable everywhere except the hidden trees: rules for every path
    /// that contains no hidden tree, descending into those that do.
    fn readable(dir: &Path, hidden: &[PathBuf], out: &mut Vec<PathBuf>) {
        if hidden.iter().any(|h| h == dir) {
            return;
        }
        if !hidden.iter().any(|h| h.starts_with(dir)) {
            out.push(dir.to_path_buf());
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                readable(&e.path(), hidden, out);
            }
        }
    }

    pub(super) fn enter(profile: &Profile, argv: &[String]) -> String {
        match mount_carve(profile).and_then(|()| confine(profile)) {
            Ok(()) => {
                use std::os::unix::process::CommandExt as _;
                let e = std::process::Command::new(&argv[0]).args(&argv[1..]).exec();
                format!("could not start `{}`: {e}", argv[0])
            }
            Err(e) => format!("the sandbox could not be applied: {e}"),
        }
    }

    fn confine(profile: &Profile) -> Result<(), String> {
        let read = AccessFs::from_read(ABI_LEVEL);
        let all = AccessFs::from_all(ABI_LEVEL);
        let mut readable_paths = Vec::new();
        readable(Path::new("/"), &profile.hidden, &mut readable_paths);
        let mut ruleset = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(all)
            .and_then(|r| r.create())
            .map_err(|e| e.to_string())?;
        for p in &readable_paths {
            if let Ok(fd) = PathFd::new(p) {
                let access = if p.is_dir() {
                    read
                } else {
                    read & AccessFs::from_file(ABI_LEVEL)
                };
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, access))
                    .map_err(|e| e.to_string())?;
            }
        }
        for w in profile.writable.iter().chain([&profile.tmp]) {
            if let Ok(fd) = PathFd::new(w) {
                let access = if w.is_dir() {
                    all
                } else {
                    AccessFs::from_file(ABI_LEVEL)
                };
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, access))
                    .map_err(|e| e.to_string())?;
            }
        }
        for dev in ["/dev/null", "/dev/zero", "/dev/tty", "/dev/ptmx"] {
            if let Ok(fd) = PathFd::new(dev) {
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, AccessFs::from_file(ABI_LEVEL)))
                    .map_err(|e| e.to_string())?;
            }
        }
        let status = ruleset.restrict_self().map_err(|e| e.to_string())?;
        if status.ruleset != RulesetStatus::FullyEnforced {
            return Err(format!("Landlock only {:?}", status.ruleset));
        }
        if profile.network != Network::On {
            no_inet()?;
        }
        Ok(())
    }

    /// `socket(AF_INET | AF_INET6, …)` fails with EACCES.
    fn no_inet() -> Result<(), String> {
        let rule = |domain: u64| {
            SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, domain)
                .and_then(|c| SeccompRule::new(vec![c]))
                .map_err(|e| e.to_string())
        };
        let rules = vec![
            rule(u64::from(libc::AF_INET.unsigned_abs()))?,
            rule(u64::from(libc::AF_INET6.unsigned_abs()))?,
        ];
        let arch: TargetArch = std::env::consts::ARCH
            .try_into()
            .map_err(|e: seccompiler::BackendError| e.to_string())?;
        let filter = SeccompFilter::new(
            BTreeMap::from([(libc::SYS_socket, rules)]),
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EACCES.unsigned_abs()),
            arch,
        )
        .map_err(|e| e.to_string())?;
        let program: seccompiler::BpfProgram = filter
            .try_into()
            .map_err(|e: seccompiler::BackendError| e.to_string())?;
        seccompiler::apply_filter(&program).map_err(|e| e.to_string())
    }
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
        let sb = seatbelt(&p);
        assert!(sb.contains("(deny network*)"));
        assert!(sb.contains("(subpath \"/home/u/.cargo/registry\")"));
        assert!(!sb.contains("(local ip \"localhost:*\"))\n(allow network*"));
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

    #[test]
    fn network_allow_is_exact_and_a_prefix_needs_two_elements() {
        let c = parse("[sandbox]\nnetwork_allow = [[\"cargo\", \"fetch\"]]\nnetwork_allow_prefix = [[\"npm\", \"ci\"]]\n").unwrap();
        let s = Sandboxed {
            profile: Profile::new(Path::new("/w"), &c, None),
            backend: Backend::Seatbelt,
            network_allow: c.network_allow.clone(),
            network_allow_prefix: c.network_allow_prefix.clone(),
            env: Vec::new(),
        };
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(s.network_for(&argv("cargo fetch")), Network::On);
        assert_ne!(s.network_for(&argv("cargo fetch --x")), Network::On);
        assert_eq!(s.network_for(&argv("npm ci --silent")), Network::On);
        assert!(parse("[sandbox]\nnetwork_allow_prefix = [[\"npm\"]]\n").is_err());
    }
}
