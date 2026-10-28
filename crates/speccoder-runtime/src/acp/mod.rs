//! External agents as executors, over the Agent Client Protocol (ACP v1):
//! speccoder is the client, the agent makes one attempt in the worktree,
//! and every permission request, file and terminal call it sends is decided
//! by speccoder's policy and approvals.
//!
//! Built on the official `agent-client-protocol` crate, and on devplane's
//! proven driver (same owner): [`agent`] (presets, pinned), [`modes`] (the
//! ask-first permission mode per adapter version), [`calls`] (tool-call
//! reports), [`session`] (one attempt); [`govern`] and [`terminal`] are
//! speccoder's — its policy, approvals, workspace tools and sandbox.
//! What an agent does without asking stays outside speccoder: its own
//! process is not sandboxed.

pub mod agent;
pub mod calls;
pub mod govern;
pub mod modes;
pub mod session;
pub mod terminal;
