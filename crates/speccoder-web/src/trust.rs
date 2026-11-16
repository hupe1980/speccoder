//! A checkout's agent files over the web host: each file with
//! where it stands and what changed, and a person's trust in the files
//! they saw — bound to the digests the page showed.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::{CommandData, CommandResult, CommandStatus};
use speccoder_runtime::service::agent_files::{self, Refused};

use crate::{Shared, envelope, off};

fn state() -> Result<std::path::PathBuf, Json<Value>> {
    speccoder_runtime::service::worktree::state_dir()
        .ok_or_else(|| Json(json!({ "ok": false, "error": "no state directory" })))
}

/// Every agent file with its state and diff.
pub(crate) async fn list(State(s): State<Shared>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    let report = off(move || agent_files::report(&root, &st)).await;
    let message = report
        .title()
        .unwrap_or_else(|| "every agent file is trusted as it is now".to_owned());
    let r = CommandResult::new("trust", CommandStatus::Ok, &message)
        .with_data(CommandData::Trust(Box::new(report)));
    envelope(&s, &r)
}

#[derive(Debug, Deserialize)]
pub(crate) struct Seen {
    path: String,
    digest: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TrustBody {
    files: Vec<Seen>,
}

/// Trusts the files the person saw, each only if it is still what they
/// saw; otherwise nothing is recorded and the changed ones are named.
pub(crate) async fn trust_files(State(s): State<Shared>, Json(b): Json<TrustBody>) -> Json<Value> {
    let root = s.root.get();
    let st = match state() {
        Ok(st) => st,
        Err(e) => return e,
    };
    if b.files.is_empty() || b.files.iter().any(|f| f.digest.trim().is_empty()) {
        return Json(json!({ "ok": false, "error": "name each file with the digest you saw" }));
    }
    let files: Vec<(String, String)> = b.files.into_iter().map(|f| (f.path, f.digest)).collect();
    let r = off(move || agent_files::trust_as_person(&root, &st, &files, "web")).await;
    Json(match r {
        Ok(done) => json!({ "ok": true, "trusted": done }),
        Err(Refused::Changed(p)) => json!({
            "ok": false,
            "changed": p,
            "error": "changed since you saw it — look again",
        }),
        Err(Refused::Unknown(p)) => json!({
            "ok": false,
            "error": format!("not an agent file of this checkout: {}", p.join(", ")),
        }),
    })
}
