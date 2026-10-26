+++
title = "Bug fixes"
description = "speccoder bug: a bug in your words, reproduced by a test that fails on the untouched code by an assertion, confirmed by you once, then fixed and judged — with the fix undone rejected."
weight = 52

[extra]
group = "Use"
+++

Every spec tool has a bug lane now. What speccoder adds is the evidence: a fix is `verified` only when a reproduction that **failed on the untouched code by an assertion** passes after it, the tests that passed before still pass, and **undoing the fix makes a reproduction fail again**. A bug that cannot be reproduced goes to you, never "fixed".

```sh
speccoder bug "saving an empty title crashes" \
  --expected "an empty title is refused with a message" \
  --unchanged "a non-empty title is saved as typed"
```

A bug is the L2 rung: the same pipeline, verdicts and inbox as a [quick change](@/docs/running.md#quick-changes) and a feature, with three additions — the triad, the reproduce and confirm stages, and its wrong candidates.

## The triad

Your words, never inferred:

| Statement | Flag | Becomes |
|---|---|---|
| **Current** — what happens | the symptom (positional) | the requirement's context |
| **Expected** — what should happen instead | `--expected` (one or more) | one reproduction each, `E1`, `E2`, … |
| **Unchanged** — what must keep working | `--unchanged` (zero or more) | one test each, `U1`, …, in the pass-to-pass set |

An empty symptom or statement is refused (`BUG-EMPTY`), as is a bug without `--expected` (`BUG-NO-EXPECTED`) — before anything is written. Text from `--issue` is context for the reproduction author only; it is never written into the triad. A symptom that reads `confirm` or `run` follows `--`: `speccoder bug -- "run"`.

## Where the bug lives

The format decides (`bug_unit`), and the choice and its reason are shown and recorded:

| Project | Home | What speccoder writes |
|---|---|---|
| Spec Kit with the first-party `bug` extension installed and enabled | `.specify/bugs/<slug>/` | nothing: the extension's own assess skill writes `assessment.md`, given your triad verbatim; speccoder reads it back |
| Spec Kit without it (or with it disabled) | the standing unit `NNN-bugfixes` | a user story whose acceptance scenarios are the triad, an approved requirement and a task — appended, then committed alone as `bug: <title>` |
| OpenSpec | the standing change `openspec/changes/bugfixes/` | a requirement of the `bugfixes` capability, a line of the proposal, a task group |
| Kiro (`bugfix.md`) | refused (`BUG-NO-UNIT`) until speccoder has a Kiro adapter (F-052) | nothing |

Whether the extension is installed comes from Spec Kit's own inventory: `specify extension list --json` when the CLI is installed, else `.specify/extensions/.registry` and `.specify/extensions.yml`. In the extension's layout, `assessment.md` must carry each of your statements verbatim (whitespace aside) as `Current:` / `Expected:` / `Unchanged:` lines — else the bug stops with `BUG-ASSESSMENT-DRIFT` before reproduce; another layout stops with `BUG-ASSESSMENT-UNREADABLE`. The skill may write only its bug's directory. A bug you assessed yourself (`/speckit.bug.assess`) continues with `speccoder bug --slug <slug>`.

## The stages

**record → reproduce → confirm → unchanged → fix → ship**

1. **Reproduce.** A test-blind author (it sees the triad and the code's file list, never a fix) writes exactly one test per *Expected* statement — and one per *Unchanged* statement — in new test files. Each reproduction must fail on the untouched code **by an assertion, twice**. A build or collection error, a timeout, nothing collected, a flaky failure or an *absent* symbol is not a bug's red: the try counts as `not_demonstrated`. Each *Unchanged* test must pass. Up to `[bug] reproduce_tries` tries (3; at most 5). A held-out variant per statement, with other inputs, is written and sealed beside them; you never see it, and neither does the fix.
2. **Confirm.** You see each reproduction — statement, file, test, how it failed — and confirm them **once**, bound to the digest you were shown: at the terminal prompt, `speccoder bug confirm <unit>/<task> --digest D`, the [Bug sheet](@/docs/web.md#bug-sheet) or Needs You. A rejection with a reason starts a new try (counted); a confirmation is never asked twice for the same digest, across crashes. Rewording the triad lapses it. Without a terminal (`--json`, `--events`, CI) the bug stops here with `next: speccoder bug confirm …`: no flag skips this.
3. **Fix.** An ordinary task attempt — worktree, approvals, integrity audit, checks, retries — with the confirmed reproductions written at its checkpoint as its frozen acceptance set (the same reuse that frozen review oracles use, with the bug red rule).
4. **Unchanged.** After the attempt's diff is known, the pass-to-pass set is chosen deterministically: baseline-green tests in test files that reference a changed source file (Rust `use crate::…`, Python imports, TS/JS relative imports, Go packages), plus each *Unchanged* test. Each must pass by name; a missing one is a failure; an empty selection is recorded as `none_selected`, never as passed.
5. **Judge.** `verified` also needs every reproduction red → green, every held-out variant green, the pass-to-pass set green, and the **revert** — the final tree with the attempt's source changes undone, its tests kept — rejected by at least one reproduction on an assertion (else `BUG-NOT-DISCRIMINATING`). A **special case** per statement — a fix that only recognises the reproduction's literal input — must be rejected by the reproductions and held-out variants together; one that survives gets one regenerated variant, then stops for you (`BUG-REPRO-WEAK`, `speccoder task rule … --tests-right`).
6. **Ship.** The verdict commit carries `Bug:`, `Acceptance: bug_confirmation …` (or `person_test`) and, with `--issue N` and a `verified` or `attested` outcome only, `Fixes: #N`; the draft pull request [`speccoder pr`](@/docs/github.md) opens then says `Fixes #N` in its body, so merging it closes the issue. In the extension's layout nothing is ticked: the commit's `Spec-Task: bugs/<slug>/bug:<slug>` is a *trailer-only task*, anchored on the triad's digest, which [`speccoder attest verify --branch`](@/docs/verifier.md) checks like a tick — `stale` when the triad at the head differs.

## Outcomes

| Outcome | Means | Needs You |
|---|---|---|
| `recorded` | the triad is recorded (and, once confirmed, the fix runs next) | — |
| `awaiting_confirmation` | every statement reproduced; your confirmation | *Blocked on you* (`bug_confirm`) |
| `cannot_reproduce` | the tries ran out; the engine's hand-off lists each try's class and excerpt and which statements were and were not reproduced | *Blocked on you* (`bug_cannot_reproduce`): bring a `--test`, reword, try again, or skip |
| `already_satisfied` | every reproduction passed on every try: nothing to fix, nothing ticked | *Ready to review* (`bug_already_satisfied`): keep it as a regression test (`task accept`, `attested`) or drop it |
| `unchanged_not_holding` | an *Unchanged* statement does not hold today | *Blocked on you* |
| `fixed` | the fix was judged (see its verdict) | — |
| `failed` | the fix did not pass | — |

## Your own test, or someone else's fix

- `--test PATH::NAME` (one per `--expected`, in order) uses your failing test as the reproduction: it must exist, be collected and fail by an assertion before anything is written (`BUG-TEST-NOT-FOUND`, `BUG-TEST-NOT-RED`, `BUG-TEST-PASSES`, `BUG-TEST-COVERAGE`). Naming it is the confirmation. Held-out variants are still written. `[bug] test` sets how one named test runs when the toolchain's rule does not fit.
- `--candidate fix.patch` or `--candidate pr:N` judges someone else's fix as the attempt — no implementer, no retries. A diff that touches a frozen reproduction, a held-out path or a judge input is refused before it is applied (`BUG-CANDIDATE-JUDGE`); one that does not apply is refused (`BUG-CANDIDATE-APPLY`). A pull request's diff is fetched read-only, its head commit recorded.

## Commands

```sh
speccoder bug "<symptom>" --expected "<e>" [--unchanged "<u>"]… [--issue N] [--test PATH::NAME]… [--candidate PATCH|pr:N] [--no-run]
speccoder bug --slug <slug>                         # a bug of Spec Kit's bug extension
speccoder bug confirm <unit>/<task> --digest D      # or --reject --reason "…"
speccoder bug run --task <unit>/<task>              # reproduce again, or run the confirmed fix
speccoder workflow install --overlay bugfix         # Spec Kit's bugfix workflow runs speccoder for its test step
```

With the `bug` extension and its `bugfix` workflow, `speccoder workflow install --overlay bugfix` adds an overlay that replaces the workflow's prompt-driven `test` step with `speccoder bug run --slug …`; `specify workflow run bugfix` then pauses at the confirmation and reports the verdict. `speccoder workflow uninstall --overlay bugfix` restores the extension's own step.

`--json` prints the `bug` payload ([schema](https://hupe1980.github.io/speccoder/schemas/v2/bug.json)); `--events` adds `bug.recorded`, `bug.reproduce_try`, `bug.reproduced`, `bug.confirm_waiting`, `bug.confirmed`, `bug.confirm_rejected`, `bug.unchanged_selected`, `bug.discrimination` and `bug.outcome`.
