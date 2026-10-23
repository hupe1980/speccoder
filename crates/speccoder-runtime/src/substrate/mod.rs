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
use agentplane::runtime::{Agent, RunOutcome};
use serde_json::Value;

use self::model_config::ModelConfig;
use self::propose::{CAPABILITY, Propose};

/// The journal file inside a project's state directory.
pub const JOURNAL: &str = "journal.redb";

/// How long a run's lease lasts without renewal.
const LEASE: std::time::Duration = std::time::Duration::from_secs(4);

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
            ];
            sources.extend(catalog.iter().map(|t| provenance::tool(t.server, t.name)));
            sources.extend(mcp.iter().map(|t| provenance::tool(&t.server, &t.tool)));
            for t in &catalog {
                let mut grant = ToolGrant {
                    description: Some(t.description.to_owned()),
                    max_sensitivity: Some(Sensitivity::Internal),
                    mutates: t.mutates,
                    ..ToolGrant::new(format!("tool://{}/{}", t.server, t.name))
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
                    ..ToolGrant::new(format!("tool://{}/{}", t.server, t.tool))
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
    /// Asks a person inline.
    pub approver: Option<Arc<dyn crate::approvals::Approver>>,
    /// Receives live progress.
    pub progress: Option<Arc<dyn crate::progress::Progress>>,
    /// Running MCP servers by name.
    pub mcp: Vec<(String, Arc<dyn agentplane::tools::ToolClient>)>,
    /// Their tools, granted in the manifest.
    pub mcp_tools: Vec<crate::mcp::McpTool>,
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

fn build(
    store: Arc<RedbStore>,
    model: &ModelConfig,
    provider: Arc<dyn ModelProvider>,
    tools: Option<Arc<crate::tools::WorkspaceTools>>,
    hooks: Hooks,
) -> Result<Arc<Runtime>, PlaneError> {
    let manifest = manifest_with(model, &hooks.mcp_tools)?;
    let agent = Agent::new(&manifest)
        .skill(Propose {
            cache_ttl: model.cache_ttl,
            style: model.prompt_style(),
        })
        .skill(attempt::Attempt {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
            approver: hooks.approver.clone(),
            progress: hooks.progress.clone(),
        })
        .skill(host::Host {
            style: model.prompt_style(),
            approver: hooks.approver,
            progress: hooks.progress,
        })
        .skill(acceptance::Acceptance {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
        })
        .skill(converge::Converge {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
        })
        .skill(monitor::Monitor {
            style: model.prompt_style(),
            cache_ttl: model.cache_ttl,
        });
    // One process owns the journal (redb locks it): a lease left by an
    // earlier process is a dead owner's, and a short one lets the next run
    // resume its attempt promptly.
    let mut builder = Runtime::builder_on(store)
        .provider(model.provider.clone(), provider)
        .agent(agent)
        .lease_ttl(LEASE);
    // The tool servers answer only when a worktree is bound; without one, a
    // call to a granted tool is refused by agentplane as unwired.
    let client: Arc<dyn agentplane::tools::ToolClient> = match tools {
        Some(t) => t,
        None => Arc::new(Unbound),
    };
    for server in ["fs", "proc", "git"] {
        builder = builder.tool_server(server, Arc::clone(&client));
    }
    for (server, mcp) in &hooks.mcp {
        builder = builder.tool_server(server.as_str(), Arc::clone(mcp));
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

    /// Runs `speccoder.feature.converge`, correlated to the feature.
    pub async fn converge(
        &self,
        feature: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .run_correlated(
                converge::CAPABILITY,
                input,
                "feature",
                &[CorrelationKey::new("feature", feature.to_owned())],
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.skill.run` for an installed skill.
    pub async fn skill(&self, name: &str, input: Tainted<Value>) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .run_correlated(
                host::CAPABILITY,
                input,
                "skill",
                &[CorrelationKey::new("skill", name.to_owned())],
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.task.monitor`, correlated to the task.
    pub async fn monitor(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .run_correlated(
                monitor::CAPABILITY,
                input,
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Runs `speccoder.task.acceptance`: acceptance tests derived from the spec.
    pub async fn acceptance(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .run_correlated(
                acceptance::CAPABILITY,
                input,
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            )
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
            .run_correlated(
                attempt::CAPABILITY,
                input,
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// [`attempt`](Self::attempt) admitted under `key`: the first
    /// time it runs; after a crash the same key finds the interrupted run and
    /// resumes it from its journal — finished model and tool calls are read
    /// back, not repeated — and a run that had finished is read back whole.
    /// The `bool` says whether this was a resume.
    pub async fn attempt_keyed(
        &self,
        feature: &str,
        task: &str,
        input: Tainted<Value>,
        key: &str,
    ) -> Result<(RunOutcome, bool), PlaneError> {
        use agentplane::runtime::{Admission, Mode};
        let admitted = self
            .runtime
            .run_correlated_once(
                attempt::CAPABILITY,
                input,
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
                key,
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))?;
        match admitted {
            Admission::Fresh(o) => Ok((o, false)),
            Admission::Replayed(o) => self
                .runtime
                .replay(o.run_id, Mode::Strict)
                .await
                .map(|o| (o, true))
                .map_err(|e| PlaneError(e.to_string())),
            Admission::InFlight(run) => {
                // The dead owner's lease lapses within `LEASE`.
                let mut waited = std::time::Duration::ZERO;
                loop {
                    match self.runtime.replay(run, Mode::Resume).await {
                        Ok(o) => return Ok((o, true)),
                        Err(agentplane::RuntimeError::LeaseHeld { .. }) if waited < LEASE * 3 => {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            waited += std::time::Duration::from_millis(500);
                        }
                        Err(e) => return Err(PlaneError(e.to_string())),
                    }
                }
            }
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
            .run_correlated(
                CAPABILITY,
                input,
                "task",
                &[CorrelationKey::new("task", format!("{feature}/{task}"))],
            )
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }
}

/// What a run was admitted with: its input, the root label and the field
/// labels by JSON Pointer — from the journal's `RunAdmitted` record, never
/// from anything a tool returned.
#[derive(Debug, Clone)]
pub struct Admitted {
    /// The input.
    pub input: Value,
    /// The join of everything in it.
    pub label: agentplane::core::Label,
    /// Each labelled part's own label.
    pub fields: std::collections::BTreeMap<String, agentplane::core::Label>,
}

/// The [`Admitted`] input of `run`, if the journal has it.
pub async fn admitted(dir: &Path, run: &str) -> Result<Option<Admitted>, PlaneError> {
    let id = agentplane::core::RunId::parse(run)
        .map_err(|_| PlaneError(format!("`{run}` is not a run id")))?;
    let store = open_store(dir)?;
    let history = store
        .read(id, 1)
        .await
        .map_err(|e| PlaneError(e.to_string()))?;
    Ok(admitted_in(&history))
}

fn admitted_in(history: &[agentplane::journal::Record]) -> Option<Admitted> {
    history.iter().find_map(|r| match r.kind() {
        agentplane::journal::RecordKind::RunAdmitted {
            input,
            input_label,
            input_field_labels,
            ..
        } => Some(Admitted {
            input: input.clone(),
            label: input_label.clone(),
            fields: input_field_labels.clone(),
        }),
        _ => None,
    })
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
    let provider: Arc<dyn ModelProvider> = Arc::new(
        agentplane::runtime::replay_only::Provider::from_history(&history),
    );
    // The run's MCP tools as its admitted input lists them: their
    // grants again, servers unbound — replay dispatches nothing.
    let mcp_tools: Vec<crate::mcp::McpTool> = admitted_in(&history)
        .and_then(|a| serde_json::from_value(a.input["settings"]["mcp"].clone()).ok())
        .unwrap_or_default();
    let mut servers: Vec<String> = mcp_tools.iter().map(|t| t.server.clone()).collect();
    servers.sort();
    servers.dedup();
    let hooks = Hooks {
        mcp: servers
            .into_iter()
            .map(|s| {
                (
                    s,
                    Arc::new(Unbound) as Arc<dyn agentplane::tools::ToolClient>,
                )
            })
            .collect(),
        mcp_tools,
        ..Hooks::default()
    };
    let runtime = build(store, model, provider, None, hooks)?;
    runtime
        .verify(id)
        .await
        .map(Some)
        .map_err(|e| PlaneError(e.to_string()))
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

/// agentplane's status word for a run.
#[must_use]
pub fn status_word(status: &RunStatus) -> String {
    status.as_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
