//! The policy classifier: every tool call is
//! allowed, needs approval, or is denied — decided by code, before the call.
//!
//! It decides *which* calls run; what an allowed command can reach is the
//! OS sandbox's job (`crate::sandbox`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::paths::{Protected, normalise};

/// The prefix of an MCP tool's name as the model sees it (`crate::mcp`).
pub const MCP_PREFIX: &str = "mcp__";

/// What the engine does with a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Run it.
    Allow,
    /// Ask a person first.
    Approval(String),
    /// Never run it.
    Deny(String),
}

/// The kind of object a decision covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// A write or edit of one file.
    Write,
    /// One exact command.
    Command,
    /// One exact command with the network.
    Network,
    /// One MCP tool as defined now.
    Mcp,
}

impl Class {
    /// Its name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Command => "command",
            Self::Network => "network",
            Self::Mcp => "mcp",
        }
    }
}

/// What a person's decision covers: exactly one resolved object — never
/// the request's spelling and never a pattern (constitution V). A path is
/// the canonical file a write reaches; a command is its exact argv with
/// the directory it runs in and the executable it resolves to; an MCP tool
/// is its name with the digest of its definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Object {
    /// `write_file` / `edit_file` of this file.
    Path {
        /// Workspace-relative, every existing ancestor's symlinks resolved.
        canonical: String,
        /// The path as the model sent it (normalised).
        asked: String,
    },
    /// `run_command` with exactly this argv.
    Command {
        /// The argv.
        argv: Vec<String>,
        /// Hex SHA-256 over the argv as a JSON array.
        argv_sha256: String,
        /// The working directory, workspace-relative (`.` for the root).
        cwd: String,
        /// The executable's absolute canonical path, or `not found`.
        executable: String,
        /// Whether the call has the network.
        network: bool,
    },
    /// This MCP tool, as its server defines it now.
    Tool {
        /// `mcp__<server>__<tool>`.
        name: String,
        /// agentplane's digest over the tool's definition.
        digest: String,
    },
}

/// What an executable that cannot be found is recorded as.
pub const NOT_FOUND: &str = "not found";

impl PartialEq for Object {
    /// A path is the file it reaches (any spelling); a command and a tool
    /// are every field.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path { canonical: a, .. }, Self::Path { canonical: b, .. }) => a == b,
            (
                Self::Command {
                    argv: a1,
                    argv_sha256: a2,
                    cwd: a3,
                    executable: a4,
                    network: a5,
                },
                Self::Command {
                    argv: b1,
                    argv_sha256: b2,
                    cwd: b3,
                    executable: b4,
                    network: b5,
                },
            ) => a1 == b1 && a2 == b2 && a3 == b3 && a4 == b4 && a5 == b5,
            (Self::Tool { name: a, digest: x }, Self::Tool { name: b, digest: y }) => {
                a == b && x == y
            }
            _ => false,
        }
    }
}

impl Eq for Object {}

/// Hex SHA-256 over `argv` as a JSON array.
#[must_use]
pub fn argv_sha256(argv: &[String]) -> String {
    use sha2::{Digest as _, Sha256};
    let json = serde_json::to_string(argv).unwrap_or_default();
    Sha256::digest(json.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn first12(hex: &str) -> &str {
    hex.get(..12).unwrap_or(hex)
}

impl Object {
    /// A command object; the argv's hash is computed.
    #[must_use]
    pub fn command(argv: Vec<String>, cwd: String, executable: String, network: bool) -> Self {
        Self::Command {
            argv_sha256: argv_sha256(&argv),
            argv,
            cwd,
            executable,
            network,
        }
    }

    /// Its class.
    #[must_use]
    pub const fn class(&self) -> Class {
        match self {
            Self::Path { .. } => Class::Write,
            Self::Command { network: true, .. } => Class::Network,
            Self::Command { .. } => Class::Command,
            Self::Tool { .. } => Class::Mcp,
        }
    }

    /// One line for people.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Path { canonical, .. } => format!("write `{canonical}`"),
            Self::Command {
                argv, cwd, network, ..
            } => {
                let place = if cwd == "." {
                    String::new()
                } else {
                    format!(" in `{cwd}`")
                };
                let net = if *network { " with the network" } else { "" };
                format!("run `{}`{place}{net}", argv.join(" "))
            }
            Self::Tool { name, digest } => format!("use `{name}`@{}", first12(digest)),
        }
    }

    /// Its grant id (F-125): `g-<16 hex>` over what the object is — the
    /// canonical path, every field of a command, a tool's name and digest —
    /// so one id names one object and nothing else.
    #[must_use]
    pub fn grant_id(&self) -> String {
        use sha2::{Digest as _, Sha256};
        let identity = match self {
            Self::Path { canonical, .. } => serde_json::json!({ "path": canonical }),
            Self::Command {
                argv,
                argv_sha256,
                cwd,
                executable,
                network,
            } => serde_json::json!({
                "argv": argv, "argv_sha256": argv_sha256, "cwd": cwd,
                "executable": executable, "network": network,
            }),
            Self::Tool { name, digest } => serde_json::json!({ "tool": name, "digest": digest }),
        };
        let hex: String = Sha256::digest(identity.to_string().as_bytes())
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("g-{hex}")
    }

    /// The short form events and lists carry: the path, `argv#<12 hex>`,
    /// `<tool>@<12 hex>` — never more of the argv than the target shows.
    #[must_use]
    pub fn short(&self) -> String {
        match self {
            Self::Path { canonical, .. } => canonical.clone(),
            Self::Command { argv_sha256, .. } => format!("argv#{}", first12(argv_sha256)),
            Self::Tool { name, digest } => format!("{name}@{}", first12(digest)),
        }
    }

    /// Whether `other` names the same thing as this one by the request's
    /// spelling (the path as asked, the argv, the tool's name) — the case
    /// in which a different object is worth telling the person about.
    #[must_use]
    pub fn same_name(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path { asked: a, .. }, Self::Path { asked: b, .. }) => a == b,
            (
                Self::Command {
                    argv: a,
                    network: x,
                    ..
                },
                Self::Command {
                    argv: b,
                    network: y,
                    ..
                },
            ) => a == b && x == y,
            (Self::Tool { name: a, .. }, Self::Tool { name: b, .. }) => a == b,
            _ => false,
        }
    }

    /// What differs from `earlier`, an object of the same name a person
    /// approved: `executable /usr/bin/make → /w/bin/make`.
    #[must_use]
    pub fn changed_from(&self, earlier: &Self) -> Option<String> {
        if self == earlier || !self.same_name(earlier) {
            return None;
        }
        let mut what = Vec::new();
        match (earlier, self) {
            (Self::Path { canonical: a, .. }, Self::Path { canonical: b, .. }) => {
                what.push(format!("resolves to {b}, was {a}"));
            }
            (
                Self::Command {
                    cwd: c1,
                    executable: e1,
                    ..
                },
                Self::Command {
                    cwd: c2,
                    executable: e2,
                    ..
                },
            ) => {
                if e1 != e2 {
                    what.push(format!("executable {e1} → {e2}"));
                }
                if c1 != c2 {
                    what.push(format!("directory {c1} → {c2}"));
                }
            }
            (Self::Tool { digest: a, .. }, Self::Tool { digest: b, .. }) => {
                what.push(format!(
                    "definition @{} → @{} (the tool's definition changed since it was approved)",
                    first12(a),
                    first12(b)
                ));
            }
            _ => {}
        }
        (!what.is_empty()).then(|| what.join("; "))
    }

    /// The MCP tool's name, for a tool object.
    #[must_use]
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            Self::Tool { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The canonical path, for a path object.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Path { canonical, .. } => Some(canonical),
            _ => None,
        }
    }
}

