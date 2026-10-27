+++
title = "Configuration"
description = "Every section and key of speccoder.toml: checks, model, budget, commands, tests, acceptance, sandbox, mutation, monitor, executor, MCP servers and more."
weight = 120

[extra]
group = "Reference"
+++

speccoder is configured by `speccoder.toml` at the project root. It reads only the **committed** file, so a change is reviewed like code. `speccoder init` writes a commented one. Unknown sections and keys are errors (`CONFIG-INVALID`).

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

Edit the file, or use `speccoder config` (or **Settings** in the [web interface](@/docs/web.md#settings)). Its edits are checked by every parser before they are written, and keep the file's comments, order and layout. A change takes effect once committed.

```sh
speccoder config                                   # every key: its value, or its default
speccoder config get budget.max_turns
speccoder config set budget.max_turns 40
speccoder config set format.argv cargo fmt         # words, read by the key's kind
speccoder config set commands.allow '[["cargo", "check"]]'   # or JSON
speccoder config set checks.lint.argv -- cargo clippy -- -D warnings   # words starting with - after --
speccoder config unset budget.max_turns            # back to the default
speccoder config commit                            # commits speccoder.toml, and nothing else
```

## `[checks.<name>]`

The commands that decide whether nothing regressed. A check's command up to its first option is also a prefix the model may run (`cargo nextest run …` allows `cargo nextest run` with any arguments); when that leaves only the program (`python3 -m pytest …`), the check allows exactly itself.

| Key | Default | Meaning |
|---|---|---|
| `argv` | required | Program and arguments, run without a shell |
| `cwd` | `"."` | Working directory, relative to the project root |
| `when_exists` | — | Run only if this path exists; otherwise the check is skipped, never passed |
| `tier` | `"fast"` | `fast` judges every task; `full` (slow linters, audits) runs once after a run ticks the feature's last task, and with `speccoder verify` |
| `timeout_secs` | `600` | Deadline for the whole process group (1–86400) |
| `junit` | — | A JUnit XML report the check writes, for per-test results and regression detection |
| `list` | — | A command that lists the tests the check would run, one test id or file path per line; speccoder runs it when it cannot otherwise tell whether a changed test file was [collected](@/docs/evidence.md#tests-that-never-ran) |

## `[model]`

| Key | Default | Meaning |
|---|---|---|
| `provider` | required | `anthropic`, `openai`, `openai-compatible` (Ollama, vLLM, llama.cpp, LM Studio, …) or `bedrock` (Amazon Bedrock) |
| `model` | required | For example `claude-sonnet-5-5`; for Bedrock a model id, inference profile id or ARN (`init` writes `global.anthropic.claude-sonnet-5-5`) |
| `region` | — | Required for `bedrock`, refused otherwise: the AWS region calls go to (the journal records it) |
| `reasoning_summary` | `true` | Show the model's summary of its reasoning, as its account (never evidence); providers without one give none |
| `max_output_tokens` | `4096` | Output tokens per model turn |
| `max_run_tokens` | `200000` | Tokens per attempt |
| `cache_ttl` | `"5m"` | Anthropic's prompt-cache lifetime: `5m` or `1h` |
| `base_url` | — | Required for `openai-compatible` (e.g. `http://localhost:11434/v1`); for `openai`, a gateway in front of the Responses API; refused for `bedrock` |
| `api_key_env` | — | `openai-compatible` only: the variable holding the server's key, if it needs one |
| `structured` | `"native"` | `openai-compatible` only: `native` structured output, or `tool` (a forced tool call) |
| `price_input_per_mtok`, `price_output_per_mtok` | — | USD per million tokens, for `speccoder report --cost` and live cost; without both, cost is tokens only |
| `price_cache_read_per_mtok` | the input price | USD per million tokens read from the cache |
| `price_cache_write_per_mtok`, `price_cache_write_1h_per_mtok` | Claude (Anthropic, or on Bedrock): 1.25× and 2× input; others: input | USD per million tokens written to the five-minute and the one-hour cache |

The API key comes from the environment, never from this file. A local model must support tool calls and structured output; the [evaluation suite](@/docs/evaluation.md) tells whether it is good enough.

### Amazon Bedrock

Preview: an attempt fails (`RUN-FAILED`, Bedrock's `ValidationException` on `toolResult…json`) when the model calls a tool whose answer is a list, until the embedded agentplane sends such answers as Bedrock expects.

```toml
[model]
provider = "bedrock"
model = "global.anthropic.claude-sonnet-5-5"
region = "eu-central-1"
```

Credentials come from the AWS credential chain where you start speccoder: a Bedrock API key in `AWS_BEARER_TOKEN_BEDROCK` (it wins over everything else), or a profile (`AWS_PROFILE`, else `default`) — an `aws login` session, SSO, `credential_process`, static keys or a role — or keys, web identity or a container role from the environment. `speccoder doctor` names the source it will use and the region, never a value. Claude models get prompt caching; other families run without it. For exact costs, state all four prices (unstated cache-write prices for Claude follow Anthropic's rule).

`speccoder config set model.provider bedrock`, then `speccoder config set model.region <region>`; the file is refused until the region is set.

## `[budget]`

| Key | Default | Meaning |
|---|---|---|
| `max_turns` | `30` | Model turns per attempt (1–1000) |
| `max_tool_calls` | `100` | Tool calls per attempt |
| `max_attempts` | `2` | Attempts per task run (1–20); a failed verdict is retried with the verifier's facts |
| `best_of` | `1` | Independent samples per task, the best by evidence kept (1–10) |

## `[commands]`, `[approvals]`, `[tests]`, `[spec]`

| Key | Default | Meaning |
|---|---|---|
| `[commands] allow` | `[]` | Exact commands the model may run without asking, besides the checks, e.g. `[["cargo", "check"]]`: `cargo check --all` still asks. Always Allow adds to it |
| `[commands] allow_prefix` | `[]` | Prefixes of two elements or more whose every extension runs without asking, e.g. `[["npm", "run"]]`. Saved only from its [coverage preview](#prefix-lists) |
| `[approvals] wait` | `"7d"` | How long a call waits for a person (`m`, `h` or `d`) before it is denied as expired; recorded as an expiry, not as anyone's decision |
| `[approvals] budget` | `12` | Prompts a run shows you (`0`–`100`); after them, calls wait together in [Needs You](@/docs/approvals.md#the-asking-budget). `0` never prompts |
| `[tests] frozen` | `"ask"` | Tests that existed before an attempt: `ask` before any change, or `additions` (new lines need no approval; changing or removing lines does) |
| `[spec] format` | detected | `speckit`, `openspec` or `kiro`; `--format` and `SPECCODER_FORMAT` override it. Kiro is recognised and refused |

### Prefix lists

`[commands] allow` and `[sandbox] network_allow` match exact argvs. A prefix in `allow_prefix` / `network_allow_prefix` lets every command that starts with it run without asking, including commands nobody has seen, so no approval ever writes one and a bare program is refused.

```sh
speccoder config coverage commands.allow_prefix npm run     # the preview only
speccoder config set commands.allow_prefix npm run          # shows it, then: type yes
```

- `config coverage KEY PREFIX…` lists every distinct command the project's journal recorded that the prefix would newly allow: how often, when last, how it was decided, and which a fixed rule (shells, privilege, network tools, remote git) still asks for or denies.
- `config set` (and **Save Prefix** in Settings) saves the prefix only as the confirmation of that list: `yes` at a terminal, `--yes` without asking; otherwise it exits needing a person. It is refused when the preview changed since it was shown, and while the journal cannot be read in full.
- A prefix added by hand shows the same list in Settings (**Coverage**). Removing an entry needs no preview.

## `[context]`

| Key | Default | Meaning |
|---|---|---|
| `instructions` | `["AGENTS.md"]` | The file names read as the [repository's instructions](@/docs/running.md#repository-instructions), per directory the first that exists, e.g. `["AGENTS.md", "CLAUDE.md"]`; `[]` reads none |

## `[acceptance]`, `[converge]`, `[review]`, `[monitor]`, `[git]`, `[format]`

| Key | Default | Meaning |
|---|---|---|
| `[acceptance] generate` | `true` | Derive acceptance tests from the spec before each attempt |
| `[acceptance] held_out` | `true` | Also write [held-out tests](@/docs/evidence.md#held-out-tests) the implementer never sees; `verified` needs them to pass. Needs `generate`; costs one author call per task and one run per held-out file |
| `[acceptance] strength` | `true` | Measure the [acceptance strength](@/docs/evidence.md#acceptance-strength) of every `verified` task: one model call and one test run per variant |
| `[acceptance] min_strength` | — | `0.0`–`1.0`: below it the set is strengthened once, then the task stops for you; unset, strength never changes a verdict |
| `[acceptance] near_miss` | `2` | Wrong variants to try (`0`–`3`) |
| `[acceptance] permitted` | `1` | Spec-permitted alternatives to try (`0`–`2`); `0` turns off the [over-strict check](@/docs/evidence.md#over-strict-tests) |
| `[acceptance] strength_timeout_secs` | `900` | Budget for the whole measurement; a measurement cut off is partial and meets no `min_strength` |
| `[converge] max_rounds` | `3` | Convergence phases a feature may get (1–20) |
| `[review] required` | `false` | A run waits until every requirement is [approved](@/docs/planning.md#review-the-spec) on its current wording (`REVIEW-PENDING`) and none is read two ways (`REVIEW-AMBIGUOUS`) |
| `[review] oracle_samples` | `2` | Independent author calls [writing tests at review](@/docs/planning.md#review-the-tests) makes: two are compared for ambiguity, `1` turns the comparison off |
| `[monitor] enabled` | `false` | Review every passing attempt with the [gaming monitor](@/docs/evidence.md#the-gaming-monitor) |
| `[git] commit` | `"per-task"` | `per-task` commits each passed task; `none` leaves changes uncommitted |
| `[format] argv` | — | A formatter run after each attempt (e.g. `["cargo", "fmt"]`); its changes are part of the attempt and audited |

## `[bug]`

[`speccoder bug`](@/docs/bugs.md). Project settings only: the same keys in your user config are ignored, with a warning naming them.

| Key | Default | Meaning |
|---|---|---|
| `reproduce_tries` | `3` | Tries of the reproduction author (`1`–`5`) before the bug goes to you as cannot reproduce; a rejection counts as one |
| `test` | the toolchain's runner | How one named test (`--test PATH::NAME`) runs, with `{file}` and `{name}` filled in, e.g. `["./check.sh", "--", "{file}"]` |

## `[sandbox]`

| Key | Default | Meaning |
|---|---|---|
| `mode` | `"auto"` | `auto` sandboxes where the OS supports it and says when it cannot; `required` refuses to run commands unsandboxed; `off` |
| `network` | `"loopback"` | `off`, `loopback` (local test servers work; on Linux the same as `off`), or `on` |
| `network_allow` | `[]` | Exact commands that get the network, e.g. `[["cargo", "fetch"]]` |
| `network_allow_prefix` | `[]` | Prefixes (two elements or more) whose extensions get the network; saved only from its [coverage preview](#prefix-lists) |
| `writable` | `[]` | Extra writable paths (`~` is your home directory), e.g. `["~/.m2"]` for Maven |

What the sandbox allows: [the OS sandbox](@/docs/security.md#the-os-sandbox).

## `[worktree]`

What a run's worktree gets that git never holds; see [dependency folders](@/docs/running.md#dependency-folders) and [copied files](@/docs/running.md#copied-files).

| Key | Default | Meaning |
|---|---|---|
| `prepare` | `"auto"` | `auto` installs a worktree's Node and uv dependencies (locked, offline) before the baseline; `off` never |
| `include` | `[]` | Git-ignored files copied from your checkout into each worktree, `.gitignore` patterns, e.g. `[".env"]`; joined with `.worktreeinclude`'s. Never committed; secret-named values masked ([copied files](@/docs/running.md#copied-files)) |

`speccoder init` proposes `include` with your checkout's git-ignored `.env` files, or says that `.worktreeinclude` already names them.

## `[attest]`

Signed [attestations](@/docs/attestations.md) of every tick. The signer is part of the judge configuration the statements' policy hash covers.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Write the statements as a git note on each tick commit |
| `signer` | `"git"` | `git` signs with your git signing key (SSH or OpenPGP), `sigstore` keyless with `cosign`, `none` leaves them unsigned |
| `sign_at` | `"tick"` | `tick` signs as each task is ticked; `later` leaves it to `speccoder attest sign` (for keys that need a touch or a passphrase) |
| `[attest.sigstore] identity`, `issuer` | — | The identity and OIDC issuer you sign with, suggested in `speccoder pr`'s verify command; `attest verify` trusts only its own flags |

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
| `command` | required | A stdio MCP server's program and arguments, started in the worktree for each task |
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

- **Themes:** `auto` uses the terminal's 16 colours, following its palette; `dark` and `light` are fixed 24-bit palettes; `daltonized` uses blue and orange instead of green and red; `mono` uses only bold, dim, underline and reverse.
- **Notifications:** `auto` sends a desktop notification where the terminal shows one, a bell elsewhere. `command` runs without a shell, for at most five seconds (e.g. `["terminal-notifier", "-title", "speccoder", "-message"]`).
- A value speccoder does not understand is a warning; the default applies.

## Environment

| Variable | Meaning |
|---|---|
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | Model credentials |
| `AWS_BEARER_TOKEN_BEDROCK`, `AWS_PROFILE`, `AWS_REGION`, … | Bedrock's credentials and region, as the AWS tools read them; `init` takes the region from `AWS_REGION`, `AWS_DEFAULT_REGION`, else the profile |
| `GH_TOKEN`, `GITHUB_TOKEN` | A GitHub token, instead of `speccoder auth login` ([GitHub](@/docs/github.md)) |
| `SPECCODER_STATE_DIR` | Where worktrees, journals and decisions live (default `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`) |
| `SPECIFY_FEATURE_DIRECTORY` | Spec Kit's feature selection, honoured by every command |
| `SPECCODER_FORMAT` | The spec format, like `--format` |
| `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR`, `TERM` | Colour, after `--color` and `[ui] color`: `NO_COLOR` (non-empty) turns it off; `FORCE_COLOR` or `CLICOLOR_FORCE` turns it on, also when piped; `CLICOLOR=0` or `TERM=dumb` turns it off; otherwise a terminal gets colour |
| `COLUMNS` | The width when the output is not a terminal (default 100) |
| `LC_ALL`, `LC_CTYPE`, `LANG` | A locale that is set and not UTF-8 means ASCII marks, unless `[ui] ascii` says otherwise |
| `SPECCODER_SCREEN_READER` | Screen-reader mode, like `--screen-reader` |
| `SPECCODER_CALL_ID` | Set *for* commands the model runs: the call's id, stable across a resume |
