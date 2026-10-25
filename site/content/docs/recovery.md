+++
title = "Recovery and replay"
description = "What happens when speccoder is interrupted, how a killed attempt resumes without repeating work, and how any recorded run replays offline without a model."
weight = 100

[extra]
group = "Trust"
nav = "Recovery and replay"
+++

speccoder records every model call and tool call in a local journal before it acts on the result, and every task outcome in git. That makes it safe to stop at any moment.

## When a run is interrupted

`speccoder report --live` says `interrupted` when the process died mid-run. What the next `speccoder run` does depends on where it died:

| Killed | Next run |
|---|---|
| During an attempt (Ctrl-C, a crash, a closed laptop) | **Resumes the attempt** from the journal: finished model and tool calls are read back, not repeated, and the worktree is kept as the attempt left it |
| While a tool call was running | Nobody knows whether that call finished, so it is not run again: the attempt is saved under its ref and undone, the run stops (`interrupted`) for you to look, and the next run starts a fresh attempt |
| After the attempt finished, before the tick | Reads the finished attempt back, judges it and ticks it, without calling the model |

**Edits by hand** in the worktree, without an interrupted attempt, are saved under an attempt ref and set aside; the task starts from the branch tip.

**`SPECCODER_CALL_ID`** is set for every command the model runs. A resumed call has the same id, so a script with side effects can tell a repeat from a new call.

## Interrupted in-place runs

An [`--in-place`](@/docs/running.md#in-place-runs) run records where it came from before it switches your checkout. If it dies, `speccoder report --live` says the checkout is still on `speccoder/<feature>` and which branch it returns to. Two ways out:

- **`speccoder run --in-place`** takes the run over and resumes it as in a worktree. A task whose verdict was committed but not ticked is ticked from that commit, but only while its line in `tasks.md` is still the one the commit names (`Task-Line`). At the end the checkout returns to the branch recorded when the run first started. If the spec, plan, tasks or the judge's inputs changed since, nothing is resumed: what the run left is saved under an attempt ref and the task starts fresh.
- **`speccoder run --in-place --return`** saves what the run left under `refs/speccoder/attempts/<feature>/in-place/<run>`, restores the checkout to the branch tip, switches back to the recorded branch and runs nothing.

Your own edits on `speccoder/<feature>` after a crash are not set aside as in a worktree: the run refuses (`error[DIRTY-CHECKOUT]`) and leaves them to you. If you changed the checkout yourself after a stop, speccoder does not switch over your change (`stopped[RETURN-BLOCKED]`): `git stash -u && git switch <branch>`, or `--return`.

## Waiting for a person

An attempt that waits for an [approval](@/docs/approvals.md#later) is not interrupted: its working tree and journal are kept, and the next `run` after `task approve` or `task reject` continues from that call. A process killed while a prompt is open loses nothing: the call is asked again on the next run, and a call decided before the kill is never asked again or performed twice. In place, the checkout stays on `speccoder/<feature>` while the call waits; `run --in-place --return` gives it back and withdraws the call.

## Failed attempts

A failed or stopped attempt is saved under `refs/speccoder/attempts/<feature>/<task>/<run>` and undone in the worktree. The next attempt never builds on it, and never treats test files it wrote as existing tests. Inspect it with `git log` and `git diff` like any commit.

## An unreadable journal

A journal this build cannot read (another format, or damage) is moved aside as `journal.<time>.unreadable.redb` and a fresh one is started. Progress lives in git, so only the ability to replay older runs is lost. A journal another speccoder process is using is never touched: the second process says so and stops.

## Replay

```sh
speccoder replay <run-id>
```

Run ids are printed in every run report and recorded on every commit (`Run:`). `replay` re-derives the run from the journal **offline, without a model and without credentials**:

| Result | Exit | Meaning |
|---|---|---|
| reproduced | 0 | Every recorded model and tool call again, in order, and the recorded ending — a failed or blocked run reproduces too |
| diverged | 22 | This build would have asked for something else, or ended differently; the report names what |
| not replayable | 25 | This build cannot re-derive the run, for example because it was interrupted and never ended |

The [evaluation suite](@/docs/evaluation.md) replays every recorded attempt after each case, as a standing check that speccoder's decisions depend only on what was recorded.
