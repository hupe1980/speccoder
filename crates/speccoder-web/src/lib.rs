//! speccoder's web interface: a host on 127.0.0.1 that
//! serves the interface — embedded in the binary — and an API over
//! speccoder's own payloads. The page, a browser tab or the desktop window,
//! holds no logic of its own: what it shows and offers comes from the
//! runtime, as for the CLI.

pub mod jobs;
pub mod security;
mod welcome;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::actions::Runnable;
use speccoder_core::{CommandData, CommandResult};

mod bundle {
    include!(concat!(env!("OUT_DIR"), "/bundle.rs"));
}

/// The project a host serves: bound at most once (one host per
/// project). A host started outside a project serves the welcome until
/// the person creates, opens, clones or tries one.
#[derive(Debug, Clone)]
pub struct Root(Arc<RootCell>);

#[derive(Debug)]
struct RootCell {
    start: PathBuf,
    bound: std::sync::OnceLock<PathBuf>,
}

impl Root {
    /// A host's root: `start`, bound now when it is a project.
    #[must_use]
    pub fn new(start: &Path, bound: bool) -> Self {
        let cell = RootCell {
            start: start.to_path_buf(),
            bound: std::sync::OnceLock::new(),
        };
        if bound {
            let _ = cell.bound.set(start.to_path_buf());
        }
        Self(Arc::new(cell))
    }

    /// The project (the start folder before a bind).
    #[must_use]
    pub fn get(&self) -> PathBuf {
        self.0
            .bound
            .get()
            .cloned()
            .unwrap_or_else(|| self.0.start.clone())
    }

    /// The project, once bound.
    #[must_use]
    pub fn bound(&self) -> Option<PathBuf> {
        self.0.bound.get().cloned()
    }

    /// Where the host was started.
    #[must_use]
    pub fn start(&self) -> PathBuf {
        self.0.start.clone()
    }

    /// Binds the host to `project`, once.
    ///
    /// # Errors
    ///
    /// The host serves a project already.
    pub fn bind(&self, project: &Path) -> Result<(), String> {
        self.0.bound.set(project.to_path_buf()).map_err(|_| {
            format!(
                "this host serves {} already: open another project with `speccoder ui -C {}`",
                self.get().display(),
                project.display()
            )
        })
    }
}

/// The host's state.
#[derive(Debug, Clone)]
pub struct Shared {
    /// The project, bound at most once.
    pub root: Root,
    /// The per-start token.
    pub token: String,
    /// The port it listens on.
    pub port: u16,
    /// Jobs and events.
    pub hub: Arc<jobs::Hub>,
    /// Turns true when the host stops: open event streams end with it.
    pub closing: tokio::sync::watch::Receiver<bool>,
    /// A GitHub sign-in waiting for the person to enter its code:
    /// the host, the app and the pending device code — never sent to the page.
    pub device: Arc<
        std::sync::Mutex<
            Option<(
                String,
                String,
                speccoder_runtime::service::github::auth::Pending,
            )>,
        >,
    >,
}

/// How long open requests get to finish after ctrl-c.
const GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Runs `f` on a thread of its own: the runtime's calls block and start
/// runtimes of their own, which the host's workers must not.
async fn off<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.await {
        Ok(v) => v,
        // The work panicked: that request fails (its connection drops), the
        // host and a running job go on. Never abort the process for one.
        Err(_) => std::panic::resume_unwind(Box::new("a request's work stopped unexpectedly")),
    }
}

