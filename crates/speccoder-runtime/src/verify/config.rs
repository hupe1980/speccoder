//! `speccoder.toml`: the checks a project declares.

use std::time::Duration;

use serde::Deserialize;

/// The configuration file, at the project root.
pub const FILE: &str = "speccoder.toml";

const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// When a check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Every task.
    Fast,
    /// Once after a run ticks the feature's last task, and with `speccoder verify`.
    Full,
}

impl Tier {
    /// Lowercase name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCheck {
    argv: Vec<String>,
    cwd: Option<String>,
    when_exists: Option<String>,
    tier: Option<Tier>,
    timeout_secs: Option<u64>,
    junit: Option<String>,
}

/// One declared check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Table name, e.g. `test` for `[checks.test]`.
    pub name: String,
    /// Program and arguments; run without a shell.
    pub argv: Vec<String>,
    /// Working directory relative to the root (`.` by default).
    pub cwd: String,
    /// Only applicable when this root-relative path exists.
    pub when_exists: Option<String>,
    /// `fast` (default) or `full`.
    pub tier: Tier,
    /// Deadline for the whole process group.
    pub timeout: Duration,
    /// Root-relative JUnit report the check writes.
    pub junit: Option<String>,
}

/// The top-level sections of `speccoder.toml`.
pub const SECTIONS: &[&str] = &[
    "checks",
    "model",
    "budget",
    "commands",
    "git",
    "acceptance",
    "tests",
    "monitor",
    "converge",
    "format",
    "sandbox",
    "mutation",
    "executor",
    "mcp",
    "spec",
];

/// Parses the configuration text; checks keep their file order.
pub fn parse(text: &str) -> Result<Vec<Check>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    // A misspelled section would otherwise be silently ignored by every
    // command; the other sections are read (and their keys checked) by the
    // modules that own them.
    // A person's UI settings are not the project's policy (D-069).
    if table.contains_key("ui") {
        return Err(format!(
            "[ui] is a person's setting, not project policy; move it to {}",
            crate::user_config::path().map_or_else(
                || "~/.config/speccoder/config.toml".to_owned(),
                |p| p.display().to_string()
            )
        ));
    }
    if let Some(unknown) = table.keys().find(|k| !SECTIONS.contains(&k.as_str())) {
        return Err(format!(
            "unknown section `[{unknown}]` (known: {})",
            SECTIONS.join(", ")
        ));
    }
    let Some(checks) = table.get("checks") else {
        return Ok(Vec::new());
    };
    let checks = checks
        .as_table()
        .ok_or_else(|| "`checks` must be a table of [checks.<name>] tables".to_owned())?;
    checks
        .iter()
        .map(|(name, value)| {
            let raw: RawCheck = value
                .clone()
                .try_into()
                .map_err(|e| format!("[checks.{name}]: {e}"))?;
            if raw.argv.is_empty() || raw.argv[0].is_empty() {
                return Err(format!("[checks.{name}]: `argv` must name a program"));
            }
            Ok(Check {
                name: name.clone(),
                argv: raw.argv,
                cwd: raw.cwd.unwrap_or_else(|| ".".to_owned()),
                when_exists: raw.when_exists,
                tier: raw.tier.unwrap_or(Tier::Fast),
                timeout: Duration::from_secs(raw.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS)),
                junit: raw.junit,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misspelled_section_is_an_error_not_ignored() {
        let e = parse("[checks.t]\nargv = [\"true\"]\n[budgett]\nmax_turns = 3\n").unwrap_err();
        assert!(e.contains("unknown section `[budgett]`"), "{e}");
        assert!(parse("[checks.t]\nargv = [\"true\"]\n[sandbox]\nmode = \"off\"\n").is_ok());
    }

    #[test]
    fn parses_checks_in_file_order_with_defaults() {
        let checks = parse(
            r#"
[model]
provider = "anthropic"

[checks.test]
argv = ["cargo", "nextest", "run"]
junit = "target/nextest/speccoder/junit.xml"
when_exists = "Cargo.toml"

[checks.clippy]
argv = ["cargo", "clippy"]
tier = "full"
timeout_secs = 30
"#,
        )
        .unwrap();
        let names: Vec<_> = checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["test", "clippy"]);
        assert_eq!(checks[0].tier, Tier::Fast);
        assert_eq!(checks[0].cwd, ".");
        assert_eq!(checks[0].timeout, Duration::from_secs(600));
        assert_eq!(checks[1].tier, Tier::Full);
        assert_eq!(checks[1].timeout, Duration::from_secs(30));
    }

    #[test]
    fn rejects_unknown_keys_and_empty_argv() {
        let e = parse("[checks.t]\nargv = [\"x\"]\nshell = true\n").unwrap_err();
        assert!(e.contains("[checks.t]") && e.contains("shell"), "{e}");
        assert!(
            parse("[checks.t]\nargv = []\n")
                .unwrap_err()
                .contains("argv")
        );
    }
}
