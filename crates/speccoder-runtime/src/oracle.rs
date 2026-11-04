//! The oracle at review: per requirement, the visible
//! acceptance tests the run would use, written on the person's request —
//! two independent author calls whose stated expectations are compared for
//! ambiguity, the first one's citing tests red-checked on the checkout's
//! `HEAD` in a scratch worktree, a held-out set sealed beside them — shaped
//! by the person (reject, ask again, edit, choose a reading), and frozen
//! with the approval ([`freeze`]). A task whose requirement has a frozen,
//! current oracle reuses it instead of calling the author.
//!
//! Drafts live in the state directory (`<state>/oracle/<unit>/<REQ>/`),
//! never in the worktree; the model writes none of them. A held-out test's
//! path or content never leaves its sealed store: the views carry its count
//! and digest only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use speccoder_core::acceptance::{Decided, PreFrozenAcceptance, Producer, SealedRef, VisibleTest};
use speccoder_core::actions::Via;
use speccoder_core::context::{ContextEntry, ContextKind};
use speccoder_core::ladder::Rung;
use speccoder_core::oracle::{
    Ambiguity, AuthoredBy, Expectation, HeldOutSummary, OracleDiscarded, OracleOp, OracleRejected,
    OracleState, OracleTest, OracleView, Reading, Resolution, Untestable,
};
use speccoder_core::snapshot::ProjectSnapshot;
use speccoder_core::verify::{RedClass, classify_red};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::format::{Adapter, OracleRequirement};
use crate::integrity::sha256_hex;
use crate::{sandbox, substrate, tools, worktree};

/// The store's directory under a project's state directory.
pub const DIR: &str = "oracle";

/// Visible tests kept per requirement.
pub const LIMIT: usize = 4;

/// A requirement's draft as the author and the person left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    /// The requirement as it read when the tests were written.
    pub requirement: DraftRequirement,
    /// Author calls per *Write Tests* (`[review] oracle_samples`).
    pub samples: u8,
    /// The kept tests.
    pub tests: Vec<DraftTest>,
    /// Tests not kept, and why.
    pub discarded: Vec<OracleDiscarded>,
    /// Tests the person rejected.
    pub rejected: Vec<OracleRejected>,
    /// What each author stated it asserts.
    pub expectations: Samples<Expectation>,
    /// What each author found untestable.
    pub untestable: Samples<Untestable>,
    /// Readings that disagree.
    pub ambiguities: Vec<Ambiguity>,
    /// The held-out set: count and digest.
    pub held_out: HeldOutSummary,
    /// Its sealed reference, when kept.
    pub sealed: Option<SealedRef>,
    /// The person's notes to later author calls (comments, readings).
    pub notes: Vec<String>,
    /// The journaled author runs.
    pub runs: Vec<String>,
    /// What reached each author call (constitution VIII): `{source,
    /// label, sha256}` in the context manifest's shape.
    pub context: Vec<ContextEntry>,
}

/// The requirement a draft was written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRequirement {
    /// `US1`.
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its text.
    pub text: String,
    /// `sha256:` of `title + "\n" + text`.
    pub sha256: String,
}

/// One stored test of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftTest {
    /// Project-relative path.
    pub path: String,
    /// `sha256:<hex>` of its content.
    pub sha256: String,
    /// The scenarios it cites (never empty).
    pub scenarios: Vec<String>,
    /// The command that runs only it.
    pub argv: Vec<String>,
    /// Its class on the checkout's `HEAD`.
    pub red_class: RedClass,
    /// Who wrote it.
    pub authored_by: AuthoredBy,
    /// The person who edited it.
    pub edited_by: Option<String>,
}

/// Something per author sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Samples<T> {
    /// The first author's (its tests are kept).
    pub sample1: Vec<T>,
    /// The second's (compared, then its tests discarded).
    pub sample2: Vec<T>,
}

impl<T> Default for Samples<T> {
    fn default() -> Self {
        Self {
            sample1: Vec::new(),
            sample2: Vec::new(),
        }
    }
}

/// An oracle entry of `review.json`: what the person froze, by digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleEntry {
    /// The requirement's wording digest.
    pub requirement_sha256: String,
    /// The visible set's manifest digest.
    pub manifest_sha256: String,
    /// The sealed held-out manifest's digest, when one was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_out_sha256: Option<String>,
    /// Who froze it.
    pub by: String,
    /// Through what.
    pub via: Via,
    /// Unix seconds.
    pub at: u64,
}

/// What a freeze keeps beside the pre-frozen record: the commands and the
/// wording, bound to the manifest digest `review.json` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frozen {
    /// The wording digest.
    pub requirement_sha256: String,
    /// The wording it was frozen on (title, then text).
    pub wording: String,
    /// The manifest digest.
    pub manifest_sha256: String,
    /// Each frozen test with the command that runs only it.
    pub tests: Vec<FrozenTest>,
}

/// One frozen test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenTest {
    /// Project-relative path.
    pub path: String,
    /// `sha256:<hex>`.
    pub sha256: String,
    /// Its command.
    pub argv: Vec<String>,
    /// The scenarios it cites.
    pub scenarios: Vec<String>,
}

/// A draft with its tests' contents (path, text).
pub type Loaded = (Draft, Vec<(String, String)>);

/// A test to keep: path, content, the scenarios it cites, its command.
type Kept = (String, String, Vec<String>, Vec<String>);

/// A stored draft that no longer matches its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tampered(pub String);