/// Refuses what does not come from this host's page or a token holder.
async fn guard(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    if !security::host_ok(s.port, headers) {
        return (
            StatusCode::MISDIRECTED_REQUEST,
            "this host answers on the loopback address only",
        )
            .into_response();
    }
    if req.uri().path().starts_with("/api/") {
        if !security::authorised(&s.token, headers) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorised" })),
            )
                .into_response();
        }
        if !security::origin_ok(s.port, req.method(), headers) {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "cross-origin request refused" })),
            )
                .into_response();
        }
    }
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    res
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// The interface: the embedded bundle (`SPECCODER_UI` names a `dist/` to
/// serve instead, for working on the interface).
async fn asset(uri: axum::http::Uri) -> Response {
    let want = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        p => p,
    };
    let csp = [(
        header::CONTENT_SECURITY_POLICY,
        security::CONTENT_SECURITY_POLICY,
    )];
    if let Some(dir) = std::env::var_os("SPECCODER_UI")
        && !want.contains("..")
        && let Ok(bytes) = std::fs::read(Path::new(&dir).join(want))
    {
        return (csp, [(header::CONTENT_TYPE, content_type(want))], bytes).into_response();
    }
    if bundle::BUNDLE.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            "<!doctype html><meta charset=utf-8><title>speccoder</title><p style=\"font:16px system-ui;max-width:60ch;margin:4rem auto\">This binary was built without its interface (<code>npm run build</code> in <code>crates/speccoder-web/ui</code>). The API and the command line work.",
        )
            .into_response();
    }
    // Vite emits flat names: a path matches an embedded file or nothing.
    let name = want.trim_start_matches("assets/");
    match bundle::BUNDLE
        .iter()
        .find(|(p, _)| *p == want || *p == name)
    {
        Some((p, bytes)) => {
            (csp, [(header::CONTENT_TYPE, content_type(p))], *bytes).into_response()
        }
        // The interface routes in the page: an unknown path is the page.
        None => match bundle::BUNDLE.iter().find(|(p, _)| *p == "index.html") {
            Some((_, bytes)) => (
                csp,
                [(header::CONTENT_TYPE, content_type("index.html"))],
                *bytes,
            )
                .into_response(),
            None => (StatusCode::NOT_FOUND, "no such file").into_response(),
        },
    }
}

#[derive(Debug, Deserialize)]
struct FeatureQuery {
    feature: Option<String>,
    task: Option<String>,
    commit: Option<String>,
    /// An attempt ref (`refs/speccoder/attempts/…`): a failed or held
    /// attempt's code, kept for a person to read.
    #[serde(rename = "ref")]
    reference: Option<String>,
}

/// A commit id, or a ref in speccoder's attempt namespace — nothing else
/// reaches `git show` (no options, no other refs, no `..`).
fn revision(q: &FeatureQuery) -> Option<String> {
    if let Some(c) = q.commit.as_deref() {
        return (c.len() >= 7 && c.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| c.to_owned());
    }
    let r = q.reference.as_deref()?;
    let rest = r.strip_prefix("refs/speccoder/attempts/")?;
    let ok = !rest.is_empty()
        && !r.contains("..")
        && rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.'));
    ok.then(|| r.to_owned())
}

/// A project-relative path the file view may show: nothing absolute,
/// nothing up. Where it leads on disk is checked where it is read — a link
/// is never followed, the file must be inside the worktree.
fn file_path_ok(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && !p.is_absolute()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn envelope(r: &CommandResult) -> Json<Value> {
    Json(serde_json::to_value(r).unwrap_or(Value::Null))
}

async fn project(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let words = speccoder_runtime::service::format_words(&root);
            let features = speccoder_runtime::service::current::list(&root, None);
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            json!({
                "name": name,
                "root": root.display().to_string(),
                "format": words.name,
                "unit": words.unit,
                "features": features.data().and_then(|d| match d {
                    CommandData::Features(f) => serde_json::to_value(f).ok(),
                    _ => None,
                }),
                "version": env!("CARGO_PKG_VERSION"),
                "planning": speccoder_runtime::service::planning_words(&root),
                "sample": welcome::sample_of(&root),
            })
        })
        .await,
    )
}

async fn report(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        speccoder_runtime::service::next::attach(
            speccoder_runtime::service::status::report(&root, q.feature.as_deref()),
            q.feature.as_deref(),
        )
    })
    .await;
    envelope(&r)
}

