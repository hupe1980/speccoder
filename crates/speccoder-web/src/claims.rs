//! Story claims: who holds which story of a feature, as this
//! repository last fetched them — no network on a page load. Claiming and
//! taking over run as actions (`Runnable::Claim`).

use axum::Json;
use axum::extract::{Query, State};
use serde_json::Value;
use speccoder_runtime::service::claims;

use crate::{FeatureQuery, Shared, envelope, off};

/// `GET /api/claims?feature=`: the feature's claims.
pub(crate) async fn list(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || claims::local(&root, q.feature.as_deref())).await;
    envelope(&s, &r)
}
