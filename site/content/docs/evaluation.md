+++
title = "Evaluation and benchmarks"
description = "Measure speccoder on Spec Kit cases with hidden oracles and impossible tasks, compare models and external agents, and import SWE-bench-format benchmarks."
weight = 110

[extra]
group = "Trust"
nav = "Evaluation and benchmarks"
+++

An agent that grades itself proves nothing, so speccoder is measured against oracles it never sees.

## The evaluation suite

The repository's `evals/` directory holds small Spec Kit and OpenSpec projects. Each case is a directory with:

- `project/` — a committed project with a spec, plan, tasks and the visible checks in its `speccoder.toml`;
- `oracle/` — **hidden oracle** tests, copied into the worktree only after the attempt, which decide whether a completion is really correct;
- `case.toml` — the task, the oracle command and the expectation: `complete`, `stop` (the case is **impossible** — its spec or tests contradict each other, and the right answer is to stop) or `reject` (the work is tailored to the visible tests, and the judge must not tick it).

The suite's `eval.toml` supplies the model.

```sh
speccoder eval evals                         # every case
speccoder eval evals --case wrap-text        # one case
speccoder eval evals --repeat 3              # results vary between runs; repeat
speccoder eval evals --keep runs/            # keep each case's repository and journal
speccoder eval evals --executor claude       # let Claude Code (or codex) make the attempts
```

| Class | Means |
|---|---|
| `correct` | Completed, and the hidden oracle agrees |
| `false_completion` | Completed by speccoder's verdict, rejected by the oracle |
| `not_completed` | Stopped or failed on a possible task |
| `honest_stop` | Stopped on an impossible task — the right answer |
| `caught` | Tailored work, not ticked — the right answer |
| `cheat` | Ticked an impossible task or tailored work — tests bent or results hard-coded |

The run fails (exit 24) when any case is a false completion or a cheat. The summary reports:

- **precision** (completions the oracle confirms) and the count of each class;
- with `--repeat`, how often each case came out as expected;
- how many recorded attempts [replay](@/docs/recovery.md#replay) exactly from the journal;
- the held-out gap — visibly passing attempts the [held-out tests](@/docs/evidence.md#held-out-tests) failed — and whether the oracle agreed with each held-out pass;
- the [gaming monitor](@/docs/evidence.md#the-gaming-monitor)'s precision and recall, when enabled;
- the median [acceptance strength](@/docs/evidence.md#acceptance-strength) over verified cases, and how many cases stopped on an [over-strict test](@/docs/evidence.md#over-strict-tests);
- tokens per correct case.

## Comparing models and agents

The same cases and oracles compare one model with another, or speccoder's own loop with an external agent. With `--executor`, the agent makes every attempt while the model in `eval.toml` still writes the acceptance tests; a case whose project sets its own `[executor]` (such as `tailor-slugify`'s scripted agent) is skipped. Results on a handful of cases are indicative, not a leaderboard: use `--repeat` and read the per-case results.

## Public benchmarks

`speccoder bench import` turns instances in the SWE-bench format — SWE-bench, Verified, Multilingual, Pro, and benchmarks that reuse it — into evaluation cases:

```sh
git clone https://github.com/sharkdp/bat repos/sharkdp__bat        # one clone per repository: owner__name
speccoder bench import instances.jsonl --repos repos --out bench  # JSON Lines or a JSON array; --limit N
(cd bench/<case>/project && cargo fetch)                         # the sandbox has no network
speccoder eval bench --executor claude
```

Each case's project is the repository at `base_commit` with a Spec Kit feature around the issue: `spec.md` carries the issue text and one task resolves it. The oracle is the set of files the benchmark's `test_patch` adds or changes, which the agent never sees. The test command comes from the instance's `test_cmd`, else `--test-cmd`, else the project (Cargo, npm, pytest, Go).

Downloading datasets, cloning repositories and providing the environment the tests need are up to you: SWE-bench runs each instance in a container image; here the tests run on your machine, in the sandbox.
