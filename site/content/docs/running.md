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

| Flag | Does |
|---|---|
| `--task T005 --fresh-acceptance` | Sets the task's held attempt aside and writes its acceptance tests anew |
| `--allow-failing-baseline` | Attempts although a check cannot run on the untouched code (that check can never judge the task) |
| `--ignore-checklists` | Runs although `checklists/*.md` has unchecked items (recorded on every commit) |
| `--in-place` | Works in your checkout instead of a worktree; see [in-place runs](#in-place-runs) |

A run started with **Approve and Run** uses the tests [frozen at review](@/docs/planning.md#review-the-tests); on [autopilot](@/docs/approvals.md#autopilot) it asks nobody.

## When the setup stops a run

A file the checks need that your checkout never committed (`speccoder.toml`, `.config/nextest.toml`), a lockfile the checks write, or build output `.gitignore` does not list stops a run with `error[SETUP-INCOMPLETE]` (exit 14).

```sh
speccoder fix --dry-run   # what it would commit
speccoder fix             # commit it in your checkout as `speccoder: setup`
```

`fix` copies a lockfile only the worktree's checks wrote, adds build output to `.gitignore`, and commits exactly those files and the uncommitted configuration. Undo: `git reset --soft HEAD~1`. At a terminal `run` offers this itself.

## Dependency folders

A worktree holds only what is committed, and `node_modules/` or `.venv/` never are. Before the baseline, the step `prepare` gives each Node or uv project in the worktree its dependency folder, once per lockfile: the next task reuses it, and a branch that changes its lockfile is prepared again. Go and Rust need no step.

- **Installed** (the default): the toolchain's locked install from its cache, offline and sandboxed — `npm ci --offline`, `pnpm install --frozen-lockfile --offline`, `yarn install --frozen-lockfile --offline` (Yarn 2+: `--immutable`), `bun install --frozen-lockfile`, `uv sync --locked --offline`. A package the cache lacks stops the task with `error[PREPARE-FAILED]`: run the install once in your checkout, or let its online form reach the network (`[sandbox] network_allow = [["npm", "ci"]]`). A project `speccoder new` made installs from the cache its own steps filled.
- **Linked** (`[worktree] link = ["node_modules", ".venv"]`): a symlink to your checkout's folder, read-only to every command. Made only while the folder is git-ignored in your checkout, the worktree's lockfile is byte for byte yours, commands are sandboxed, and — for a `.venv` — the project's package imports from the worktree; checks then get the worktree's source root first on `PYTHONPATH`. Otherwise the folder is installed and the run says why. git never sees the link: speccoder adds `/node_modules` under `# speccoder: [worktree] link` to your repository's `.git/info/exclude`, its only write outside the worktree. Each tick names the links and their lockfiles' digests (`Linked-Dependencies:` and the [signed statement](@/docs/task-verification.md)).

Lane and bisection worktrees are prepared the same way; in-place runs use your checkout's own folders; `[worktree] prepare = "off"` skips the step. `speccoder doctor` says for each linked folder whether a run would link it.

## Changes set aside

Uncommitted changes found in a feature's worktree when a run starts (edits by hand, without an interrupted attempt) are saved under an attempt ref and set aside; the task starts from the branch tip. They wait in **Needs You** and on the task's card:

```sh
speccoder task restore T005   # commit them onto speccoder/<feature> as yours
speccoder task drop T005      # discard them
```

`restore` merges them when the branch moved since. It changes nothing when they conflict, or when the worktree has uncommitted edits to tracked files.

## The model's account

Each turn, the model's summary of its reasoning and its words beside its calls are shown muted: in the terminal as `∴ …` and quoted text (in full with `-v`; `model thinks:` / `model says:` in screen-reader mode), the latest of each in the dashboard and `report --live`, per turn in the [app](@/docs/web.md)'s inspector, as the agent's thoughts and message in an editor, and afterwards with `speccoder task show T005 --account`. A verdict never rests on it. The summary comes from the provider (Anthropic, OpenAI Responses; an OpenAI-compatible server and Bedrock give none) and is asked for on the attempt's turns only; `[model] reasoning_summary = false` turns it off. With the OpenAI Responses API, the words are the model's commentary, which that API marks apart from its answer; a final turn's commentary is shown too.

## Quick changes

A small change without a planning round, judged like any other (Spec Kit projects):

```sh
speccoder do "Show the version number in the footer" --check "every page's footer shows the version"
speccoder do "Make the login button blue" --no-run      # record it; prints the run --task command
```

`do` records the change in a standing feature, `NNN-quick-changes`: a user story (your words, each `--check` as an acceptance scenario), an approved requirement and a task citing it. It commits exactly those files (`spec(NNN-quick-changes): quick change: …`) and runs the task. Quick changes do not wait for each other. In the [app](@/docs/web.md): **Quick Change**, or `q`. Bugs go through [`speccoder bug`](@/docs/bugs.md).

## Outcomes

| Outcome | Exit | Meaning | What to do |
|---|---|---|---|
| `verified`, `regression_only` | 0 | Passed: ticked and committed | Review and merge |
| `failed` | 20 | Checks or acceptance tests failed after the retries | Inspect the saved attempt; `task nudge`, or fix the spec |
| `failed` (judge input changed) | 20 | A command changed `speccoder.toml`, CI, test-runner or agent configuration; not retried | The task must not need that change |
| `stopped[ACCEPTANCE-WEAK]`, `stopped[TESTS-OVER-STRICT]` | 23 | The acceptance tests are too weak, or stricter than the spec; not ticked | [Strength](@/docs/evidence.md#acceptance-strength), [over-strict tests](@/docs/evidence.md#over-strict-tests) |
| `failed` (`TESTS-NOT-COLLECTED`) | 20 | A test file the attempt wrote never ran | Retried with the runner's rule; see [tests that never ran](@/docs/evidence.md#tests-that-never-ran) |
| `stopped[TESTS-COLLECTION-UNKNOWN]` | 23 | speccoder cannot tell whether a changed test file ran; not ticked | Declare `junit` or `list` for the check, or `task accept` (it ticks the saved attempt's code) |
| `acceptance_failed` | 23 | The acceptance tests could not be written (`ACCEPTANCE-UNWRITTEN`); nothing was attempted | Check the provider and key, then run again |
| `error[NO-TASKS]` | 16 | The feature has no task list yet, or one without a task: nothing ran, and it is not done | `speccoder spec tasks` (OpenSpec: `spec propose`), then run |
| feature done, full tier failed | 20 | Every task is ticked, but a `full` check failed | Fix it, or `speccoder pr --force` |
| unfinished checklists | 23 | `checklists/*.md` has unchecked items; nothing ran | Finish them, or `--ignore-checklists` |
| `blocked` | 23 | The model says the task cannot be done as written | Fix the spec or a wrong test, or `task skip` |
| `needs_approval` | 23 | A call needs a person | `task approve` or `task reject` ([approvals](@/docs/approvals.md#later)) |
| `denied` | 23 | Three denied calls in a row | Usually a task that asks for something protected |
| `not_demonstrated` | 23 | A convergence gap no failing test shows | Decide whether it is real; `task skip` or fix it |
| `suspected_gaming` | 23 | The monitor suspects a shortcut | Review the saved attempt; `task accept` if it is fine |
| `budget_exhausted`, `incomplete` | 23 | Out of turns or calls, or the model did not finish | Raise `[budget]`, split the task, or `task nudge` |
| `interrupted` | 23 | Killed while a tool call was running | Run again; see [recovery](@/docs/recovery.md) |

The `task` commands (`approve`, `reject`, `accept`, `skip`, `nudge`, `rule`, `restore`, `drop`, `show`) are in the [CLI reference](@/docs/cli.md#work). `task accept` ticks on your word and shows as `attested`, never `verified`.

## Where tests go

speccoder detects the project's test layout — each runner's collection rule, where tests live, the naming habit, files nothing runs — and gives it to the implementer and every test author.

- Its acceptance tests go to **one file per story** (`tests/acceptance_us1.rs`, `tests/test_acceptance_us1.py`, `src/acceptance-us1.test.ts`, `<package>/acceptance_us1_test.go`), run by the command that runs only that file (`cargo test --test acceptance_us1`).
- A later task of the story appends its section; changing, inserting among or skipping earlier tests is refused. Convergence tasks keep a file of their own.
- With no place the runner collects (Cargo's `autotests = false`, a missing `testpaths`), no acceptance test is written and `speccoder doctor` names the setting.

## Retries

A failed verdict is retried up to `[budget] max_attempts` (default 2). The retry starts from the same clean checkpoint and is told the verifier's facts (which check, which test, the output), not the previous model's transcript. Identical failures twice in a row, infrastructure failures (a check that cannot even start) and integrity findings are not retried.

## Parallel tasks

`speccoder run --parallel N` runs up to N (at most 8) ready `[P]` tasks at once, each in its own **lane**: a worktree on `speccoder-lane/<feature>/<task>`, started at the feature branch's tip. Lanes never ask inline; a call needing approval stops its lane. speccoder merges each passed lane into `speccoder/<feature>` and **runs the checks on the merged whole**:

- a lane that does not merge runs again on its own, on top of the others;
- tasks that pass alone but fail together are undone, and the remaining tasks run one at a time.

## In-place runs

```sh
speccoder run --in-place
```

A worktree holds only what is committed, so it cannot see specs you keep out of git (`specs/` and `.specify/` in `.gitignore`). `--in-place` works in your checkout instead:

- It needs a clean checkout (else `error[DIRTY-CHECKOUT]`, exit 14), at least one commit, and `speccoder.toml` committed.
- It switches the checkout to `speccoder/<feature>` (created from your current commit, or reused), runs the same judge as worktree mode, and **switches back** to your branch when the run ends, however it ends. Your branch is never committed to.
- Each passed task is committed with its evidence first and ticked second. When `tasks.md` is ignored, the tick lives only in your checkout; the commit says so (`Tick: local specs/<feature>/tasks.md`) and records the judged task line (`Task-Line:`) and intent (`Intent-Digest:`).
- `speccoder report` reads local ticks against those commits: a tick without such a commit, or a line reworded since, counts as without evidence; a commit whose tick is missing shows as `evidence_without_tick`.
- Do not use the checkout while it runs. A change to HEAD, the branch, the tree, the spec, plan, tasks, constitution or the judge's inputs stops the run (`stopped[CHECKOUT-CHANGED]`, `stopped[INTENT-CHANGED]`, `stopped[JUDGE-CHANGED]`, exit 23) and leaves your change where it is. What is guarded and what is not: [in-place mode](@/docs/security.md#in-place-mode).
- `--parallel` is refused with `--in-place` (`error[IN-PLACE-PARALLEL]`, exit 2).

`task accept --in-place` and `converge --in-place` work the same way: an attestation is committed before its local tick; a convergence commit says `Tasks: local`. While a feature has an in-place run (or an interrupted one), a worktree-mode `run`, `task accept` or `converge` of it is refused (`error[IN-PLACE-SESSION]`, exit 14); see [recovery](@/docs/recovery.md#interrupted-in-place-runs).

## Best of n

With `[budget] best_of = 3` (default 1), each task gets three independent attempts from the same start, and speccoder keeps the best by evidence (`verified` over `regression_only`, then fewer surviving mutants, the smaller diff, fewer tokens), at n times the cost.

## Convergence

```sh
speccoder converge
```

Once every task is ticked, a read-only model call compares the code with `spec.md`, `plan.md`, `tasks.md` and the constitution and lists gaps: `missing`, `partial`, `contradicts`, `unrequested`. Actionable gaps are appended as a new `## Phase N: Convergence` in `tasks.md`, gravest first, and committed; `run --converge` works through them and assesses again. It stops for you when only unrequested findings remain, when findings repeat earlier convergence tasks, or after `[converge] max_rounds` (default 3).

## Repository instructions

speccoder gives its model the repository's [`AGENTS.md`](https://agents.md) files with every task: from the root down to the directories the task names, one per directory, root first, so a nearer file has the last word. The acceptance test author gets them too; the gaming monitor does not.

- They rank below the task, the spec, the checks and speccoder's rules.
- At most 32 KiB: the nearest files whole, farther ones cut, and the report says so.
- They are read from the commit the attempt starts from; the run summary (`Guided by  AGENTS.md, services/api/AGENTS.md`) and the attestation name them with each file's hash.
- Changing an `AGENTS.md` during an attempt needs your approval.
- External agents (Claude Code, Codex) read their own files; speccoder does not add them twice. Other file names, such as `CLAUDE.md`, are a [setting](@/docs/configuration.md#context).

## The dashboard

At a terminal, `speccoder run` runs inside the dashboard. A call that needs you appears over the screen with the [inline prompt](@/docs/approvals.md#inline)'s question, scope and keys. When you leave, the run's summary is printed as plain `run` prints it, and the exit status is the run's.

| Screen | Shows |
|---|---|
| `1` Run | Tasks, the running task's pipeline, tool calls, usage, the approval queue and `Next:` |
| `2` Evidence (`enter` on a task) | The verdict in words and every piece of evidence behind it, read from the task commit's [attestation](@/docs/attestations.md); last, muted, what the model said |
| `3` Report | A verdict bar (`[###~A..]` in ASCII), the totals, what waits for a person, requirements no task cites, convergence rounds, `Next:` |

| Key | Does |
|---|---|
| `1` `2` `3`, `enter` | Switch screens; `enter` opens the selected task's evidence (or a waiting call) |
| `d` | The task commit's diff: `]` `[` next and previous hunk, `n` `p` next and previous file |
| `tab` | Switch between the task list and the approval queue; there `a` approves, `r` rejects with a reason, `B` decides a [batch](@/docs/approvals.md#the-asking-budget) |
| `n` | Leave and take the next step at the terminal (once the run has ended) |
| `f` | Pick the feature shown, making it [current](@/docs/planning.md#the-current-feature) (once the run has ended); with several features and none current, the dashboard opens on this picker |
| `esc` | Back: out of an overlay, then to the Run screen; never quits |
| `q`, `ctrl-c` | Quit; while the run works it asks: **finish this task, then stop**, **stop now** (the attempt is kept and the next `run` resumes it), or keep watching |
| `?` | Every key |

The dashboard needs a terminal of at least 60×15; `--plain` (or no terminal, `--json`, `--events`, screen-reader mode) gives line output. To watch a run from elsewhere:

```sh
speccoder report --live   # running (pid …): 001-x T002 — attempt 1/2 for 33s; last: read_file … (allow)
speccoder                 # at a terminal: the same dashboard, watching (leaving does not stop the run)
```

## Watching a run

Every task goes through the same stages, named the same everywhere: `baseline · red · attempt · verify · audit · mutation · strength · monitor · commit`, after `prepare` where the worktree has [dependency folders](#dependency-folders). A stage that does not apply is shown only with `-v`. The [quickstart](@/docs/quickstart.md#5-run) shows the `--plain` lines. A run picked up after a kill starts with `Resuming — T004 was interrupted during attempt (run_command cargo test); finished calls will not repeat.`

### Look and accessibility

Each state has its own colour, mark and word: `✓` verified, `~` regression only, `A` attested, `!` no evidence, `–` skipped, `·` ready, `⧗` blocked. `--ascii` (or a non-UTF-8 locale) uses `[ok] [~] [A] [!] [-] [ ] [..]` and plain borders; `NO_COLOR` turns colour off; themes are in [your settings](@/docs/configuration.md#your-settings).

`--screen-reader` (or `SPECCODER_SCREEN_READER=1`, or `[ui] screen_reader = true`) gives labelled plain lines, choices as a numbered list, and the report instead of the dashboard:

```text
task: T004 Handle Unicode input — started
stage: T004 attempt — started (1/2)
approval required: T004 — run_command cargo add unicode-normalization
choice 1: Yes, once (or type y)
```

### Getting your attention

When a call waits for you, a task fails, the feature ends or nothing happens for 15 minutes, speccoder sends a desktop notification in terminals that show one (iTerm2, WezTerm, Ghostty, Kitty, Warp, foot, Konsole), a bell elsewhere. The terminal title follows the run (`speccoder ▸ T004`, `speccoder △ approval T004`, `speccoder ✓ done`). Neither happens with `--json` or `--events`. Configure it in [your settings](@/docs/configuration.md#your-settings).

## Cost

```sh
speccoder report --cost     # tokens per task, every attempt, convergence, and the total
```

`report --cost` reads the usage recorded on commits and saved attempts. For money, set your prices in [`[model]`](@/docs/configuration.md#model) (`price_*_per_mtok`); they then also appear in the progress lines. speccoder ships no price list. With Claude Code as the agent, its own reported cost is used.
