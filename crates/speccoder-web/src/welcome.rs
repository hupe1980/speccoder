//! A host started outside a project: the welcome (New Project, Open,
//! Clone, Try the Sample, recent projects), and the read-only views the
//! New Feature sheet and the composer ask for. Until it is bound, the host
//! answers every project route with `409 NO-PROJECT-YET`; it binds once —
//! to the project a job made, cloned or opened, or a folder the person
//! opened — and then serves only that project.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::actions::{ActionKind, Runnable};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::service::{new_project, planning, welcome};

use crate::{Root, Shared, envelope, off};

/// Whether a host started at `path` serves it right away.
pub(crate) fn is_project(path: &Path) -> bool {
    welcome::is_project(path)
}

/// Routes a host not bound to a project answers.
const PROJECTLESS: &[&str] = &[
    "/api/welcome",
    "/api/open",
    "/api/recent/forget",
    "/api/folders",
    "/api/new/plan",
    "/api/github/repos",
    "/api/run",
    "/api/job",
    "/api/events",
    "/api/answer",
    "/api/stop",
];

/// `409 NO-PROJECT-YET`.
pub(crate) fn no_project_yet(why: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "code": "NO-PROJECT-YET", "error": why })),
    )
        .into_response()
}

/// Before the host is bound, project routes wait for a project.
pub(crate) async fn projectless(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if s.root.bound().is_none() && path.starts_with("/api/") && !PROJECTLESS.contains(&path) {
        return no_project_yet("no project yet: create, open, clone or try one");
    }
    next.run(req).await
}

/// The folder a job runs in. Bound: the project. Not yet: only the kinds
/// that make or open a project run — in the start folder, or (Undo of a
/// recent project) in the folder speccoder created.
pub(crate) fn job_root(
    root: &Root,
    run: &Runnable,
    given: Option<&str>,
) -> Result<PathBuf, String> {
    // A bound host serves one project: it undoes only that one.
    if let (Some(bound), ActionKind::UndoNewProject, Some(g)) = (root.bound(), run.kind(), given)
        && std::fs::canonicalize(g).ok().as_deref() != Some(bound.as_path())
    {
        return Err(format!(
            "this host serves {}: undo another project in its own host",
            bound.display()
        ));
    }
    // Undo names the folder speccoder created.
    if run.kind() == ActionKind::UndoNewProject
        && let Some(g) = given
    {
        let state = speccoder_runtime::service::worktree::state_dir()
            .ok_or_else(|| "no state directory".to_owned())?;
        let p = PathBuf::from(g);
        return new_project::creation(&state, &p)
            .map(|_| p)
            .ok_or_else(|| "Undo is offered only for a project speccoder created".to_owned());
    }
    if let Some(p) = root.bound() {
        return Ok(p);
    }
    match run.kind() {
        ActionKind::NewProject | ActionKind::CloneRepository | ActionKind::OpenSample => {
            Ok(root.start())
        }
        ActionKind::UndoNewProject => {
            let state = speccoder_runtime::service::worktree::state_dir()
                .ok_or_else(|| "no state directory".to_owned())?;
            let p = given
                .map(PathBuf::from)
                .filter(|p| new_project::creation(&state, p).is_some())
                .ok_or_else(|| "Undo is offered only for a project speccoder created".to_owned())?;
            Ok(p)
        }
        _ => Err("no project yet: create, open, clone or try one first".to_owned()),
    }
}

