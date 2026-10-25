+++
title = "Troubleshooting"
description = "Common speccoder errors and what they mean: uncommitted files, missing keys, commands that keep asking, sandbox network errors, stuck runs and exhausted provider accounts."
weight = 140

[extra]
group = "Reference"
+++

Every failure names a [code](@/docs/codes.md) and the command that fixes it — `speccoder explain CODE` says more, offline.

| Symptom | Cause and fix |
|---|---|
| `error[CHECK-BROKEN]` before any model call | A check fails on the untouched worktree without running a test. Most often a file it needs was never committed — a worktree holds only commits (`speccoder doctor` lists untracked configuration such as `.config/`). Commit it, fix the check, or run with `--allow-failing-baseline`. |
| No colour | Output is not a terminal, or `NO_COLOR` or `TERM=dumb` is set. `--color always` forces it. |
| Odd symbols or boxes | The font lacks them: `--ascii`, or `[ui] ascii = true` in your settings. |
| `error[CONFIG-MISSING]` — the worktree has no speccoder.toml (13) | speccoder reads committed files. Commit `speccoder.toml`. |
| `error[UNCOMMITTED-INTENT]` — … are not committed (14) | Commit the feature's Spec Kit files and `.specify/`. |
| `error[KEY-MISSING]` — OPENAI_API_KEY is not set (13) | Export the key in this shell, e.g. `set -a; . ./.env; set +a`. |
| `unknown section [budgett]` (13) | A typo in `speccoder.toml`; the message lists the known sections. |
| Every attempt stops for `cargo test` (or similar) | Add the prefix to `[commands] allow`; `init` does this for toolchains it knows. |
| The baseline fails on a project without tests | For nextest add `--no-tests=warn`, as `init` does. |
| A build cannot download dependencies | The sandbox has no network. Fetch first (`cargo fetch`, `npm ci`, `uv sync`) or add the fetch command to `[sandbox] network_allow`. |
| A tool writes outside the project and fails | Add its cache to `[sandbox] writable`, e.g. `["~/.m2"]` or `["~/.gradle"]`. |
| `commands run unsandboxed: …` | The OS offers no sandbox (Linux without Landlock, another OS). Use `[sandbox] mode = "required"` to refuse instead. |
| `error[TASK-NOT-READY]`, `TASK-BLOCKED` — no task can run (16) | `speccoder inspect` shows what blocks what; a skipped task blocks its dependents. |
| `stopped[BUDGET-EXHAUSTED]` | Raise `[budget] max_turns` or split the task in `tasks.md`. |
| `acceptance skipped: … is not an allowed command` | The acceptance tests' command must be allowed: add its prefix to `[commands] allow`. |
| `stopped[ACCEPTANCE-UNWRITTEN]` or `PROVIDER-REFUSED` — refused the request: insufficient_quota … / credit_balance_exhausted | The provider account has no credit; nothing is retried. Top it up and run again. |
| `another speccoder is working on this project` | A second process found the journal in use. Wait for the first, or check `speccoder report --live`. |
| `MCP tool … changed since it was allowed` | The server changed a tool you allowed. Read it again; approving one call records it as it is now. |
| A run seems stuck | `speccoder report --live` from another terminal shows the step and the last tool call. |

## Getting help

Search the [issues](https://github.com/hupe1980/speccoder/issues) or open one with what you ran (`speccoder --version`, the command, `--json` output if possible) and what you expected. Report security problems privately through [security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