async fn review(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r =
        off(move || speccoder_runtime::service::review::report(&root, q.feature.as_deref())).await;
    envelope(&r)
}

/// The comment tray and the last revision turn.
async fn review_comments(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        speccoder_runtime::service::review::comments(&root, q.feature.as_deref(), None)
    })
    .await;
    envelope(&r)
}

#[derive(Debug, Deserialize)]
struct RunQuery {
    run: String,
}

/// The model's account of a run, turn by turn: its reasoning and its words
/// (a claim).
async fn account(State(s): State<Shared>, Query(q): Query<RunQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(
            move || match speccoder_runtime::service::evidence::account_of(&root, &q.run) {
                Ok(turns) => json!({ "turns": turns }),
                Err(e) => json!({ "error": e }),
            },
        )
        .await,
    )
}

async fn inbox(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || speccoder_runtime::service::inbox::inbox(&root, None)).await;
    envelope(&r)
}

/// A bug by `<unit>/<task>` (`task`): its triad, reproductions and stage.
async fn bug(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let Some(task) = q.task else {
        return Json(json!({ "error": "a bug is needed: task=<unit>/<task>" }));
    };
    let r = off(move || speccoder_runtime::service::bug::show(&root, &task)).await;
    envelope(&r)
}

async fn actions(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            json!(speccoder_runtime::service::actions::available(
                &root,
                q.feature.as_deref(),
                q.task.as_deref()
            ))
        })
        .await,
    )
}

async fn approvals(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r =
        off(move || speccoder_runtime::service::approvals::list(&root, q.feature.as_deref())).await;
    envelope(&r)
}

async fn evidence(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let report = speccoder_runtime::service::status::report(&root, q.feature.as_deref());
            let Some(CommandData::Report(r)) = report.data() else {
                return json!({ "error": report.message() });
            };
            let Some(t) = r.tasks.iter().find(|t| Some(&t.id) == q.task.as_ref()) else {
                return json!({ "error": "no such task" });
            };
            let statements = t
                .commit
                .as_deref()
                .map(|c| speccoder_runtime::service::evidence::statements(&root, c))
                .unwrap_or_default();
            let (mark, words) = speccoder_runtime::service::evidence::banner(&t.state);
            json!({
                "task": t,
                "banner": { "mark": mark, "words": words },
                "rows": speccoder_runtime::service::evidence::rows(t, &statements, None),
            })
        })
        .await,
    )
}

/// The task's evidence tour (payload `evidence_tour`, the same as
/// `speccoder attest tour --json`), read as recorded: the app shows what
/// the signed statements and the commit say; a maintainer's verifier
/// checks the signatures.
async fn tour(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let Some(task) = q.task else {
                return json!({ "error": "a task is needed" });
            };
            let result =
                speccoder_runtime::service::tour::of_task(&root, q.feature.as_deref(), &task);
            serde_json::to_value(&result).unwrap_or_else(|e| json!({ "error": e.to_string() }))
        })
        .await,
    )
}

async fn diff(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Response {
    let root = s.root.get();
    let Some(commit) = revision(&q) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "a commit id or an attempt ref is needed" })),
        )
            .into_response();
    };
    let patch = off(move || speccoder_runtime::service::commit_patch(&root, &commit)).await;
    Json(json!({ "patch": patch })).into_response()
}

async fn documents(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            json!(speccoder_runtime::service::documents::of_feature(
                &root,
                q.feature.as_deref()
            ))
        })
        .await,
    )
}

async fn doctor(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || speccoder_runtime::service::doctor::run(&root, q.feature.as_deref(), None))
        .await;
    envelope(&r)
}

async fn config(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || speccoder_runtime::service::settings::show(&root)).await;
    envelope(&r)
}

#[derive(Debug, Deserialize)]
struct ConfigBody {
    changes: Vec<speccoder_core::ConfigChange>,
}

