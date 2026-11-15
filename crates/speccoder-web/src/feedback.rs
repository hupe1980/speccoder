//! Feedback between projects (spec 125) over the web host: what this
//! project asks of its upstreams and what others ask of it; a person's
//! decision on a request (kept in this project's own state); an item
//! prepared, or filed after the page's confirm, as a GitHub issue.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::feedback::Status;
use speccoder_runtime::service::feedback;

use crate::{Shared, envelope, off};

fn state() -> Result<std::path::PathBuf, Json<Value>> {
    speccoder_runtime::service::worktree::state_dir()
        .ok_or_else(|| Json(json!({ "ok": false, "error": "no state directory" })))
}

/// `speccoder feedback`, as the page reads it.
pub(crate) async fn list(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    let r = off(move || feedback::report(&st, &root)).await;
    envelope(&s, &r)
}

#[derive(Debug, Deserialize)]
pub(crate) struct DecideBody {
    /// The asking project, as the requests name it.
    source: String,
    id: String,
    /// `taken_up`, `bug` or `declined`.
    decision: String,
    #[serde(default)]
    reason: Option<String>,
}

/// A person's decision on a request: taken up as a spec, recorded as a
/// bug (once its sheet opened), or declined with a reason.
pub(crate) async fn decide(State(s): State<Shared>, Json(b): Json<DecideBody>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    let status = match b.decision.as_str() {
        "taken_up" => Status::TakenUp {
            unit: String::new(),
        },
        "bug" => Status::Bug {
            task: String::new(),
        },
        "declined" => match b.reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => Status::Declined {
                reason: r.chars().take(500).collect(),
            },
            None => return Json(json!({ "ok": false, "error": "say why it is declined" })),
        },
        other => return Json(json!({ "ok": false, "error": format!("`{other}` is no decision") })),
    };
    let r = off(move || feedback::decide(&st, &root, &b.source, &b.id, status)).await;
    Json(match r {
        Ok(()) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct ItemQuery {
    upstream: String,
    id: String,
}

/// An item prepared as a GitHub issue: its title, body and new-issue page.
pub(crate) async fn prepared(Query(q): Query<ItemQuery>, State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    Json(
        match off(move || feedback::prepared(&st, &root, &q.upstream, &q.id)).await {
            Ok(p) => json!({ "ok": true, "prepared": p }),
            Err(e) => json!({ "ok": false, "error": e }),
        },
    )
}

/// Files the prepared issue with the person's GitHub sign-in (the page
/// showed exactly what is sent and asked first); never twice.
pub(crate) async fn file(State(s): State<Shared>, Json(q): Json<ItemQuery>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    let r = off(move || {
        let ctx = speccoder_runtime::service::github::Ctx::from_env();
        feedback::file_issue(&ctx, &st, &root, &q.upstream, &q.id)
    })
    .await;
    Json(match r {
        Ok((number, url)) => json!({ "ok": true, "number": number, "url": url }),
        Err(e) => json!({ "ok": false, "error": e }),
    })
}

/// Removes an item from this project's own outbox: adopted, or no longer
/// needed.
pub(crate) async fn withdraw(State(s): State<Shared>, Json(q): Json<ItemQuery>) -> Json<Value> {
    let root = s.root.get();
    Json(
        match off(move || feedback::withdraw(&root, &q.upstream, &q.id)).await {
            Ok(()) => json!({ "ok": true }),
            Err(e) => json!({ "ok": false, "error": e }),
        },
    )
}
