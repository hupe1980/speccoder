+++
title = "Configuration"
description = "Every section and key of speccoder.toml: checks, model, budget, commands, tests, acceptance, sandbox, mutation, monitor, executor, MCP servers and more."
weight = 120

[extra]
group = "Reference"
+++

speccoder is configured by `speccoder.toml` at the project root. speccoder reads only the **committed** file, so a change is reviewed like code. `speccoder init` writes a commented one. Unknown sections and keys are errors.

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

## Changing it

Edit the file, or use `speccoder config`. Its edits are checked by every parser before they are written, and keep the file's comments, order and layout:

```sh
speccoder config                                   # every key: its value, or its default
speccoder config get budget.max_turns
speccoder config set budget.max_turns 40
speccoder config set format.argv cargo fmt         # words, read by the key's kind
speccoder config set commands.allow '[["cargo", "check"]]'   # or JSON
speccoder config coverage commands.allow_prefix npm run    # what a prefix would newly allow
speccoder config set commands.allow_prefix '[["npm", "run"]]'  # shows that, asks to confirm (--yes)
speccoder config set checks.lint.argv -- cargo clippy -- -D warnings
speccoder config unset budget.max_turns            # back to the default
speccoder config commit                            # commits speccoder.toml, and nothing else
```

**Settings** in the [web interface](@/docs/web.md#settings) does the same. Either way, a change takes effect once committed.

## `[checks.<name>]`

The commands that decide whether nothing regressed. A check's command up to its first option is also something the model may run (`cargo nextest run …` allows `cargo nextest run`); a check that starts with an option allows only itself.

| Key | Default | Meaning |
|---|---|---|
| `argv` | required | Program and arguments, run without a shell |
| `cwd` | `"."` | Working directory, relative to the project root |
| `when_exists` | — | Run only if this path exists; otherwise the check is skipped, never passed |
| `tier` | `"fast"` | `fast` judges every task; `full` (slow linters, audits) runs once after a run ticks the feature's last task, and with `speccoder verify` |
| `timeout_secs` | `600` | Deadline for the whole process group |
| `junit` | — | A JUnit XML report the check writes, for per-test results and regression detection |

## `[model]`

| Key | Default | Meaning |
|---|---|---|
| `provider` | required | `anthropic`, `openai`, or `openai-compatible` (Ollama, vLLM, llama.cpp, LM Studio, TGI, …) |
| `model` | required | For example `claude-sonnet-5-5` or `gpt-5.4-mini` |
| `reasoning_summary` | `true` | Show the model's summary of its reasoning on attempt turns, as its account (never evidence); providers without a way to ask give none |
| `max_output_tokens` | `4096` | Per model turn |
| `max_run_tokens` | `200000` | Per attempt |
| `cache_ttl` | `"5m"` | Anthropic prompt-cache lifetime: `5m` or `1h` |
| `base_url` | — | Required for `openai-compatible` (e.g. `http://localhost:11434/v1`); for `openai`, a gateway in front of the Responses API |
| `api_key_env` | — | `openai-compatible` only: the variable holding the server's key, if it needs one |
| `structured` | `"native"` | `openai-compatible` only: `native` structured output, or `tool` (a forced tool call) |
| `price_input_per_mtok`, `price_output_per_mtok`, `price_cache_read_per_mtok` | — | USD per million tokens, for `speccoder report --cost` and live cost |
| `price_cache_write_per_mtok`, `price_cache_write_1h_per_mtok` | Anthropic: 1.25× and 2× input; others: input | USD per million tokens written to the five-minute and the one-hour cache |

The API key comes from `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`, never from this file. A local model must support tool calls and structured output; the [evaluation suite](@/docs/evaluation.md) tells whether it is good enough.

## `[budget]`

| Key | Default | Meaning |
|---|---|---|
| `max_turns` | `30` | Model turns per attempt |
| `max_tool_calls` | `100` | Tool calls per attempt |
| `max_attempts` | `2` | Attempts per task run; a failed verdict is retried with the verifier's facts |
| `best_of` | `1` | Independent samples per task, the best by evidence kept (at most 10) |

## `[commands]`, `[approvals]`, `[tests]`, `[spec]`

| Key | Default | Meaning |
|---|---|---|
| `[commands] allow` | `[]` | Exact commands the model may run without approval, besides the checks, e.g. `[["cargo", "check"]]`: `cargo check --all` still asks. Always Allow adds to it |
| `[commands] allow_prefix` | `[]` | Command prefixes of two elements or more whose every extension runs without asking, e.g. `[["npm", "run"]]`. Saved only from its [coverage preview](#prefix-lists); a bare program is refused |
| `[approvals] wait` | `"7d"` | How long a call waits for a person (`m`, `h` or `d`) before it is denied as expired; recorded as an expiry, not as anyone's decision |
| `[approvals] budget` | `12` | Prompts a run shows you (`0`–`100`); after them, calls wait together in [Needs You](@/docs/approvals.md#the-asking-budget). `0` never prompts |
| `[tests] frozen` | `"ask"` | Tests that existed before an attempt: `ask` before any change, or `additions` (new lines need no approval; changing or removing lines does) |
| `[spec] format` | detected | `speckit`, `openspec` or `kiro`; `--format` and `SPECCODER_FORMAT` override it. Kiro is recognised and refused as unsupported |

### Prefix lists

`[commands] allow` and `[sandbox] network_allow` match exact argvs. A prefix — every command that starts with it runs without asking — lives in `allow_prefix` / `network_allow_prefix`, and no approval ever writes it. To add one, speccoder first lists every distinct command the project's journal recorded that the prefix would newly allow: how often, when last, how it was decided then, and which a fixed rule (shells, privilege, network tools, remote git) keeps asking for or denying. The prefix is saved only as the confirmation of that list, and not at all while the journal cannot be read in full (another speccoder holds it): the preview then says what it could not read. A prefix added by hand is shown in Settings with the same list. Removing an entry is always possible.

## `[context]`

| Key | Default | Meaning |
|---|---|---|
| `instructions` | `["AGENTS.md"]` | The file names read as the [repository's instructions](@/docs/running.md#repository-instructions), per directory the first that exists, e.g. `["AGENTS.md", "CLAUDE.md"]`; `[]` reads none |

## `[acceptance]`, `[converge]`, `[review]`, `[monitor]`, `[git]`, `[format]`

| Key | Default | Meaning |
|---|---|---|
| `[acceptance] generate` | `true` | Derive acceptance tests from the spec before each attempt |
| `[acceptance] held_out` | `true` | Also write [held-out tests](@/docs/evidence.md#held-out-tests) the implementer never sees; `verified` needs them to pass. Needs `generate`. Costs one author call per task and one run per held-out file |
| `[acceptance] strength` | `true` | Measure the [acceptance strength](@/docs/evidence.md#acceptance-strength) of every `verified` task: one model call and one test run per variant |
| `[acceptance] min_strength` | — | `0.0`–`1.0`: below it the set is strengthened once, then the task stops for you; unset, strength never changes a verdict |
| `[acceptance] near_miss` | `2` | Wrong variants to try (`0`–`3`) |
| `[acceptance] permitted` | `1` | Spec-permitted alternatives to try (`0`–`2`); `0` turns off the [over-strict check](@/docs/evidence.md#over-strict-tests) |
| `[acceptance] strength_timeout_secs` | `900` | Budget for the whole measurement; a measurement cut off is recorded as partial and meets no `min_strength` |
| `[converge] max_rounds` | `3` | Convergence phases a feature may get |
| `[review] required` | `false` | A run waits until every requirement is [approved](@/docs/planning.md#review-the-spec) on its current wording (`REVIEW-PENDING`), and no requirement is read two ways (`REVIEW-AMBIGUOUS`) |
| `[review] oracle_samples` | `2` | Independent author calls [Write Tests](@/docs/planning.md#review-the-tests) makes: two are compared for ambiguity, `1` turns the comparison off |
| `[monitor] enabled` | `false` | Review every passing attempt with the [gaming monitor](@/docs/evidence.md#the-gaming-monitor) |
| `[git] commit` | `"per-task"` | `per-task` commits each passed task; `none` leaves changes uncommitted |
| `[format] argv` | — | A formatter run after each attempt (e.g. `["cargo", "fmt"]`); its changes are part of the attempt and audited. The spec format is `[spec] format` |

## `[bug]`

```toml
[bug]
reproduce_tries = 3                     # 1–5: tries before a bug is "cannot reproduce" (a rejection counts)
test = ["./check.sh", "--", "{file}"]   # how one named test (--test PATH::NAME) runs; default: the toolchain's runner
```

A project setting only: the same keys in your user config are ignored, with a warning naming them. See [bug fixes](@/docs/bugs.md).

## `[sandbox]`

| Key | Default | Meaning |
|---|---|---|
| `mode` | `"auto"` | `auto` sandboxes where the OS supports it and says when it cannot; `required` refuses to run commands unsandboxed; `off` |
| `network` | `"loopback"` | `off`, `loopback` (local test servers work; on Linux the same as `off`), or `on` |
| `network_allow` | `[]` | Exact commands that get the network, e.g. `[["cargo", "fetch"]]` |
| `network_allow_prefix` | `[]` | Prefixes (two elements or more) whose extensions get the network; saved only from its coverage preview |
| `writable` | `[]` | Extra writable paths (`~` is your home directory), e.g. `["~/.m2"]` for Maven |

What the sandbox allows: [the OS sandbox](@/docs/security.md#the-os-sandbox).

## `[attest]`

Signed [attestations](@/docs/attestations.md) of every tick. The signer is part of the judge configuration the statements' policy hash covers.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Write the statements as a git note on each tick commit |
| `signer` | `"git"` | `git` signs with your git signing key (SSH or OpenPGP), `sigstore` keyless with `cosign`, `none` leaves them unsigned |
| `sign_at` | `"tick"` | `tick` signs as each task is ticked; `later` leaves it to `speccoder attest sign` (for keys that need a touch or a passphrase) |
| `[attest.sigstore] identity`, `issuer` | — | The identity and OIDC issuer you sign with, printed in `speccoder pr`'s verify command. A suggestion for the reviewer: `attest verify` trusts only its own flags |

## `[mutation]`

| Key | Default | Meaning |
|---|---|---|
| `tool` | required | `cargo-mutants`, or `command` for any tool writing the mutation-testing JSON report |
| `argv` | — | For `command`: the program; may use `{diff}`, `{out}`, `{files}` and `{files_csv}` |
| `report` | — | For `command`: the report file (relative to the project, or under `{out}`) |
| `mode` | `"report"` | `report` records the result; `require` fails the verdict when mutants survive |
| `timeout_secs` | `900` | Limit for the whole mutation run |
| `suite` | `true` | Run the mutants against the whole suite after a passing verdict; `false` uses the tool for [acceptance strength](@/docs/evidence.md#acceptance-strength) only |
| `acceptance_argv` | — | For `command`: the program that runs the mutants against the acceptance tests alone; `{tests}` is the acceptance command. cargo-mutants needs none |

## `[executor]`

Who makes the attempts; see [external agents](@/docs/integrations.md#external-agents). Without the section, speccoder's own loop.

| Key | Default | Meaning |
|---|---|---|
| `kind` | required | `native`, `claude` (Claude Code), `codex`, or `command` |
| `model` | — | The agent's model |
| `timeout_secs` | `1800` | How long one attempt may take |
| `argv` | — | For `command`: the program, which reads the task on stdin |

## `[mcp.<name>]`

| Key | Default | Meaning |
|---|---|---|
| `command` | required | A stdio MCP server, started in the worktree for each task |
| `allow` | `[]` | Tools called without approval |
| `deny` | `[]` | Tools never offered |

Server names are lowercase letters, digits and `-`, and not `fs`, `proc` or `git`. Their tools appear to the model as `mcp__<server>__<tool>`.

## Your settings

How speccoder looks and notifies you is your setting, not the project's. It lives in `$XDG_CONFIG_HOME/speccoder/config.toml` (else `~/.config/speccoder/config.toml`); a `[ui]` table in `speccoder.toml` is refused (exit 13).

```toml
[ui]
theme = "auto"         # auto | dark | light | high-contrast | daltonized | mono
color = "auto"         # auto | always | never; --color wins, this wins over NO_COLOR
ascii = false          # [ok] [~] [A] … instead of ✓ ~ A …, plain box borders
screen_reader = false  # labelled plain lines, no dashboard

[ui.notify]
on = ["approval", "task_failed", "feature_done", "stalled"]   # the default: all four
when = "unfocused"     # or "always"
method = "auto"        # auto | osc9 | osc777 | bel | command | off
command = []           # with method = "command": program and arguments; the message is appended
stall_after = "15m"    # Ns | Nm | Nh, at least 30s
```

- **Themes:** `auto` uses the terminal's 16 colours and so follows its palette; `dark` and `light` are fixed 24-bit palettes; `daltonized` uses blue and orange instead of green and red; `mono` uses only bold, dim, underline and reverse. Every state also has a word or mark.
- **Notifications:** `auto` sends a desktop notification where the terminal shows one, a bell elsewhere. `command` runs without a shell, for at most five seconds (e.g. `["terminal-notifier", "-title", "speccoder", "-message"]`).
- A value speccoder does not understand is a warning; the default applies.

## Environment

| Variable | Meaning |
|---|---|
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | Model credentials |
| `GH_TOKEN`, `GITHUB_TOKEN` | A GitHub token, instead of `speccoder auth login` ([GitHub](@/docs/github.md)) |
| `SPECCODER_STATE_DIR` | Where worktrees, journals and decisions live (default `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`) |
| `SPECIFY_FEATURE_DIRECTORY` | Spec Kit's feature selection, honoured by every command |
| `SPECCODER_FORMAT` | The spec format, like `--format` |
| `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR`, `TERM` | Colour, after `--color` and `[ui] color`: `NO_COLOR` (non-empty) turns it off; `FORCE_COLOR` or `CLICOLOR_FORCE` turns it on, also when piped; `CLICOLOR=0` or `TERM=dumb` turns it off; otherwise a terminal gets colour |
| `COLUMNS` | The width when the output is not a terminal (default 100) |
| `LC_ALL`, `LC_CTYPE`, `LANG` | A locale that is set and not UTF-8 means ASCII marks |
| `SPECCODER_SCREEN_READER` | Screen-reader mode, like `--screen-reader` |
| `SPECCODER_CALL_ID` | Set *for* commands the model runs: the call's id, stable across a resume |