/// Saves changes to `speccoder.toml`, all or nothing, after every parser
/// accepts the result. Safe while a job runs: runs read the committed file,
/// so a saved change is in force only once committed.
async fn config_save(State(s): State<Shared>, Json(b): Json<ConfigBody>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || speccoder_runtime::service::settings::apply(&root, &b.changes)).await;
    envelope(&r)
}

#[derive(Debug, Deserialize)]
struct CoverageQuery {
    /// `commands.allow_prefix` or `sandbox.network_allow_prefix`.
    key: String,
    /// The prefix as a JSON array of strings.
    prefix: String,
}

/// What a prefix entry would newly allow, from the project's journal: the
/// preview a prefix is saved from (`WidenPolicy` through `/api/run`).
async fn policy_coverage(State(s): State<Shared>, Query(q): Query<CoverageQuery>) -> Json<Value> {
    let Some(list) = speccoder_core::actions::PolicyList::of_key(&q.key) else {
        return Json(
            json!({ "status": "usage_error", "message": format!("`{}` is not a prefix list", q.key) }),
        );
    };
    let Ok(prefix) = serde_json::from_str::<Vec<String>>(&q.prefix) else {
        return Json(
            json!({ "status": "usage_error", "message": "the prefix is a JSON list of strings" }),
        );
    };
    let root = s.root.get();
    let r = off(move || {
        let state = speccoder_runtime::service::worktree::state_dir();
        speccoder_runtime::service::settings::coverage(&root, state.as_deref(), list, &prefix)
    })
    .await;
    envelope(&r)
}

/// Commits `speccoder.toml` alone — not while a job runs (an in-place run
/// commits in the same checkout).
async fn config_commit(State(s): State<Shared>) -> Json<Value> {
    let (root, hub) = (s.root.get(), Arc::clone(&s.hub));
    let r =
        off(move || hub.exclusive(|| speccoder_runtime::service::settings::commit(&root))).await;
    match r {
        Ok(result) => envelope(&result),
        Err(e) => Json(json!({ "status": "error", "message": e })),
    }
}

async fn cost(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r =
        off(move || speccoder_runtime::service::controls::cost(&root, q.feature.as_deref())).await;
    envelope(&r)
}

/// What the feature's branch changed: commits, files, the patch.
async fn changes(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(json!(
        off(move || speccoder_runtime::service::code::changes(&root, q.feature.as_deref())).await
    ))
}

/// The files at the feature's revision.
async fn tree(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let ((rev, files), worktree) = off(move || {
        (
            speccoder_runtime::service::code::tree(&root, q.feature.as_deref()),
            speccoder_runtime::service::code::worktree_view(&root, q.feature.as_deref()),
        )
    })
    .await;
    Json(json!({ "rev": rev, "files": files, "worktree": worktree }))
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    feature: Option<String>,
    path: String,
}

/// One file at the feature's revision, when the tree lists it.
async fn file(State(s): State<Shared>, Query(q): Query<FileQuery>) -> Json<Value> {
    let root = s.root.get();
    // Project paths only: nothing absolute, nothing up, nothing of the
    // state directory (where drafts, frozen and held-out sets live).
    if !file_path_ok(&q.path) {
        return Json(json!({ "error": format!("{} is not a project file", q.path) }));
    }
    let markdown = q.path.to_ascii_lowercase().ends_with(".md");
    // Committed at the revision, else untracked in the worktree.
    let r = off(move || {
        speccoder_runtime::service::code::file(&root, q.feature.as_deref(), &q.path).or_else(|e| {
            speccoder_runtime::service::code::worktree_file(&root, q.feature.as_deref(), &q.path)
                .map_err(|_| e)
        })
    })
    .await;
    Json(match r {
        // Markdown also rendered (raw HTML shown as text), for the preview.
        Ok(text) if markdown => {
            json!({ "text": text, "html": speccoder_runtime::service::documents::render(&text) })
        }
        Ok(text) => json!({ "text": text }),
        Err(e) => json!({ "error": e }),
    })
}

