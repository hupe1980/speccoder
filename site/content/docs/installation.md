+++
title = "Installation"
description = "Install speccoder from source with Cargo, and what it needs: git, a Spec Kit project, a model and your project's toolchain."
weight = 10

[extra]
group = "Start"
+++

## Requirements

| You need | Why |
|---|---|
| **git** | speccoder works in a git worktree and records every attempt in git. |
| **A GitHub Spec Kit project** | `specify init` creates one. Projects from Spec Kit **0.13 to 1.2** are supported; each release in that range is tested against what it generates. |
| **A model** | An Anthropic or OpenAI API key in the environment, or a local OpenAI-compatible server such as Ollama or vLLM. |
| **Your toolchain** | Whatever your checks run — for a Rust project `cargo` (and ideally `cargo-nextest`), for Python `pytest`, and so on. |
| **macOS or Linux** | The OS sandbox uses Seatbelt on macOS and Landlock (kernel 5.13+) on Linux. |

## Install

speccoder is a single Rust binary. Install it with Cargo (Rust 1.99 or newer, as declared in its `rust-version`):

```sh
cargo install --git https://github.com/hupe1980/speccoder speccoder
```

Or from a clone:

```sh
git clone https://github.com/hupe1980/speccoder
cargo install --path speccoder/crates/speccoder
```

Check it:

```sh
speccoder --version
```

## API keys

speccoder reads keys **only from the environment** — `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` — and never from files, never into its journal or output. To keep a key in a gitignored `.env` file, load it into the shell first:

```sh
set -a; . ./.env; set +a
```

Use a key with a spending limit. A local model needs no key; see [`[model]`](@/docs/configuration.md#model).

## Optional tools

| Tool | Enables |
|---|---|
| `cargo-nextest` | Per-test results and regression detection for Rust (`speccoder init` configures it). |
| `cargo-mutants` | [Mutation evidence](@/docs/evidence.md#mutation-testing). |
| `gh` | [Draft pull requests](@/docs/integrations.md#pull-requests). |
| `specify` (Spec Kit CLI) | Creating projects and running [Spec Kit workflows](@/docs/planning.md#spec-kit-workflows). |
| `claude`, `codex` | Using Claude Code or Codex as [external agents](@/docs/integrations.md#external-agents). |
