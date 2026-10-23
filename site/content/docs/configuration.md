+++
title = "Configuration"
description = "Every section and key of speccoder.toml: checks, model, budget, commands, tests, acceptance, sandbox, mutation, monitor, executor, MCP servers and more."
weight = 120

[extra]
group = "Reference"
+++

speccoder is configured by `speccoder.toml` at the project root. It must be **committed** — speccoder reads committed files only — so a change to it is reviewed like code. `speccoder init` writes a commented one; unknown sections and keys are errors, so a typo never goes unnoticed.

```toml
[checks.test]
argv = ["cargo", "nextest", "run", "--workspace", "--profile", "speccoder"]
junit = "target/nextest/speccoder/junit.xml"

[checks.clippy]
argv = ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]
tier = "full"

[model]
provider = "anthropic"
model = "claude-sonnet-5-5"

[commands]
allow = [["cargo", "test"], ["cargo", "check"]]

[sandbox]
network_allow = [["cargo", "fetch"]]
```

## `[checks.<name>]`

The commands that decide whether nothing regressed. A check's command up to its first option is also something the model may run (`cargo nextest run …` allows `cargo nextest run`); a check that starts with an option allows only itself.

| Key | Default | Meaning |
|---|---|---|
| `argv` | required | Program and arguments, run without a shell |
| `cwd` | `"."` | Working directory, relative to the project root |
| `when_exists` | — | Run only if this path exists; otherwise the check is skipped, never "passed" |
| `tier` | `"fast"` | `fast` checks judge every task; `full` checks (slow linters, audits) run once after a run ticks the feature's last task, and with `speccoder verify` |
| `timeout_secs` | `600` | Deadline for the whole process group |
| `junit` | — | A JUnit XML report the check writes, for per-test results and regression detection |

## `[model]`

| Key | Default | Meaning |
|---|---|---|
| `provider` | required | `anthropic`, `openai`, or `openai-compatible` (Ollama, vLLM, llama.cpp, LM Studio, TGI, …) |
| `model` | required | For example `claude-sonnet-5-5` or `gpt-5.4-mini` |
| `max_output_tokens` | `4096` | Per model turn |
| `max_run_tokens` | `200000` | Per attempt |
| `cache_ttl` | `"5m"` | Anthropic prompt-cache lifetime: `5m` or `1h` |
| `base_url` | — | Required for `openai-compatible` (e.g. `http://localhost:11434/v1`); for `openai`, a gateway in front of the Responses API |
| `api_key_env` | — | `openai-compatible` only: the variable holding the server's key, if it wants one |
| `structured` | `"native"` | `openai-compatible` only: `native` structured output, or `tool` (a forced tool call; for steps without tools) |
| `price_input_per_mtok`, `price_output_per_mtok`, `price_cache_read_per_mtok` | — | USD per million tokens, for `speccoder cost` and live cost |

The key comes from `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` — never from this file. A local model must support tool calls and structured output; whether it is good enough is a question for the [evaluation suite](@/docs/evaluation.md).

## `[budget]`

| Key | Default | Meaning |
|---|---|---|
| `max_turns` | `30` | Model turns per attempt |
| `max_tool_calls` | `100` | Tool calls per attempt |
| `max_attempts` | `2` | Attempts per task run; a failed verdict is retried with the verifier's facts |
| `best_of` | `1` | Independent samples per task, the best by evidence kept (at most 10) |

## `[commands]`

| Key | Default | Meaning |
|---|---|---|
| `allow` | `[]` | Argv prefixes the model may run without approval, besides the checks, e.g. `[["cargo", "check"]]` |

Keep the list narrow: an allowed command runs whatever the model passes after the prefix.

## `[tests]`

| Key | Default | Meaning |
|---|---|---|
| `frozen` | `"ask"` | Tests that existed before an attempt: `ask` before any change, or `additions` — new lines may be added without asking; changing or removing lines still asks |

## `[spec]`

| Key | Default | Meaning |
|---|---|---|
| `[spec] format` | detected | The spec format: `speckit`, `openspec` or `kiro`. `--format` and `SPECCODER_FORMAT` override it. This version runs Spec Kit; OpenSpec and Kiro are recognised and refused as not yet supported |

## `[acceptance]`, `[converge]`, `[monitor]`, `[git]`, `[format]`

