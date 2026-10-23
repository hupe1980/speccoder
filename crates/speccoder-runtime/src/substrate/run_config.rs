//! `[budget]` and `[commands]` in `speccoder.toml` (policy).

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBudget {
    max_turns: Option<u32>,
    max_tool_calls: Option<u32>,
    max_attempts: Option<u32>,
    best_of: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAcceptance {
    generate: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFormat {
    argv: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTests {
    frozen: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMonitor {
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConverge {
    max_rounds: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGit {
    commit: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommands {
    #[serde(default)]
    allow: Vec<Vec<String>>,
}

/// Limits and command allowances for an attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    /// Model turns per attempt.
    pub max_turns: u32,
    /// Tool calls per attempt.
    pub max_tool_calls: u32,
    /// Attempts per task run; failed verdicts are retried up to this.
    pub max_attempts: u32,
    /// Independent samples per task, the best by evidence kept.
    pub best_of: u32,
    /// Extra allowed argv prefixes for `run_command`.
    pub allow: Vec<Vec<String>>,
    /// Commit each passed task on the speccoder branch (`[git] commit = "per-task"`).
    pub commit_per_task: bool,
    /// Derive acceptance tests from the spec before attempts (`[acceptance] generate`).
    pub acceptance: bool,
    /// Convergence phases a feature may get (`[converge] max_rounds`).
    pub max_converge_rounds: u32,
    /// Ask the gaming monitor about every passing task (`[monitor] enabled`).
    pub monitor: bool,
    /// Test files that existed before an attempt may gain lines without
    /// approval (`[tests] frozen = "additions"`; default `"ask"`).
    pub frozen_additions: bool,
    /// The formatter speccoder runs after each attempt, before verifying
    /// (`[format] argv`): formatting is not the model's job to remember.
    pub format: Option<Vec<String>>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_turns: 30,
            max_tool_calls: 100,
            max_attempts: 2,
            best_of: 1,
            allow: Vec::new(),
            commit_per_task: true,
            acceptance: true,
            max_converge_rounds: 3,
            monitor: false,
            frozen_additions: false,
            format: None,
        }
    }
}

/// Reads `[budget]` and `[commands]`; absent sections give defaults.
pub fn parse(text: &str) -> Result<RunConfig, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let budget: RawBudget = match table.get("budget") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[budget]: {e}"))?,
        None => RawBudget::default(),
    };
    let commands: RawCommands = match table.get("commands") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[commands]: {e}"))?,
        None => RawCommands::default(),
    };
    let git: RawGit = match table.get("git") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[git]: {e}"))?,
        None => RawGit::default(),
    };
    let commit_per_task = match git.commit.as_deref() {
        None | Some("per-task") => true,
        Some("none") => false,
        Some(other) => return Err(format!("[git]: commit = \"{other}\" (per-task or none)")),
    };
    let acceptance: RawAcceptance = match table.get("acceptance") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[acceptance]: {e}"))?,
        None => RawAcceptance::default(),
    };
    let converge: RawConverge = match table.get("converge") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[converge]: {e}"))?,
        None => RawConverge::default(),
    };
    let monitor: RawMonitor = match table.get("monitor") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[monitor]: {e}"))?,
        None => RawMonitor::default(),
    };
    let tests: RawTests = match table.get("tests") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[tests]: {e}"))?,
        None => RawTests::default(),
    };
    let frozen_additions = match tests.frozen.as_deref() {
        None | Some("ask") => false,
        Some("additions") => true,
        Some(other) => {
            return Err(format!("[tests]: frozen = \"{other}\" (ask or additions)"));
        }
    };
    let format: Option<RawFormat> = match table.get("format") {
        Some(v) => Some(v.clone().try_into().map_err(|e| format!("[format]: {e}"))?),
        None => None,
    };
    if format
        .as_ref()
        .is_some_and(|f| f.argv.first().is_none_or(String::is_empty))
    {
        return Err("[format]: `argv` must name a program".to_owned());
    }
    let d = RunConfig::default();
    Ok(RunConfig {
        max_turns: budget.max_turns.unwrap_or(d.max_turns).max(1),
        max_tool_calls: budget.max_tool_calls.unwrap_or(d.max_tool_calls),
        max_attempts: budget.max_attempts.unwrap_or(d.max_attempts).max(1),
        best_of: budget.best_of.unwrap_or(d.best_of).clamp(1, 10),
        allow: commands
            .allow
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect(),
        commit_per_task,
        acceptance: acceptance.generate.unwrap_or(true),
        max_converge_rounds: converge.max_rounds.unwrap_or(d.max_converge_rounds),
        monitor: monitor.enabled.unwrap_or(false),
        frozen_additions,
        format: format.map(|f| f.argv),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        assert_eq!(parse("").unwrap(), RunConfig::default());
        let c =
            parse("[budget]\nmax_turns = 5\n[commands]\nallow = [[\"cargo\", \"fmt\"]]\n").unwrap();
        assert_eq!(c.max_turns, 5);
        assert_eq!(c.max_tool_calls, 100);
        assert_eq!(c.max_attempts, 2);
        assert_eq!(
            parse("[budget]\nmax_attempts = 0\n").unwrap().max_attempts,
            1
        );
        assert_eq!(c.allow, [vec!["cargo".to_owned(), "fmt".to_owned()]]);
        assert!(parse("[budget]\nmax_cost = 1\n").is_err());
        assert!(!parse("[git]\ncommit = \"none\"\n").unwrap().commit_per_task);
        assert!(parse("[git]\ncommit = \"always\"\n").is_err());
        assert!(
            parse("[tests]\nfrozen = \"additions\"\n")
                .unwrap()
                .frozen_additions
        );
        assert!(
            !parse("[tests]\nfrozen = \"ask\"\n")
                .unwrap()
                .frozen_additions
        );
        assert!(parse("[tests]\nfrozen = \"anything\"\n").is_err());
    }
}