fn slug(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Where a requirement's draft lives.
#[must_use]
pub fn dir(project_state: &Path, unit: &str, key: &str) -> PathBuf {
    project_state.join(DIR).join(unit).join(slug(key))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn digest_of(text: &str) -> String {
    format!("sha256:{}", sha256_hex(text.as_bytes()))
}

fn write_atomic(file: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| format!("{}: {e}", file.display()))
}

/// Stores `draft` with its tests' `contents` (path, text); each content
/// must hash to its test's digest. Files of tests no longer listed go.
pub fn store(
    project_state: &Path,
    unit: &str,
    draft: &Draft,
    contents: &[(String, String)],
) -> Result<(), String> {
    let d = dir(project_state, unit, &draft.requirement.key);
    let _ = std::fs::remove_dir_all(d.join("files"));
    for t in &draft.tests {
        let content = contents
            .iter()
            .find(|(p, _)| *p == t.path)
            .map(|(_, c)| c)
            .ok_or_else(|| format!("{} has no content", t.path))?;
        if digest_of(content) != t.sha256 {
            return Err(format!("{} does not match its digest", t.path));
        }
        let file = d.join("files").join(&t.path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, content).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    let text = serde_json::to_string_pretty(draft).map_err(|e| e.to_string())?;
    write_atomic(&d.join("draft.json"), &text)
}

/// A requirement's draft and its tests' contents, every hash checked;
/// `Ok(None)`: none was written.
pub fn load(project_state: &Path, unit: &str, key: &str) -> Result<Option<Loaded>, Tampered> {
    let d = dir(project_state, unit, key);
    let Ok(text) = std::fs::read_to_string(d.join("draft.json")) else {
        return Ok(None);
    };
    let draft: Draft = serde_json::from_str(&text)
        .map_err(|e| Tampered(format!("the oracle draft of {key} is not valid: {e}")))?;
    let mut contents = Vec::new();
    for t in &draft.tests {
        let content = std::fs::read_to_string(d.join("files").join(&t.path))
            .map_err(|_| Tampered(format!("the test {} of {key} is missing", t.path)))?;
        if digest_of(&content) != t.sha256 {
            return Err(Tampered(format!("the test {} of {key} changed", t.path)));
        }
        contents.push((t.path.clone(), content));
    }
    Ok(Some((draft, contents)))
}

/// What a freeze kept beside the record, if this machine has it.
#[must_use]
pub fn load_frozen(project_state: &Path, unit: &str, key: &str) -> Option<Frozen> {
    std::fs::read_to_string(dir(project_state, unit, key).join("frozen.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// The manifest digest of a visible set: `sha256` over its sorted
/// `path=sha256 argv=[…]` lines — what `review.json` and the tick name.
#[must_use]
pub fn manifest_digest<'a>(
    tests: impl IntoIterator<Item = (&'a str, &'a str, &'a [String])>,
) -> String {
    let mut lines: Vec<String> = tests
        .into_iter()
        .map(|(path, sha, argv)| {
            format!(
                "{path}={sha} argv={}",
                serde_json::to_string(argv).unwrap_or_default()
            )
        })
        .collect();
    lines.sort();
    digest_of(&lines.join("\n"))
}

impl Draft {
    /// The tests that can be frozen: red on the checkout's `HEAD`.
    pub fn red(&self) -> impl Iterator<Item = &DraftTest> {
        self.tests.iter().filter(|t| t.red_class.is_red())
    }

    /// The red set's manifest digest; `None` when nothing is red.
    #[must_use]
    pub fn manifest(&self) -> Option<String> {
        let red: Vec<&DraftTest> = self.red().collect();
        (!red.is_empty()).then(|| {
            manifest_digest(
                red.iter()
                    .map(|t| (t.path.as_str(), t.sha256.as_str(), t.argv.as_slice())),
            )
        })
    }

    /// Whether a scenario's reading is still open.
    pub fn open_ambiguities(&self) -> impl Iterator<Item = &Ambiguity> {
        self.ambiguities.iter().filter(|a| a.open())
    }

    /// Every reason and note a later author call is told.
    fn all_notes(&self) -> Vec<String> {
        self.rejected
            .iter()
            .map(|r| format!("Rejected `{}`: {}", r.path, r.reason))
            .chain(self.notes.iter().cloned())
            .collect()
    }
}

/// What the views need from the configuration.
#[derive(Debug, Clone, Copy)]
pub struct ViewConfig {
    /// `[review] oracle_samples`.
    pub samples: u8,
    /// `[acceptance] min_strength`, as a percentage.
    pub strength_target: Option<u8>,
    /// `[acceptance] held_out`.
    pub held_out: bool,
    /// `[acceptance] generate`.
    pub acceptance: bool,
}

impl ViewConfig {
    /// From `speccoder.toml` at or above `root` (defaults when absent).
    #[must_use]
    pub fn at(root: &Path) -> Self {
        let rc = crate::verify::find_config(root)
            .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
            .and_then(|t| substrate::run_config::parse(&t).ok())
            .unwrap_or_default();
        Self::of(&rc)
    }

    /// From a parsed configuration.
    #[must_use]
    pub fn of(rc: &substrate::run_config::RunConfig) -> Self {
        Self {
            samples: rc.oracle_samples,
            strength_target: rc.strength.min.map(|m| {
                // A share from 0 to 1, as a whole percentage.
                let percent = (m * 100.0).round().clamp(0.0, 100.0);
                format!("{percent:.0}").parse().unwrap_or(0)
            }),
            held_out: rc.held_out,
            acceptance: rc.acceptance,
        }
    }
}

fn test_view(t: &DraftTest, content: &str) -> OracleTest {
    OracleTest {
        path: t.path.clone(),
        content: content.to_owned(),
        scenarios: t.scenarios.clone(),
        red_class: crate::pipeline::class_word(t.red_class),
        red: t.red_class.is_red(),
        authored_by: t.authored_by,
        edited_by: t.edited_by.clone(),
    }
}

/// One requirement's oracle as the review shows it: the draft, the
/// frozen entry `review.json` holds (`entry`), and whether this machine
/// has the frozen set.
#[must_use]
pub fn view(
    project_state: &Path,
    unit: &str,
    req: &OracleRequirement,
    entry: Option<&OracleEntry>,
    cfg: ViewConfig,
) -> OracleView {
    let digest = req.digest();
    let mut v = OracleView {
        req: req.key.clone(),
        title: req.title.clone(),
        scenarios: req.scenarios.clone(),
        state: OracleState::None,
        frozen_on: None,
        frozen_by: None,
        frozen_at: None,
        tests: Vec::new(),
        discarded: Vec::new(),
        rejected: Vec::new(),
        uncovered: req.scenarios.iter().map(|s| s.id.clone()).collect(),
        untestable: Vec::new(),
        ambiguities: Vec::new(),
        held_out: HeldOutSummary {
            state: if cfg.held_out { "absent" } else { "off" }.to_owned(),
            count: 0,
            manifest_sha256: None,
            reason: None,
            discarded: Vec::new(),
        },
        strength_target: cfg.strength_target,
        calls: cfg.samples,
        ambiguity_check: cfg.samples >= 2,
        manifest_sha256: None,
        error: (!cfg.acceptance)
            .then(|| "acceptance tests are off ([acceptance] generate = false)".to_owned()),
    };
    let loaded = load(project_state, unit, &req.key);
    let draft = match loaded {
        Ok(d) => d,
        Err(t) => {
            v.state = OracleState::Failed;
            v.error = Some(t.0);
            None
        }
    };
    let current = draft
        .as_ref()
        .filter(|(d, _)| d.requirement.sha256 == digest);
    if let Some((d, contents)) = current {
        let content = |p: &str| {
            contents
                .iter()
                .find(|(q, _)| q == p)
                .map(|(_, c)| c.as_str())
                .unwrap_or_default()
        };
        v.tests = d
            .tests
            .iter()
            .map(|t| test_view(t, content(&t.path)))
            .collect();
        v.discarded.clone_from(&d.discarded);
        v.rejected.clone_from(&d.rejected);
        v.untestable.clone_from(&d.untestable.sample1);
        v.ambiguities.clone_from(&d.ambiguities);
        v.held_out = if cfg.held_out {
            d.held_out.clone()
        } else {
            HeldOutSummary {
                state: "off".to_owned(),
                count: 0,
                manifest_sha256: None,
                reason: None,
                discarded: Vec::new(),
            }
        };
        v.manifest_sha256 = d.manifest();
        let covered: Vec<&String> = d.red().flat_map(|t| t.scenarios.iter()).collect();
        v.uncovered.retain(|s| !covered.contains(&s));
        v.state = if d.open_ambiguities().next().is_some() {
            OracleState::Ambiguous
        } else if d.red().next().is_none() {
            OracleState::NotRed
        } else {
            OracleState::Written
        };
    } else if draft.is_some() {
        v.error = Some("written for an earlier wording: write them again".to_owned());
    }
    if let Some(e) = entry {
        if e.requirement_sha256 == digest {
            v.frozen_by = Some(e.by.clone());
            v.frozen_at = Some(e.at);
            v.state = match crate::pre_frozen::load(project_state, unit, &req.key) {
                Ok(Some((rec, files))) => {
                    // The card shows what the run uses: the frozen set,
                    // never a draft changed since (P4).
                    let drafted = current.map(|(d, _)| d);
                    v.tests = rec
                        .visible
                        .iter()
                        .map(|t| {
                            let same = drafted.and_then(|d| {
                                d.tests
                                    .iter()
                                    .find(|x| x.path == t.path && x.sha256 == t.sha256)
                            });
                            OracleTest {
                                path: t.path.clone(),
                                content: files
                                    .iter()
                                    .find(|(p, _)| *p == t.path)
                                    .map(|(_, c)| c.clone())
                                    .unwrap_or_default(),
                                scenarios: t.cites.clone(),
                                red_class: same.map_or_else(
                                    || crate::pipeline::class_word(RedClass::Assertion),
                                    |x| crate::pipeline::class_word(x.red_class),
                                ),
                                red: true,
                                authored_by: same.map_or(AuthoredBy::Model, |x| x.authored_by),
                                edited_by: same.and_then(|x| x.edited_by.clone()),
                            }
                        })
                        .collect();
                    v.manifest_sha256 = Some(e.manifest_sha256.clone());
                    let covered: Vec<&String> =
                        rec.visible.iter().flat_map(|t| t.cites.iter()).collect();
                    v.uncovered = req
                        .scenarios
                        .iter()
                        .map(|s| s.id.clone())
                        .filter(|s| !covered.contains(&s))
                        .collect();
                    if cfg.held_out {
                        v.held_out = match &rec.held_out_sealed {
                            Some(h) => HeldOutSummary {
                                state: "kept".to_owned(),
                                count: h.count,
                                manifest_sha256: Some(format!("sha256:{}", h.manifest_sha256)),
                                reason: None,
                                discarded: Vec::new(),
                            },
                            None => HeldOutSummary {
                                state: "absent".to_owned(),
                                count: 0,
                                manifest_sha256: None,
                                reason: Some("none_kept".to_owned()),
                                discarded: Vec::new(),
                            },
                        };
                    }
                    OracleState::Frozen
                }
                Ok(None) => OracleState::FrozenElsewhere,
                Err(t) => {
                    v.error = Some(format!("the frozen set was tampered with: {}", t.0));
                    OracleState::Frozen
                }
            };
        } else if current.is_none() {
            v.state = OracleState::Lapsed;
            v.frozen_on = load_frozen(project_state, unit, &req.key).map(|f| f.wording);
            v.frozen_by = Some(e.by.clone());
            v.frozen_at = Some(e.at);
        }
    } else if let Ok(Some((rec, _))) = crate::pre_frozen::load(project_state, unit, &req.key)
        && rec.producer == Producer::Run
        && rec.is_current(&digest)
    {
        // Written by a run nobody reviewed: the story's oracle, frozen by it.
        v.state = OracleState::Frozen;
        v.frozen_by = Some(rec.decided.by.clone());
        v.frozen_at = Some(rec.decided.at);
    }
    v
}

/// Requirements of the selected unit whose tests two authors read
/// differently, with the scenario: `(REQ, scenario)`.
///
/// # Errors
/// A stored draft was tampered with or cannot be read: whether it holds an
/// open reading is unknown, never "none".
pub fn open_ambiguities(
    project_state: &Path,
    unit: &str,
    requirements: &[OracleRequirement],
) -> Result<Vec<(String, String)>, Tampered> {
    let mut out = Vec::new();
    for r in requirements {
        let Some((d, _)) = load(project_state, unit, &r.key)? else {
            continue;
        };
        if d.requirement.sha256 != r.digest() {
            continue;
        }
        out.extend(
            d.open_ambiguities()
                .map(|a| (d.requirement.key.clone(), a.scenario.clone())),
        );
    }
    Ok(out)
}

/// The rung of the selected unit, by the format's standing units.
#[must_use]
pub fn rung(format: &dyn Adapter, root: &Path, unit: &str) -> Rung {
    speccoder_core::ladder::rung_of(unit, &format.standing_units(root))
}

/// What one oracle action is asked.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The requirement.
    pub req: &'a str,
    /// What to do.
    pub op: OracleOp,
    /// The test (reject, again, edit).
    pub path: Option<&'a str>,
    /// The scenario (reading).
    pub scenario: Option<&'a str>,
    /// The reading (reading).
    pub reading: Option<Reading>,
    /// The reason, comment or content.
    pub text: Option<&'a str>,
}

impl Request<'_> {
    /// Whether it calls the test author.
    #[must_use]
    pub fn needs_model(&self) -> bool {
        matches!(self.op, OracleOp::Write | OracleOp::Again)
            || (self.op == OracleOp::Reading && self.reading == Some(Reading::B))
    }
}

/// Everything an oracle action works with.
struct Ctx {
    root: PathBuf,
    feature: String,
    dir: String,
    format: &'static dyn Adapter,
    snapshot: ProjectSnapshot,
    plane_dir: PathBuf,
    config: substrate::run_config::RunConfig,
    sandbox: sandbox::Config,
    worktree: crate::prepare::Config,
    /// speccoder's state directory (the cache `speccoder new` filled).
    state: PathBuf,
    policy: tools::policy::Policy,
    model: Option<substrate::model_config::ModelConfig>,
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new("review", status, message).with_code(code)
}

fn open(path: &Path, feature: Option<&str>, state: &Path) -> Result<Ctx, Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried("review", &inspected)));
    };
    let snapshot = (**snapshot).clone();
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            "review",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = PathBuf::from(&snapshot.root);
    let config_root = crate::verify::find_config(&root).ok_or_else(|| {
        Box::new(refuse(
            CommandStatus::ConfigError,
            "CONFIG-MISSING",
            "the project has no speccoder.toml",
        ))
    })?;
    let text =
        std::fs::read_to_string(config_root.join(crate::verify::config::FILE)).unwrap_or_default();
    let config = substrate::run_config::parse(&text).map_err(|e| {
        Box::new(refuse(
            CommandStatus::ConfigError,
            "CONFIG-INVALID",
            &format!("speccoder.toml: {e}"),
        ))
    })?;
    let sandbox = sandbox::parse(&text).map_err(|e| {
        Box::new(refuse(
            CommandStatus::ConfigError,
            "CONFIG-INVALID",
            &format!("speccoder.toml: {e}"),
        ))
    })?;
    let worktree_config = crate::prepare::parse(&text).map_err(|e| {
        Box::new(refuse(
            CommandStatus::ConfigError,
            "CONFIG-INVALID",
            &format!("speccoder.toml: {e}"),
        ))
    })?;
    let format = crate::format::of(&snapshot);
    let policy =
        tools::policy::Policy::new(&crate::verify::check_argvs(&config_root), &config.allow)
            .with_prefixes(config.allow_prefix.clone())
            .rules(format.rules());
    let model = substrate::model_config::parse(&text).ok().flatten();
    Ok(Ctx {
        feature: f.feature.name.clone(),
        dir: f.feature.dir.clone(),
        plane_dir: worktree::project_state_dir(state, &root),
        root,
        format,
        snapshot,
        config,
        sandbox,
        worktree: worktree_config,
        state: state.to_path_buf(),
        policy,
        model,
    })
}

/// The project's directory inside a worktree of its repository.
fn project_in(root: &Path, worktree_root: &Path) -> PathBuf {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let top = crate::git::Git::at(root)
        .toplevel()
        .ok()
        .flatten()
        .map_or_else(|| root.to_path_buf(), |t| canonical(&t));
    match canonical(root).strip_prefix(&top) {
        Ok(rel) if !rel.as_os_str().is_empty() => worktree_root.join(rel),
        _ => worktree_root.to_path_buf(),
    }
}

/// One writer of a unit's oracle at a time: *Write Tests* and its kin, and
/// the freeze of *Approve and Run*. A holder elsewhere is refused, never
/// waited for (a write runs model calls).
pub(crate) struct UnitLock(PathBuf);

