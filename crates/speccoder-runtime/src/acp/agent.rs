//! Which agents can make an attempt, and how each is started: the command
//! the ACP registry publishes for it, an `npx` package pinned to an exact
//! version (`npx` would otherwise fetch whatever is newest).
//!
//! Adapted from devplane's `acp::agent` (same owner, MIT OR Apache-2.0).

/// One preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// `agent = "<id>"` in `[executor]`.
    pub id: &'static str,
    /// The agent's name, for a person.
    pub name: &'static str,
    /// The command line.
    pub argv: &'static [&'static str],
    /// A program on `PATH` that is the same agent without `npx` (it is used
    /// when found).
    pub installed: Option<&'static [&'static str]>,
    /// How a person logs the agent in, when the agent does not say.
    pub login: &'static str,
}

/// The presets, each `npx` package pinned to an exact `x.y.z` (the current
/// release on 2026-10-28).
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "copilot",
        name: "GitHub Copilot",
        argv: &["npx", "-y", "@github/copilot@1.0.105", "--acp"],
        installed: Some(&["copilot", "--acp"]),
        login: "copilot login",
    },
    Preset {
        id: "claude",
        name: "Claude Code",
        argv: &["npx", "-y", "@agentclientprotocol/claude-agent-acp@0.98.0"],
        installed: None,
        login: "claude /login",
    },
    Preset {
        id: "codex",
        name: "Codex",
        argv: &["npx", "-y", "@agentclientprotocol/codex-acp@2.3.0"],
        installed: None,
        login: "codex login",
    },
    Preset {
        id: "gemini",
        name: "Gemini CLI",
        argv: &["npx", "-y", "@google/gemini-cli@0.66.0", "--acp"],
        installed: None,
        login: "gemini (sign in when it asks)",
    },
    // The registry distributes OpenCode as a binary; there is no `npx`
    // entry to pin.
    Preset {
        id: "opencode",
        name: "OpenCode",
        argv: &["opencode", "acp"],
        installed: None,
        login: "opencode auth login",
    },
];

/// The preset called `id`.
#[must_use]
pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// The preset ids, for messages: `copilot, claude, codex, gemini, opencode`.
#[must_use]
pub fn ids() -> String {
    PRESETS.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
}

/// The agent an attempt starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// A preset's id, or `acp` for an `argv` of the person's.
    pub id: String,
    /// For a person.
    pub name: String,
    /// The command line, as configured (a preset's own, or `argv`).
    pub argv: Vec<String>,
    /// The permission mode the person chose, in the agent's own spelling;
    /// `None` lets speccoder ask for the agent's ask-first mode.
    pub mode: Option<String>,
}

impl Agent {
    /// The preset `p`.
    #[must_use]
    pub fn of(p: &Preset, mode: Option<String>) -> Self {
        Self {
            id: p.id.to_owned(),
            name: p.name.to_owned(),
            argv: p.argv.iter().map(|a| (*a).to_owned()).collect(),
            mode,
        }
    }

    /// Any ACP agent, started as `argv`.
    #[must_use]
    pub fn custom(argv: Vec<String>, mode: Option<String>) -> Self {
        let name = argv
            .first()
            .map(|p| p.rsplit('/').next().unwrap_or(p).to_owned())
            .unwrap_or_default();
        Self {
            id: "acp".to_owned(),
            name,
            argv,
            mode,
        }
    }

    /// The preset, if it is one.
    #[must_use]
    pub fn preset(&self) -> Option<&'static Preset> {
        preset(&self.id)
    }

    /// The command line started: an installed program where the preset has
    /// one and `on_path` finds it, else the configured one.
    #[must_use]
    pub fn command(&self, on_path: impl Fn(&str) -> bool) -> Vec<String> {
        match self.preset().and_then(|p| p.installed) {
            Some(installed) if installed.first().is_some_and(|p| on_path(p)) => {
                installed.iter().map(|a| (*a).to_owned()).collect()
            }
            _ => self.argv.clone(),
        }
    }

    /// The npm package and exact version an `npx` command is pinned to.
    #[must_use]
    pub fn npm_pin(argv: &[String]) -> Option<(&str, &str)> {
        if argv.first().map(String::as_str) != Some("npx") {
            return None;
        }
        let package = argv[1..]
            .iter()
            .find(|w| !w.starts_with('-') && w.rfind('@').is_some_and(|i| i > 0))?;
        let at = package.rfind('@')?;
        Some((&package[..at], &package[at + 1..]))
    }
}

/// Whether `program` is an executable on `PATH`.
#[must_use]
pub fn on_path(program: &str) -> bool {
    let env: Vec<(String, String)> = std::env::vars().collect();
    crate::sandbox::resolve_executable(program, std::path::Path::new("."), &env).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_the_registry_commands_pinned_exactly() {
        let mut npx = 0;
        for p in PRESETS {
            let argv: Vec<String> = p.argv.iter().map(|a| (*a).to_owned()).collect();
            let Some((package, version)) = Agent::npm_pin(&argv) else {
                assert_eq!(p.argv, ["opencode", "acp"], "{} is not pinned", p.id);
                continue;
            };
            npx += 1;
            let parts: Vec<&str> = version.split('.').collect();
            assert!(
                parts.len() == 3
                    && parts
                        .iter()
                        .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())),
                "{} pins {package}@{version}, a range rather than a version",
                p.id
            );
        }
        assert_eq!(npx, 4);
    }

    #[test]
    fn copilot_runs_installed_when_it_is_on_path() {
        let copilot = Agent::of(preset("copilot").unwrap_or(&PRESETS[0]), None);
        assert_eq!(copilot.command(|p| p == "copilot"), ["copilot", "--acp"]);
        assert_eq!(
            copilot.command(|_| false),
            ["npx", "-y", "@github/copilot@1.0.105", "--acp"]
        );
        // An argv of the person's is started as written.
        let own = Agent::custom(vec!["/opt/agent".to_owned(), "--acp".to_owned()], None);
        assert_eq!(own.command(|_| true), ["/opt/agent", "--acp"]);
        assert_eq!((own.id.as_str(), own.name.as_str()), ("acp", "agent"));
    }
}
