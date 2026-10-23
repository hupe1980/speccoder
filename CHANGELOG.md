# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

The first release: speccoder implements spec-driven work task by task and
ticks a task only on evidence the model cannot produce or edit.

### Added

**Spec formats**

- [GitHub Spec Kit](https://github.com/github/spec-kit) projects, tested
  against Spec Kit 0.13.4, 0.15.2, 0.16.5, 1.0.13, 1.1.3 and 1.2.3 (a weekly
  job tests the newest release): features, `tasks.md` with phases, `[P]`,
  `[USn]` and `depends on`, `SPECIFY_FEATURE_DIRECTORY`, the checklist gate,
  extension hooks, and a step type for Spec Kit's workflow engine
  (`speccoder workflow install`).
- [OpenSpec](https://github.com/Fission-AI/OpenSpec) 1.0+ projects (`spec-driven`
  schema): a change runs as a chain, its scenarios become the acceptance
  tests, deltas `openspec archive` would refuse stop the run before any model
  call, and an installed `openspec` CLI is asked for a second opinion.
  `--change` selects a change; `speccoder spec archive` archives one — OpenSpec's
  checks first, then the diff, then your yes, then one commit.
- `--format`, `SPECCODER_FORMAT` and `[spec] format`; otherwise the nearest
  project decides. Kiro projects are recognised and refused as not yet
  supported.

**Planning and inspecting**

- `speccoder spec [SKILL]` (also `speccoder skill`): the format's own planning
  skills run with speccoder's model under a planning policy, answering
  clarifying questions inline.
- `speccoder inspect`: features or changes, artifacts, tasks and what blocks
  what, with stable diagnostic codes. `speccoder init` detects a project's
  checks (Cargo/nextest, npm/pnpm/yarn/bun, pytest/ruff/mypy, Go, make) and
  writes `speccoder.toml`.

**Running, judged by evidence**

- `speccoder run`: works through ready tasks (or one, `--task`) on
  `speccoder/<feature>` in a worktree of its own; your checkout is never
  touched. Before an attempt, the spec's acceptance examples become tests that
  must fail on an assertion (or because what the task adds does not exist
  yet); afterwards your checks and those tests run again and the diff is
  audited for deleted, weakened or skipped tests. Only then is the task ticked
  and committed, with its evidence in trailers (`Verdict:`, `Red:`, `Run:`, …).
- Verdicts `verified`, `regression_only`, `failed`, `not_demonstrated`; a
  flaky check is never evidence; the `full` tier runs once at the end of a
  feature; retries with a verifier-built digest (`[budget] max_attempts`);
  `best_of` samples; `--parallel N` for `[P]` tasks; mutation evidence
  (`[mutation]`); an optional gaming monitor (`[monitor]`).
- `speccoder converge`: a finished feature assessed against its spec; gaps
  become new tasks, run with `run --converge`.
- `[executor] kind = "claude" | "codex" | "command"`: Claude Code, Codex or
  your own agent makes the attempts under the same judge.
- Providers: Anthropic, OpenAI, and any OpenAI-compatible server (Ollama,
  vLLM, llama.cpp, LM Studio, …); keys from the environment only.

**Safety**

- Every tool call is allowed, sent to a person, or denied before it runs. The
  spec, the checks' configuration, agent configuration, `.git` and secrets are
  out of the model's reach; these inputs are re-checked after every command.
- Every command runs in an OS sandbox: Seatbelt on macOS, Landlock and
  seccomp on Linux — writes only to the worktree, no credentials, no network
  beyond loopback unless a person allows it.
- MCP servers (`[mcp.<name>]`) run per task, their tools governed like
  speccoder's own.

**People in the loop**

- Approvals inline at the terminal, in editors over ACP (`speccoder acp`),
  in the dashboard (`speccoder tui`), or later (`speccoder approvals`,
  `approve`, `reject`); every decision records who made it.
- `accept`, `skip`, `nudge`, `cost`, `report`, `status`, `pr` (a draft pull
  request with every task's evidence, after you confirm).
- Every stop says what to do next (`Next:`, `data.next`); notifications and
  a terminal title from your own `~/.config/speccoder/config.toml`;
  `-q`/`-v`; `--json` on every command with documented exit codes.

**Durable and replayable**

- Every model and tool call is journaled (agentplane 0.51): a killed run
  resumes without repeating finished calls, tested by killing a recorded
  attempt at each of its crash points;
  `speccoder replay` re-derives a recorded run offline. Failed attempts are
  kept under `refs/speccoder/attempts/…`.

**Evaluation**

- `speccoder eval` judges attempts with hidden oracles (precision, cheats,
  honest stops, tokens per correct case; `--repeat`, `--executor`), including
  impossible tasks an honest agent must refuse; `speccoder bench import` turns
  SWE-bench instances into cases.

**Distribution**

- Release archives for macOS and Linux (x86_64, aarch64) with SHA-256
  checksums and GitHub build-provenance attestations; a shell installer
  (`--verify-attestation`); a Homebrew tap; the crates on crates.io.
- Documentation at https://hupe1980.github.io/speccoder/.

## [0.0.1] - 2026-10-20

### Added

- Name reservation placeholder for `speccoder` on crates.io.

[Unreleased]: https://github.com/hupe1980/speccoder/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/hupe1980/speccoder/releases/tag/v0.0.1
