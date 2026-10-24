+++
title = "Recovery and replay"
description = "What happens when speccoder is interrupted, how a killed attempt resumes without repeating work, and how any recorded run replays offline without a model."
weight = 100

[extra]
group = "Trust"
nav = "Recovery and replay"
+++

speccoder records every model call and tool call in a local journal before it acts on the result, and every task outcome in git. That record is what makes it safe to stop it at any moment.

## When a run is interrupted

**Killed during an attempt** (Ctrl-C, a crash, a closed laptop) — `speccoder status` says `interrupted`. The next `speccoder run` **resumes the attempt** from the journal: model calls and tool calls that had finished are read back, not repeated, and the worktree is kept as the attempt left it. A command that appends to a log is not run a second time.

**Killed while a tool call was running** — nobody can know whether that call finished, so speccoder does not guess and does not run it again. The attempt is saved under its ref and undone, and the run stops (`interrupted`) for you to look. The next run starts a fresh attempt.

**Killed after the attempt finished, before it was ticked** — the next run reads the finished attempt back, judges it, and ticks it without calling the model again.

**Edits by hand** in the worktree, without an interrupted attempt, are saved under an attempt ref and set aside; the task starts from the branch tip.

**`SPECCODER_CALL_ID`** is set for every command the model runs. A call that is resumed has the same id, so a script with side effects can tell a repeat from a new call.

## Interrupted in-place runs

A [`--in-place`](@/docs/running.md#in-place-runs) run records where it came from before it switches your checkout. If it dies, `speccoder status` says the checkout is still on `speccoder/<feature>`, which branch it returns to, and names both ways out:

- **`speccoder run --in-place`** — takes the interrupted run over and **resumes it as in a worktree**: an attempt killed mid-way continues from the journal (finished model and tool calls are read back, a command that ran is not run again), and a task whose verdict was committed but not yet ticked is ticked from that commit — only when its line in `tasks.md` is still exactly the one the commit names (`Task-Line`). At the end the checkout returns to the branch recorded when the run first started, not to `speccoder/<feature>`. If the spec, plan, tasks or the judge's inputs changed since the run died, nothing is resumed against them: what the run left is saved under an attempt ref and the task starts fresh.
- **`speccoder run --in-place --return`** — saves whatever the run left under `refs/speccoder/attempts/<feature>/in-place/<run>`, restores the checkout to the branch tip, switches back to the recorded branch and runs nothing.

Edits of your own on `speccoder/<feature>` after a crash, with no attempt to resume, are not set aside for you as in a worktree: the run refuses (`error[DIRTY-CHECKOUT]`) and leaves them to you.

If you changed the checkout yourself after a stop, speccoder does not switch over your change (`stopped[RETURN-BLOCKED]`): `git stash -u && git switch <branch>`, or `--return`.

## Waiting for a person

An attempt that waits for an approval is not an interrupted one: it was stopped on purpose, its working tree and journal are kept, and `speccoder approvals` lists the call. Decide it with `approve` or `reject` — from any terminal, no model needed — and the next `run` continues the attempt from that call. A process killed while a prompt is open loses nothing: the call is asked again on the next run, and a call decided before the kill is never asked again or performed twice. In place, the checkout stays on `speccoder/<feature>` while the call waits; `run --in-place --return` gives it back and withdraws the call.

## Failed attempts

A failed or stopped attempt is saved under `refs/speccoder/attempts/<feature>/<task>/<run>` and undone in the worktree. The next attempt never builds on it, and never treats test files it wrote as existing tests. Inspect it with `git log` and `git diff` like any commit.

## Upgrades

A journal the current build cannot read is moved aside (`journal.<time>.unreadable.redb`) and a fresh one is started. Progress lives in git, so only the ability to replay older runs is lost. A journal that another speccoder process is using is never touched: the second process says so and stops.

## Replay

```sh
speccoder replay <run-id>
```

Run ids are printed in every run report and recorded on every commit. `replay` re-derives the run from the journal **offline, without a model and without credentials**, and reports one of:

| Result | Exit | Meaning |
|---|---|---|
| reproduced | 0 | Every recorded model and tool call again, in order, and the recorded ending — whatever it was. A failed or blocked run reproduces too. |
| diverged | 22 | This build would have asked for something else, or ended differently; the report names what. |
| not replayable | 25 | This build cannot re-derive the run, for example because it was interrupted and never ended. |

The evaluation suite replays every recorded attempt after each case, as a standing check that speccoder's decisions depend only on what was recorded.
