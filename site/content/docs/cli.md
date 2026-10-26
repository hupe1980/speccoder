+++
title = "CLI reference"
description = "Every speccoder command and its options, the global flags, the JSON output contract and the exit codes."
weight = 130

[extra]
group = "Reference"
nav = "CLI reference"
+++

`speccoder --help` shows the daily commands, grouped as below; `speccoder help advanced` the rest; `speccoder help <command>` one command. Bare `speccoder` opens the dashboard at a terminal and prints the report otherwise; at a terminal, `speccoder run` runs inside the [dashboard](@/docs/running.md#the-dashboard).

Every command takes:

| Option | Meaning |
|---|---|
| `-C PATH`, `--project PATH` | The project: a path inside it (default: the current directory) |
| `--feature NAME` (`--change`) | The feature: directory name or numeric prefix; an [OpenSpec](@/docs/openspec.md) change's id |
| `--json` | One JSON object on stdout ([JSON output](#json-output)) |
| `--events` | JSON Lines as it works, ending in that object ([events](#events)) |
| `-q`, `-v` | Only stops, finishes, resumes and what needs you; or also durations, skipped stages and the tails of what failed |
| `--color auto\|always\|never`, `--ascii`, `--screen-reader` | How output looks; also [your settings](@/docs/configuration.md#your-settings) |
| `--plain` | Lines instead of the [dashboard](@/docs/running.md#the-dashboard): `run` prints its progress, bare `speccoder` the report |
| `--format speckit\|openspec\|kiro` | The spec format (also `SPECCODER_FORMAT`; default: `[spec] format`, else detected) |

Every stop and finish ends with a `Next:` block of copy-pasteable commands (`next` in JSON). Every failure and stop names a [code](@/docs/codes.md): `error[CODE]`, `stopped[CODE]` when the run waits for you, `warning[CODE]` when it worked with less evidence.

## Start

| Command | What it does |
|---|---|
| `speccoder init [--provider anthropic\|openai] [--model M] [--force] [--no-verify] [--yes]` | Detect the checks, write `speccoder.toml`, run the checks on the untouched code, and at a terminal offer to commit the file. `--yes` asks nothing and commits nothing |
| `speccoder doctor [--bug-report]` | Check git, the project and its spec tool, the configuration, the model key, the sandbox, the state directory, the signing key, the repository instructions and the look; changes nothing. `--bug-report`: a redacted Markdown bundle for an issue |
| `speccoder config [get KEY \| set KEY VALUE… [--yes] \| unset KEY \| commit]` | Every key of `speccoder.toml` with its value or default; change one, checked by every parser, comments kept; commit the file alone ([configuration](@/docs/configuration.md#changing-it)). A prefix list (`commands.allow_prefix`, `sandbox.network_allow_prefix`) is written only after its preview is confirmed |
| `speccoder config coverage KEY ARGV…` | What a prefix would newly allow: every command the project's journal recorded under it, how often, when, how decided ([prefix lists](@/docs/configuration.md#prefix-lists)) |
| `speccoder auth [login [--with-token] \| status \| logout] [--host H]` | GitHub: sign in with a short code through speccoder's GitHub App (or a token on stdin), kept in your keychain; who speccoder is there; sign out ([GitHub](@/docs/github.md)) |

## Plan

| Command | What it does |
|---|---|
| `speccoder feature [NAME \| NUMBER] [--clear]` | List the features with their progress, or make one [current](@/docs/planning.md#the-current-feature): the one commands use without `--feature` |
| `speccoder spec [SKILL] [ARGS…] [--no-prompt]` | List the format's planning skills, or run one (Spec Kit: `specify`, `clarify`, `plan`, `tasks`, …; OpenSpec: `propose`, `explore`, …) |
| `speccoder spec archive [CHANGE] [--yes] [--allow-unverified TASK]…` | Archive an OpenSpec change: OpenSpec's checks, the diff, your yes, one commit |
| `speccoder review` | Every requirement and its review state ([review](@/docs/planning.md#review-the-spec)) |
| `speccoder review approve [ID…] [--comment C]` | Approve requirements on their current wording; none named: every unapproved one |
| `speccoder review request ID… --comment C` | Request changes |
| `speccoder review answer Q ANSWER` | Answer an open question in place |
| `speccoder review edit ID WORDING [--approve]` | Reword a requirement; its approval lapses unless `--approve` |
| `speccoder review clear ID…` | Remove the decision |
| `speccoder issue [list]` | The repository's open GitHub issues |
| `speccoder issue start N [--no-prompt] [--comment \| --no-comment] [--assign]` | Specify a feature from an issue, optionally commenting on and assigning it |
| `speccoder issue comment N [--assign]` | Tell the issue that the current feature is specified from it |

## Work

| Command | What it does |
|---|---|
| `speccoder run [--task T] [--max-tasks N] [--parallel N] [--converge] [--no-prompt] [--ignore-checklists]` | Work through the ready tasks, or one task; resumes where it stopped. `--parallel` (1–8) runs ready `[P]` tasks at once |
| `speccoder run --task T --fresh-acceptance` | Set the task's held attempt aside and write its acceptance tests anew |
| `speccoder run --allow-failing-baseline` | Attempt although a check cannot run on the untouched code (`CHECK-BROKEN`); that check can never judge the task |
| `speccoder run --in-place [--return]` | The same in your checkout on `speccoder/<feature>`, switching back at the end; `--return` leaves an interrupted in-place run |
| `speccoder do "…" [--check "…"]… [--no-run] [--no-prompt]` | A small change: a story, an approved requirement and a task, committed, then run ([quick changes](@/docs/running.md#quick-changes)) |
| `speccoder bug "…" --expected "…"… [--unchanged "…"]… [--issue N] [--test PATH::NAME]… [--candidate PATCH\|pr:N] [--slug S] [--no-run] [--no-prompt]` | A bug in your words: reproduced by a test that fails by an assertion first, confirmed by you once, fixed with the revert rejected ([bug fixes](@/docs/bugs.md)) |
| `speccoder bug confirm <unit>/<task> [--digest D] [--reject --reason R] [--no-run]` | Confirm a bug's reproductions (bound to the digest shown), or reject them with a reason |
| `speccoder bug run (--task <unit>/<task> \| --slug S)` | Reproduce a bug again, or run its confirmed fix |
| `speccoder task approve [T] [--call ID] [--all] [--scope once\|task\|run] [--note "…"]` | Approve a waiting call (default scope: its object for the task, in its run); the next `run` continues from it. `--all` without `T`: every waiting call of the feature |
| `speccoder task reject [T] [--call ID] [--all] --reason R` | Reject it; the attempt continues and the model is told why |
| `speccoder task accept T --reason R [--in-place]` | Tick a task on your word (`attested`) |
| `speccoder task skip T (--reason R \| --undo)` | Skip a task, or take the skip back; its dependents stay blocked |
| `speccoder task nudge T (--note N \| --clear)` | Leave a note the next attempts read, or remove the notes |
| `speccoder task rule T --tests-right --reason R [--variant ID]` | Rule that the tests are right about a rejected alternative; the next `run` ticks it ([over-strict tests](@/docs/evidence.md#over-strict-tests)) |
| `speccoder task show T [--account]` | The task's state and evidence; `--account` adds the model's reasoning and words in its last attempt, turn by turn (a claim) |
| `speccoder task restore T` / `task drop T` | Keep the changes set aside before T's run (committed onto the branch as yours), or discard them |

## Review

| Command | What it does |
|---|---|
| `speccoder inbox` | Every decision waiting for you, in every feature (or `--feature`), most urgent first, each with the command that settles it |
| `speccoder report` | Every task and the evidence behind its tick, the requirement trace, ticks proven against an earlier spec, and what is open |
| `speccoder report --live` | What the current or last run is doing, from another terminal |
| `speccoder report --approvals` | Calls waiting for a decision, and the decisions made, with who made them |
| `speccoder approvals batch RUN approve\|reject --digest D [--call ID]… [--reason R] [--resume-asking]` | Decide the calls waiting together in a run that used its [asking budget](@/docs/approvals.md#the-asking-budget), each once; refused when the list changed |
| `speccoder review tests REQ [--write\|--reject P --reason R\|--again P --comment C\|--edit P --from F\|--reading A\|B SCENARIO]` | The tests the run would use for a requirement, [written and shaped at review](@/docs/planning.md#review-the-tests) |
| `speccoder review approve --run ask\|autopilot [--grant ID]… [--list-grants]` | Approve and Run: approve every open decision line, freeze the written tests, commit `review.json`, start the run |
| `speccoder review comments [add\|delete\|keep\|send\|undo]` | Comments anchored to the spec, sent as one scoped revision; undo restores the documents |
| `speccoder report --cost` | Tokens per task, and money when `[model]` has prices |
| `speccoder report --markdown` | The report as Markdown, for pull requests and CI summaries |
| `speccoder ui [--port N] [--no-open] [--window]` | The [web interface](@/docs/web.md) on 127.0.0.1, or a desktop window (`--window`: built in on macOS and Windows, elsewhere with `--features app`) |
| `speccoder pr [--remote R] [--base B] [--yes] [--dry-run] [--force]` | Push the branch, its attestation notes and the held-out tests of the tasks closed on it, and open a draft pull request with the evidence and each task's tour; `--force` hands off although the full-tier checks failed |

## Advanced

Listed by `speccoder help advanced`.

| Command | What it does |
|---|---|
| `speccoder actions [--task T]` | What you can do now, by group — the same list the dashboard, the app and an editor offer: each action's command, why it is blocked, what it asks for. With `--json`, scripts read it (payload `actions`): every action has an `id` (`kind` and `subject`) to find it by, and a planning skill its `role` (`create`, `toward_tasks`, `revise`, `check`) |
| `speccoder fix [--dry-run]` | Commit what in the project's setup stops a run (an uncommitted `.config/nextest.toml`, a lockfile the checks write, build output not ignored) in your checkout as `speccoder: setup` |
| `speccoder attest verify [REV\|RANGE] [--allowed-signers FILE] [--certificate-identity I --certificate-oidc-issuer O] [--require-signed]` | Re-check every tick's [attestation](@/docs/attestations.md) against its commit, and its signatures against your trust |
| `speccoder attest verify --branch [REV] [--base B] [--allowed-signers FILE \| --allowed-signers-rev REV:PATH] [--require-signed]` | Check every tick of the branch at `REV` (default `HEAD`) as [`speccoder-verify`](@/docs/verifier.md) does: payload `branch_verify` |
| `speccoder attest sign [REV\|RANGE]` | Sign the statements ticks left unsigned |
| `speccoder verify [--tier fast\|full] [--against FILE]` | Run the configured checks; compare with a saved `--json` result |
| `speccoder converge [--in-place]` | Assess a finished feature against its spec and append what remains |
| `speccoder inspect` | The features, artifacts and tasks speccoder sees, with what blocks what |
| `speccoder replay RUN` | Re-derive a recorded run from the journal, without a model |
| `speccoder worktree` | Create or reuse the feature's worktree and print its path |
| `speccoder acp` | Serve the Agent Client Protocol on stdio, for editors |
| `speccoder workflow install [--force]` | Add a speccoder step type and an example Spec Kit workflow |
| `speccoder eval [DIR] [--case C] [--repeat N] [--keep DIR] [--executor KIND]` | Run the evaluation suite (default `evals`) against hidden oracles |
| `speccoder bench import FILE --repos DIR --out DIR [--limit N] [--test-cmd CMD]` | Turn SWE-bench-format instances into evaluation cases |
| `speccoder propose T` | Ask the model how it would do one task; nothing is executed |
| `speccoder explain [CODE] [--list]` | What a code means, why it happens and how to fix it, offline |
| `speccoder schema [NAME]` | The JSON Schema of an output: `result`, `event`, or a payload (`run`, `report`, `doctor`, …); without a name, the list |
| `speccoder completion bash\|zsh\|fish\|elvish\|powershell` | A shell completion script, e.g. `speccoder completion zsh > ~/.zfunc/_speccoder` |
| `speccoder tui` | The dashboard (what bare `speccoder` opens at a terminal) |

## speccoder-verify

The [verifier](@/docs/verifier.md) is a binary of its own (no model key, no network). Every subcommand takes `--json`.

| Command | What it does |
|---|---|
| `speccoder-verify [check] [--repo DIR] [--head REV] [--base REV] [--all] TRUST [--require-signed=BOOL] [--require-verified] [--json] [--markdown FILE] [--github-check FILE] [--allow-neutral] [--github-annotations]` | Every tick of the units the branch touches against its signed evidence; payload `branch_verify`. Exit 0 success, 1 failure or refusal, 78 neutral |
| `speccoder-verify tour --feature F --task T [--head REV] [TRUST] [--json]` | One task's evidence tour, six steps; payload `evidence_tour` |
| `speccoder-verify badge [--head REV] TRUST [--svg FILE] [--endpoint FILE] [--json]` | The share of ticks with verifying evidence over every unit |

`TRUST` is `--allowed-signers FILE`, `--allowed-signers-rev REV:PATH` (read from git at that revision) and/or `--certificate-identity I --certificate-oidc-issuer O`; `check` and `badge` refuse without it.

## JSON output

With `--json` every command prints exactly one object on stdout and no progress lines, also when the arguments are wrong:

```json
{ "schema_version": 2, "command": "run", "status": "needs_human", "exit_code": 23,
  "message": "T004 needs approval: cargo add unicode-normalization",
  "code": "NEEDS-APPROVAL",
  "fix": ["speccoder report --approvals", "speccoder task approve T004", "speccoder task reject T004 --reason \"…\""],
  "docs": "https://hupe1980.github.io/speccoder/docs/codes/#needs-approval",
  "next": [{ "command": "speccoder report --approvals", "why": "see what waits" }],
  "payload": "run", "data": { … } }
```

- `status` is the exit code's name in `snake_case`.
- `code`, `fix` and `docs` are present on every result that is not plainly `ok`, and on an `ok` with less evidence (`REGRESSION-ONLY`). Branch on `code`, never on `message`.
- `remedy`, when present, is a fix speccoder can apply itself: `label`, `effect`, the exact `command`, `undo`, and whether it writes git `history`. Nothing applies it unasked: at a terminal `speccoder run` offers it (`[y/N]`) and runs again on a yes; the app shows it as a card; an editor gets a permission request.
- `payload` names the schema `data` follows.
- An action (in `speccoder actions`, Needs You and the events) carries `id`: its `kind` (`skill`, `run_task`, `approve`, `pull_request`, …, or `command` for one you run yourself) and `subject` (the task, the skill, the issue). Find an action by `id`, never by its `label`, which may be reworded.
- `--help` and `--version` always print text.

Every output has a JSON Schema (draft 2020-12), generated from the types that print it and published at `https://hupe1980.github.io/speccoder/schemas/v2/<name>.json`: `result.json`, `event.json`, and one per payload. `speccoder schema <name>` prints the same offline. Within a version fields are only added; ignore fields you do not know. Renaming, removing or changing the meaning of a field raises `schema_version`.

## Events

With `--events` stdout carries only JSON Lines as the command works; stderr stays quiet (an approval prompt still appears at a terminal):

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
| `session.started` | First line: version, command, `capabilities` (feature-detect on these) |
| `run.started` | A feature run begins, with its plan |
| `task.started`, `task.resumed`, `task.finished` | A task's run begins, picks up a killed attempt, ends (outcome, run id, tokens) |
| `stage.started`, `stage.finished` | `baseline`, `red`, `attempt`, `verify`, `audit`, `mutation`, `strength`, `monitor`, `commit` — and a bug's `reproduce`, `confirm`, `unchanged` |
| `bug.recorded`, `bug.confirmed`, `bug.confirm_rejected`, `bug.outcome` | A bug's records (public projections: [schemas](https://hupe1980.github.io/speccoder/schemas/v2/events/bug.outcome.json)) |
| `bug.reproduce_try`, `bug.reproduced`, `bug.confirm_waiting`, `bug.unchanged_selected`, `bug.discrimination` | A bug's progress |
| `tool_call` | A call the model made, with the policy's decision |
| `approval.requested`, `approval.decided` | A call waits for a person; who decided, how, and the scope |
| `usage` | Tokens so far (at most once a second per task) |
| `said` | What the model said beside its calls in a turn (`task`, `turn`, `text`): a claim |
| `reasoning` | The model's summary of its reasoning in a turn (`task`, `turn`, `text`): a claim |
| `notice` | Anything else worth a line |
| `result` | Last line, exactly once |

`seq` rises by one per line, so a gap means a lost line; a stream without `result` was interrupted. If the reader goes away, speccoder stops writing and finishes the command. `--events` with `--json` is a usage error (`USAGE-CONFLICT`).

## Exit codes

The [codes page](@/docs/codes.md) lists which code exits with which status.

| Code | Status | Meaning |
|---|---|---|
| 0 | `ok` | Success |
| 1 | `error` | Internal error |
| 2 | `usage_error` | Invalid arguments |
| 10 | `not_a_project` | No spec project found |
| 11 | `diagnostics_error` | The spec artifacts have errors |
| 12 | `selection_needed` | Feature selection needed or failed |
| 13 | `config_error` | `speccoder.toml` missing or invalid, or no applicable check |
| 14 | `git_precondition` | No repository or commits, uncommitted spec files, a dirty worktree, or an in-place run that cannot start |
| 15 | `not_found` | No such task, run or pending call |
| 16 | `task_not_ready` | The task is done, or blocked by open dependencies |
| 20 | `verification_failed` | A check or acceptance test failed |
| 21 | `run_failed` | The model call or the run failed |
| 22 | `replay_diverged` | Replay differed from the recorded run |
| 23 | `needs_human` | Stopped for a person: approval, denials, budget, not demonstrated, suspected gaming, interrupted, an in-place run stopped, … |
| 24 | `eval_failed` | The evaluation found a false completion or a cheat |
| 25 | `not_replayable` | This build cannot re-derive the recorded run |
| 26 | `attestation_failed` | `attest verify`: an attestation is missing, malformed, mismatched, or not signed by someone you trust (unsigned only with `--require-signed`) |
