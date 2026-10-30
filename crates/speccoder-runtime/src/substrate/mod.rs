//! The execution substrate: agentplane, embedded.
//!
//! This module is the only place speccoder touches agentplane. A [`Plane`]
//! is a redb journal in speccoder's state directory, the speccoder agent
//! declared in Rust, and the model driver. Every run is journaled; every
//! recorded run can be replayed strictly, without the model.

pub mod acceptance;
pub mod attempt;
pub mod converge;
pub mod host;
pub mod model_config;
pub mod monitor;
pub mod near_miss;
pub mod propose;
pub mod provenance;
pub mod run_config;
pub mod shape;

use std::path::Path;
use std::sync::Arc;

use agentplane::core::CorrelationKey;
use agentplane::core::{ProtectedField, SourceId};
use agentplane::manifest::{Budgets, Manifest, ModelRef, Models, ToolGrant};
use agentplane::model::ModelProvider;
use agentplane::prelude::*;
use agentplane::runtime::{Agent, RunOutcome, RunTerms};
use serde_json::Value;

use self::model_config::ModelConfig;
use self::propose::{CAPABILITY, Propose};

/// The agentplane release speccoder is built against (the exact pin in the
/// workspace `Cargo.toml`), named in every verification summary.
pub const AGENTPLANE_VERSION: &str = "0.56.1";

/// The journal file inside a project's state directory.
pub const JOURNAL: &str = "journal.redb";

/// A failure to open or use the plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaneError(pub String);

/// The speccoder agent, declared in code (Manifest::builder runs the same
/// validation as a parsed manifest).
pub fn manifest(model: &ModelConfig) -> Result<Manifest, PlaneError> {
    manifest_with(model, &[])
}

