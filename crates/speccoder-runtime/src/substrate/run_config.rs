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
    held_out: Option<bool>,
    strength: Option<bool>,
    min_strength: Option<f64>,
    near_miss: Option<u32>,
    permitted: Option<u32>,
    strength_timeout_secs: Option<u64>,
}

/// How the acceptance set's strength is measured: `[acceptance]`
/// `strength`, `min_strength`, `near_miss`, `permitted`,
/// `strength_timeout_secs`.
#[derive(Debug, Clone, PartialEq)]
pub struct StrengthConfig {
    /// Measure after every `verified` verdict (default on).
    pub enabled: bool,
    /// The gate; `None` (default): strength never changes a verdict.
    pub min: Option<f64>,
    /// Wrong variants asked for (0–3, default 2).
    pub near_miss: u32,
    /// Spec-permitted alternatives asked for (0–2, default 1; 0: no
    /// over-strict check).
    pub permitted: u32,
    /// The whole stage's budget.
    pub timeout: std::time::Duration,
}

// `min` is validated to 0.0–1.0, so it is never NaN.
impl Eq for StrengthConfig {}

impl Default for StrengthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min: None,
            near_miss: 2,
            permitted: 1,
            timeout: std::time::Duration::from_secs(900),
        }
    }
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
struct RawContext {
    instructions: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConverge {
    max_rounds: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReview {
    required: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAttest {
    enabled: Option<bool>,
    signer: Option<String>,
    sign_at: Option<String>,
    sigstore: Option<RawSigstore>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSigstore {
    identity: Option<String>,
    issuer: Option<String>,
}

/// Who signs a task's attestation (`[attest] signer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signer {
    /// The person's git signing key (SSH or OpenPGP), as git would use it.
    Git,
    /// Sigstore keyless, through `cosign`.
    Sigstore,
    /// Nobody: statements are stored unsigned.
    None,
}

impl Signer {
    /// The configured word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Sigstore => "sigstore",
            Self::None => "none",
        }
    }
}

/// `[attest]`: signed evidence per ticked task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestConfig {
    /// Write attestations on ticks.
    pub enabled: bool,
    /// Who signs.
    pub signer: Signer,
    /// Sign at the tick, or later with `speccoder attest sign` (touch keys).
    pub sign_later: bool,
    /// The Sigstore certificate identity the project says signs: shown to
    /// reviewers (`pr`) to confirm, never trusted by `attest verify`.
    pub sigstore_identity: Option<String>,
    /// That identity's OIDC issuer, likewise.
    pub sigstore_issuer: Option<String>,
}

impl Default for AttestConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            signer: Signer::Git,
            sign_later: false,
            sigstore_identity: None,
            sigstore_issuer: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApprovals {
    wait: Option<String>,
    budget: Option<i64>,
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
    #[serde(default)]
    allow_prefix: Vec<Vec<String>>,
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
    /// Exact argvs `run_command` runs without asking (`[commands] allow`).
    pub allow: Vec<Vec<String>>,
    /// Argv prefixes (two elements or more) it runs without asking
    /// (`[commands] allow_prefix`), saved only from a coverage preview.
    pub allow_prefix: Vec<Vec<String>>,
    /// Prompts a run shows a person before further calls wait together in
    /// Needs You (`[approvals] budget`, 0–100, default 12).
    pub approval_budget: u32,
    /// Commit each passed task on the speccoder branch (`[git] commit = "per-task"`).
    pub commit_per_task: bool,
    /// Derive acceptance tests from the spec before attempts (`[acceptance] generate`).
    pub acceptance: bool,
    /// Also author held-out acceptance tests the implementer never sees
    /// (`[acceptance] held_out`, default on; nothing without `generate`).
    pub held_out: bool,
    /// Acceptance strength.
    pub strength: StrengthConfig,
    /// Convergence phases a feature may get (`[converge] max_rounds`).
    pub max_converge_rounds: u32,
    /// Ask the gaming monitor about every passing task (`[monitor] enabled`).
    pub monitor: bool,
    /// A run waits until every requirement is approved on its current
    /// wording (`[review] required`).
    pub review_required: bool,
    /// Test files that existed before an attempt may gain lines without
    /// approval (`[tests] frozen = "additions"`; default `"ask"`).
    pub frozen_additions: bool,
    /// The formatter speccoder runs after each attempt, before verifying
    /// (`[format] argv`): formatting is not the model's job to remember.
    pub format: Option<Vec<String>>,
    /// How long a call waits for a person before it is denied as expired
    /// (`[approvals] wait`, default 7 days).
    pub approval_wait: std::time::Duration,
    /// `[attest]`.
    pub attest: AttestConfig,
    /// `[context] instructions`: the file names read per directory as the
    /// repository's instructions for agents; empty: none.
    pub instructions: Vec<String>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_turns: 30,
            max_tool_calls: 100,
            max_attempts: 2,
            best_of: 1,
            allow: Vec::new(),
            allow_prefix: Vec::new(),
            approval_budget: super::attempt::DEFAULT_BUDGET,
            commit_per_task: true,
            acceptance: true,
            held_out: true,
            strength: StrengthConfig::default(),
            max_converge_rounds: 3,
            monitor: false,
            review_required: false,
            frozen_additions: false,
            format: None,
            approval_wait: std::time::Duration::from_secs(7 * 24 * 3600),
            attest: AttestConfig::default(),
            instructions: vec!["AGENTS.md".to_owned()],
        }
    }
}

