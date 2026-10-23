//! The policy classifier: every tool call is
//! allowed, needs approval, or is denied — decided by code, before the call.
//!
//! It decides *which* calls run; what an allowed command can reach is the
//! OS sandbox's job (`crate::sandbox`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::paths::{Protected, normalise};

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

/// The class of calls a person's decision covers: writing
/// one path (any content — content hashes would make every retry a new
/// approval), or one exact command.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Grant {
    /// `write_file` / `edit_file` of this workspace-relative path.
    Write {
        /// Normalised path.
        path: String,
    },
    /// `run_command` with exactly this argv.
    Command {
        /// The argv.
        argv: Vec<String>,
    },
    /// `run_command` with exactly this argv, with the network.
    Network {
        /// The argv.
        argv: Vec<String>,
    },
    /// Any call of this MCP tool.
    Tool {
        /// `mcp__<server>__<tool>`.
        name: String,
    },
}

impl Grant {
    /// The class a call belongs to, for the tools a person can decide on.
    #[must_use]
    pub fn of(tool: &str, args: &Value) -> Option<Self> {
        match tool {
            "write_file" | "edit_file" => args
                .get("path")
                .and_then(Value::as_str)
                .and_then(|p| normalise(p).ok())
                .map(|path| Self::Write { path }),
            name if name.starts_with(crate::mcp::PREFIX) => Some(Self::Tool {
                name: name.to_owned(),
            }),
            "run_command" => args.get("argv").and_then(Value::as_array).map(|a| {
                let argv = a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                if args.get("network").and_then(Value::as_bool) == Some(true) {
                    Self::Network { argv }
                } else {
                    Self::Command { argv }
                }
            }),
            _ => None,
        }
    }

    /// One line for people.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Write { path } => format!("write `{path}`"),
            Self::Command { argv } => format!("run `{}`", argv.join(" ")),
            Self::Network { argv } => format!("run `{}` with the network", argv.join(" ")),
            Self::Tool { name } => format!("use `{name}`"),
        }
    }
}

/// A person's "no", with the reason the model is told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    /// What was rejected.
    pub grant: Grant,
    /// Why.
    pub reason: String,
}

/// The classifier for one project.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    allowed: Vec<Vec<String>>,
    protected: Protected,
    frozen: std::collections::BTreeSet<String>,
    approved: Vec<Grant>,
    rejected: Vec<Rejection>,
    /// MCP tools by model-facing name: `allow`, `approval` or `deny`.
    mcp: std::collections::BTreeMap<String, String>,
    /// The planning profile for hosted Spec Kit skills.
    authoring: bool,
    /// Commands run without the network; these prefixes get it,
    /// asking for it otherwise needs approval.
    network: Vec<Vec<String>>,
}

/// What a hosted planning skill writes: the feature's artifacts, the
/// feature selection and the constitution — Spec Kit's own outputs.
fn is_authoring_path(p: &str) -> bool {
    p.starts_with("specs/") || p == ".specify/feature.json" || p.starts_with(".specify/memory/")
}

