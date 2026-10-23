+++
title = "Troubleshooting"
description = "Common speccoder errors and what they mean: uncommitted files, missing keys, commands that keep asking, sandbox network errors, stuck runs and exhausted provider accounts."
weight = 140

[extra]
group = "Reference"
+++

| Symptom | Cause and fix |
|---|---|
| `the worktree has no speccoder.toml (is it committed?)` (13) | speccoder reads committed files. Commit `speccoder.toml`. |
| `… are not committed` (14) | Commit the feature's Spec Kit files and `.specify/`. |
| `OPENAI_API_KEY is not set` (13) | Export the key in this shell, e.g. `set -a; . ./.env; set +a`. |
| `unknown section [budgett]` (13) | A typo in `speccoder.toml`; the message lists the known sections. |
| Every attempt stops for `cargo test` (or similar) | Add the prefix to `[commands] allow`; `init` does this for toolchains it knows. |
| The baseline fails on a project without tests | For nextest add `--no-tests=warn`, as `init` does. |
| A build cannot download dependencies | The sandbox has no network. Fetch first (`cargo fetch`, `npm ci`, `uv sync`) or add the fetch command to `[sandbox] network_allow`. |
| A tool writes outside the project and fails | Add its cache to `[sandbox] writable`, e.g. `["~/.m2"]` or `["~/.gradle"]`. |
| `commands run unsandboxed: …` | The OS offers no sandbox (Linux without Landlock, another OS). Use `[sandbox] mode = "required"` to refuse instead. |
| `no task is ready` / `no task can run` (16) | `speccoder inspect` shows what blocks what; a skipped task blocks its dependents. |
| `budget_exhausted` | Raise `[budget] max_turns` or split the task in `tasks.md`. |
| `acceptance skipped: … is not an allowed command` | The acceptance tests' command must be allowed: add its prefix to `[commands] allow`. |
| `refused the request: insufficient_quota …` / `credit_balance_exhausted` | The provider account has no credit; nothing is retried. Top it up and run again. |
| `another speccoder is working on this project` | A second process found the journal in use. Wait for the first, or check `speccoder status`. |
| `MCP tool … changed since it was allowed` | The server changed a tool you allowed. Read it again; approving one call records it as it is now. |
| A run seems stuck | `speccoder status` from another terminal shows the step and the last tool call. |

## Getting help

Search the [issues](https://github.com/hupe1980/speccoder/issues) or open one with what you ran (`speccoder --version`, the command, `--json` output if possible) and what you expected. Report security problems privately through [security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
