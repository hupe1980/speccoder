//! The Ask plane: a journal of its own beside the project's, holding one
//! run per conversation. Only Ask's skill, only reading tools.

use std::path::Path;
use std::sync::Arc;

use agentplane::core::{CorrelationKey, Delivery, InboundEvent};
use agentplane::manifest::{Budgets, Manifest, ModelRef, Models, ToolGrant};
use agentplane::model::ModelProvider;
use agentplane::prelude::*;
use agentplane::runtime::{Agent, RunOutcome, RunStreamObserver, RunTerms};
use serde_json::Value;

use super::model_config::ModelConfig;
use super::{Plane, PlaneError, RunClaim, open_or_replace};

/// A conversation's whole life: the plane enforces Ask's limits per turn,
/// so the run's own ceiling is only a backstop.
const CONVERSATION_TOKENS: u64 = 50_000_000;

fn manifest(model: &ModelConfig) -> Result<Manifest, PlaneError> {
    Manifest::builder("speccoder-ask", env!("CARGO_PKG_VERSION"))
        .configure(|spec| {
            spec.capabilities
                .provides
                .push(crate::ask::skill::CAPABILITY.to_owned());
            spec.models = Some(Models {
                privileged: Some(ModelRef {
                    max_tokens: Some(model.max_output_tokens),
                    ..ModelRef::new(model.provider.clone(), model.model.clone())
                }),
                ..Models::default()
            });
            spec.security.max_sensitivity_egress = Some(Sensitivity::Internal);
            spec.budgets = Some(Budgets {
                max_tokens: Some(CONVERSATION_TOKENS),
                ..Budgets::default()
            });
            // Reading only: no grant mutates.
            let workspace = crate::tools::catalog()
                .into_iter()
                .filter(|t| crate::ask::tools::WORKSPACE.contains(&t.name))
                .map(|t| {
                    (
                        t.server.to_owned(),
                        t.name.to_owned(),
                        t.description.to_owned(),
                    )
                });
            let readers = crate::ask::tools::declarations()
                .into_iter()
                .filter(|d| d.name != crate::ask::PROPOSE)
                .map(|d| (crate::ask::tools::SERVER.to_owned(), d.name, d.description));
            for (server, name, description) in workspace.chain(readers) {
                spec.tools.push(ToolGrant {
                    description: Some(description),
                    max_sensitivity: Some(Sensitivity::Internal),
                    mutates: false,
                    ..ToolGrant::new(agentplane::tools::ToolId::new(&server, &name).reference())
                });
            }
        })
        .build()
        .map_err(|e| PlaneError(format!("the Ask manifest: {e}")))
}

/// What answers an Ask plane besides the model.
pub struct AskWiring {
    /// The workspace's reading tools, bound to the checkout.
    pub workspace: Arc<crate::tools::WorkspaceTools>,
    /// Ask's own readers.
    pub readers: Arc<crate::ask::tools::AskTools>,
    /// Live steps and the turn's end.
    pub progress: Option<Arc<dyn crate::progress::Progress>>,
    /// The answer's words as they arrive.
    pub stream: Option<Arc<dyn RunStreamObserver>>,
    /// Set when the person stops the turn in flight.
    pub halt: Arc<std::sync::atomic::AtomicBool>,
}

impl Plane {
    /// Opens (or creates) the Ask journal at `dir/journal.redb`.
    ///
    /// # Errors
    ///
    /// Another turn holds the journal, or it cannot be opened.
    pub fn open_ask(
        dir: &Path,
        model: &ModelConfig,
        provider: Arc<dyn ModelProvider>,
        wiring: AskWiring,
    ) -> Result<Self, PlaneError> {
        let claim = RunClaim::take(dir)?;
        let (store, replaced_journal) = open_or_replace(dir)?;
        let manifest = manifest(model)?;
        let agent = Agent::new(&manifest).skill(crate::ask::skill::Ask {
            style: model.prompt_style(),
            progress: wiring.progress,
            halt: wiring.halt,
        });
        let workspace: Arc<dyn agentplane::tools::ToolClient> = wiring.workspace;
        let mut builder = Runtime::builder_on(store)
            .agent(agent)
            .provider(model.provider.clone(), provider)
            .tool_server("fs", Arc::clone(&workspace))
            .tool_server("git", workspace)
            .tool_server(crate::ask::tools::SERVER, wiring.readers);
        if let Some(stream) = wiring.stream {
            builder = builder.observe_model_streams(stream);
        }
        let runtime = builder
            .try_build()
            .map_err(|e| PlaneError(format!("building the Ask plane: {e}")))?;
        Ok(Self {
            runtime,
            replaced_journal,
            _run: Some(claim),
        })
    }

    /// Starts conversation `id`: the run waits for its first message.
    ///
    /// # Errors
    ///
    /// The run cannot be admitted.
    pub async fn ask_start(
        &self,
        id: &str,
        input: Tainted<Value>,
    ) -> Result<RunOutcome, PlaneError> {
        self.runtime
            .start(crate::ask::skill::CAPABILITY, input)
            .terms(
                RunTerms::default().correlated("ask", &[CorrelationKey::new("ask", id.to_owned())]),
            )
            .run()
            .await
            .map_err(|e| PlaneError(e.to_string()))
    }

    /// Delivers a person's message to conversation `id`; its run answers
    /// and waits again before this returns. `false`: no run waits (it
    /// ended or went idle).
    ///
    /// # Errors
    ///
    /// The message cannot be stored, or the run cannot resume.
    pub async fn ask_deliver(
        &self,
        id: &str,
        message_id: &str,
        payload: Value,
    ) -> Result<bool, PlaneError> {
        let event = InboundEvent::new(
            "urn:speccoder:person",
            message_id,
            crate::ask::skill::MESSAGE,
            payload,
        )
        .correlate(CorrelationKey::new("ask", id.to_owned()));
        match self.runtime.deliver(&event).await {
            Ok(Delivery::Resumed { .. }) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(PlaneError(e.to_string())),
        }
    }

    /// Stops conversation run `run` at its next step boundary, on the
    /// record (`by`): the turn in flight ends and the run with it.
    ///
    /// # Errors
    ///
    /// `run` is not a run id, or the stop cannot be recorded.
    pub async fn ask_stop(&self, run: &str, by: &str) -> Result<(), PlaneError> {
        let run = agentplane::core::RunId::parse(run).map_err(|e| PlaneError(e.to_string()))?;
        let by = agentplane::core::Operator::asserted(by).map_err(|e| PlaneError(e.to_string()))?;
        self.runtime
            .request_cancel(run, &by, "stopped by the person asking")
            .await
            .map(|_| ())
            .map_err(|e| PlaneError(e.to_string()))
    }
}
