//! `spec explore`: an idea explored read-only by the format's own skill
//! (or speccoder's fallback), ending in approaches the person chooses
//! from; the choice composes the brief that starts the spec. Sessions are
//! files in the project's state, never in the checkout.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentplane::model::ModelProvider;
use serde_json::Value;
use speccoder_core::actions::Via;
use speccoder_core::explore::{
    self as core, Choice, ExplorePayload, ExploreReport, ExploreSession, FALLBACK, Item, QaItem,
    RunRef,
};
use speccoder_core::{CommandData, CommandResult, CommandStatus};

use crate::RunOptions;

fn refuse(status: CommandStatus, code: &str, message: &str) -> CommandResult {
    CommandResult::new("explore", status, message).with_code(code)
}

fn now() -> u64 {
    crate::bug::confirm::now()
}

/// Whether `id` has the shape of a session id (`ex-` and hex).
#[must_use]
pub fn valid_id(id: &str) -> bool {
    id.strip_prefix("ex-")
        .is_some_and(|h| !h.is_empty() && h.len() <= 40 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("ex-{:012x}", (nanos / 1000) & 0xffff_ffff_ffff)
}

/// The project's root and its Explore folder.
fn dirs(path: &Path, state: &Path) -> Result<(PathBuf, PathBuf), Box<CommandResult>> {
    let Some(root) = crate::format::at(path).detect(path) else {
        return Err(Box::new(refuse(
            CommandStatus::NotAProject,
            "NO-PROJECT",
            "not a spec project here",
        )));
    };
    let dir = crate::worktree::project_state_dir(state, &root).join("explore");
    Ok((root, dir))
}

fn session_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn brief_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.brief.md"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn load(dir: &Path, id: &str) -> Option<ExploreSession> {
    if !valid_id(id) {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(session_path(dir, id)).ok()?).ok()
}

fn save(dir: &Path, s: &ExploreSession) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&session_path(dir, &s.id), &text)
}

fn sessions(dir: &Path) -> Vec<ExploreSession> {
    let mut all: Vec<ExploreSession> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?;
            load(dir, id)
        })
        .collect();
    all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
    all
}

fn payload(dir: &Path, s: ExploreSession) -> ExplorePayload {
    let (approaches, constraints, open_questions) = s.on_the_table();
    let brief = s
        .choices
        .last()
        .and_then(|_| std::fs::read_to_string(brief_path(dir, &s.id)).ok());
    ExplorePayload {
        choice: s.choices.last().cloned(),
        session: Some(s),
        approaches,
        constraints,
        open_questions,
        brief,
        sessions: None,
    }
}

fn result(status: CommandStatus, message: &str, p: ExplorePayload) -> CommandResult {
    CommandResult::new("explore", status, message).with_data(CommandData::Explore(Box::new(p)))
}