impl UnitLock {
    pub(crate) fn take(project_state: &Path, unit: &str) -> Result<Self, String> {
        let path = project_state.join(DIR).join(format!("{unit}.lock"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        // A lock older than an hour is a crashed writer's.
        if std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 3600)
        {
            let _ = std::fs::remove_file(&path);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| Self(path))
            .map_err(|_| {
                format!("{unit}'s tests are being written or frozen elsewhere right now: look again when that ends")
            })
    }
}

impl Drop for UnitLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What a progress sink is told: a stage, by name and count.
fn stage(options: &crate::RunOptions, req: &str, stage: &str, n: u32, of: u32) {
    if let Some(p) = &options.progress {
        p.event(&crate::progress::Event::Oracle {
            step: crate::progress::OracleEvent::Stage {
                requirement: req.to_owned(),
                stage: stage.to_owned(),
                n,
                of,
            },
        });
    }
}

/// `speccoder review tests <REQ> …` with the configured model when the
/// action calls the test author.
#[must_use]
pub fn tests(
    path: &Path,
    feature: Option<&str>,
    request: &Request<'_>,
    via: Via,
    state: Option<&Path>,
    options: &crate::RunOptions,
) -> CommandResult {
    let Some(state) = state.map(Path::to_path_buf).or_else(worktree::state_dir) else {
        return refuse(
            CommandStatus::ConfigError,
            "NO-STATE-DIR",
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    if !request.needs_model() {
        return tests_with(path, feature, request, via, &state, None, options);
    }
    let ctx = match open(path, feature, &state) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if let Err(r) = check(&ctx, request) {
        return *r;
    }
    let Some(model) = ctx.model.as_ref() else {
        return refuse(
            CommandStatus::ConfigError,
            "CONFIG",
            "speccoder.toml has no [model] section: the test author needs a model",
        );
    };
    match substrate::model_config::open_driver(model) {
        Ok((provider, _)) => {
            tests_with(path, feature, request, via, &state, Some(provider), options)
        }
        Err(e) => substrate::model_config::refused("review", &e),
    }
}

/// [`tests`] against `state`, with `provider` for the author calls.
#[must_use]
pub fn tests_with(
    path: &Path,
    feature: Option<&str>,
    request: &Request<'_>,
    via: Via,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    options: &crate::RunOptions,
) -> CommandResult {
    let ctx = match open(path, feature, state) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let req = match check(&ctx, request) {
        Ok(r) => r,
        Err(r) => return *r,
    };
    let _lock = match UnitLock::take(&ctx.plane_dir, &ctx.feature) {
        Ok(l) => l,
        Err(e) => return refuse(CommandStatus::GitPrecondition, "RUN-ACTIVE", &e),
    };
    let who = crate::approvals::Who::local_in(&ctx.root, via);
    let done = match request.op {
        OracleOp::Write => write(&ctx, &req, provider, options),
        OracleOp::Reject => reject(&ctx, &req, request, &who.name),
        OracleOp::Again => again(&ctx, &req, request, &who.name, provider, options),
        OracleOp::Edit => edit(&ctx, &req, request, &who.name),
        OracleOp::Reading => reading(&ctx, &req, request, &who.name, provider, options),
    };
    match done {
        Ok(message) => {
            let r = crate::review::report_in(path, feature, Some(state));
            let status = r.status();
            r.reworded(status, &message)
        }
        Err(r) => *r,
    }
}

/// What refuses an oracle action before anything runs: the unit's rung,
/// acceptance off, an unknown requirement or one without scenarios.
fn check(ctx: &Ctx, request: &Request<'_>) -> Result<OracleRequirement, Box<CommandResult>> {
    let unit_rung = rung(ctx.format, &ctx.root, &ctx.feature);
    if unit_rung != Rung::Feature {
        return Err(Box::new(
            refuse(
                CommandStatus::UsageError,
                "REVIEW-TESTS-OWN-PIPELINE",
                &format!(
                    "{} is a {} unit: its tests are written and confirmed by its own pipeline",
                    ctx.feature,
                    if unit_rung == Rung::Quick {
                        "quick-change"
                    } else {
                        "bug"
                    }
                ),
            )
            .with_fix(vec![if unit_rung == Rung::Quick {
                "speccoder do \"…\"".to_owned()
            } else {
                format!("speccoder bug confirm {}/<task>", ctx.feature)
            }]),
        ));
    }
    if !ctx.config.acceptance {
        return Err(Box::new(refuse(
            CommandStatus::ConfigError,
            "ACCEPTANCE-OFF",
            "acceptance tests are off ([acceptance] generate = false): no tests are written at review",
        )));
    }
    let requirements = ctx.format.oracle_requirements(&ctx.snapshot);
    let Some(req) = requirements
        .iter()
        .find(|r| r.key.eq_ignore_ascii_case(request.req))
        .cloned()
    else {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "REVIEW-NO-SCENARIOS",
            &format!(
                "the spec has no requirement {}; it names {}",
                request.req,
                if requirements.is_empty() {
                    "none with scenarios".to_owned()
                } else {
                    requirements
                        .iter()
                        .map(|r| r.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        )));
    };
    if req.scenarios.is_empty() {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "REVIEW-NO-SCENARIOS",
            &format!(
                "{} has no acceptance scenarios to write tests from",
                req.key
            ),
        )));
    }
    // A frozen set is what the run uses, sealed with its held-out set: it
    // is not rewritten under the approval that names it (C4).
    let (entries, _) = crate::review::oracle_entries(&ctx.root, &ctx.dir)
        .map_err(|e| Box::new(refuse(CommandStatus::NeedsHuman, "REVIEW-UNREADABLE", &e)))?;
    if let Some(e) = entries
        .get(&req.key)
        .filter(|e| e.requirement_sha256 == req.digest())
    {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "ORACLE-FROZEN",
            &format!(
                "{}'s tests were frozen with the approval by {}: the run uses them as frozen — reword {} to lapse the freeze, then write its tests again",
                req.key, e.by, req.key
            ),
        )));
    }
    Ok(req)
}

type Outcome = Result<String, Box<CommandResult>>;

fn failed(message: impl Into<String>) -> Box<CommandResult> {
    Box::new(refuse(
        CommandStatus::RunFailed,
        "ORACLE-AUTHOR-FAILED",
        &message.into(),
    ))
}

fn usage(message: &str) -> Box<CommandResult> {
    Box::new(refuse(CommandStatus::UsageError, "USAGE", message))
}

fn existing(ctx: &Ctx, req: &OracleRequirement) -> Result<Loaded, Box<CommandResult>> {
    match load(&ctx.plane_dir, &ctx.feature, &req.key) {
        Ok(Some((d, c))) if d.requirement.sha256 == req.digest() => Ok((d, c)),
        Ok(Some(_)) => Err(usage(&format!(
            "{}'s tests were written for an earlier wording: write them again (`speccoder review tests {} --write`)",
            req.key, req.key
        ))),
        Ok(None) => Err(usage(&format!(
            "{} has no tests yet: `speccoder review tests {} --write`",
            req.key, req.key
        ))),
        Err(t) => Err(Box::new(refuse(
            CommandStatus::NeedsHuman,
            "ORACLE-TAMPERED",
            &t.0,
        ))),
    }
}

/// The plane and runtime of the author calls.
/// Owned at review; lent by the run whose plane already holds the journal.
enum Held<'a, T> {
    Own(T),
    Lent(&'a T),
}

impl<T> std::ops::Deref for Held<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Own(t) => t,
            Self::Lent(t) => t,
        }
    }
}

struct Author<'a> {
    ctx: &'a Ctx,
    rt: Held<'a, tokio::runtime::Runtime>,
    plane: Held<'a, substrate::Plane>,
    bundle: Value,
    instructions: Vec<crate::instructions::Instruction>,
    /// The test layout's text: where the review's files are placed.
    test_layout: String,
    /// Every call's token usage, summed: what the oracle cost.
    spent: std::cell::RefCell<Option<Value>>,
}

impl<'a> Author<'a> {
    fn open(
        ctx: &'a Ctx,
        req: &OracleRequirement,
        provider: Option<Arc<dyn ModelProvider>>,
        options: &crate::RunOptions,
    ) -> Result<Self, Box<CommandResult>> {
        let (Some(provider), Some(model)) = (provider, ctx.model.as_ref()) else {
            return Err(Box::new(refuse(
                CommandStatus::ConfigError,
                "CONFIG",
                "the test author needs a model ([model] in speccoder.toml)",
            )));
        };
        let rt = crate::pipeline::tokio_runtime().map_err(failed)?;
        let plane = rt
            .block_on(async {
                substrate::Plane::open_with(
                    &ctx.plane_dir,
                    model,
                    provider,
                    None,
                    substrate::Hooks {
                        format: Some(ctx.format),
                        progress: options.progress.clone(),
                        ..substrate::Hooks::default()
                    },
                )
            })
            .map_err(|e| failed(e.0))?;
        Ok(Self::with(ctx, req, Held::Own(rt), Held::Own(plane)))
    }

    /// The author of a run: the run's runtime and plane, lent.
    fn lent(
        ctx: &'a Ctx,
        req: &OracleRequirement,
        rt: &'a tokio::runtime::Runtime,
        plane: &'a substrate::Plane,
    ) -> Self {
        Self::with(ctx, req, Held::Lent(rt), Held::Lent(plane))
    }

    fn with(
        ctx: &'a Ctx,
        req: &OracleRequirement,
        rt: Held<'a, tokio::runtime::Runtime>,
        plane: Held<'a, substrate::Plane>,
    ) -> Self {
        let unit_dir = ctx.root.join(&ctx.dir);
        let [constitution, _, plan, _] = ctx.format.convergence_texts(&ctx.root, &unit_dir);
        let bundle = json!({
            "feature": ctx.feature,
            "constitution": constitution,
            "plan": plan,
            "story": req.text,
            "task": {
                "id": format!("req-{}", req.key),
                "title": format!("Acceptance tests for {}: {}", req.key, req.title),
                "phase": "Review",
                "subsection": "",
                "story": req.key,
                "parallel": false,
            },
            "dependencies": [],
        });
        let instructions =
            crate::instructions::collect(&ctx.root, &ctx.root, &[], &ctx.config.instructions);
        Self {
            ctx,
            rt,
            plane,
            bundle,
            instructions,
            test_layout: crate::layout::text_at(&ctx.root),
            spent: std::cell::RefCell::new(None),
        }
    }

    /// The parts a call is handed, as its context manifest lists them.
    fn context(
        &self,
        req: &OracleRequirement,
        notes: &[String],
    ) -> speccoder_core::context::ContextManifest {
        let mut m = crate::context::Manifest::new();
        m.push(
            ContextKind::Story,
            "person",
            "/requirement",
            req.text.as_bytes(),
        );
        for s in &req.scenarios {
            m.push(
                ContextKind::Scenario,
                "person",
                &format!("/scenarios/{}", s.id),
                s.text.as_bytes(),
            );
        }
        m.push(
            ContextKind::Constitution,
            "worktree",
            "/bundle/constitution",
            self.bundle["constitution"]
                .as_str()
                .unwrap_or_default()
                .as_bytes(),
        );
        m.push(
            ContextKind::Plan,
            "worktree",
            "/bundle/plan",
            self.bundle["plan"].as_str().unwrap_or_default().as_bytes(),
        );
        m.push(
            ContextKind::TestLayout,
            "worktree",
            "/test_layout",
            self.test_layout.as_bytes(),
        );
        for (i, n) in notes.iter().enumerate() {
            let kind = if n.starts_with("Rejected `") {
                ContextKind::Rejection
            } else if n.starts_with("Comment ") {
                ContextKind::Comment
            } else {
                ContextKind::Note
            };
            m.push(kind, "person", &format!("/notes/{i}"), n.as_bytes());
        }
        m.finish()
    }

