//! Application services of speccoder.
//!
//! The only crate that executes anything: every side effect (file writes,
//! processes, model calls) goes through here, and model calls and tool calls
//! through the embedded `agentplane` runtime (see [`substrate`]).
//! The CLI only parses arguments, calls a service and renders its result.

pub mod acceptance_at;
pub mod actions;
pub mod approvals;
pub mod attempts;
pub mod attest;
pub mod bench;
pub mod bug;
pub mod code;
pub mod collection;
pub mod commands;
pub mod comments;
pub mod context;
pub mod controls;
pub mod convergence;
pub mod current;
pub mod doctor;
pub mod documents;
mod drift;
pub mod eval;
pub mod evidence;
pub mod external;
pub mod format;
pub mod git;
pub mod github;
pub mod handoff;
pub mod held_out;
pub mod hosting;
pub mod in_place;
pub mod inbox;
pub mod init;
pub mod instructions;
pub mod integrity;
pub mod ladder;
pub mod layout;
pub mod mcp;
pub mod mutation;
pub mod new_project;
pub mod next;
pub mod oracle;
pub mod pipeline;
pub mod plan_lint;
pub mod planning_chain;
pub mod pre_frozen;
pub mod progress;
pub mod quick;
pub mod resume;
pub mod review;
pub mod sample;
pub mod sandbox;
pub mod service;
pub mod settings;
pub mod skills;
pub mod status;
pub mod strength;
pub mod substrate;
pub mod tools;
pub mod tour;
pub mod user_config;
pub mod verify;
pub mod welcome;
pub mod worktree;

pub use commands::*;
pub use format::speckit::{hooks, preset, workflow};
pub use pipeline::*;