/// The project's sessions, newest first.
#[must_use]
pub fn list(path: &Path, state: Option<&Path>) -> CommandResult {
    let Some(state) = state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
    else {
        return refuse(CommandStatus::Error, "ERROR", "no state directory");
    };
    let (_, dir) = match dirs(path, &state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    let entries: Vec<_> = sessions(&dir).iter().map(ExploreSession::entry).collect();
    let message = match entries.len() {
        0 => "no explorations yet: `speccoder spec explore \"<idea>\"`".to_owned(),
        n => format!("{n} exploration(s)"),
    };
    result(
        CommandStatus::Ok,
        &message,
        ExplorePayload {
            session: None,
            approaches: Vec::new(),
            constraints: Vec::new(),
            open_questions: Vec::new(),
            choice: None,
            brief: None,
            sessions: Some(entries),
        },
    )
}

/// One session as it stands.
#[must_use]
pub fn session(path: &Path, state: Option<&Path>, id: &str) -> CommandResult {
    let Some(state) = state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
    else {
        return refuse(CommandStatus::Error, "ERROR", "no state directory");
    };
    let (_, dir) = match dirs(path, &state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    match load(&dir, id) {
        Some(s) => {
            let title = s.idea.clone();
            result(CommandStatus::Ok, &title, payload(&dir, s))
        }
        None => refuse(
            CommandStatus::NotFound,
            "NOT-FOUND",
            &format!("no exploration {id} — see `speccoder spec explore list`"),
        ),
    }
}

/// The skill that explores here, and why: `--skill`, else the format's
/// own if installed and hosted, else the fallback.
fn select(root: &Path, flag: Option<&str>) -> Result<(String, &'static str), String> {
    if let Some(name) = flag {
        return Ok((name.to_owned(), "flag"));
    }
    let planning = crate::format::at(root).planning();
    let installed = crate::skills::discover_with(root, planning);
    let found = planning.explores.and_then(|short| {
        let name = format!("{}{short}", planning.prefix);
        installed
            .into_iter()
            .find(|k| k.name == name && k.invocable && k.refused.is_none())
    });
    Ok(found.map_or_else(|| (FALLBACK.to_owned(), "fallback"), |k| (k.name, "format")))
}

/// What a run asked the person and what they answered, from its calls.
fn answers(calls: &[Value]) -> Vec<QaItem> {
    calls
        .iter()
        .filter(|c| c["tool"] == crate::substrate::attempt::ASK_PERSON)
        .map(|c| QaItem {
            question: c["arguments"]["question"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            answer: c["answer"].as_str().map(str::to_owned),
            by: c["operator"]["actor"].as_str().map(str::to_owned),
        })
        .collect()
}

/// `spec explore "<idea>"` (or `--resume ID ["words"]`) with the model
/// from `speccoder.toml`.
#[must_use]
pub fn explore(
    path: &Path,
    idea: &str,
    skill: Option<&str>,
    resume: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    crate::with_driver("explore", path, None, |state, provider, key| {
        explore_in(
            path,
            idea,
            skill,
            resume,
            state,
            Arc::clone(provider),
            Some(key),
            options,
        )
    })
}

/// [`explore`] with `provider` answering.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn explore_in(
    path: &Path,
    idea: &str,
    skill: Option<&str>,
    resume: Option<&str>,
    state: &Path,
    provider: Arc<dyn ModelProvider>,
    secret: Option<&str>,
    options: &RunOptions,
) -> CommandResult {
    let (root, dir) = match dirs(path, state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    let words = idea.trim();
    let (mut session, input, kind) = match resume {
        Some(id) => {
            let Some(s) = load(&dir, id) else {
                return refuse(
                    CommandStatus::NotFound,
                    "NOT-FOUND",
                    &format!("no exploration {id} — see `speccoder spec explore list`"),
                );
            };
            if let Err(why) = core::next_state(&s.state, core::Event::Resume) {
                return refuse(CommandStatus::UsageError, "EXPLORE-STARTED", why);
            }
            let mut input = format!("{}\n\n{}", s.idea, core::carried(&s));
            if !words.is_empty() {
                input.push_str(&format!("# More from the person\n\n{words}\n"));
            }
            (s, input, "resume")
        }
        None => {
            if words.is_empty() {
                return refuse(
                    CommandStatus::UsageError,
                    "EXPLORE-EMPTY",
                    "say what to explore: `speccoder spec explore \"<idea>\"`",
                );
            }
            let (name, because) = match select(&root, skill) {
                Ok(s) => s,
                Err(e) => return refuse(CommandStatus::NotFound, "NOT-FOUND", &e),
            };
            let at = now();
            let s = ExploreSession {
                version: 1,
                id: new_id(),
                idea: words.to_owned(),
                skill: name,
                selected_because: because.to_owned(),
                state: "open".to_owned(),
                created_at: at,
                updated_at: at,
                runs: Vec::new(),
                reports: Vec::new(),
                qa: Vec::new(),
                choices: Vec::new(),
                started: None,
            };
            (s, words.to_owned(), "explore")
        }
    };
    let (ran, report) = crate::hosting::run_exploring(
        path,
        &session.skill,
        &input,
        state,
        provider,
        secret,
        options,
    );
    let Some(CommandData::Skill(run)) = ran.data() else {
        return ran;
    };
    // Read-only, proven: anything written outside the format's own
    // exploration paths fails the session.
    let planning = crate::format::at(&root).planning();
    let wrote: Vec<String> = run
        .changes
        .iter()
        .filter(|(p, _)| !(planning.explore_paths)(p))
        .map(|(p, _)| p.clone())
        .collect();
    let report: Option<ExploreReport> = serde_json::from_value(report).ok();
    let checked = report.as_ref().map(|r| {
        if session.skill == FALLBACK {
            core::validate_fallback(r)
        } else if r.approaches.is_empty() {
            Err("no approach to choose from".to_owned())
        } else {
            Ok(())
        }
    });
    let ok = wrote.is_empty() && matches!(checked, Some(Ok(())));
    session.runs.push(RunRef {
        run_id: run.run_id.clone(),
        kind: kind.to_owned(),
        words: (kind == "resume" && !words.is_empty()).then(|| words.to_owned()),
        status: if ok { "completed" } else { "failed" }.to_owned(),
    });
    session.qa.extend(answers(&run.tool_calls));
    if ok && let Some(r) = report {
        session.reports.push(r);
        if kind == "resume" {
            session.state = "open".to_owned();
        }
    }
    session.updated_at = now();
    if let Err(e) = save(&dir, &session) {
        return refuse(CommandStatus::Error, "ERROR", &e);
    }
    let id = session.id.clone();
    let skill_name = session.skill.clone();
    let p = payload(&dir, session);
    if !wrote.is_empty() {
        return result(
            CommandStatus::RunFailed,
            &format!(
                "{skill_name} wrote while exploring ({}) — exploring writes nothing; the session keeps its earlier approaches",
                wrote.join(", ")
            ),
            p,
        )
        .with_code("EXPLORE-WROTE");
    }
    match checked {
        // Approaches to choose from: the next step is the person's, nothing
        // is blocked.
        Some(Ok(())) => result(
            CommandStatus::Ok,
            &format!(
                "{} approaches from {skill_name} — choose one: `speccoder spec explore choose N {id}`",
                p.approaches.len()
            ),
            p,
        ),
        Some(Err(why)) => result(
            CommandStatus::RunFailed,
            &format!("{skill_name} ended without approaches to choose from: {why}"),
            p,
        )
        .with_code("EXPLORE-INCOMPLETE"),
        None => result(
            ran.status(),
            &format!("{skill_name} ended without its report: {}", ran.message()),
            p,
        )
        .with_code("EXPLORE-INCOMPLETE"),
    }
}

/// The person's choice in `session`: approach `n` (1-based) with the
/// constraints and open questions as they keep and add them. Composes and
/// stores the brief; no model call.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn choose(
    path: &Path,
    state: Option<&Path>,
    id: &str,
    n: usize,
    constraints: &[String],
    questions: &[String],
    via: Via,
    basis: &str,
) -> CommandResult {
    let Some(state) = state
        .map(Path::to_path_buf)
        .or_else(crate::worktree::state_dir)
    else {
        return refuse(CommandStatus::Error, "ERROR", "no state directory");
    };
    let (root, dir) = match dirs(path, &state) {
        Ok(d) => d,
        Err(e) => return *e,
    };
    let Some(mut s) = load(&dir, id) else {
        return refuse(
            CommandStatus::NotFound,
            "NOT-FOUND",
            &format!("no exploration {id} — see `speccoder spec explore list`"),
        );
    };
    let next = match core::next_state(&s.state, core::Event::Choose) {
        Ok(n) => n,
        Err(why) => return refuse(CommandStatus::UsageError, "EXPLORE-STARTED", why),
    };
    let (approaches, model_constraints, model_questions) = s.on_the_table();
    let Some(approach) = n.checked_sub(1).and_then(|i| approaches.get(i)) else {
        return refuse(
            CommandStatus::UsageError,
            "USAGE",
            &format!(
                "choose 1–{} (approach {n} does not exist)",
                approaches.len()
            ),
        );
    };
    // Each kept item keeps its source; what the person added is theirs.
    let source = |text: &String, from: &[Item]| Item {
        text: text.trim().to_owned(),
        source: from
            .iter()
            .find(|i| i.text.trim() == text.trim())
            .map_or_else(|| "person".to_owned(), |i| i.source.clone()),
    };
    let kept_c: Vec<Item> = constraints
        .iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| source(t, &model_constraints))
        .collect();
    let kept_q: Vec<Item> = questions
        .iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| source(t, &model_questions))
        .collect();
    let brief = match core::compose(approach, &kept_c, &kept_q, &s.id, |b| {
        crate::integrity::sha256_hex(b)
    }) {
        Ok(b) => b,
        Err(e) => return refuse(CommandStatus::UsageError, "EXPLORE-BRIEF-LARGE", &e),
    };
    if let Err(e) = write_atomic(&brief_path(&dir, &s.id), brief.text.as_bytes()) {
        return refuse(CommandStatus::Error, "ERROR", &e);
    }
    s.choices.push(Choice {
        approach: n,
        constraints: kept_c,
        open_questions: kept_q,
        by: crate::approvals::who_in(&root),
        via,
        basis: basis.to_owned(),
        at: now(),
        digest: brief.digest.clone(),
        bytes: brief.bytes,
    });
    next.clone_into(&mut s.state);
    s.updated_at = now();
    if let Err(e) = save(&dir, &s) {
        return refuse(CommandStatus::Error, "ERROR", &e);
    }
    let id = s.id.clone();
    result(
        CommandStatus::Ok,
        &format!(
            "brief {} ready — start the spec from it: `speccoder spec explore start {id}`",
            brief.digest.get(..12).unwrap_or(&brief.digest)
        ),
        payload(&dir, s),
    )
}