/// After a job: the project it made, cloned or opened, bound — when the
/// host has none yet and the job succeeded.
pub(crate) fn bind_after(root: &Root, run: &Runnable, result: &CommandResult) -> Option<PathBuf> {
    if root.bound().is_some() || result.status() != CommandStatus::Ok {
        return None;
    }
    let project = match (run, result.data()) {
        (Runnable::NewProject { .. }, Some(CommandData::NewProject(r))) => {
            PathBuf::from(&r.plan.target)
        }
        (Runnable::CloneRepository { target, .. }, _) => expand(target),
        (Runnable::OpenSample { .. }, Some(CommandData::Sample(r))) => PathBuf::from(&r.path),
        _ => return None,
    };
    let project = std::fs::canonicalize(&project).ok()?;
    root.bind(&project).ok()?;
    // The sample is speccoder's own copy, not one of the person's projects.
    if run.kind() == ActionKind::OpenSample {
        return Some(project);
    }
    if let Some(state) = speccoder_runtime::service::worktree::state_dir() {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let created = run.kind() == ActionKind::NewProject;
        welcome::remember(&state, &project, &name, created);
    }
    Some(project)
}

/// The welcome: where the host started, the default parent for a new
/// folder, the recent projects, whether this build has the sample.
pub(crate) async fn view(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.clone();
    Json(
        off(move || {
            let state = speccoder_runtime::service::worktree::state_dir();
            let parent = state
                .as_deref()
                .map_or_else(|| root.start(), welcome::default_parent);
            json!({
                "bound": root.bound().map(|p| p.display().to_string()),
                "start": root.start().display().to_string(),
                "start_shown": welcome::tilde(&root.start()),
                "parent": parent.display().to_string(),
                "parent_shown": welcome::tilde(&parent),
                "recents": state.as_deref().map(welcome::views).unwrap_or_default(),
                "sample": speccoder_runtime::service::sample::available(),
                "sample_copy": state.as_deref().and_then(speccoder_runtime::service::sample::copy).is_some(),
                "version": env!("CARGO_PKG_VERSION"),
            })
        })
        .await,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct PathBody {
    path: String,
}

fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => PathBuf::from(h).join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

/// Opens a folder in a host not bound yet: an existing, readable directory.
pub(crate) async fn open(State(s): State<Shared>, Json(b): Json<PathBody>) -> Response {
    if s.root.bound().is_some() {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "this host serves a project already: open another with `speccoder ui -C <path>`" })),
        )
            .into_response();
    }
    let p = expand(b.path.trim());
    let Ok(dir) = std::fs::canonicalize(&p) else {
        return Json(json!({ "error": "Not a folder" })).into_response();
    };
    if !dir.is_dir() {
        return Json(json!({ "error": "Not a folder" })).into_response();
    }
    if std::fs::read_dir(&dir).is_err() {
        return Json(json!({ "error": "Not readable" })).into_response();
    }
    if let Err(e) = s.root.bind(&dir) {
        return (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response();
    }
    if let Some(state) = speccoder_runtime::service::worktree::state_dir() {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        welcome::remember(&state, &dir, &name, false);
    }
    Json(json!({ "ok": true, "root": dir.display().to_string() })).into_response()
}

/// Forgets a recent project (nothing of it is read or removed).
pub(crate) async fn forget(Json(b): Json<PathBody>) -> Json<Value> {
    if let Some(state) = speccoder_runtime::service::worktree::state_dir() {
        welcome::forget(&state, Path::new(&b.path));
    }
    Json(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct UnderQuery {
    #[serde(default)]
    under: String,
}

/// Folder names under the person's home, for the Open field.
pub(crate) async fn folders(Query(q): Query<UnderQuery>) -> Json<Value> {
    Json(
        off(move || match welcome::folders(&q.under) {
            Ok(f) => json!({ "folders": f }),
            Err(e) => json!({ "folders": [], "error": e }),
        })
        .await,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct PlanQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    idea: String,
    stack: Option<String>,
    format: Option<String>,
    /// One extension per line.
    #[serde(default)]
    extensions: String,
    #[serde(default)]
    no_preset: bool,
}

/// The New Project sheet's plan (read-only; `speccoder new --dry-run`'s).
pub(crate) async fn plan(Query(q): Query<PlanQuery>) -> Json<Value> {
    Json(
        off(move || {
            let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
                return json!({ "error": "no state directory" });
            };
            let target = if q.path.trim().is_empty() {
                welcome::default_parent(&state).join(new_project::slug(&q.idea))
            } else {
                expand(q.path.trim())
            };
            let choices = new_project::NewChoices {
                idea: q.idea.clone(),
                stack: q.stack.as_deref().and_then(new_project::Stack::parse),
                format: q.format.clone().filter(|f| !f.is_empty()),
                extensions: q
                    .extensions
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect(),
                no_preset: q.no_preset,
            };
            let plan = new_project::plan(&target, &choices, &state);
            json!({
                "plan": plan,
                "target_shown": welcome::tilde(Path::new(&plan.target)),
                "choices": choices,
            })
        })
        .await,
    )
}

/// The idea of a project speccoder just created, offered once while it has
/// no unit: the New Feature sheet opens with it.
pub(crate) async fn first_feature(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
                return json!({ "idea": null });
            };
            let made = new_project::creation(&state, &root);
            let idea = new_project::first_feature(&state, &root);
            json!({
                "idea": idea,
                "name": made.as_ref().filter(|_| idea.is_some()).map(|c| c.name.clone()),
                "commit": made.as_ref().filter(|_| idea.is_some()).and_then(|c| c.commit.clone()),
            })
        })
        .await,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct TextQuery {
    #[serde(default)]
    text: String,
}

