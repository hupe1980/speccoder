//! Views: an extension's data shown by speccoder's own widgets. A `run`
//! source runs like an observer command — in the sandbox, speccoder's
//! state hidden, the checkout read-only — but with no network; a `file`
//! source is read as it is. Results are kept with the digest of what they
//! read and shown while that holds; nothing of the extension reaches the
//! page but data.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use speccoder_core::extensions::{
    ViewData, ViewDef, ViewPlace, ViewResult, ViewShown, ViewSource, ViewWidget, ViewsReport,
};
use speccoder_core::hooks::TrustState;

use super::{Loaded, trust};
use crate::verify::process::Invocation;

/// The most output a view's command may print.
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_ROWS: usize = 500;
const MAX_METRICS: usize = 24;
const MAX_METRIC_TEXT: usize = 80;
const FILE: &str = "views.json";

fn store(state: &Path, root: &Path) -> std::path::PathBuf {
    crate::worktree::project_state_dir(state, root).join("extensions")
}

fn read_kept(dir: &Path) -> BTreeMap<String, ViewResult> {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The views placed on `feature` (or, with `None`, on the project), each
/// with its result for the current inputs: kept when they did not change,
/// else produced now. `again` names a view to produce anew.
#[must_use]
pub fn views(
    root: &Path,
    state: &Path,
    feature: Option<&str>,
    again: Option<(&str, &str)>,
) -> ViewsReport {
    let dir = crate::hooks::dir(state, root);
    let kept_dir = store(state, root);
    let mut kept = read_kept(&kept_dir);
    let feature_dir = feature.and_then(|f| super::feature_dir(root, f));
    let place = if feature.is_some() {
        ViewPlace::Feature
    } else {
        ViewPlace::Project
    };
    let mut out = Vec::new();
    let mut changed = false;
    for e in super::load(root).iter().filter(|e| e.usable()) {
        let trusted = trust::state(&dir, e) == TrustState::Trusted;
        for v in e.views.iter().filter(|v| v.place == place) {
            if !trusted {
                out.push(ViewShown {
                    extension: e.name.clone(),
                    view: v.clone(),
                    result: None,
                    waits_for_trust: true,
                });
                continue;
            }
            let key = format!("{}/{}/{}", e.name, v.name, feature.unwrap_or("-"));
            let inputs = inputs(e, v, root, feature_dir.as_deref());
            let fresh = again.is_none_or(|(x, y)| x != e.name || y != v.name);
            let result = match kept.get(&key) {
                Some(r) if r.inputs == inputs && fresh => r.clone(),
                _ => {
                    let r = produce(e, v, root, state, feature, feature_dir.as_deref(), inputs);
                    kept.insert(key, r.clone());
                    changed = true;
                    r
                }
            };
            out.push(ViewShown {
                extension: e.name.clone(),
                view: v.clone(),
                result: Some(result),
                waits_for_trust: false,
            });
        }
    }
    if changed {
        crate::hooks::write_json(&kept_dir, FILE, &kept);
    }
    ViewsReport { views: out }
}

/// The digest of what `v` reads: the extension, its source, and the
/// feature's files (a `run` source) or the file (a `file` source).
fn inputs(e: &Loaded, v: &ViewDef, root: &Path, feature_dir: Option<&Path>) -> String {
    use sha2::{Digest as _, Sha256};
    let mut h = Sha256::new();
    h.update(e.digest.as_bytes());
    h.update(serde_json::to_vec(v).unwrap_or_default());
    match &v.source {
        ViewSource::File(f) => {
            let base = match v.place {
                ViewPlace::Feature => feature_dir.map(Path::to_path_buf),
                ViewPlace::Project => Some(super::project(root)),
            };
            if let Some(b) = base {
                h.update(std::fs::read(b.join(f)).unwrap_or_default());
            }
        }
        ViewSource::Run(_) => {
            if let Some(d) = feature_dir
                && let Ok((digest, _)) = super::digest(d)
            {
                h.update(digest.as_bytes());
            }
        }
    }
    format!(
        "sha256:{}",
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn produce(
    e: &Loaded,
    v: &ViewDef,
    root: &Path,
    state: &Path,
    feature: Option<&str>,
    feature_dir: Option<&Path>,
    inputs: String,
) -> ViewResult {
    let began = Instant::now();
    let raw = match &v.source {
        ViewSource::File(f) => {
            let base = match v.place {
                ViewPlace::Feature => feature_dir.map(Path::to_path_buf),
                ViewPlace::Project => Some(super::project(root)),
            };
            match base {
                None => Err("the feature's folder was not found".to_owned()),
                Some(b) => std::fs::read_to_string(b.join(f)).map_err(|err| match err.kind() {
                    std::io::ErrorKind::NotFound => format!("{f} does not exist yet"),
                    _ => format!("{f}: {err}"),
                }),
            }
        }
        ViewSource::Run(argv) => run(e, v, argv, root, state, feature, feature_dir),
    };
    let duration_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (data, error) = match raw.and_then(|text| shape(v, &text, root)) {
        Ok(d) => (Some(d), None),
        Err(err) => (None, Some(err)),
    };
    ViewResult {
        inputs,
        ran_at: crate::attest::now_rfc3339(),
        duration_ms,
        data,
        error,
    }
}

fn run(
    e: &Loaded,
    v: &ViewDef,
    argv: &[String],
    root: &Path,
    state: &Path,
    feature: Option<&str>,
    feature_dir: Option<&Path>,
) -> Result<String, String> {
    let (runner, _) = crate::sandbox::runner_at(root)?;
    if !runner.is_sandboxed() {
        return Err(
            "no sandbox is enforced here, and a view's command runs only in one".to_owned(),
        );
    }
    let (runner, _) = runner.carve(
        &[root.to_path_buf()],
        &crate::hooks::gate::hidden_state(state, root),
    );
    if !runner.hides() {
        return Err("no sandbox is enforced here that hides speccoder's state".to_owned());
    }
    let dir_text = feature_dir
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let argv = super::resolve(
        argv,
        Some(&e.folder),
        &[("SPECCODER_FEATURE_DIR", dir_text.clone())],
    );
    let mut env = crate::verify::process::scrubbed_env(std::env::vars());
    if let Some(f) = feature {
        env.push(("SPECCODER_FEATURE".to_owned(), f.to_owned()));
        env.push(("SPECCODER_FEATURE_DIR".to_owned(), dir_text));
    }
    env.push(("PYTHONDONTWRITEBYTECODE".to_owned(), "1".to_owned()));
    let inv = Invocation {
        argv,
        cwd: e.folder.clone(),
        env,
        timeout: Duration::from_secs(v.timeout),
        output_cap: MAX_OUTPUT + 1,
    };
    // No network at all, whatever the project's sandbox allows commands.
    let ended = runner.run_offline(&inv);
    if let Some(err) = ended.spawn_error {
        return Err(format!("could not start: {err}"));
    }
    if ended.timed_out {
        return Err(format!("it ran longer than {} s", v.timeout));
    }
    if ended.stdout.kept.len() > MAX_OUTPUT {
        return Err("it printed more than 2 MB".to_owned());
    }
    let code = ended.exit_code.unwrap_or(-1);
    if !v.exit.contains(&code) {
        let stderr = String::from_utf8_lossy(&ended.stderr.kept);
        let last = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        return Err(format!(
            "it exited {code}{}",
            if last.is_empty() {
                String::new()
            } else {
                format!(": {}", speccoder_core::hooks::clean(last))
            }
        ));
    }
    Ok(String::from_utf8_lossy(&ended.stdout.kept).into_owned())
}

/// The output as the widget's data, or why it does not fit.
fn shape(v: &ViewDef, text: &str, root: &Path) -> Result<ViewData, String> {
    let project = super::project(root);
    if v.widget == ViewWidget::Markdown {
        return Ok(ViewData::Markdown {
            html: crate::documents::render(text),
        });
    }
    if v.widget == ViewWidget::Findings {
        return speccoder_core::extensions::sarif(text, |p| super::relative_to(&project, p))
            .map(|findings| ViewData::Findings { findings });
    }
    let json: Value =
        serde_json::from_str(text).map_err(|err| format!("the output is not JSON: {err}"))?;
    let picked = match &v.path {
        None => &json,
        Some(p) => p
            .split('.')
            .try_fold(&json, |at, k| at.get(k))
            .ok_or_else(|| format!("the output has no `{p}`"))?,
    };
    let text_of = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    match v.widget {
        ViewWidget::Table => {
            let rows = picked
                .as_array()
                .ok_or("a table's data is a list of objects")?;
            if rows.len() > MAX_ROWS {
                return Err(format!(
                    "{} rows; a table shows at most {MAX_ROWS}",
                    rows.len()
                ));
            }
            let columns: Vec<String> = if v.columns.is_empty() {
                rows.first()
                    .and_then(Value::as_object)
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default()
            } else {
                v.columns.clone()
            };
            let rows = rows
                .iter()
                .map(|r| {
                    let o = r.as_object().ok_or("a table's data is a list of objects")?;
                    Ok(columns
                        .iter()
                        .map(|c| o.get(c).map(text_of).unwrap_or_default())
                        .collect())
                })
                .collect::<Result<Vec<Vec<String>>, &str>>()?;
            Ok(ViewData::Table { columns, rows })
        }
        ViewWidget::Metrics => {
            let o = picked
                .as_object()
                .ok_or("metrics are an object of names and values")?;
            if o.len() > MAX_METRICS {
                return Err(format!(
                    "{} metrics; at most {MAX_METRICS} are shown",
                    o.len()
                ));
            }
            let metrics = o
                .iter()
                .map(|(k, v)| match v {
                    Value::Number(_) | Value::Bool(_) => Ok((k.clone(), v.to_string())),
                    Value::String(s) if s.chars().count() <= MAX_METRIC_TEXT => {
                        Ok((k.clone(), s.clone()))
                    }
                    _ => Err(format!(
                        "metric `{k}` is not a number or a word of at most {MAX_METRIC_TEXT} characters"
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ViewData::Metrics { metrics })
        }
        ViewWidget::Findings | ViewWidget::Markdown => unreachable!("handled above"),
    }
}