/// One of Spec Kit's own scripts, run directly or by `bash`/`sh`.
fn is_speckit_script(argv: &[String]) -> bool {
    let script = |a: &str| {
        let a = a.strip_prefix("./").unwrap_or(a);
        // Spec Kit's scripts and those of installed extensions
        // (`.specify/extensions/<ext>/scripts/…`): the person installed them,
        // and the model can write neither.
        let ext_script = a
            .strip_prefix(".specify/extensions/")
            .is_some_and(|rest| rest.split('/').nth(1) == Some("scripts"));
        !a.contains("..")
            && (a.starts_with(".specify/scripts/bash/") && a.ends_with(".sh")
                || a.starts_with(".specify/scripts/powershell/") && a.ends_with(".ps1")
                || ext_script && (a.ends_with(".sh") || a.ends_with(".ps1")))
    };
    match argv {
        [first, ..] if script(first) => true,
        [shell, second, ..] if matches!(shell.as_str(), "bash" | "sh") => script(second),
        _ => false,
    }
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

impl Policy {
    /// A policy allowing the command prefixes of `checks` (program and
    /// subcommands, up to the first option) and the `extra` prefixes.
    #[must_use]
    pub fn new(checks: &[Vec<String>], extra: &[Vec<String>]) -> Self {
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
        allowed.extend(extra.iter().filter(|p| !p.is_empty()).cloned());
        allowed.sort();
        allowed.dedup();
        Self {
            allowed,
            protected: Protected::default(),
            frozen: std::collections::BTreeSet::new(),
            approved: Vec::new(),
            rejected: Vec::new(),
            mcp: std::collections::BTreeMap::new(),
            authoring: false,
            network: Vec::new(),
        }
    }

    /// The planning profile: Spec Kit's artifacts and scripts are the
    /// work; Spec Kit's machinery, verification inputs and secrets stay
    /// denied; anything else — source code included — needs approval.
    #[must_use]
    pub fn authoring(mut self) -> Self {
        self.authoring = true;
        self
    }

    /// `[sandbox] network_allow`: prefixes that get the network.
    #[must_use]
    pub fn with_network(mut self, allow: Vec<Vec<String>>) -> Self {
        self.network = allow.into_iter().filter(|p| !p.is_empty()).collect();
        self
    }

    /// Whether `argv` gets the network without asking.
    #[must_use]
    pub fn network_allowed(&self, argv: &[String]) -> bool {
        self.network.iter().any(|p| argv.starts_with(p))
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
    pub fn with_decisions(mut self, approved: Vec<Grant>, rejected: Vec<Rejection>) -> Self {
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
    /// writing under this policy: the authoring profile writes Spec Kit's
    /// artifacts, nothing else of the protected set.
    #[must_use]
    pub fn write_protected(&self, rel: &str) -> Option<super::paths::Class> {
        if self.authoring && is_authoring_path(rel) && !self.protected.is_secret(rel) {
            return None;
        }
        self.protected.write_class(rel)
    }

    /// Classifies `tool` called with `args`.
    #[must_use]
    pub fn classify(&self, tool: &str, args: &Value) -> Decision {
        let decision = self.base(tool, args);
        if !matches!(decision, Decision::Approval(_)) {
            return decision;
        }
        let Some(grant) = Grant::of(tool, args) else {
            return decision;
        };
        if let Some(r) = self.rejected.iter().find(|r| r.grant == grant) {
            return Decision::Deny(format!("a person rejected this: {}", r.reason));
        }
        if self.approved.contains(&grant) {
            return Decision::Allow;
        }
        decision
    }

    fn base(&self, tool: &str, args: &Value) -> Decision {
        if tool.starts_with(crate::mcp::PREFIX) {
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
        let argv: Vec<String> = args["argv"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
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
        if self.authoring && is_authoring_path(&p) && !self.protected.is_secret(&p) {
            return Decision::Allow;
        }
        if let Some(class) = self.protected.write_class(&p) {
            return Decision::Deny(format!("`{p}` is protected: {}", class.reason()));
        }
        if self.authoring {
            return Decision::Approval(format!(
                "a planning skill writes Spec Kit artifacts (specs/, .specify/feature.json, .specify/memory/); `{p}` is not one"
            ));
        }
        if self.frozen.contains(&p) {
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
        let argv: Vec<String> = args
            .get("argv")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if self.authoring && is_speckit_script(&argv) {
            return Decision::Allow;
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
        if self
            .allowed
            .iter()
            .any(|prefix| argv.len() >= prefix.len() && argv[..prefix.len()] == prefix[..])
        {
            return Decision::Allow;
        }
        Decision::Approval(format!(
            "`{}` is not a configured check or allowed command",
            argv.join(" ")
        ))
    }
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
            &[argv("cargo fmt"), argv("cargo check")],
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
            assert_eq!(kind(p.classify(tool, args)), *want, "{tool} {args}");
        }
    }

    #[test]
    fn decisions_cover_classes_and_never_override_a_denial() {
        let approved = vec![
            Grant::Write {
                path: "Cargo.toml".to_owned(),
            },
            Grant::Command {
                argv: vec!["cargo".to_owned(), "build".to_owned()],
            },
            Grant::Write {
                path: "speccoder.toml".to_owned(),
            },
        ];
        let rejected = vec![Rejection {
            grant: Grant::Command {
                argv: vec!["curl".to_owned(), "https://x".to_owned()],
            },
            reason: "no network".to_owned(),
        }];
        let p = policy().with_decisions(approved, rejected);
        // Any content for an approved path; the same exact command only.
        assert_eq!(
            p.classify(
                "edit_file",
                &json!({"path": "./Cargo.toml", "old_string": "a"})
            ),
            Decision::Allow
        );
        assert_eq!(
            p.classify("run_command", &cmd("cargo build")),
            Decision::Allow
        );
        assert!(matches!(
            p.classify("run_command", &cmd("cargo build --release")),
            Decision::Approval(_)
        ));
        // A rejection is a denial with the person's reason.
        assert_eq!(
            p.classify("run_command", &cmd("curl https://x")),
            Decision::Deny("a person rejected this: no network".to_owned())
        );
        // A protected path stays denied whatever was approved.
        assert!(matches!(
            p.classify("write_file", &json!({"path": "speccoder.toml"})),
            Decision::Deny(_)
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
                &cmd("python3 -m pytest -q --junitxml=.speccoder/junit.xml")
            ),
            Decision::Allow
        );
        assert!(matches!(
            p.classify("run_command", &cmd("python3 -c print(1)")),
            Decision::Approval(_)
        ));
        assert!(matches!(
            p.classify("run_command", &cmd("python3 -m pytest tests/x.py")),
            Decision::Approval(_)
        ));
        // Explicit allowances stay prefixes.
        let q = Policy::new(&[], &[argv("python3 -m pytest")]);
        assert_eq!(
            q.classify("run_command", &cmd("python3 -m pytest tests/x.py")),
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
        assert_eq!(p.classify("mcp__db__query", &json!({})), Decision::Allow);
        assert!(matches!(
            p.classify("mcp__db__drop", &json!({})),
            Decision::Deny(_)
        ));
        assert!(matches!(
            p.classify("mcp__db__insert", &json!({})),
            Decision::Approval(_)
        ));
        assert!(matches!(
            p.classify("mcp__db__other", &json!({})),
            Decision::Deny(_)
        ));
        // An approval for the task covers the tool; it never lifts a denial.
        let q = p.clone().with_decisions(
            vec![
                Grant::Tool {
                    name: "mcp__db__insert".to_owned(),
                },
                Grant::Tool {
                    name: "mcp__db__drop".to_owned(),
                },
            ],
            Vec::new(),
        );
        assert_eq!(
            q.classify("mcp__db__insert", &json!({ "row": 1 })),
            Decision::Allow
        );
        assert!(matches!(
            q.classify("mcp__db__drop", &json!({})),
            Decision::Deny(_)
        ));
        assert_eq!(
            Grant::of("mcp__db__insert", &json!({}))
                .map(|g| g.describe())
                .as_deref(),
            Some("use `mcp__db__insert`")
        );
    }

    #[test]
    fn the_authoring_profile_writes_spec_kit_artifacts_and_runs_its_scripts_only() {
        let p = Policy::new(&[], &[]).authoring();
        let w = |path: &str| p.classify("write_file", &json!({ "path": path }));
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
        assert!(matches!(w("src/lib.rs"), Decision::Approval(_)));
        let run = |argv: &[&str]| p.classify("run_command", &json!({ "argv": argv }));
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
            Policy::new(&[], &[])
                .classify("write_file", &json!({ "path": "specs/002-login/spec.md" })),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn asking_for_the_network_needs_a_person_unless_allowed() {
        let checks = [vec!["cargo".to_owned(), "test".to_owned()]];
        let p = Policy::new(&checks, &[]);
        let net = json!({ "argv": ["cargo", "test"], "network": true });
        assert!(matches!(
            p.classify("run_command", &net),
            Decision::Approval(_)
        ));
        assert_eq!(
            Grant::of("run_command", &net),
            Some(Grant::Network {
                argv: vec!["cargo".to_owned(), "test".to_owned()]
            })
        );
        // Without the network the same command is allowed; a denial stays one.
        assert_eq!(
            p.classify("run_command", &json!({ "argv": ["cargo", "test"] })),
            Decision::Allow
        );
        assert!(matches!(
            p.classify(
                "run_command",
                &json!({ "argv": ["git", "push"], "network": true })
            ),
            Decision::Deny(_)
        ));
        let allowed = p.clone().with_network(vec![vec!["cargo".to_owned()]]);
        assert_eq!(allowed.classify("run_command", &net), Decision::Allow);
        let approved = Policy::new(&checks, &[]).with_decisions(
            vec![Grant::Network {
                argv: vec!["cargo".to_owned(), "test".to_owned()],
            }],
            vec![],
        );
        assert_eq!(approved.classify("run_command", &net), Decision::Allow);
    }
}
