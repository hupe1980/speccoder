# speccoder

[![CI](https://github.com/hupe1980/speccoder/actions/workflows/ci.yml/badge.svg)](https://github.com/hupe1980/speccoder/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-hupe1980.github.io%2Fspeccoder-5b4bff)](https://hupe1980.github.io/speccoder/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

**Spec-driven coding, decided by evidence.** speccoder implements [GitHub Spec Kit](https://github.com/github/spec-kit) features task by task, and ticks a task only when tests derived from your spec go from red to green — never on the model's word.

**[Documentation](https://hupe1980.github.io/speccoder/docs/)** · [Quickstart](https://hupe1980.github.io/speccoder/docs/quickstart/) · [How it works](https://hupe1980.github.io/speccoder/docs/how-it-works/) · [Configuration](https://hupe1980.github.io/speccoder/docs/configuration/) · [CLI](https://hupe1980.github.io/speccoder/docs/cli/)

## Why

Coding agents are good at writing code and too good at declaring victory. speccoder separates the two: a model proposes, and code decides.

- **Evidence decides.** Before each attempt, the spec's acceptance examples become tests that must fail. After it, your checks run again and the diff is audited for deleted or weakened tests. Optional mutation testing and a gaming monitor catch tests written to pass.
- **Isolated and sandboxed.** Work happens on `speccoder/<feature>` in a worktree of its own. Every command runs in an OS sandbox (Seatbelt on macOS, Landlock and seccomp on Linux): writes only to the worktree, no credentials, no network beyond loopback.
- **Every call governed.** Each tool call is allowed, sent to a person, or denied before it runs. Decisions record who made them — at the terminal, in an editor over ACP, or later.
- **Durable and replayable.** Every model and tool call is journaled. A killed run resumes without repeating finished calls; a recorded run replays offline.
- **Your model, or your agent.** Anthropic, OpenAI, any OpenAI-compatible server (Ollama, vLLM, …), or Claude Code and Codex making the attempts under speccoder's judgement.

## Quickstart

You need git, a Spec Kit project (Spec Kit 0.13 to 1.2) and a model.

```sh
cargo install --git https://github.com/hupe1980/speccoder speccoder

cd my-project
speccoder init                       # detects your checks, writes speccoder.toml — review it
speccoder verify                     # the checks pass before any change
git add -A && git commit -m "speccoder config"   # speccoder reads committed files only

export OPENAI_API_KEY=…              # or ANTHROPIC_API_KEY — read from the environment only
speccoder run                        # works through the feature's tasks
speccoder report                     # every task and the evidence behind its tick
git merge speccoder/001-my-feature   # after reviewing the branch
```

`speccoder status` or `speccoder tui` in another terminal shows a run as it happens. No spec yet? `speccoder skill specify "…"`, then `plan` and `tasks`, runs Spec Kit's own planning skills.

## How a task is decided

1. **Ready** — its dependencies in `tasks.md` are ticked.
2. **Baseline** — your checks run on the untouched worktree, so existing failures are not blamed on the change.
3. **Red** — a separate model call with no tools writes acceptance tests from the spec. They must fail now, and the implementer cannot change them.
4. **Attempt** — the model works in the worktree; every call is classified before it runs.
5. **Judgement** — `verified` when the acceptance tests went red → green and nothing regressed; `regression_only` when nothing task-specific was shown; `failed` otherwise.
6. **Outcome** — a verified task is ticked and committed by speccoder. A failed one is retried once with the verifier's facts. Anything else is saved under `refs/speccoder/attempts/…`, undone, and handed to a person.

`verified` does not mean correct, complete or safe. It means the evidence held. Review the branch like any contribution.

## Status

Pre-release (0.x): it works end to end on small projects with OpenAI, Anthropic and local models. **Your checks run code the model wrote** — read the [security model](https://hupe1980.github.io/speccoder/docs/security/) and [SECURITY.md](https://github.com/hupe1980/speccoder/blob/main/SECURITY.md) before pointing it at anything important.

## Development

| Crate | Role |
|---|---|
| `speccoder-core` | Pure domain types: no IO, no async |
| `speccoder-speckit` | Reads Spec Kit projects |
| `speccoder-runtime` | Everything that executes: checks, tools, sandbox, git, the embedded [agentplane](https://crates.io/crates/agentplane) runtime |
| `speccoder` | The CLI |

`just ci` runs every gate CI runs: formatting, clippy, tests, cargo-deny, layering, docs and a package dry run. It needs `just`, `cargo-nextest` and `cargo-deny`; the toolchain is pinned in `rust-toolchain.toml`. `just site` serves the documentation site (needs [Zola](https://www.getzola.org/)). [`evals/`](https://github.com/hupe1980/speccoder/tree/main/evals) holds Spec Kit projects with hidden oracles and impossible tasks that measure whether completions are really correct.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/hupe1980/speccoder/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/hupe1980/speccoder/blob/main/LICENSE-MIT) at your option.
