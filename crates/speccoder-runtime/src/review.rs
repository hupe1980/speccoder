//! `speccoder review`: the person's review of the spec, one
//! requirement at a time. A decision — approved, or changes requested with a
//! comment — is bound to the exact line it was made on, and names who made
//! it; when the line changes, the decision lapses. It is recorded in
//! `review.json` beside the spec, to commit and share like the spec itself.
//! The model never writes it ([`crate::tools::paths::REVIEW_FILE`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use speccoder_core::{
    CommandData, CommandResult, CommandStatus, OpenQuestion, RequirementDrift, ReviewItem,
    ReviewReport, ReviewTotals, SpecVersion,
};

use crate::tools::paths::REVIEW_FILE;

/// One decision as recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Decision {
    /// `approved` or `changes_requested`.
    verdict: String,
    /// The line decided on.
    line: String,
    /// `sha256:<hex>` of the line, trailing space aside.
    sha256: String,
    /// Who (git's `user.name`, else the OS user).
    by: String,
    /// `cli`, `web`, `editor <name>`.
    via: String,
    /// Unix seconds.
    at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

/// The file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    requirements: BTreeMap<String, Decision>,
    /// The frozen oracle, by requirement (`US1`): digests only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    oracle: BTreeMap<String, crate::oracle::OracleEntry>,
    /// The approval that froze it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval: Option<speccoder_core::oracle::ReviewApproval>,
}

fn digest(line: &str) -> String {
    format!(
        "sha256:{}",
        crate::integrity::sha256_hex(line.trim_end().as_bytes())
    )
}

/// `- **FR-001**: Users MUST …` → `Users MUST …`.
fn wording(line: &str) -> String {
    let t = line.trim_start().trim_start_matches("- ");
    match t.strip_prefix("**").and_then(|r| r.split_once("**")) {
        Some((_, rest)) => rest.trim_start_matches(':').trim().to_owned(),
        None => t.trim().to_owned(),
    }
}

/// What a review is about: the feature, its directory, the project root.
struct Unit {
    feature: String,
    root: PathBuf,
    dir: String,
    format: &'static dyn crate::format::Adapter,
    /// The project as inspected (the requirements with scenarios come
    /// from it); `None` for the run's gate, which reads decision lines only.
    snapshot: Option<Box<speccoder_core::snapshot::ProjectSnapshot>>,
    /// The project's state directory, when there is one.
    state: Option<PathBuf>,
}

impl Unit {
    fn file(&self) -> PathBuf {
        self.root.join(&self.dir).join(REVIEW_FILE)
    }
    fn rel(&self) -> String {
        format!("{}/{REVIEW_FILE}", self.dir.trim_end_matches('/'))
    }
    /// The requirements as the spec in the checkout has them now.
    fn requirements(&self) -> Vec<(String, String)> {
        let Some(file) = self.format.requirements_file() else {
            return Vec::new();
        };
        let text =
            std::fs::read_to_string(self.root.join(&self.dir).join(file)).unwrap_or_default();
        self.format.requirements_in(&text)
    }
    /// The recorded review; none written yet is the empty record, one
    /// that cannot be read is an `Err` (never read as empty).
    fn record(&self) -> Result<Record, String> {
        read_record(&self.file())
    }
    /// The requirements with scenarios a review writes tests for.
    fn oracle_requirements(&self) -> Vec<crate::format::OracleRequirement> {
        self.snapshot
            .as_ref()
            .map(|s| self.format.oracle_requirements(s))
            .unwrap_or_default()
            .into_iter()
            .filter(|r| !r.scenarios.is_empty())
            .collect()
    }
    /// The unit's rung (a quick change's, a bug's, a feature's).
    fn rung(&self) -> speccoder_core::ladder::Rung {
        crate::oracle::rung(self.format, &self.root, &self.feature)
    }
}

fn unit(command: &str, path: &Path, feature: Option<&str>) -> Result<Unit, Box<CommandResult>> {
    unit_in(command, path, feature, None)
}

fn unit_in(
    command: &str,
    path: &Path,
    feature: Option<&str>,
    state: Option<&Path>,
) -> Result<Unit, Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried(command, &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            command,
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = PathBuf::from(&snapshot.root);
    let state = state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
        .map(|s| crate::worktree::project_state_dir(&s, &root));
    Ok(Unit {
        feature: f.feature.name.clone(),
        root,
        dir: f.feature.dir.clone(),
        format: crate::format::of(snapshot),
        snapshot: Some(snapshot.clone()),
        state,
    })
}

/// Each requirement with its state against the record.
fn items(requirements: &[(String, String)], record: &Record) -> Vec<ReviewItem> {
    requirements
        .iter()
        .map(|(id, line)| {
            let d = record.requirements.get(id);
            let current = d.is_some_and(|d| d.sha256 == digest(line));
            let state = match d {
                None => "open",
                Some(_) if !current => "lapsed",
                Some(d) if d.verdict == "approved" => "approved",
                Some(_) => "changes_requested",
            };
            ReviewItem {
                id: id.clone(),
                text: wording(line),
                state: state.to_owned(),
                by: d.map(|d| d.by.clone()),
                at: d.map(|d| d.at),
                comment: d.and_then(|d| d.comment.clone()),
                decided_on: d.filter(|_| !current).map(|d| wording(&d.line)),
            }
        })
        .collect()
}

fn totals(items: &[ReviewItem], questions: usize) -> ReviewTotals {
    let n = |s: &str| items.iter().filter(|i| i.state == s).count();
    ReviewTotals {
        approved: n("approved"),
        changes_requested: n("changes_requested"),
        open: n("open"),
        lapsed: n("lapsed"),
        questions,
        ambiguous: 0,
    }
}

/// The marker a skill leaves where it could not decide.
const MARKER: &str = "[NEEDS CLARIFICATION";

