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
| **A spec project** | A GitHub Spec Kit project (Spec Kit **0.13 to 1.2**) or an [OpenSpec](@/docs/openspec.md) project (OpenSpec 1.0 or newer). Kiro specs are recognised and refused. |
| **A model** | An Anthropic or OpenAI API key in the environment, AWS credentials for Amazon Bedrock (a Bedrock API key or an `aws login` session), or a local OpenAI-compatible server such as Ollama or vLLM. |
| **Your toolchain** | Whatever your checks run: `cargo` (ideally with `cargo-nextest`), `pytest`, and so on. |
| **macOS or Linux** | The OS sandbox uses Seatbelt on macOS and Landlock on Linux 6.2 or newer (Landlock ABI 3). |

## Install

Release builds exist for macOS on Apple silicon and Linux (glibc) on x86_64 and arm64. On an Intel Mac, build from source; musl-based Linux and Windows are not supported.

```sh
brew install hupe1980/tap/speccoder                     # Homebrew (macOS, Linux)
curl -LsSf https://github.com/hupe1980/speccoder/releases/latest/download/speccoder-installer.sh | sh
cargo binstall speccoder                                # the release archive
cargo install speccoder --locked                        # build it (Rust 1.99 or newer)
```

The installer checks the archive's SHA-256 and installs to `~/.local/bin` (no `sudo`):

| Option | Environment | Meaning |
|---|---|---|
| `--version V` | `SPECCODER_VERSION` | A given release (default: the latest) |
| `--to DIR` | `SPECCODER_INSTALL_DIR` | Another directory |
| `--verify-attestation` | | Also check the build provenance with the GitHub CLI |

Its exit status: 0 installed, 1 a download or a tool is missing, 2 unsupported platform, 3 checksum mismatch, 4 attestation failed.

From source: `git clone https://github.com/hupe1980/speccoder`, then `cargo install --locked --path speccoder/crates/speccoder`. Check it with `speccoder --version`.

### The desktop window

`speccoder ui --window` opens the [web interface](@/docs/web.md) in its own window. On macOS it is part of every build. On Linux it needs WebKitGTK and a build with the `app` feature:

```sh
sudo apt install libwebkit2gtk-4.1-dev   # Debian, Ubuntu
cargo install speccoder --locked --features app
```

Without it, `speccoder ui` serves the same page to your browser.

## Verify a download

Every release lists `SHA256SUMS` and carries a [build-provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) for each archive, naming the repository, `.github/workflows/release.yml`, the tag and the commit:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify speccoder-0.4.0-aarch64-apple-darwin.tar.gz --repo hupe1980/speccoder
```

## The branch check

[`speccoder attest verify --branch`](@/docs/verifier.md) is part of `speccoder`. It needs `git`, and `ssh-keygen`, `gpg` or `cosign` for the signatures it meets; no model key. On GitHub, the [evidence action](@/docs/github.md#evidence-check) downloads the `speccoder` archive for the runner and checks its provenance and sha256 before it runs it.

## API keys

speccoder reads keys **only from the environment** (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`; for Bedrock the AWS credential chain: `AWS_BEARER_TOKEN_BEDROCK` or a profile) and never writes them to its journal or output. To use a gitignored `.env` file, load it into the shell first:

```sh
set -a; . ./.env; set +a
```

Use a key with a spending limit. A local model needs no key; see [`[model]`](@/docs/configuration.md#model).

## Optional tools

| Tool | Enables |
|---|---|
| `cargo-nextest` | Per-test results and regression detection for Rust (`speccoder init` configures it). |
| `cargo-mutants` | [Mutation evidence](@/docs/evidence.md#mutation-testing). |
| `gh` | The installer's `--verify-attestation` and `gh attestation verify` (`speccoder pr` does not need it; see [GitHub](@/docs/github.md#sign-in)). |
| `specify` (Spec Kit CLI) | Creating projects and running [Spec Kit workflows](@/docs/planning.md#spec-kit-workflows). |
| `claude`, `codex` | Claude Code or Codex as [external agents](@/docs/integrations.md#external-agents). |
