+++
title = "Reading the evidence"
description = "What each task state means, where the evidence lives in git, how the diff audit, acceptance strength, mutation testing and the gaming monitor work, and how to inspect any attempt."
weight = 60

[extra]
group = "Use"
+++

## The report

```sh
speccoder report               # every task
speccoder task show T004       # one task and its evidence
```

<figure class="shot"><img class="shot-light" src="../../screenshots/drawer-light.webp" alt="The evidence of one task in the app: acceptance red to green, checks, diff audit, attestation" width="1440" height="900" loading="lazy" decoding="async"><img class="shot-dark" src="../../screenshots/drawer-dark.webp" alt="The evidence of one task in the app: acceptance red to green, checks, diff audit, attestation" width="1440" height="900" loading="lazy" decoding="async"></figure>

| Mark | State | Means |
|---|---|---|
| `✓` | `verified` | Spec-derived acceptance tests went red → green; nothing regressed |
| `~` | `regression_only` | Checks pass; nothing task-specific was shown |
| `A` | `attested` | Ticked by `speccoder task accept`: a person's word, not evidence |
| `!` | `ticked_without_evidence` | Ticked, but not by speccoder |
| `?` | `evidence_without_tick` | In place: the verdict is committed, the tick in `tasks.md` is not; the next `run --in-place` writes it |
| `–` | `skipped` | Passed by on purpose; tasks that depend on it stay blocked |
| `·` | `ready` | Can run now |
| `⧗` | `blocked` | Waits for the tasks listed |

