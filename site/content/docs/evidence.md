+++
title = "Reading the evidence"
description = "What each task state means, where the evidence lives in git, how the diff audit, acceptance strength, mutation testing and the gaming monitor work, and how to inspect any attempt."
weight = 60

[extra]
group = "Use"
+++

## The report

```sh
speccoder report
```

```text
Phase 2: User Story 1 - Order versions by precedence (Priority: P1)
  ✓ T002 Implement `compare(…)`                    verified
Phase 3: Convergence
  ✓ T003 Validate the build metadata segment …     verified
  · T004 Parse the pre-release segment …           ready
      last attempt: not_demonstrated — its acceptance tests already pass …
3/4 done: 2 verified · 1 ready · 3 commits on speccoder/001-semver not merged
```

| Mark | State | Means |
|---|---|---|
| `✓` | `verified` | Spec-derived acceptance tests went red → green; nothing regressed |
| `~` | `regression_only` | Checks pass; nothing task-specific was shown |
| `A` | `attested` | Ticked by `speccoder task accept` — a person's word, not evidence |
| `!` | `ticked_without_evidence` | Ticked, but not by speccoder |
| `–` | `skipped` | Passed by on purpose; tasks that depend on it stay blocked |
| `·` | `ready` | Can run now |
| `⧗` | `blocked` | Waits for the tasks listed |

`report` reads only git, so it works on any machine with the repository. Below the tasks it traces the spec's requirements (`FR-001`, `SC-002`) to the tasks that cite them — `Trace 9/11 requirement(s) cited by a task; not cited: FR-007, SC-003` — and it counts ticks whose commit carries no [attestation](@/docs/attestations.md).

## What counts as red

Acceptance tests are evidence only if they failed before the task's work existed. speccoder reads the test runner's own report (JUnit, or the summary line of libtest, nextest, pytest, jest, vitest and `go test`) and records the kind of red:

| Red | Means |
|---|---|
| `assertion` | A collected test failed on an assertion |
| `absent` | The tests could not build or import only because what the task adds does not exist yet (rustc `E0425`, Python `ImportError`, TypeScript `TS2305`, Go `undefined:`, …) — the usual start of a task that adds a function |

If the call that writes the acceptance tests fails — the provider refuses it (no credits, a rejected key) or the run breaks — nothing is attempted: the task stops for you instead of being judged on the regression checks alone, which would look like a spec without examples.

Anything else is **not demonstrated**: another build or syntax error, a collection error, a timeout, no tests collected, or output speccoder does not recognise. The task can then be at most `regression_only`, and the report quotes the first lines of the error. Red is confirmed by a second run; red once and not again is `flaky`, not red.

A check that fails and then passes when run again is marked `flaky`. It does not fail the task, but a flaky task-specific check is never evidence for `verified`.

## Held-out tests

Frozen visible tests stop an implementer from editing the tests, but not from tailoring to them: code that returns exactly the examples the tests check passes them. So the judge keeps part of the oracle. After the visible acceptance tests are written and red, a second author call writes **held-out** tests for the same scenarios with other inputs and the edge cases the spec states. They never enter the worktree or any prompt the implementer gets; speccoder keeps them in its state directory and checks they are red too.

After an attempt passes the visible checks, speccoder writes the held-out tests over the final tree, runs them in the sandbox, and puts the tree back exactly as it was. `verified` needs both. If a held-out test fails, the verdict fails, and the retry is told only which scenario — `held-out acceptance failed: US1-2 ("Given …")`, the spec's own words — never a path, test name, input or output. The same scenarios failing twice stop the retries.

What can happen to a held-out test, and what the report says:

| Outcome | Means |
|---|---|
| `passed` | every held-out test passed; the commit carries `Held-Out: passed=2/2 manifest=sha256:…` |
| `failed` | some failed: the attempt failed (`gap on attempt 1: US1-2`) and is retried |
| `absent` | none was kept — not generated, no visible tests to vary (`no_visible_tests`), or every file discarded (`not_red`, `unknown_scenario`, `path`, `command`, `limit`, `collision`); the commit says `Held-Out: absent=<reason>` and the verdict rests on the visible tests |
| `tampered`, `leak_suspected` | the stored set changed, or the attempt contains a literal only the held-out tests hold: stopped for a person, saved, not ticked |