/// The setup assistant's view: what `doctor` sees, and what `init` would
/// write.
async fn setup(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let (doctor, preview) = off(move || {
        (
            speccoder_runtime::service::doctor::run(&root, None, None),
            speccoder_runtime::service::init::preview(&root),
        )
    })
    .await;
    Json(json!({ "doctor": doctor, "preview": preview }))
}

#[derive(Debug, Deserialize)]
struct SetupBody {
    provider: Option<String>,
    model: Option<String>,
    /// Bedrock's region.
    region: Option<String>,
}

/// Writes `speccoder.toml` and runs its checks on the untouched code; the
/// commit is the person's next click.
async fn setup_init(State(s): State<Shared>, Json(b): Json<SetupBody>) -> Json<Value> {
    if s.hub.job().is_some_and(|j| j.running) {
        return Json(json!({ "status": "error", "message": "a job is running" }));
    }
    let root = s.root.get();
    let r = off(move || {
        speccoder_runtime::service::init::prepare(
            &root,
            b.provider.as_deref(),
            b.model.as_deref(),
            b.region.as_deref(),
            false,
            false,
        )
    })
    .await;
    match r {
        Err(refused) => envelope(&refused),
        Ok(p) => {
            let (to_commit, failed) = (p.to_commit.clone(), p.failed.clone());
            let Json(mut v) = envelope(&speccoder_runtime::service::init::finish(p));
            v["to_commit"] = json!(to_commit);
            v["failed"] = json!(failed);
            Json(v)
        }
    }
}

#[derive(Debug, Deserialize)]
struct SetupCommit {
    paths: Vec<String>,
}

/// Commits what `init` wrote — only files `init` commits.
async fn setup_commit(State(s): State<Shared>, Json(b): Json<SetupCommit>) -> Json<Value> {
    let (root, hub) = (s.root.get(), Arc::clone(&s.hub));
    let r = off(move || {
        hub.exclusive(|| {
            speccoder_runtime::service::init::commit_written(
                &speccoder_runtime::service::init::root_of(&root),
                &b.paths,
            )
        })
    })
    .await;
    Json(match r.and_then(|r| r) {
        Ok(id) => json!({ "status": "ok", "message": format!("committed as {id}") }),
        Err(e) => json!({ "status": "error", "message": e }),
    })
}

/// GitHub as the person's token sees it: whether it is ready, the open
/// issues (each with the feature that names it), and the feature's pull
/// request.
async fn github(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            use speccoder_runtime::service::github;
            let ctx = github::Ctx::from_env();
            let status = github::status(&ctx, &root);
            if !status.ready {
                return json!({ "status": status });
            }
            let issues = github::issues(&ctx, &root, &github::started(&root));
            let branch = match speccoder_runtime::service::status::report(
                &root,
                q.feature.as_deref(),
            )
            .data()
            {
                Some(speccoder_core::CommandData::Report(r)) => r.branch.clone(),
                _ => None,
            };
            let pull = branch.as_deref().map(|b| github::pr_for(&ctx, &root, b));
            json!({
                "status": status,
                "issues": issues.as_ref().ok(),
                "issues_error": issues.as_ref().err(),
                "branch": branch,
                "pull": pull.as_ref().and_then(|p| p.as_ref().ok()).cloned().flatten(),
                "pull_error": pull.as_ref().and_then(|p| p.as_ref().err()),
            })
        })
        .await,
    )
}

#[derive(Debug, Deserialize)]
struct IssueQuery {
    number: String,
}

