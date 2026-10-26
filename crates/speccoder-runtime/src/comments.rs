//! Anchored review comments (spec 093, F-108): the person comments on a
//! passage of a feature document, a line of the spec's changes, a line of a
//! shown visible test or a finding; the drafts collect in a tray in the
//! state directory (`<state>/comments/<unit>.json`, never browser storage,
//! never the worktree) and go out together as one revision turn of the
//! format's revising skill, scoped to the sections they touch. Before the
//! turn the feature's documents are snapshot; after it the change is shown
//! per section, sections no comment touched flagged; *Undo Revision*
//! restores the snapshot byte for byte, on the person's command only
//! (constitution 2.3.0, principle I). Code-view line comments become one
//! nudge for a task's next attempt.
//!
//! Nothing here reads a held-out store: an anchor outside the feature's
//! documents, its changes, its shown tests and its findings is refused.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use serde::{Deserialize, Serialize};
use speccoder_core::actions::{CommentOp, Via};
use speccoder_core::oracle::{
    Anchor, CodeComment, CommentDraft, CommentsView, DraftState, RevisionView,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::integrity::sha256_hex;
use crate::worktree;

/// The store's directory under a project's state directory.
pub const DIR: &str = "comments";

/// How far a quote may move from its anchored line and still be found.
const DRIFT: usize = 20;

/// A file's digest before or after a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDigest {
    /// Project-relative path.
    pub path: String,
    /// `sha256:<hex>`, or `absent`.
    pub sha256: String,
}

/// One revision turn as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// `t1`, `t2`, …
    pub id: String,
    /// The drafts it was sent with.
    pub comments: Vec<String>,
    /// The sections they touch.
    pub sections: Vec<String>,
    /// The skill.
    pub skill: String,
    /// The documents before the turn.
    pub before: Vec<FileDigest>,
    /// After it.
    pub after: Vec<FileDigest>,
    /// Sections it changed.
    pub changed: Vec<String>,
    /// Of those, the ones no comment touched.
    pub outside: Vec<String>,
    /// `pending`, `kept` or `undone`.
    pub outcome: String,
    /// Requirements asked again from test comments.
    #[serde(default)]
    pub asked_again: Vec<String>,
}

/// The tray of one unit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tray {
    /// The next draft's number.
    #[serde(default)]
    pub next: u32,
    /// Every draft (sent ones too, until their turn is kept).
    #[serde(default)]
    pub drafts: Vec<CommentDraft>,
    /// The turns, oldest first.
    #[serde(default)]
    pub turns: Vec<Turn>,
}

fn file(project_state: &Path, unit: &str) -> PathBuf {
    project_state.join(DIR).join(format!("{unit}.json"))
}

fn snapshot_dir(project_state: &Path, unit: &str, turn: &str) -> PathBuf {
    project_state.join(DIR).join(unit).join(turn)
}

/// The tray of `unit` (empty when none was written).
#[must_use]
pub fn load(project_state: &Path, unit: &str) -> Tray {
    std::fs::read_to_string(file(project_state, unit))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(project_state: &Path, unit: &str, tray: &Tray) -> Result<(), String> {
    let f = file(project_state, unit);
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = f.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(tray).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &f).map_err(|e| format!("{}: {e}", f.display()))
}

/// One writer at a time: a send that finds the lock waits for nobody, it
/// is refused (a second browser's send of the same drafts).
struct Lock(PathBuf);