    /// One author call: its answer, run id and context.
    #[allow(clippy::too_many_arguments)]
    fn call(
        &self,
        req: &OracleRequirement,
        files: &[String],
        manifest: &str,
        notes: &[String],
        refused: &[String],
        again: Option<(&str, &str)>,
        sample: u8,
        places: &[substrate::acceptance::ScenarioPlace],
    ) -> Result<(Value, String, Vec<ContextEntry>), String> {
        let context = self.context(req, notes);
        let suite = crate::pipeline::acceptance::suite(&self.ctx.root, &req.text, &[]);
        let interface = crate::pipeline::acceptance::interface(&self.ctx.root, &req.text);
        let commands: Vec<String> = if crate::layout::via_command(&req.text) {
            crate::layout::commands(&self.ctx.root)
                .into_iter()
                .map(|c| c.shown)
                .collect()
        } else {
            Vec::new()
        };
        let input = substrate::acceptance::review_input(&substrate::acceptance::ReviewParts {
            bundle: self.bundle.clone(),
            requirement: json!({ "key": req.key, "title": req.title, "text": req.text }),
            scenarios: &req.scenarios,
            files,
            manifest,
            notes,
            again,
            instructions: &self.instructions,
            context: &context,
            sample,
            test_layout: Some(&self.test_layout),
            suite: &suite,
            interface: &interface,
            commands: &commands,
            refused,
            places,
        });
        let task = format!("req-{}", req.key);
        let outcome = self
            .rt
            .block_on(self.plane.acceptance(&self.ctx.feature, &task, input))
            .map_err(|e| format!("the test author failed: {}", e.0))?;
        let output = outcome
            .output
            .as_ref()
            .map(|o| o.peek().clone())
            .unwrap_or_default();
        let answer = output["answer"].clone();
        if !matches!(outcome.status, agentplane::prelude::RunStatus::Succeeded) || answer.is_null()
        {
            let status = format!("{:?}", outcome.status);
            return Err(substrate::provider_refusal(&status).map_or_else(
                || {
                    format!(
                        "the test author did not answer: {}",
                        substrate::why_not(&outcome.status)
                    )
                },
                |r| format!("the model provider refused the request: {r}"),
            ));
        }
        let total =
            crate::pipeline::commit::add_usage(self.spent.borrow().as_ref(), output.get("usage"));
        *self.spent.borrow_mut() = total;
        Ok((answer, outcome.run_id.to_string(), context.entries))
    }
}

/// What one author states it expects of each scenario, and what it found
/// untestable, by the requirement's own scenario ids (case aside). `Err`:
/// an answer the comparison cannot read — not parseable, naming a scenario
/// the requirement does not have, or saying nothing of any scenario — is
/// an author failure, never "no ambiguity".
fn expectations(
    answer: &Value,
    req: &OracleRequirement,
) -> Result<(Vec<Expectation>, Vec<Untestable>), String> {
    let canonical = |id: &str| {
        req.scenarios
            .iter()
            .find(|s| s.id.eq_ignore_ascii_case(id.trim()))
            .map(|s| s.id.clone())
            .ok_or_else(|| {
                format!(
                    "the test author's expectations name `{id}`, which {} does not have",
                    req.key
                )
            })
    };
    let list = |field: &str| -> Result<Value, String> {
        match &answer[field] {
            Value::Null => Ok(Value::Array(Vec::new())),
            v @ Value::Array(_) => Ok(v.clone()),
            _ => Err(format!("the test author's `{field}` is not a list")),
        }
    };
    let mut e: Vec<Expectation> = serde_json::from_value(list("expectations")?)
        .map_err(|x| format!("the test author's expectations cannot be read: {x}"))?;
    let mut u: Vec<Untestable> = serde_json::from_value(list("untestable")?)
        .map_err(|x| format!("the test author's untestable list cannot be read: {x}"))?;
    for x in &mut e {
        x.scenario = canonical(&x.scenario)?;
    }
    for x in &mut u {
        x.scenario = canonical(&x.scenario)?;
    }
    if e.is_empty() && u.is_empty() {
        return Err(
            "the test author stated no expectation for any scenario: the readings cannot be compared"
                .to_owned(),
        );
    }
    Ok((e, u))
}

/// A scratch worktree of the checkout's `HEAD`, its project directory and
/// a sandboxed runner in it.
struct Bench {
    /// The worktree the tests are checked in: a scratch one at review, the
    /// task's own at a run (kept alive with it).
    root: PathBuf,
    _scratch: Option<worktree::Scratch>,
    project: PathBuf,
    runner: sandbox::Runner,
    tmp: PathBuf,
}

impl Bench {
    fn open(ctx: &Ctx) -> Result<Self, Box<CommandResult>> {
        let tmp = ctx.plane_dir.join("tmp");
        let scratch = worktree::scratch(&ctx.root, &tmp)
            .map_err(|e| failed(format!("the scratch worktree could not be made: {}", e.0)))?;
        let project = project_in(&ctx.root, &scratch.path);
        let (runner, notice) = sandbox::runner(&scratch.path, &ctx.sandbox)
            .map_err(|e| Box::new(refuse(CommandStatus::ConfigError, "CONFIG", &e)))?;
        // Model-written tests and model-chosen commands run here, before
        // anyone approved them: only confined by the OS sandbox.
        if !runner.is_sandboxed() {
            return Err(Box::new(
                refuse(
                    CommandStatus::ConfigError,
                    "SANDBOX-UNAVAILABLE",
                    &format!(
                        "the red check runs model-written tests and needs the OS sandbox: {}",
                        notice.unwrap_or_else(|| "commands would run unsandboxed".to_owned())
                    ),
                )
                .with_fix(vec![
                    "set [sandbox] mode = \"auto\" on a machine with a sandbox backend (speccoder doctor)".to_owned(),
                ]),
            ));
        }
        // Its copied files and dependency folders, as a run's worktree gets them.
        crate::include::copy(&scratch.path, &project, &ctx.root, &ctx.worktree.include).map_err(
            |e| {
                Box::new(refuse(
                    CommandStatus::ConfigError,
                    "PREPARE-FAILED",
                    &format!("the files [worktree] include names could not be copied: {e}"),
                ))
            },
        )?;
        if ctx.worktree.prepare && crate::prepare::needed(&project) {
            crate::prepare::prepare(&crate::prepare::Prepare {
                worktree: &scratch.path,
                project: &project,
                config: &ctx.worktree,
                runner: &runner,
                state: Some(&ctx.state),
            })
            .map_err(|f| {
                Box::new(
                    refuse(CommandStatus::ConfigError, "PREPARE-FAILED", &f.message)
                        .with_fix(f.fix)
                        .with_commands(f.commands),
                )
            })?;
        }
        Ok(Self {
            root: scratch.path.clone(),
            _scratch: Some(scratch),
            project,
            runner,
            tmp,
        })
    }

    /// The bench of a run: the task's own worktree (`wt_root`), already
    /// prepared, at the tree the task starts from, with the task's runner.
    fn here(ctx: &Ctx, wt_root: &Path, runner: sandbox::Runner) -> Self {
        Self {
            root: wt_root.to_path_buf(),
            _scratch: None,
            project: ctx.root.clone(),
            runner,
            tmp: ctx.plane_dir.join("tmp"),
        }
    }

    fn files(&self) -> (Vec<String>, String) {
        let files: Vec<String> = worktree::checkpoint(&self.root, &self.tmp)
            .and_then(|c| worktree::files(&self.root, &c))
            .unwrap_or_default()
            .into_iter()
            .take(400)
            .collect();
        let manifest = crate::pipeline::MANIFESTS
            .iter()
            .find_map(|m| {
                std::fs::read_to_string(self.project.join(m))
                    .ok()
                    .map(|t| format!("{m}:\n{t}"))
            })
            .map(|t| t.chars().take(8192).collect::<String>())
            .unwrap_or_default();
        (files, manifest)
    }

    /// Red-checks each test alone: only its own file is in the
    /// scratch tree while its command runs — first without it, then twice
    /// with it — so a failure is its own, never a neighbour's. A command
    /// that is red without the file (a failing suite, a command that runs
    /// more than the file) decides nothing about it: `Err("red_without_it")`.
    fn red(
        &self,
        tests: &[(String, String, Vec<String>)],
    ) -> Result<Vec<Result<RedClass, &'static str>>, String> {
        let mut out = Vec::new();
        for (path, content, argv) in tests {
            let full = self.project.join(path);
            let _ = std::fs::remove_file(&full);
            let run = || crate::pipeline::acceptance_run(&self.project, argv, &self.runner);
            let (bare, summary) = run();
            if classify_red(&bare, summary.as_ref()).is_red() {
                out.push(Err("red_without_it"));
                continue;
            }
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{path}: {e}"))?;
            }
            std::fs::write(&full, content).map_err(|e| format!("{path}: {e}"))?;
            let (first, summary) = run();
            let said = format!("{}\n{}", first.stdout_tail, first.stderr_tail);
            if crate::integrity::invalid_escape(&said, &[path.as_str()]).is_some() {
                let _ = std::fs::remove_file(&full);
                out.push(Err("invalid_escape"));
                continue;
            }
            let class = classify_red(&first, summary.as_ref());
            let class = if !class.is_red() {
                class
            } else {
                let (second, summary) = run();
                if classify_red(&second, summary.as_ref()).is_red() {
                    class
                } else {
                    RedClass::Flaky
                }
            };
            std::fs::remove_file(&full).map_err(|e| format!("{path}: {e}"))?;
            out.push(Ok(class));
        }
        Ok(out)
    }
}

/// The answer's files as tests to keep, with what was discarded and why.
fn keep(
    ctx: &Ctx,
    bench: &Bench,
    answer: &Value,
    req: &OracleRequirement,
    taken: &[(String, Vec<String>)],
) -> (Vec<Kept>, Vec<OracleDiscarded>) {
    let (valid, refused) =
        crate::pipeline::validate_answer(answer, &bench.project, &ctx.policy, &req.scenarios, None);
    let mut discarded: Vec<OracleDiscarded> = refused
        .into_iter()
        .map(|r| OracleDiscarded {
            path: r.path,
            why: r.why.to_owned(),
        })
        .collect();
    let set_argv: Vec<String> = crate::pipeline::acceptance::argv_of(&answer["argv"]);
    let layout = crate::layout::detect(&bench.project, &crate::verify::check_argvs(&ctx.root));
    // The story's interface is the product's command: a test of it runs it.
    let commands = crate::layout::commands(&bench.project);
    let via_command = !commands.is_empty() && crate::layout::via_command(&req.text);
    let mut kept = Vec::new();
    for f in valid {
        let mut discard = |why: &str| {
            discarded.push(OracleDiscarded {
                path: f.path.clone(),
                why: why.to_owned(),
            });
        };
        if f.cited.is_empty() {
            discard("cites_no_scenario");
            continue;
        }
        if via_command && !commands.iter().any(|c| c.run_by(&f.content)) {
            discard("no_command");
            continue;
        }
        if taken.iter().any(|(p, _)| *p == f.path)
            || kept.iter().any(|(p, ..): &(String, _, _, _)| *p == f.path)
        {
            discard("exists");
            continue;
        }
        let argv = if f.argv.is_empty() {
            set_argv.clone()
        } else {
            f.argv.clone()
        };
        // One command per test: a command several tests share cannot say
        // whose assertion failed (at the checkpoint they all exist).
        if taken.iter().any(|(_, a)| *a == argv)
            || kept
                .iter()
                .any(|(.., a): &(_, _, _, Vec<String>)| *a == argv)
        {
            discard("command_shared");
            continue;
        }
        if argv.is_empty()
            || !crate::pipeline::test_command_allowed(
                &ctx.policy,
                Some(&layout),
                &argv,
                &[f.path.as_str()],
            )
        {
            discard("command_not_allowed");
            continue;
        }
        if kept.len() + taken.len() >= LIMIT {
            discard("limit");
            continue;
        }
        kept.push((f.path, f.content, f.cited, argv));
    }
    (kept, discarded)
}

