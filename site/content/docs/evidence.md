+++
title = "Reading the evidence"
description = "What each task state means, where the evidence lives in git, how the diff audit, mutation testing and the gaming monitor work, and how to inspect any attempt."
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
Mutation: caught=26 missed=13 timeout=0 unviable=6
```

Other trailers appear when they apply: `Held-Out:` (the [held-out tests](#held-out-tests)), `Flaky:` (checks that passed only when run again), `Scope-Outside:` (changed files the task line did not name — reported, never a failure), `Checklists: overridden` (the run was started with `--ignore-checklists`).

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

## Mutation testing

Green tests prove that the code passes them, not that they test it. With `[mutation] tool = "cargo-mutants"`, speccoder mutates **only the code the passing attempt changed** — flips a comparison, returns a default — and runs the tests against each mutant. A mutant the tests do not catch is behaviour nobody checks.

The report shows caught and surviving mutants and the commit carries a `Mutation:` trailer. With `mode = "require"`, survivors fail the verdict and the retry is told which survived; this is strict, because some mutants are equivalent to the original. Other languages work through `tool = "command"` with any tool that writes the standard mutation-testing JSON report (Stryker, Infection, PIT). See [configuration](@/docs/configuration.md#mutation).

## The gaming monitor

With `[monitor] enabled = true`, a separate model call reviews every passing attempt. It sees the task, the diff and the tool calls — never the implementing model's explanation — and looks for hard-coded results, special-cased inputs, weakened tests and similar shortcuts. A suspicion keeps the task unticked (`suspected_gaming`) until you decide. It can only add doubt: it never makes a failing task pass, and the implementing model never learns what it found.

## What the evidence does not show

The checks and tests you configured, and the examples in your spec, bound what `verified` can mean. An `absent` red shows the tests needed the task's work to build; how sharply they then test it is what [mutation testing](#mutation-testing) measures. A spec without acceptance examples yields `regression_only`; an example that is wrong yields a wrong test. When speccoder cannot demonstrate something, it says so — `not_demonstrated`, `blocked` — rather than guessing.
