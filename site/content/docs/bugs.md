+++
title = "Bug fixes"
description = "speccoder bug: a bug in your words, reproduced by a test that fails on the untouched code by an assertion, confirmed by you once, then fixed and judged — with the fix undone rejected."
weight = 52

[extra]
group = "Use"
+++

A fix is `verified` only when a reproduction that **failed on the untouched code by an assertion** passes after it, the tests that passed before still pass, and **undoing the fix makes a reproduction fail again**. A bug that cannot be reproduced goes to you, never "fixed".

```sh
speccoder bug "saving an empty title crashes" \
  --expected "an empty title is refused with a message" \
  --unchanged "a non-empty title is saved as typed"
```

A bug runs through the same pipeline, verdicts and Needs You as a [quick change](@/docs/running.md#quick-changes), plus the triad, the reproduce and confirm stages, and the revert check.

## The triad

Your words, never inferred:

| Statement | Flag | Becomes |
|---|---|---|
| **Current**: what happens | the symptom (positional) | the requirement's context |
| **Expected**: what should happen instead | `--expected` (one or more) | one reproduction each, `E1`, `E2`, … |
| **Unchanged**: what must keep working | `--unchanged` (zero or more) | one test each, `U1`, …, in the pass-to-pass set |

Refused before anything is written: an empty symptom or statement (`BUG-EMPTY`), no `--expected` (`BUG-NO-EXPECTED`), a statement the bug's home could not read back exactly (`BUG-ASSESSMENT-DRIFT`; your words are kept byte for byte, whitespace aside), and an `--issue` that cannot be read (`GITHUB-FAILED`). Text from `--issue` is context for the reproduction author only, never part of the triad. A symptom that reads `confirm` or `run` follows `--`: `speccoder bug -- "run"`.

## Where the bug lives

The spec format decides; the choice and its reason are shown and recorded:

| Project | Home | What speccoder writes |
|---|---|---|
| Spec Kit with its `bug` extension installed and enabled | `.specify/bugs/<slug>/` | nothing: the extension's assess skill writes `assessment.md`, given your triad verbatim; speccoder reads it back |
| Spec Kit without it (or with it disabled) | the standing unit `NNN-bugfixes` | a user story whose acceptance scenarios are the triad, an approved requirement and a task, committed alone as `bug: <title>` |
| OpenSpec | the standing change `openspec/changes/bugfixes/` | a requirement of the `bugfixes` capability, a line of the proposal, a task group |
| Kiro | not supported (`BUG-NO-UNIT`) | nothing |

Whether the extension is installed comes from `specify extension list --json` when the CLI is installed, else `.specify/extensions/.registry` and `.specify/extensions.yml`. In the extension's layout:

- `assessment.md` must carry exactly your statements (whitespace aside) as `Current:` / `Expected:` / `Unchanged:` lines, else `BUG-ASSESSMENT-DRIFT` before reproduce; another layout stops with `BUG-ASSESSMENT-UNREADABLE`.
- The skill may write only its bug's directory.
- A bug you assessed yourself (`/speckit.bug.assess`) continues with `speccoder bug --slug <slug>`.
- A slug is lowercase letters, digits and `-` (at most 64), never a path, wherever it comes from.

## The stages

1. **Reproduce.** A test-blind author (it sees the triad and the code's file list, never a fix) writes one test per *Expected* and per *Unchanged* statement, in new test files (never one that configures how tests run, such as a `conftest.py`). speccoder runs each with its own command for that one test (`[bug] test`, else the toolchain's rule); a run that executes more than that test, or another test's failure, is not its red.
   - Each reproduction must fail on the untouched code **by an assertion of its own, twice**. A build or collection error, a timeout, nothing collected, a flaky failure or an *absent* symbol counts as `not_demonstrated` for the try.
   - Each *Unchanged* test must pass.
   - Up to `[bug] reproduce_tries` tries (default 3, at most 5), rejected ones included; then `cannot_reproduce`.
   - With `[acceptance] held_out` on, a held-out variant per statement is written and sealed; neither you nor the fix sees it. When none can be kept, you are told before you confirm: the fix cannot be verified without one.
2. **Confirm.** You see each reproduction (statement, file, test, the exact command, how it failed) and confirm them **once**, bound to the digest you were shown: at the terminal prompt, with `speccoder bug confirm <unit>/<task> --digest D`, in the [Bug sheet](@/docs/web.md#bug-sheet) or Needs You.
   - A rejection with a reason (`--reject --reason …`) discards that try and starts a new one, told why; it counts against the tries.
   - A confirmation is never asked twice for the same digest, across crashes. Rewording the triad lapses it.
   - Without a terminal (`--json`, `--events`, `--no-prompt`, CI) the bug stops here with `next: speccoder bug confirm …`. No flag skips this.
3. **Fix.** An ordinary task attempt with the confirmed reproductions as its frozen acceptance set.
4. **Unchanged.** Once the attempt's diff is known, the pass-to-pass set is chosen deterministically: baseline-green tests in test files that reference a changed source file (Rust `use crate::…`, Python imports, TS/JS relative imports, Go packages), plus each *Unchanged* test. Each must pass by name; a missing one is a failure; an empty selection is recorded as `none_selected`, never as passed.
5. **Judge.** `verified` also needs:
   - every reproduction red → green, and every held-out variant green on the final tree;
   - the pass-to-pass set green;
   - the **revert** (the final tree with the attempt's source changes undone, its tests kept) rejected by at least one reproduction on an assertion, else `BUG-NOT-DISCRIMINATING`;
   - a **special case** per statement (a fix that only recognises the reproduction's literal input) rejected by the reproductions and held-out variants together. A statement without one withholds `verified`, saying why; one that survives gets one regenerated variant, then stops for you (`BUG-REPRO-WEAK`; `speccoder task rule … --tests-right` if the tests are right).
6. **Ship.** The verdict commit carries `Bug:`, `Acceptance: bug_confirmation …` (or `person_test`) and, with `--issue N` and a `verified` or `attested` verdict only, `Fixes: #N`, so the [pull request](@/docs/github.md#its-pull-request) closes the issue on merge. In the extension's layout nothing is ticked: the commit's `Spec-Task: bugs/<slug>/bug:<slug>` is a *trailer-only task*, anchored on the triad's digest, which [`speccoder attest verify --branch`](@/docs/verifier.md) checks like a tick (`stale` when the triad at the head differs).

## Outcomes

| Outcome | Means | Needs You |
|---|---|---|
| `recorded` | the triad is recorded; nothing reproduced yet | — |
| `awaiting_confirmation` | every statement reproduced; your confirmation | *Blocked on you* (`bug_confirm`) |
| `cannot_reproduce` (`BUG-CANNOT-REPRODUCE`) | the tries ran out; the hand-off lists each try's class and excerpt and which statements were reproduced | *Blocked on you*: bring a `--test`, reword, try again, or skip |
| `already_satisfied` (`BUG-ALREADY-SATISFIED`) | every reproduction passed on every try: nothing to fix, nothing ticked | *Ready to review*: `speccoder bug keep`, or `speccoder task skip` (in the extension's layout `task skip bug:<slug>`) |
| `unchanged_not_holding` (`BUG-UNCHANGED-NOT-HOLDING`) | an *Unchanged* statement does not hold today | *Blocked on you* |
| `fixed` | the fix was judged `verified` or `attested` | — |
| `failed` | the fix did not pass | — |

`speccoder bug keep` commits the last try's reproductions as a regression test and accepts the task as `attested`.

## Your own test, or someone else's fix

- `--test PATH::NAME` (one per `--expected`, in order) uses your failing test as the reproduction. It must exist, be collected and fail by an assertion, twice (`BUG-TEST-NOT-FOUND`, `BUG-TEST-NOT-RED`, `BUG-TEST-PASSES`, `BUG-TEST-COVERAGE`). Naming it is the confirmation; held-out variants are still written. `[bug] test` sets how one named test runs when the toolchain's rule does not fit.
- `--candidate fix.patch` or `--candidate pr:N` judges someone else's fix as the attempt: no implementer, no retries.
  - A diff that touches a frozen reproduction, a held-out path or a judge input (by an edit, a rename, a deletion or a mode change) is refused before it is applied (`BUG-CANDIDATE-JUDGE`).
  - One that does not apply, or carries a binary hunk or a symbolic link, is refused (`BUG-CANDIDATE-APPLY`).
  - Through the web, ACP or MCP a patch file must be inside the project. A pull request's diff is fetched read-only, its head commit recorded.

## Commands

```sh
speccoder bug "<symptom>" --expected "<e>" [--unchanged "<u>"]… [--issue N] [--test PATH::NAME]… [--candidate PATCH|pr:N] [--no-run] [--no-prompt]
speccoder bug --slug <slug>                          # a bug of Spec Kit's bug extension
speccoder bug confirm <unit>/<task> --digest D       # or --reject --reason "…"; --no-run records only
speccoder bug run --task <unit>/<task>               # or --slug <slug>: reproduce again, or run the confirmed fix
speccoder bug keep <unit>/<task> --reason "…"        # already satisfied: commit the reproductions, attested
```

`<unit>/<task>` is, for example, `007-bugfixes/T003` or `bugs/login-timeout/bug:login-timeout`. `--no-run` records the bug (and commits exactly that) without reproducing it. `--json` prints the `bug` payload ([schema](https://hupe1980.github.io/speccoder/schemas/v2/bug.json)); the `bug.*` events are in the [CLI reference](@/docs/cli.md#events).

With the `bug` extension and its `bugfix` workflow, `speccoder workflow install --overlay bugfix` writes `.specify/workflows/overlays/bugfix/speccoder.yml`, replacing the workflow's `test` step with `speccoder bug run --slug <slug>`; `specify workflow run bugfix` then pauses at the confirmation and reports the verdict. `speccoder workflow uninstall --overlay bugfix` removes it.
