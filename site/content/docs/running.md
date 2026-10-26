+++
title = "Running a feature"
description = "Run tasks one by one or all at once, in parallel worktrees or as best-of-n samples; read outcomes, act on a task, converge on the spec, and watch progress and cost."
weight = 50

[extra]
group = "Use"
+++

## Run

```sh
speccoder run                    # every ready task in order, until done or a person is needed
speccoder run --task T005        # one task
speccoder run --max-tasks 3      # at most three tasks
speccoder run --parallel 3       # ready [P] tasks three at a time (1–8)
speccoder run --converge         # afterwards: converge, run what it appends, repeat
speccoder run --no-prompt        # never ask inline; stop instead (unattended runs, CI)
```

`run` works on the [current feature](@/docs/planning.md#the-current-feature). Run it again after any stop: it continues where it stopped and never repeats a passed task. Progress goes to stderr; the run report (tool calls and their decisions, changes, acceptance tests, checks, tokens, the commit) goes to stdout.

Less common flags:

| Flag | Does |
|---|---|
| `--task T005 --fresh-acceptance` | Sets the task's held attempt aside and writes its acceptance tests anew |
| `--allow-failing-baseline` | Attempts although a check cannot run on the untouched code (that check can never judge the task) |
| `--ignore-checklists` | Runs although `checklists/*.md` has unchecked items (recorded on every commit) |
| `--in-place` | Works in your checkout instead of a worktree; see [in-place runs](#in-place-runs) |

Runs started from the review with **Approve and Run** use the tests you [froze there](@/docs/planning.md#review-the-tests) and ask no acceptance author. On [autopilot](@/docs/approvals.md#autopilot) nothing asks you: a call that would is refused for the run and listed afterwards.

## When the setup stops a run

A run works in a worktree checked out from a commit. A file the checks need that your checkout never committed (`speccoder.toml`, `.config/nextest.toml`), a lockfile the checks write, or build output `.gitignore` does not list stops it with `error[SETUP-INCOMPLETE]` (exit 14).

```sh
speccoder fix --dry-run   # what it would commit
speccoder fix             # commit it in your checkout as `speccoder: setup`
```

`fix` copies a lockfile only the worktree's checks wrote, adds build output to `.gitignore`, and commits exactly those files and the uncommitted configuration. The next run picks it up. Undo: `git reset --soft HEAD~1`.

## Changes set aside

Uncommitted changes found in a feature's worktree when a run starts (edits by hand, without an interrupted attempt) are saved under an attempt ref and set aside; the task starts from the branch tip. They wait in **Needs You** and on the task's card:

```sh
speccoder task restore T005   # commit them onto speccoder/<feature> as yours
speccoder task drop T005      # discard them
```

`restore` merges them when the branch moved since. It changes nothing when they conflict, or when the worktree has uncommitted edits to tracked files.

## The model's account

Each turn, the model's summary of its reasoning and its words beside its calls are shown muted, apart from the calls:

- **terminal:** the reasoning's conclusion as `∴ …` and the words quoted (all of both with `-v`); screen-reader mode reads `model thinks:` and `model says:`;
- **dashboard and `report --live`:** the latest of each;
- **[app](@/docs/web.md):** a thinking line in the live panel, and per turn in the inspector's **The Model's Account**;
- **editor (ACP):** the reasoning as the agent's thoughts, the words as its message;
- **afterwards:** `speccoder task show T005 --account`, from the journal.

It is a claim: a verdict never rests on it. The summary comes from the provider (Anthropic, OpenAI Responses; an OpenAI-compatible server gives none) and is asked for on the attempt's turns only. `[model] reasoning_summary = false` turns it off. On the OpenAI Responses API the model's words beside its calls are not passed on, so only its reasoning shows.

## Quick changes

A small change without a planning round, judged like any other (Spec Kit projects):

```sh
speccoder do "Show the version number in the footer" --check "every page's footer shows the version"
speccoder do "Make the login button blue" --no-run      # record it, run later
```

`do` records the change in a standing feature, `NNN-quick-changes`: a user story (your words, each `--check` as an acceptance scenario), an approved requirement and a task citing it. It commits exactly those files (`spec(NNN-quick-changes): quick change: …`), then runs the task like any other. Quick changes do not wait for each other. `--no-run` prints the `run --task` command for later. In the [app](@/docs/web.md): **Quick Change** in Needs You, or `q`.

A bug is fixed with [`speccoder bug`](@/docs/bugs.md): reproduced by a failing test first, confirmed by you once.

## Outcomes

| Outcome | Exit | Meaning | What to do |
|---|---|---|---|
| `verified`, `regression_only` | 0 | Passed: ticked and committed | Review and merge eventually |
| `failed` | 20 | Checks or acceptance tests failed after the retries | `speccoder report`; inspect the saved attempt; `task nudge`, or fix the spec |
| `failed` (judge input changed) | 20 | A command changed `speccoder.toml`, CI, test-runner or agent configuration; the attempt ended at once, not retried | Read the finding; the task must not need that change |
| `stopped[ACCEPTANCE-WEAK]`, `stopped[TESTS-OVER-STRICT]` | 23 | The checks passed, but the acceptance tests are too weak or stricter than the spec; not ticked | See [acceptance strength](@/docs/evidence.md#acceptance-strength) and [over-strict tests](@/docs/evidence.md#over-strict-tests) |
| `acceptance_failed` | 23 | The acceptance tests could not be written (`ACCEPTANCE-UNWRITTEN`); nothing was attempted | Check the provider and key, then run again |
| feature done, full tier failed | 20 | Every task is ticked, but a `full` check failed | Fix it, or `speccoder pr --force` |
| unfinished checklists | 23 | `checklists/*.md` has unchecked items; nothing ran | Finish them, or `--ignore-checklists` |
| `blocked` | 23 | The model says the task cannot be done as written | Read the reason; fix the spec or a wrong test, or `task skip` |
| `needs_approval` | 23 | A call needs a person | `speccoder report --approvals`, then `task approve` or `task reject` |
| `denied` | 23 | Three denied calls in a row | Usually a task that asks for something protected |
| `not_demonstrated` | 23 | A convergence gap no failing test shows | Decide whether it is real; `task skip` or fix it |
| `suspected_gaming` | 23 | The monitor suspects a shortcut | Review the saved attempt; `task accept` if it is fine |
| `budget_exhausted`, `incomplete` | 23 | Out of turns or calls, or the model did not finish | Raise `[budget]`, split the task, or `task nudge` |
| `interrupted` | 23 | Killed while a tool call was running | Run again; see [recovery](@/docs/recovery.md) |

## Task actions

| Command | Does |
|---|---|
| `task approve T005 [--call ID] [--all] [--scope once\|task\|run] [--note …]` | Approves the waiting call (default scope: `task`); see [approvals](@/docs/approvals.md#later) |
| `task reject T005 --reason …` | Rejects it; the model reads the reason |
| `task accept T005 --reason …` | Ticks the task on your word: `attested`, never shown as verified |
| `task skip T005 [--reason …] [--undo]` | Runs pass it by; what depends on it stays blocked |
| `task nudge T005 --note …` (`--clear`) | A note the task's next attempts read |
| `task rule T005 --tests-right --reason … [--variant ID]` | Rules on a [held over-strict finding](@/docs/evidence.md#over-strict-tests): the tests are right; the next run ticks the held attempt without a model call |
| `task restore T005`, `task drop T005` | Keeps or discards [changes set aside](#changes-set-aside) |
| `task show T005 [--account]` | The task's state and evidence |

## Retries

A failed verdict is retried (`[budget] max_attempts`, default 2). The retry starts from the same clean checkpoint and is told the verifier's facts (which check, which test, the output), not the previous model's transcript. Identical failures twice in a row, infrastructure failures (a check that cannot even start) and integrity findings are not retried.

## Parallel tasks

`speccoder run --parallel N` runs up to N (at most 8) ready tasks marked `[P]` at once. Each runs in its own **lane**: a worktree on `speccoder-lane/<feature>/<task>`, started at the feature branch's tip. Lanes never ask inline; a call needing approval stops its lane. speccoder then merges each passed lane into `speccoder/<feature>` and **runs the checks on the merged whole**:

- a lane that does not merge runs again on its own, on top of the others;
- tasks that pass alone but fail together are undone, and the remaining tasks run one at a time.

## In-place runs

```sh
speccoder run --in-place
```

A worktree holds only what is committed, so it cannot see specs you keep out of git (`specs/` and `.specify/` in `.gitignore`). `--in-place` works in your checkout instead:

- It needs a clean checkout (no modified, staged or untracked files git does not ignore; else `error[DIRTY-CHECKOUT]`, exit 14), at least one commit, and `speccoder.toml` committed.
- It switches the checkout to `speccoder/<feature>` (created from your current commit, or reused), works through the tasks with the same judge as in worktree mode, and **switches back** to your branch when the run ends: finished, stopped or failed. Your branch is never committed to.
- Each passed task is committed with its evidence first and ticked second. When `tasks.md` is ignored, the tick lives only in your checkout; the commit says so (`Tick: local specs/<feature>/tasks.md`) and records the judged task line (`Task-Line:`) and intent (`Intent-Digest:`).
- `speccoder report` reads local ticks against those commits: a tick without such a commit, or a line reworded since, counts as without evidence; a commit whose tick is missing shows as `evidence_without_tick`; evidence from before the spec, plan or constitution changed is marked [judged against an earlier spec](@/docs/evidence.md#when-the-spec-changes).
- Do not use the checkout while it runs. Before every task and every tick, speccoder checks HEAD, the branch, the tree, and that the spec, plan, tasks, constitution and the judge's inputs are byte for byte what the run started with. A change stops the run (`stopped[CHECKOUT-CHANGED]`, `stopped[INTENT-CHANGED]`, `stopped[JUDGE-CHANGED]`, exit 23) and leaves your change where it is.
- `--parallel` is refused with `--in-place` (`error[IN-PLACE-PARALLEL]`, exit 2).

`task accept --in-place` and `converge --in-place` (and `run --in-place --converge`) work the same way: an attestation is committed before its local tick; a convergence round appends to your `tasks.md` and its commit says so (`Tasks: local`). In place, the model sees only what a worktree would hold, and commands cannot write `.git`; see [security](@/docs/security.md#in-place-mode).

While a feature has an in-place run (or an interrupted one), a worktree-mode `run`, `task accept` or `converge` of it is refused (`error[IN-PLACE-SESSION]`, exit 14); see [recovery](@/docs/recovery.md#interrupted-in-place-runs).

## Best of n

With `[budget] best_of = 3` (default 1), each task gets three independent attempts from the same starting point, and speccoder keeps the best by evidence: a passing verdict first (`verified` over `regression_only`), then fewer surviving mutants, the smaller diff, fewer tokens. It costs n times as much and pays off on tasks a model gets right only sometimes.

## Convergence

```sh
speccoder converge
```

Once every task is ticked, a read-only model call compares the code with `spec.md`, `plan.md`, `tasks.md` and the constitution and lists gaps: `missing`, `partial`, `contradicts`, `unrequested`. Actionable gaps are appended as a new `## Phase N: Convergence` in `tasks.md`, gravest first, and committed; `run --converge` then works through them and assesses again. It stops for you when only unrequested findings remain, when findings repeat earlier convergence tasks, or after `[converge] max_rounds` (default 3).

## Repository instructions

speccoder gives its model the repository's [`AGENTS.md`](https://agents.md) files with every task: from the repository root down to the directories the task names, one per directory, root first and nearest last, so a nearer file has the last word. The acceptance test author gets them too; the gaming monitor does not.

- They are the repository's text, not your orders: the task, the spec, the checks and speccoder's rules come first.
- At most 32 KiB: the nearest files whole, farther ones cut, and the report says so.
- They are read from the commit the attempt starts from and journaled with it. The run summary (`Guided by  AGENTS.md, services/api/AGENTS.md`) and the attestation name them, with each file's hash.
- Changing an `AGENTS.md` during an attempt needs your approval.
- External agents (Claude Code, Codex) read their own files; speccoder does not add them twice. Other file names, such as `CLAUDE.md`, are a [setting](@/docs/configuration.md#context).

## The dashboard

At a terminal, `speccoder run` runs inside the dashboard. A call that needs you appears over the screen with the same question, scope and keys as the [inline prompt](@/docs/approvals.md#inline). When you leave, the run's summary is printed as plain `run` prints it, and the exit status is the run's.

```text
 speccoder 001-slugify    1 Run   2 Evidence   3 Report     ? keys  q quit
┌ Tasks (6) ─────────────────────┐┌ Pipeline ──────────────────────────────────────────────┐
│ ✓ T001 Create the crate        ││ T004 Handle Unicode input                              │
│ ✓ T002 Slugify ASCII           ││ ✓ baseline  ✓ red  ◐ attempt  · verify  · audit  …     │
│ ~ T003 Trim separators         ││ baseline 1.2s · red 4.1s · attempt 1/2…                │
│ ◐ T004 Handle Unicode input    │└────────────────────────────────────────────────────────┘
│ ⧗ T005 Document the API        │┌ Tool calls ────────────────────────────────────────────┐
```

| Screen | Shows |
|---|---|
| `1` Run | Tasks, the running task's pipeline, tool calls, usage, the approval queue and `Next:` |
| `2` Evidence (`enter` on a task) | The verdict in words (`✓ VERIFIED`, `~ REGRESSION ONLY`, `A ATTESTED`) and every piece of evidence behind it, read from the task commit's [attestation](@/docs/attestations.md) (what `speccoder attest verify` checks); last, muted, what the model said |
| `3` Report | A verdict bar (`[###~A..]` in ASCII), the totals, what waits for a person, requirements no task cites, convergence rounds, `Next:` |

| Key | Does |
|---|---|
| `1` `2` `3`, `enter` | Switch screens; `enter` opens the selected task's evidence (or a waiting call) |
| `d` | The task commit's diff: `]` `[` next and previous hunk, `n` `p` next and previous file |
| `tab` | Switch between the task list and the approval queue; there `a` approves, `r` rejects with a reason, `B` decides a [batch](@/docs/approvals.md) |
| `n` | Leave and take the next step at the terminal (once the run has ended) |
| `f` | Pick the feature shown, making it [current](@/docs/planning.md#the-current-feature) (once the run has ended); with several features and none current, the dashboard opens on this picker |
| `esc` | Back: out of an overlay, then to the Run screen; never quits |
| `q`, `ctrl-c` | Quit; while the run works it asks: **finish this task, then stop**, **stop now** (the attempt is kept and the next `run` resumes it), or keep watching |
| `?` | Every key |

The dashboard needs a terminal of at least 60×15. `--plain` (or no terminal, `--json`, `--events`, screen-reader mode) gives the line output below. Quitting stops the run the dashboard hosts; to watch a run from elsewhere:

```sh
speccoder report --live   # running (pid …): 001-x T002 — attempt 1/2 for 33s; last: read_file … (allow)
speccoder                 # at a terminal: the same dashboard, watching (leaving does not stop the run)
```

## Watching a run

Every task goes through the same stages, named the same in the progress lines, `report --live`, the dashboard and editors: `baseline · red · attempt · verify · audit · mutation · strength · monitor · commit`. A stage that does not apply (no acceptance examples, mutation or the monitor not configured, strength switched off) is shown only with `-v`. With `--plain` (abridged):

```text
▶ T004 Handle Unicode input  (001-slugify)
  ✓ baseline
  ✓ red assertion
  · attempt 1/2
    turn 1   read_file    src/lib.rs                                                   allow
  ✓ attempt 1/2
  ✓ verify
  ✓ audit
  ✓ commit 9c1e2ab
✓ T004 verified · 40.2k in / 3.4k out · 171s — verdict: verified
```

A run picked up after a kill starts with `Resuming — T004 was interrupted during attempt (run_command cargo test); finished calls will not repeat.`

### Look and accessibility

Each state has its own colour, mark and word, so nothing depends on colour: `✓` verified, `~` regression only, `A` attested, `!` no evidence, `–` skipped, `·` ready, `⧗` blocked. Tables fit the terminal's width. `--ascii` (or a non-UTF-8 locale) uses `[ok] [~] [A] [!] [-] [ ] [..]` and plain borders; `NO_COLOR` turns colour off; themes for light terminals and colour-blind readers are in [your settings](@/docs/configuration.md#your-settings).

`--screen-reader` (or `SPECCODER_SCREEN_READER=1`, or `[ui] screen_reader = true`) gives labelled plain lines, choices as a numbered list, and the report instead of the dashboard:

```text
task: T004 Handle Unicode input — started
stage: T004 attempt — started (1/2)
tool: run_command cargo test — allow, ok
approval required: T004 — run_command cargo add unicode-normalization
choice 1: Yes, once (or type y)
```

### Getting your attention

When a call waits for you, a task fails, the feature ends or nothing happens for 15 minutes, speccoder notifies you: a desktop notification in terminals that show one (iTerm2, WezTerm, Ghostty, Kitty, Warp, foot, Konsole), a bell elsewhere. The terminal title follows the run (`speccoder ▸ T004`, `speccoder △ approval T004`, `speccoder ✓ done`). Neither happens with `--json` or `--events`. Configure it in [your settings](@/docs/configuration.md#your-settings).

## Cost

```sh
speccoder report --cost     # tokens per task, every attempt, convergence, and the total
```

`report --cost` reads the usage recorded on commits and saved attempts. For money, add your prices in USD per million tokens to `[model]`: `price_input_per_mtok` and `price_output_per_mtok`, optionally `price_cache_read_per_mtok` (default: the input rate), `price_cache_write_per_mtok` and `price_cache_write_1h_per_mtok` (default: 1.25× and 2× the input rate on Anthropic, the input rate elsewhere). Prices then also appear in the progress lines. speccoder ships no price list. With Claude Code as the agent, its own reported cost is used.
