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
| **Your toolchain** | Whatever your checks run: for Rust `cargo` (ideally with `cargo-nextest`), for Python `pytest`, and so on. |
| **macOS or Linux** | The OS sandbox uses Seatbelt on macOS and Landlock on Linux 6.2 or newer (Landlock ABI 3). |

## Install

Release builds exist for macOS on Apple silicon and Linux (glibc) on x86_64 and arm64. On an Intel Mac, build from source (`cargo install speccoder --locked`); musl-based Linux and Windows are not supported.

**Homebrew** (macOS, Linux):

```sh
brew install hupe1980/tap/speccoder
```

**Installer:** downloads the archive for your platform, checks its SHA-256 and installs to `~/.local/bin` (no `sudo`):

```sh
curl -LsSf https://github.com/hupe1980/speccoder/releases/latest/download/speccoder-installer.sh | sh
```

| Option | Environment | Meaning |
|---|---|---|
| `--version V` | `SPECCODER_VERSION` | A given release (default: the latest) |
| `--to DIR` | `SPECCODER_INSTALL_DIR` | Another directory |
| `--verify-attestation` | | Also check the build provenance with the GitHub CLI |

Exit status: 0 installed, 1 a download or a tool is missing, 2 unsupported platform, 3 checksum mismatch, 4 attestation failed.

**Cargo:** [`cargo binstall`](https://github.com/cargo-bins/cargo-binstall) takes the release archive; `cargo install` builds it (Rust 1.99 or newer):

```sh
cargo binstall speccoder
cargo install speccoder --locked
```

**From source:**

```sh
git clone https://github.com/hupe1980/speccoder
cargo install --locked --path speccoder/crates/speccoder
```

Check it with `speccoder --version`.

### The desktop window

`speccoder ui --window` opens the [web interface](@/docs/web.md) in its own window. On macOS it is part of every build (WKWebView, nothing to install). On Linux it needs WebKitGTK and a build with the `app` feature:

```sh
sudo apt install libwebkit2gtk-4.1-dev   # Debian, Ubuntu
cargo install speccoder --locked --features app
```

Without it, `speccoder ui` serves the same page to your browser.

## Verify a download

Every release lists `SHA256SUMS` and carries a [build-provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) for each archive, signed by the release workflow:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify speccoder-0.2.0-aarch64-apple-darwin.tar.gz --repo hupe1980/speccoder
```

The attestation names the repository, `.github/workflows/release.yml`, the tag and the commit the archive was built from.

## The verifier

[`speccoder-verify`](@/docs/verifier.md), for maintainers checking evidence, ships from the same release as `speccoder-verify-<version>-<target>.tar.gz` for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` (statically linked: it runs on any Linux) and `aarch64-apple-darwin`, in the same `SHA256SUMS` and with the same build provenance:

```sh
gh release download v0.2.0 --repo hupe1980/speccoder --pattern 'speccoder-verify-0.2.0-x86_64-unknown-linux-musl.tar.gz' --pattern SHA256SUMS
gh attestation verify speccoder-verify-0.2.0-x86_64-unknown-linux-musl.tar.gz --repo hupe1980/speccoder
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Or `cargo binstall speccoder-verify` (the archive) or `cargo install speccoder-verify --locked`. Homebrew and the installer install `speccoder` only. The verifier needs `git`, and `ssh-keygen`, `gpg` or `cosign` for the signatures it meets; no model key. On GitHub, the [action](@/docs/github.md#evidence-check) does all of this.

## API keys

speccoder reads keys **only from the environment** (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`), never from files, and never writes them to its journal or output. To keep a key in a gitignored `.env` file, load it into the shell first:

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
