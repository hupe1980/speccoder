+++
title = "Running a feature"
description = "Run tasks one by one or all at once, in parallel worktrees or as best-of-n samples; read outcomes, converge on the spec, and watch progress and cost."
weight = 50

[extra]
group = "Use"
+++

## Run

```sh
speccoder run                    # every ready task in order, until done or a person is needed
speccoder run --task T005        # one task
speccoder run --max-tasks 3      # at most three tasks
speccoder run --parallel 3       # ready [P] tasks three at a time
speccoder run --converge         # afterwards: converge, run what it appends, repeat
speccoder run --no-prompt        # never ask inline; stop instead (unattended runs, CI)
```

The feature is selected the way Spec Kit selects it: `--feature`, then `SPECIFY_FEATURE_DIRECTORY`, then `.specify/feature.json`, then the only feature. Run `speccoder run` again after any stop: it continues where it stopped and never repeats a passed task.

Progress streams to stderr; the run report — tool calls with their decisions, changes, acceptance tests, checks, tokens and the commit — goes to stdout.

## Outcomes

| Outcome | Exit | Meaning | What to do |
|---|---|---|---|
| `verified`, `regression_only` | 0 | Passed: ticked and committed | Review and merge eventually |
| `failed` | 20 | Checks or acceptance tests failed after the retries | `speccoder report`; inspect the saved attempt; `nudge`, or fix the spec |
| `failed` (judge input changed) | 20 | A command changed `speccoder.toml`, CI, test-runner or agent configuration; the attempt ended at once, not retried | Read the finding; the task must not need that change |
| feature done, full tier failed | 20 | Every task is ticked, but a `full` check failed | Fix it, or `speccoder pr --force` |
| unfinished checklists | 23 | `checklists/*.md` has unchecked items; nothing ran | Finish them, or `speccoder run --ignore-checklists` (recorded on the commits) |
| `blocked` | 23 | The model says the task cannot be done as written | Read the reason; fix the spec or a wrong test, or `skip` |
| `needs_approval` | 23 | A call needs a person | `speccoder approvals`, then `approve` or `reject` |
| `denied` | 23 | Three denied calls in a row | Usually a task that asks for something protected |
| `not_demonstrated` | 23 | A convergence gap no failing test shows | Decide whether it is real; `skip` or fix it |
| `suspected_gaming` | 23 | The monitor suspects a shortcut | Review the saved attempt; `accept` if it is fine |
| `budget_exhausted`, `incomplete` | 23 | Out of turns or calls, or the model did not finish | Raise `[budget]`, split the task, or `nudge` |
| `interrupted` | 23 | Killed while a tool call was running | Run again; see [recovery](@/docs/recovery.md) |

## Retries

A failed verdict is retried (`[budget] max_attempts`, default 2). The retry starts from the same clean checkpoint and is told the verifier's facts — which check, which test, the output — not the previous model's transcript. Identical failures twice in a row, infrastructure failures (a check that cannot even start) and integrity findings are not retried.

## Parallel tasks

`speccoder run --parallel N` runs up to N ready tasks marked `[P]` at once. Each runs in its own **lane**: a worktree on `speccoder-lane/<feature>/<task>`, started at the feature branch's tip. Lanes never ask inline (prompts would interleave); a call needing approval stops its lane. Then speccoder merges each passed lane into `speccoder/<feature>` and **runs the checks on the merged whole**:

- a lane that conflicts with another runs again on its own, on top of the others;
- tasks that pass alone but fail together are undone, and the remaining tasks run one at a time.

## In-place runs

```sh
speccoder run --in-place
```

By default speccoder works in a worktree, and a worktree holds only what is committed. If you keep your specs out of git — `specs/` and `.specify/` in `.gitignore` — a worktree cannot see them. `--in-place` works in your checkout instead:

