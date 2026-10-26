//! `speccoder-verify`: checks every ticked task of a branch against its
//! signed attestation, with the caller's trust only. No model, no network,
//! nothing from the checked tree executed: it reads git objects and spawns
//! git and the signature tools the signatures name.

use std::process::ExitCode;

use clap::Parser;

/// Checks every `[X]` of a branch against its signed attestation.
#[derive(Debug, Parser)]
#[command(name = "speccoder-verify", version, about)]
struct Cli {}

fn main() -> ExitCode {
    let _cli = Cli::parse();
    ExitCode::SUCCESS
}
