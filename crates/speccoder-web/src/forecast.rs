//! The merge forecast for a feature's branch (spec 129): the last one, as
//! stored, for the Pull Request card and the Overview; and a new one on the
//! person's click — fetched, paired, its tests run in the sandbox. One at a
//! time per feature (`FORECAST-BUSY`).

use axum::Json;
use axum::extract::{Query, State};
use serde_json::Value;
use speccoder_runtime::service::forecast;

use crate::{FeatureQuery, Shared, envelope, off};

/// `GET /api/forecast?feature=`: the last forecast (none yet: no data).
pub(crate) async fn last(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || forecast::last(&root, q.feature.as_deref(), None)).await;
    envelope(&s, &r)
}

/// `POST /api/forecast?feature=`: a new forecast.
pub(crate) async fn check(State(s): State<Shared>, Query(q): Query<FeatureQuery>) -> Json<Value> {
    let root = s.root.get();
    let r = off(move || {
        forecast::run(
            &root,
            &forecast::Options {
                feature: q.feature.as_deref(),
                fetch: true,
                ..forecast::Options::default()
            },
        )
    })
    .await;
    envelope(&s, &r)
}