impl Lock {
    fn take(project_state: &Path, unit: &str) -> Result<Self, String> {
        let path = project_state.join(DIR).join(format!("{unit}.lock"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        // A lock older than an hour is a crashed writer's.
        if let Ok(meta) = std::fs::metadata(&path)
            && meta
                .modified()
                .ok()
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
            .map_err(|_| "the comments are being sent from elsewhere right now".to_owned())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// What a comment action works on.
struct Unit {
    root: PathBuf,
    feature: String,
    dir: String,
    format: &'static dyn crate::format::Adapter,
    snapshot: speccoder_core::snapshot::ProjectSnapshot,
    plane: PathBuf,
}

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new("review", status, message).with_code(code)
}

fn unit(
    path: &Path,
    feature: Option<&str>,
    state: Option<&Path>,
) -> Result<Unit, Box<CommandResult>> {
    let inspected = crate::inspect(path, feature);
    let Some(CommandData::Inspect(snapshot)) = inspected.data() else {
        return Err(Box::new(CommandResult::carried("review", &inspected)));
    };
    let Some(f) = snapshot.feature.as_ref() else {
        return Err(Box::new(CommandResult::new(
            "review",
            CommandStatus::SelectionNeeded,
            inspected.message(),
        )));
    };
    let root = PathBuf::from(&snapshot.root);
    let Some(state) = state.map(Path::to_path_buf).or_else(worktree::state_dir) else {
        return Err(Box::new(refuse(
            CommandStatus::ConfigError,
            "NO-STATE-DIR",
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        )));
    };
    Ok(Unit {
        plane: worktree::project_state_dir(&state, &root),
        feature: f.feature.name.clone(),
        dir: f.feature.dir.clone(),
        format: crate::format::of(snapshot),
        snapshot: (**snapshot).clone(),
        root,
    })
}

/// The unit's documents: every Markdown file under its directory,
/// project-relative, by path.
fn documents(u: &Unit) -> Vec<String> {
    let base = u.root.join(&u.dir);
    let mut out = Vec::new();
    let mut stack = vec![base];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "md")
                && let Ok(rel) = p.strip_prefix(&u.root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// The heading path of `line` (1-based) in Markdown `text`:
/// `Requirements › Functional Requirements`; empty before any heading.
#[must_use]
pub fn section_at(text: &str, line: usize) -> String {
    let mut path: Vec<(usize, String)> = Vec::new();
    for (i, l) in text.lines().enumerate() {
        if i + 1 > line {
            break;
        }
        let hashes = l.bytes().take_while(|b| *b == b'#').count();
        if hashes == 0 || !l[hashes..].starts_with(' ') {
            continue;
        }
        path.retain(|(level, _)| *level < hashes);
        path.push((hashes, l[hashes..].trim().to_owned()));
    }
    path.into_iter()
        .map(|(_, h)| h)
        .collect::<Vec<_>>()
        .join(" › ")
}

/// Each section of Markdown `text` by its heading path, with its body.
#[must_use]
pub fn sections(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut current = String::new();
    let mut body = String::new();
    for l in text.lines() {
        let hashes = l.bytes().take_while(|b| *b == b'#').count();
        if hashes > 0 && l[hashes..].starts_with(' ') {
            out.push((current.clone(), std::mem::take(&mut body)));
            path.retain(|(level, _)| *level < hashes);
            path.push((hashes, l[hashes..].trim().to_owned()));
            current = path
                .iter()
                .map(|(_, h)| h.as_str())
                .collect::<Vec<_>>()
                .join(" › ");
            continue;
        }
        body.push_str(l);
        body.push('\n');
    }
    out.push((current, body));
    out.retain(|(h, b)| !(h.is_empty() && b.trim().is_empty()));
    out
}

fn normal(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `quote` is in `text` within [`DRIFT`] lines of `start..=end`.
fn found(text: &str, start: usize, end: usize, quote: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let from = start.saturating_sub(DRIFT + 1);
    let to = (end + DRIFT).min(lines.len());
    if from >= to {
        return false;
    }
    normal(&lines[from..to].join("\n")).contains(&normal(quote))
}

fn read(u: &Unit, rel: &str) -> Option<String> {
    std::fs::read_to_string(u.root.join(rel)).ok()
}

fn at_head(u: &Unit, rel: &str) -> Option<String> {
    let git = crate::git::Git::at(&u.root);
    let top_prefix = git.line(&["rev-parse", "--show-prefix"]).unwrap_or_default();
    git.run(&["show", &format!("HEAD:{top_prefix}{rel}")])
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// A shown visible test of the unit: its content (from the oracle draft).
fn shown_test(u: &Unit, req: &str, path: &str) -> Option<String> {
    let (_, contents) = crate::oracle::load(&u.plane, &u.feature, req)
        .ok()
        .flatten()?;
    contents
        .into_iter()
        .find(|(p, _)| p == path)
        .map(|(_, c)| c)
}

/// Checks a new anchor against what the Review shows, and fills its
/// digest and section. `Err`: why it may not be anchored there.
fn check(u: &Unit, anchor: Anchor) -> Result<Anchor, String> {
    let docs = documents(u);
    let doc = |p: &str| -> Result<(), String> {
        if docs.iter().any(|d| d == p) {
            Ok(())
        } else {
            Err(format!(
                "{p} is not one of {}'s documents: a comment is anchored to the feature's documents, its changes, its shown tests or its findings",
                u.feature
            ))
        }
    };
    match anchor {
        Anchor::Passage {
            path,
            start_line,
            end_line,
            quote,
            ..
        } => {
            doc(&path)?;
            if start_line == 0 || end_line < start_line || quote.trim().is_empty() {
                return Err(
                    "a passage needs its lines (from ≤ to, from 1) and its words".to_owned(),
                );
            }
            let text = read(u, &path).unwrap_or_default();
            if !found(&text, start_line, end_line, &quote) {
                return Err(format!(
                    "\"{}\" is not at {path}:{start_line}-{end_line}",
                    quote.chars().take(60).collect::<String>()
                ));
            }
            Ok(Anchor::Passage {
                section: section_at(&text, start_line),
                quote_sha256: digest(normal(&quote).as_bytes()),
                path,
                start_line,
                end_line,
                quote,
            })
        }
        Anchor::DiffLine {
            path,
            side,
            line,
            quote,
        } => {
            doc(&path)?;
            let text = match side.as_str() {
                "new" => read(u, &path),
                "old" => at_head(u, &path),
                other => return Err(format!("a diff line's side is old or new, not {other}")),
            }
            .unwrap_or_default();
            if line == 0 || !found(&text, line, line, &quote) {
                return Err(format!("\"{quote}\" is not at {path}:{line} ({side})"));
            }
            Ok(Anchor::DiffLine {
                path,
                side,
                line,
                quote,
            })
        }
        Anchor::TestLine {
            req,
            path,
            line,
            quote,
        } => {
            let Some(content) = shown_test(u, &req, &path) else {
                return Err(format!("{req} shows no test {path}"));
            };
            if line == 0 || !found(&content, line, line, &quote) {
                return Err(format!("\"{quote}\" is not at {path}:{line}"));
            }
            Ok(Anchor::TestLine {
                req,
                path,
                line,
                quote,
            })
        }
        Anchor::Finding { source, id, text } => {
            if id.trim().is_empty() || text.trim().is_empty() {
                return Err("a finding needs its id and its words".to_owned());
            }
            let shown = match source.as_str() {
                // The review's own: a decision line or an open question.
                "review" => crate::review::summary(&u.root, Some(&u.feature)).is_some_and(|r| {
                    r.requirements.iter().any(|i| i.id == id)
                        || r.questions.iter().any(|q| q.id == id)
                }),
                // An analysis the format wrote into the unit's documents.
                "analyze" => docs
                    .iter()
                    .filter_map(|d| read(u, d))
                    .any(|t| t.contains(&id)),
                other => {
                    return Err(format!(
                        "findings of {other} are not shown in the Review (analyze and review findings are)"
                    ));
                }
            };
            if !shown {
                return Err(format!("the Review shows no {source} finding {id}"));
            }
            Ok(Anchor::Finding { source, id, text })
        }
    }
}

/// Whether a stored draft's quote is still where it was anchored.
fn current(u: &Unit, anchor: &Anchor) -> bool {
    match anchor {
        Anchor::Passage {
            path,
            start_line,
            end_line,
            quote,
            ..
        } => read(u, path).is_some_and(|t| found(&t, *start_line, *end_line, quote)),
        Anchor::DiffLine {
            path,
            side,
            line,
            quote,
        } => {
            let text = if side == "old" {
                at_head(u, path)
            } else {
                read(u, path)
            };
            text.is_some_and(|t| found(&t, *line, *line, quote))
        }
        Anchor::TestLine {
            req,
            path,
            line,
            quote,
        } => shown_test(u, req, path).is_some_and(|t| found(&t, *line, *line, quote)),
        Anchor::Finding { .. } => true,
    }
}

fn turn_view(t: &Turn) -> RevisionView {
    let files: Vec<String> = t
        .before
        .iter()
        .chain(&t.after)
        .filter(|f| {
            let a = t
                .before
                .iter()
                .find(|b| b.path == f.path)
                .map(|b| &b.sha256);
            let b = t.after.iter().find(|b| b.path == f.path).map(|b| &b.sha256);
            !t.after.is_empty() && a != b
        })
        .map(|f| f.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    RevisionView {
        id: t.id.clone(),
        comments: t.comments.clone(),
        sections: t.sections.clone(),
        skill: t.skill.clone(),
        changed: t.changed.clone(),
        outside: t.outside.clone(),
        outcome: t.outcome.clone(),
        files,
        asked_again: t.asked_again.clone(),
    }
}

fn view_of(u: &Unit, tray: &Tray) -> CommentsView {
    CommentsView {
        feature: u.feature.clone(),
        drafts: tray
            .drafts
            .iter()
            .filter(|d| d.state != DraftState::Sent)
            .cloned()
            .map(|mut d| {
                d.state = if d.kept || current(u, &d.anchor) {
                    DraftState::Draft
                } else {
                    DraftState::Outdated
                };
                d
            })
            .collect(),
        turn: tray.turns.last().map(turn_view),
        revise_with: u.format.planning().revises.map(str::to_owned),
    }
}

fn result(u: &Unit, tray: &Tray, message: &str) -> CommandResult {
    CommandResult::new("review", CommandStatus::Ok, message)
        .with_data(CommandData::Comments(Box::new(view_of(u, tray))))
}

/// `speccoder review comments`: the tray and the last turn.
#[must_use]
pub fn list(path: &Path, feature: Option<&str>, state: Option<&Path>) -> CommandResult {
    let u = match unit(path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let tray = load(&u.plane, &u.feature);
    let view = view_of(&u, &tray);
    let outdated = view
        .drafts
        .iter()
        .filter(|d| d.state == DraftState::Outdated)
        .count();
    let message = format!(
        "{} comment(s){}{}",
        view.drafts.len(),
        if outdated > 0 {
            format!(", {outdated} outdated")
        } else {
            String::new()
        },
        view.turn
            .as_ref()
            .map(|t| format!("; last revision {} {}", t.id, t.outcome))
            .unwrap_or_default()
    );
    result(&u, &tray, &message)
}

/// Adds, deletes or keeps a comment (or keeps a revision turn, `id` `tN`),
/// on the person's command.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn comment(
    path: &Path,
    feature: Option<&str>,
    op: CommentOp,
    anchor: Option<&Anchor>,
    id: Option<&str>,
    text: Option<&str>,
    via: Via,
    state: Option<&Path>,
) -> CommandResult {
    let u = match unit(path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let usage = |m: &str| refuse(CommandStatus::UsageError, "USAGE", m);
    let _lock = match Lock::take(&u.plane, &u.feature) {
        Ok(l) => l,
        Err(e) => return usage(&e),
    };
    let mut tray = load(&u.plane, &u.feature);
    let message = match op {
        CommentOp::Add => {
            let Some(anchor) = anchor else {
                return usage("a comment needs what it is about (its anchor)");
            };
            let Some(words) = text.map(str::trim).filter(|t| !t.is_empty()) else {
                return usage("a comment needs its words");
            };
            let anchor = match check(&u, anchor.clone()) {
                Ok(a) => a,
                Err(e) => return usage(&e),
            };
            tray.next += 1;
            let id = format!("c{}", tray.next);
            let place = anchor.place();
            tray.drafts.push(CommentDraft {
                id: id.clone(),
                anchor,
                text: words.to_owned(),
                by: crate::approvals::Who::local_in(&u.root, via).name,
                at: now(),
                state: DraftState::Draft,
                kept: false,
            });
            format!("added {id} on {place}")
        }
        CommentOp::Delete => {
            let Some(id) = id else {
                return usage("name the comment to delete");
            };
            let before = tray.drafts.len();
            tray.drafts
                .retain(|d| !(d.id == id && d.state != DraftState::Sent));
            if tray.drafts.len() == before {
                return usage(&format!("no comment {id} in the tray"));
            }
            format!("deleted {id}")
        }
        CommentOp::Keep => {
            let Some(id) = id else {
                return usage("name the comment (or the revision) to keep");
            };
            if let Some(t) = tray.turns.iter_mut().find(|t| t.id == id) {
                if t.outcome != "pending" {
                    return usage(&format!("revision {id} is {} already", t.outcome));
                }
                t.outcome = "kept".to_owned();
                let sent: Vec<String> = t.comments.clone();
                tray.drafts.retain(|d| !sent.contains(&d.id));
                format!("kept revision {id}")
            } else if let Some(d) = tray
                .drafts
                .iter_mut()
                .find(|d| d.id == id && d.state != DraftState::Sent)
            {
                d.kept = true;
                format!("kept {id}: it is sent although its words moved")
            } else {
                return usage(&format!("no comment or revision {id}"));
            }
        }
    };
    if let Err(e) = save(&u.plane, &u.feature, &tray) {
        return refuse(CommandStatus::Error, "INTERNAL", &e);
    }
    result(&u, &tray, &message)
}

/// The revising skill's arguments: each comment with its quote and section,
/// the sections touched, and that every other section stays as it is.
#[must_use]
pub fn revision_argument(drafts: &[CommentDraft], sections: &[String]) -> String {
    let mut out = String::from(
        "Revise the spec from the reviewer's comments below. Change only these sections: ",
    );
    out.push_str(&if sections.is_empty() {
        "(the ones the comments name)".to_owned()
    } else {
        sections.join("; ")
    });
    out.push_str(
        ". Keep every requirement ID. Every other section stays exactly as it is.\n\nComments:\n",
    );
    for d in drafts {
        let section = match &d.anchor {
            Anchor::Passage { section, .. } if !section.is_empty() => format!(" [{section}]"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "- {}{section} \"{}\": {}\n",
            d.anchor.place(),
            normal(d.anchor.quote())
                .chars()
                .take(400)
                .collect::<String>(),
            d.text
        ));
    }
    out
}

fn digests(u: &Unit, docs: &[String]) -> Vec<FileDigest> {
    docs.iter()
        .map(|p| FileDigest {
            path: p.clone(),
            sha256: std::fs::read(u.root.join(p))
                .map_or_else(|_| "absent".to_owned(), |b| digest(&b)),
        })
        .collect()
}

/// The sections changed between `before` and `after` of one document.
fn changed_sections(path: &str, before: &str, after: &str) -> Vec<String> {
    let (a, b) = (sections(before), sections(after));
    let mut seen = BTreeSet::new();
    a.iter()
        .chain(&b)
        .map(|(h, _)| h.clone())
        .filter(|h| seen.insert(h.clone()))
        .filter(|h| {
            let x = a.iter().find(|(n, _)| n == h).map(|(_, t)| t);
            let y = b.iter().find(|(n, _)| n == h).map(|(_, t)| t);
            x != y
        })
        .map(|h| {
            if h.is_empty() {
                format!("{path} (before the first heading)")
            } else {
                h
            }
        })
        .collect()
}

/// *Send N Comments*: one revision turn of the format's revising skill,
/// scoped to the sections the comments touch; test comments become one
/// *Ask Again* per requirement after it (unless the revision lapsed that
/// requirement's tests). The drafts are consumed atomically: a second send
/// of the same drafts is refused.
#[must_use]
pub fn send(
    path: &Path,
    feature: Option<&str>,
    ids: &[String],
    via: Via,
    state: Option<&Path>,
    options: &crate::RunOptions,
) -> CommandResult {
    let Some(st) = state.map(Path::to_path_buf).or_else(worktree::state_dir) else {
        return refuse(
            CommandStatus::ConfigError,
            "NO-STATE-DIR",
            "no state directory (set SPECCODER_STATE_DIR or HOME)",
        );
    };
    crate::with_driver("review", path, feature, |_, provider, key| {
        send_with(
            path,
            feature,
            ids,
            via,
            &st,
            Some(Arc::clone(provider)),
            Some(key),
            options,
        )
    })
}

/// [`send`] against `state`, with `provider` for the skill and the author.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn send_with(
    path: &Path,
    feature: Option<&str>,
    ids: &[String],
    via: Via,
    state: &Path,
    provider: Option<Arc<dyn ModelProvider>>,
    secret: Option<&str>,
    options: &crate::RunOptions,
) -> CommandResult {
    let u = match unit(path, feature, Some(state)) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let usage = |m: &str| refuse(CommandStatus::UsageError, "USAGE", m);
    // Consume the drafts and open the turn as one step.
    let (turn, sent, docs) = {
        let _lock = match Lock::take(&u.plane, &u.feature) {
            Ok(l) => l,
            Err(e) => return usage(&e),
        };
        let mut tray = load(&u.plane, &u.feature);
        let open: Vec<&CommentDraft> = tray
            .drafts
            .iter()
            .filter(|d| d.state != DraftState::Sent)
            .collect();
        let chosen: Vec<CommentDraft> = if ids.is_empty() {
            open.iter()
                .filter(|d| d.kept || current(&u, &d.anchor))
                .map(|d| (*d).clone())
                .collect()
        } else {
            let mut out = Vec::new();
            for id in ids {
                match open.iter().find(|d| d.id == *id) {
                    Some(d) if d.kept || current(&u, &d.anchor) => out.push((*d).clone()),
                    Some(_) => {
                        return usage(&format!(
                            "{id} is outdated: its words are no longer where it was anchored — keep it or delete it first"
                        ));
                    }
                    None => {
                        return usage(&format!(
                            "{id} was sent already (or deleted): look at the tray again"
                        ));
                    }
                }
            }
            out
        };
        if chosen.is_empty() {
            return usage("no comments to send");
        }
        let docs = documents(&u);
        let sections: Vec<String> = chosen
            .iter()
            .filter_map(|d| match &d.anchor {
                Anchor::Passage { section, .. } if !section.is_empty() => Some(section.clone()),
                Anchor::DiffLine {
                    path, line, side, ..
                } => {
                    let text = if side == "old" {
                        at_head(&u, path)
                    } else {
                        read(&u, path)
                    };
                    text.map(|t| section_at(&t, *line))
                        .filter(|s| !s.is_empty())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let id = format!("t{}", tray.turns.len() + 1);
        // The snapshot: every document's bytes, before the turn.
        let snap = snapshot_dir(&u.plane, &u.feature, &id);
        let _ = std::fs::remove_dir_all(&snap);
        for d in &docs {
            if let Ok(bytes) = std::fs::read(u.root.join(d)) {
                let f = snap.join(d);
                if let Some(parent) = f.parent()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    return refuse(CommandStatus::Error, "INTERNAL", &e.to_string());
                }
                if let Err(e) = std::fs::write(&f, bytes) {
                    return refuse(CommandStatus::Error, "INTERNAL", &e.to_string());
                }
            }
        }
        let skill = u.format.planning().revises.unwrap_or_default().to_owned();
        let turn = Turn {
            id: id.clone(),
            comments: chosen.iter().map(|d| d.id.clone()).collect(),
            sections,
            skill,
            before: digests(&u, &docs),
            after: Vec::new(),
            changed: Vec::new(),
            outside: Vec::new(),
            outcome: "pending".to_owned(),
            asked_again: Vec::new(),
        };
        for d in &mut tray.drafts {
            if turn.comments.contains(&d.id) {
                d.state = DraftState::Sent;
            }
        }
        tray.turns.push(turn.clone());
        if let Err(e) = save(&u.plane, &u.feature, &tray) {
            return refuse(CommandStatus::Error, "INTERNAL", &e);
        }
        (turn, chosen, docs)
    };
    let for_skill: Vec<CommentDraft> = sent
        .iter()
        .filter(|d| !matches!(d.anchor, Anchor::TestLine { .. }))
        .cloned()
        .collect();
    let mut notes: Vec<String> = Vec::new();
    let before_texts: Vec<(String, String)> = docs
        .iter()
        .map(|d| (d.clone(), read(&u, d).unwrap_or_default()))
        .collect();
    if !for_skill.is_empty() {
        if turn.skill.is_empty() {
            notes.push(
                "the format has no revising skill: only the test comments were sent".to_owned(),
            );
        } else if let Some(provider) = provider.clone() {
            let argument = revision_argument(&for_skill, &turn.sections);
            let ran = crate::hosting::run_in(
                path,
                Some(&u.feature),
                &turn.skill,
                &argument,
                state,
                provider,
                secret,
                options,
            );
            notes.push(ran.message().to_owned());
        } else {
            notes.push("no model: the revision was not run".to_owned());
        }
    }
    // After the turn: what changed, per section, and what no comment touched.
    let after_docs: Vec<String> = documents(&u)
        .into_iter()
        .chain(docs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let after = digests(&u, &after_docs);
    let mut changed: Vec<String> = Vec::new();
    for d in &after_docs {
        let before = before_texts
            .iter()
            .find(|(p, _)| p == d)
            .map(|(_, t)| t.as_str())
            .unwrap_or_default();
        let now = read(&u, d).unwrap_or_default();
        if before != now {
            changed.extend(changed_sections(d, before, &now));
        }
    }
    let outside: Vec<String> = changed
        .iter()
        .filter(|c| !turn.sections.iter().any(|s| s == *c))
        .cloned()
        .collect();
    // Test comments: one Ask Again per test, unless the revision lapsed
    // its requirement's tests.
    let mut asked_again = Vec::new();
    let mut by_test: Vec<(String, String, Vec<String>)> = Vec::new();
    for d in &sent {
        if let Anchor::TestLine {
            req,
            path: p,
            line,
            quote,
        } = &d.anchor
        {
            let line_note = format!("line {line} \"{}\": {}", normal(quote), d.text);
            match by_test.iter_mut().find(|(r, q, _)| r == req && q == p) {
                Some((_, _, n)) => n.push(line_note),
                None => by_test.push((req.clone(), p.clone(), vec![line_note])),
            }
        }
    }
    let requirements = match crate::inspect(path, Some(&u.feature)).data() {
        Some(CommandData::Inspect(s)) => u.format.oracle_requirements(s),
        _ => u.format.oracle_requirements(&u.snapshot),
    };
    for (req, test, lines) in &by_test {
        let lapsed = requirements.iter().find(|r| r.key == *req).is_none_or(|r| {
            crate::oracle::load(&u.plane, &u.feature, req)
                .ok()
                .flatten()
                .is_none_or(|(d, _)| d.requirement.sha256 != r.digest())
        });
        if lapsed {
            notes.push(format!(
                "{req}'s tests lapsed with the revision: not asked again"
            ));
            continue;
        }
        let text = lines.join("; ");
        let r = crate::oracle::tests_with(
            path,
            Some(&u.feature),
            &crate::oracle::Request {
                req,
                op: speccoder_core::oracle::OracleOp::Again,
                path: Some(test),
                scenario: None,
                reading: None,
                text: Some(&text),
            },
            via,
            state,
            provider.clone(),
            options,
        );
        notes.push(r.message().to_owned());
        if r.status() == CommandStatus::Ok && !asked_again.contains(req) {
            asked_again.push(req.clone());
        }
    }
    let _lock = Lock::take(&u.plane, &u.feature).ok();
    let mut tray = load(&u.plane, &u.feature);
    if let Some(t) = tray.turns.iter_mut().find(|t| t.id == turn.id) {
        t.after = after;
        t.changed.clone_from(&changed);
        t.outside.clone_from(&outside);
        t.asked_again.clone_from(&asked_again);
    }
    if let Err(e) = save(&u.plane, &u.feature, &tray) {
        return refuse(CommandStatus::Error, "INTERNAL", &e);
    }
    let message = format!(
        "sent {} comment(s) as revision {}: {} section(s) changed{}{}",
        sent.len(),
        turn.id,
        changed.len(),
        if outside.is_empty() {
            String::new()
        } else {
            format!(
                ", {} outside your comments ({}) — keep or undo it",
                outside.len(),
                outside.join("; ")
            )
        },
        if notes.is_empty() {
            String::new()
        } else {
            format!(" — {}", notes.join("; "))
        }
    );
    result(&u, &tray, &message)
}

/// *Undo Revision*, on the person's command only: the documents back to
/// the snapshot before the turn, byte for byte — refused when one changed
/// since the turn (naming it); the comments return to the tray.
#[must_use]
pub fn undo(
    path: &Path,
    feature: Option<&str>,
    turn: Option<&str>,
    state: Option<&Path>,
) -> CommandResult {
    let u = match unit(path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    let usage = |m: &str| refuse(CommandStatus::UsageError, "USAGE", m);
    let _lock = match Lock::take(&u.plane, &u.feature) {
        Ok(l) => l,
        Err(e) => return usage(&e),
    };
    let mut tray = load(&u.plane, &u.feature);
    let Some(last) = tray.turns.last().cloned() else {
        return usage("no revision to undo");
    };
    if let Some(t) = turn
        && t != last.id
    {
        return usage(&format!(
            "only the last revision ({}) can be undone, not {t}",
            last.id
        ));
    }
    if last.outcome == "undone" {
        return usage(&format!("revision {} is undone already", last.id));
    }
    // Every document as the turn left it, or the undo is refused.
    let paths: Vec<String> = last
        .before
        .iter()
        .chain(&last.after)
        .map(|f| f.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let now = digests(&u, &paths);
    for f in &now {
        let expected = last
            .after
            .iter()
            .find(|a| a.path == f.path)
            .or_else(|| last.before.iter().find(|b| b.path == f.path))
            .map_or("absent", |a| a.sha256.as_str());
        if f.sha256 != expected {
            return refuse(
                CommandStatus::NeedsHuman,
                "USAGE",
                &format!(
                    "{} changed since revision {}: undo would overwrite that — nothing was restored",
                    f.path, last.id
                ),
            );
        }
    }
    let snap = snapshot_dir(&u.plane, &u.feature, &last.id);
    // Every snapshot checked before anything is written.
    let mut restore: Vec<(String, Option<Vec<u8>>)> = Vec::new();
    for f in &now {
        match last.before.iter().find(|b| b.path == f.path) {
            Some(b) if b.sha256 != "absent" => {
                let Ok(bytes) = std::fs::read(snap.join(&f.path)) else {
                    return refuse(
                        CommandStatus::Error,
                        "INTERNAL",
                        &format!("the snapshot of {} is missing: nothing restored", f.path),
                    );
                };
                if digest(&bytes) != b.sha256 {
                    return refuse(
                        CommandStatus::NeedsHuman,
                        "INTERNAL",
                        &format!("the snapshot of {} was changed: nothing restored", f.path),
                    );
                }
                if f.sha256 != b.sha256 {
                    restore.push((f.path.clone(), Some(bytes)));
                }
            }
            _ => {
                if f.sha256 != "absent" {
                    restore.push((f.path.clone(), None));
                }
            }
        }
    }
    let mut written = Vec::new();
    for (p, bytes) in restore {
        let target = u.root.join(&p);
        let done = match bytes {
            Some(b) => std::fs::write(&target, b),
            None => std::fs::remove_file(&target),
        };
        if let Err(e) = done {
            return refuse(CommandStatus::Error, "INTERNAL", &format!("{p}: {e}"));
        }
        written.push(p);
    }
    if let Some(t) = tray.turns.last_mut() {
        t.outcome = "undone".to_owned();
    }
    for d in &mut tray.drafts {
        if last.comments.contains(&d.id) {
            d.state = DraftState::Draft;
        }
    }
    if let Err(e) = save(&u.plane, &u.feature, &tray) {
        return refuse(CommandStatus::Error, "INTERNAL", &e);
    }
    let message = if written.is_empty() {
        format!(
            "undid revision {}: nothing to restore; the comments are back in the tray",
            last.id
        )
    } else {
        format!(
            "undid revision {}: restored {}; the comments are back in the tray",
            last.id,
            written.join(", ")
        )
    };
    result(&u, &tray, &message)
}

/// *Send N Comments as a Nudge*: Code-view line comments on a task's diff
/// as one note for its next attempt (`path:line "quote" — text` per line).
/// A line of a frozen test is a review comment instead, never a nudge.
#[must_use]
pub fn code_nudge(
    path: &Path,
    feature: Option<&str>,
    task: &str,
    comments: &[CodeComment],
    state: Option<&Path>,
) -> CommandResult {
    let u = match unit(path, feature, state) {
        Ok(u) => u,
        Err(r) => return *r,
    };
    if comments.is_empty() || comments.iter().any(|c| c.text.trim().is_empty()) {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            "a nudge needs at least one comment, each with its words",
        );
    }
    let frozen: Vec<String> = u
        .format
        .oracle_requirements(&u.snapshot)
        .iter()
        .filter_map(|r| {
            crate::pre_frozen::load(&u.plane, &u.feature, &r.key)
                .ok()
                .flatten()
        })
        .flat_map(|(rec, _)| rec.visible.into_iter().map(|v| v.path))
        .collect();
    if let Some(c) = comments.iter().find(|c| frozen.contains(&c.path)) {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            &format!(
                "{} is a frozen test: comment on it in Review › Tests, not as a nudge to the implementer",
                c.path
            ),
        )
        .with_fix(vec![
            "speccoder review comments add --test <REQ>:<PATH>:<LINE> \"…\"".to_owned(),
        ]);
    }
    let note = comments
        .iter()
        .map(CodeComment::nudge_line)
        .collect::<Vec<_>>()
        .join("\n");
    let Some(st) = state.map(Path::to_path_buf).or_else(worktree::state_dir) else {
        return refuse(CommandStatus::ConfigError, "NO-STATE-DIR", "no state directory");
    };
    let r = crate::controls::nudge_with(path, Some(&u.feature), task, Some(&note), &st);
    if r.status() != CommandStatus::Ok {
        return r;
    }
    let message = format!(
        "nudge saved — the next attempt of {task} reads {} comment(s)",
        comments.len()
    );
    r.reworded(CommandStatus::Ok, &message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Spec\n\nIntro.\n\n## Requirements\n\n### Functional\n\n- FR-001 a\n\n## Edge Cases\n\n- none\n";

    #[test]
    fn a_line_knows_its_section() {
        assert_eq!(section_at(DOC, 3), "Spec");
        assert_eq!(section_at(DOC, 9), "Spec › Requirements › Functional");
        assert_eq!(section_at(DOC, 13), "Spec › Edge Cases");
        assert_eq!(section_at("text\n", 1), "");
    }

    #[test]
    fn only_the_changed_sections_are_named() {
        let after = DOC.replace("- none", "- an empty name");
        assert_eq!(
            changed_sections("spec.md", DOC, &after),
            ["Spec › Edge Cases"]
        );
        assert!(changed_sections("spec.md", DOC, DOC).is_empty());
        let added = format!("{DOC}\n## New\n\nx\n");
        assert_eq!(changed_sections("spec.md", DOC, &added), ["Spec › New"]);
    }

    #[test]
    fn a_quote_is_found_near_its_lines_only() {
        assert!(found(DOC, 9, 9, "FR-001 a"));
        assert!(found(DOC, 9, 9, "FR-001   a"));
        assert!(!found(DOC, 9, 9, "FR-002"));
        let far = format!("{}{DOC}", "x\n".repeat(40));
        assert!(!found(&far, 1, 1, "FR-001 a"));
    }

    #[test]
    fn the_argument_scopes_the_turn() {
        let d = CommentDraft {
            id: "c1".to_owned(),
            anchor: Anchor::Passage {
                path: "specs/001/spec.md".to_owned(),
                start_line: 9,
                end_line: 9,
                quote: "FR-001 a".to_owned(),
                quote_sha256: String::new(),
                section: "Spec › Requirements › Functional".to_owned(),
            },
            text: "say b".to_owned(),
            by: "Ada".to_owned(),
            at: 0,
            state: DraftState::Draft,
            kept: false,
        };
        let a = revision_argument(&[d], &["Spec › Requirements › Functional".to_owned()]);
        assert!(a.contains("Change only these sections: Spec › Requirements › Functional"));
        assert!(a.contains("Every other section stays exactly as it is"));
        assert!(a.contains("\"FR-001 a\": say b"));
    }
}