/// `30m`, `12h` or `7d`; never zero.
fn wait(text: &str) -> Result<std::time::Duration, String> {
    let text = text.trim();
    // The unit is the last character, which may be more than a byte (`7д`).
    let (n, unit) = text.split_at(text.char_indices().last().map_or(0, |(i, _)| i));
    let secs = match unit {
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => 0,
    };
    match n.parse::<u64>() {
        Ok(n) if n > 0 && secs > 0 => Ok(std::time::Duration::from_secs(n.saturating_mul(secs))),
        _ => Err(format!(
            "[approvals]: wait = \"{text}\" (a positive number of m, h or d, e.g. \"7d\")"
        )),
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
    let review: RawReview = match table.get("review") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[review]: {e}"))?,
        None => RawReview::default(),
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
    let approvals: RawApprovals = match table.get("approvals") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[approvals]: {e}"))?,
        None => RawApprovals::default(),
    };
    let raw_attest: RawAttest = match table.get("attest") {
        Some(v) => v.clone().try_into().map_err(|e| format!("[attest]: {e}"))?,
        None => RawAttest::default(),
    };
    let attest = AttestConfig {
        enabled: raw_attest.enabled.unwrap_or(true),
        signer: match raw_attest.signer.as_deref() {
            None | Some("git") => Signer::Git,
            Some("sigstore") => Signer::Sigstore,
            Some("none") => Signer::None,
            Some(other) => {
                return Err(format!(
                    "[attest]: signer = \"{other}\" (git, sigstore or none)"
                ));
            }
        },
        sign_later: match raw_attest.sign_at.as_deref() {
            None | Some("tick") => false,
            Some("later") => true,
            Some(other) => {
                return Err(format!("[attest]: sign_at = \"{other}\" (tick or later)"));
            }
        },
        sigstore_identity: raw_attest
            .sigstore
            .as_ref()
            .and_then(|s| s.identity.clone()),
        sigstore_issuer: raw_attest.sigstore.and_then(|s| s.issuer),
    };
    let context: RawContext = match table.get("context") {
        Some(v) => v
            .clone()
            .try_into()
            .map_err(|e| format!("[context]: {e}"))?,
        None => RawContext::default(),
    };
    if let Some(bad) = context.instructions.iter().flatten().find(|n| {
        n.is_empty() || n.contains('/') || n.contains('\\') || matches!(n.as_str(), "." | "..")
    }) {
        return Err(format!(
            "[context]: instructions = \"{bad}\": a file name such as AGENTS.md, without a directory"
        ));
    }
    let d = RunConfig::default();
    let strength = {
        let ds = StrengthConfig::default();
        if let Some(m) = acceptance.min_strength
            && !(0.0..=1.0).contains(&m)
        {
            return Err(format!(
                "[acceptance]: min_strength = {m} (a share from 0.0 to 1.0)"
            ));
        }
        let near_miss = acceptance.near_miss.unwrap_or(ds.near_miss);
        if near_miss > 3 {
            return Err(format!("[acceptance]: near_miss = {near_miss} (0 to 3)"));
        }
        let permitted = acceptance.permitted.unwrap_or(ds.permitted);
        if permitted > 2 {
            return Err(format!("[acceptance]: permitted = {permitted} (0 to 2)"));
        }
        StrengthConfig {
            enabled: acceptance.strength.unwrap_or(ds.enabled),
            min: acceptance.min_strength,
            near_miss,
            permitted,
            timeout: acceptance
                .strength_timeout_secs
                .map_or(ds.timeout, |s| std::time::Duration::from_secs(s.max(1))),
        }
    };
    let approval_wait = match approvals.wait.as_deref() {
        Some(w) => wait(w)?,
        None => d.approval_wait,
    };
    let approval_budget = match approvals.budget {
        None => d.approval_budget,
        Some(n) => u32::try_from(n)
            .ok()
            .filter(|n| *n <= 100)
            .ok_or_else(|| format!("[approvals]: budget = {n} (a whole number from 0 to 100)"))?,
    };
    if let Some(why) = commands
        .allow_prefix
        .iter()
        .find_map(|p| crate::tools::policy::prefix_refusal(p))
    {
        return Err(format!("[commands] allow_prefix: {why}"));
    }
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
        allow_prefix: commands.allow_prefix,
        approval_budget,
        commit_per_task,
        acceptance: acceptance.generate.unwrap_or(true),
        held_out: acceptance.held_out.unwrap_or(true),
        strength,
        max_converge_rounds: converge.max_rounds.unwrap_or(d.max_converge_rounds),
        monitor: monitor.enabled.unwrap_or(false),
        review_required: review.required.unwrap_or(false),
        frozen_additions,
        format: format.map(|f| f.argv),
        approval_wait,
        attest,
        instructions: context.instructions.unwrap_or(d.instructions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_md_is_read_unless_configured_otherwise() {
        assert_eq!(parse("").unwrap().instructions, ["AGENTS.md"]);
        let c = parse("[context]\ninstructions = [\"AGENTS.md\", \"CLAUDE.md\"]\n").unwrap();
        assert_eq!(c.instructions, ["AGENTS.md", "CLAUDE.md"]);
        assert!(
            parse("[context]\ninstructions = []\n")
                .unwrap()
                .instructions
                .is_empty()
        );
        assert!(parse("[context]\ninstructions = [\"docs/AGENTS.md\"]\n").is_err());
        assert!(parse("[context]\nfiles = []\n").is_err());
    }

    #[test]
    fn attestations_are_on_and_signed_by_git_unless_configured() {
        let d = parse("").unwrap_or_default().attest;
        assert!(d.enabled && d.signer == Signer::Git && !d.sign_later);
        let c = parse("[attest]\nsigner = \"sigstore\"\nsign_at = \"later\"\n[attest.sigstore]\nidentity = \"ada@example.com\"\nissuer = \"https://accounts.example\"\n")
            .unwrap_or_default()
            .attest;
        assert_eq!((c.signer, c.sign_later), (Signer::Sigstore, true));
        assert_eq!(c.sigstore_identity.as_deref(), Some("ada@example.com"));
        assert!(parse("[attest]\nsigner = \"x509\"\n").is_err());
        assert!(parse("[attest]\nsign_at = \"never\"\n").is_err());
        assert!(parse("[attest]\nkey = \"x\"\n").is_err());
    }

    #[test]
    fn an_approval_waits_seven_days_unless_configured() {
        assert_eq!(
            parse("").unwrap_or_default().approval_wait.as_secs(),
            7 * 86_400
        );
        let w = |t: &str| {
            parse(&format!("[approvals]\nwait = \"{t}\"\n")).map(|c| c.approval_wait.as_secs())
        };
        assert_eq!(w("30m"), Ok(1800));
        assert_eq!(w("12h"), Ok(43_200));
        for bad in ["0d", "7", "7w", "-1d", ""] {
            assert!(w(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn strength_is_measured_by_default_and_bounded() {
        let c = parse("").unwrap().strength;
        assert!(c.enabled && c.min.is_none());
        assert_eq!((c.near_miss, c.permitted), (2, 1));
        let c = parse("[acceptance]\nmin_strength = 0.8\nnear_miss = 3\npermitted = 0\nstrength_timeout_secs = 60\n")
            .unwrap()
            .strength;
        assert_eq!(c.min, Some(0.8));
        assert_eq!((c.near_miss, c.permitted, c.timeout.as_secs()), (3, 0, 60));
        assert!(
            !parse("[acceptance]\nstrength = false\n")
                .unwrap()
                .strength
                .enabled
        );
        for bad in ["min_strength = 1.5", "near_miss = 4", "permitted = 3"] {
            assert!(parse(&format!("[acceptance]\n{bad}\n")).is_err(), "{bad}");
        }
    }

    #[test]
    fn held_out_is_on_by_default_and_can_be_switched_off() {
        assert!(parse("").unwrap().held_out);
        let c = parse("[acceptance]\nheld_out = false\n").unwrap();
        assert!(!c.held_out && c.acceptance);
        assert!(parse("[acceptance]\nheldout = true\n").is_err());
    }

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

    #[test]
    fn the_asking_budget_and_prefixes_are_checked() {
        assert_eq!(parse("").unwrap().approval_budget, 12);
        assert_eq!(
            parse("[approvals]\nbudget = 0\n").unwrap().approval_budget,
            0
        );
        for bad in ["101", "-1"] {
            let e = parse(&format!("[approvals]\nbudget = {bad}\n")).unwrap_err();
            assert!(e.contains("budget"), "{e}");
        }
        let c = parse("[commands]\nallow_prefix = [[\"npm\", \"run\"]]\n").unwrap();
        assert_eq!(c.allow_prefix, [vec!["npm".to_owned(), "run".to_owned()]]);
        let e = parse("[commands]\nallow_prefix = [[\"python3\"]]\n").unwrap_err();
        assert!(e.contains("a bare program allows any argument"), "{e}");
    }
}
