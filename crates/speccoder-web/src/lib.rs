//! speccoder's web interface (F-076, D-087): a host on 127.0.0.1 that
//! serves the interface — embedded in the binary — and an API over
//! speccoder's own payloads. The page, a browser tab or the desktop window,
//! holds no logic of its own: what it shows and offers comes from the
//! runtime, as for the CLI.

pub mod jobs;
pub mod security;

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

/// The host's state.
#[derive(Debug, Clone)]
pub struct Shared {
    /// The project.
    pub root: PathBuf,
    /// The per-start token.
    pub token: String,
    /// The port it listens on.
    pub port: u16,
    /// Jobs and events.
    pub hub: Arc<jobs::Hub>,
    /// Turns true when the host stops: open event streams end with it.
    pub closing: tokio::sync::watch::Receiver<bool>,
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
        Err(_) => std::process::abort(),
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

fn envelope(r: &CommandResult) -> Json<Value> {
    Json(serde_json::to_value(r).unwrap_or(Value::Null))
}

async fn project(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.clone();
    Json(
        off(move || {
            let format = speccoder_runtime::format::at(&root);
            let features = speccoder_runtime::current::list(&root, None);
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            json!({
                "name": name,
                "root": root.display().to_string(),
                "format": format.wording().name,
                "unit": format.messages().unit,
                "features": features.data().and_then(|d| match d {
                    CommandData::Features(f) => serde_json::to_value(f).ok(),
                    _ => None,
                }),
                "version": env!("CARGO_PKG_VERSION"),
            })
        })
        .await,
    )
}

async fn report(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    let r = off(move || {
        speccoder_runtime::next::attach(
            speccoder_runtime::status::report(&root, q.feature.as_deref()),
            q.feature.as_deref(),
        )
    })
    .await;
    envelope(&r)
}

async fn actions(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    Json(
        off(move || {
            json!(speccoder_runtime::actions::available(
                &root,
                q.feature.as_deref(),
                q.task.as_deref()
            ))
        })
        .await,
    )
}

async fn approvals(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    let r = off(move || {
        speccoder_runtime::approvals::decide(
            &root,
            q.feature.as_deref(),
            speccoder_runtime::approvals::Verb::List,
        )
    })
    .await;
    envelope(&r)
}

async fn evidence(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    Json(
        off(move || {
            let report = speccoder_runtime::status::report(&root, q.feature.as_deref());
            let Some(CommandData::Report(r)) = report.data() else {
                return json!({ "error": report.message() });
            };
            let Some(t) = r.tasks.iter().find(|t| Some(&t.id) == q.task.as_ref()) else {
                return json!({ "error": "no such task" });
            };
            let statements = t
                .commit
                .as_deref()
                .map(|c| speccoder_runtime::evidence::statements(&root, c))
                .unwrap_or_default();
            let (mark, words) = speccoder_runtime::evidence::banner(&t.state);
            json!({
                "task": t,
                "banner": { "mark": mark, "words": words },
                "rows": speccoder_runtime::evidence::rows(t, &statements, None),
            })
        })
        .await,
    )
}

async fn diff(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Response {
    let root = s.root.clone();
    let Some(commit) = revision(&q) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "a commit id or an attempt ref is needed" })),
        )
            .into_response();
    };
    let patch = off(move || {
        speccoder_runtime::git::Git::at(&root)
            .run(&[
                "show",
                "--no-color",
                "--patch",
                "--format=%h %s%n%n%b",
                &commit,
            ])
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    })
    .await;
    Json(json!({ "patch": patch })).into_response()
}

async fn documents(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    Json(
        off(move || {
            json!(speccoder_runtime::documents::of_feature(
                &root,
                q.feature.as_deref()
            ))
        })
        .await,
    )
}

async fn doctor(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    let r = off(move || speccoder_runtime::doctor::run(&root, q.feature.as_deref(), None)).await;
    envelope(&r)
}

async fn config(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.clone();
    let r = off(move || speccoder_runtime::settings::show(&root)).await;
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
    let root = s.root.clone();
    let r = off(move || speccoder_runtime::settings::apply(&root, &b.changes)).await;
    envelope(&r)
}

/// Commits `speccoder.toml` alone — not while a job runs (an in-place run
/// commits in the same checkout).
async fn config_commit(State(s): State<Shared>) -> Json<Value> {
    if s.hub.job().is_some_and(|j| j.running) {
        return Json(
            json!({ "status": "error", "message": "a job is running; commit when it ends" }),
        );
    }
    let root = s.root.clone();
    let r = off(move || speccoder_runtime::settings::commit(&root)).await;
    envelope(&r)
}