/// A person's "no", with the reason the model is told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejection {
    /// What was rejected.
    pub object: Object,
    /// Why.
    pub reason: String,
}

/// The classifier for one project.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    /// Check-derived prefixes: each configured check's program and
    /// subcommands up to its first option (committed configuration).
    checks: Vec<Vec<String>>,
    /// `[commands] allow`: exact argvs.
    exact: std::collections::BTreeSet<Vec<String>>,
    /// `[commands] allow_prefix`: prefixes of at least two elements.
    prefix: Vec<Vec<String>>,
    protected: Protected,
    frozen: std::collections::BTreeSet<String>,
    approved: Vec<Object>,
    rejected: Vec<Rejection>,
    /// MCP tools by model-facing name: `allow`, `approval` or `deny`.
    mcp: std::collections::BTreeMap<String, String>,
    /// The planning profile for hosted planning skills.
    authoring: bool,
    /// `[sandbox] network_allow`: exact argvs that get the network.
    network_exact: std::collections::BTreeSet<Vec<String>>,
    /// `[sandbox] network_allow_prefix`: prefixes that get it.
    network_prefix: Vec<Vec<String>>,
    /// An in-place attempt: git commands that move the person's checkout
    /// are denied.
    in_place: bool,
    /// In place: what git ignored at the run's start (directories end in
    /// `/`), invisible to the file tools unless protected.
    hidden: Vec<String>,
    /// An autopilot run (spec 093): a call that would ask a person, and that
    /// no grant covers, is refused for the run instead.
    autopilot: bool,
}

/// Why an autopilot run refused a call that would have asked a person.
pub const AUTOPILOT_DENIED: &str = "autopilot: not asked — listed for you";

/// Whether `argv` starts with `prefix`, a prefix of at least two elements
/// (a bare program allows any argument, so it is never a prefix).
#[must_use]
pub fn prefix_matches(prefix: &[String], argv: &[String]) -> bool {
    prefix.len() >= 2 && argv.starts_with(prefix)
}

/// Why a prefix entry is refused, if it is: a bare program allows any
/// argument.
#[must_use]
pub fn prefix_refusal(prefix: &[String]) -> Option<&'static str> {
    (prefix.len() < 2).then_some(
        "a bare program allows any argument; add the exact command to [commands] allow instead",
    )
}

const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "pwsh",
];
const PRIVILEGE: &[&str] = &["sudo", "su", "doas", "pkexec"];
const NETWORK: &[&str] = &[
    "curl", "wget", "nc", "ncat", "ssh", "scp", "sftp", "rsync", "ftp", "telnet",
];
const GIT_REMOTE: &[&str] = &[
    "push",
    "fetch",
    "pull",
    "clone",
    "remote",
    "ls-remote",
    "submodule",
];
/// What would move the checkout of an in-place run under speccoder.
const GIT_IN_PLACE: &[&str] = &[
    "checkout",
    "switch",
    "reset",
    "commit",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "stash",
    "branch",
    "tag",
    "update-ref",
    "worktree",
    "am",
];

impl Policy {
    /// A policy allowing the command prefixes of `checks` (program and
    /// subcommands, up to the first option) and exactly the argvs of
    /// `exact` (`[commands] allow`).
    #[must_use]
    pub fn new(checks: &[Vec<String>], exact: &[Vec<String>]) -> Self {
        let mut allowed: Vec<Vec<String>> = checks
            .iter()
            .map(|argv| {
                let prefix: Vec<String> = argv
                    .iter()
                    .take_while(|a| !a.starts_with('-'))
                    .cloned()
                    .collect();
                // `python3 -m pytest …` would leave the bare interpreter as
                // the prefix — and allow `python3 -c <anything>`. A check
                // whose prefix is only its program allows exactly itself.
                if prefix.len() == 1 && argv.len() > 1 {
                    argv.clone()
                } else {
                    prefix
                }
            })
            .filter(|p: &Vec<String>| !p.is_empty())
            .collect();
        allowed.sort();
        allowed.dedup();
        Self {
            checks: allowed,
            exact: exact.iter().filter(|p| !p.is_empty()).cloned().collect(),
            ..Self::default()
        }
    }

    /// `[commands] allow_prefix`: prefixes of two elements or more (a
    /// shorter one is dropped; the configuration refuses it).
    #[must_use]
    pub fn with_prefixes(mut self, prefixes: Vec<Vec<String>>) -> Self {
        self.prefix = prefixes.into_iter().filter(|p| p.len() >= 2).collect();
        self
    }

    /// The check-derived prefixes, for a coverage view.
    #[must_use]
    pub fn check_prefixes(&self) -> &[Vec<String>] {
        &self.checks
    }

    /// Paths git ignored when an in-place run started: the file tools
    /// refuse them, except the protected files (the intent stays readable).
    #[must_use]
    pub fn with_hidden(mut self, hidden: impl IntoIterator<Item = String>) -> Self {
        self.hidden = hidden.into_iter().collect();
        self
    }