/// The unit's Markdown documents, the requirements file first, then by
/// name: (project-relative path, text).
fn documents(u: &Unit) -> Vec<(String, String)> {
    let dir = u.root.join(&u.dir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    let first = u.format.requirements_file().unwrap_or_default();
    names.sort_by_key(|n| (n != first, n.clone()));
    names
        .into_iter()
        .filter_map(|n| {
            let text = std::fs::read_to_string(dir.join(&n)).ok()?;
            Some((format!("{}/{n}", u.dir.trim_end_matches('/')), text))
        })
        .collect()
}

/// Every `[NEEDS CLARIFICATION: …]` in `text`: (the marker as written, the
/// question), in order.
fn markers(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find(MARKER) {
        let tail = &rest[at..];
        let Some(end) = tail.find(']') else { break };
        let raw = &tail[..=end];
        let question = raw[MARKER.len()..raw.len() - 1]
            .trim_start_matches(':')
            .trim()
            .to_owned();
        out.push((raw.to_owned(), question));
        rest = &tail[end + 1..];
    }
    out
}

/// The questions the unit's documents leave open, with their markers.
fn questions(u: &Unit) -> Vec<(OpenQuestion, String)> {
    let mut out = Vec::new();
    for (file, text) in documents(u) {
        let mut section: Option<String> = None;
        for (i, line) in text.lines().enumerate() {
            if let Some(h) = line
                .strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
            {
                section = Some(h.trim().to_owned());
            }
            let near = u
                .format
                .requirements_in(line)
                .first()
                .map(|(id, _)| id.clone())
                .or_else(|| section.clone());
            for (raw, text) in markers(line) {
                out.push((
                    OpenQuestion {
                        id: format!("Q{}", out.len() + 1),
                        text,
                        file: file.clone(),
                        line: i + 1,
                        near: near.clone(),
                    },
                    raw,
                ));
            }
        }
    }
    out
}

/// The requirements as `rev` (`HEAD`, a commit) has the requirements file.
fn requirements_at(u: &Unit, rev: &str) -> Option<Vec<(String, String)>> {
    let file = u.format.requirements_file()?;
    let rel = format!("{}/{file}", u.dir.trim_end_matches('/'));
    let text = crate::git::Git::at(&u.root)
        .run(&["show", &format!("{rev}:./{rel}")])
        .ok()?;
    Some(u.format.requirements_in(&String::from_utf8_lossy(&text)))
}

/// The requirements changed in the checkout since the spec's last commit,
/// each with its line number on both sides.
fn changes_since_head(u: &Unit) -> Vec<speccoder_core::ReviewChange> {
    let Some(file) = u.format.requirements_file() else {
        return Vec::new();
    };
    let Some(then) = requirements_at(u, "HEAD") else {
        return Vec::new();
    };
    let path = format!("{}/{file}", u.dir.trim_end_matches('/'));
    let old_text = crate::git::Git::at(&u.root)
        .run(&["show", &format!("HEAD:./{path}")])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let new_text = std::fs::read_to_string(u.root.join(&path)).unwrap_or_default();
    let line_of = |text: &str, line: &Option<String>| {
        let line = line.as_deref()?.trim_end();
        text.lines()
            .position(|l| l.trim_end() == line)
            .and_then(|i| u32::try_from(i + 1).ok())
    };
    compare(&then, &u.requirements())
        .into_iter()
        .map(|d| speccoder_core::ReviewChange {
            then_line: line_of(&old_text, &d.then),
            now_line: line_of(&new_text, &d.now),
            path: path.clone(),
            id: d.id,
            then: d.then,
            now: d.now,
        })
        .collect()
}

/// What changed from `before` to `after`, requirement by requirement.
fn compare(before: &[(String, String)], after: &[(String, String)]) -> Vec<RequirementDrift> {
    let mut ids: Vec<&String> = after.iter().map(|(id, _)| id).collect();
    for (id, _) in before {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let line = |v: &[(String, String)], id: &str| {
        v.iter()
            .find(|(i, _)| i == id)
            .map(|(_, l)| l.trim_end().to_owned())
    };
    ids.into_iter()
        .filter_map(|id| {
            let (then, now) = (line(before, id), line(after, id));
            (then != now).then(|| RequirementDrift {
                id: id.clone(),
                then,
                now,
            })
        })
        .collect()
}

/// The spec's last commits and what each changed in its requirements.
fn history(u: &Unit) -> Vec<SpecVersion> {
    let Some(file) = u.format.requirements_file() else {
        return Vec::new();
    };
    let rel = format!("{}/{file}", u.dir.trim_end_matches('/'));
    let git = crate::git::Git::at(&u.root);
    let log = git
        .run(&[
            "log",
            "-n",
            "12",
            "--format=%H%x1f%as%x1f%an%x1f%s",
            "--",
            &rel,
        ])
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    log.lines()
        .filter_map(|l| {
            let mut f = l.split('\u{1f}');
            let (commit, date, author, subject) = (f.next()?, f.next()?, f.next()?, f.next()?);
            let now = requirements_at(u, commit).unwrap_or_default();
            let before = requirements_at(u, &format!("{commit}^")).unwrap_or_default();
            let changes = compare(&before, &now);
            let pick = |want: fn(&RequirementDrift) -> bool| {
                changes
                    .iter()
                    .filter(|c| want(c))
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>()
            };
            Some(SpecVersion {
                commit: commit.to_owned(),
                date: date.to_owned(),
                author: author.to_owned(),
                subject: subject.to_owned(),
                added: pick(|c| c.then.is_none()),
                reworded: pick(|c| c.then.is_some() && c.now.is_some()),
                removed: pick(|c| c.now.is_none()),
            })
        })
        .collect()
}

/// The review state of the feature in `dir` (project-relative) under
/// `root`; `None` when the format has no requirement identifiers or the
/// spec names none.
#[must_use]
pub fn totals_of(
    root: &Path,
    format: &'static dyn crate::format::Adapter,
    feature: &str,
    dir: &str,
) -> Option<ReviewTotals> {
    let u = Unit {
        feature: feature.to_owned(),
        root: root.to_path_buf(),
        dir: dir.to_owned(),
        format,
        snapshot: None,
        state: None,
    };
    let requirements = u.requirements();
    // A record that cannot be read approves nothing: every line counts open.
    (!requirements.is_empty()).then(|| {
        totals(
            &items(&requirements, &u.record().unwrap_or_default()),
            questions(&u).len(),
        )
    })
}

/// The review; `full` adds what changed since the last commit and the
/// history (git reads, for the review view — not for the inbox).
fn report_of(u: &Unit, full: bool) -> ReviewReport {
    // Shown as it is: a record that cannot be read approves and freezes
    // nothing, and the report says why.
    let (record, error) = match u.record() {
        Ok(r) => (r, None),
        Err(e) => (Record::default(), Some(e)),
    };
    let items = items(&u.requirements(), &record);
    let skills = crate::skills::discover_with(&u.root, u.format.planning());
    let installed = |name: &str| {
        let full = format!("{}{name}", u.format.planning().prefix);
        skills.iter().any(|s| s.name == full && s.refused.is_none())
    };
    let open: Vec<OpenQuestion> = questions(u).into_iter().map(|(q, _)| q).collect();
    let readiness = readiness_of(u, open.len());
    let totals = totals(&items, open.len());
    let revision = (totals.changes_requested > 0).then(|| revision_request(&items));
    let changes = if full {
        changes_since_head(u)
    } else {
        Vec::new()
    };
    let rung = u.rung();
    let (oracle, tests_notice) = if rung == speccoder_core::ladder::Rung::Feature {
        let cfg = crate::oracle::ViewConfig::at(&u.root);
        let views = u
            .oracle_requirements()
            .iter()
            .map(|r| match &u.state {
                Some(state) => {
                    crate::oracle::view(state, &u.feature, r, record.oracle.get(&r.key), cfg)
                }
                None => crate::oracle::view(
                    Path::new("/nonexistent-speccoder-state"),
                    &u.feature,
                    r,
                    record.oracle.get(&r.key),
                    cfg,
                ),
            })
            .collect::<Vec<_>>();
        (views, None)
    } else {
        (
            Vec::new(),
            Some("Tests for this unit are written and confirmed by its own pipeline".to_owned()),
        )
    };
    let bug_tests = if rung == speccoder_core::ladder::Rung::Bug {
        u.state
            .as_deref()
            .map(|plane| confirmed_reproductions(plane, &u.feature))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut totals = totals;
    totals.ambiguous = oracle
        .iter()
        .filter(|v| v.open_ambiguities().next().is_some())
        .count();
    // The approve sheet is the view's (`report`), never a decision's result.
    let sheet = None;
    ReviewReport {
        feature: u.feature.clone(),
        file: u.rel(),
        totals,
        revision,
        questions: open,
        changes,
        history: if full { history(u) } else { Vec::new() },
        requirements: items,
        revise_with: u
            .format
            .planning()
            .revises
            .filter(|s| installed(s))
            .map(str::to_owned),
        oracle,
        tests_notice,
        bug_tests,
        approval: record.approval,
        sheet,
        error,
        readiness: Some(readiness),
    }
}

/// The unit's readiness: its requirements, the scenarios over its
/// stories (or the format's requirements), the open questions, and the
/// stories or requirements no scenario covers — each counted from the
/// files through the format adapter.
fn readiness_of(u: &Unit, open_questions: usize) -> speccoder_core::Readiness {
    let all = u
        .snapshot
        .as_ref()
        .map(|s| u.format.oracle_requirements(s))
        .unwrap_or_default();
    let identifiers = u.format.requirements_file().is_some();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    speccoder_core::Readiness {
        requirements: if identifiers {
            count(u.requirements().len())
        } else {
            count(all.len())
        },
        scenarios: count(all.iter().map(|r| r.scenarios.len()).sum()),
        open_questions: count(open_questions),
        untestable: all
            .iter()
            .filter(|r| r.scenarios.is_empty())
            .map(|r| r.key.clone())
            .collect(),
        identifiers,
    }
}

/// The readiness of `feature` (default: the current one) now; `None` when
/// there is no such unit.
#[must_use]
pub fn readiness(path: &Path, feature: Option<&str>) -> Option<speccoder_core::Readiness> {
    let u = unit("review", path, feature).ok()?;
    let open = questions(&u).len();
    Some(readiness_of(&u, open))
}

/// The confirmed reproductions of the bugs recorded in bug unit `unit`,
/// each with the content kept when it was confirmed (digest checked).
fn confirmed_reproductions(plane: &Path, unit: &str) -> Vec<speccoder_core::oracle::BugTests> {
    let mut bugs: Vec<speccoder_core::bug::BugReport> = crate::bug::state::all(plane)
        .into_iter()
        .filter(|b| b.unit == unit && b.confirmation.is_some())
        .collect();
    bugs.sort_by(|a, b| a.task.cmp(&b.task));
    bugs.into_iter()
        .filter_map(|b| {
            let paths: Vec<String> = b.reproductions.iter().map(|r| r.file.clone()).collect();
            let kept = crate::bug::state::repro(plane, &b.unit, &b.task, &paths);
            let reproductions = b
                .reproductions
                .iter()
                .map(|r| speccoder_core::oracle::BugTest {
                    statement: r.statement.clone(),
                    file: r.file.clone(),
                    test: r.test.clone(),
                    red_class: r
                        .baseline
                        .last()
                        .map(|c| crate::pipeline::class_word(*c))
                        .unwrap_or_default(),
                    content: kept
                        .iter()
                        .find(|(p, _)| *p == r.file)
                        .filter(|(_, c)| {
                            format!("sha256:{}", crate::integrity::sha256_hex(c.as_bytes()))
                                == r.digest
                        })
                        .map(|(_, c)| c.clone()),
                })
                .collect();
            Some(speccoder_core::oracle::BugTests {
                task: b.task.clone(),
                title: b.title.clone(),
                confirmation: b.confirmation.clone()?,
                reproductions,
            })
        })
        .collect()
}

/// Whether a feature run holds this project now.
fn run_active(u: &Unit) -> bool {
    crate::worktree::state_dir()
        .and_then(|s| crate::progress::read_live(&u.root, &s))
        .is_some_and(|(live, running)| running && live.finished.is_none())
}

/// Why autopilot cannot run here: it needs the OS sandbox enforced.
fn autopilot_blocked(root: &Path) -> Option<String> {
    let text = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let mode = crate::sandbox::parse(&text)
        .map(|c| c.mode)
        .unwrap_or(crate::sandbox::Mode::Auto);
    if mode == crate::sandbox::Mode::Off {
        return Some("autopilot needs the OS sandbox ([sandbox] mode = \"off\")".to_owned());
    }
    crate::sandbox::backend()
        .err()
        .map(|e| format!("autopilot needs the OS sandbox ({e})"))
}

/// The sandbox line of the approve sheet.
fn sandbox_line(root: &Path) -> Option<String> {
    let text = crate::verify::find_config(root)
        .and_then(|d| std::fs::read_to_string(d.join(crate::verify::config::FILE)).ok())
        .unwrap_or_default();
    let config = crate::sandbox::parse(&text).ok()?;
    let backend = crate::sandbox::backend().ok()?;
    let network = match config.network {
        crate::sandbox::Network::Off => "network off",
        crate::sandbox::Network::Loopback => "loopback only",
        crate::sandbox::Network::On => "network on",
    };
    let backend = match backend {
        crate::sandbox::Backend::Seatbelt => "Seatbelt",
        crate::sandbox::Backend::Landlock(_) => "Landlock+seccomp",
    };
    Some(format!("Sandbox: {backend}, {network}"))
}

/// What *Approve and Run* would approve, what blocks it, and what an
/// autopilot run may hold.
fn sheet(
    u: &Unit,
    items: &[ReviewItem],
    questions: &[OpenQuestion],
    oracle: &[speccoder_core::oracle::OracleView],
) -> speccoder_core::oracle::ApproveSheet {
    let mut blocked = Vec::new();
    if !questions.is_empty() {
        blocked.push(format!(
            "{} open question(s): {}",
            questions.len(),
            questions
                .iter()
                .map(|q| q.near.clone().unwrap_or_else(|| q.id.clone()))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for v in oracle {
        for a in v.open_ambiguities() {
            blocked.push(format!(
                "{}: two readings of {} disagree — choose one, or reword",
                v.req, a.scenario
            ));
        }
    }
    if run_active(u) {
        blocked.push("a run is active".to_owned());
    }
    // What the approval would record: the click sends it back, and a
    // change in between refuses it (REVIEW-CHANGED).
    let sha256 = match (&u.state, u.record()) {
        (Some(plane), Ok(record)) => {
            match crate::oracle::freezable(plane, &u.feature, &u.oracle_requirements()) {
                Ok(f) => Some(would_approve(&u.requirements(), &record, &f)),
                Err(t) => {
                    blocked.push(format!("a test draft cannot be read: {}", t.0));
                    None
                }
            }
        }
        (_, Err(e)) => {
            blocked.push(e);
            None
        }
        (None, Ok(_)) => None,
    };
    speccoder_core::oracle::ApproveSheet {
        sha256,
        approves: items
            .iter()
            .filter(|i| matches!(i.state.as_str(), "open" | "lapsed"))
            .map(|i| i.id.clone())
            .collect(),
        blocked,
        autopilot_blocked: autopilot_blocked(&u.root),
        sandbox: sandbox_line(&u.root),
        grants: u
            .state
            .as_ref()
            .map(|s| crate::approvals::offered_grants(s, &u.feature))
            .unwrap_or_default(),
    }
}

/// The frozen oracle entries and the approval digest `review.json` in
/// `dir` (project-relative, under `root`) holds.
///
/// # Errors
/// The record exists but cannot be read: never read as "nothing frozen".
pub fn oracle_entries(
    root: &Path,
    dir: &str,
) -> Result<(BTreeMap<String, crate::oracle::OracleEntry>, Option<String>), String> {
    let record = read_record(&root.join(dir).join(REVIEW_FILE))?;
    Ok((record.oracle, record.approval.map(|a| a.sha256)))
}

/// `review.json` at `file`: absent is the empty record; present and not
/// valid is an error naming it.
fn read_record(file: &Path) -> Result<Record, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a valid review record: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Record::default()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// The refusal for a review record that cannot be read.
fn unreadable(e: &str) -> CommandResult {
    CommandResult::new("review", CommandStatus::NeedsHuman, e).with_code("REVIEW-UNREADABLE")
}

fn message(r: &ReviewReport) -> String {
    let t = r.totals;
    let n = r.requirements.len();
    if n == 0 {
        return format!("{}'s spec names no requirement IDs to review", r.feature);
    }
    if t.complete() {
        return format!("{n}/{n} requirements approved");
    }
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let parts: Vec<String> = [
        (
            t.questions,
            plural(t.questions, "open question", "open questions"),
        ),
        (t.open, format!("{} to review", t.open)),
        (
            t.changes_requested,
            format!("{} with changes requested", t.changes_requested),
        ),
        (
            t.lapsed,
            format!("{} changed since they were decided", t.lapsed),
        ),
    ]
    .into_iter()
    .filter(|(k, _)| *k > 0)
    .map(|(_, w)| w)
    .collect();
    format!(
        "{}/{n} requirements approved; {}",
        t.approved,
        parts.join(", ")
    )
}

/// The review without its git reads (changes, history): what the inbox
/// needs, per feature, on every refresh.
#[must_use]
pub fn summary(path: &Path, feature: Option<&str>) -> Option<ReviewReport> {
    unit("review", path, feature)
        .ok()
        .map(|u| report_of(&u, false))
}

/// `speccoder review`: every requirement and its review.
#[must_use]
pub fn report(path: &Path, feature: Option<&str>) -> CommandResult {
    report_in(path, feature, None)
}

/// [`report`] with the state directory given (`None`: the environment's).
#[must_use]
pub fn report_in(path: &Path, feature: Option<&str>, state: Option<&Path>) -> CommandResult {
    let u = match unit_in("review", path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let mut r = report_of(&u, true);
    r.sheet = Some(sheet(&u, &r.requirements, &r.questions, &r.oracle));
    CommandResult::new("review", CommandStatus::Ok, &message(&r))
        .with_data(CommandData::Review(Box::new(r)))
}

/// `speccoder review approve|request|clear [ID…]`: records the person's
/// decision on the requirements named (none: every one not approved on its
/// current wording) and writes `review.json`. `request` needs a comment.
#[must_use]
pub fn decide(
    path: &Path,
    feature: Option<&str>,
    ids: &[String],
    verdict: &str,
    comment: Option<&str>,
    via: speccoder_core::actions::Via,
) -> CommandResult {
    decide_in(path, feature, ids, verdict, comment, via, None)
}

/// [`decide`] with the state directory given (`None`: the environment's).
#[must_use]
pub fn decide_in(
    path: &Path,
    feature: Option<&str>,
    ids: &[String],
    verdict: &str,
    comment: Option<&str>,
    via: speccoder_core::actions::Via,
    state: Option<&Path>,
) -> CommandResult {
    let u = match unit_in("review", path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let comment = comment.map(str::trim).filter(|c| !c.is_empty());
    if verdict == "request" && comment.is_none() {
        return CommandResult::new(
            "review",
            CommandStatus::UsageError,
            "a change request needs a comment: what should the requirement say?",
        )
        .with_code("USAGE");
    }
    let requirements = u.requirements();
    let mut record = match u.record() {
        Ok(r) => r,
        Err(e) => return unreadable(&e),
    };
    let before = items(&requirements, &record);
    // A wording that still asks a question is not one to sign: approving
    // it is refused; approving "the rest" passes it by and says so.
    let asks = |line: &str| line.contains(MARKER);
    let undecided: Vec<&str> = requirements
        .iter()
        .filter(|(_, l)| asks(l))
        .map(|(id, _)| id.as_str())
        .collect();
    if verdict == "approve" && !ids.is_empty() {
        let blocked: Vec<&str> = ids
            .iter()
            .map(String::as_str)
            .filter(|id| undecided.contains(id))
            .collect();
        if !blocked.is_empty() {
            return CommandResult::new(
                "review",
                CommandStatus::UsageError,
                &format!(
                    "{} still {} an open question: answer it first (`speccoder review answer …`), or reword it (`speccoder review edit {} \"…\" --approve`)",
                    blocked.join(", "),
                    if blocked.len() == 1 { "asks" } else { "ask" },
                    blocked[0]
                ),
            )
            .with_code("USAGE");
        }
    }
    let chosen: Vec<&(String, String)> = if ids.is_empty() {
        requirements
            .iter()
            .filter(|(id, l)| {
                before.iter().any(|i| i.id == *id && i.state != "approved")
                    && !(verdict == "approve" && asks(l))
            })
            .collect()
    } else {
        let unknown: Vec<&String> = ids
            .iter()
            .filter(|id| !requirements.iter().any(|(r, _)| r == *id))
            .collect();
        if !unknown.is_empty() {
            return CommandResult::new(
                "review",
                CommandStatus::NotFound,
                &format!(
                    "the spec has no {}; it names {}",
                    unknown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    requirements
                        .iter()
                        .map(|(r, _)| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_code("NOT-FOUND")
            .with_fix(vec!["speccoder review".to_owned()]);
        }
        requirements
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .collect()
    };
    let who = crate::approvals::Who::local_in(&u.root, via);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for (id, line) in &chosen {
        match verdict {
            "clear" => {
                record.requirements.remove(id);
            }
            "approve" | "request" => {
                record.requirements.insert(
                    id.clone(),
                    Decision {
                        verdict: if verdict == "approve" {
                            "approved"
                        } else {
                            "changes_requested"
                        }
                        .to_owned(),
                        line: line.trim_end().to_owned(),
                        sha256: digest(line),
                        by: who.name.clone(),
                        via: who.via.clone(),
                        at: now,
                        comment: comment.map(str::to_owned),
                    },
                );
            }
            other => {
                return CommandResult::new(
                    "review",
                    CommandStatus::UsageError,
                    &format!("`{other}` is not a review verdict: approve, request or clear"),
                )
                .with_code("USAGE");
            }
        }
    }
    // Decisions on requirements the spec no longer names go with them.
    record
        .requirements
        .retain(|id, _| requirements.iter().any(|(r, _)| r == id));
    let named = u.oracle_requirements();
    if u.snapshot.is_some() {
        record
            .oracle
            .retain(|key, _| named.iter().any(|r| r.key == *key));
    }
    let text = serde_json::to_string_pretty(&record).unwrap_or_default() + "\n";
    if let Err(e) = std::fs::write(u.file(), text) {
        return CommandResult::new("review", CommandStatus::Error, &format!("{}: {e}", u.rel()));
    }
    let r = report_of(&u, true);
    let word = match verdict {
        "approve" => "approved",
        "request" => "changes requested on",
        _ => "cleared",
    };
    let skipped = if verdict == "approve" && ids.is_empty() && !undecided.is_empty() {
        format!(
            "; not {} — {} still {} an open question",
            undecided.join(", "),
            if undecided.len() == 1 { "it" } else { "they" },
            if undecided.len() == 1 { "asks" } else { "ask" }
        )
    } else {
        String::new()
    };
    let ambiguous: Vec<String> = r
        .oracle
        .iter()
        .filter(|v| v.open_ambiguities().next().is_some())
        .map(|v| v.req.clone())
        .collect();
    let skipped = if verdict == "approve" && !ambiguous.is_empty() {
        format!(
            "{skipped}; {} {} ambiguous — Approve and Run stays blocked",
            ambiguous.join(", "),
            if ambiguous.len() == 1 { "is" } else { "are" }
        )
    } else {
        skipped
    };
    let message = if chosen.is_empty() {
        format!("nothing to decide{skipped}: {}", message(&r))
    } else {
        format!(
            "{word} {}{skipped}; {} — commit {} to share it",
            chosen
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            message(&r),
            r.file
        )
    };
    CommandResult::new("review", CommandStatus::Ok, &message)
        .with_data(CommandData::Review(Box::new(r)))
}

/// The approval digest *Approve and Run* records when it runs now: every
/// decision line approved after it (each open or lapsed one approved on
/// its wording), and the requirements it freezes — what the sheet shows,
/// so the click binds to it.
fn would_approve(
    requirements: &[(String, String)],
    record: &Record,
    freezable: &[crate::oracle::Freezable],
) -> String {
    let before = items(requirements, record);
    let approved: Vec<(String, String)> = requirements
        .iter()
        .filter_map(|(id, line)| {
            let state = before.iter().find(|i| i.id == *id)?.state.as_str();
            match state {
                "open" | "lapsed" => Some((id.clone(), digest(line))),
                _ => record
                    .requirements
                    .get(id)
                    .filter(|d| d.verdict == "approved")
                    .map(|d| (id.clone(), d.sha256.clone())),
            }
        })
        .collect();
    approval_digest(
        approved.iter().map(|(a, b)| (a.as_str(), b.as_str())),
        freezable
            .iter()
            .map(|f| (f.req.key.as_str(), f.manifest.as_str())),
    )
}

/// The approval digest (research R6): SHA-256 over the approved decision
/// lines (`ID=line digest`, sorted) and the frozen oracle entries
/// (`REQ=manifest digest`, sorted).
#[must_use]
pub fn approval_digest<'a>(
    decisions: impl IntoIterator<Item = (&'a str, &'a str)>,
    oracle: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    let mut lines: Vec<String> = decisions
        .into_iter()
        .map(|(id, sha)| format!("{id}={sha}"))
        .collect();
    lines.sort();
    let mut frozen: Vec<String> = oracle
        .into_iter()
        .map(|(req, sha)| format!("{req}={sha}"))
        .collect();
    frozen.sort();
    format!(
        "sha256:{}",
        crate::integrity::sha256_hex(
            format!("{}\n{}", lines.join("\n"), frozen.join("\n")).as_bytes()
        )
    )
}

/// `speccoder review approve --run ask|autopilot [--grant …]` — *Approve
/// and Run*: approves every decision line that is open or lapsed, freezes
/// each requirement's red tests (one pre-frozen set each, `review.json`'s
/// `oracle` and `approval`), commits `review.json` and starts the
/// feature's run. Refused while a question is open, a requirement is read
/// two ways, or a run is active.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn approve_and_run(
    path: &Path,
    feature: Option<&str>,
    mode: speccoder_core::oracle::RunMode,
    grants: &[String],
    sheet: Option<&str>,
    via: speccoder_core::actions::Via,
    state: Option<&Path>,
    provider: Option<std::sync::Arc<dyn agentplane::model::ModelProvider>>,
    options: &crate::RunOptions,
) -> CommandResult {
    use speccoder_core::oracle::RunMode;
    let u = match unit_in("review", path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let Some(plane) = u.state.clone() else {
        return CommandResult::new(
            "review",
            CommandStatus::ConfigError,
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )
        .with_code("NO-STATE-DIR");
    };
    let refuse =
        |status, code: &str, m: String| CommandResult::new("review", status, &m).with_code(code);
    if run_active(&u) {
        return refuse(
            CommandStatus::GitPrecondition,
            "RUN-ACTIVE",
            format!(
                "a run is active for {}: approve and run when it ends",
                u.feature
            ),
        );
    }
    // Nothing writes the unit's tests while they are frozen.
    let lock = match crate::oracle::UnitLock::take(&plane, &u.feature) {
        Ok(l) => l,
        Err(e) => return refuse(CommandStatus::GitPrecondition, "RUN-ACTIVE", e),
    };
    let open = questions(&u);
    if !open.is_empty() {
        return refuse(
            CommandStatus::NeedsHuman,
            "REVIEW-PENDING",
            format!(
                "{} open question(s) first: {} — answer (`speccoder review answer Q1 \"…\"`) or reword",
                open.len(),
                open.iter()
                    .map(|(q, _)| q.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    let named = u.oracle_requirements();
    let ambiguous = match crate::oracle::open_ambiguities(&plane, &u.feature, &named) {
        Ok(a) => a,
        Err(t) => return refuse(CommandStatus::NeedsHuman, "ORACLE-TAMPERED", t.0),
    };
    if !ambiguous.is_empty() {
        return refuse(
            CommandStatus::NeedsHuman,
            "REVIEW-AMBIGUOUS",
            format!(
                "Approve and Run waits: {} read two ways — choose a reading (`speccoder review tests {} --reading A|B {}`) or reword",
                ambiguous
                    .iter()
                    .map(|(r, s)| format!("{r}/{s}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                ambiguous[0].0,
                ambiguous[0].1
            ),
        );
    }
    // Autopilot: an enforced sandbox, and only grants offered for this
    // feature, by their grant id.
    let mut objects = Vec::new();
    if mode == RunMode::Autopilot {
        if let Some(why) = autopilot_blocked(&u.root) {
            return refuse(CommandStatus::ConfigError, "SANDBOX-UNAVAILABLE", why).with_fix(vec![
                "speccoder review approve --run ask".to_owned(),
                "→ or enable the OS sandbox ([sandbox] mode = \"auto\" on macOS or Linux 6.2+)"
                    .to_owned(),
            ]);
        }
        let offered = crate::approvals::offered_objects(&plane, &u.feature);
        for id in grants {
            match offered.iter().find(|(g, _)| g.id == *id) {
                Some((_, o)) => objects.push(o.clone()),
                None => {
                    return refuse(
                        CommandStatus::UsageError,
                        "USAGE",
                        format!(
                            "{id} is not a grant offered for {}; offered: {}",
                            u.feature,
                            if offered.is_empty() {
                                "none".to_owned()
                            } else {
                                offered
                                    .iter()
                                    .map(|(g, _)| format!("{} ({})", g.id, g.object))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        ),
                    );
                }
            }
        }
    } else if !grants.is_empty() {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            "--grant is for an autopilot run (--run autopilot)".to_owned(),
        );
    }
    let freezable = match crate::oracle::freezable(&plane, &u.feature, &named) {
        Ok(f) => f,
        Err(t) => {
            return refuse(CommandStatus::NeedsHuman, "ORACLE-TAMPERED", t.0);
        }
    };
    // The decision lines: every one open or lapsed is approved now.
    let requirements = u.requirements();
    let mut record = match u.record() {
        Ok(r) => r,
        Err(e) => return unreadable(&e),
    };
    let before = items(&requirements, &record);
    let who = crate::approvals::Who::local_in(&u.root, via);
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut approved_now = Vec::new();
    for (id, line) in &requirements {
        if before
            .iter()
            .any(|i| i.id == *id && matches!(i.state.as_str(), "open" | "lapsed"))
        {
            record.requirements.insert(
                id.clone(),
                Decision {
                    verdict: "approved".to_owned(),
                    line: line.trim_end().to_owned(),
                    sha256: digest(line),
                    by: who.name.clone(),
                    via: who.via.clone(),
                    at,
                    comment: None,
                },
            );
            approved_now.push(id.clone());
        }
    }
    record
        .requirements
        .retain(|id, _| requirements.iter().any(|(r, _)| r == id));
    let approved: Vec<(String, String)> = record
        .requirements
        .iter()
        .filter(|(_, d)| d.verdict == "approved")
        .map(|(id, d)| (id.clone(), d.sha256.clone()))
        .collect();
    let sha = approval_digest(
        approved.iter().map(|(a, b)| (a.as_str(), b.as_str())),
        freezable
            .iter()
            .map(|f| (f.req.key.as_str(), f.manifest.as_str())),
    );
    // A click binds to what its sheet showed; a terminal command acts on
    // what it reads now.
    if sheet.is_none() && via != speccoder_core::actions::Via::Terminal {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            "Approve and Run names what its sheet showed (the sheet's sha256): open the sheet again".to_owned(),
        );
    }
    if let Some(shown) = sheet
        && shown != sha
    {
        return refuse(
            CommandStatus::NeedsHuman,
            "REVIEW-CHANGED",
            "what Approve and Run would approve and freeze changed since the sheet showed it (another tab, a finished Write Tests): nothing was approved — look at the sheet again".to_owned(),
        );
    }
    record.oracle.clear();
    let mut frozen = Vec::new();
    for f in &freezable {
        let decided = speccoder_core::acceptance::Decided {
            by: who.name.clone(),
            via,
            at,
        };
        match crate::oracle::freeze(&plane, &u.feature, f, &sha, decided) {
            Ok(rec) => {
                record.oracle.insert(
                    f.req.key.clone(),
                    crate::oracle::OracleEntry {
                        requirement_sha256: f.draft.requirement.sha256.clone(),
                        manifest_sha256: f.manifest.clone(),
                        held_out_sha256: rec
                            .held_out_sealed
                            .as_ref()
                            .map(|h| format!("sha256:{}", h.manifest_sha256)),
                        by: who.name.clone(),
                        via,
                        at,
                    },
                );
                frozen.push((f, rec));
            }
            Err(e) => return refuse(CommandStatus::NeedsHuman, "ORACLE-TAMPERED", e),
        }
    }
    record.approval = Some(speccoder_core::oracle::ReviewApproval {
        sha256: sha.clone(),
        mode,
        by: who.name.clone(),
        via,
        at,
    });
    let text = serde_json::to_string_pretty(&record).unwrap_or_default() + "\n";
    if let Err(e) = std::fs::write(u.file(), text) {
        return CommandResult::new("review", CommandStatus::Error, &format!("{}: {e}", u.rel()));
    }
    // The run reads the committed spec: the approval is committed with it.
    let rel = {
        let (top_prefix, rel) = (
            crate::git::Git::at(&u.root)
                .line(&["rev-parse", "--show-prefix"])
                .unwrap_or_default(),
            u.rel(),
        );
        format!("{top_prefix}{rel}")
    };
    let committed = crate::hosting::commit_exactly(
        &u.root,
        &[rel],
        &format!("review: approve {} and freeze its tests", u.feature),
    );
    if committed.status() != CommandStatus::Ok {
        return CommandResult::carried("review", &committed);
    }
    for (f, rec) in &frozen {
        if let Some(p) = &options.progress {
            p.event(&crate::progress::Event::Oracle {
                step: crate::progress::OracleEvent::Frozen {
                    unit: u.feature.clone(),
                    requirement: f.req.key.clone(),
                    manifest_sha256: f.manifest.clone(),
                    approval_sha256: sha.clone(),
                    visible_count: rec.visible.len(),
                    held_out_count: rec.held_out_sealed.as_ref().map_or(0, |h| h.count),
                    by: who.name.clone(),
                    via: via.as_str().to_owned(),
                },
            });
        }
    }
    let said = format!(
        "approved {} decision line(s), froze tests for {} requirement(s){}; committed {}",
        approved_now.len(),
        frozen.len(),
        if mode == RunMode::Autopilot {
            format!(", autopilot with {} grant(s)", objects.len())
        } else {
            String::new()
        },
        u.rel()
    );
    drop(lock);
    let mut run_options = options.clone();
    run_options.mode = mode;
    if mode == RunMode::Autopilot {
        // Nobody is asked: the ticked grants are the run's only grants.
        run_options.approver = None;
        run_options.run_grants = std::sync::Arc::new(std::sync::Mutex::new(objects));
    }
    let ran = match (state, provider) {
        (Some(state), Some(provider)) => crate::pipeline::run_feature_with(
            path,
            Some(&u.feature),
            state,
            &provider,
            None,
            &run_options,
        ),
        _ => crate::run_feature(path, Some(&u.feature), &run_options, false),
    };
    let status = ran.status();
    let message = format!("{said} — {}", ran.message());
    ran.reworded(status, &message)
}

/// `speccoder review answer Q1 "…"`: writes the person's answer where the
/// question was — the marker replaced by the answer, nothing else touched.
#[must_use]
pub fn answer(path: &Path, feature: Option<&str>, id: &str, text: &str) -> CommandResult {
    let u = match unit("review", path, feature) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let text = text.trim();
    if text.is_empty() {
        return CommandResult::new("review", CommandStatus::UsageError, "an answer needs words")
            .with_code("USAGE");
    }
    let open = questions(&u);
    let Some((q, raw)) = open.iter().find(|(q, _)| q.id.eq_ignore_ascii_case(id)) else {
        return CommandResult::new(
            "review",
            CommandStatus::NotFound,
            &format!(
                "no open question {id}; open: {}",
                if open.is_empty() {
                    "none".to_owned()
                } else {
                    open.iter()
                        .map(|(q, _)| q.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ),
        )
        .with_code("NOT-FOUND");
    };
    let file = u.root.join(&q.file);
    let Ok(body) = std::fs::read_to_string(&file) else {
        return CommandResult::new(
            "review",
            CommandStatus::Error,
            &format!("{} cannot be read", q.file),
        );
    };
    let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
    let Some(line) = lines
        .get_mut(q.line - 1)
        .filter(|l| l.contains(raw.as_str()))
    else {
        return CommandResult::new(
            "review",
            CommandStatus::Error,
            &format!("{} changed under the question; look again", q.file),
        );
    };
    *line = line.replacen(raw.as_str(), text, 1);
    let mut out = lines.join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    if let Err(e) = std::fs::write(&file, out) {
        return CommandResult::new("review", CommandStatus::Error, &format!("{}: {e}", q.file));
    }
    let r = report_of(&u, true);
    CommandResult::ok(
        "review",
        &format!("answered {} in {} — {}", q.id, q.file, message(&r)),
    )
    .with_data(CommandData::Review(Box::new(r)))
}

/// `speccoder review edit FR-003 "…" [--approve]`: the person rewords a
/// requirement — its line keeps its identifier, its decision lapses (or is
/// approved on the new words with `approve`).
#[must_use]
pub fn edit(
    path: &Path,
    feature: Option<&str>,
    id: &str,
    wording: &str,
    approve: bool,
    via: speccoder_core::actions::Via,
) -> CommandResult {
    let u = match unit("review", path, feature) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let wording = wording.trim();
    if wording.is_empty() || wording.contains('\n') {
        return CommandResult::new(
            "review",
            CommandStatus::UsageError,
            "a requirement's wording is one line of words",
        )
        .with_code("USAGE");
    }
    let Some(spec) = u.format.requirements_file() else {
        return CommandResult::new(
            "review",
            CommandStatus::UsageError,
            "this format has no requirement identifiers",
        )
        .with_code("USAGE");
    };
    let Some((_, old)) = u.requirements().into_iter().find(|(r, _)| r == id) else {
        return CommandResult::new(
            "review",
            CommandStatus::NotFound,
            &format!("the spec has no {id}"),
        )
        .with_code("NOT-FOUND");
    };
    // `- **FR-003**: …` keeps everything up to the identifier and its colon.
    let key = format!("**{id}**");
    let Some(at) = old.find(&key) else {
        return CommandResult::new(
            "review",
            CommandStatus::Error,
            &format!("{id}'s line has no `{key}`"),
        );
    };
    let new = format!("{}{key}: {wording}", &old[..at]);
    let file = u.root.join(&u.dir).join(spec);
    let body = std::fs::read_to_string(&file).unwrap_or_default();
    if !body.lines().any(|l| l.trim_end() == old.trim_end()) {
        return CommandResult::new(
            "review",
            CommandStatus::Error,
            &format!("{spec} changed under {id}; look again"),
        );
    }
    let mut done = false;
    let mut out: Vec<String> = Vec::new();
    for l in body.lines() {
        if !done && l.trim_end() == old.trim_end() {
            out.push(new.clone());
            done = true;
        } else {
            out.push(l.to_owned());
        }
    }
    let mut text = out.join("\n");
    if body.ends_with('\n') {
        text.push('\n');
    }
    if let Err(e) = std::fs::write(&file, text) {
        return CommandResult::new("review", CommandStatus::Error, &format!("{spec}: {e}"));
    }
    if approve {
        let r = decide(path, feature, &[id.to_owned()], "approve", None, via);
        let status = r.status();
        let message = if status == CommandStatus::Ok {
            format!("reworded {id} and approved it; {}", r.message())
        } else {
            format!("reworded {id}; not approved: {}", r.message())
        };
        return r.reworded(status, &message);
    }
    let r = report_of(&u, true);
    CommandResult::ok("review", &format!("reworded {id}; {}", message(&r)))
        .with_data(CommandData::Review(Box::new(r)))
}

/// The arguments for the revising skill: each change request with its
/// requirement, so the spec is revised to say what the person asked.
#[must_use]
fn revision_request(items: &[ReviewItem]) -> String {
    let lines: Vec<String> = items
        .iter()
        .filter(|i| i.state == "changes_requested")
        .map(|i| {
            format!(
                "- {} (\"{}\"): {}",
                i.id,
                i.text,
                i.comment.as_deref().unwrap_or_default()
            )
        })
        .collect();
    format!(
        "Revise the spec per the reviewer's change requests; keep every requirement ID, reword only what is asked:\n{}",
        lines.join("\n")
    )
}

/// The review records under `root`'s units, as they are before a hosted
/// skill runs: it must not change them, nor leave one in a unit it creates.
#[derive(Debug)]
pub struct Snapshot {
    root: PathBuf,
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
}

/// Every unit's review record under `root` (absent ones as `None`).
fn records(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let format = crate::format::at(root);
    let Some(snap) = format.inspect(root, None) else {
        return Vec::new();
    };
    snap.features
        .iter()
        .map(|f| {
            let file = Path::new(&snap.root).join(&f.dir).join(REVIEW_FILE);
            let bytes = std::fs::read(&file).ok();
            (file, bytes)
        })
        .collect()
}

/// The records as they are now.
#[must_use]
pub fn snapshot(root: &Path) -> Snapshot {
    Snapshot {
        root: root.to_path_buf(),
        files: records(root),
    }
}

/// Puts back every record that changed since `before`, and removes one in
/// a unit that appeared since; the paths restored.
pub fn restore(before: &Snapshot) -> Vec<String> {
    let mut restored = Vec::new();
    let now = records(&before.root);
    for (file, bytes) in &now {
        let was = before
            .files
            .iter()
            .find(|(f, _)| f == file)
            .and_then(|(_, b)| b.clone());
        if *bytes == was {
            continue;
        }
        let ok = match &was {
            Some(b) => std::fs::write(file, b).is_ok(),
            None => std::fs::remove_file(file).is_ok(),
        };
        if ok {
            restored.push(file.display().to_string());
        }
    }
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wording_is_the_line_without_its_markup() {
        assert_eq!(
            wording("- **FR-001**: Users MUST log in."),
            "Users MUST log in."
        );
        assert_eq!(digest("a  "), digest("a"));
    }
}