/// *Write Tests*: the author calls, the comparison, the red check and the
/// held-out set; the draft is stored only when the first call answered.
fn write(
    ctx: &Ctx,
    req: &OracleRequirement,
    provider: Option<Arc<dyn ModelProvider>>,
    options: &crate::RunOptions,
) -> Outcome {
    let samples = ctx.config.oracle_samples.clamp(1, 2);
    let author = Author::open(ctx, req, provider, options)?;
    let bench = Bench::open(ctx)?;
    // What the person said about earlier tests is told again; a draft
    // that cannot be read is not overwritten with its rejections lost.
    let earlier = load(&ctx.plane_dir, &ctx.feature, &req.key).map_err(|t| {
        Box::new(
            refuse(
                CommandStatus::NeedsHuman,
                "ORACLE-TAMPERED",
                &format!(
                    "{}: writing anew would lose the rejections and notes it holds",
                    t.0
                ),
            )
            .with_fix(vec![format!(
                "→ restore it, or remove {} to start {}'s tests over",
                dir(&ctx.plane_dir, &ctx.feature, &req.key).display(),
                req.key
            )]),
        )
    })?;
    let (rejected, mut notes) = earlier
        .as_ref()
        .filter(|(d, _)| d.requirement.sha256 == req.digest())
        .map(|(d, _)| (d.rejected.clone(), d.notes.clone()))
        .unwrap_or_default();
    notes.dedup();
    let told: Vec<String> = rejected
        .iter()
        .map(|r| format!("Rejected `{}`: {}", r.path, r.reason))
        .chain(notes.iter().cloned())
        .collect();
    let (draft, contents) = draft(
        ctx,
        req,
        &author,
        &bench,
        samples,
        &told,
        &[],
        rejected,
        notes,
        true,
        options,
    )?;
    store(&ctx.plane_dir, &ctx.feature, &draft, &contents).map_err(failed)?;
    let red = draft.red().count();
    let open = draft.open_ambiguities().count();
    Ok(format!(
        "wrote {} test(s) for {}: {red} red{}{}",
        draft.tests.len(),
        req.key,
        if draft.held_out.state == "kept" {
            format!(", {} held out", draft.held_out.count)
        } else {
            String::new()
        },
        if open > 0 {
            format!(
                "; {} — {open} scenario(s) read two ways: choose a reading or reword (`speccoder review tests {} --reading A|B <scenario>`)",
                req.key, req.key
            )
        } else {
            String::new()
        }
    ))
}

/// Each scenario's file, named after it where the runner collects tests
/// (`US2-1` → `tests/test_acceptance_us2_1.py`) and free in `project`: a
/// file there already (an older task's test) is never the oracle's.
fn places(
    project: &Path,
    root: &Path,
    req: &OracleRequirement,
) -> Vec<substrate::acceptance::ScenarioPlace> {
    let layout = crate::layout::detect(project, &crate::verify::check_argvs(root));
    req.scenarios
        .iter()
        .filter_map(|s| {
            let name = s.id.replace('-', "_");
            let path = (1..10)
                .filter_map(|n| {
                    let name = if n == 1 {
                        name.clone()
                    } else {
                        format!("{name}_{n}")
                    };
                    layout.acceptance_file(&name, None)
                })
                .find(|p| !project.join(p).exists())?;
            Some(substrate::acceptance::ScenarioPlace {
                scenario: s.id.clone(),
                command: layout.command_for(&path).unwrap_or_default(),
                path,
            })
        })
        .collect()
}

/// One author pass over `req` (`samples` calls), its tests red-checked on
/// `bench`, the held-out set sealed: the draft and its tests' contents.
#[allow(clippy::too_many_arguments)]
fn draft(
    ctx: &Ctx,
    req: &OracleRequirement,
    author: &Author<'_>,
    bench: &Bench,
    samples: u8,
    told: &[String],
    refused: &[String],
    rejected: Vec<OracleRejected>,
    notes: Vec<String>,
    with_held_out: bool,
    options: &crate::RunOptions,
) -> Result<Loaded, Box<CommandResult>> {
    let (files, manifest) = bench.files();
    let places = places(&bench.project, &ctx.root, req);
    let of = u32::from(samples);
    stage(options, &req.key, "writing", 1, of);
    let (first, run1, mut context) = author
        .call(req, &files, &manifest, told, refused, None, 1, &places)
        .map_err(failed)?;
    // The comparison needs both readings; with one sample they are kept
    // as stated, for the card.
    let read = |answer: &Value| {
        let got = expectations(answer, req);
        if samples >= 2 {
            got.map_err(failed)
        } else {
            Ok(got.unwrap_or_default())
        }
    };
    let (e1, u1) = read(&first)?;
    let mut runs = vec![run1];
    let (mut e2, mut u2) = (Vec::new(), Vec::new());
    if samples >= 2 {
        stage(options, &req.key, "writing", 2, of);
        let (second, run2, more) = author
            .call(req, &files, &manifest, told, refused, None, 2, &places)
            .map_err(failed)?;
        (e2, u2) = read(&second)?;
        runs.push(run2);
        for c in more {
            if !context.contains(&c) {
                context.push(c);
            }
        }
        stage(options, &req.key, "comparing", 1, 1);
    }
    let ambiguities = if samples >= 2 {
        speccoder_core::oracle::compare(&e1, &u1, &e2, &u2)
    } else {
        Vec::new()
    };
    let (kept, mut discarded) = keep(ctx, bench, &first, req, &[]);
    stage(options, &req.key, "checking_red", 1, 1);
    let plan: Vec<(String, String, Vec<String>)> = kept
        .iter()
        .map(|(p, c, _, a)| (p.clone(), c.clone(), a.clone()))
        .collect();
    let classes = bench.red(&plan).map_err(failed)?;
    let mut tests: Vec<DraftTest> = Vec::new();
    let mut contents: Vec<(String, String)> = Vec::new();
    for ((path, content, cited, argv), class) in kept.into_iter().zip(classes) {
        match class {
            Ok(red_class) => {
                tests.push(DraftTest {
                    path: path.clone(),
                    sha256: digest_of(&content),
                    scenarios: cited,
                    argv,
                    red_class,
                    authored_by: AuthoredBy::Model,
                    edited_by: None,
                });
                contents.push((path, content));
            }
            Err(why) => discarded.push(OracleDiscarded {
                path,
                why: why.to_owned(),
            }),
        }
    }
    // An amendment keeps the story's sealed set: none is written for it.
    let (held_out, sealed) = if with_held_out {
        held_out(ctx, author, bench, req, &tests, options)
    } else {
        (
            HeldOutSummary {
                state: "off".to_owned(),
                count: 0,
                manifest_sha256: None,
                reason: None,
                discarded: Vec::new(),
            },
            None,
        )
    };
    let draft = Draft {
        requirement: DraftRequirement {
            key: req.key.clone(),
            title: req.title.clone(),
            text: req.text.clone(),
            sha256: req.digest(),
        },
        samples,
        tests,
        discarded,
        rejected,
        expectations: Samples {
            sample1: e1,
            sample2: e2,
        },
        untestable: Samples {
            sample1: u1,
            sample2: u2,
        },
        ambiguities,
        held_out,
        sealed,
        notes,
        runs,
        context,
    };
    Ok((draft, contents))
}

/// The held-out set at review, after a red visible set: authored as at
/// run time, red by overlay, sealed under `(unit, REQ)`; only its
/// count and digest come back.
fn held_out(
    ctx: &Ctx,
    author: &Author<'_>,
    bench: &Bench,
    req: &OracleRequirement,
    tests: &[DraftTest],
    options: &crate::RunOptions,
) -> (HeldOutSummary, Option<SealedRef>) {
    let summary = |state: &str, reason: Option<&str>| HeldOutSummary {
        state: state.to_owned(),
        count: 0,
        manifest_sha256: None,
        reason: reason.map(str::to_owned),
        discarded: Vec::new(),
    };
    if !ctx.config.held_out {
        return (summary("off", None), None);
    }
    let red: Vec<&DraftTest> = tests.iter().filter(|t| t.red_class.is_red()).collect();
    if red.is_empty() {
        return (summary("absent", Some("not_red")), None);
    }
    stage(options, &req.key, "held_out", 1, 1);
    let visible = crate::pipeline::PreparedAcceptance {
        report: json!({ "files": red.iter().map(|t| t.path.clone()).collect::<Vec<_>>() }),
        argv: red.first().map(|t| t.argv.clone()),
        more_argv: Vec::new(),
        red: true,
        red_class: Some(RedClass::Assertion),
        held_out: None,
        generation_failed: None,
        skip_cause: None,
    };
    let task = format!("req-{}", req.key);
    let store = crate::held_out::sealed_dir(&ctx.plane_dir, &ctx.feature, &req.key);
    let (report, sha) = crate::pipeline::prepare_held_out(
        &crate::pipeline::AcceptanceSetup {
            rt: &author.rt,
            plane: &author.plane,
            project: &bench.project,
            worktree: &bench.root,
            tmp: &bench.tmp,
            policy: &ctx.policy,
            feature: &ctx.feature,
            task: &task,
            bundle: author.bundle.clone(),
            scenarios: &req.scenarios,
            instructions: &author.instructions,
            runner: &bench.runner,
            layout: None,
            told: &[],
            related: &[],
            oracle: true,
        },
        &visible,
        &req.scenarios,
        &store,
        false,
    );
    // The held-out author's call is the oracle's cost too.
    let total =
        crate::pipeline::commit::add_usage(author.spent.borrow().as_ref(), report.get("usage"));
    *author.spent.borrow_mut() = total;
    match sha {
        Some(sha) => {
            let count = report["kept"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            (
                HeldOutSummary {
                    state: "kept".to_owned(),
                    count,
                    manifest_sha256: Some(format!("sha256:{sha}")),
                    reason: None,
                    discarded: Vec::new(),
                },
                Some(SealedRef {
                    key: req.key.clone(),
                    manifest_sha256: sha,
                    count,
                }),
            )
        }
        None => (
            HeldOutSummary {
                discarded: report["discarded"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d["reason"].as_str().map(str::to_owned))
                    .collect(),
                ..summary(
                    "absent",
                    Some(report["reason"].as_str().unwrap_or("none_kept")),
                )
            },
            None,
        ),
    }
}

fn reject(ctx: &Ctx, req: &OracleRequirement, r: &Request<'_>, by: &str) -> Outcome {
    let Some(path) = r.path else {
        return Err(usage("name the test to reject (--reject PATH)"));
    };
    let Some(reason) = r.text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Err(usage(
            "a rejection needs a reason: why is this test wrong? (--reason \"…\")",
        ));
    };
    let (mut d, contents) = existing(ctx, req)?;
    let Some(at) = d.tests.iter().position(|t| t.path == path) else {
        return Err(usage(&format!("{} has no test {path}", req.key)));
    };
    d.tests.remove(at);
    d.rejected.push(OracleRejected {
        path: path.to_owned(),
        reason: reason.to_owned(),
        by: by.to_owned(),
        at: now(),
    });
    store(&ctx.plane_dir, &ctx.feature, &d, &contents).map_err(failed)?;
    Ok(format!(
        "rejected {path} of {}: \"{reason}\" — later author calls for {} are told",
        req.key, req.key
    ))
}

fn edit(ctx: &Ctx, req: &OracleRequirement, r: &Request<'_>, by: &str) -> Outcome {
    let Some(path) = r.path else {
        return Err(usage("name the test to edit (--edit PATH)"));
    };
    let Some(content) = r.text.filter(|t| !t.trim().is_empty()) else {
        return Err(usage("an edit needs the test's new content (--from FILE)"));
    };
    let (mut d, mut contents) = existing(ctx, req)?;
    let Some(at) = d.tests.iter().position(|t| t.path == path) else {
        return Err(usage(&format!("{} has no test {path}", req.key)));
    };
    let bench = Bench::open(ctx)?;
    let plan = [(
        path.to_owned(),
        content.to_owned(),
        d.tests[at].argv.clone(),
    )];
    let checked = bench
        .red(&plan)
        .map_err(failed)?
        .pop()
        .unwrap_or(Ok(RedClass::Unclassified));
    // Red without the test: its command decides nothing about it.
    let class = checked.unwrap_or(RedClass::Unclassified);
    let t = &mut d.tests[at];
    t.sha256 = digest_of(content);
    t.red_class = class;
    t.authored_by = AuthoredBy::Person;
    t.edited_by = Some(by.to_owned());
    if let Some(c) = contents.iter_mut().find(|(p, _)| p == path) {
        c.1 = content.to_owned();
    }
    store(&ctx.plane_dir, &ctx.feature, &d, &contents).map_err(failed)?;
    Ok(format!(
        "edited {path} of {}: {}",
        req.key,
        if class.is_red() {
            format!("red ({})", crate::pipeline::class_word(class))
        } else if checked.is_err() {
            "its command is red without it — it decides nothing about this test, which cannot be frozen".to_owned()
        } else {
            format!(
                "not red ({}) — no evidence, it cannot be frozen",
                crate::pipeline::class_word(class)
            )
        }
    ))
}

