//! Application services of speccoder.
//!
//! The only crate that executes anything: every side effect (file writes,
//! processes, model calls) goes through here, and model calls and tool calls
//! through the embedded `agentplane` runtime (see [`substrate`]).
//! The CLI only parses arguments, calls a service and renders its result.

#[cfg(feature = "engine")]
pub mod actions;
#[cfg(feature = "engine")]
pub mod approvals;
#[cfg(feature = "engine")]
pub mod attempts;
pub mod attest;
#[cfg(feature = "engine")]
pub mod bench;
#[cfg(feature = "engine")]
pub mod bug;
#[cfg(feature = "engine")]
pub mod code;
pub mod collection;
pub mod commands;
#[cfg(feature = "engine")]
pub mod comments;
pub mod context;
#[cfg(feature = "engine")]
pub mod controls;
#[cfg(feature = "engine")]
pub mod convergence;
#[cfg(feature = "engine")]
pub mod current;
#[cfg(feature = "engine")]
pub mod doctor;
#[cfg(feature = "engine")]
pub mod documents;
mod drift;
#[cfg(feature = "engine")]
pub mod eval;
pub mod evidence;
#[cfg(feature = "engine")]
pub mod external;
pub mod format;
pub mod git;
#[cfg(feature = "engine")]
pub mod github;
#[cfg(feature = "engine")]
pub mod handoff;
#[cfg(feature = "engine")]
pub mod held_out;
#[cfg(feature = "engine")]
pub mod hosting;
#[cfg(feature = "engine")]
pub mod in_place;
#[cfg(feature = "engine")]
pub mod inbox;
#[cfg(feature = "engine")]
pub mod init;
#[cfg(feature = "engine")]
pub mod instructions;
pub mod integrity;
pub mod ladder;
#[cfg(feature = "engine")]
pub mod mcp;
#[cfg(feature = "engine")]
pub mod mutation;
#[cfg(feature = "engine")]
pub mod next;
#[cfg(feature = "engine")]
pub mod oracle;
#[cfg(feature = "engine")]
pub mod pipeline;
#[cfg(feature = "engine")]
pub mod pre_frozen;
#[cfg(feature = "engine")]
pub mod progress;
#[cfg(feature = "engine")]
pub mod quick;
#[cfg(feature = "engine")]
pub mod resume;
#[cfg(feature = "engine")]
pub mod review;
#[cfg(feature = "engine")]
pub mod sandbox;
#[cfg(feature = "engine")]
pub mod service;
#[cfg(feature = "engine")]
pub mod settings;
#[cfg(feature = "engine")]
pub mod skills;
#[cfg(feature = "engine")]
pub mod status;
#[cfg(feature = "engine")]
pub mod strength;
#[cfg(feature = "engine")]
pub mod substrate;
/// Without the engine: only the policy parse of `speccoder.toml`.
#[cfg(not(feature = "engine"))]
pub mod substrate {
    pub mod run_config;
}
#[cfg(feature = "engine")]
pub mod tools;
pub mod tour;
/// Without the engine: only the path classes and the write policy.
#[cfg(not(feature = "engine"))]
pub mod tools {
    pub mod paths;
    pub mod policy;
}
pub mod user_config;
pub mod verify;
pub mod worktree;

pub use commands::*;
pub use format::speckit::{hooks, workflow};
#[cfg(feature = "engine")]
pub use pipeline::*;
