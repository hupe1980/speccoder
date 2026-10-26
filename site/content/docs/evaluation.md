+++
title = "Evaluation and benchmarks"
description = "Measure speccoder on Spec Kit cases with hidden oracles, impossible tasks and bugs, compare models and external agents, and import SWE-bench-format benchmarks."
weight = 110

[extra]
group = "Trust"
nav = "Evaluation and benchmarks"
+++

## The evaluation suite

speccoder is measured against oracles it never sees. A suite is a directory of cases plus an `eval.toml` whose `[model]` and `[budget]` every case runs with. The repository's `evals/` suite holds small Spec Kit and OpenSpec projects; `evals/bugs/` is a suite of bug cases. Each case is a directory with:

- `project/`: a committed project with a spec, plan, tasks and the visible checks in its `speccoder.toml`;
- `oracle/`: **hidden oracle** tests, copied into the worktree only after the attempt, which decide whether a completion is correct;
- `case.toml`: the task, the oracle command (`[oracle] argv`) and the expectation (`expect`).

| `expect` | The right answer |
|---|---|
| `complete` | Complete the task so the oracle passes |
| `stop` | Stop: the case is **impossible** (its spec or tests contradict each other) |
| `reject` | Do not tick: the work is tailored to the visible tests |
| `already_satisfied`, `cannot_reproduce` | Bug cases only: end with that bug outcome, never ticked |

A **bug case** adds a `[bug]` table (`symptom`, `expected`, `unchanged`) and runs through [`speccoder bug`](@/docs/bugs.md); the harness confirms the reproductions it is shown. A fixed bug's oracle runs on the verdict commit's tree.

```sh
speccoder eval evals                         # every case (default directory: evals)
speccoder eval evals/bugs                    # the bug cases
speccoder eval evals --case wrap-text        # one case
speccoder eval evals --repeat 3              # repeat each case (1–100)
speccoder eval evals --keep runs/            # keep each case's repository and journal
speccoder eval evals --executor claude       # let Claude Code (or codex) make the attempts
```

| Class | Means |
|---|---|
| `correct` | Completed, and the hidden oracle agrees |
| `false_completion` | Completed by speccoder's verdict, rejected by the oracle |
| `not_completed` | Stopped or failed on a possible task |
| `honest_stop` | Stopped on an impossible task, or a bug case ended with the expected `already_satisfied` or `cannot_reproduce` |
| `caught` | Tailored work, not ticked |
| `cheat` | Ticked where it must not: an impossible task, tailored work, or a bug that was not to be fixed |
| `skipped` | The case sets its own `[executor]` and the suite runs with `--executor` |
| `invalid_case`, `error` | The case could not be judged |

The run fails (exit 24, `EVAL-FAILED`) when any case is a false completion or a cheat. The summary reports:

- **precision** (completions the oracle confirms) and the count of each class;
- with `--repeat`, how often each case came out as expected;
- how many recorded attempts [replay](@/docs/recovery.md#replay) exactly from the journal (speccoder's own attempts; not those of an external executor);
- the held-out gap (visibly passing attempts the [held-out tests](@/docs/evidence.md#held-out-tests) failed) and whether the oracle agreed with each held-out pass;
- the [gaming monitor](@/docs/evidence.md#the-gaming-monitor)'s precision and recall, when enabled;
- the median [acceptance strength](@/docs/evidence.md#acceptance-strength) over verified cases, and how many cases stopped on an [over-strict test](@/docs/evidence.md#over-strict-tests);
- tokens per correct case.

If the model provider refuses a request, the suite stops there and says which cases were not run.

## Comparing models and agents

The same cases compare one model with another, or speccoder's own loop with an external agent. With `--executor`, the agent makes every attempt while the model in `eval.toml` still writes the acceptance tests; it is refused when `eval.toml` already has an `[executor]`. Results on a handful of cases are indicative: use `--repeat` and read the per-case results.

## Public benchmarks

`speccoder bench import` turns instances in the SWE-bench format (SWE-bench, Verified, Multilingual, Pro, and benchmarks that reuse it) into evaluation cases:

```sh
git clone https://github.com/sharkdp/bat repos/sharkdp__bat        # one clone per repository: owner__name
speccoder bench import instances.jsonl --repos repos --out bench  # JSON Lines or a JSON array; --limit N
(cd bench/<case>/project && cargo fetch)                         # the sandbox has no network
speccoder eval bench --executor claude
```

Each case's project is the repository at `base_commit` with a Spec Kit feature around the issue: `spec.md` carries the issue text and one task resolves it, `expect = "complete"`. The oracle is the set of files the benchmark's `test_patch` adds or changes, which the agent never sees. The test command comes from the instance's `test_cmd`, else `--test-cmd`, else the project (Cargo, npm, pytest, Go).

Downloading datasets, cloning repositories and providing the environment the tests need are up to you: the tests run on your machine, in the sandbox, not in SWE-bench's container images.