/// Writes tests anew from one author call (`again`, a reading): the
/// answer's citing files replace `replaced`, red-checked beside the rest.
#[allow(clippy::too_many_arguments)]
fn rewrite(
    ctx: &Ctx,
    req: &OracleRequirement,
    mut d: Draft,
    mut contents: Vec<(String, String)>,
    replaced: &[String],
    note: String,
    again: Option<(&str, &str)>,
    only: Option<&str>,
    provider: Option<Arc<dyn ModelProvider>>,
    options: &crate::RunOptions,
) -> Result<(Draft, Vec<String>), Box<CommandResult>> {
    let author = Author::open(ctx, req, provider, options)?;
    let bench = Bench::open(ctx)?;
    let (files, manifest) = bench.files();
    d.notes.push(note);
    let told = d.all_notes();
    stage(options, &req.key, "writing", 1, 1);
    let (answer, run, context) = author
        .call(req, &files, &manifest, &told, &[], again, 1, &[])
        .map_err(failed)?;
    d.runs.push(run);
    for c in context {
        if !d.context.contains(&c) {
            d.context.push(c);
        }
    }
    let keep_paths: Vec<(String, Vec<String>)> = d
        .tests
        .iter()
        .filter(|t| !replaced.contains(&t.path))
        .map(|t| (t.path.clone(), t.argv.clone()))
        .collect();
    // The replaced test's own path may come back.
    let mut answer = answer;
    if let Some(files) = answer["files"].as_array_mut() {
        files.retain(|f| {
            only.is_none_or(|s| {
                f["scenarios"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|v| v.as_str() == Some(s)))
            })
        });
    }
    let (new, discarded) = keep(ctx, &bench, &answer, req, &keep_paths);
    if new.is_empty() {
        return Err(failed(format!(
            "the test author wrote no usable test{}",
            discarded
                .first()
                .map(|x| format!(" ({} {})", x.path, x.why))
                .unwrap_or_default()
        )));
    }
    d.tests.retain(|t| !replaced.contains(&t.path));
    contents.retain(|(p, _)| !replaced.contains(p));
    d.discarded.extend(discarded);
    stage(options, &req.key, "checking_red", 1, 1);
    // Each test is red-checked alone: the kept ones stand as checked.
    let plan: Vec<(String, String, Vec<String>)> = new
        .iter()
        .map(|(p, c, _, a)| (p.clone(), c.clone(), a.clone()))
        .collect();
    let classes = bench.red(&plan).map_err(failed)?;
    let mut written = Vec::new();
    for ((path, content, cited, argv), class) in new.into_iter().zip(classes) {
        let red_class = match class {
            Ok(c) => c,
            Err(why) => {
                d.discarded.push(OracleDiscarded {
                    path,
                    why: why.to_owned(),
                });
                continue;
            }
        };
        d.tests.push(DraftTest {
            sha256: digest_of(&content),
            scenarios: cited,
            argv,
            red_class,
            authored_by: AuthoredBy::Model,
            edited_by: None,
            path: path.clone(),
        });
        contents.push((path.clone(), content));
        written.push(path);
    }
    store(&ctx.plane_dir, &ctx.feature, &d, &contents).map_err(failed)?;
    Ok((d, written))
}

fn again(
    ctx: &Ctx,
    req: &OracleRequirement,
    r: &Request<'_>,
    _by: &str,
    provider: Option<Arc<dyn ModelProvider>>,
    options: &crate::RunOptions,
) -> Outcome {
    let Some(path) = r.path else {
        return Err(usage("name the test to write again (--again PATH)"));
    };
    let comment = r
        .text
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("write it again");
    let (d, contents) = existing(ctx, req)?;
    let Some(content) = contents
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, c)| c.clone())
    else {
        return Err(usage(&format!("{} has no test {path}", req.key)));
    };
    let (d, written) = rewrite(
        ctx,
        req,
        d,
        contents,
        &[path.to_owned()],
        format!("Comment on `{path}`: {comment}"),
        Some((path, &content)),
        None,
        provider,
        options,
    )?;
    let red = d
        .tests
        .iter()
        .filter(|t| written.contains(&t.path) && t.red_class.is_red())
        .count();
    Ok(format!(
        "wrote {path} of {} again: {}",
        req.key,
        if written.is_empty() {
            "nothing kept — its command is red without it, so it decides nothing".to_owned()
        } else if red == written.len() {
            "red".to_owned()
        } else {
            "not red — no evidence, it cannot be frozen".to_owned()
        }
    ))
}

fn reading(
    ctx: &Ctx,
    req: &OracleRequirement,
    r: &Request<'_>,
    by: &str,
    provider: Option<Arc<dyn ModelProvider>>,
    options: &crate::RunOptions,
) -> Outcome {
    let (Some(scenario), Some(choice)) = (r.scenario, r.reading) else {
        return Err(usage(
            "name the reading and the scenario (--reading A|B US1-2)",
        ));
    };
    let (mut d, contents) = existing(ctx, req)?;
    let Some(item) = d.ambiguities.iter_mut().find(|a| a.scenario == scenario) else {
        return Err(usage(&format!(
            "{} has no ambiguity on {scenario}",
            req.key
        )));
    };
    item.resolution = Some(Resolution {
        reading: choice,
        by: by.to_owned(),
        at: now(),
    });
    let meant = if choice == Reading::A {
        item.a.clone()
    } else {
        item.b.clone()
    };
    if choice == Reading::A {
        d.notes.push(format!(
            "For {scenario} the person means reading A: given {}, expect {}",
            meant.given, meant.expect
        ));
        store(&ctx.plane_dir, &ctx.feature, &d, &contents).map_err(failed)?;
        return Ok(format!(
            "{scenario} of {}: reading A is meant — the tests stay",
            req.key
        ));
    }
    let replaced: Vec<String> = d
        .tests
        .iter()
        .filter(|t| t.scenarios.iter().any(|s| s == scenario))
        .map(|t| t.path.clone())
        .collect();
    let (_, written) = rewrite(
        ctx,
        req,
        d,
        contents,
        &replaced,
        format!(
            "For {scenario} the person means reading B: given {}, expect {}",
            meant.given, meant.expect
        ),
        None,
        Some(scenario),
        provider,
        options,
    )?;
    Ok(format!(
        "{scenario} of {}: reading B is meant — wrote {} anew",
        req.key,
        written.join(", ")
    ))
}

/// A requirement whose tests an approval can freeze: its draft on the
/// current wording, no reading open, at least one red test.
#[derive(Debug, Clone)]
pub struct Freezable {
    /// The requirement.
    pub req: OracleRequirement,
    /// Its draft.
    pub draft: Draft,
    /// The red tests' contents.
    pub contents: Vec<(String, String)>,
    /// The red set's manifest digest.
    pub manifest: String,
}

/// The requirements an approval freezes, in the spec's order. `Err`: a
/// stored draft was tampered with.
pub fn freezable(
    project_state: &Path,
    unit: &str,
    requirements: &[OracleRequirement],
) -> Result<Vec<Freezable>, Tampered> {
    let mut out = Vec::new();
    for r in requirements {
        let Some((draft, contents)) = load(project_state, unit, &r.key)? else {
            continue;
        };
        if draft.requirement.sha256 != r.digest() || draft.open_ambiguities().next().is_some() {
            continue;
        }
        let Some(manifest) = draft.manifest() else {
            continue;
        };
        let red: Vec<String> = draft.red().map(|t| t.path.clone()).collect();
        out.push(Freezable {
            req: r.clone(),
            contents: contents
                .into_iter()
                .filter(|(p, _)| red.contains(p))
                .collect(),
            draft,
            manifest,
        });
    }
    Ok(out)
}

/// Freezes one requirement's red set with the approval: one
/// [`PreFrozenAcceptance`] (producer `review`) in the pre-frozen store,
/// its sealed held-out set checked, and the commands and wording beside it.
pub fn freeze(
    project_state: &Path,
    unit: &str,
    f: &Freezable,
    approval_digest: &str,
    decided: Decided,
    producer: Producer,
) -> Result<PreFrozenAcceptance, String> {
    let red: Vec<&DraftTest> = f.draft.red().collect();
    freeze_tests(
        project_state,
        unit,
        f,
        &red,
        approval_digest,
        decided,
        producer,
    )
}

