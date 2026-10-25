+++
title = "Installation"
description = "Install speccoder with Homebrew, the installer or Cargo, verify what you downloaded, and what it needs: git, a Spec Kit or OpenSpec project, a model and your project's toolchain."
weight = 10

[extra]
group = "Start"
+++

## Requirements

| You need | Why |
|---|---|
| **git** | speccoder works in a git worktree and records every attempt in git. |
| **A spec project** | A GitHub Spec Kit project (`specify init --here --integration claude`; Spec Kit **0.13 to 1.2**) or an [OpenSpec](@/docs/openspec.md) project (OpenSpec 1.0 or newer). Kiro specs are recognised and refused. |
| **A model** | An Anthropic or OpenAI API key in the environment, or a local OpenAI-compatible server such as Ollama or vLLM. |
| **Your toolchain** | Whatever your checks run — for a Rust project `cargo` (and ideally `cargo-nextest`), for Python `pytest`, and so on. |
| **macOS or Linux** | The OS sandbox uses Seatbelt on macOS and Landlock on Linux 6.2 or newer (Landlock ABI 3). |

## Install

Release builds exist for macOS on Apple silicon and Linux (glibc) on arm64 and x86_64. On an Intel Mac, build from source (`cargo install speccoder --locked`); musl-based Linux and Windows are not supported.

**Homebrew** (macOS, Linux):

```sh
brew install hupe1980/tap/speccoder
```

**Installer** — downloads the archive for your platform, checks its SHA-256 and installs to `~/.local/bin` (no `sudo`):

```sh
curl -LsSf https://github.com/hupe1980/speccoder/releases/latest/download/speccoder-installer.sh | sh
```

`--version 0.1.0` (or `SPECCODER_VERSION`) installs a given release, `--to DIR` (or `SPECCODER_INSTALL_DIR`) another directory, and `--verify-attestation` also checks the build provenance with the GitHub CLI.

**Cargo** (Rust 1.99 or newer):

```sh
cargo install speccoder --locked
```

**From source:**

```sh
git clone https://github.com/hupe1980/speccoder
cargo install --locked --path speccoder/crates/speccoder
```

Check it:

```sh
speccoder --version
```

### The desktop window

`speccoder ui --window` opens the [web interface](@/docs/web.md) as an app. On macOS it is part of every build (WKWebView, nothing to install). On Linux it needs WebKitGTK and a build with the feature:

```sh
sudo apt install libwebkit2gtk-4.1-dev   # Debian, Ubuntu
cargo install speccoder --locked --features app
```

Without it, `speccoder ui` serves the same page to your browser.

## Verify a download

Every release lists `SHA256SUMS` and carries a [build-provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) for each archive, signed by the release workflow:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify speccoder-0.1.0-aarch64-apple-darwin.tar.gz --repo hupe1980/speccoder
```

The attestation names the repository, `.github/workflows/release.yml`, the tag and the commit the archive was built from.

## API keys

speccoder reads keys **only from the environment** — `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` — never from files, and never writes them to its journal or output. To keep a key in a gitignored `.env` file, load it into the shell first:

```sh
set -a; . ./.env; set +a
```

Use a key with a spending limit. A local model needs no key; see [`[model]`](@/docs/configuration.md#model).

## Optional tools

| Tool | Enables |
|---|---|
| `cargo-nextest` | Per-test results and regression detection for Rust (`speccoder init` configures it). |
| `cargo-mutants` | [Mutation evidence](@/docs/evidence.md#mutation-testing). |
| `gh` | `--verify-attestation` and `gh attestation verify` (pull requests do not need it; see [GitHub](@/docs/github.md#sign-in)). |
| `specify` (Spec Kit CLI) | Creating projects and running [Spec Kit workflows](@/docs/planning.md#spec-kit-workflows). |
| `claude`, `codex` | Using Claude Code or Codex as [external agents](@/docs/integrations.md#external-agents). |
