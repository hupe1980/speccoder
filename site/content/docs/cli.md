+++
title = "CLI reference"
description = "Every speccoder command and its options, the global flags, the JSON output contract and the exit codes."
weight = 130

[extra]
group = "Reference"
nav = "CLI reference"
+++

Every command takes `--json` (one JSON object on stdout), `--events` (JSON Lines as it works, ending in that object), `-q`/`--quiet` (only stops, finishes, resumes and what needs you), `-v`/`--verbose` (also durations, skipped stages and the last lines of what failed), `--format <speckit|openspec|kiro>` (the project's spec format, also `SPECCODER_FORMAT`; default: `[spec] format`, else detected), `--color auto|always|never`, `--ascii` (ASCII marks and borders), `--screen-reader` (labelled plain lines, no dashboard) and `--help`. How output looks is [your setting](@/docs/configuration.md#your-settings), never the project's. Commands that work on a project take an optional `PATH` inside it (default: the current directory) and `--feature <name or number>` (for an [OpenSpec](@/docs/openspec.md) change also `--change <id>`).

Every stop and finish ends with a `Next:` block — the commands that continue, copy-pasteable, the same in `--json` as `next` (`[{"command", "why"}]`). Every failure and stop names a [code](@/docs/codes.md) with what to run: `error[CODE]`, `stopped[CODE]` when the run waits for you, `warning[CODE]` when it worked with less evidence.

## Plan and set up

| Command | What it does |
|---|---|
| `speccoder init [--provider P] [--model M] [--force]` | Detect the project's checks and write `speccoder.toml` |
| `speccoder spec [SKILL] [ARGS…] [--no-prompt]` | List the format's installed planning skills, or run one (Spec Kit: `specify`, `clarify`, `plan`, `tasks`, …; OpenSpec: `propose`, `explore`, `update-change`). `speccoder skill` is the same |
| `speccoder spec archive [CHANGE] [--yes] [--allow-unverified TASK]…` | Archive an OpenSpec change: OpenSpec's checks first, then the diff, then your yes, then one commit |
| `speccoder inspect` | Show the features, artifacts and tasks speccoder sees, with what blocks what |
| `speccoder verify [--tier fast\|full] [--against FILE]` | Run the configured checks; compare with a saved result |
| `speccoder workflow install [--force]` | Add a speccoder step type and an example Spec Kit workflow |

## Run

| Command | What it does |
|---|---|
| `speccoder run [--task T] [--max-tasks N] [--parallel N] [--converge] [--no-prompt] [--ignore-checklists]` | Work through the ready tasks, or one task |
| `speccoder run --in-place [--task T] …` | The same in your checkout on `speccoder/<feature>`, for git-ignored specs; switches back at the end |
| `speccoder run --in-place --return` | Leave an interrupted in-place run: save its work, switch back, run nothing |
| `speccoder accept --in-place …`, `speccoder converge --in-place` | The same commands in your checkout, for git-ignored specs |
| `speccoder converge` | Assess a finished feature against its spec and append what remains |
| `speccoder propose --task T` | Ask the model how it would do one task; nothing is executed |
| `speccoder worktree` | Create or reuse the feature's worktree and print its path |

## Watch and review

| Command | What it does |
|---|---|
| `speccoder status` | What the current or last run is doing |
| `speccoder tui` | Live dashboard with the approval queue |
| `speccoder report [--markdown]` | Every task and the evidence behind its tick; `--markdown` for pull requests and CI summaries |
| `speccoder cost` | Tokens, and money with prices configured, per task |
| `speccoder replay <RUN>` | Re-derive a recorded run from the journal, without a model |
| `speccoder attest verify [REV\|RANGE] [--allowed-signers FILE] [--certificate-identity I --certificate-oidc-issuer O] [--require-signed]` | Re-check every tick's [attestation](@/docs/attestations.md) against its commit, and its signatures against your trust |
| `speccoder attest sign [REV\|RANGE]` | Sign the statements ticks left unsigned (`[attest] sign_at = "later"`, or no key yet) |

## Decide

| Command | What it does |
|---|---|
| `speccoder approvals` | Calls waiting for a decision, and the decisions made, with who made them |
| `speccoder approve --task T [--call ID] [--all] [--scope once\|task\|run] [--note "…"]` | Approve a waiting call (its class for the task by default); no model needed; the next `run` continues the attempt from that call |
| `speccoder reject --task T [--call ID] [--all] --reason R` | Reject it; the attempt continues and the model is told why |
| `speccoder accept --task T --reason R` | Tick a task on your word (`attested`) |
| `speccoder skip --task T [--reason R] [--undo]` | Skip a task; its dependents stay blocked |
| `speccoder nudge --task T [--note N] [--clear]` | Leave a note the next attempts read |

## Share and integrate

| Command | What it does |
|---|---|
| `speccoder pr [--remote R] [--base B] [--yes] [--dry-run] [--force]` | Push the branch and its attestation notes and open a draft pull request with the evidence; `--force` hands off although the full-tier checks failed |
| `speccoder acp` | Serve the Agent Client Protocol on stdio, for editors |

## Measure

| Command | What it does |
|---|---|
| `speccoder eval [DIR] [--case C] [--repeat N] [--keep DIR] [--executor KIND]` | Run the evaluation suite against hidden oracles |
| `speccoder bench import FILE --repos DIR --out DIR [--limit N] [--test-cmd CMD]` | Turn SWE-bench-format instances into evaluation cases |

## Explain and schemas

| Command | What it does |
|---|---|
| `speccoder explain [CODE] [--list]` | What a code means, why it happens and how to fix it, offline |
| `speccoder schema [NAME]` | The JSON Schema of an output: `result`, `event`, or a payload (`run`, `report`, `attest`, …); without a name, the list |

## JSON output

With `--json` every command prints exactly one object on stdout and no progress lines — also when the arguments are wrong:

```json
{ "schema_version": 2, "command": "run", "status": "needs_human", "exit_code": 23,
  "message": "T004 needs approval: cargo add unicode-normalization",
  "code": "NEEDS-APPROVAL",
  "fix": ["speccoder approvals", "speccoder approve --task T004", "speccoder reject --task T004 --reason \"…\""],
  "docs": "https://hupe1980.github.io/speccoder/docs/codes/#needs-approval",
  "next": [{ "command": "speccoder approvals", "why": "see what waits" }],
  "payload": "run", "data": { … } }
```

- `status` is the exit code's name in `snake_case`.
- `code`, `fix` and `docs` are present on every result that is not plainly `ok`, and on an `ok` with less evidence (`REGRESSION-ONLY`); the [codes page](@/docs/codes.md) lists them all. Branch on `code`, never on `message`.
- `payload` names the schema `data` follows.
- `--help` and `--version` always print text.

Every output has a JSON Schema (draft 2020-12), generated from the types that print it and published at `https://hupe1980.github.io/speccoder/schemas/v2/<name>.json` — `result.json`, `event.json`, and one per payload. `speccoder schema <name>` prints the same offline. Fields are only added within a version; ignore fields you do not know. A change that renames, removes or changes the meaning of a field raises `schema_version`.

## Events

With `--events` stdout carries JSON Lines as the command works, and nothing else; stderr stays quiet (an approval prompt still appears when you are at a terminal):

```jsonl
{"v":2,"seq":1,"ts":"2026-10-24T10:00:00.000Z","type":"session.started","speccoder":"0.1.0","command":"run","capabilities":["events_v2","approvals_inline"]}
{"v":2,"seq":2,"ts":"…","type":"task.started","feature":"001-demo","task":"T001","title":"…"}
{"v":2,"seq":3,"ts":"…","type":"stage.started","task":"T001","stage":"baseline","detail":null}
{"v":2,"seq":9,"ts":"…","type":"tool_call","task":"T001","turn":3,"tool":"run_command","target":"cargo test","decision":"allow","ok":true,"tail":[]}
{"v":2,"seq":14,"ts":"…","type":"task.finished","task":"T001","outcome":"verified","message":"…","run":"run_01J…","input_tokens":40250,…}
{"v":2,"seq":15,"ts":"…","type":"result","result":{ …what --json prints… }}
```

| Type | When |
|---|---|
| `session.started` | first line: version, command, `capabilities` — feature-detect on these |
| `run.started` | a feature run begins, with its plan |
| `task.started`, `task.resumed`, `task.finished` | a task's run begins, picks up a killed attempt, ends (verdict, run id, tokens) |
| `stage.started`, `stage.finished` | `baseline`, `red`, `attempt`, `verify`, `audit`, `mutation`, `monitor`, `commit` |
| `tool_call` | a call the model made, with the policy's decision |
| `approval.requested`, `approval.decided` | a call waits for a person; who decided, and the scope |
| `usage` | tokens so far (at most once a second per task) |
| `notice` | anything else worth a line |
| `result` | last line, exactly once |

`seq` rises by one per line, so a gap means a lost line; a stream without `result` was interrupted. If the reader goes away, speccoder stops writing and finishes the command. `--events` with `--json` is a usage error (`USAGE-CONFLICT`).

## Exit codes

| Code | Status | Meaning |
|---|---|---|
| 0 | `ok` | Success |
| 1 | `error` | Internal error |
| 2 | `usage_error` | Invalid arguments |
| 10 | `not_a_project` | No Spec Kit project found |
| 11 | `diagnostics_error` | The Spec Kit artifacts have errors |
| 12 | `selection_needed` | Feature selection needed or failed |
| 13 | `config_error` | `speccoder.toml` missing or invalid, a missing key, or no applicable check |
| 14 | `git_precondition` | No repository or commits, uncommitted Spec Kit files, a dirty worktree, or an in-place run that cannot start (`DIRTY-CHECKOUT`, `UNBORN-HEAD`, `CONFIG-NOT-TRACKED`, `BRANCH-IN-WORKTREE`, `IN-PLACE-ACTIVE`, `IN-PLACE-SESSION`, `MERGE-CONFLICT`) |
| 15 | `not_found` | No such task, run or pending call |
| 16 | `task_not_ready` | The task is done, or blocked by open dependencies |
| 20 | `verification_failed` | A check or acceptance test failed |
| 21 | `run_failed` | The model call or the run failed |
| 22 | `replay_diverged` | Replay differed from the recorded run |
| 23 | `needs_human` | Stopped for a person: blocked, approval, denials, budget, incomplete, not demonstrated, suspected gaming, interrupted, or an in-place run stopped (`INTENT-CHANGED`, `JUDGE-CHANGED`, `CHECKOUT-CHANGED`, `GIT-CHANGED`, `RETURN-BLOCKED`) |
| 24 | `eval_failed` | The evaluation found a false completion or a cheat |
| 25 | `not_replayable` | This build cannot re-derive the recorded run |
| 26 | `attestation_failed` | `attest verify`: an attestation is missing, malformed, mismatched, or not signed by someone you trust (unsigned only with `--require-signed`) |