/// [`freeze`] of `tests` (red, and at a run already passing too).
fn freeze_tests(
    project_state: &Path,
    unit: &str,
    f: &Freezable,
    red: &[&DraftTest],
    approval_digest: &str,
    decided: Decided,
    producer: Producer,
) -> Result<PreFrozenAcceptance, String> {
    let sealed = match &f.draft.sealed {
        Some(s) => {
            crate::held_out::open_sealed(project_state, unit, s)
                .map_err(|t| format!("{}'s held-out set was tampered with: {}", f.req.key, t.0))?;
            Some(s.clone())
        }
        None => None,
    };
    let record = PreFrozenAcceptance {
        unit: unit.to_owned(),
        requirement: f.req.key.clone(),
        wording_digest: f.draft.requirement.sha256.clone(),
        approval_digest: approval_digest.to_owned(),
        visible: red
            .iter()
            .map(|t| VisibleTest {
                path: t.path.clone(),
                sha256: t.sha256.clone(),
                cites: t.scenarios.clone(),
            })
            .collect(),
        held_out_sealed: sealed,
        producer,
        decided,
    };
    crate::pre_frozen::store(project_state, &record, &f.contents)?;
    let frozen = Frozen {
        requirement_sha256: f.draft.requirement.sha256.clone(),
        wording: format!(
            "{}\n{}",
            f.draft.requirement.title, f.draft.requirement.text
        ),
        manifest_sha256: f.manifest.clone(),
        tests: red
            .iter()
            .map(|t| FrozenTest {
                path: t.path.clone(),
                sha256: t.sha256.clone(),
                argv: t.argv.clone(),
                scenarios: t.scenarios.clone(),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&frozen).map_err(|e| e.to_string())?;
    write_atomic(
        &dir(project_state, unit, &f.req.key).join("frozen.json"),
        &text,
    )?;
    Ok(record)
}

/// Why an oracle test cannot be kept, if it cannot. Kept: red on an
/// assertion, already passing, or — in a compiled language, where a test of
/// what does not exist yet cannot build — red by an absent symbol (in
/// Python or JavaScript an absent import stops the whole run's
/// collection). A file of several scenarios, when its story has more, is
/// never kept: its scenarios could only turn green together.
/// A discarded test's reason, in words (`exists` → "already exists").
fn discard_words(why: &str) -> &'static str {
    match why {
        "cites_no_scenario" => "cites no scenario of the list",
        "no_command" => {
            "never runs the project's command — the story's interface is the command, not a module behind it"
        }
        "command_shared" => {
            "shares its command with another test — each test needs the command that runs only it"
        }
        "command_not_allowed" => "gives a command the policy does not allow",
        "limit" => "is beyond the number of tests one answer may hold",
        "red_without_it" => "fails without its own file — its command runs other, failing tests",
        _ => crate::pipeline::acceptance::refusal_words(why),
    }
}

fn not_kept(t: &DraftTest, many: bool) -> Option<String> {
    if many && t.scenarios.len() > 1 {
        return Some(format!(
            "it tests {} together — one file per scenario, so each turns green on its own",
            t.scenarios.join(", ")
        ));
    }
    (!usable(t)).then(|| {
        format!(
            "it did not fail on an assertion ({})",
            crate::pipeline::class_word(t.red_class)
        )
    })
}

fn usable(t: &DraftTest) -> bool {
    match t.red_class {
        RedClass::Assertion | RedClass::Passed => true,
        RedClass::Absent => [".rs", ".go", ".java", ".kt", ".cs", ".swift"]
            .iter()
            .any(|e| t.path.ends_with(e)),
        _ => false,
    }
}

/// A story's oracle as a run wrote it: the kept tests (red, or already
/// passing), their contents, each scenario's contract, and what could not
/// be kept.
#[derive(Debug)]
pub(crate) struct RunOracle {
    /// Each kept test with its command and scenarios (none: the author
    /// found nothing testable).
    pub tests: Vec<FrozenTest>,
    /// Their contents (path, text).
    pub contents: Vec<(String, String)>,
    /// What each scenario must hold, as the author stated it.
    pub contracts: Vec<Expectation>,
    /// The scenarios the author could not test, and why.
    pub untestable: Vec<Untestable>,
    /// The token usage of its author calls.
    pub usage: Option<Value>,
}

/// Writes `story`'s oracle for a run nobody reviewed: one author pass on
/// the task's own worktree (`wt_root`, its project at `project`, clean at
/// the tree the task starts from), every test checked alone; a scenario
/// left without a usable test is asked for once more. Tests red or already
/// passing are kept and frozen by `speccoder run`; the held-out set is
/// sealed beside them. The worktree is left as it was.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_for_run(
    project: &Path,
    wt_root: &Path,
    feature: &str,
    state: &Path,
    story: &str,
    runner: sandbox::Runner,
    (rt, plane): (&tokio::runtime::Runtime, &substrate::Plane),
    (only, told): (&[String], &[String]),
    run: &str,
    options: &crate::RunOptions,
) -> Result<RunOracle, Box<CommandResult>> {
    let ctx = open(project, Some(feature), state)?;
    let requirements = ctx.format.oracle_requirements(&ctx.snapshot);
    // The unit as one story (OpenSpec): every requirement's scenarios.
    let whole = (story == ctx.feature && !requirements.iter().any(|r| r.key == story)).then(|| {
        OracleRequirement {
            key: story.to_owned(),
            title: ctx.feature.clone(),
            text: requirements
                .iter()
                .map(|r| format!("{}: {}\n{}", r.key, r.title, r.text))
                .collect::<Vec<_>>()
                .join("\n\n"),
            scenarios: requirements
                .iter()
                .flat_map(|r| r.scenarios.clone())
                .collect(),
        }
    });
    let Some(mut req) = whole.or_else(|| requirements.into_iter().find(|r| r.key == story)) else {
        return Err(Box::new(refuse(
            CommandStatus::NeedsHuman,
            "ORACLE-UNWRITTEN",
            &format!("{story} has no scenarios to write tests for"),
        )));
    };
    // An amendment: only the scenarios a person ruled wrong, told why.
    if !only.is_empty() {
        req.scenarios.retain(|s| only.contains(&s.id));
    }
    let _lock = UnitLock::take(&ctx.plane_dir, &ctx.feature)
        .map_err(|e| Box::new(refuse(CommandStatus::GitPrecondition, "RUN-ACTIVE", &e)))?;
    let author = Author::lent(&ctx, &req, rt, plane);
    let many = req.scenarios.len() > 1;
    let bench = Bench::here(&ctx, wt_root, runner);
    let kept = |d: &Draft| -> Vec<String> {
        let mut covered: Vec<String> = d
            .tests
            .iter()
            .filter(|t| not_kept(t, many).is_none())
            .flat_map(|t| t.scenarios.iter().cloned())
            .collect();
        covered.sort();
        covered.dedup();
        covered
    };
    // Two passes at most, neither sealing a held-out set: the kept one is
    // sealed once, after it is chosen.
    let pass = |refused: &[String]| {
        draft(
            &ctx,
            &req,
            &author,
            &bench,
            1,
            told,
            refused,
            Vec::new(),
            told.to_vec(),
            false,
            options,
        )
    };
    let (mut draft, mut contents) = pass(&[])?;
    // Asked once more, told every reason in speccoder's words: a test that
    // broke or took a path that exists, nothing kept at all, or every
    // scenario called untestable with no test written.
    let untestable: Vec<String> = draft
        .untestable
        .sample1
        .iter()
        .map(|u| u.scenario.clone())
        .collect();
    let broke =
        !draft.discarded.is_empty() || draft.tests.iter().any(|t| not_kept(t, many).is_some());
    let left: Vec<String> = req
        .scenarios
        .iter()
        .map(|s| s.id.clone())
        .filter(|id| !kept(&draft).contains(id) && !untestable.contains(id))
        .collect();
    let opted_out = draft.tests.is_empty() && draft.discarded.is_empty() && !untestable.is_empty();
    if (broke && (!left.is_empty() || kept(&draft).is_empty())) || opted_out {
        let again: Vec<String> = if left.is_empty() {
            req.scenarios.iter().map(|s| s.id.clone()).collect()
        } else {
            left
        };
        let refused: Vec<String> = draft
            .discarded
            .iter()
            .map(|d| format!("Not kept `{}`: it {}", d.path, discard_words(&d.why)))
            .chain(draft.tests.iter().filter_map(|t| {
                not_kept(t, many).map(|why| format!("Not kept `{}`: {why}", t.path))
            }))
            .chain(opted_out.then(|| {
                format!(
                    "Called untestable: {} — the story's code not existing yet never makes a scenario untestable; test it through the story's interface",
                    untestable.join(", ")
                )
            }))
            .chain(std::iter::once(format!(
                "Write tests again for {} — each must build and be collected without the code it tests existing yet: exercise the story through its interface, import the code under test inside each test",
                again.join(", ")
            )))
            .collect();
        let (second, more) = pass(&refused)?;
        if kept(&second).len() > kept(&draft).len() {
            (draft, contents) = (second, more);
        }
    }
    // The kept pass's held-out set (an amendment keeps the story's).
    if only.is_empty() {
        let keep: Vec<DraftTest> = draft
            .tests
            .iter()
            .filter(|t| not_kept(t, many).is_none())
            .cloned()
            .collect();
        if !keep.is_empty() {
            (draft.held_out, draft.sealed) = held_out(&ctx, &author, &bench, &req, &keep, options);
        }
    }
    // An amendment's draft is a part: the story's own draft stays.
    if only.is_empty() {
        store(&ctx.plane_dir, &ctx.feature, &draft, &contents).map_err(failed)?;
    }
    let keep: Vec<&DraftTest> = draft
        .tests
        .iter()
        .filter(|t| not_kept(t, many).is_none())
        .collect();
    // The author found nothing testable (no test written at all): no
    // oracle, and no stop — the story is judged by its checks.
    if keep.is_empty() && draft.tests.is_empty() && draft.discarded.is_empty() {
        return Ok(RunOracle {
            tests: Vec::new(),
            contents: Vec::new(),
            contracts: draft.expectations.sample1.clone(),
            untestable: draft.untestable.sample1.clone(),
            usage: author.spent.borrow().clone(),
        });
    }
    if keep.is_empty() {
        let why: Vec<String> = draft
            .discarded
            .iter()
            .map(|d| format!("`{}` {}", d.path, discard_words(&d.why)))
            .chain(
                draft
                    .tests
                    .iter()
                    .filter_map(|t| not_kept(t, many).map(|why| format!("`{}`: {why}", t.path))),
            )
            .collect();
        return Err(Box::new(
            refuse(
                CommandStatus::NeedsHuman,
                "ORACLE-UNWRITTEN",
                &format!(
                    "no test written for {story} could be used, asked twice: {}",
                    why.join("; ")
                ),
            )
            .with_fix(vec![format!("speccoder review tests {story} --write")]),
        ));
    }
    let manifest = manifest_digest(
        keep.iter()
            .map(|t| (t.path.as_str(), t.sha256.as_str(), t.argv.as_slice())),
    );
    let paths: Vec<&str> = keep.iter().map(|t| t.path.as_str()).collect();
    let kept_contents: Vec<(String, String)> = contents
        .iter()
        .filter(|(p, _)| paths.contains(&p.as_str()))
        .cloned()
        .collect();
    let f = Freezable {
        req: req.clone(),
        draft: draft.clone(),
        contents: kept_contents.clone(),
        manifest,
    };
    // An amendment is merged into the frozen oracle by its caller; the
    // story's record and sealed set stay.
    if only.is_empty() {
        freeze_tests(
            &ctx.plane_dir,
            &ctx.feature,
            &f,
            &keep,
            &format!(
                "run:{run}{}",
                if only.is_empty() { "" } else { ":amendment" }
            ),
            Decided {
                by: "speccoder run".to_owned(),
                via: Via::Terminal,
                at: now(),
            },
            Producer::Run,
        )
        .map_err(failed)?;
    }
    Ok(RunOracle {
        tests: keep
            .iter()
            .map(|t| FrozenTest {
                path: t.path.clone(),
                sha256: t.sha256.clone(),
                argv: t.argv.clone(),
                scenarios: t.scenarios.clone(),
            })
            .collect(),
        contents: kept_contents,
        contracts: draft.expectations.sample1.clone(),
        untestable: draft.untestable.sample1.clone(),
        usage: author.spent.borrow().clone(),
    })
}

/// Replaces the tests a frozen oracle runs (an amendment: the ruled
/// scenarios' tests swapped), its manifest recomputed.
///
/// # Errors
///
/// Nothing is frozen for `key`, or it cannot be written.
pub(crate) fn refreeze(
    project_state: &Path,
    unit: &str,
    key: &str,
    tests: Vec<FrozenTest>,
) -> Result<(), String> {
    let mut frozen = load_frozen(project_state, unit, key)
        .ok_or_else(|| format!("{key} has no frozen oracle here"))?;
    frozen.manifest_sha256 = manifest_digest(
        tests
            .iter()
            .map(|t| (t.path.as_str(), t.sha256.as_str(), t.argv.as_slice())),
    );
    frozen.tests = tests;
    let text = serde_json::to_string_pretty(&frozen).map_err(|e| e.to_string())?;
    write_atomic(&dir(project_state, unit, key).join("frozen.json"), &text)
}

/// What a task's checkpoint does with a frozen oracle.
#[derive(Debug)]
pub(crate) enum TaskReuse {
    /// Reuse it: write, red-check, no author call.
    Ready {
        /// The record.
        record: Box<PreFrozenAcceptance>,
        /// Its files.
        files: Vec<(String, String)>,
        /// Each test with its command.
        tests: Vec<crate::pipeline::ReusedTest>,
        /// The manifest digest the tick names.
        manifest: String,
    },
    /// An earlier task reused it (its tick names the manifest).
    Reused {
        /// The requirement.
        req: String,
    },
    /// `review.json` names a frozen set this machine does not have.
    Elsewhere {
        /// The requirement.
        req: String,
    },
    /// The requirement's wording changed since it was frozen.
    Lapsed {
        /// The requirement.
        req: String,
        /// The wording it was frozen on.
        frozen: String,
        /// The wording now.
        current: String,
    },
    /// The stored set no longer matches what was frozen.
    Tampered(String),
    /// `review.json` cannot be read: whether anything is frozen is unknown.
    Unreadable(String),
}

