//! `speccoder review` (F-087): the person's review of the spec, one
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
    fn record(&self) -> Record {
        std::fs::read_to_string(self.file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

fn unit(command: &str, path: &Path, feature: Option<&str>) -> Result<Unit, Box<CommandResult>> {
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
    Ok(Unit {
        feature: f.feature.name.clone(),
        root: PathBuf::from(&snapshot.root),
        dir: f.feature.dir.clone(),
        format: crate::format::of(snapshot),
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
    };
    let requirements = u.requirements();
    (!requirements.is_empty())
        .then(|| totals(&items(&requirements, &u.record()), questions(&u).len()))
}

/// The review; `full` adds what changed since the last commit and the
/// history (git reads, for the review view — not for the inbox).
fn report_of(u: &Unit, full: bool) -> ReviewReport {
    let items = items(&u.requirements(), &u.record());
    let skills = crate::skills::discover_with(&u.root, u.format.planning());
    let installed = |name: &str| {
        let full = format!("{}{name}", u.format.planning().prefix);
        skills.iter().any(|s| s.name == full && s.refused.is_none())
    };
    let open: Vec<OpenQuestion> = questions(u).into_iter().map(|(q, _)| q).collect();
    let totals = totals(&items, open.len());
    let revision = (totals.changes_requested > 0).then(|| revision_request(&items));
    let changes = if full {
        requirements_at(u, "HEAD")
            .map(|then| compare(&then, &u.requirements()))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
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
    }
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
    let u = match unit("review", path, feature) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let r = report_of(&u, true);
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
    via: &str,
) -> CommandResult {
    let u = match unit("review", path, feature) {
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
    let mut record = u.record();
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
    via: &str,
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