The last task of a story with two or more scenarios also gets a composition test that exercises them together. When a feature run ends with every task ticked, its held-out sets are published under `refs/speccoder/held-out/<feature>` — each manifest hashes to its task's `Held-Out:` trailer — so anyone can check what judged each task. speccoder never pushes that ref itself.

**External agents** (`[executor]`) run with your permissions, so the held-out set is *withheld, not sealed*: Claude Code is told not to read its directory, and a held-out literal copied into the attempt stops the tick. `[acceptance] held_out = false` switches held-out tests off; the commit then says `Held-Out: off`.

## Where the evidence lives

Each passed task is one commit on `speccoder/<feature>`, with trailers:

```text
speccoder: T002 Implement `compare(…)`

Spec-Task: 001-semver/T002
Verdict: verified
Red: assertion
Run: run_01M5…
Usage: input=40250 output=3383 cache_read=30976 cache_write=0
Acceptance-Strength: 0.83 mutants=9/11 near-miss=1/2
Mutation: caught=26 missed=13 timeout=0 unviable=6
```

Other trailers appear when they apply: `Held-Out:` (the [held-out tests](#held-out-tests)), `Acceptance-Strength-Visible:` (the visible tests alone, with held-out tests on), `Ruling:` (a person ruled on an [over-strict test](#over-strict-tests)), `Flaky:` (checks that passed only when run again), `Scope-Outside:` (changed files the task line did not name — reported, never a failure), `Checklists: overridden` (the run was started with `--ignore-checklists`).

Each tick commit also carries signed in-toto statements of the same evidence in a git note, which anyone can re-check with `speccoder attest verify` — see [attestations](@/docs/attestations.md).

After a run ticks the feature's last task, the `full` checks run once; the result is recorded under `refs/speccoder/full/<feature>`, and `speccoder pr` refuses a branch whose full checks failed unless `--force`.

Every attempt that did not pass is a commit too, under its own ref, then undone in the worktree:

```sh
git for-each-ref refs/speccoder/attempts/      # every saved attempt
git log -1 --format=%B <ref>                   # its outcome, reason, run and usage
git diff speccoder/<feature> <ref>             # what it changed
```

`speccoder replay <run>` re-derives any recorded run from the journal without a model — see [recovery and replay](@/docs/recovery.md#replay).

## The diff audit

Regardless of what the checks say, a verdict fails when the attempt:

- deletes or rewrites a test file that existed before it (adding tests to it can be allowed — see [approvals](@/docs/approvals.md#frozen-tests));
- removes more test functions than it adds;
- adds skip markers (`#[ignore]`, `@pytest.mark.skip`, `.skip(`, `xit(`, …);
- changes anything that decides verification: `speccoder.toml`, CI configuration, test-runner configuration;
- changes agent configuration (`.claude/`, `.agents/`, `.codex/`, `.gemini/`, `.opencode/`, `.cursor/`, `.kiro/`, `.mcp.json`, …) — hooks and settings there run code in a later agent session;
- changes an agent context file (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`) without a person's approval.

The audit reads the diff, so it also catches what a command or an MCP server wrote. Files git ignores are checked too (an agent loads `.claude/settings.local.json`), and after every command speccoder runs, these inputs are re-checked: a command that changes one ends the attempt at once.

## Acceptance strength

A test that went red → green shows the task's work made it pass. It does not show the test would have failed a *wrong* implementation — studies of model-written tests find many that do not ([SWE-Mutation](https://preview.aclanthology.org/ingest-acl/2026.findings-acl.1976/), [*Beyond Fail-to-Pass*](https://arxiv.org/abs/2607.19843)). So after every `verified` verdict speccoder tries wrong implementations against the acceptance tests and counts how many they reject:

- **near misses** — two (up to three) variants of the changed code, written by a separate model call that sees the spec and the code but never a test: plausible, and wrong about exactly one scenario;
- **mutants** — when `[mutation]` is configured, the tool's mutants of the changed code, judged by the acceptance command alone.

Each variant is written into the worktree, the acceptance tests run, and the tree is put back; afterwards speccoder checks the tree and the judge's inputs are exactly as they were. A variant the tests fail on an assertion (or by timing out) is **rejected**; one that does not build is **unviable** and not counted; one that passes **survived**. Strength is rejected / judged:

```text
Strength   0.67 mutants=9/11 near-miss=1/2
  survived: near-miss-2 (US1-2: an empty name is accepted)
```

It is recorded on the tick (`Acceptance-Strength:`), in the report and the attestation, and in `--json` (`strength`). With [held-out tests](#held-out-tests) there are two numbers: the visible tests alone, and visible and held-out together — the set the verdict rests on. Survivors' patches are kept in the state directory. Nothing of it reaches the implementing model.

Strength never changes a verdict unless you ask: with `[acceptance] min_strength = 0.8`, a set below it is strengthened **once** — the test author is told what got through and adds tests, which must fail without the change and pass with it — and measured again. Still below, or not measurable (no variant, a tool error, the budget ran out), the task stops for you: `stopped[ACCEPTANCE-WEAK]`. No evidence is not good evidence.

How this differs from [mutation testing](#mutation-testing): mutation testing asks whether *all* your tests check the changed code; acceptance strength asks whether the *acceptance tests* — what `verified` rests on — tell right from wrong.

### Over-strict tests

The variant author also writes one alternative the spec **permits** — another order where the spec fixes none, other wording, another tie-break. If the acceptance tests reject it on an assertion, they may encode a choice the spec did not make, and only you can decide. The task stops — `stopped[TESTS-OVER-STRICT]` — with the scenario, the alternative's patch and the failing test, and its attempt is **held**: its tree stays, the feature's other tasks wait.

```sh
speccoder task rule T004 --tests-right --reason "the plan fixes the order"   # the tests are right
speccoder run --task T004                     # judges the held attempt again — no model call — and ticks it
speccoder run --task T004 --fresh-acceptance  # the tests are too strict: write them anew, attempt again
speccoder task accept T004 --reason "…"       # tick it on your word (attested)
```

A ruling is bound to that exact alternative (its hash) and recorded on the tick (`Ruling:`) and in the attestation, with who made it. The implementing model is never told.

## Mutation testing

Green tests prove that the code passes them, not that they test it. With `[mutation] tool = "cargo-mutants"`, speccoder mutates **only the code the passing attempt changed** — flips a comparison, returns a default — and runs the tests against each mutant. A mutant the tests do not catch is behaviour nobody checks.

The report shows caught and surviving mutants and the commit carries a `Mutation:` trailer. With `mode = "require"`, survivors fail the verdict and the retry is told which survived; this is strict, because some mutants are equivalent to the original. Other languages work through `tool = "command"` with any tool that writes the standard mutation-testing JSON report (Stryker, Infection, PIT). See [configuration](@/docs/configuration.md#mutation).

## The gaming monitor

With `[monitor] enabled = true`, a separate model call reviews every passing attempt. It sees the task, the diff and the tool calls — never the implementing model's explanation — and looks for hard-coded results, special-cased inputs, weakened tests and similar shortcuts. A suspicion keeps the task unticked (`suspected_gaming`) until you decide. It can only add doubt: it never makes a failing task pass, and the implementing model never learns what it found.

## What the evidence does not show

The checks and tests you configured, and the examples in your spec, bound what `verified` can mean. An `absent` red shows the tests needed the task's work to build; how sharply they then test it is what [acceptance strength](#acceptance-strength) and [mutation testing](#mutation-testing) measure — a strength of 1.0 means the tests rejected every wrong variant that was tried, not every possible one, and a surviving near miss may be one the author got wrong. A spec without acceptance examples yields `regression_only`; an example that is wrong yields a wrong test. When speccoder cannot demonstrate something, it says so — `not_demonstrated`, `blocked` — rather than guessing.
