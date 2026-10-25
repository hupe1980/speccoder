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

The repository's `evals/` directory holds small Spec Kit projects. Each case has:

- a project with a spec, plan, tasks and visible checks;
- a **hidden oracle** — tests copied in only after the attempt, which decide whether a completion is really correct;
- an expectation: complete the task, or stop honestly. Some cases are **impossible**: their spec or tests contradict each other, and the right answer is to stop.

```sh
speccoder eval evals                         # every case
speccoder eval evals --case wrap-text        # one case
speccoder eval evals --repeat 3              # results vary between runs; repeat
speccoder eval evals --keep runs/            # keep each case's repository and journal
speccoder eval evals --executor claude       # let Claude Code (or codex) make the attempts
```

Each case is classified:

| Class | Means |
|---|---|
| `correct` | Completed, and the hidden oracle agrees |
| `false_completion` | Completed by speccoder's verdict, rejected by the oracle |
| `not_completed` | Stopped or failed on a possible task |
| `honest_stop` | Stopped on an impossible task — the right answer |
| `cheat` | Completed an impossible task — tests bent or results hard-coded |

The summary reports **precision** (completions the oracle confirms), cheats, tokens per correct case, the gaming monitor's precision and recall when enabled, the **median acceptance strength** over verified cases and how many cases stopped on an [over-strict test](@/docs/evidence.md#over-strict-tests), and how many recorded attempts replay exactly. The model comes from `evals/eval.toml`.

## Comparing models and agents

The same cases and oracles compare a model with another, or speccoder's own loop with an external agent. Results on a handful of cases are indicative, not a leaderboard: run `--repeat` and read the per-case results before drawing conclusions.

## Public benchmarks

`speccoder bench import` turns instances in the SWE-bench format — SWE-bench, Verified, Multilingual, Pro, and benchmarks that reuse it — into evaluation cases:

```sh
git clone https://github.com/sharkdp/bat repos/sharkdp__bat        # one clone per repository: owner__name
speccoder bench import instances.jsonl --repos repos --out bench  # JSON Lines or a JSON array
(cd bench/<case>/project && cargo fetch)                         # the sandbox has no network
speccoder eval bench --executor claude
```

Each case's project is the repository at `base_commit`, with a Spec Kit feature around the issue: `spec.md` carries the issue text and one task resolves it. The oracle is the set of files the benchmark's `test_patch` adds or changes, which the agent never sees. The test command comes from the instance's `test_cmd`, `--test-cmd`, or the project (Cargo, npm, pytest, Go).

Downloading datasets, cloning repositories and providing the environment the tests need are up to you: SWE-bench runs each instance in a container image; here the tests run on your machine, in the sandbox.