- It needs a clean checkout: no modified, staged or untracked files git does not ignore (`error[DIRTY-CHECKOUT]`, exit 14, with the paths and the fix), at least one commit, and `speccoder.toml` committed.
- It switches the checkout to `speccoder/<feature>` (created from your current commit, or reused), works through the tasks with the same judge as in worktree mode, and **switches back** to the branch you were on when the run ends — finished, stopped or failed. Your branch is never committed to.
- Each passed task is committed with its evidence first and ticked second. When `tasks.md` is ignored, the tick lives only in your checkout: the commit says so (`Tick: local specs/<feature>/tasks.md`) and names the task line it judged (`Task-Line:`) and the intent it judged against (`Intent-Digest:`). `speccoder report` reads the local ticks against those commits: a tick without such a commit, or a ticked line reworded since, counts as without evidence; a commit whose local tick is missing shows as `evidence_without_tick`; and evidence committed before the spec, plan or constitution changed is marked as judged against an earlier spec.
- Do not use the checkout while it runs. Before every task speccoder checks that HEAD, the branch and the tree are as it left them, and that the spec, plan, tasks, constitution and the judge's inputs are byte for byte what the run started with; before every tick it checks HEAD and those files again. A change stops the run (`stopped[CHECKOUT-CHANGED]`, `stopped[INTENT-CHANGED]`, `stopped[JUDGE-CHANGED]`, exit 23) and leaves your change where it is.
- `--parallel` needs worktrees and is refused with `--in-place` (`error[IN-PLACE-PARALLEL]`, exit 2).

`accept --in-place` and `converge --in-place` (and `run --in-place --converge`) work the same way: an attestation is committed before its local tick; a convergence round appends to your `tasks.md` and its commit says so (`Tasks: local`). In place the model sees only what a worktree would hold, and commands cannot write `.git`; see [security](@/docs/security.md#in-place-mode).

While a feature has an in-place run (or an interrupted one), a worktree-mode `run`, `accept` or `converge` of it is refused (`error[IN-PLACE-SESSION]`); see [recovery](@/docs/recovery.md).

## Best of n

With `[budget] best_of = 3`, each task gets three independent attempts from the same starting point, and speccoder keeps the best by evidence: a passing verdict first (`verified` over `regression_only`), then fewer surviving mutants, the smaller diff, fewer tokens. It costs n times as much; it pays off on tasks a model gets right only sometimes.

## Convergence

```sh
speccoder converge
```

Once every task is ticked, a read-only model call compares the code with `spec.md`, `plan.md`, `tasks.md` and the constitution and lists gaps: `missing`, `partial`, `contradicts`, `unrequested`. Actionable gaps are appended as a new `## Phase N: Convergence` in `tasks.md`, gravest first, and committed; `run --converge` then works through them and assesses again. It stops for you when only unrequested findings remain, when findings repeat earlier convergence tasks, or after `[converge] max_rounds`.

## Watching a run

```sh
speccoder status   # running (pid …): 001-x T002 — attempt 1/2 for 33s; last: read_file … (allow)
speccoder tui      # dashboard: tasks with evidence, the live attempt, the approval queue
```

Every task goes through the same stages, named the same in the progress lines, `status`, the dashboard and editors: `baseline · red · attempt · verify · audit · mutation · monitor · commit`. A stage that does not apply (no acceptance examples, mutation or the monitor not configured) is shown only with `-v`.

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

In the dashboard, `tab` switches pane, `↑ ↓` or `j k` select, `a` approves the selected waiting call, `r` rejects it with a reason, `q` quits. Its footer shows the next step.

A run that is picked up after a kill starts with `Resuming — T004 was interrupted during attempt (run_command cargo test); finished calls will not repeat.`

### Getting your attention

When a call waits for you, a task fails, the feature ends or nothing happens for 15 minutes, speccoder notifies you — a desktop notification in terminals that show one (iTerm2, WezTerm, Ghostty, Kitty, Warp, foot, Konsole), a bell elsewhere — and the terminal title follows the run (`speccoder ▸ T004`, `speccoder △ approval T004`, `speccoder ✓ done`). Nothing of this happens with `--json` or in an editor. It is your setting, not the project's; see [your settings](@/docs/configuration.md#your-settings).

## Cost

```sh
speccoder cost     # tokens per task — every attempt — convergence, and the total
```

`cost` reads the usage speccoder records on its commits and saved attempts. For money, add your prices in USD per million tokens to `[model]` (`price_input_per_mtok`, `price_output_per_mtok`, `price_cache_read_per_mtok`, and optionally the cache-write rates); they then also appear in the progress lines. Tokens written to the cache are priced at their own rate — by default Anthropic's 1.25× input for the five-minute cache and 2× for the hour, the input rate elsewhere. speccoder ships no price list — prices change and depend on your contract. With Claude Code as the agent, its own reported cost is used.
