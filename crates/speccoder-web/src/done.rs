//! The definition of done for a feature's branch (spec 127): what the
//! Pull Request card shows before Open Draft PR. Read-only.

use axum::Json;
use axum::extract::{Query, State};
use serde_json::Value;

use crate::{FeatureQuery, Shared, envelope, off};

/// `GET /api/done?feature=`.
pub(crate) async fn report(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || speccoder_runtime::service::done::for_feature(&root, q.feature.as_deref()))
        .await;
    envelope(&s, &r)
}