/// Before a spec starts from `id`'s brief: the stored brief must still be
/// the one chosen (`digest`), the description it verbatim, the session
/// not started. The session, and the brief.
///
/// # Errors
///
/// Why it cannot start, with its code.
pub fn verify_start(
    root: &Path,
    state: &Path,
    id: &str,
    digest: &str,
    description: &str,
) -> Result<ExploreSession, Box<CommandResult>> {
    let (_, dir) = dirs(root, state)?;
    let Some(s) = load(&dir, id) else {
        return Err(Box::new(refuse(
            CommandStatus::NotFound,
            "NOT-FOUND",
            &format!("no exploration {id} — see `speccoder spec explore list`"),
        )));
    };
    if let Some(started) = &s.started {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "EXPLORE-STARTED",
            &format!(
                "{id} already started {} — one brief, one spec; explore again for another",
                started.unit
            ),
        )));
    }
    let Some(chosen) = s.choices.last() else {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "EXPLORE-NOT-CHOSEN",
            &format!("choose an approach first: `speccoder spec explore choose N {id}`"),
        )));
    };
    let stored = std::fs::read_to_string(brief_path(&dir, id)).unwrap_or_default();
    let now_digest = core::body_digest(&stored, crate::integrity::sha256_hex);
    if now_digest != chosen.digest || chosen.digest != digest {
        return Err(Box::new(refuse(
            CommandStatus::Error,
            "EXPLORE-DIGEST",
            &format!("{id}'s brief changed since it was chosen — choose again to make a new one"),
        )));
    }
    if description != stored {
        return Err(Box::new(refuse(
            CommandStatus::UsageError,
            "EXPLORE-EDITED",
            "a brief starts a spec verbatim or not at all — edit it by choosing again",
        )));
    }
    Ok(s)
}