/// Starts GitHub's device sign-in for the repository's host: the code for
/// the person to enter, and where.
async fn github_device(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let slot = Arc::clone(&s.device);
    Json(
        off(move || {
            use speccoder_runtime::service::github;
            let host = github::repo(&root, None).map_or_else(|_| "github.com".to_owned(), |r| r.host);
            let Some(client) = github::auth::client_id(&host) else {
                return json!({ "error": format!("{host} has no speccoder app: sign in with a token instead") });
            };
            match github::auth::start(&github::http::oauth(&host), &client) {
                Ok(p) => {
                    let shown = json!({ "user_code": p.user_code, "verification_uri": p.verification_uri, "host": host });
                    if let Ok(mut d) = slot.lock() {
                        *d = Some((host, client, p));
                    }
                    shown
                }
                Err(e) => json!({ "error": e }),
            }
        })
        .await,
    )
}

/// Waits for the person to enter the code, then keeps the token in the
/// keychain; says whether the app can reach this repository.
async fn github_device_wait(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let slot = Arc::clone(&s.device);
    let r = off(move || {
        use speccoder_runtime::service::github;
        let Some((host, client, pending)) = slot.lock().ok().and_then(|mut d| d.take()) else {
            return speccoder_core::CommandResult::new(
                "auth",
                speccoder_core::CommandStatus::UsageError,
                "no sign-in is waiting: start it again",
            );
        };
        let ctx = github::Ctx::from_env();
        match github::auth::wait(&github::http::oauth(&host), &client, &pending) {
            Ok(token) => {
                let signed = github::login_with_token(&ctx, &host, token.expose());
                match (signed.status(), github::reach(&ctx, &root)) {
                    (speccoder_core::CommandStatus::Ok, Some(todo)) => {
                        let message = format!("{}; {todo}", signed.message());
                        signed.reworded(speccoder_core::CommandStatus::Ok, &message)
                    }
                    _ => signed,
                }
            }
            Err(e) => github::refused("auth", &e),
        }
    })
    .await;
    envelope(&r)
}

#[derive(Debug, Deserialize)]
struct TokenBody {
    token: String,
}

/// Signs in to the repository's GitHub host with a token the person pasted
/// (kept in the credential store, never echoed or logged).
async fn github_login(State(s): State<Shared>, Json(b): Json<TokenBody>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        let host = speccoder_runtime::service::github::repo(&root, None)
            .map_or_else(|_| "github.com".to_owned(), |r| r.host);
        speccoder_runtime::service::github::login_with_token(
            &speccoder_runtime::service::github::Ctx::from_env(),
            &host,
            &b.token,
        )
    })
    .await;
    envelope(&r)
}

/// One issue, the description a feature started from it gets, and the
/// skill that starts it.
async fn github_issue(State(s): State<Shared>, Query(q): Query<IssueQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            match speccoder_runtime::service::github::issue(
                &speccoder_runtime::service::github::Ctx::from_env(),
                &root,
                &q.number,
            ) {
                Ok(issue) => {
                    json!({
                        "description": speccoder_runtime::service::github::description(&issue),
                        "skill": speccoder_runtime::service::create_skill(&root),
                        "issue": issue,
                    })
                }
                Err(e) => json!({ "error": e }),
            }
        })
        .await,
    )
}

/// What `pr` would do — pushed where, the title, the body — without doing it.
async fn pr_preview(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        speccoder_runtime::service::handoff::pr(
            &root,
            q.feature.as_deref(),
            &speccoder_runtime::service::handoff::PrOptions {
                remote: "origin",
                base: None,
                open: false,
                force: false,
                github: &speccoder_runtime::service::github::Ctx::from_env(),
            },
        )
    })
    .await;
    envelope(&r)
}

/// The project's current (or last) run, whoever started it: this host's
/// job, or `speccoder run` in a terminal. `running`: its process lives and
/// it has not finished; `own`: it is this host's.
async fn live(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    Json(
        off(move || {
            let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
                return json!({ "live": null });
            };
            match speccoder_runtime::service::progress::read_live(&root, &state) {
                Some((live, running)) => json!({
                    "live": live,
                    "running": running,
                    "own": live.pid == std::process::id(),
                    "interrupted": live.finished.is_none() && !running,
                    "now": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs()),
                }),
                None => json!({ "live": null }),
            }
        })
        .await,
    )
}

