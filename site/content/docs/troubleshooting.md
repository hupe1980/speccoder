+++
title = "Troubleshooting"
description = "Common speccoder errors and what they mean: uncommitted files, missing keys, commands that keep asking, sandbox network errors, stuck runs and exhausted provider accounts."
weight = 140

[extra]
group = "Reference"
+++

Every failure names a [code](@/docs/codes.md) and the command that fixes it; `speccoder explain CODE` says more, offline.

| Symptom | Cause and fix |
|---|---|
| `error[SETUP-INCOMPLETE]` (14) | The worktree lacks setup your checkout never committed, or the checks leave a lockfile or build output there. At a terminal, `speccoder run` offers the fix and runs again on a yes; the web interface offers a button; else [`speccoder fix`](@/docs/running.md#when-the-setup-stops-a-run). |
| `error[CHECK-BROKEN]` (13) before any model call | A check fails on the untouched worktree without running a test; the message quotes the check's own error. When the first task creates the project, run with `--allow-failing-baseline`. |
| `stopped[SECRET-IN-DIFF]` (23) | The attempt wrote a value of a file `[worktree] include` copied (a `.env`) into a file git would commit; it was undone. Have the code read the variable, then run again. |
| `error[PREPARE-FAILED]` (13) before the baseline or after an attempt | The worktree's locked, offline install failed (after an attempt: for the lockfile it changed). A package the toolchain's cache lacks: run `npm install` or `uv sync` once in your checkout, or allow the install's online form in `[sandbox] network_allow`. No lockfile committed: commit the one `npm install` or `uv lock` writes. `[worktree] prepare = "off"` when the checks need none of it. |
| `error[CONFIG-MISSING]` (13) | No `speccoder.toml`: run `speccoder init`. "The worktree has no speccoder.toml": commit it, since speccoder reads committed files. |
| `error[CONFIG-INVALID]` (13), e.g. ``unknown section `[budgett]` `` | A typo or a wrong value in `speccoder.toml`; the message names the line and lists what is known. |
| `error[UNCOMMITTED-INTENT]` (14) | Commit the feature's spec files, or run with `--in-place`. |
| `error[KEY-MISSING]` (13) | Export the variable the message names in this shell, e.g. `set -a; . ./.env; set +a`. For Bedrock (`doctor` and the app report it; a run still tries the AWS credential chain, e.g. an instance role): `export AWS_BEARER_TOKEN_BEDROCK=…`, or `aws login` (and `AWS_PROFILE`) in the shell you start speccoder from. |
| `error[CONFIG-INVALID]`: provider `bedrock` needs `region` | `speccoder config set model.region <region>`, e.g. `eu-central-1`. |
| Every attempt stops for the same command | The stop names the command and its exact `[commands] allow` entry. `[commands] allow` matches exact commands: add the command there, or a prefix to `[commands] allow_prefix` after `speccoder config coverage` ([prefix lists](@/docs/configuration.md#prefix-lists)). |
| `… is not an allowed command` (acceptance skipped) | The acceptance tests' command must be allowed. The one that runs only their file (`cargo test --test acceptance_us1`) is; another needs `[commands] allow`. |
| The baseline fails on a project without tests | For nextest add `--no-tests=warn`, as `init` does. |
| A build cannot download dependencies | The sandbox has no network. Fetch first (`cargo fetch`, `npm ci`, `uv sync`) or add the fetch command to `[sandbox] network_allow`. |
| A tool writes outside the project and fails | Add its cache to `[sandbox] writable`, e.g. `["~/.m2"]` or `["~/.gradle"]`. |
| `commands run unsandboxed: …` | The OS offers no sandbox (Linux without Landlock, another OS). Set `[sandbox] mode = "required"` to refuse instead. |
| `error[TASK-NOT-READY]` or `TASK-BLOCKED` (16) | `speccoder inspect` shows what blocks what; a skipped task blocks its dependents. |
| `stopped[BUDGET-EXHAUSTED]` (23) | Raise the `[budget]` limit it hit, or split the task in `tasks.md`. |
| `error[PROVIDER-REFUSED]` (21) or `stopped[ACCEPTANCE-UNWRITTEN]` (23), e.g. `credit_balance_exhausted` | The provider refuses the account, not the request; nothing is retried. Top it up or fix the key, and run again. On Bedrock, when access is denied (e.g. an invalid API key), the first fix renews the credential source (a new key, `aws login --profile P`, `aws sso login --profile P`). |
| `another speccoder is working on this project` | Another process is writing the journal (a run, or a decision). Reads still work beside it; wait for it to finish before you decide or run, or watch it with `speccoder report --live`. |
| `waiting for a person: unknown — the last run ended without closing its journal…` | A run was killed (Ctrl-C at a terminal, a closed terminal, out of memory) before it closed the journal. A read cannot repair it; the next `speccoder run`, `task approve` or `task reject` does, and decides as usual. Nothing is reported as not waiting meanwhile. |
| `waiting for a person: unknown — …` (other) | The journal could not be read: damaged, or held alone by a speccoder 0.2.0 or earlier (let that process finish). Nothing is reported as not waiting. |
| `the calls waiting for a person could not be read, so whether another task's attempt holds the worktree is unknown` | A run refuses to start rather than guess. The reason follows: another process writing the journal (wait for it), or one this build cannot open. |
| `MCP tool … changed since it was allowed` | The server changed a tool you allowed. Read it again; approving one call records it as it is now. |
| No colour | Output is not a terminal, `NO_COLOR` or `TERM=dumb` is set, or screen-reader mode is on. `--color always` forces it (not in screen-reader mode). |
| Odd symbols or boxes | The font lacks them: `--ascii`, or `[ui] ascii = true` in [your settings](@/docs/configuration.md#your-settings). |
| `spec tasks` stops: "Template 'tasks-template' was not found…" | A Spec Kit preset is installed and Spec Kit's scripts found no Python with PyYAML (`common.sh` reads a preset's manifest with it; the system `python3` usually lacks it). speccoder gives its commands the installed `specify`'s own interpreter as `SPECKIT_PYTHON_EXECUTABLE` when it imports PyYAML, and never replaces a value you set; `speccoder doctor` warns when none is found. Fix: `python3 -m pip install pyyaml`, or `export SPECKIT_PYTHON_EXECUTABLE=/path/to/a/python-with-pyyaml`. |
| `stopped[EXECUTOR-LOGIN]` (23) | The `[executor]` agent needs signing in; nothing was attempted. Run the login the fix names in your terminal (the app opens one: Open Terminal here), e.g. `copilot login` or `opencode auth login`, then run again. `speccoder doctor --agents` shows every agent's ticks. |
| `[executor]: kind = "claude" is now agent = "claude"` | Agents run over ACP now: write `agent = "claude"` (or `"codex"`). |
| The run says an agent *decides alone* | It is in a mode that decides without asking, or it changed something without asking (an allow rule in its own settings, e.g. Claude Code's `permissions.allow`): those calls never reached speccoder. Remove the rule or set its ask-first mode in its own settings. *Not known whether it asks*: no recording of this version says. |
| Every command the agent runs asks you | A command the agent runs itself runs outside the sandbox, so it always needs your decision; choose *for this task* to let that exact command run on. Agents that use speccoder's terminal get the command policy instead. |
| A run seems stuck | `speccoder report --live` from another terminal shows the step and the last tool call. |

## Getting help

Search the [issues](https://github.com/hupe1980/speccoder/issues) or open one with `speccoder doctor --bug-report`, the command you ran (with `--json` output if possible) and what you expected. Report security problems privately through [security advisories](https://github.com/hupe1980/speccoder/security/advisories/new).