/// Once plan through made the unit from `session`'s brief: its origin,
/// written once, and the session started.
pub fn explored(
    root: &Path,
    state: &Path,
    made: &CommandResult,
    session: &str,
    digest: &str,
    via: Via,
) {
    let Some(CommandData::PlanningChain(r)) = made.data() else {
        return;
    };
    let Some(unit) = r.unit.as_deref() else {
        return;
    };
    let plane = crate::worktree::project_state_dir(state, root);
    let _ = crate::ladder::record_origin(
        &plane,
        unit,
        vec![speccoder_core::ladder::Origin::ExploredFrom {
            session: session.to_owned(),
            digest: digest.to_owned(),
            by: crate::approvals::who_in(root),
            via,
            at: now(),
        }],
    );
    started(root, state, session, unit);
}

/// Once the spec exists: the session is started, with its unit.
pub fn started(root: &Path, state: &Path, id: &str, unit: &str) {
    let Ok((_, dir)) = dirs(root, state) else {
        return;
    };
    if let Some(mut s) = load(&dir, id) {
        s.state = "started".to_owned();
        s.started = Some(core::Started {
            unit: unit.to_owned(),
            at: now(),
        });
        s.updated_at = now();
        let _ = save(&dir, &s);
    }
}

/// The newest session in `state` ("open" to choose in, "chosen" to start).
#[must_use]
pub fn newest(path: &Path, state: &Path, in_state: &str) -> Option<ExploreSession> {
    let (_, dir) = dirs(path, state).ok()?;
    sessions(&dir).into_iter().find(|s| s.state == in_state)
}