    /// Whether `rel` is ignored by git in an in-place checkout and not one
    /// of the files speccoder protects.
    fn is_hidden(&self, rel: &str) -> bool {
        self.protected.write_class(rel).is_none()
            && !self.protected.is_context(rel)
            && self.hidden.iter().any(|h| match h.strip_suffix('/') {
                Some(dir) => rel == dir || rel.starts_with(h.as_str()),
                None => rel == h,
            })
    }

    /// An in-place attempt (`run --in-place`): the checkout is the
    /// person's, so git commands that move it are denied.
    #[must_use]
    pub fn in_place(mut self) -> Self {
        self.in_place = true;
        self
    }

    /// An autopilot run: nothing asks ([`Policy::settle`]).
    #[must_use]
    pub const fn autopilot(mut self) -> Self {
        self.autopilot = true;
        self
    }

    /// Whether it is an autopilot run's.
    #[must_use]
    pub const fn is_autopilot(&self) -> bool {
        self.autopilot
    }

    /// What a decision comes to once every grant had its say: on
    /// autopilot a call that would still ask a person is refused for the
    /// run ([`AUTOPILOT_DENIED`]); every other decision stands — a denial,
    /// a rejection or a protected path is never lifted.
    #[must_use]
    pub fn settle(&self, decision: Decision) -> Decision {
        match decision {
            Decision::Approval(_) if self.autopilot => Decision::Deny(AUTOPILOT_DENIED.to_owned()),
            d => d,
        }
    }

    /// The planning profile: the format's artifacts and scripts are the
    /// work; its machinery, verification inputs and secrets stay denied;
    /// anything else — source code included — needs approval.
    #[must_use]
    pub fn authoring(mut self) -> Self {
        self.authoring = true;
        self
    }

    /// The project's format's rules (intent, planning artifacts); the
    /// reference format's by default.
    #[must_use]
    pub fn rules(mut self, rules: &'static crate::format::Rules) -> Self {
        self.protected = Protected::new(rules);
        self
    }

    /// `[sandbox] network_allow`: exact argvs that get the network.
    #[must_use]
    pub fn with_network(mut self, exact: Vec<Vec<String>>) -> Self {
        self.network_exact = exact.into_iter().filter(|p| !p.is_empty()).collect();
        self
    }

    /// `[sandbox] network_allow_prefix`: prefixes (two elements or more)
    /// that get the network.
    #[must_use]
    pub fn with_network_prefixes(mut self, prefixes: Vec<Vec<String>>) -> Self {
        self.network_prefix = prefixes.into_iter().filter(|p| p.len() >= 2).collect();
        self
    }

    /// Whether `argv` gets the network without asking: an exact entry, or a
    /// prefix entry it extends.
    #[must_use]
    pub fn network_allowed(&self, argv: &[String]) -> bool {
        self.network_exact.contains(argv)
            || self.network_prefix.iter().any(|p| prefix_matches(p, argv))
    }

    /// The project's MCP tools and their configured decisions.
    #[must_use]
    pub fn with_mcp(mut self, tools: impl IntoIterator<Item = (String, String)>) -> Self {
        self.mcp = tools.into_iter().collect();
        self
    }

    /// A person's decisions for this task: an approval turns a call that
    /// needs approval into an allowed one (never a denied one); a rejection
    /// turns it into a denial with the person's reason.
    #[must_use]
    pub fn with_decisions(mut self, approved: Vec<Object>, rejected: Vec<Rejection>) -> Self {
        self.approved = approved;
        self.rejected = rejected;
        self
    }

    /// Freezes these test files: writing them needs approval.
    #[must_use]
    pub fn with_frozen(mut self, frozen: impl IntoIterator<Item = String>) -> Self {
        self.frozen = frozen.into_iter().collect();
        self
    }

    /// The protected set this policy uses.
    #[must_use]
    pub fn protected(&self) -> &Protected {
        &self.protected
    }

    /// Whether this (resolved, workspace-relative) path is protected for
    /// writing under this policy: the authoring profile writes the format's
    /// artifacts, nothing else of the protected set.
    #[must_use]
    pub fn write_protected(&self, rel: &str) -> Option<super::paths::Class> {
        if self.authoring
            && (self.protected.rules().authoring_path)(rel)
            && !self.protected.is_secret(rel)
            && !self.protected.is_review(rel)
        {
            return None;
        }
        self.protected.write_class(rel)
    }

    /// Classifies `tool` called with `args`. A person's decision applies
    /// only to the call's resolved `object` (none: the request alone was
    /// seen, and nobody's decision covers it): a rejection of that very
    /// object denies it, an approval of it allows it, never a denial.
    #[must_use]
    pub fn classify(&self, tool: &str, args: &Value, object: Option<&Object>) -> Decision {
        let decision = self.base(tool, args);
        if !matches!(decision, Decision::Approval(_)) {
            return decision;
        }
        let Some(object) = object else {
            return decision;
        };
        if let Some(r) = self.rejected.iter().find(|r| r.object == *object) {
            return Decision::Deny(format!("a person rejected this: {}", r.reason));
        }
        if self.approved.iter().any(|a| a == object) {
            return Decision::Allow;
        }
        decision
    }

    /// The person's rejections and approvals, for the changed-object line.
    #[must_use]
    pub fn approved(&self) -> &[Object] {
        &self.approved
    }

    /// The rule that decides `tool` with `args` when no person does: what a
    /// call's record names, so a rule's decision is never read as a
    /// person's.
    #[must_use]
    pub fn rule(&self, tool: &str, args: &Value, object: Option<&Object>) -> String {
        if self.autopilot && matches!(self.classify(tool, args, object), Decision::Approval(_)) {
            return "autopilot".to_owned();
        }
        if let Some(object) = object
            && matches!(self.base(tool, args), Decision::Approval(_))
        {
            if self.rejected.iter().any(|r| r.object == *object) {
                return format!("rejection of {} for the task", object.describe());
            }
            if self.approved.iter().any(|a| a == object) {
                return format!("grant {} for the task", object.describe());
            }
        }
        if tool.starts_with(MCP_PREFIX) {
            let entry = self.mcp.get(tool).map_or("unlisted", String::as_str);
            return format!("[mcp] {entry}");
        }
        match tool {
            "read_file" | "list_files" | "search_code" => "workspace read".to_owned(),
            "write_file" | "edit_file" => "workspace write".to_owned(),
            "git_status" | "git_diff" => "read-only git".to_owned(),
            "run_command" => {
                let argv = argv_of(args);
                if self.exact.contains(&argv) {
                    return format!("exact command `{}`", argv.join(" "));
                }
                if let Some(p) = self
                    .checks
                    .iter()
                    .find(|p| argv.len() >= p.len() && argv[..p.len()] == p[..])
                {
                    return format!("check prefix `{}`", p.join(" "));
                }
                match self.prefix.iter().find(|p| prefix_matches(p, &argv)) {
                    Some(p) => format!("allowed prefix `{}`", p.join(" ")),
                    None => "command policy".to_owned(),
                }
            }
            _ => "tool policy".to_owned(),
        }
    }