| Key | Default | Meaning |
|---|---|---|
| `[acceptance] generate` | `true` | Derive acceptance tests from the spec before each attempt |
| `[acceptance] held_out` | `true` | Also write [held-out tests](@/docs/evidence.md#held-out-tests) the implementer never sees; `verified` needs them to pass. No effect without `generate`. Costs one more author call per task and one run per held-out file |
| `[converge] max_rounds` | `3` | Convergence phases a feature may get |
| `[monitor] enabled` | `false` | Review every passing attempt with the [gaming monitor](@/docs/evidence.md#the-gaming-monitor) |
| `[git] commit` | `"per-task"` | `per-task` commits each passed task; `none` leaves changes uncommitted |
| `[format] argv` | — | A formatter speccoder runs after each attempt (e.g. `["cargo", "fmt"]`); its changes are part of the attempt and audited. Not the spec format: that is `[spec] format` |

## `[sandbox]`

| Key | Default | Meaning |
|---|---|---|
| `mode` | `"auto"` | `auto` sandboxes where the OS supports it and says when it cannot; `required` refuses to run commands unsandboxed; `off` |
| `network` | `"loopback"` | `off`, `loopback` (local test servers work; on Linux the same as `off`), or `on` |
| `network_allow` | `[]` | Command prefixes that get the network, e.g. `[["cargo", "fetch"]]` |
| `writable` | `[]` | Extra writable paths (`~` is your home directory), e.g. `["~/.m2"]` for Maven |

See [security and the sandbox](@/docs/security.md#the-os-sandbox) for what the sandbox allows.

## `[mutation]`

| Key | Default | Meaning |
|---|---|---|
| `tool` | — | `cargo-mutants`, or `command` for any tool writing the mutation-testing JSON report |
| `argv` | — | For `command`: the program; may use `{diff}`, `{out}`, `{files}` and `{files_csv}` |
| `report` | — | For `command`: the report file (relative to the project, or under `{out}`) |
| `mode` | `"report"` | `report` records the result; `require` fails the verdict when mutants survive |
| `timeout_secs` | `900` | Limit for the whole mutation run |

## `[executor]`

| Key | Default | Meaning |
|---|---|---|
| `kind` | `"native"` | Who makes the attempts: `native`, `claude` (Claude Code), `codex`, or `command` |
| `model` | — | The agent's model |
| `timeout_secs` | `1800` | How long one attempt may take |
| `argv` | — | For `command`: the program, which reads the task on stdin |

See [external agents](@/docs/integrations.md#external-agents).

## `[mcp.<name>]`

| Key | Default | Meaning |
|---|---|---|
| `command` | required | A stdio MCP server, started in the worktree for each task |
| `allow` | `[]` | Tools called without approval |
| `deny` | `[]` | Tools never offered |

Server names are lowercase letters, digits and `-` (not `fs`, `proc` or `git`). Their tools appear to the model as `mcp__<server>__<tool>`.

## Your settings

How speccoder gets *your* attention is yours, not the project's: it lives in `$XDG_CONFIG_HOME/speccoder/config.toml` (else `~/.config/speccoder/config.toml`), never in `speccoder.toml` — a `[ui]` table there is refused (exit 13), naming where it belongs.

```toml
[ui.notify]
on = ["approval", "task_failed", "feature_done", "stalled"]   # the default: all four
when = "unfocused"     # or "always"
method = "auto"        # auto | osc9 | osc777 | bel | command | off
command = []           # with method = "command": the program and arguments; the message is appended
stall_after = "15m"    # Ns | Nm | Nh, at least 30s
```

`auto` uses a desktop notification where the terminal shows one and a bell elsewhere. `command` runs the program without a shell, at most five seconds (for example `["terminal-notifier", "-title", "speccoder", "-message"]`). A value speccoder does not understand is a warning, and the default is used.

## Environment

| Variable | Meaning |
|---|---|
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | Model credentials |
| `SPECCODER_STATE_DIR` | Where speccoder keeps worktrees, journals and decisions (default `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`) |
| `SPECIFY_FEATURE_DIRECTORY` | Spec Kit's feature selection, honoured by every command |
| `SPECCODER_FORMAT` | The spec format, like `--format` |
| `SPECCODER_CALL_ID` | Set *for* commands the model runs: the call's id, stable across a resume |
