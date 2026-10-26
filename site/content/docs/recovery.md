+++
title = "Recovery and replay"
description = "What happens when speccoder is interrupted, how a killed attempt resumes without repeating work, and how any recorded run replays offline without a model."
weight = 100

[extra]
group = "Trust"
nav = "Recovery and replay"
+++

speccoder journals every model call and tool call before it acts on the result, and records every task outcome in git, so it is safe to stop at any moment.

## When a run is interrupted

`speccoder report --live` says `interrupted` when the process died mid-run. The next `speccoder run` depends on where it died:

| Killed | Next run |
|---|---|
| During an attempt (Ctrl-C, a crash, a closed laptop) | **Resumes the attempt** from the journal: finished model and tool calls are read back, not repeated, and the worktree is kept as the attempt left it |
| While a tool call was running | Whether that call finished is unknown, so it is not run again: the attempt is saved under its ref and undone, the run stops (`interrupted`), and the next run starts a fresh attempt |
| After the attempt finished, before the tick | Reads the finished attempt back, judges it and ticks it, without calling the model |

Edits by hand in the worktree, without an interrupted attempt, are [set aside](@/docs/running.md#changes-set-aside). **`SPECCODER_CALL_ID`** is set for every command the model runs and is the same for a resumed call, so a script with side effects can tell a repeat from a new call.

An attempt [waiting for an approval](@/docs/approvals.md#later) is not interrupted. A process killed while a prompt is open loses nothing: the call is asked again on the next run, and a call decided before the kill is never asked again or performed twice.

## Interrupted in-place runs

An [`--in-place`](@/docs/running.md#in-place-runs) run records where it came from before it switches your checkout. If it dies, `speccoder report --live` says the checkout is still on `speccoder/<feature>` and which branch it returns to. Two ways out:

- **`speccoder run --in-place`** takes the run over and resumes it. A task whose verdict was committed but not ticked is ticked from that commit, while its line in `tasks.md` is still the one the commit names (`Task-Line`). At the end the checkout returns to the branch recorded when the run first started. If the spec, plan, tasks or the judge's inputs changed since, nothing is resumed: what the run left is saved under an attempt ref and the task starts fresh.
- **`speccoder run --in-place --return`** saves what the run left under `refs/speccoder/attempts/<feature>/in-place/<run>`, restores the checkout to the branch tip, switches back to the recorded branch and runs nothing. While a call waits for approval in place, the checkout stays on `speccoder/<feature>`; `--return` gives it back and withdraws the call.

Your own edits on `speccoder/<feature>` after a crash are not set aside: the run refuses (`error[DIRTY-CHECKOUT]`). If you changed the checkout yourself after a stop, speccoder does not switch over your change (`stopped[RETURN-BLOCKED]`): `git stash -u && git switch <branch>`, or `--return`.

## Failed attempts

A failed or stopped attempt is saved under `refs/speccoder/attempts/<feature>/<task>/<run>` and undone in the worktree. The next attempt never builds on it, and never treats test files it wrote as existing tests.

## An unreadable journal

A journal this build cannot read (another format, or damage) is moved aside as `journal.<time>.unreadable.redb` and a fresh one is started; progress lives in git, so only replay of older runs is lost. While one speccoder process writes the journal (a run, a decision), others read it as it stands — `status`, `report --approvals`, `inbox`, `task show --account` and the app answer at once; a second writer is refused at once and touches nothing. A reader never creates, moves or repairs the journal, so it cannot read one that needs repair: a run killed before it closed the journal (Ctrl-C, a closed terminal, out of memory) leaves it so, and `status`, `inbox` and `report --approvals` say `unknown — the last run ended without closing its journal` until the next writer repairs it. Every writer repairs it on opening: the next run, and a decision (`task approve`, `task reject`, `--all`, a batch, Always Allow), so a killed run never blocks a person's decision. Whatever cannot be read leaves the waiting calls `unknown`, never none; a run that cannot read them does not start, and a decision lists the calls through the writer it decides with.

## After upgrading from 0.2.0

The journal 0.2.0 wrote stays; its runs still list and show. A person's decision recorded under 0.2.0 does not read in this build (agentplane 0.54 records a decision as a verdict). The attempt that waited on it fails once with "this attempt waited on a decision recorded by speccoder 0.2.0 … run the task again", and is set aside; the next `speccoder run` starts the task afresh and asks again. `speccoder replay` of a run that read such a decision says so instead of reporting a divergence. Nothing needs deleting: ticks and attempt refs live in git.

While both versions work on one project, a 0.2.0 process holds the journal alone, so a newer `status` or `inbox` shows its waiting calls as `unknown`. The other way round, a 0.2.0 run or decision waits up to 2 s for a newer read, which holds the journal only while it reads. Upgrade every speccoder that works on a project together.

## Replay

```sh
speccoder replay <run-id>
```

Run ids are printed in every run report and recorded on every commit (`Run:`). `replay` re-derives the run from the journal **offline, without a model and without credentials**:

| Result | Exit | Meaning |
|---|---|---|
| reproduced | 0 | Every recorded model and tool call again, in order, and the recorded ending; a failed or blocked run reproduces too |
| diverged (`REPLAY-DIVERGED`) | 22 | This build would have asked for something else, or ended differently; the report names what |
| not replayable (`NOT-REPLAYABLE`) | 25 | This build cannot re-derive the run, for example because it was interrupted and never ended |

The [evaluation suite](@/docs/evaluation.md) replays every recorded attempt after each case.