async fn job(State(s): State<Shared>) -> Json<Value> {
    Json(json!({ "job": s.hub.job(), "waiting": s.hub.waiting() }))
}

#[derive(Debug, Deserialize)]
struct Start {
    run: Runnable,
    input: Option<String>,
    feature: Option<String>,
    command: String,
    /// Before the host is bound: the folder an Undo is for (a recent project).
    #[serde(default)]
    root: Option<String>,
}

async fn start(State(s): State<Shared>, Json(b): Json<Start>) -> Response {
    let root = match welcome::job_root(&s.root, &b.run, b.root.as_deref()) {
        Ok(r) => r,
        Err(e) => return welcome::no_project_yet(&e),
    };
    match s.hub.start(b.run, b.input, b.feature, b.command, root) {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct AnswerBody {
    id: u64,
    answer: Option<speccoder_runtime::service::approvals::Answer>,
    text: Option<String>,
}

async fn answer(State(s): State<Shared>, Json(b): Json<AnswerBody>) -> Response {
    let reply = match b.answer {
        Some(a) => jobs::Reply::Answer(a),
        None => jobs::Reply::Text(b.text.filter(|t| !t.trim().is_empty())),
    };
    match s.hub.answer(b.id, reply) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response(),
    }
}

async fn stop(State(s): State<Shared>) -> Json<Value> {
    s.hub.stop();
    Json(json!({ "ok": true }))
}

/// Server-sent events: what the job does, what it asks. Read with `fetch`
/// (the token is a header, never in the address).
async fn events(State(s): State<Shared>) -> Response {
    use futures_util::StreamExt;
    let rx = s.hub.subscribe();
    let hello = futures_util::stream::once(async {
        Ok::<_, std::convert::Infallible>(
            axum::response::sse::Event::default().data(json!({ "type": "hello" }).to_string()),
        )
    });
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|m| async move {
        m.ok().map(|text| {
            Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(text))
        })
    });
    // An event stream never ends on its own: it ends when the host stops,
    // or a graceful shutdown would wait for it forever.
    let mut closing = s.closing.clone();
    let stream = stream.take_until(async move {
        let _ = closing.wait_for(|c| *c).await;
    });
    axum::response::sse::Sse::new(hello.chain(stream))
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

/// The routes.
pub fn router(s: Shared) -> Router {
    Router::new()
        .route("/api/project", get(project))
        .route("/api/report", get(report))
        .route("/api/actions", get(actions))
        .route("/api/inbox", get(inbox))
        .route("/api/bug", get(bug))
        .route("/api/account", get(account))
        .route("/api/review", get(review))
        .route("/api/review/comments", get(review_comments))
        .route("/api/approvals", get(approvals))
        .route("/api/evidence", get(evidence))
        .route("/api/tour", get(tour))
        .route("/api/diff", get(diff))
        .route("/api/documents", get(documents))
        .route("/api/doctor", get(doctor))
        .route("/api/job", get(job))
        .route("/api/live", get(live))
        .route("/api/config", get(config).post(config_save))
        .route("/api/config/commit", post(config_commit))
        .route("/api/policy/coverage", get(policy_coverage))
        .route("/api/cost", get(cost))
        .route("/api/changes", get(changes))
        .route("/api/tree", get(tree))
        .route("/api/file", get(file))
        .route("/api/setup", get(setup).post(setup_init))
        .route("/api/github", get(github))
        .route("/api/github/issue", get(github_issue))
        .route("/api/github/login", post(github_login))
        .route("/api/github/device", post(github_device))
        .route("/api/github/device/wait", post(github_device_wait))
        .route("/api/pr/preview", get(pr_preview))
        .route("/api/setup/commit", post(setup_commit))
        .route("/api/events", get(events))
        .route("/api/run", post(start))
        .route("/api/answer", post(answer))
        .route("/api/stop", post(stop))
        .route("/api/welcome", get(welcome::view))
        .route("/api/open", post(welcome::open))
        .route("/api/recent/forget", post(welcome::forget))
        .route("/api/folders", get(welcome::folders))
        .route("/api/new/plan", get(welcome::plan))
        .route("/api/new/first-feature", post(welcome::first_feature))
        .route("/api/similar", get(welcome::similar))
        .route("/api/suggest", get(welcome::suggest))
        .route("/api/github/repos", get(welcome::repositories))
        .route("/api/skill-cost", get(welcome::skill_cost))
        .fallback(get(asset))
        .layer(axum::middleware::from_fn_with_state(
            s.clone(),
            welcome::projectless,
        ))
        .layer(axum::middleware::from_fn_with_state(s.clone(), guard))
        .with_state(s)
}

