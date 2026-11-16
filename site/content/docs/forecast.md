+++
title = "Merge forecast"
description = "Will your branch still work when it meets the others? speccoder pairs it with every other open branch and runs your oracle on each merge, before anyone merges."
weight = 79

[extra]
group = "Use"
+++

Two branches can each pass their tests and still break each other once merged. ([Claims](@/docs/claims.md) keep two people off one story; the forecast catches two stories that meet in the code.) `speccoder forecast` finds that before anyone merges. It pairs the feature's branch with every other open branch, whether yours, a teammate's or another agent's. For each pair it makes the merge without touching your checkout, then runs your oracle on the merged code.

```console
$ speccoder forecast
Merge forecast for speccoder/002-export (4 branches, fetched)
  your oracle: 3 of 3 scenarios pass on your branch
  ✗ speccoder/003-import     Agent · 10m ago  conflicts in src/csv.rs
  ✗ origin/dana/csv-quoting  Dana · 2h ago    breaks US1-2 (a quoted field → stays quoted)
      tests/export_us1.rs fails on the merge, passes on its own branch
  · origin/fix/readme        Sam · 1d ago     not judged — docs only
  ✓ origin/fix/typo          Sam · 3h ago     no conflict observed (3 scenarios held)
  4 test runs, 0 reused
2 of 4 branches conflict with yours
→ `speccoder/003-import` edits src/csv.rs too: merge it now while it is small (`git merge speccoder/003-import`), or talk to Agent
→ `origin/dana/csv-quoting` breaks US1-2 (a quoted field → stays quoted): see it with `git merge --no-commit --no-ff origin/dana/csv-quoting` then `cargo test --test export_us1`, and talk to Dana before either merges
```

It exits 23 (`FORECAST-CONFLICT`) while a branch conflicts with yours, so it works as a CI step.

## What each line means

| Verdict | When |
|---|---|
| **conflicts** | The merge cannot be made: both branches change the same lines, or the same file in ways git cannot combine (an edit and a delete, two new files with one name). The files are named, and no test runs |
| **breaks** | The merge is made, and a scenario whose test passes on its own branch fails on the merge, again on a second run. Or the merge changes one of your frozen oracle tests, which would rewrite what judges your work. |
| **no conflict observed** | The merge is made, and every scenario that passed on your branch still passes. This is evidence about these tests, not a promise |
| **not judged** | The merge is made, but the oracle did not judge it. The reason is given: only docs changed, no frozen oracle here, none of your scenarios passes on your branch yet, a test timed out or ran nothing, no sandbox, or the merge does not set up here (a dependency that is not on this machine looks the same as a lockfile the merge broke, so it is shown, not called a break) |

A test that fails on the merge and passes when run again is named *flaky* and not counted.

## Which branches

The other branches are your other local `speccoder/*` feature branches and every remote-tracking branch with work beyond the base that yours does not have yet. The base is the remote's default branch (`origin/HEAD`), else your local `main` or `master`. A branch and its remote copy at the same commit are checked once. The 20 most recently updated are checked, and the rest are counted. Name branches to check exactly those: `speccoder forecast origin/dana/csv-quoting`.

The forecast fetches first and never asks for a password. When the fetch fails, it says so and uses the branches as last fetched. `--no-fetch` skips it.

## Whose oracle

Your feature's [frozen oracle](@/docs/evidence.md) judges every merge, even with a branch that has no tests of its own. When another feature's oracle is on this machine (two agents running two features here), it runs on the merge too: *yours breaks 003-import's US2-1*. An oracle's commands are kept on the machine that froze it, so a teammate's oracle on another machine does not run here.

Only scenarios that pass on their own branch can break, because a frozen test was red before its code was written. So two runs per pair answer the question: on the branch, then on the merge.

## Cost

Tests run one at a time, in a fresh worktree of the merge, inside the [OS sandbox](@/docs/security.md): the merge holds other people's code. None of your untracked files (an `.env`) is copied in, and its dependencies install from your caches read-only. Without a sandbox no test runs, and each pair says *not judged — no sandbox*. Each test's result is kept by the tree it ran on and your `speccoder.toml`; a test that timed out is never kept. A second forecast with nothing new runs no test, and a new commit on one branch re-checks only that merge. Only committed work is part of the forecast.

## In the web app

The feature's **Pull Request** card shows the last forecast: how many branches and when. **Check Open Branches** runs a new one. Problems come first, each with its author, the scenario and its contract or the conflicting files, **Open in Editor**, and how to see it. The other branches fold into one line. On the **Overview**, a feature card says *Conflicts with 2 branches* from its last forecast.
