+++
title = "CLI reference"
description = "Every speccoder command and its options, the global flags, the JSON output contract and the exit codes."
weight = 130

[extra]
group = "Reference"
nav = "CLI reference"
+++

Every command takes `--json` (one JSON object on stdout), `-q`/`--quiet` (only stops, finishes, resumes and what needs you), `-v`/`--verbose` (also durations, skipped stages and the last lines of what failed), `--format <speckit|openspec|kiro>` (the project's spec format, also `SPECCODER_FORMAT`; default: `[spec] format`, else detected) and `--help`. Commands that work on a project take an optional `PATH` inside it (default: the current directory) and `--feature <name or number>` (for an [OpenSpec](@/docs/openspec.md) change also `--change <id>`).

Every stop and finish ends with a `Next:` block — the commands that continue, copy-pasteable, the same in `--json` as `data.next` (`[{"command", "why"}]`).

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
| `speccoder report` | Every task and the evidence behind its tick |
| `speccoder cost` | Tokens, and money with prices configured, per task |
| `speccoder replay <RUN>` | Re-derive a recorded run from the journal, without a model |

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
| `speccoder pr [--remote R] [--base B] [--yes] [--dry-run] [--force]` | Push the branch and open a draft pull request with the evidence; `--force` hands off although the full-tier checks failed |
| `speccoder acp` | Serve the Agent Client Protocol on stdio, for editors |

## Measure

| Command | What it does |
|---|---|
| `speccoder eval [DIR] [--case C] [--repeat N] [--keep DIR] [--executor KIND]` | Run the evaluation suite against hidden oracles |
| `speccoder bench import FILE --repos DIR --out DIR [--limit N] [--test-cmd CMD]` | Turn SWE-bench-format instances into evaluation cases |

## JSON output

With `--json` every command prints exactly one object on stdout and no progress lines:

```json
{ "schema_version": 1, "command": "run", "status": "ok", "message": "…", "exit_code": 0, "data": { … } }
```

`status` is the exit code's name in `snake_case`; `data` depends on the command. `--help` and `--version` always print text.

Fields are only added within a schema version; ignore fields you do not know. Recent additions: a test outcome `"error"` (a JUnit `<error>`), `flaky` on check results, `red_class` and `excerpt` in a run's `acceptance`, `scope` on a run (`stated`, `named`, `outside`), `full_tier` on a feature run, `data.checklists` when unfinished checklists stop a run, and for in-place runs `data.code`, `data.paths` and `data.fix` on a refusal or stop, a task state `evidence_without_tick` with `totals.evidence_without_tick`, and `intent_changed` on a task.

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