/// [`manifest`] with grants for the run's MCP tools: mutating
/// (speccoder cannot know otherwise), every argument field the model's.
pub fn manifest_with(
    model: &ModelConfig,
    mcp: &[crate::mcp::McpTool],
) -> Result<Manifest, PlaneError> {
    Manifest::builder("speccoder", env!("CARGO_PKG_VERSION"))
        .configure(|spec| {
            spec.capabilities.provides.push(CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(attempt::CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(acceptance::CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(converge::CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(monitor::CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(near_miss::CAPABILITY.to_owned());
            spec.capabilities
                .provides
                .push(shape::CAPABILITY.to_owned());
            spec.capabilities.provides.push(host::CAPABILITY.to_owned());
            spec.models = Some(Models {
                privileged: Some(ModelRef {
                    max_tokens: Some(model.max_output_tokens),
                    ..ModelRef::new(model.provider.clone(), model.model.clone())
                }),
                ..Models::default()
            });
            // Task context is the project's own material: internal, never public.
            spec.security.max_sensitivity_egress = Some(Sensitivity::Internal);
            spec.budgets = Some(Budgets {
                max_tokens: Some(model.max_run_tokens),
                ..Budgets::default()
            });
            // Mutating grants name the fields the model authors and the
            // sources that may shape them: the model, and the
            // material it legitimately reads — speccoder's own tools, the
            // verifier's facts, the worktree, the project's MCP tools.
            // Anything else (a peer, an unconfigured tool) is refused; what a
            // command or a write may *be* is speccoder's policy's decision.
            let author =
                agentplane::model::ModelId::new(model.provider.clone(), model.model.clone())
                    .source();
            let catalog = crate::tools::catalog();
            let mut sources: Vec<SourceId> = vec![
                author,
                SourceId::new(provenance::VERIFIER),
                SourceId::new(provenance::WORKTREE),
                // What a person answered may shape what the model writes.
                SourceId::new(provenance::PERSON),
            ];
            sources.extend(catalog.iter().map(|t| provenance::tool(t.server, t.name)));
            sources.extend(mcp.iter().map(|t| provenance::tool(&t.server, &t.tool)));
            // speccoder's own read-only steps (`fs/resolve`) are granted
            // too; the model is never offered them.
            for t in catalog.iter().chain(crate::tools::internal().iter()) {
                let mut grant = ToolGrant {
                    description: Some(t.description.to_owned()),
                    max_sensitivity: Some(Sensitivity::Internal),
                    mutates: t.mutates,
                    ..ToolGrant::new(agentplane::tools::ToolId::new(t.server, t.name).reference())
                };
                grant.protected_fields = t
                    .model_fields
                    .iter()
                    .map(|f| ProtectedField::from_sources(*f, sources.clone()))
                    .collect();
                spec.tools.push(grant);
            }
            for t in mcp {
                let mut grant = ToolGrant {
                    description: Some(t.description.clone()),
                    max_sensitivity: Some(Sensitivity::Internal),
                    mutates: true,
                    ..ToolGrant::new(agentplane::tools::ToolId::new(&t.server, &t.tool).reference())
                };
                grant.protected_fields = if t.fields.is_empty() {
                    // No arguments: the call itself is the act, and the
                    // only value it may carry is nothing.
                    vec![ProtectedField::one_of("", [serde_json::json!({})])]
                } else {
                    t.fields
                        .iter()
                        .map(|f| ProtectedField::from_sources(f.as_str(), sources.clone()))
                        .collect()
                };
                spec.tools.push(grant);
            }
        })
        .build()
        .map_err(|e| PlaneError(format!("speccoder manifest: {e}")))
}

/// What a live run hands its skills besides the model: someone to ask
/// and somewhere to report progress. Neither is journaled;
/// replay gets neither.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    /// Receives live progress.
    pub progress: Option<Arc<dyn crate::progress::Progress>>,
    /// Running MCP servers by name.
    pub mcp: Vec<(String, Arc<dyn agentplane::tools::ToolClient>)>,
    /// Their tools, granted in the manifest.
    pub mcp_tools: Vec<crate::mcp::McpTool>,
    /// The project's spec format; the reference format when unset.
    pub format: Option<&'static dyn crate::format::Adapter>,
    /// The sandbox as a person asked about a command is told.
    pub present: crate::approvals::PresentContext,
}

/// Calls the journal records, and what of it could not be read.
#[derive(Debug, Clone, Default)]
pub struct RecordedCalls {
    /// The calls.
    pub calls: Vec<RecordedCall>,
    /// What could not be read, and why.
    pub unreadable: Vec<String>,
}

/// One recorded call.
#[derive(Debug, Clone)]
pub struct RecordedCall {
    /// Its arguments as dispatched.
    pub arguments: Value,
    /// When its run last wrote (seconds since the epoch).
    pub at: u64,
}

/// What [`Plane::decide_inline`] leaves.
#[derive(Debug)]
pub struct Inline {
    /// The run as it rests.
    pub outcome: RunOutcome,
    /// The call it still waits on.
    pub waiting: Option<agentplane::core::Task>,
    /// Calls answered Always Allow.
    pub always: Vec<Always>,
}

/// A call answered Always Allow inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Always {
    /// `feature/task`.
    pub task: String,
    /// The waiting call's id.
    pub call: String,
    /// The interface it was answered through.
    pub via: speccoder_core::actions::Via,
}

/// An open plane.
pub struct Plane {
    runtime: Arc<Runtime>,
    /// Where an unreadable journal was moved before a fresh one was opened.
    pub replaced_journal: Option<std::path::PathBuf>,
    /// The run's claim on its journal in this process, released on drop.
    _run: Option<RunClaim>,
}

/// The journals a run holds in this process. Reads share a journal's
/// handle; two runs on one journal would race, so a second is refused.
static RUNS: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

struct RunClaim(std::path::PathBuf);

impl RunClaim {
    fn take(dir: &Path) -> Result<Self, PlaneError> {
        let path = dir.join(JOURNAL);
        let mut runs = RUNS
            .lock()
            .map_err(|_| PlaneError("the run registry is poisoned".to_owned()))?;
        if runs.contains(&path) {
            return Err(PlaneError(format!(
                "another speccoder is working on this project (a run in this process holds its journal in {})",
                dir.display()
            )));
        }
        runs.push(path.clone());
        Ok(Self(path))
    }
}

impl Drop for RunClaim {
    fn drop(&mut self) {
        if let Ok(mut runs) = RUNS.lock() {
            runs.retain(|p| *p != self.0);
        }
    }
}

/// The journals this process writes, shared: a journal has one writer
/// (redb refuses a second even in this process), so a decision the app
/// records while its own job runs goes through the job's handle.
static OPEN: std::sync::Mutex<Vec<(std::path::PathBuf, std::sync::Weak<RedbStore>)>> =
    std::sync::Mutex::new(Vec::new());

/// Opens the journal to write, sharing the handle this process already
/// writes through, and sharing the file with readers in other processes
/// (`open_shared`). A journal this build cannot read — another format
/// after an upgrade, or damage — is moved aside and a fresh one opened:
/// durable outcomes live in git, the journal is replaceable. A journal
/// another process writes is refused at once.
fn open_or_replace(dir: &Path) -> Result<(Arc<RedbStore>, Option<std::path::PathBuf>), PlaneError> {
    std::fs::create_dir_all(dir).map_err(|e| PlaneError(format!("{}: {e}", dir.display())))?;
    let path = dir.join(JOURNAL);
    let mut open = OPEN
        .lock()
        .map_err(|_| PlaneError("the journal registry is poisoned".to_owned()))?;
    open.retain(|(_, w)| w.strong_count() > 0);
    if let Some(store) = open
        .iter()
        .find(|(p, _)| *p == path)
        .and_then(|(_, w)| w.upgrade())
    {
        return Ok((store, None));
    }
    let (store, aside) = open_fresh(dir, &path)?;
    open.push((path, Arc::downgrade(&store)));
    Ok((store, aside))
}

fn open_fresh(
    dir: &Path,
    path: &Path,
) -> Result<(Arc<RedbStore>, Option<std::path::PathBuf>), PlaneError> {
    match RedbStore::open_shared(path) {
        Ok(store) => Ok((Arc::new(store), None)),
        // Another speccoder writes it: its run's record is live. Touch nothing.
        Err(agentplane::core::StoreError::InUse(e)) => Err(PlaneError(format!(
            "another speccoder is working on this project (its journal in {} is in use): {e}",
            dir.display()
        ))),
        // Written by a build that cannot be read here, or damaged: moved
        // aside (progress lives in git) and started afresh.
        Err(agentplane::core::StoreError::Unreadable(e)) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let aside = dir.join(format!("journal.{stamp}.unreadable.redb"));
            std::fs::rename(path, &aside).map_err(|r| {
                PlaneError(format!(
                    "the journal is unreadable ({e}) and cannot be moved aside: {r}"
                ))
            })?;
            RedbStore::open_shared(path)
                .map(|store| (Arc::new(store), Some(aside)))
                .map_err(|e| PlaneError(format!("cannot open a fresh journal: {e}")))
        }
        // Anything else is not the journal's fault: leave it alone.
        Err(e) => Err(PlaneError(format!(
            "cannot open the journal in {}: {e}",
            dir.display()
        ))),
    }
}

/// Opens the journal to read beside its writer, in this process or
/// another: `None` when there is none (a read never creates one). Never
/// registered, moved aside or written — any write through it is refused.
///
/// # Errors
///
/// It cannot be read: a writer that holds it alone (an earlier speccoder
/// build), damage, another format.
pub fn open_reader(dir: &Path) -> Result<Option<Arc<RedbStore>>, PlaneError> {
    let path = dir.join(JOURNAL);
    if !path.exists() {
        return Ok(None);
    }
    match RedbStore::open_read_only(&path) {
        Ok(store) => Ok(Some(Arc::new(store))),
        Err(agentplane::core::StoreError::InUse(e)) => Err(PlaneError(format!(
            "another process holds the journal in {} alone (a speccoder build that does not share it): {e}",
            dir.display()
        ))),
        // A writer that died without closing it (killed, Ctrl-C, a closed
        // terminal) leaves it to be repaired, which only a writer does.
        Err(agentplane::core::StoreError::NeedsRepair(e)) => Err(PlaneError(format!(
            "the last run ended without closing its journal in {}; the next run or decision repairs it ({e})",
            dir.display()
        ))),
        Err(e) => Err(PlaneError(format!(
            "cannot read the journal in {}: {e}",
            dir.display()
        ))),
    }
}

/// Whether `value` holds a decision as agentplane 0.53 and earlier
/// recorded it: `approved` and `decided`, and no `verdict`.
fn holds_a_pre_054_decision(value: &Value) -> bool {
    match value {
        Value::Object(o) => {
            (o.contains_key("approved") && o.contains_key("decided") && !o.contains_key("verdict"))
                || o.values().any(holds_a_pre_054_decision)
        }
        Value::Array(a) => a.iter().any(holds_a_pre_054_decision),
        Value::String(s) if s.contains("\"approved\"") => {
            serde_json::from_str::<Value>(s).is_ok_and(|v| holds_a_pre_054_decision(&v))
        }
        _ => false,
    }
}

/// What a run that failed on a decision speccoder 0.2.0 recorded is told.
pub const PRE_054_DECISION: &str = "this attempt waited on a decision recorded by speccoder 0.2.0, which this build cannot read (agentplane 0.54 records a decision as a verdict); the attempt is set aside — run the task again to start it afresh";

/// What a model provider refused, from a run's failure: the text after
/// agentplane's "refused the request:" — a refusal, which no retry changes.
#[must_use]
pub fn provider_refusal(failure: &str) -> Option<String> {
    let start = failure.find("refused the request: ")? + "refused the request: ".len();
    let rest = &failure[start..];
    let end = rest
        .find("); ")
        .or_else(|| rest.rfind(')'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches(['"', ')']).trim().to_owned())
}

/// A plane budget that stopped an attempt, in words, with the setting
/// that raises it.
#[must_use]
pub fn exhausted(limit: &agentplane::core::BudgetExceeded) -> String {
    let said = match limit {
        agentplane::core::BudgetExceeded::Recorded { limit, .. } => limit.clone(),
        other => other.to_string(),
    };
    let raise = if said.starts_with("token") {
        "; raise `[model] max_run_tokens`, or split the task"
    } else {
        ""
    };
    format!("the attempt's budget ran out: {said}{raise}")
}

/// Why a run did not succeed, for a person: a failure in its own words,
/// without agentplane's effect key and attempt count (`'openai/gpt-5.4-mini'
/// returned an unusable answer: …`), cut at 300 characters; else the
/// status (`suspended`, `exhausted`, …).
#[must_use]
pub fn why_not(status: &RunStatus) -> String {
    let RunStatus::Failed(failure) = status else {
        return status.as_str().to_owned();
    };
    let said = ["consumed resources and failed: ", "failed: "]
        .iter()
        .find_map(|m| failure.find(m).map(|i| &failure[i + m.len()..]))
        .or_else(|| {
            failure
                .split_once(" of ")
                .and_then(|(_, r)| r.split_once(": "))
                .map(|(_, r)| r)
        })
        .unwrap_or(failure);
    let cut: String = said.chars().take(300).collect();
    if cut.len() < said.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

/// The first run in the journal at `dir` that ended on a provider refusing
/// the **account** — an exhausted quota or credit, billing, a missing or
/// invalid key, a permission the key lacks — rather than one request:
/// every later call would be refused too. Read from the records (the
/// effect the run failed on and the provider's code), read-only; a
/// refusal without a code is not taken for one. `None` with no journal.
///
/// # Errors
///
/// The journal cannot be read.
pub async fn refused_account(
    dir: &Path,
) -> Result<Option<agentplane::journal::FailedEffect>, PlaneError> {
    use agentplane::journal::JournalStore as _;
    let Some(store) = open_reader(dir)? else {
        return Ok(None);
    };
    let mut after: Option<(u64, agentplane::core::RunId)> = None;
    loop {
        let page = store
            .recent_runs(after, 500)
            .await
            .map_err(|e| PlaneError(format!("listing the journal's runs: {e}")))?;
        let Some(last) = page.last().cloned() else {
            return Ok(None);
        };
        for (run, _) in &page {
            let records = store
                .read(*run, 0)
                .await
                .map_err(|e| PlaneError(format!("run {run}: {e}")))?;
            if let Some(failed) = agentplane::journal::failed_effect(&records)
                .filter(agentplane::journal::FailedEffect::refuses_the_account)
            {
                return Ok(Some(failed));
            }
        }
        after = Some((last.1, last.0));
    }
}

/// The runtime every plane shares: the journal, the agent with speccoder's
/// skills under its manifest, and the lease. What answers model and tool
/// calls is registered by the caller — live drivers or replay-only ones.
fn builder(
    store: Arc<RedbStore>,
    model: &ModelConfig,
    hooks: Hooks,
) -> Result<(agentplane::runtime::RuntimeBuilder, Manifest), PlaneError> {
    let manifest = manifest_with(model, &hooks.mcp_tools)?;
    let format = hooks.format.unwrap_or_else(crate::format::reference);
    let agent = Agent::new(&manifest)
        .skill(Propose {
            cache_ttl: model.cache_ttl,
            style: model.prompt_style(),
            format,
        })
        .skill(attempt::Attempt {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            progress: hooks.progress.clone(),
            format,
            present: hooks.present.clone(),
        })
        .skill(host::Host {
            style: model.prompt_style(),
            progress: hooks.progress,
            format,
            present: hooks.present,
        })
        .skill(acceptance::Acceptance {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            format,
        })
        .skill(converge::Converge {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            format,
        })
        .skill(monitor::Monitor {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            format,
        })
        .skill(near_miss::NearMiss {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            format,
        })
        .skill(shape::Shape {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
        });
    // One process owns the journal (redb locks it): opening it frees the
    // leases a dead process left, so the next run resumes at once.
    let builder = Runtime::builder_on(store).agent(agent);
    Ok((builder, manifest))
}

fn build(
    store: Arc<RedbStore>,
    model: &ModelConfig,
    provider: Arc<dyn ModelProvider>,
    tools: Option<Arc<crate::tools::WorkspaceTools>>,
    hooks: Hooks,
) -> Result<Arc<Runtime>, PlaneError> {
    let mcp = hooks.mcp.clone();
    let (mut builder, _) = builder(store, model, hooks)?;
    builder = builder.provider(model.provider.clone(), provider);
    // The tool servers answer only when a worktree is bound; without one, a
    // call to a granted tool is refused.
    let client: Arc<dyn agentplane::tools::ToolClient> = match tools {
        Some(t) => t,
        None => Arc::new(Unbound),
    };
    for server in ["fs", "proc", "git"] {
        builder = builder.tool_server(server, Arc::clone(&client));
    }
    for (server, client) in &mcp {
        builder = builder.tool_server(server.as_str(), Arc::clone(client));
    }
    builder
        .try_build()
        .map_err(|e| PlaneError(format!("building the plane: {e}")))
}

impl Plane {
    /// Opens (or creates) the journal at `dir/journal.redb` with `provider`
    /// registered under the configured provider name.
    pub fn open(
        dir: &Path,
        model: &ModelConfig,
        provider: Arc<dyn ModelProvider>,
    ) -> Result<Self, PlaneError> {
        Self::open_with_tools(dir, model, provider, None)
    }

    /// [`Plane::open`] with speccoder's tools bound to a worktree.
    pub fn open_with_tools(
        dir: &Path,
        model: &ModelConfig,
        provider: Arc<dyn ModelProvider>,
        tools: Option<Arc<crate::tools::WorkspaceTools>>,
    ) -> Result<Self, PlaneError> {
        Self::open_with(dir, model, provider, tools, Hooks::default())
    }

    /// [`Plane::open_with_tools`] with someone to ask inline.
    pub fn open_with(
        dir: &Path,
        model: &ModelConfig,
        provider: Arc<dyn ModelProvider>,
        tools: Option<Arc<crate::tools::WorkspaceTools>>,
        hooks: Hooks,
    ) -> Result<Self, PlaneError> {
        let claim = RunClaim::take(dir)?;
        let (store, replaced_journal) = open_or_replace(dir)?;
        Ok(Self {
            runtime: build(store, model, provider, tools, hooks)?,
            replaced_journal,
            _run: Some(claim),
        })
    }

    /// The journal with no skills and no model: enough to record a person's
    /// decision on a waiting call (`speccoder task approve`), which the attempt's
    /// own run picks up when it resumes (agentplane's `Delivery::Buffered`).
    pub fn open_for_decisions(dir: &Path) -> Result<Self, PlaneError> {
        let (store, replaced_journal) = open_or_replace(dir)?;
        let runtime = Runtime::builder_on(store)
            .try_build()
            .map_err(|e| PlaneError(format!("building the plane: {e}")))?;
        Ok(Self {
            runtime,
            replaced_journal,
            _run: None,
        })
    }

    /// The journal read beside its writer, with no skills and no model:
    /// the waiting calls and recorded calls as they stand. `None` when the
    /// project has no journal. Nothing is written — not even an expired
    /// call's denial, which the next writer records.
    ///
    /// # Errors
    ///
    /// The journal cannot be read ([`open_reader`]).
    pub fn open_for_reading(dir: &Path) -> Result<Option<Self>, PlaneError> {
        let Some(store) = open_reader(dir)? else {
            return Ok(None);
        };
        let runtime = Runtime::builder_on(store)
            .try_build()
            .map_err(|e| PlaneError(format!("building the plane: {e}")))?;
        Ok(Some(Self {
            runtime,
            replaced_journal: None,
            _run: None,
        }))
    }

    /// Settles every waiting call whose deadline has passed at `now`: the
    /// call is denied as expired, nobody's decision. Only on a plane without
    /// skills ([`Plane::open_for_decisions`]) — the attempt resumes in its own
    /// run, with its own worktree, the next time it runs.
    pub async fn expire_due(&self, now: agentplane::core::Timestamp) -> Result<(), PlaneError> {
        self.runtime
            .sweep(now, std::time::Duration::from_secs(7 * 86_400))
            .await
            .map(|_| ())
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Withdraws a waiting call whose attempt was set aside: its run is
    /// cancelled on the record (who, why) and the call leaves the queue.
    pub async fn withdraw(
        &self,
        task: &agentplane::core::Task,
        by: &agentplane::core::Operator,
        reason: &str,
    ) -> Result<(), PlaneError> {
        let err = |e: String| PlaneError(e);
        let _ = self
            .runtime
            .request_cancel(task.run, by, reason)
            .await
            .map_err(|e| err(e.to_string()))?;
        if let Some(tasks) = self.runtime.tasks() {
            tasks
                .withdraw_run(task.run, &[task.id])
                .await
                .map_err(|e| err(e.to_string()))?;
        }
        Ok(())
    }

    /// The person's task a suspended run waits on, if it waits on one.
    pub async fn waiting_on(
        &self,
        outcome: &RunOutcome,
    ) -> Result<Option<agentplane::core::Task>, PlaneError> {
        let RunStatus::Suspended(reason) = &outcome.status else {
            return Ok(None);
        };
        let Some(id) = reason.awaited_task() else {
            return Ok(None);
        };
        self.task(id).await
    }

    /// One task row.
    pub async fn task(
        &self,
        id: agentplane::core::TaskId,
    ) -> Result<Option<agentplane::core::Task>, PlaneError> {
        let Some(tasks) = self.runtime.tasks() else {
            return Ok(None);
        };
        tasks.task(id).await.map_err(|e| PlaneError(e.to_string()))
    }

    /// Every call of `server/tool` the journal records as dispatched, in
    /// every run, with the time its run last wrote and whether it was
    /// decided on an object (a person's decision or grant) — and what could
    /// not be read: a run unreadable is reported, never skipped silently.
    pub async fn tool_calls(&self, server: &str, tool: &str) -> Result<RecordedCalls, PlaneError> {
        let journal = self.runtime.journal();
        let mut out = RecordedCalls::default();
        let mut after: Option<(u64, agentplane::core::RunId)> = None;
        loop {
            let page = journal
                .recent_runs(after, 500)
                .await
                .map_err(|e| PlaneError(format!("listing the journal's runs: {e}")))?;
            let Some(last) = page.last().cloned() else {
                break;
            };
            for (run, at) in &page {
                let records = match journal.read(*run, 0).await {
                    Ok(r) => r,
                    Err(e) => {
                        out.unreadable.push(format!("run {run}: {e}"));
                        continue;
                    }
                };
                let mut seen = std::collections::BTreeSet::new();
                for record in records {
                    let agentplane::RecordKind::EffectStarted { descriptor, .. } = record.kind()
                    else {
                        continue;
                    };
                    let args = &descriptor.args;
                    if descriptor.kind != "tool.call"
                        || args["server"] != server
                        || args["tool"] != tool
                    {
                        continue;
                    }
                    // A retried effect is one call.
                    let key = record
                        .body
                        .effect_key
                        .as_ref()
                        .map_or_else(|| record.body.seq.to_string(), |k| format!("{k:?}"));
                    if seen.insert(key) {
                        out.calls.push(RecordedCall {
                            arguments: args["arguments"].clone(),
                            at: *at,
                        });
                    }
                }
            }
            after = Some((last.1, last.0));
        }
        Ok(out)
    }

    /// Whether `run` read a person's decision recorded by speccoder 0.2.0
    /// (agentplane 0.53 and earlier): a decision with `approved`, which
    /// agentplane 0.54 replaced by a verdict, so the run cannot read it
    /// and fails on it. Told from the record's shape, never a message.
    pub async fn holds_a_pre_054_decision(&self, run: agentplane::core::RunId) -> bool {
        let Ok(records) = self.runtime.journal().read(run, 0).await else {
            return false;
        };
        records.iter().any(|r| {
            serde_json::to_value(r.kind())
                .ok()
                .is_some_and(|v| holds_a_pre_054_decision(&v))
        })
    }

    /// Every task waiting for a person, oldest first.
    pub async fn open_tasks(&self) -> Result<Vec<agentplane::core::Task>, PlaneError> {
        let Some(tasks) = self.runtime.tasks() else {
            return Ok(Vec::new());
        };
        let mut open = tasks
            .queue(&[], 1000)
            .await
            .map_err(|e| PlaneError(e.to_string()))?;
        open.sort_by_key(|t| t.created_at);
        Ok(open)
    }

    /// Records a person's decision on `task`: the run continues at once
    /// when this plane holds its skill, else when it next resumes.
    pub async fn decide(
        &self,
        task: agentplane::core::TaskId,
        decision: &agentplane::core::Decision,
    ) -> Result<agentplane::core::Delivery, PlaneError> {
        self.decide_at(task, decision, None).await
    }

    /// [`Plane::decide`], refused (nothing recorded) unless the task row
    /// is still the one whose justification digest is `expected`.
    pub async fn decide_at(
        &self,
        task: agentplane::core::TaskId,
        decision: &agentplane::core::Decision,
        expected: Option<agentplane::core::Digest>,
    ) -> Result<agentplane::core::Delivery, PlaneError> {
        self.runtime
            .decide_task_at(task, decision, &[], expected)
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Asks `approver` about each call the run waits on, records the
    /// decision and lets the run continue, until it no longer waits on a
    /// person. Returns the run as it rests, the call still waiting, if
    /// the person answered "later", nobody is there or the call waits in
    /// its run's batch, and the calls answered Always Allow (approved once
    /// here; their argv is added to the allow list by
    /// `actions::perform(AllowAlways)`, which the caller runs).
    pub async fn decide_inline(
        &self,
        mut outcome: RunOutcome,
        approver: Option<&dyn crate::approvals::Approver>,
        context: &crate::approvals::PresentContext,
    ) -> Result<Inline, PlaneError> {
        let mut always = Vec::new();
        while let Some(task) = self.waiting_on(&outcome).await? {
            let Some((decision, answer)) =
                approver.and_then(|a| crate::approvals::tasks::ask(a, &task, context))
            else {
                return Ok(Inline {
                    outcome,
                    waiting: Some(task),
                    always,
                });
            };
            self.decide(task.id, &decision).await?;
            if answer.always && decision.approved() {
                let via = answer
                    .by
                    .as_ref()
                    .and_then(|b| speccoder_core::actions::Via::read(&b.via))
                    .map_or(speccoder_core::actions::Via::Terminal, |(v, _)| v);
                crate::approvals::answered_always(&task.id.to_hex());
                always.push(Always {
                    task: crate::approvals::tasks::request_of(&task.justification.proposed_action)
                        .map(|r| r.task)
                        .unwrap_or_default(),
                    call: task.id.to_hex(),
                    via,
                });
            }
            match self.outcome(outcome.run_id).await? {
                Some(o) => outcome = o,
                None => break,
            }
        }
        Ok(Inline {
            outcome,
            waiting: None,
            always,
        })
    }

    /// The run as it rests now, its output included: the journal records
    /// what happened, not a step's value, so the value is re-derived by
    /// strict replay, which performs nothing.
    pub async fn outcome(
        &self,
        run: agentplane::core::RunId,
    ) -> Result<Option<RunOutcome>, PlaneError> {
        if self
            .runtime
            .recorded_outcome(run)
            .await
            .map_err(|e| PlaneError(e.to_string()))?
            .is_none()
        {
            return Ok(None);
        }
        self.runtime
            .replay(run, agentplane::runtime::Mode::Strict)
            .await
            .map(Some)
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// A plane over `store` instead of the journal file: a crash point's
    /// prefix of a recorded run, resumed in a test.
    #[doc(hidden)]
    pub fn on_store(
        store: Arc<RedbStore>,
        model: &ModelConfig,
        provider: Arc<dyn ModelProvider>,
        tools: Option<Arc<crate::tools::WorkspaceTools>>,
    ) -> Result<Self, PlaneError> {
        Ok(Self {
            runtime: build(store, model, provider, tools, Hooks::default())?,
            replaced_journal: None,
            _run: None,
        })
    }

    /// Resumes `run` as a restarted process would, agentplane's own answer
    /// unchanged (for `testkit::assert_resumed_honestly`).
    #[doc(hidden)]
    pub async fn resume(
        &self,
        run: agentplane::core::RunId,
    ) -> Result<RunOutcome, agentplane::RuntimeError> {
        self.runtime
            .replay(run, agentplane::runtime::Mode::Resume)
            .await
    }

    /// Runs `speccoder.feature.converge`, correlated to the feature.
    pub async fn converge(
        &self,
        feature: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(converge::CAPABILITY, input)
            .terms(RunTerms::default().correlated(
                "feature",
                &[CorrelationKey::new("feature", feature.to_owned())],
            ))
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.shape`: a person's words made into the field's kind
    /// of text.
    pub async fn shape(&self, input: Tainted<Value>) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(shape::CAPABILITY, input)
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.skill.run` for an installed skill.
    pub async fn skill(&self, name: &str, input: Tainted<Value>) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(host::CAPABILITY, input)
            .terms(
                RunTerms::default()
                    .correlated("skill", &[CorrelationKey::new("skill", name.to_owned())]),
            )
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.task.monitor`, correlated to the task, admitted once
    /// per input: judging the same change again reads the recorded answer
    /// back instead of asking again.
    pub async fn monitor(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.judge_once(monitor::CAPABILITY, feature, task, input)
            .await
    }

    /// Runs `speccoder.task.near_miss`, admitted once per input like
    /// [`monitor`](Self::monitor): a held attempt judged again after a
    /// person's ruling meets the same variants, not new ones.
    pub async fn near_miss(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.judge_once(near_miss::CAPABILITY, feature, task, input)
            .await
    }

    /// A judge-side call admitted once per task and input: the first time it
    /// runs; afterwards the recorded run is re-derived by strict replay,
    /// which performs nothing (an interrupted one resumes).
    async fn judge_once(
        &self,
        capability: &str,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        use agentplane::runtime::{Admission, Mode};
        use sha2::Digest as _;
        let err = |e: agentplane::RuntimeError| PlaneError(e.to_string());
        let digest = sha2::Sha256::digest(
            format!("{capability}\n{feature}/{task}\n{}", input.peek()).as_bytes(),
        );
        let key: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
        let key = format!("judge-{key}");
        let key = key.as_str();
        let terms = || {
            RunTerms::default().correlated(
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            )
        };
        let admitted = self
            .runtime
            .start(capability, input.clone())
            .terms(terms())
            .once(key)
            .run()
            .await
            .map_err(err)?;
        match admitted {
            Admission::Fresh(o) => Ok(o),
            // A call that failed (a provider error) is asked again, unkeyed:
            // only an answer is worth reading back.
            Admission::Replayed(o) if !matches!(o.status, RunStatus::Succeeded) => self
                .runtime
                .start(capability, input)
                .terms(terms())
                .run()
                .await
                .map_err(err),
            Admission::Replayed(o) => self
                .runtime
                .replay(o.run_id, Mode::Strict)
                .await
                .map_err(err),
            Admission::InFlight(run) => self.runtime.replay(run, Mode::Resume).await.map_err(err),
        }
    }

    /// Runs `speccoder.task.acceptance`: acceptance tests derived from the spec.
    pub async fn acceptance(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(acceptance::CAPABILITY, input)
            .terms(RunTerms::default().correlated(
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            ))
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs one `speccoder.task.attempt` for a bundle, correlated to the task.
    pub async fn attempt(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(attempt::CAPABILITY, input)
            .terms(RunTerms::default().correlated(
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            ))
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// [`attempt`](Self::attempt) admitted under `key`: the first time it
    /// runs; after a crash the same key finds the interrupted run — a run
    /// that had rested is re-derived by strict replay, which performs
    /// nothing, and one still in flight is resumed from its journal (a dead
    /// process's lease is freed when the store opens), so finished model and
    /// tool calls are read back, not repeated. The `bool` says whether this
    /// was such a resume: what the run's notice may claim.
    pub async fn attempt_keyed(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
        key: &str,
    ) -> Result<(RunOutcome, bool), PlaneError> {
        use agentplane::runtime::{Admission, Mode};
        let err = |e: agentplane::RuntimeError| PlaneError(e.to_string());
        let admitted = self
            .runtime
            .start(attempt::CAPABILITY, input)
            .terms(RunTerms::default().correlated(
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            ))
            .once(key)
            .run()
            .await
            .map_err(err)?;
        match admitted {
            Admission::Fresh(o) => Ok((o, false)),
            // A run waiting for a person resumes: a decision recorded since
            // is delivered and the attempt continues; one still undecided
            // waits again. Any other rested run is re-derived, performing
            // nothing.
            Admission::Replayed(o) if matches!(o.status, RunStatus::Suspended(_)) => self
                .runtime
                .replay(o.run_id, Mode::Resume)
                .await
                .map(|o| (o, true))
                .map_err(err),
            Admission::Replayed(o) => self
                .runtime
                .replay(o.run_id, Mode::Strict)
                .await
                .map(|o| (o, true))
                .map_err(err),
            Admission::InFlight(run) => self
                .runtime
                .replay(run, Mode::Resume)
                .await
                .map(|o| (o, true))
                .map_err(err),
        }
    }

    /// Runs `speccoder.task.propose` for a bundle, correlated to the task.
    pub async fn propose(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(CAPABILITY, input)
            .terms(RunTerms::default().correlated(
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            ))
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }
}

/// The input `run` was admitted with, with its root and field labels — from
/// the journal's `RunAdmitted` record, never from anything a tool returned.
pub async fn admitted(
    dir: &Path,
    run: &str,
) -> Result<Option<agentplane::core::Tainted<Value>>, PlaneError> {
    let id = agentplane::core::RunId::parse(run)
        .map_err(|_| PlaneError(format!("`{run}` is not a run id")))?;
    let Some(store) = open_reader(dir)? else {
        return Ok(None);
    };
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    Ok(history
        .iter()
        .find_map(agentplane::journal::Record::admitted_input))
}

/// Strictly re-derives a recorded run and says whether this build
/// reproduces it: history is consumed, nothing is
/// dispatched. The model driver is agentplane's replay-only provider, which
/// reads the request profile back from the history and refuses every call,
/// so no credential is needed. A faithful replay of a run that failed is
/// `Finding::Verified` too — the recorded ending is what is verified.
/// `Ok(None)` when the run is not in the journal.
pub async fn replay_strict(
    dir: &Path,
    model: &ModelConfig,
    run: &str,
    format: &'static dyn crate::format::Adapter,
) -> Result<Option<agentplane::runtime::Verdict>, PlaneError> {
    let id = agentplane::core::RunId::parse(run)
        .map_err(|_| PlaneError(format!("`{run}` is not a run id")))?;
    let Some(store) = open_reader(dir)? else {
        return Ok(None);
    };
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    if history.is_empty() {
        return Ok(None);
    }
    // A decision speccoder 0.2.0 recorded does not read in this build:
    // named, never shown as a divergence or a decoding error.
    if history.iter().any(|r| {
        serde_json::to_value(r.kind())
            .ok()
            .is_some_and(|v| holds_a_pre_054_decision(&v))
    }) {
        return Err(PlaneError(format!(
            "run {run} read a person's decision recorded by speccoder 0.2.0, which this build cannot replay (agentplane 0.54 records a decision as a verdict); its ticks and attempt refs in git are unaffected"
        )));
    }
    // The run's MCP tools as its admitted input lists them: the same
    // manifest again. agentplane's replay-only drivers answer for every
    // provider and tool server it names — replay dispatches nothing.
    let mcp_tools: Vec<crate::mcp::McpTool> = history
        .iter()
        .find_map(agentplane::journal::Record::admitted_input)
        .and_then(|a| serde_json::from_value(a.peek()["settings"]["mcp"].clone()).ok())
        .unwrap_or_default();
    let hooks = Hooks {
        mcp_tools,
        format: Some(format),
        ..Hooks::default()
    };
    let (builder, manifest) = builder(store, model, hooks)?;
    let runtime = agentplane::runtime::replay_only::wire(builder, &[manifest], &history)
        .try_build()
        .map_err(|e| PlaneError(format!("building the replay plane: {e}")))?;
    runtime
        .verify(id)
        .await
        .map(Some)
        .map_err(|e| PlaneError(e.to_string()))
}

/// What the model said in a turn beside its answer — a claim, never
/// evidence: its commentary, where the provider marks it apart (OpenAI
/// Responses), then its text when the turn called tools (a turn without
/// calls answers with its text). `None` when it said nothing.
#[must_use]
pub fn said(completion: &agentplane::model::Completion) -> Option<String> {
    let commentary = completion.commentary.as_deref().map(str::trim);
    let text = (!completion.tool_calls.is_empty()).then(|| completion.text.trim());
    let parts: Vec<&str> = [commentary, text]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// One model turn of an attempt, as the person is shown it afterwards: the
/// model's summary of its reasoning and its words beside its calls — its
/// account, a claim. Control characters escaped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AccountTurn {
    /// The turn (1-based) within the attempt.
    pub turn: u32,
    /// Its reasoning summary, when one was asked for and given.
    pub reasoning: Option<String>,
    /// Its words beside its calls (a turn without calls is the final
    /// answer, reported as such).
    pub said: Option<String>,
}

/// The model's account of `run`, turn by turn: every turn that reasoned or
/// said something, in order.
///
/// # Errors
///
/// The journal cannot be read.
pub async fn account(dir: &Path, run: &str) -> Result<Vec<AccountTurn>, PlaneError> {
    let Ok(id) = agentplane::core::RunId::parse(run) else {
        return Ok(Vec::new());
    };
    let Some(store) = open_reader(dir)? else {
        return Ok(Vec::new());
    };
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    let text = |t: &str| {
        let t = t.trim();
        (!t.is_empty()).then(|| crate::approvals::present::escape(t))
    };
    Ok(agentplane::model::turns(&history)
        .into_iter()
        .zip(1u32..)
        .filter_map(|(t, turn)| {
            let c = t.completion?;
            let said = said(&c).as_deref().and_then(text);
            let reasoning = c.reasoning.as_deref().and_then(text);
            (said.is_some() || reasoning.is_some()).then_some(AccountTurn {
                turn,
                reasoning,
                said,
            })
        })
        .collect())
}

/// The tool servers when no worktree is bound: every call is refused.
#[derive(Debug)]
struct Unbound;

#[async_trait]
impl agentplane::tools::ToolClient for Unbound {
    async fn call(
        &self,
        tool: &agentplane::tools::ToolId,
        _arguments: &Value,
        _provenance: Option<&agentplane::core::Provenance>,
    ) -> Result<Value, agentplane::tools::ToolError> {
        Err(agentplane::tools::ToolError::Refused {
            tool: tool.clone(),
            detail: "no worktree is bound to this plane".to_owned(),
        })
    }

    fn destination(&self, _tool: &agentplane::tools::ToolId) -> agentplane::tools::Destination {
        agentplane::tools::Destination::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agentplane_version_named_in_summaries_is_the_pinned_one() {
        let workspace = include_str!("../../../../Cargo.toml");
        assert!(
            workspace.contains(&format!(
                "agentplane = {{ version = \"={AGENTPLANE_VERSION}\""
            )),
            "AGENTPLANE_VERSION is not the workspace's pin"
        );
    }

    #[test]
    fn a_provider_refusal_is_read_from_the_run_failure() {
        let failure = "Failed(\"effect ek:21 was refused on attempt 1, and the refusal is an answer rather than a fault (effect refused: 'openai/gpt-5.4-mini' refused the request: credit_balance_exhausted: You have no credits remaining. Add credits at https://platform.openai.com/settings/organization/billing/.); no retry would change it\")";
        let r = provider_refusal(failure).unwrap();
        assert_eq!(
            r,
            "credit_balance_exhausted: You have no credits remaining. Add credits at https://platform.openai.com/settings/organization/billing/."
        );
        assert_eq!(provider_refusal("Failed(\"timeout\")"), None);
    }

    #[test]
    fn a_failed_run_says_why_in_the_failure_s_own_words() {
        let unusable = RunStatus::Failed("effect ek:774b failed on attempt 1 of 1: effect consumed resources and failed: 'openai/gpt-5.4-mini' returned an unusable answer: a schema was required and the answer is not JSON: trailing characters at line 1 column 2531".to_owned());
        assert_eq!(
            why_not(&unusable),
            "'openai/gpt-5.4-mini' returned an unusable answer: a schema was required and the answer is not JSON: trailing characters at line 1 column 2531"
        );
        let timeout =
            RunStatus::Failed("effect ek:9 failed on attempt 2 of 2: timed out".to_owned());
        assert_eq!(why_not(&timeout), "timed out");
        assert_eq!(why_not(&RunStatus::Succeeded), "succeeded");
    }

    #[test]
    fn an_unreadable_journal_is_moved_aside_and_a_locked_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.path().join(JOURNAL), b"written by another format")
            .unwrap_or_else(|e| panic!("{e}"));
        let (store, aside) = open_or_replace(dir.path()).unwrap_or_else(|e| panic!("{}", e.0));
        let aside = aside.unwrap_or_else(|| panic!("moved aside"));
        assert!(aside.exists() && dir.path().join(JOURNAL).exists());
        // In this process a second opener shares the handle.
        let shared = open_or_replace(dir.path()).unwrap_or_else(|e| panic!("{}", e.0));
        assert!(Arc::ptr_eq(&store, &shared.0) && shared.1.is_none());
        drop((store, shared));
        // Written elsewhere (another process's lock, here a direct open):
        // refused at once, and nothing is moved.
        let held = RedbStore::open(dir.path().join(JOURNAL)).unwrap_or_else(|e| panic!("{e}"));
        let started = std::time::Instant::now();
        let second = open_or_replace(dir.path())
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(second.contains("another speccoder"), "{second}");
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        // A writer that holds the file alone cannot be read beside: said,
        // never read as empty.
        assert!(open_reader(dir.path()).is_err());
        drop(held);
        assert!(
            open_or_replace(dir.path())
                .unwrap_or_else(|e| panic!("{}", e.0))
                .1
                .is_none()
        );
    }

    #[test]
    fn a_reader_creates_nothing_and_reads_beside_this_process_s_writer() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        // No journal: nothing to read, and none is created.
        assert!(
            open_reader(dir.path())
                .unwrap_or_else(|e| panic!("{}", e.0))
                .is_none()
        );
        assert!(
            Plane::open_for_reading(dir.path())
                .unwrap_or_else(|e| panic!("{}", e.0))
                .is_none()
        );
        assert!(
            !dir.path().join(JOURNAL).exists(),
            "a read created a journal"
        );
        // Beside a writer in this process (the app reading while its job
        // runs): the reader opens; a second run here is still refused.
        let model =
            model_config::parse("[model]\nprovider = \"openai\"\nmodel = \"gpt-5.4-mini\"\n")
                .unwrap_or_else(|e| panic!("{e}"))
                .unwrap_or_else(|| panic!("no model"));
        let fake: Arc<dyn ModelProvider> = agentplane::model::fake::FakeProvider::new();
        let run = Plane::open(dir.path(), &model, Arc::clone(&fake))
            .unwrap_or_else(|e| panic!("{}", e.0));
        let reader = open_reader(dir.path())
            .unwrap_or_else(|e| panic!("{}", e.0))
            .unwrap_or_else(|| panic!("no journal"));
        assert!(reader.is_read_only());
        assert!(Plane::open(dir.path(), &model, fake).is_err());
        drop(run);
    }
}