/// Requirements close to a description (the New Feature sheet's hint).
pub(crate) async fn similar(State(s): State<Shared>, Query(q): Query<TextQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || planning::similar_command(&root, &q.text)).await;
    envelope(&r)
}

/// The rung a description suggests (the composer).
pub(crate) async fn suggest(State(s): State<Shared>, Query(q): Query<TextQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || planning::suggest(&root, &q.text)).await;
    envelope(&r)
}

/// The person's GitHub repositories (Clone), read-only.
pub(crate) async fn repositories() -> Json<Value> {
    Json(
        off(move || {
            use speccoder_runtime::service::github;
            match github::repositories(&github::Ctx::from_env()) {
                Ok((login, repos)) => json!({ "login": login, "repos": repos }),
                Err(e) => json!({ "error": e, "signed_in": !e.contains("not signed in"), "fix": "speccoder auth login" }),
            }
        })
        .await,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct SkillsQuery {
    /// Installed skill names, comma-separated.
    #[serde(default)]
    skills: String,
}

/// What running these skills cost before here (the cost line), and the model.
pub(crate) async fn skill_cost(
    State(s): State<Shared>,
    Query(q): Query<SkillsQuery>,
) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let skills: Vec<String> = q
                .skills
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let (cost, model) = speccoder_runtime::service::controls::skill_cost(&root, &skills);
            json!({ "cost": cost, "model": model })
        })
        .await,
    )
}

/// The sample, when the host serves its copy: how it was recorded and
/// whether each run reproduced (`sample.json` beside the copy).
pub(crate) fn sample_of(root: &Path) -> Option<Value> {
    let state = speccoder_runtime::service::worktree::state_dir()?;
    let copy = speccoder_runtime::service::sample::copy(&state)?;
    let root = std::fs::canonicalize(root).ok()?;
    if std::fs::canonicalize(&copy).ok()? != root {
        return None;
    }
    let home = speccoder_runtime::service::sample::home(&state);
    let text = std::fs::read_to_string(home.join("sample.json")).ok()?;
    let mut v: Value = serde_json::from_str(&text).ok()?;
    // Whether *Run the checks yourself* can run here, and why not.
    v["checks_missing"] = json!(speccoder_runtime::service::sample::missing_tool(&root));
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_host_undoes_only_its_own_project() {
        let mine = std::env::temp_dir();
        let root = Root::new(&mine, true);
        let run = Runnable::UndoNewProject {
            commit: String::new(),
        };
        let other = mine.join("not-this-hosts-project");
        let e = job_root(&root, &run, Some(&other.display().to_string()))
            .err()
            .unwrap_or_default();
        assert!(e.contains("this host serves"), "{e}");
    }
}
