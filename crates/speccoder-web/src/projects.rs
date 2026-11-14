//! Several projects, one view (spec 097): the recent projects rolled up,
//! and another project opened in its own host — its own process and key.
//! Nothing here answers with another host's address or key, decides
//! anything in another project, or writes into it.

use std::path::Path;

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use speccoder_core::projects::Place;
use speccoder_runtime::service::projects;

use crate::{Shared, envelope, off};

/// The recent projects rolled up, and what waits across them.
pub(crate) async fn list(State(s): State<Shared>) -> Json<Value> {
    let here = s.root.bound();
    let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
        return Json(json!({ "status": "config_error", "message": "no state directory" }));
    };
    let r = off(move || projects::list(&state, here.as_deref())).await;
    envelope(&s, &r)
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenBody {
    /// A recent project's folder.
    path: String,
    /// `inbox[:feature[/task]]`.
    #[serde(default)]
    at: Option<String>,
}

fn opened(r: Result<projects::Opened, String>) -> Json<Value> {
    Json(match r {
        Ok(o) => json!({ "ok": true, "opened": o }),
        Err(e) => json!({ "ok": false, "error": e }),
    })
}

/// Opens a recent project in its own host, at a place.
pub(crate) async fn open_project(State(s): State<Shared>, Json(b): Json<OpenBody>) -> Json<Value> {
    let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
        return opened(Err("no state directory".into()));
    };
    let at = match b.at.as_deref() {
        None => None,
        Some(a) => match Place::parse(a) {
            Some(p) => Some(p),
            None => return opened(Err(format!("{a} is not a place to open"))),
        },
    };
    let window = s.window;
    opened(off(move || projects::open(&state, &b.path, at.as_ref(), None, window)).await)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReportBody {
    /// The recent project to report to.
    path: String,
    /// This project's feature and task that stopped.
    feature: String,
    task: String,
    /// What should happen instead: given, the bug is recorded there now;
    /// absent, its bug sheet opens there with the stop.
    #[serde(default)]
    expected: Option<String>,
}

/// Opens another recent project's bug sheet, prefilled from a stop here:
/// the text is built here from this project's own record, never sent by
/// the page.
pub(crate) async fn report_to_project(
    State(s): State<Shared>,
    Json(b): Json<ReportBody>,
) -> Json<Value> {
    let Some(state) = speccoder_runtime::service::worktree::state_dir() else {
        return opened(Err("no state directory".into()));
    };
    let Some(here) = s.root.bound() else {
        return opened(Err("this host serves no project".into()));
    };
    if Path::new(&b.path) == here {
        return opened(Err("that is this project: file the bug here".into()));
    }
    let window = s.window;
    if let Some(expected) = b.expected {
        let r = off(move || {
            projects::file_report(&state, &here, &b.path, &b.feature, &b.task, &expected)
        })
        .await;
        return Json(match r {
            Ok(m) => json!({ "ok": true, "recorded": m }),
            Err(e) => json!({ "ok": false, "error": e }),
        });
    }
    // Open elsewhere already: the draft would not reach it.
    opened(
        off(move || {
            let text = projects::report_text(&here, &b.feature, &b.task)?;
            match projects::open(&state, &b.path, None, Some(&text), window)? {
                projects::Opened::Elsewhere { name, pid, .. } => Err(format!(
                    "{name} is open in another speccoder (process {pid}): Report to {name} records it from here, or report it there"
                )),
                o => Ok(o),
            }
        })
        .await,
    )
}
