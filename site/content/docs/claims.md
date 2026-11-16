+++
title = "Story claims"
description = "One person or agent per story: a claim is a ref on your shared remote, taken by whoever starts the story first. No server, no account."
weight = 80

[extra]
group = "Use"
+++

When two people, or two agents, start the same story, a day of work is done twice and the merge hurts. speccoder gives each story one holder. Whoever starts it first claims it, and everyone else's speccoder sees the claim before starting.

```console
$ speccoder run
  US2 claimed on origin
  …
```

Sam's run, a minute later on another machine:

```console
$ speccoder run
stopped[CLAIMED]: another person holds this story
  fix: talk to Dana <dana@example.com>
  fix: speccoder claim US2 --force --feature 004-export
T009: US2 is claimed by Dana (since 1m ago, until in 24h)
```

The run stops before the task: no model call, nothing spent.

## How a claim works

A claim is a git ref on your shared remote, `refs/speccoder/claims/<feature>/<story>`, pointing to an empty commit that names the holder, since when, until when and the branch. It is created by a push that succeeds only while the ref does not exist yet, so when two people claim at the same moment, exactly one wins and the other is told who. No speccoder server or account is involved, only your remote (GitHub, GitLab or plain git), and the first claim adds a fetch line so that every `git fetch` brings claims along.

Claims live on `origin` (or your only remote). In a fork workflow, where `origin` is your fork, they coordinate only the people who push there. Without a remote, claims live in the repository itself and every worktree sees them. A claim is a person's, by git `user.email`: two people's agents never take one story, but two runs of your own share your claims.

A claim is advisory. git does not enforce it; speccoder's run and `pr` respect it, and a person can always take it over. It never locks files: when two stories touch the same code, the [merge forecast](@/docs/forecast.md) shows it.

## How a claim ends

- **It runs out.** A claim lasts 24 hours. A run renews it each hour it works on the story, so a working run never loses it. After a night off it is *stale*: anyone's run or `claim` takes it, and says whose it was.
- **It is done.** When a run ticks the story's last task, the claim says *done on speccoder/004-export*. A done claim holds while the work is unmerged, because the story exists on a branch and nobody should start it again. Once that commit is in the base branch, the next look at the claims removes it. A squash or rebase merge leaves no trace of that commit in the base, so a done claim also runs out after two weeks.
- **It is released.** `speccoder claim US2 --release`. Another person's live or done claim needs `--release --force`; a stale one goes with `--release`.

## The command

```console
$ speccoder claim
Claims on 004-export (on origin)
  ◐ US1  you   since 2h ago · until in 22h
  △ US2  Dana  since 1m ago · until in 24h
  ✓ US3  Lee   done on speccoder/004-export, held until it is merged
you have US1 · Dana has US2 · Lee has US3
```

| | |
|---|---|
| `speccoder claim` | the feature's claims, read from the remote |
| `speccoder claim US2` | claim it (or renew your own); refused with `CLAIMED` (exit 23) while another person holds it |
| `speccoder claim US2 --force` | take it over (they are not told: talk to them) |
| `speccoder claim US2 --release` | let yours go; `--force` removes another's |

Your git `user.email` is who you are, so you on two machines are one holder. Without it, nothing is claimed: `CLAIM-NO-IDENTITY`. When the remote cannot be reached, `claim` fails (`CLAIM-UNREACHABLE`), but a run goes on unclaimed and says so once: claims never stop your work offline.

## At a pull request

`speccoder pr` stops when another person holds a story your branch has work on (`CLAIMED`): two people did one story. Talk first; `--force` hands it off anyway.

## In the web app

**Needs You** shows *US2 is Dana's* when your next task is in a story someone else holds, with **Take It Over** behind a confirmation. On the **Overview**, a feature card says *Dana has US2*. Both read your repository's copy of the claims, as last fetched.

## Feature numbers that never clash

Spec Kit numbers features `001`, `002`, … by default, so two people on two branches both make `005`. Projects made with `speccoder new` use Spec Kit's timestamp numbering instead (`20261116-101500-export`), which never clashes. Select a feature by its name without the number: `speccoder feature export`. In a project that already has two features with one number, speccoder warns (`discover.same-number`) and asks which one you mean instead of picking. To switch an existing project, set `"feature_numbering": "timestamp"` in `.specify/init-options.json`.