    /// The decision by rule alone, before any person's: what decides
    /// whether a call's object is resolved and a person asked.
    #[must_use]
    pub fn base(&self, tool: &str, args: &Value) -> Decision {
        self.base_of(tool, args)
    }

    fn base_of(&self, tool: &str, args: &Value) -> Decision {
        if tool.starts_with(MCP_PREFIX) {
            return match self.mcp.get(tool).map(String::as_str) {
                Some("allow") => Decision::Allow,
                Some("deny") => Decision::Deny(format!("`{tool}` is denied in [mcp]")),
                Some(_) => Decision::Approval(format!(
                    "`{tool}` is an MCP tool; speccoder cannot know what it does"
                )),
                None => Decision::Deny(format!("unknown tool `{tool}`")),
            };
        }
        match tool {
            "read_file" | "list_files" | "search_code" => self.read(args),
            "write_file" | "edit_file" => self.write(args),
            "run_command" => self.with_network_rule(args, self.command(args)),
            "git_status" | "git_diff" => Decision::Allow,
            other => Decision::Deny(format!("unknown tool `{other}`")),
        }
    }

    /// A command that asks for the network (`"network": true`) needs a
    /// person, unless its prefix is in `[sandbox] network_allow`.
    fn with_network_rule(&self, args: &Value, decision: Decision) -> Decision {
        if args.get("network").and_then(Value::as_bool) != Some(true)
            || matches!(decision, Decision::Deny(_))
        {
            return decision;
        }
        let argv = argv_of(args);
        if self.network_allowed(&argv) {
            return decision;
        }
        Decision::Approval(format!(
            "`{}` asks for the network; commands run without it",
            argv.join(" ")
        ))
    }

    fn read(&self, args: &Value) -> Decision {
        // No path, `.`, `./` or "": the whole workspace (models pass all of them).
        let Some(path) = args
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !matches!(p.trim(), "" | "." | "./"))
        else {
            return Decision::Allow; // list/search over the whole workspace
        };
        match normalise(path) {
            Err(e) => Decision::Deny(e.0),
            Ok(p) if self.protected.is_secret(&p) => {
                Decision::Deny(format!("`{p}` looks like a credential"))
            }
            Ok(p) if self.is_hidden(&p) => Decision::Deny(format!(
                "`{p}` is ignored by git in this checkout; an in-place run works only with what a worktree would hold"
            )),
            Ok(_) => Decision::Allow,
        }
    }

    fn write(&self, args: &Value) -> Decision {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return Decision::Deny("no `path`".to_owned());
        };
        let p = match normalise(path) {
            Ok(p) => p,
            Err(e) => return Decision::Deny(e.0),
        };
        if self.authoring
            && (self.protected.rules().authoring_path)(&p)
            && !self.protected.is_secret(&p)
            && !self.protected.is_review(&p)
        {
            return Decision::Allow;
        }
        if self.is_hidden(&p) {
            return Decision::Deny(format!(
                "`{p}` is ignored by git in this checkout; an in-place run does not write it"
            ));
        }
        if self.protected.is_review(&p) {
            return Decision::Deny(format!(
                "`{p}` is protected: it records a person's review of the spec; only the person writes it (`speccoder review`)"
            ));
        }
        if let Some(class) = self.protected.write_class(&p) {
            return Decision::Deny(format!(
                "`{p}` is protected: {}",
                self.protected.reason(class)
            ));
        }
        if self.authoring {
            return Decision::Approval((self.protected.rules().authoring_refusal)(&p));
        }
        if self.protected.is_context(&p) {
            return Decision::Approval(format!(
                "`{p}` is an agent context file: every later agent session reads it"
            ));
        }
        if self
            .frozen
            .iter()
            .any(|f| f.to_lowercase() == p.to_lowercase())
        {
            return Decision::Approval(format!(
                "`{p}` is a frozen test (it existed when the attempt started)"
            ));
        }
        if self.protected.is_manifest(&p) {
            return Decision::Approval(format!("`{p}` is a dependency manifest"));
        }
        Decision::Allow
    }

    fn command(&self, args: &Value) -> Decision {
        let argv = argv_of(args);
        match (self.protected.rules().command)(&argv, self.authoring) {
            Some(Ok(())) => return Decision::Allow,
            Some(Err(why)) => return Decision::Deny(why),
            None => {}
        }
        let Some(program) = argv.first() else {
            return Decision::Deny("empty `argv`".to_owned());
        };
        let name = program.rsplit('/').next().unwrap_or(program);
        let has = |flag: &str| argv.iter().any(|a| a == flag);

        if PRIVILEGE.contains(&name) {
            return Decision::Deny(format!("`{name}` raises privileges"));
        }
        if name == "git" {
            let sub = argv.get(1).map_or("", String::as_str);
            if GIT_REMOTE.contains(&sub) {
                return Decision::Deny(format!(
                    "`git {sub}` talks to a remote; speccoder never does"
                ));
            }
            // Attestations are git notes only speccoder writes.
            let notes_action = argv[2..]
                .iter()
                .find(|a| !a.starts_with('-'))
                .map_or("list", String::as_str);
            if (sub == "notes" && !matches!(notes_action, "show" | "list"))
                || (sub == "update-ref" && argv.iter().any(|a| a.starts_with("refs/notes")))
            {
                return Decision::Deny(
                    "git notes are speccoder's attestations; only speccoder writes them".to_owned(),
                );
            }
            // Read-only queries (skills and models ask where they are).
            let listing_branches = sub == "branch"
                && argv[2..].iter().all(|a| {
                    matches!(
                        a.as_str(),
                        "--show-current" | "--list" | "-l" | "-a" | "--all" | "-v" | "-vv"
                    )
                });
            if matches!(
                sub,
                "rev-parse" | "status" | "log" | "diff" | "show" | "ls-files" | "describe"
            ) || listing_branches
            {
                return Decision::Allow;
            }
            if self.in_place
                && !listing_branches
                && (GIT_IN_PLACE.contains(&sub)
                    || (sub == "apply" && has("--index"))
                    || (sub == "restore" && has("--staged")))
            {
                return Decision::Deny(format!(
                    "`git {sub}` would move the checkout of an in-place run; speccoder commits and switches it"
                ));
            }
            if sub == "clean" || (sub == "reset" && has("--hard")) || sub == "filter-branch" {
                return Decision::Deny(format!("`git {sub}` destroys work"));
            }
            if matches!(
                sub,
                "commit"
                    | "checkout"
                    | "switch"
                    | "rebase"
                    | "merge"
                    | "stash"
                    | "branch"
                    | "tag"
                    | "config"
                    | "worktree"
            ) {
                return Decision::Approval(format!("`git {sub}` changes repository state"));
            }
        }
        if SHELLS.contains(&name) || name == "eval" || name == "env" || name == "xargs" {
            return Decision::Approval(format!("`{name}` runs arbitrary commands"));
        }
        if NETWORK.contains(&name) {
            return Decision::Approval(format!("`{name}` uses the network"));
        }
        if name == "rm" && argv.iter().any(|a| a.starts_with('-') && a.contains('r')) {
            return Decision::Approval("recursive `rm`".to_owned());
        }
        // `cargo --version`, `rustc -V`, `python3 --version`: harmless probes.
        if argv.len() == 2 && matches!(argv[1].as_str(), "--version" | "-V" | "version") {
            return Decision::Allow;
        }
        // `[commands] allow` is exact; a check allows its prefix; a
        // prefix entry (two elements or more) what extends it.
        if self.exact.contains(&argv)
            || self
                .checks
                .iter()
                .any(|prefix| argv.len() >= prefix.len() && argv[..prefix.len()] == prefix[..])
            || self.prefix.iter().any(|p| prefix_matches(p, &argv))
        {
            return Decision::Allow;
        }
        Decision::Approval("not a configured check or allowed command".to_owned())
    }
}

