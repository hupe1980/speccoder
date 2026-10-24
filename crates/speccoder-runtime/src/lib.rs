//! Application services of speccoder.
//!
//! The only crate that executes anything: every side effect (file writes,
//! processes, model calls) goes through here, and model calls and tool calls
//! through the embedded `agentplane` runtime (see [`substrate`]).
//! The CLI only parses arguments, calls a service and renders its result.

pub mod approvals;
pub mod attempts;
pub mod attest;
pub mod bench;
pub mod commands;
pub mod context;
pub mod controls;
pub mod convergence;
pub mod eval;
pub mod external;
pub mod format;
pub mod git;
pub mod handoff;
pub mod held_out;
pub mod hosting;
pub mod in_place;
pub mod init;
pub mod integrity;
pub mod mcp;
pub mod mutation;
pub mod next;
pub mod pipeline;
pub mod progress;
pub mod resume;
pub mod sandbox;
pub mod skills;
pub mod status;
pub mod substrate;
pub mod tools;
pub mod user_config;
pub mod verify;
pub mod worktree;

pub use commands::*;
pub use format::speckit::{hooks, workflow};
pub use pipeline::*;