/// The host's events as a shell in the same process reads them.
pub type Events = tokio::sync::broadcast::Receiver<String>;
/// Why reading [`Events`] failed: fell behind (`Lagged`) or the host ended.
pub use tokio::sync::broadcast::error::RecvError;

/// A started host.
#[derive(Debug)]
pub struct Started {
    /// The address to open: the page, with the token in its fragment.
    pub url: String,
    /// The port.
    pub port: u16,
    /// What the host's jobs do, as the page's events (JSON lines): for a
    /// shell in the same process (the window) to follow without a bridge.
    pub events: Option<Events>,
    /// The host's project: the window binds it in-process to the folder
    /// the person picked (the path never transits the page).
    pub root: Root,
}

/// Serves the project at `root` on `port` (0: any free one) until the
/// process ends; `ready` is told the address once it listens. Outside a
/// project the host serves the welcome until it is bound to one.
pub fn serve(root: &Path, port: u16, ready: impl FnOnce(Started) + Send) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (close, closed) = tokio::sync::watch::channel(false);
        let token = security::token();
        if token.is_empty() {
            return Err("no random source for the token".to_owned());
        }
        let root = Root::new(root, welcome::is_project(root));
        let shared = Shared {
            root: root.clone(),
            token: token.clone(),
            port,
            hub: jobs::Hub::new(root.clone()),
            closing: closed,
            device: Arc::default(),
        };
        ready(Started {
            url: format!("http://127.0.0.1:{port}/#token={token}"),
            port,
            events: Some(shared.hub.subscribe()),
            root,
        });
        // ctrl-c: end the event streams, give open requests a moment, then
        // stop whatever still holds on.
        let (stop, stopping) = tokio::sync::oneshot::channel::<()>();
        let server = axum::serve(listener, router(shared)).with_graceful_shutdown(async move {
            let _ = stopping.await;
        });
        let signal = async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = close.send(true);
            let _ = stop.send(());
            tokio::time::sleep(GRACE).await;
        };
        tokio::select! {
            served = server => served.map_err(|e| e.to_string()),
            () = signal => Ok(()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(commit: Option<&str>, reference: Option<&str>) -> FeatureQuery {
        FeatureQuery {
            feature: None,
            task: None,
            commit: commit.map(str::to_owned),
            reference: reference.map(str::to_owned),
        }
    }

    #[test]
    fn only_commits_and_attempt_refs_reach_git_show() {
        assert!(revision(&q(Some("e23d4197bb"), None)).is_some());
        assert!(revision(&q(Some("e23d"), None)).is_none());
        assert!(revision(&q(Some("--output=/tmp/x"), None)).is_none());
        let attempt = "refs/speccoder/attempts/001-greet/T001/run_01M5S5VJ";
        assert_eq!(revision(&q(None, Some(attempt))).as_deref(), Some(attempt));
        for bad in [
            "refs/heads/main",
            "refs/speccoder/attempts/",
            "refs/speccoder/attempts/../../heads/main",
            "refs/speccoder/attempts/a b",
            "--all",
            "HEAD",
        ] {
            assert!(revision(&q(None, Some(bad))).is_none(), "{bad}");
        }
    }
}