/// The argv a `run_command` call asks for (strings only).
#[must_use]
pub fn argv_of(args: &Value) -> Vec<String> {
    args.get("argv")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy() -> Policy {
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        Policy::new(
            &[
                argv("cargo nextest run --workspace --profile speccoder"),
                argv("cargo clippy --workspace -- -D warnings"),
            ],
            &[],
        )
        .with_prefixes(vec![argv("cargo fmt"), argv("cargo check")])
    }

    fn path(canonical: &str, asked: &str) -> Object {
        Object::Path {
            canonical: canonical.to_owned(),
            asked: asked.to_owned(),
        }
    }

    fn command(s: &str) -> Object {
        Object::command(
            s.split(' ').map(str::to_owned).collect(),
            ".".to_owned(),
            "/usr/bin/x".to_owned(),
            false,
        )
    }

    fn cmd(s: &str) -> Value {
        json!({ "argv": s.split(' ').collect::<Vec<_>>() })
    }

    #[test]
    fn classifies_the_table() {
        use Decision::{Allow, Approval, Deny};
        let p = policy();
        let kind = |d: Decision| match d {
            Allow => "allow",
            Approval(_) => "approval",
            Deny(_) => "deny",
        };
        let cases: &[(&str, Value, &str)] = &[
            ("read_file", json!({"path": "src/lib.rs"}), "allow"),
            ("read_file", json!({"path": ".env"}), "deny"),
            ("read_file", json!({"path": "../secret"}), "deny"),
            ("list_files", json!({}), "allow"),
            ("search_code", json!({"pattern": "fn main"}), "allow"),
            ("write_file", json!({"path": "src/new.rs"}), "allow"),
            ("edit_file", json!({"path": "specs/001-x/tasks.md"}), "deny"),
            ("write_file", json!({"path": "speccoder.toml"}), "deny"),
            (
                "write_file",
                json!({"path": ".github/workflows/ci.yml"}),
                "deny",
            ),
            (
                "write_file",
                json!({"path": ".git/hooks/pre-commit"}),
                "deny",
            ),
            ("edit_file", json!({"path": "Cargo.toml"}), "approval"),
            ("edit_file", json!({"path": "web/package.json"}), "approval"),
            ("run_command", cmd("cargo nextest run -p x"), "allow"),
            ("run_command", cmd("cargo clippy --all-targets"), "allow"),
            ("run_command", cmd("cargo fmt --all"), "allow"),
            ("run_command", cmd("cargo build"), "approval"),
            ("run_command", cmd("cargo --version"), "allow"),
            ("run_command", cmd("rustc -V"), "allow"),
            ("run_command", cmd("sudo --version"), "deny"),
            ("run_command", cmd("npm install left-pad"), "approval"),
            ("run_command", cmd("git push origin main"), "deny"),
            ("run_command", cmd("git fetch"), "deny"),
            ("run_command", cmd("git reset --hard HEAD"), "deny"),
            ("run_command", cmd("git clean -fdx"), "deny"),
            ("run_command", cmd("git commit -m x"), "approval"),
            (
                "run_command",
                cmd("git rev-parse --is-inside-work-tree"),
                "allow",
            ),
            ("run_command", cmd("git branch --show-current"), "allow"),
            ("run_command", cmd("git branch feature-x"), "approval"),
            ("run_command", cmd("git log --oneline -3"), "allow"),
            ("run_command", cmd("sh -c ls"), "approval"),
            ("run_command", cmd("sudo cargo test"), "deny"),
            ("run_command", cmd("curl https://example.com"), "approval"),
            ("run_command", cmd("rm -rf target"), "approval"),
            ("run_command", json!({"argv": []}), "deny"),
            ("git_status", json!({}), "allow"),
            ("git_diff", json!({}), "allow"),
            ("delete_everything", json!({}), "deny"),
        ];
        for (tool, args, want) in cases {
            assert_eq!(kind(p.classify(tool, args, None)), *want, "{tool} {args}");
        }
    }

    #[test]
    fn decisions_cover_classes_and_never_override_a_denial() {
        let approved = vec![
            path("Cargo.toml", "Cargo.toml"),
            command("cargo build"),
            path("speccoder.toml", "speccoder.toml"),
            // Approved and rejected alike: the rejection wins.
            command("curl https://x"),
        ];
        let rejected = vec![Rejection {
            object: command("curl https://x"),
            reason: "no network".to_owned(),
        }];
        let p = policy().with_decisions(approved, rejected);
        // Any content for an approved file, whatever spelling reaches it.
        assert_eq!(
            p.classify(
                "edit_file",
                &json!({"path": "./Cargo.toml", "old_string": "a"}),
                Some(&path("Cargo.toml", "./Cargo.toml"))
            ),
            Decision::Allow
        );
        // The object, not the request: no object, no grant.
        assert!(matches!(
            p.classify("edit_file", &json!({"path": "Cargo.toml"}), None),
            Decision::Approval(_)
        ));
        assert_eq!(
            p.classify(
                "run_command",
                &cmd("cargo build"),
                Some(&command("cargo build"))
            ),
            Decision::Allow
        );
        assert!(matches!(
            p.classify(
                "run_command",
                &cmd("cargo build --release"),
                Some(&command("cargo build --release"))
            ),
            Decision::Approval(_)
        ));
        // A rejection is a denial with the person's reason, approved or not.
        assert_eq!(
            p.classify(
                "run_command",
                &cmd("curl https://x"),
                Some(&command("curl https://x"))
            ),
            Decision::Deny("a person rejected this: no network".to_owned())
        );
        // A protected path stays denied whatever was approved.
        assert!(matches!(
            p.classify("write_file", &json!({"path": "speccoder.toml"}), None),
            Decision::Deny(_)
        ));
        // Attestations are speccoder's: the model reads notes, never writes them.
        let p = Policy::new(&[], &[]);
        for argv in [
            vec![
                "git",
                "notes",
                "--ref=speccoder",
                "add",
                "-f",
                "-m",
                "x",
                "HEAD",
            ],
            vec!["git", "notes", "remove", "HEAD"],
            vec!["git", "update-ref", "refs/notes/speccoder", "HEAD"],
        ] {
            assert!(
                matches!(
                    p.classify("run_command", &json!({ "argv": argv }), None),
                    Decision::Deny(_)
                ),
                "{argv:?}"
            );
        }
        for argv in [
            vec!["git", "notes", "--ref=speccoder", "show", "HEAD"],
            vec!["git", "notes", "list"],
        ] {
            assert!(
                !matches!(
                    p.classify("run_command", &json!({ "argv": argv }), None),
                    Decision::Deny(_)
                ),
                "{argv:?}"
            );
        }
        // In place, what moves the checkout is denied; queries still run.
        let in_place = Policy::new(&[], &[]).in_place();
        for argv in [
            vec!["git", "switch", "main"],
            vec!["git", "commit", "-m", "x"],
            vec!["git", "reset", "HEAD~1"],
            vec!["git", "stash"],
            vec!["git", "branch", "x"],
            vec!["git", "apply", "--index", "p.diff"],
            vec!["git", "restore", "--staged", "a"],
            vec!["git", "update-ref", "HEAD", "x"],
        ] {
            assert!(
                matches!(
                    in_place.classify("run_command", &json!({ "argv": argv }), None),
                    Decision::Deny(_)
                ),
                "{argv:?}"
            );
        }
        for argv in [
            vec!["git", "status"],
            vec!["git", "branch", "--show-current"],
        ] {
            assert_eq!(
                in_place.classify("run_command", &json!({ "argv": argv }), None),
                Decision::Allow,
                "{argv:?}"
            );
        }
        // Ignored files are out of sight; the protected intent is not.
        let hidden = Policy::new(&[], &[]).in_place().with_hidden([
            "target/".to_owned(),
            "notes.txt".to_owned(),
            "specs/".to_owned(),
        ]);
        for (tool, path) in [
            ("read_file", "target/debug/x"),
            ("read_file", "notes.txt"),
            ("list_files", "target"),
            ("write_file", "notes.txt"),
        ] {
            assert!(
                matches!(
                    hidden.classify(tool, &json!({ "path": path }), None),
                    Decision::Deny(_)
                ),
                "{tool} {path}"
            );
        }
        assert_eq!(
            hidden.classify("read_file", &json!({ "path": "specs/001-x/spec.md" }), None),
            Decision::Allow
        );
        assert_eq!(
            hidden.classify("read_file", &json!({ "path": "src/lib.rs" }), None),
            Decision::Allow
        );
        assert!(matches!(
            Policy::new(&[], &[]).classify(
                "run_command",
                &json!({ "argv": ["git", "switch", "main"] }),
                None
            ),
            Decision::Approval(_)
        ));
    }

    #[test]
    fn a_check_whose_prefix_is_its_bare_program_allows_only_itself() {
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let p = Policy::new(
            &[argv("python3 -m pytest -q --junitxml=.speccoder/junit.xml")],
            &[],
        );
        assert_eq!(
            p.classify(
                "run_command",
                &cmd("python3 -m pytest -q --junitxml=.speccoder/junit.xml"),
                None
            ),
            Decision::Allow
        );
        assert!(matches!(
            p.classify("run_command", &cmd("python3 -c print(1)"), None),
            Decision::Approval(_)
        ));
        assert!(matches!(
            p.classify("run_command", &cmd("python3 -m pytest tests/x.py"), None),
            Decision::Approval(_)
        ));
        // `[commands] allow` is exact: the entry allows itself only.
        let q = Policy::new(&[], &[argv("python3 -m pytest")]);
        assert!(matches!(
            q.classify("run_command", &cmd("python3 -m pytest tests/x.py"), None),
            Decision::Approval(_)
        ));
        assert_eq!(
            q.classify("run_command", &cmd("python3 -m pytest"), None),
            Decision::Allow
        );
    }

    #[test]
    fn mcp_tools_follow_their_configured_decision_and_unknown_ones_are_denied() {
        let p = Policy::new(&[], &[]).with_mcp([
            ("mcp__db__query".to_owned(), "allow".to_owned()),
            ("mcp__db__drop".to_owned(), "deny".to_owned()),
            ("mcp__db__insert".to_owned(), "approval".to_owned()),
        ]);
        assert_eq!(
            p.classify("mcp__db__query", &json!({}), None),
            Decision::Allow
        );
        assert!(matches!(
            p.classify("mcp__db__drop", &json!({}), None),
            Decision::Deny(_)
        ));
        assert!(matches!(
            p.classify("mcp__db__insert", &json!({}), None),
            Decision::Approval(_)
        ));
        assert!(matches!(
            p.classify("mcp__db__other", &json!({}), None),
            Decision::Deny(_)
        ));
        // An approval covers the tool as defined then; it never lifts a denial.
        let tool = |name: &str, digest: &str| Object::Tool {
            name: name.to_owned(),
            digest: digest.to_owned(),
        };
        let q = p.clone().with_decisions(
            vec![tool("mcp__db__insert", "d1"), tool("mcp__db__drop", "d1")],
            Vec::new(),
        );
        assert_eq!(
            q.classify(
                "mcp__db__insert",
                &json!({ "row": 1 }),
                Some(&tool("mcp__db__insert", "d1"))
            ),
            Decision::Allow
        );
        // Its server changed the definition: asked again.
        assert!(matches!(
            q.classify(
                "mcp__db__insert",
                &json!({ "row": 1 }),
                Some(&tool("mcp__db__insert", "d2"))
            ),
            Decision::Approval(_)
        ));
        assert!(matches!(
            q.classify(
                "mcp__db__drop",
                &json!({}),
                Some(&tool("mcp__db__drop", "d1"))
            ),
            Decision::Deny(_)
        ));
        assert_eq!(
            tool("mcp__db__insert", "9f8e7d6c5b4a3210").short(),
            "mcp__db__insert@9f8e7d6c5b4a"
        );
        assert!(
            tool("mcp__db__insert", "d2")
                .changed_from(&tool("mcp__db__insert", "d1"))
                .is_some_and(|c| c.contains("definition changed"))
        );
    }

    #[test]
    fn the_authoring_profile_writes_spec_kit_artifacts_and_runs_its_scripts_only() {
        let p = Policy::new(&[], &[]).authoring();
        let w = |path: &str| p.classify("write_file", &json!({ "path": path }), None);
        assert_eq!(w("specs/002-login/spec.md"), Decision::Allow);
        assert_eq!(w(".specify/feature.json"), Decision::Allow);
        assert_eq!(w(".specify/memory/constitution.md"), Decision::Allow);
        assert!(matches!(
            w(".specify/scripts/bash/common.sh"),
            Decision::Deny(_)
        ));
        assert!(matches!(
            w(".specify/templates/spec-template.md"),
            Decision::Deny(_)
        ));
        assert!(matches!(w("speccoder.toml"), Decision::Deny(_)));
        assert!(matches!(w("specs/.env"), Decision::Deny(_)));
        // A planning skill writes the spec, never the person's review of it.
        assert!(
            matches!(w("specs/002-login/review.json"), Decision::Deny(ref why) if why.contains("review")),
            "{:?}",
            w("specs/002-login/review.json")
        );
        assert!(p.write_protected("specs/002-login/review.json").is_some());
        assert!(matches!(w("src/lib.rs"), Decision::Approval(_)));
        let run = |argv: &[&str]| p.classify("run_command", &json!({ "argv": argv }), None);
        assert_eq!(
            run(&[".specify/scripts/bash/setup-plan.sh", "--json"]),
            Decision::Allow
        );
        assert_eq!(
            run(&[
                "bash",
                "./.specify/scripts/bash/check-prerequisites.sh",
                "--json"
            ]),
            Decision::Allow
        );
        assert!(matches!(
            run(&["bash", ".specify/scripts/bash/../../evil.sh"]),
            Decision::Approval(_)
        ));
        assert_eq!(
            run(&[
                "bash",
                ".specify/extensions/git/scripts/bash/create-new-feature-branch.sh",
                "--json"
            ]),
            Decision::Allow
        );
        assert!(matches!(
            run(&["bash", ".specify/extensions/git/other.sh"]),
            Decision::Approval(_)
        ));
        assert!(matches!(
            run(&["bash", "-c", "rm -rf /"]),
            Decision::Approval(_)
        ));
        // Outside the profile, the spec stays protected.
        assert!(matches!(
            Policy::new(&[], &[]).classify(
                "write_file",
                &json!({ "path": "specs/002-login/spec.md" }),
                None
            ),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn asking_for_the_network_needs_a_person_unless_allowed() {
        let checks = [vec!["cargo".to_owned(), "test".to_owned()]];
        let p = Policy::new(&checks, &[]);
        let net = json!({ "argv": ["cargo", "test"], "network": true });
        assert!(matches!(
            p.classify("run_command", &net, None),
            Decision::Approval(_)
        ));
        // Without the network the same command is allowed; a denial stays one.
        assert_eq!(
            p.classify("run_command", &json!({ "argv": ["cargo", "test"] }), None),
            Decision::Allow
        );
        assert!(matches!(
            p.classify(
                "run_command",
                &json!({ "argv": ["git", "push"], "network": true }),
                None
            ),
            Decision::Deny(_)
        ));
        // `[sandbox] network_allow` is exact; a prefix entry extends.
        let exact = p
            .clone()
            .with_network(vec![vec!["cargo".to_owned(), "test".to_owned()]]);
        assert_eq!(exact.classify("run_command", &net, None), Decision::Allow);
        let longer = json!({ "argv": ["cargo", "test", "--release"], "network": true });
        assert!(matches!(
            exact.classify("run_command", &longer, None),
            Decision::Approval(_)
        ));
        let bare = p.clone().with_network(vec![vec!["cargo".to_owned()]]);
        assert!(matches!(
            bare.classify("run_command", &net, None),
            Decision::Approval(_)
        ));
        let prefixed = p.clone().with_network_prefixes(vec![
            vec!["cargo".to_owned()],
            vec!["cargo".to_owned(), "test".to_owned()],
        ]);
        assert_eq!(
            prefixed.classify("run_command", &longer, None),
            Decision::Allow
        );
        let object = Object::command(
            vec!["cargo".to_owned(), "test".to_owned()],
            ".".to_owned(),
            "/usr/bin/cargo".to_owned(),
            true,
        );
        let approved = Policy::new(&checks, &[]).with_decisions(vec![object.clone()], vec![]);
        assert_eq!(
            approved.classify("run_command", &net, Some(&object)),
            Decision::Allow
        );
        // The same argv resolving to another executable is another object.
        let shadowed = Object::command(
            vec!["cargo".to_owned(), "test".to_owned()],
            ".".to_owned(),
            "/w/bin/cargo".to_owned(),
            true,
        );
        assert!(matches!(
            approved.classify("run_command", &net, Some(&shadowed)),
            Decision::Approval(_)
        ));
        assert!(
            shadowed
                .changed_from(&object)
                .is_some_and(|c| c == "executable /usr/bin/cargo → /w/bin/cargo")
        );
    }

    #[test]
    fn commands_allow_is_exact_and_prefixes_need_two_elements() {
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let p = Policy::new(&[], &[argv("npm run build")]);
        assert_eq!(
            p.classify("run_command", &cmd("npm run build"), None),
            Decision::Allow
        );
        for longer in [
            "npm run build --script-shell=/tmp/x",
            "npm run build x",
            "npm run",
        ] {
            assert!(
                matches!(
                    p.classify("run_command", &cmd(longer), None),
                    Decision::Approval(_)
                ),
                "{longer}"
            );
        }
        assert_eq!(
            p.rule("run_command", &cmd("npm run build"), None),
            "exact command `npm run build`"
        );
        let q = Policy::new(&[], &[]).with_prefixes(vec![argv("npm"), argv("npm run")]);
        assert_eq!(
            q.classify("run_command", &cmd("npm run build"), None),
            Decision::Allow
        );
        assert_eq!(
            q.rule("run_command", &cmd("npm run build"), None),
            "allowed prefix `npm run`"
        );
        // The one-element entry was dropped: a bare program allows anything.
        assert!(matches!(
            q.classify("run_command", &cmd("npm install x"), None),
            Decision::Approval(_)
        ));
        assert!(
            prefix_refusal(&argv("npm")).is_some() && prefix_refusal(&argv("npm run")).is_none()
        );
        // Fixed rules decide before every list.
        let r = Policy::new(&[], &[argv("sh -c ls")]).with_prefixes(vec![argv("git push")]);
        assert!(matches!(
            r.classify("run_command", &cmd("sh -c ls"), None),
            Decision::Approval(_)
        ));
        assert!(matches!(
            r.classify("run_command", &cmd("git push origin"), None),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn autopilot_never_asks_never_lifts_a_denial_and_names_its_rule() {
        let argv = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let p = Policy::new(&[], &[argv("cargo test")]).autopilot();
        assert!(p.is_autopilot());
        // Allowed stays allowed; asking becomes refusing; denied stays denied.
        assert_eq!(
            p.settle(p.classify("run_command", &cmd("cargo test"), None)),
            Decision::Allow
        );
        assert_eq!(
            p.settle(p.classify("run_command", &cmd("cargo add x"), None)),
            Decision::Deny(AUTOPILOT_DENIED.to_owned())
        );
        assert_eq!(
            p.rule("run_command", &cmd("cargo add x"), None),
            "autopilot"
        );
        assert!(matches!(
            p.settle(p.classify("run_command", &cmd("git push origin"), None)),
            Decision::Deny(r) if r != AUTOPILOT_DENIED
        ));
        // Without autopilot nothing changes.
        let q = Policy::new(&[], &[]);
        assert!(matches!(
            q.settle(q.classify("run_command", &cmd("cargo add x"), None)),
            Decision::Approval(_)
        ));
        // A grant id names exactly one object.
        let a = command("make check");
        let mut b = a.clone();
        if let Object::Command { cwd, .. } = &mut b {
            *cwd = "sub".to_owned();
        }
        assert_ne!(a.grant_id(), b.grant_id());
        assert_eq!(a.grant_id(), a.clone().grant_id());
        assert_eq!(
            path("src/b/x.rs", "src/a/x.rs").grant_id(),
            path("src/b/x.rs", "src/b/x.rs").grant_id()
        );
        assert!(a.grant_id().starts_with("g-") && a.grant_id().len() == 18);
    }

    proptest::proptest! {
        #[test]
        fn on_autopilot_no_call_waits_for_a_person(words in proptest::collection::vec("[a-z]{1,6}", 1..4), tool in 0usize..4) {
            let p = Policy::new(&[], &[]).autopilot();
            let argv: Vec<String> = words.clone();
            let (name, args) = match tool {
                0 => ("run_command", json!({ "argv": argv })),
                1 => ("write_file", json!({ "path": format!("src/{}.rs", words[0]), "content": "" })),
                2 => ("write_file", json!({ "path": format!(".github/{}.yml", words[0]), "content": "" })),
                _ => ("mcp__x__y", json!({})),
            };
            let before = Policy::new(&[], &[]).classify(name, &args, None);
            let after = p.settle(p.classify(name, &args, None));
            proptest::prop_assert!(!matches!(after, Decision::Approval(_)));
            if matches!(before, Decision::Deny(_)) {
                proptest::prop_assert!(matches!(after, Decision::Deny(_)));
            }
            if before == Decision::Allow {
                proptest::prop_assert_eq!(after, Decision::Allow);
            }
        }
    }

    #[test]
    fn a_path_object_is_its_canonical_file_whatever_the_spelling() {
        assert_eq!(
            path("src/b/x.rs", "src/a/x.rs"),
            path("src/b/x.rs", "src/b/x.rs")
        );
        assert_ne!(
            path("src/b/x.rs", "src/a/x.rs"),
            path("src/a/x.rs", "src/a/x.rs")
        );
        assert!(
            path("src/b/x.rs", "src/a/x.rs")
                .changed_from(&path("src/a/x.rs", "src/a/x.rs"))
                .is_some_and(|c| c == "resolves to src/b/x.rs, was src/a/x.rs")
        );
        // Commands differ by every field.
        let a = command("make check");
        let mut b = a.clone();
        if let Object::Command { cwd, .. } = &mut b {
            *cwd = "sub".to_owned();
        }
        assert_ne!(a, b);
        assert_eq!(a.class(), Class::Command);
        assert!(a.short().starts_with("argv#") && a.short().len() == 17);
        // The record round-trips; an older grant shape does not read.
        let back: Object = serde_json::from_value(serde_json::to_value(&a).unwrap()).unwrap();
        assert_eq!(back, a);
        assert!(serde_json::from_value::<Object>(json!({ "kind": "write", "path": "x" })).is_err());
        assert!(
            serde_json::from_value::<Rejection>(json!({ "grant": { "kind": "path", "canonical": "x", "asked": "x" }, "reason": "r" }))
                .is_err()
        );
    }

    #[test]
    fn the_grant_path_has_no_pattern() {
        // Matching a person's decision is object equality only.
        let source = include_str!("policy.rs");
        let classify = source
            .split("pub fn classify(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .unwrap_or_default();
        assert!(!classify.is_empty());
        for pattern in ["starts_with", "glob", "prefix"] {
            assert!(!classify.contains(pattern), "{pattern} in the grant path");
        }
    }
}
