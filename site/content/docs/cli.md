+++
title = "CLI reference"
description = "Every speccoder command and its options, the global flags, the JSON output contract and the exit codes."
weight = 130

[extra]
group = "Reference"
nav = "CLI reference"
+++

`speccoder --help` lists the daily commands, `speccoder help advanced` the rest, `speccoder help <command>` one command. Bare `speccoder` opens the [dashboard](@/docs/running.md#the-dashboard) at a terminal and prints the report otherwise.

Every command takes:

| Option | Meaning |
|---|---|
| `-C PATH`, `--project PATH` | A path inside the project (default: the current directory) |
| `--feature NAME` (`--change`) | The feature: directory name or numeric prefix; an [OpenSpec](@/docs/openspec.md) change's id |
| `--json` | One JSON object on stdout ([JSON output](#json-output)) |
| `--events` | JSON Lines as it works, ending in that object ([events](#events)) |
| `-q`, `--quiet` | Only stops, finishes, resumes and what needs you |
| `-v`, `--verbose` | Also durations, skipped stages and the tails of what failed |
| `--color auto\|always\|never`, `--ascii`, `--screen-reader` | How output looks; also [your settings](@/docs/configuration.md#your-settings) |
| `--plain` | Lines instead of the dashboard: `run` prints its progress, bare `speccoder` the report |
| `--format speckit\|openspec\|kiro` | The spec format (also `SPECCODER_FORMAT`; default: `[spec] format`, else detected) |
| `--copy` | When the command fails, put what went wrong on the clipboard (`pbcopy`, `wl-copy` or `xclip`): the code and message, each fix, the docs link, the version, the OS and the run — secrets masked; the exit code is the command's |

Every stop and finish ends with a `Next:` block of commands to copy (`next` in JSON). Every failure and stop names a [code](@/docs/codes.md): `error[CODE]`, `stopped[CODE]` when the run waits for you, `warning[CODE]` when it worked with less evidence.

## Start

| Command | What it does |
|---|---|
| `speccoder new [PATH] [--idea I] [--stack rust\|python\|node\|go\|none] [--format speckit\|openspec] [--extension X]… [--no-preset] [--dry-run] [--yes] [--no-prompt]` | [Start a project](@/docs/quickstart.md#start-a-project): the plan of the stack's and the format's own commands, `speccoder init --yes`, one commit as you. `--dry-run` prints the plan and its digest; without `--yes` it asks once (no terminal: `NEW-CONFIRM`). Payload `new_project` |
| `speccoder new --undo [PATH] [--yes]` | Undo a project speccoder created: the folder to the Trash, or a revert of its commit in an existing repository — only while nothing else happened there (`NEW-UNDO-REFUSED`) |
| `speccoder sample [--fresh] [--yes]` | Copy the bundled sample into speccoder's state directory and replay its recorded run strictly, no model (payload `sample`; `SAMPLE-DIVERGED`, `SAMPLE-MISSING`) |
| `speccoder init [--provider anthropic\|openai\|bedrock] [--model M] [--region R] [--force] [--no-verify] [--yes]` | Write `speccoder.toml` with the detected checks, run them on the untouched code, offer the commit. `--region` is Bedrock's (default: `AWS_REGION`, `AWS_DEFAULT_REGION`, the profile's); `--force` replaces the file; `--yes` asks nothing and commits nothing |
| `speccoder doctor [--bug-report] [--agents]` | Check git, the project, the configuration, the key, the sandbox, the state directory, signing, GitHub and the look; changes nothing. `--bug-report`: a redacted Markdown bundle for an issue. `--agents`: every agent's *installed*, *signed in*, *asks first* (an installed one is asked for a session, never a prompt) |
| `speccoder config` | Every key of `speccoder.toml` with its value or default ([configuration](@/docs/configuration.md#changing-it)) |
| `speccoder config get KEY` / `unset KEY` | One key's value; remove a key so its default applies |
| `speccoder config set KEY VALUE… [--yes]` | Set a key, comments kept; a prefix list only after its preview is confirmed |
| `speccoder config coverage KEY PREFIX…` | What a prefix would newly allow ([prefix lists](@/docs/configuration.md#prefix-lists)) |
| `speccoder config commit` | Commit `speccoder.toml` alone |
| `speccoder auth [login [--with-token] \| status \| logout] [--host H]` | Sign in to GitHub (a device code, or a token on stdin), show the account, sign out ([GitHub](@/docs/github.md#sign-in)) |

## Plan

| Command | What it does |
|---|---|
| `speccoder feature [QUERY] [--clear]` | List the features, or make one [current](@/docs/planning.md#the-current-feature) by name, number or a unique part; `--clear` forgets it |
| `speccoder spec [SKILL] [ARGS…] [--no-prompt]` | List the planning skills, or run one (`specify`, `plan`, `tasks`, …; OpenSpec: `propose`, …) |
| `speccoder spec SKILL [ARGS…] --through spec\|plan\|tasks\|analyze` | [Plan through](@/docs/planning.md#plan-through): the format's skills in order from SKILL (the creating skill with a description, or `plan`/`tasks`/`analyze` to continue), stopping for open questions (`THROUGH-QUESTIONS`) and your review (`THROUGH-REVIEW`). Payload `planning_chain` |
| `speccoder feature similar TEXT…` | The requirements close to a description, with the terms they share (payload `similar`); no model |
| `speccoder spec archive [CHANGE] [--yes] [--allow-unverified TASK]…` | [Archive](@/docs/openspec.md#archiving) an OpenSpec change |
| `speccoder review` | Every requirement and its review state ([review](@/docs/planning.md#review-the-spec)) |
| `speccoder review approve [ID…] [--comment C]` | Approve requirements on their current wording; none named: every unapproved one |
| `speccoder review approve --run ask\|autopilot [--grant ID]… [--list-grants]` | [Approve and Run](@/docs/planning.md#approve-and-run) |
| `speccoder review request ID… --comment C` | Request changes |
| `speccoder review answer Q ANSWER` | Answer an open question in place |
| `speccoder review edit ID WORDING [--approve]` | Reword a requirement; its approval lapses unless `--approve` |
| `speccoder review clear ID…` | Remove the decision |
| `speccoder review tests REQ [--write \| --reject P --reason R \| --again P --comment C \| --edit P --from F \| --reading A\|B SCENARIO]` | [Write and shape a requirement's tests](@/docs/planning.md#review-the-tests) |
| `speccoder review comments [add TEXT \| delete ID \| keep ID \| send \| undo]` | [Comments](@/docs/planning.md#comments), sent as one revision |
| `speccoder issue [list]` | The repository's open GitHub issues |
| `speccoder issue start N [--through T] [--no-prompt] [--comment \| --no-comment] [--assign]` | Specify a feature from an issue: its description verbatim, the unit's origin recorded once |
| `speccoder issue comment N [--assign]` | Tell the issue the current feature is specified from it |

## Work

| Command | What it does |
|---|---|
| `speccoder run [--task T] [--max-tasks N] [--parallel N] [--converge] [--no-prompt] [--ignore-checklists]` | Work through the ready tasks, or one; resumes where it stopped ([running](@/docs/running.md)) |
| `speccoder run --agent copilot` | This run's attempts by that [agent](@/docs/integrations.md#external-agents) (`native`: speccoder's own); `speccoder.toml` is unchanged |
| `speccoder run --task T --fresh-acceptance` | Set the task's held attempt aside and write its acceptance tests anew |
| `speccoder run --allow-failing-baseline` | Attempt although a check cannot run on the untouched code (`CHECK-BROKEN`) |
| `speccoder run --in-place [--return]` | [Work in your checkout](@/docs/running.md#in-place-runs); `--return` leaves an interrupted in-place run |
| `speccoder do "…" [--check "…"]… [--no-run] [--no-prompt]` | A [quick change](@/docs/running.md#quick-changes) |
| `speccoder bug "…" --expected "…"… [--unchanged "…"]… [--issue N] [--test PATH::NAME]… [--candidate PATCH\|pr:N] [--slug S] [--no-run] [--no-prompt]` | A [bug fix](@/docs/bugs.md) |
| `speccoder bug confirm UNIT/TASK [--digest D] [--reject --reason R] [--no-run]` | Confirm a bug's reproductions (bound to the digest shown), or reject them |
| `speccoder bug keep UNIT/TASK --reason R` | An already-fixed bug: commit its reproductions as a regression test, attest the task |
| `speccoder bug run (--task UNIT/TASK \| --slug S) [--no-prompt]` | Reproduce a bug again, or run its confirmed fix |
| `speccoder task approve [T] [--call ID] [--all] [--scope once\|task\|run] [--note N]` | Approve a waiting call (default scope `task`; [approvals](@/docs/approvals.md#later)). `--all` without `T`: every waiting call of the feature |
| `speccoder task reject [T] [--call ID] [--all] --reason R` | Reject it; the model is told why |
| `speccoder task accept T --reason R [--in-place]` | Tick a task on your word (`attested`) |
| `speccoder task skip T (--reason R \| --undo)` | Skip a task, or take the skip back; its dependents stay blocked |
| `speccoder task done T [--reason R]` | I did it: tick a step you took on your word (`attested`; the reason defaults to "I did it") |
| `speccoder task hint T NOTE [--no-prompt]` | Hint & retry: the note, then a new attempt of the task now |
| `speccoder task nudge T (--note N \| --clear)` | Leave a note the next attempts read, or remove the notes |
| `speccoder task rule T --tests-right --reason R [--variant ID]` | Rule that the tests are right ([over-strict tests](@/docs/evidence.md#over-strict-tests)); the next run ticks the held attempt without a model call |
| `speccoder task restore T` / `task drop T` | Keep or discard the [changes set aside](@/docs/running.md#changes-set-aside) |
| `speccoder task acceptance-at T --story US [--revoke]` | Write the story's acceptance tests at this Setup or Foundational task ([plan checks](@/docs/planning.md#plan-checks)) |
| `speccoder task show T [--account]` | The task's state and evidence; `--account` adds what the model said in its last attempt (a claim) |

## Review

| Command | What it does |
|---|---|
| `speccoder inbox` | Every decision waiting, in every feature (or `--feature`), most urgent first, with its command |
| `speccoder report` | Every task and the evidence behind its tick, the requirement trace, and what is open |
| `speccoder report --live` | What the current or last run is doing, from another terminal |
| `speccoder report --approvals` | Calls waiting for a person, and the decisions made |
| `speccoder report --cost` | Tokens per task, and money when `[model]` has prices |
| `speccoder report --markdown` | The report as Markdown, for pull requests and CI summaries |
| `speccoder ui [--port N] [--no-open] [--window]` | The [web interface](@/docs/web.md) on 127.0.0.1 |
| `speccoder pr [--remote R] [--base B] [--yes] [--dry-run] [--force]` | Push the branch and open a [draft pull request](@/docs/github.md#its-pull-request); `--force` despite failed full-tier checks |

## Advanced

| Command | What it does |
|---|---|
| `speccoder suggest TEXT…` | The rung a description suggests — bug fix, quick change, feature or a question — and the cue that decided it (payload `rung_suggestion`); no model |
| `speccoder preset add [--yes]` | Install or update the [speccoder Spec Kit preset](@/docs/planning.md#the-speccoder-preset) with `specify preset add --dev` |
| `speccoder actions [--task T]` | What you can do now, as every interface offers it: each action's command, why it is blocked, what it asks for (payload `actions`) |
| `speccoder approvals` | Calls waiting for a person, and the decisions made |
| `speccoder approvals batch RUN approve\|reject --digest D [--call ID]… [--reason R] [--resume-asking]` | Decide the [calls waiting together](@/docs/approvals.md#the-asking-budget) |
| `speccoder fix [--dry-run]` | Commit the setup that stops a run (`SETUP-INCOMPLETE`) in your checkout |
| `speccoder attest verify [REV\|RANGE] [--allowed-signers FILE] [--certificate-identity I --certificate-oidc-issuer O] [--require-signed]` | [Check the attestations](@/docs/attestations.md#checking-them) (exit 26 when one fails) |
| `speccoder attest verify --branch [REV] [--base B \| --all] [--allowed-signers-rev REV:PATH] [--require-signed=false] [--github-check FILE]` | [Check every tick of a branch](@/docs/verifier.md) from git objects, with your trust (exit 0 / 26 failure / 15 neutral) |
| `speccoder attest tour TASK [REV] --feature F` | One tick's [evidence tour](@/docs/verifier.md#the-evidence-tour) (payload `evidence_tour`) |
| `speccoder attest badge [REV] [--svg FILE] [--endpoint FILE]` | The [evidence badge](@/docs/verifier.md#the-badge) |
| `speccoder attest sign [REV\|RANGE]` | Sign the statements left unsigned |
| `speccoder verify [--tier fast\|full] [--against FILE]` | Run the configured checks; compare with a saved `--json` result |
| `speccoder converge [--in-place]` | Assess a finished feature against its spec and append what remains |
| `speccoder inspect` | The artifacts and tasks as speccoder reads them, with what blocks what |
| `speccoder replay RUN` | Re-derive a recorded run from the journal, without a model ([replay](@/docs/recovery.md#replay)) |
| `speccoder worktree` | Create or reuse the feature's worktree and print its path |
| `speccoder acp` | Serve the Agent Client Protocol on stdio, for [editors](@/docs/integrations.md#editors-acp) |
| `speccoder workflow install [--force] [--overlay bugfix]` | Add the [workflow step type](@/docs/planning.md#spec-kit-workflows), or the [`bugfix` overlay](@/docs/bugs.md#commands) |
| `speccoder workflow uninstall --overlay bugfix` | Remove that overlay |
| `speccoder eval [DIR] [--case C] [--repeat N] [--keep DIR] [--executor native\|copilot\|opencode\|claude\|codex\|gemini]` | Run the [evaluation suite](@/docs/evaluation.md) (default `evals`) |
| `speccoder bench import FILE --repos DIR --out DIR [--limit N] [--test-cmd CMD]` | Turn SWE-bench-format instances into evaluation cases |
| `speccoder propose T` | Ask the model how it would do one task; nothing is executed |
| `speccoder explain [CODE] [--list]` | What a code means and how to fix it, offline |
| `speccoder schema [NAME]` | The JSON Schema of an output (`result`, `event`, a payload); without a name, the list |
| `speccoder completion bash\|zsh\|fish\|elvish\|powershell` | A shell completion script, e.g. `speccoder completion zsh > ~/.zfunc/_speccoder` |
| `speccoder tui` | The dashboard |

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
- `fix_steps` (beside `fix`) and every `next` entry say how an interface takes the line in `how`: `perform` (a `run` speccoder performs, with the `input` it asks for), `view` (a read-only view: `doctor`, `report`, …), `command` (the person's own command as exact `argvs`; `interactive` when it reads the keyboard), `shell` (`export …`: it must be set where speccoder starts) or `words`.
- `remedy`, when present, is a fix speccoder can apply itself: `label`, `effect`, the exact `command`, `undo`, and whether it writes git `history` (a commit). Nothing applies it unasked.
- `payload` names the schema `data` follows.
- An action (in `actions`, Needs You and the events) carries an `id`: its `kind` (`run_task`, `approve`, `pull_request`, …, or `command`) and `subject`. Match actions by `id`, never by `label`.
- `--help` and `--version` always print text.

Every output has a JSON Schema (draft 2020-12), generated from the code, at `https://hupe1980.github.io/speccoder/schemas/v2/<name>.json` (offline: `speccoder schema <name>`). Within a version fields are only added, so ignore fields you do not know; renaming, removing or changing the meaning of a field raises `schema_version`.

## Events

With `--events` stdout carries only JSON Lines; stderr stays quiet (an approval prompt still appears at a terminal):

```jsonl
{"v":2,"seq":1,"ts":"2026-10-24T10:00:00.000Z","type":"session.started","speccoder":"0.4.0","command":"run","capabilities":["events_v2","approvals_inline"]}
{"v":2,"seq":9,"ts":"…","type":"tool_call","task":"T001","turn":3,"tool":"run_command","target":"cargo test","decision":"allow","ok":true,"tail":[]}
{"v":2,"seq":15,"ts":"…","type":"result","result":{ …what --json prints… }}
```

| Type | When |
|---|---|
| `session.started` | First line: version, command, `capabilities` (feature-detect on these, never on the version) |
| `run.started` | A feature run begins, with its plan |
| `task.started`, `task.resumed`, `task.finished` | A task's run begins, picks up a killed attempt, ends (outcome, run id, tokens) |
| `stage.started`, `stage.finished` | `prepare`, `baseline`, `reproduce`, `confirm`, `red`, `attempt`, `verify`, `unchanged`, `audit`, `mutation`, `strength`, `monitor`, `commit` |
| `tool_call` | A call the model made, with the policy's decision |
| `approval.requested`, `approval.decided` | A call waits for a person; who decided, how, and the scope |
| `usage` | Tokens so far (at most once a second per task) |
| `said`, `reasoning` | What the model said beside its calls, or its reasoning summary, in a turn: a claim |
| `notice` | Anything else worth a line |
| `bug.recorded`, `bug.confirmed`, `bug.confirm_rejected`, `bug.outcome` | A bug's records |
| `bug.reproduce_try`, `bug.reproduced`, `bug.confirm_waiting`, `bug.unchanged_selected`, `bug.discrimination` | A bug's progress |
| `oracle.stage` | Writing tests at review went one step further |
| `oracle.frozen`, `oracle.lapsed` | A requirement's tests were frozen by an approval, or lapsed with its wording |
| `result` | Last line, exactly once |

Record events (`bug.recorded`, `bug.confirmed`, `bug.confirm_rejected`, `bug.outcome`, `oracle.frozen`, `oracle.lapsed`) have published schemas under `events/` (e.g. [`events/bug.outcome.json`](https://hupe1980.github.io/speccoder/schemas/v2/events/bug.outcome.json)).

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
| 14 | `git_precondition` | No repository or commits, uncommitted spec files or setup, a dirty worktree, or an in-place run that cannot start |
| 15 | `not_found` | No such task, run or pending call |
| 16 | `task_not_ready` | The task is done, or blocked by open dependencies |
| 20 | `verification_failed` | A check or acceptance test failed |
| 21 | `run_failed` | The model call or the run failed |
| 22 | `replay_diverged` | Replay differed from the recorded run |
| 23 | `needs_human` | Stopped for a person: approval, denials, budget, not demonstrated, suspected gaming, interrupted, … |
| 24 | `eval_failed` | The evaluation found a false completion or a cheat |
| 25 | `not_replayable` | This build cannot re-derive the recorded run |
| 26 | `attestation_failed` | An attestation is missing, malformed, mismatched, or not signed by someone you trust (unsigned only with `--require-signed`) |