/// The frozen oracle `task` (citing `scenarios`) reuses, if any: the
/// first requirement whose scenarios it cites and whose oracle `review.json`
/// (in `project`, the worktree's committed copy) froze — unless an earlier
/// task's tick in the worktree's history names its manifest.
pub(crate) fn for_task(
    project_state: &Path,
    project: &Path,
    wt_root: &Path,
    format: &dyn Adapter,
    snapshot: &ProjectSnapshot,
    scenarios: &[speccoder_core::spec::ScenarioRef],
) -> Option<TaskReuse> {
    let f = snapshot.feature.as_ref()?;
    let unit = f.feature.name.as_str();
    let (entries, approval) = match crate::review::oracle_entries(project, &f.feature.dir) {
        Ok(x) => x,
        // Unreadable is never "nothing frozen": the task stops for a person.
        Err(e) => return Some(TaskReuse::Unreadable(e)),
    };
    if entries.is_empty() {
        return None;
    }
    let ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
    let requirements = format.oracle_requirements(snapshot);
    let (req, entry) = requirements.iter().find_map(|r| {
        let e = entries.get(&r.key)?;
        r.scenarios
            .iter()
            .any(|s| ids.contains(&s.id.as_str()))
            .then_some((r, e))
    })?;
    let digest = req.digest();
    if entry.requirement_sha256 != digest {
        return Some(TaskReuse::Lapsed {
            req: req.key.clone(),
            frozen: entry.requirement_sha256.clone(),
            current: digest,
        });
    }
    let (record, files) = match crate::pre_frozen::load(project_state, unit, &req.key) {
        Ok(Some(x)) => x,
        Ok(None) => {
            return Some(TaskReuse::Elsewhere {
                req: req.key.clone(),
            });
        }
        Err(t) => return Some(TaskReuse::Tampered(t.0)),
    };
    // Reused by an earlier task: its tick names the frozen manifest in the
    // one `Acceptance:` trailer form, which only speccoder's commit writes —
    // never decided by a file the implementer could commit (P2).
    let needle = format!("Acceptance: review manifest={} ", entry.manifest_sha256);
    let reused = crate::git::Git::at(wt_root)
        .run(&[
            "log",
            "--format=%H",
            "--fixed-strings",
            &format!("--grep={needle}"),
            "HEAD",
        ])
        .map(|out| !out.is_empty());
    match reused {
        Ok(true) => {
            return Some(TaskReuse::Reused {
                req: req.key.clone(),
            });
        }
        Ok(false) => {}
        Err(e) => {
            return Some(TaskReuse::Unreadable(format!(
                "the worktree's history cannot be read: {}",
                e.0
            )));
        }
    }
    if !record.is_current(&digest) {
        return Some(TaskReuse::Lapsed {
            req: req.key.clone(),
            frozen: record.wording_digest.clone(),
            current: digest,
        });
    }
    let Some(frozen) = load_frozen(project_state, unit, &req.key) else {
        return Some(TaskReuse::Tampered(format!(
            "{}'s frozen commands are missing from the state directory",
            req.key
        )));
    };
    let manifest = manifest_digest(
        frozen
            .tests
            .iter()
            .map(|t| (t.path.as_str(), t.sha256.as_str(), t.argv.as_slice())),
    );
    let same_files = frozen.tests.len() == record.visible.len()
        && frozen.tests.iter().all(|t| {
            record
                .visible
                .iter()
                .any(|v| v.path == t.path && v.sha256 == t.sha256)
        });
    if manifest != entry.manifest_sha256 || manifest != frozen.manifest_sha256 || !same_files {
        return Some(TaskReuse::Tampered(format!(
            "{}'s frozen tests no longer match the manifest review.json names",
            req.key
        )));
    }
    // Bound both ways (P3): the approval review.json records, and the
    // held-out set it names — a record that dropped either is not reused.
    if approval.as_deref() != Some(record.approval_digest.as_str()) {
        return Some(TaskReuse::Tampered(format!(
            "{}'s frozen set names another approval than review.json{}",
            req.key,
            if approval.is_none() {
                " (which records none)"
            } else {
                ""
            }
        )));
    }
    let sealed = record
        .held_out_sealed
        .as_ref()
        .map(|h| format!("sha256:{}", h.manifest_sha256));
    if sealed != entry.held_out_sha256 {
        return Some(TaskReuse::Tampered(format!(
            "{}'s frozen held-out set is not the one review.json names",
            req.key
        )));
    }
    let tests = frozen
        .tests
        .iter()
        .map(|t| crate::pipeline::ReusedTest {
            path: t.path.clone(),
            cites: t.scenarios.clone(),
            argv: t.argv.clone(),
            green: false,
        })
        .collect();
    Some(TaskReuse::Ready {
        record: Box::new(record),
        files,
        tests,
        manifest,
    })
}

/// Records that `req`'s oracle lapsed from `frozen` to `current`; `true`
/// the first time (the `oracle.lapsed` event is emitted once per lapse).
pub(crate) fn note_lapse(project_state: &Path, unit: &str, req: &str, frozen: &str) -> bool {
    let file = project_state.join(DIR).join(unit).join("lapsed.json");
    let mut seen: Vec<String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let key = format!("{req}={frozen}");
    if seen.contains(&key) {
        return false;
    }
    seen.push(key);
    let _ = write_atomic(&file, &serde_json::to_string(&seen).unwrap_or_default());
    true
}

/// The calls an autopilot run refused, as its tool log recorded them.
#[must_use]
pub fn autopilot_denials(
    task: &str,
    tool_calls: &[Value],
) -> Vec<speccoder_core::oracle::AutopilotDenial> {
    tool_calls
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c["decision"] == "deny" && c["reason"].as_str() == Some(tools::policy::AUTOPILOT_DENIED)
        })
        .map(|(i, c)| {
            let object: Option<tools::policy::Object> =
                serde_json::from_value(c["object"].clone()).ok();
            speccoder_core::oracle::AutopilotDenial {
                task: task.to_owned(),
                turn: c["turn"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or_else(|| u32::try_from(i + 1).unwrap_or(u32::MAX)),
                class: object
                    .as_ref()
                    .map(|o| o.class().as_str().to_owned())
                    .unwrap_or_default(),
                object: object.as_ref().map_or_else(
                    || {
                        c["arguments"]["path"]
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| {
                                c["arguments"]["argv"].as_array().map(|a| {
                                    a.iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                            })
                            .unwrap_or_else(|| c["tool"].as_str().unwrap_or_default().to_owned())
                    },
                    tools::policy::Object::describe,
                ),
                reason: tools::policy::AUTOPILOT_DENIED.to_owned(),
            }
        })
        .collect()
}

/// The last autopilot run's denials of a unit, for Needs You.
#[must_use]
pub fn last_denials(
    project_state: &Path,
    unit: &str,
) -> Vec<speccoder_core::oracle::AutopilotDenial> {
    std::fs::read_to_string(project_state.join(DIR).join(unit).join("autopilot.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Records a run's denials (none: clears them).
pub(crate) fn record_denials(
    project_state: &Path,
    unit: &str,
    denials: &[speccoder_core::oracle::AutopilotDenial],
) {
    let file = project_state.join(DIR).join(unit).join("autopilot.json");
    if denials.is_empty() {
        let _ = std::fs::remove_file(file);
        return;
    }
    let _ = write_atomic(
        &file,
        &serde_json::to_string_pretty(denials).unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each scenario gets a file named after it, never one a task wrote
    /// before the story had an oracle.
    #[test]
    fn each_scenario_is_placed_in_a_free_file_of_its_own() {
        let d = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        for (path, text) in [
            ("pytest.ini", "[pytest]\n"),
            (
                "tests/test_acceptance_us2.py",
                "def test_t014():\n    pass\n",
            ),
            (
                "tests/test_acceptance_us2_1.py",
                "def test_t017():\n    pass\n",
            ),
        ] {
            std::fs::create_dir_all(d.path().join(path).parent().unwrap()).unwrap();
            std::fs::write(d.path().join(path), text).unwrap();
        }
        let scenario = |id: &str| speccoder_core::spec::ScenarioRef {
            id: id.to_owned(),
            text: String::new(),
        };
        let req = OracleRequirement {
            key: "US2".to_owned(),
            title: "File input".to_owned(),
            text: String::new(),
            scenarios: vec![scenario("US2-1"), scenario("US2-2")],
        };
        let got: Vec<(String, String)> = places(d.path(), d.path(), &req)
            .into_iter()
            .map(|p| (p.scenario, p.path))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "US2-1".to_owned(),
                    "tests/test_acceptance_us2_1_2.py".to_owned()
                ),
                (
                    "US2-2".to_owned(),
                    "tests/test_acceptance_us2_2.py".to_owned()
                ),
            ]
        );
    }

    fn draft(content: &str) -> Draft {
        Draft {
            requirement: DraftRequirement {
                key: "US1".to_owned(),
                title: "Greet".to_owned(),
                text: "### User Story 1 - Greet".to_owned(),
                sha256: "sha256:w".to_owned(),
            },
            samples: 2,
            tests: vec![DraftTest {
                path: "tests/acceptance_us1.rs".to_owned(),
                sha256: digest_of(content),
                scenarios: vec!["US1-1".to_owned()],
                argv: vec!["cargo".to_owned(), "test".to_owned()],
                red_class: RedClass::Assertion,
                authored_by: AuthoredBy::Model,
                edited_by: None,
            }],
            discarded: Vec::new(),
            rejected: Vec::new(),
            expectations: Samples::default(),
            untestable: Samples::default(),
            ambiguities: Vec::new(),
            held_out: HeldOutSummary {
                state: "absent".to_owned(),
                count: 0,
                manifest_sha256: None,
                reason: None,
                discarded: Vec::new(),
            },
            sealed: None,
            notes: Vec::new(),
            runs: Vec::new(),
            context: Vec::new(),
        }
    }

    #[test]
    fn a_draft_round_trips_and_a_changed_test_is_tampering() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let d = draft("#[test] fn t() {}");
        let files = vec![(
            "tests/acceptance_us1.rs".to_owned(),
            "#[test] fn t() {}".to_owned(),
        )];
        store(state.path(), "001-greet", &d, &files).unwrap_or_else(|e| panic!("{e}"));
        let (back, contents) = load(state.path(), "001-greet", "US1")
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|| panic!("stored"));
        assert_eq!(back, d);
        assert_eq!(contents, files);
        std::fs::write(
            dir(state.path(), "001-greet", "US1").join("files/tests/acceptance_us1.rs"),
            "changed",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(load(state.path(), "001-greet", "US1").is_err());
        assert!(store(state.path(), "001-greet", &d, &[]).is_err());
        assert!(
            load(state.path(), "001-greet", "US2")
                .unwrap_or_default()
                .is_none()
        );
    }

    #[test]
    fn the_manifest_names_paths_contents_and_commands() {
        let argv = vec!["cargo".to_owned()];
        let other = vec!["make".to_owned()];
        let a = manifest_digest([("a", "sha256:1", argv.as_slice())]);
        assert_eq!(a, manifest_digest([("a", "sha256:1", argv.as_slice())]));
        assert_ne!(a, manifest_digest([("a", "sha256:2", argv.as_slice())]));
        assert_ne!(a, manifest_digest([("a", "sha256:1", other.as_slice())]));
        let both = manifest_digest([("a", "s", argv.as_slice()), ("b", "s", argv.as_slice())]);
        assert_eq!(
            both,
            manifest_digest([("b", "s", argv.as_slice()), ("a", "s", argv.as_slice())])
        );
        let mut d = draft("x");
        assert!(d.manifest().is_some());
        d.tests[0].red_class = RedClass::Passed;
        assert!(d.manifest().is_none());
        assert_eq!(slug("cap: a / b"), "cap__a___b");
    }

    #[test]
    fn a_lapse_is_noted_once() {
        let state = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        assert!(note_lapse(state.path(), "u", "US1", "sha256:a"));
        assert!(!note_lapse(state.path(), "u", "US1", "sha256:a"));
        assert!(note_lapse(state.path(), "u", "US1", "sha256:b"));
    }
}