`report` reads only git, so it works on any clone. It also traces requirements (`FR-001`, `SC-002`) to the tasks that cite them (`Trace 9/11 requirement(s) cited by a task; not cited: FR-007, SC-003`), counts ticks without an [attestation](@/docs/attestations.md), and marks [stale ticks](#when-the-spec-changes).

## When the spec changes
A tick is proven against the spec as it was at the task's commit. When a requirement it cites is reworded, added or removed afterwards, the tick is **stale** (`spec changed: FR-001 reworded since the tick`).

- speccoder compares each tick's commit with your committed `HEAD`: the feature's documents (the task list aside) and shared ones such as the constitution. Uncommitted edits and other features do not count.
- A requirement is cited when its ID is a word in the task's title.
- **Stale** (a cited requirement changed): the verdict stays; the report, the board, the inspector and the pull request mark it, and the requirements matrix stops counting it as proven. **Changed elsewhere** (the plan, the constitution, an uncited part of the spec): shown, not counted.
- `speccoder run --converge` re-checks: convergence is told which requirements changed and adds tasks for what no longer holds. speccoder never unticks a task on its own.
- In place with an ignored `specs/`, the commits' `Intent-Digest` tells that the intent changed, not where.

## What counts as red

Acceptance tests are evidence only if they failed before the story's work existed. speccoder reads the runner's report (JUnit, or the summary of libtest, nextest, pytest, jest, vitest and `go test`):

| Red | Means |
|---|---|
| `assertion` | A collected test failed on an assertion |
| `absent` | The tests could not build or import only because what the task adds does not exist yet (rustc `E0425`, Python `ImportError`, TypeScript `TS2305`, Go `undefined:`, …) |

Anything else is **not demonstrated** (another build or syntax error, a collection error, a timeout, no tests collected, unrecognised output): the task can then be at most `regression_only`, and the report quotes the first lines of the error.

- Red is confirmed by a second run; red once and not again is `flaky`, not red.
- A check that fails, then passes on a rerun, is `flaky`: it does not fail the task, and a flaky task-specific check is never evidence for `verified`.
- If writing the acceptance tests fails, nothing is attempted and the task stops for you.

## Tests that never ran

A test that never ran is evidence of nothing. After every attempt, each test file it added or changed (a test path that defines tests: `#[test]` and `#[<crate>::test]`, `def test…`, `it(`/`test(`, `func Test…`) is checked against what the fast checks collected — a test counts only if a check the verdict rests on runs it, and ignored or skipped tests never count. In this order:

1. The toolchain's static rule, which alone can prove a file is never collected: Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` and the modules they declare (`mod`, `#[path]`, `autotests`, `[[test]]`); pytest collects `python_files` under `testpaths`, outside `norecursedirs`; vitest and jest collect their `include`/`testMatch` (read from `package.json` or a JSON config, never from a config written as code); Go runs `*_test.go` outside `testdata/` and `_`/`.` directories, under the build tags the check sets, with functions named `Test…`, `Example…`, `Fuzz…`.
2. The check's JUnit report, when it was freshly written by this run.
3. A listing built from the command of a fast check that runs the toolchain — its packages, targets, paths and filters kept: `cargo nextest list`, `cargo test … -- --list` (with `--ignored` to leave ignored tests out), `pytest --collect-only`, `vitest list`, `jest --listTests`, `go test -list` — or the check's own [`list`](@/docs/configuration.md#checks-name) command; sandboxed, under the check's deadline. A listing that changes the worktree fails the attempt.

A file from which no test ran fails the verdict with the rule as the reason, and the retry is told the file, the rule and the ways out:

```text
tests not collected: `tests/unit/greet_name.rs` — Cargo builds `tests/*.rs` and `tests/<dir>/main.rs` (and the modules they declare); this file is neither
```

A file none of the three can decide stops the task, never ticked (`TESTS-COLLECTION-UNKNOWN`): one no fast check runs, one with a skip a listing cannot see (`pytest.mark.skip`, `it.skip`, `t.Skip`), or one the static rule cannot settle. Declare a JUnit report or a `list` command for the check, or `speccoder task accept` the task (it puts the saved attempt back before ticking).

- Test helpers without tests, deleted files and build output are not judged.
- A JUnit report counts only when this run wrote it: an older one is removed first, and one tracked by git is never read.
- The evidence shows a **Collection** row; the signed statement carries the files and what decided each.

Acceptance and held-out tests count at green only when the green run collected at least as many tests as the red run — or, when red reported no count (it did not build), as many as the files declare — and at least one. A green run whose count cannot be read is no evidence.

## What judges each task

A story's acceptance is written once — its **oracle** — and every task of the story is judged by the progress it makes on it.

- **The oracle.** Before a story's first code task implements anything, one author call writes a test for every scenario of the story, through the interface its *Independent Test* names (a command, an API, a page), each scenario first stated as a contract (`a table on stdin → its CSV on stdout`). Tests you [froze at review](@/docs/planning.md#review-the-tests) are the oracle instead, with no author call.
- **Kept tests.** Each test runs alone: red or already green, it is kept; one that does not build or collect, or that tests several scenarios at once, is asked for once more, then left out (the scenario is *untested*): each scenario turns green on its own. The tests are committed before any code (`speccoder: oracle US1`) and frozen.
- **Progress.** A task ticks `verified` for the scenarios it turned green (`Scenarios: US1-1@3f9a0c1d2e4b`, a digest of each one's wording), `regression_only` (`in_progress`) when it turned none and broke none. A scenario green before it and red after it fails the verdict. Scenarios that need later tasks may stay red, and a check never fails on them: the oracle's tests are the oracle's to judge.
- **Linked.** A task that names its scenarios — `(scenarios US3-1, US3-2)`, or `US3/AC2` as Spec Kit's converge cites them — must leave exactly those green. A link to no scenario of the story is ignored and reported (`PLAN-SCENARIO-UNKNOWN`).
- **Checkpoint.** The story's last open code task must leave every scenario green, and the story's held-out tests passing. Otherwise it stops (`STORY-OPEN`), naming each red scenario with its contract: leave a note (`task nudge`) and run again, or — when a test asserts what the spec does not say — `speccoder task rule T021 --tests-wrong --scenario US3-2 --reason "…"`; the next run writes that scenario's tests again, and nothing else (`Oracle-Amends: US3-2`). A scenario its author declared untestable is shown, and never blocks the story.
- **Outside a story.** A convergence gap writes its own test, cited to the scenario it closes; a bug is judged by its reproductions. When that test asserts what the spec does not say, the stop names it: `speccoder task rule T027 --tests-wrong --reason "FR-005 keeps cell whitespace"` has it written again, your words told to its author and recorded on the tick (`Ruling: tests-wrong`).
- **The suite.** Every test author is shown the project's tests on the code its task or story names in backticks, and told not to contradict them; a scenario it could only state against them is *untested*.

`speccoder report` and the board show each story on its oracle — `US1 ●●○ 2 of 3 green · T008 turned US1-1, US1-2 · T010 closes it` — and each open task what judges it. A story whose every task is documentation or yours has no checkpoint (`PLAN-STORY-UNJUDGED`); one ticked before it had an oracle reads *ticked before its oracle*, its scenarios never run.

## Why a tick is regression-only

Every `regression_only` tick carries one reason, decided by code, recorded in the commit (`Regression-Only:` trailer) and its signed statement, and worded the same everywhere:

| Reason | Means |
|---|---|
| `acceptance_off` | `[acceptance] generate = false` |
| `no_story` | The task belongs to no user story: nothing to derive a test from |
| `no_scenarios` | Its story states no acceptance scenario |
| `author_failed` | The test author's files could not be used (refused path, command not allowed, no place the runner collects) |
| `author_wrote_none` | The test author wrote no test |
| `tests_not_collected` | The acceptance command collected no test |
| `not_red` | The acceptance tests did not fail first (build error, timeout, flaky, …) |
| `already_passing` | Its story's scenarios were green before any of the story's work (built earlier: a Setup task, before the feature) |
| `planning_work` | A planning task: design documents, not code ([who does a task](@/docs/running.md#who-does-a-task)) |
| `in_progress` | A task of a story that turned none of its scenarios green and broke none |
| `story_verified` | Its story's checkpoint, every scenario turned green by earlier ticks |
| `regression_unchecked` | The test check failed before and after without per-test results, so a regression could not be told apart from the story's red tests — set `junit` on the check |
| `unrecorded` | Ticked by an earlier speccoder: the reason was not recorded |

A feature with at least three ticks of which more than half are regression-only for a reason other than progress (`in_progress`, `story_verified`, `planning_work`) gets one **Little Was Verified** item in Needs You, with the reasons by count and the fix, until its branch is pushed.

## Held-out tests

Frozen visible tests stop an implementer from editing the tests, not from tailoring code to them. So when a story's oracle is written, a second author call writes **held-out** tests for the same scenarios, with other inputs and the edge cases the spec states. They must be red too.

At the story's checkpoint, after an attempt passes the visible checks, speccoder writes the held-out tests over the final tree, runs them in the sandbox and restores the tree. Closing the story needs both. (A convergence gap and a bug have held-out tests of their own.) A failing held-out test fails the verdict; the retry is told only the scenario (`held-out acceptance failed: US1-2 ("Given …")`), never a path, test name, input or output.

| Outcome | Means |
|---|---|
| `passed` | Every held-out test passed; the commit carries `Held-Out: passed=2/2 manifest=sha256:…` |
| `failed` | Some failed: the attempt fails and is retried |
| `absent` | None was kept: not generated, no visible tests to vary (`no_visible_tests`), or every file discarded (`not_red`, `unknown_scenario`, `path`, `command`, `limit`, `collision`). The commit says `Held-Out: absent=<reason>`; the verdict rests on the visible tests |
| `tampered`, `leak_suspected` | The stored set changed, or the attempt contains a literal only the held-out tests hold: stopped for a person, saved, not ticked |

`[acceptance] held_out = false` turns held-out tests off; the checkpoint's commit says `Held-Out: off`.

### Where they are kept

Held-out tests never enter the worktree (except overlaid while a judge run checks them), the project's repository or any prompt the implementer gets; the app, the CLI and the events show only their count, state and digest. Since every attempt can read the project's refs and objects, they live in speccoder's state directory (`$SPECCODER_STATE_DIR`, else `$XDG_STATE_HOME/speccoder`, else `~/.local/state/speccoder`), under the repository's own directory:

| Path | Holds |
|---|---|
| `held-out/<feature>/req-<story>/` | A story's sealed set: `manifest.json` and `files/…` (a convergence gap's: `held-out/<feature>/<task>/`) |
| `held-out.git` | A private bare repository (borrowing the project's objects) where publications are built |

**Publication.** `refs/speccoder/held-out/<feature>` carries exactly the sets whose manifest digest a task closed on the pushed head names in its signed evidence (or `Held-Out:` trailer); sets of open tasks stay local. A feature run builds it when every task is ticked; [`speccoder pr`](@/docs/github.md#its-pull-request) builds and pushes it from the private store, never forced, writing nothing into the project's repository. Each publication is parented on the branch and the previous one, so the ref only grows.

**External agents** (`[executor]`) run with your permissions, so the set is *withheld, not sealed*: it lies outside the worktree, where speccoder refuses every read the agent asks for, and a held-out literal copied into the attempt stops the tick.

## Where the evidence lives

Each passed task is one commit on `speccoder/<feature>`, with trailers:

```text
speccoder: T002 Implement `compare(…)`

Spec-Task: 001-semver/T002
Verdict: verified
Red: assertion
Acceptance-Strength: 0.83 mutants=9/11 near-miss=1/1
Run: run_01M5…
Usage: input=40250 output=3383 cache_read=30976 cache_write=0
Mutation: caught=26 missed=13 timeout=0 unviable=6
```

When they apply:

| Trailer | Says |
|---|---|
| `Scenarios:` | The scenarios the task turned green, each with a digest of its wording |
| `Regression-Only:` | Why a regression-only tick has no red → green evidence (`in_progress`, …) |
| `Held-Out:` | The [held-out tests](#held-out-tests) |
| `Acceptance-Strength-Visible:` | Strength of the visible tests alone, with held-out tests on |
| `Acceptance:` | Tests decided before the run: `<source> manifest=sha256:… approval=sha256:… wording=sha256:… by=…` (see below) |
| `Ruling:` | A person's ruling on the tests: `tests-right` on an [over-strict test](#over-strict-tests), `tests-wrong` on a task's own test |
| `Flaky:` | Checks that passed only when run again |
| `Scope-Outside:` | Changed files the task line did not name: reported, never a failure |
| `Checklists: overridden` | The run was started with `--ignore-checklists` |
| `Bug:` | A [bug fix](@/docs/bugs.md): `<outcome> (<home>) triad=sha256:…` |

A story's oracle is its own commit, `speccoder: oracle US1`, before the story's first code:

| Trailer | Says |
|---|---|
| `Oracle:` | The story |
| `Oracle-Scenarios:` | Each scenario's state when written: `US1-1=red, US1-2=green, US1-3=untested` |
| `Oracle-Tests:` | How many tests it holds |
| `Oracle-By:` | Who decided them: `speccoder run`, or the person who froze them at review |
| `Oracle-Untestable:` | Scenarios its author could not test, with why |
| `Oracle-Amends:` | The scenarios a ruling had written again |

Each tick commit also carries signed statements of the same evidence in a git note ([attestations](@/docs/attestations.md)).

After a run ticks the feature's last task, the `full` checks run once; the result is recorded under `refs/speccoder/full/<feature>`, and `speccoder pr` refuses a branch whose full checks failed unless `--force`.

Every attempt that did not pass is a commit under `refs/speccoder/attempts/…` (`git for-each-ref refs/speccoder/attempts/`), then undone in the worktree; its message holds the outcome, reason, run and usage, and `git diff speccoder/<feature> <ref>` shows what it changed. [Replay](@/docs/recovery.md#replay) re-derives any recorded run without a model.

**Tests decided before the run.** The run report's `acceptance` section names where the visible tests came from: `authored` (written by the run), `review` ([frozen at review](@/docs/planning.md#review-the-tests)), `bug_confirmation` or `person_test` ([bug fixes](@/docs/bugs.md)). Pre-decided tests carry their confirmation's digests in `pre_frozen` and on the `Acceptance:` trailer; for `review`, the approval digest is the one `review.json` records, so a reviewer can match the tick to the committed sign-off.

## The diff audit

Whatever the checks say, a verdict fails when the attempt:

- deletes or rewrites a test file that existed before it (adding tests can be allowed: see [frozen tests](@/docs/approvals.md#frozen-tests));
- removes more test functions than it adds (the finding names them);
- adds skip markers (`#[ignore]`, `@pytest.mark.skip`, `.skip(`, `xit(`, …);
- changes anything that decides verification: `speccoder.toml`, CI configuration, test-runner configuration;
- changes agent configuration (`.claude/`, `.agents/`, `.codex/`, `.gemini/`, `.cursor/`, `.kiro/`, `.mcp.json`, …), whose hooks and settings run code in a later agent session;
- changes an agent context file (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`) without a person's approval.

The audit reads the diff, so it catches what a command, an MCP server or an external agent wrote, including files git ignores (an agent loads `.claude/settings.local.json`). These inputs are also re-checked after every command: a command that changes one ends the attempt at once.

## Acceptance strength

Red → green shows the task's work made the tests pass, not that they would fail a *wrong* implementation, and many model-written tests would not ([SWE-Mutation](https://preview.aclanthology.org/ingest-acl/2026.findings-acl.1976/), [*Beyond Fail-to-Pass*](https://arxiv.org/abs/2607.19843)). So after every `verified` verdict speccoder runs wrong implementations against the acceptance tests:

- **near misses**: variants of the changed code (`[acceptance] near_miss`, default 2, at most 3), written by a separate call that sees the spec and the code but never a test; each is wrong about exactly one scenario;
- **mutants**: with `[mutation]` configured, the tool's mutants of the changed code, judged by the acceptance command alone.

A variant the tests fail on an assertion or a timeout is **rejected**; one that does not build is **unviable** and not counted; one that passes **survived**. Strength is rejected / judged:

```text
Strength   0.77 mutants=9/11 near-miss=1/2
  survived: near-miss-2 (US1-2: an empty name is accepted)
```

- It is recorded on the tick (`Acceptance-Strength:`), in the report, the attestation and `--json` (`strength`); with [held-out tests](#held-out-tests), as two numbers: visible alone, and visible plus held-out (what the verdict rests on).
- Survivors' patches stay in the state directory; none reaches the implementing model.
- `[acceptance] strength = false` turns it off.

Strength changes a verdict only with `[acceptance] min_strength` (for example `0.8`). A set below it is strengthened **once**: the test author is told what got through and adds tests, which must fail without the change and pass with it; then it is measured again. Still below, or not measurable (no variant, a tool error, the budget ran out), the task stops: `stopped[ACCEPTANCE-WEAK]`.

### Over-strict tests

The variant author also writes alternatives the spec **permits** (`[acceptance] permitted`, default 1; `0` turns this off): another order where the spec fixes none, other wording, another tie-break. If the acceptance tests reject one on an assertion, they may encode a choice the spec did not make. The task stops with `stopped[TESTS-OVER-STRICT]` (the scenario, the alternative's patch, the failing test), and its attempt is **held**: its tree stays, the feature's other tasks wait.

```sh
speccoder task rule T004 --tests-right --reason "the plan fixes the order"   # the tests are right
speccoder run --task T004                     # judges the held attempt again, no model call, and ticks it
speccoder run --task T004 --fresh-acceptance  # the tests are too strict: write them anew, attempt again
speccoder task accept T004 --reason "…"       # tick it on your word (attested)
```

`task rule --variant permitted-1` rules on one finding; without it, on every finding held. A ruling is bound to that alternative's hash and recorded, with who made it, on the tick (`Ruling:`) and in the attestation. The implementing model is never told.

## Mutation testing

With `[mutation] tool = "cargo-mutants"`, speccoder mutates **only the code the passing attempt changed** and runs the tests against each mutant; a mutant the tests miss is behaviour nobody checks. Strength asks whether the *acceptance tests* tell right from wrong; mutation testing asks whether *all* your tests check the changed code.

- The report shows caught and surviving mutants; the commit carries `Mutation:`.
- `mode = "report"` (default) records it; `mode = "require"` fails the verdict on survivors and tells the retry which survived (strict: some mutants are equivalent to the original).
- Other languages: `tool = "command"` with any tool that writes the standard mutation-testing JSON report (Stryker, Infection, PIT); see [configuration](@/docs/configuration.md#mutation).

## The gaming monitor

With `[monitor] enabled = true`, a separate model call reviews every passing attempt. It sees the task, the diff and the tool calls (never the implementer's explanation) and looks for hard-coded results, special-cased inputs, weakened tests and similar shortcuts. A suspicion keeps the task unticked (`suspected_gaming`) until you decide. It only adds doubt: it never makes a failing task pass, and the implementer never learns what it found.

## What the evidence does not show

- `verified` means no more than the checks and tests you configured and the examples in your spec.
- A strength of 1.0 means the tests rejected every wrong variant tried, not every possible one; a surviving near miss may be one its author got wrong.
- A spec without acceptance examples yields `regression_only`; a wrong example yields a wrong test.
- What speccoder cannot demonstrate it reports as such (`not_demonstrated`, `blocked`), never guessed.
