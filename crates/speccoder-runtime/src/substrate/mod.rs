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
pub const AGENTPLANE_VERSION: &str = "0.52.0";

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
            for t in &catalog {
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

/// An open plane.
pub struct Plane {
    runtime: Arc<Runtime>,
    /// Where an unreadable journal was moved before a fresh one was opened.
    pub replaced_journal: Option<std::path::PathBuf>,
}

fn open_store(dir: &Path) -> Result<Arc<RedbStore>, PlaneError> {
    open_or_replace(dir).map(|(store, _)| store)
}

/// Opens the journal. A journal this build cannot read — another format
/// after an upgrade, or damage — is moved aside and a fresh one opened:
/// durable outcomes live in git, the journal is replaceable
///. A journal locked by another speccoder is an error.
fn open_or_replace(dir: &Path) -> Result<(Arc<RedbStore>, Option<std::path::PathBuf>), PlaneError> {
    std::fs::create_dir_all(dir).map_err(|e| PlaneError(format!("{}: {e}", dir.display())))?;
    let path = dir.join(JOURNAL);
    match RedbStore::open(&path) {
        Ok(store) => Ok((Arc::new(store), None)),
        // Another speccoder holds it: its run's record is live. Touch nothing.
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
            std::fs::rename(&path, &aside).map_err(|r| {
                PlaneError(format!(
                    "the journal is unreadable ({e}) and cannot be moved aside: {r}"
                ))
            })?;
            RedbStore::open(&path)
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

/// Whether a provider refusal concerns the account (no credits, quota,
/// a key it does not accept) rather than one request: every later call
/// would be refused too.
#[must_use]
pub fn account_refusal(refusal: &str) -> bool {
    let r = refusal.to_lowercase();
    [
        "credit",
        "billing",
        "quota",
        "insufficient",
        "api key",
        "api_key",
        "authentication",
        "unauthorized",
    ]
    .iter()
    .any(|w| r.contains(w))
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
        let (store, replaced_journal) = open_or_replace(dir)?;
        Ok(Self {
            runtime: build(store, model, provider, tools, hooks)?,
            replaced_journal,
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
        })
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
        let RunStatus::Suspended(agentplane::core::SuspendReason::AwaitingEvent {
            correlation,
            ..
        }) = &outcome.status
        else {
            return Ok(None);
        };
        let Some(id) = correlation
            .iter()
            .find(|k| k.namespace == "task")
            .and_then(|k| agentplane::core::TaskId::parse(&k.value).ok())
        else {
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
        self.runtime
            .decide_task(task, decision, &[])
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Asks `approver` about each call the run waits on, records the
    /// decision and lets the run continue, until it no longer waits on a
    /// person. Returns the run as it rests and the call still waiting, if
    /// the person answered "later" or nobody is there.
    pub async fn decide_inline(
        &self,
        mut outcome: RunOutcome,
        approver: Option<&dyn crate::approvals::Approver>,
        context: &crate::approvals::PresentContext,
    ) -> Result<(RunOutcome, Option<agentplane::core::Task>), PlaneError> {
        while let Some(task) = self.waiting_on(&outcome).await? {
            let Some(decision) =
                approver.and_then(|a| crate::approvals::tasks::ask(a, &task, context))
            else {
                return Ok((outcome, Some(task)));
            };
            self.decide(task.id, &decision).await?;
            match self.outcome(outcome.run_id).await? {
                Some(o) => outcome = o,
                None => break,
            }
        }
        Ok((outcome, None))
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

    /// Runs `speccoder.task.near_miss` (F-060), admitted once per input like
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
    let store = open_store(dir)?;
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
    let store = open_store(dir)?;
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    if history.is_empty() {
        return Ok(None);
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

/// The effects speccoder recorded a person's answer with before F-072.
pub const EARLIER_ASKS: [&str; 2] = ["speccoder.approval.ask", "speccoder.person.answer"];

/// The earlier ask effect `run` recorded, if any: such a run asked a person
/// through a mechanism this build no longer has, so it cannot be
/// re-derived and replay says so rather than reporting a divergence.
pub async fn earlier_ask(dir: &Path, run: &str) -> Result<Option<String>, PlaneError> {
    let Ok(id) = agentplane::core::RunId::parse(run) else {
        return Ok(None);
    };
    let store = open_store(dir)?;
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    Ok(history.iter().find_map(|r| match &r.body.kind {
        agentplane::journal::RecordKind::EffectStarted { descriptor, .. }
            if EARLIER_ASKS.contains(&descriptor.kind.as_str()) =>
        {
            Some(descriptor.kind.clone())
        }
        _ => None,
    }))
}

/// What the model said in each turn of `run`, in order (F-095): the text
/// of every `model.complete` effect the journal holds — its account, a
/// claim. Empty turns are left out.
///
/// # Errors
///
/// The journal cannot be read.
pub async fn narration(dir: &Path, run: &str) -> Result<Vec<String>, PlaneError> {
    let Ok(id) = agentplane::core::RunId::parse(run) else {
        return Ok(Vec::new());
    };
    let store = open_store(dir)?;
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    let mut out = Vec::new();
    let mut model = false;
    for r in &history {
        match &r.body.kind {
            agentplane::journal::RecordKind::EffectStarted { descriptor, .. } => {
                model = descriptor.kind == "model.complete";
            }
            agentplane::journal::RecordKind::EffectDone { output, .. } if model => {
                model = false;
                if let Some(text) = output["text"]
                    .as_str()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    out.push(crate::approvals::present::escape(text));
                }
            }
            _ => {}
        }
    }
    Ok(out)
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
        assert!(account_refusal(&r));
        assert!(!account_refusal(
            "the continuation was not an Anthropic assistant-content array"
        ));
        assert_eq!(provider_refusal("Failed(\"timeout\")"), None);
    }

    #[test]
    fn an_unreadable_journal_is_moved_aside_and_a_locked_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.path().join(JOURNAL), b"written by another format")
            .unwrap_or_else(|e| panic!("{e}"));
        let (store, aside) = open_or_replace(dir.path()).unwrap_or_else(|e| panic!("{}", e.0));
        let aside = aside.unwrap_or_else(|| panic!("moved aside"));
        assert!(aside.exists() && dir.path().join(JOURNAL).exists());
        // While it is open, a second opener is refused, and nothing is moved.
        let second = open_or_replace(dir.path())
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(second.contains("another speccoder"), "{second}");
        drop(store);
        assert!(
            open_or_replace(dir.path())
                .unwrap_or_else(|e| panic!("{}", e.0))
                .1
                .is_none()
        );
    }
}
