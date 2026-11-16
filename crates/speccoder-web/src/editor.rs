//! The person's editor over the web host: the editors found
//! here and the chosen one, choosing one, and opening a project file at a
//! line in it. The page names a project-relative path; the host resolves
//! and checks it, and runs only the chosen window editor's launcher.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::Value;
use speccoder_runtime::service::editor;

use crate::{Shared, envelope, off};

/// `GET /api/editor`.
pub(crate) async fn list(State(s): State<Shared>) -> Json<Value> {
    let r = off(editor::report).await;
    envelope(&s, &r)
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choose {
    editor: String,
}

/// `POST /api/editor`: makes one editor the person's.
pub(crate) async fn choose(State(s): State<Shared>, Json(b): Json<Choose>) -> Json<Value> {
    let r = off(move || editor::choose(&b.editor)).await;
    envelope(&s, &r)
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenBody {
    path: String,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    column: Option<u32>,
    #[serde(default)]
    feature: Option<String>,
}

/// `POST /api/editor/open`: never a terminal editor — the host has no terminal.
pub(crate) async fn open(State(s): State<Shared>, Json(b): Json<OpenBody>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        editor::open(
            &root,
            b.feature.as_deref(),
            &b.path,
            b.line,
            b.column,
            false,
        )
    })
    .await;
    envelope(&s, &r)
}