async fn cost(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    let r = off(move || speccoder_runtime::controls::cost(&root, q.feature.as_deref())).await;
    envelope(&r)
}

/// What the feature's branch changed: commits, files, the patch.
async fn changes(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    Json(json!(
        off(move || speccoder_runtime::code::changes(&root, q.feature.as_deref())).await
    ))
}

/// The files at the feature's revision.
async fn tree(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.clone();
    let (rev, files) =
        off(move || speccoder_runtime::code::tree(&root, q.feature.as_deref())).await;
    Json(json!({ "rev": rev, "files": files }))
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    feature: Option<String>,
    path: String,
}

/// One file at the feature's revision, when the tree lists it.
async fn file(State(s): State<Shared>, Query(q): Query<FileQuery>) -> Json<Value> {
    let root = s.root.clone();
    let markdown = q.path.to_ascii_lowercase().ends_with(".md");
    let r = off(move || speccoder_runtime::code::file(&root, q.feature.as_deref(), &q.path)).await;
    Json(match r {
        // Markdown also rendered (raw HTML shown as text), for the preview.
        Ok(text) if markdown => {
            json!({ "text": text, "html": speccoder_runtime::documents::render(&text) })
        }
        Ok(text) => json!({ "text": text }),
        Err(e) => json!({ "error": e }),
    })
}

/// The setup assistant's view: what `doctor` sees, and what `init` would
/// write.
async fn setup(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.clone();
    let (doctor, preview) = off(move || {
        (
            speccoder_runtime::doctor::run(&root, None, None),
            speccoder_runtime::init::preview(&root),
        )
    })
    .await;
    Json(json!({ "doctor": doctor, "preview": preview }))
}

#[derive(Debug, Deserialize)]
struct SetupBody {
    provider: Option<String>,
    model: Option<String>,
}

/// Writes `speccoder.toml` and runs its checks on the untouched code; the
/// commit is the person's next click.
async fn setup_init(State(s): State<Shared>, Json(b): Json<SetupBody>) -> Json<Value> {
    if s.hub.job().is_some_and(|j| j.running) {
        return Json(json!({ "status": "error", "message": "a job is running" }));
    }
    let root = s.root.clone();
    let r = off(move || {
        speccoder_runtime::init::prepare(
            &root,
            b.provider.as_deref(),
            b.model.as_deref(),
            false,
            false,
        )
    })
    .await;
    match r {
        Err(refused) => envelope(&refused),
        Ok(p) => {
            let (to_commit, failed) = (p.to_commit.clone(), p.failed.clone());
            let Json(mut v) = envelope(&speccoder_runtime::init::finish(p));
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
    let root = s.root.clone();
    let r = off(move || {
        speccoder_runtime::init::commit_written(&speccoder_runtime::init::root_of(&root), &b.paths)
    })
    .await;
    Json(match r {
        Ok(id) => json!({ "status": "ok", "message": format!("committed as {id}") }),
        Err(e) => json!({ "status": "error", "message": e }),
    })
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
}

async fn start(State(s): State<Shared>, Json(b): Json<Start>) -> Response {
    match s.hub.start(b.run, b.input, b.feature, b.command) {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct AnswerBody {
    id: u64,
    answer: Option<speccoder_runtime::approvals::Answer>,
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
        .route("/api/approvals", get(approvals))
        .route("/api/evidence", get(evidence))
        .route("/api/diff", get(diff))
        .route("/api/documents", get(documents))
        .route("/api/doctor", get(doctor))
        .route("/api/job", get(job))
        .route("/api/config", get(config).post(config_save))
        .route("/api/config/commit", post(config_commit))
        .route("/api/cost", get(cost))
        .route("/api/changes", get(changes))
        .route("/api/tree", get(tree))
        .route("/api/file", get(file))
        .route("/api/setup", get(setup).post(setup_init))
        .route("/api/setup/commit", post(setup_commit))
        .route("/api/events", get(events))
        .route("/api/run", post(start))
        .route("/api/answer", post(answer))
        .route("/api/stop", post(stop))
        .fallback(get(asset))
        .layer(axum::middleware::from_fn_with_state(s.clone(), guard))
        .with_state(s)
}

/// The host's events as a shell in the same process reads them.
pub type Events = tokio::sync::broadcast::Receiver<String>;

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
}

/// Serves the project at `root` on `port` (0: any free one) until the
/// process ends; `ready` is told the address once it listens.
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
        let shared = Shared {
            root: root.to_path_buf(),
            token: token.clone(),
            port,
            hub: jobs::Hub::new(root),
            closing: closed,
        };
        ready(Started {
            url: format!("http://127.0.0.1:{port}/#token={token}"),
            port,
            events: Some(shared.hub.subscribe()),
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
