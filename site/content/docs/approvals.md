+++
title = "Approvals and controls"
description = "Decide calls inline, in an editor or later; how decisions are scoped and recorded with who made them; frozen tests; and accepting, skipping and nudging tasks."
weight = 70

[extra]
group = "Use"
+++

## What needs a person

Every tool call is classified before it runs. The full policy is on the [security page](@/docs/security.md#the-policy); in short:

- **denied, always** — the spec, your check configuration, `.git`, secrets, privilege escalation, git remotes, paths outside the worktree;
- **needs a person** — dependency manifests, existing tests, shells, network tools, recursive deletes, git commands that change state, unlisted commands, MCP tools you have not allowed, and any command that asks for the network;
- **allowed** — reading, writing ordinary source files, your checks and `[commands] allow`.

No approval lifts a denial.

## Inline

In a foreground run at a terminal — in the [dashboard](@/docs/running.md#the-dashboard), or with `--plain` as a prompt — speccoder asks while the attempt waits — one question, with what the call does, why it stopped, what it can reach, what the model claimed, and exactly what each answer covers:

```text
△ Approval needed — 001-slugify/T004 Handle Unicode input · turn 8
  Edit this file?  Cargo.toml                                         risk ■■□ medium
      3     3
      4     4   [dependencies]
      5     5   serde = "1"
            6 + unicode-normalization = "0.1"
  Why stopped  `Cargo.toml` is a dependency manifest
  Sandbox      a file write — nothing is executed
  Model says   "NFKD normalisation is needed for the Unicode acceptance test"   (a claim)
  1. Yes, once                                                          (y)
  2. Yes, for the rest of T004: this path, any content                  (t)
  3. Yes, for this run: this path, any content, in any task             (r)
  4. No, and tell the model why …                                       (n)
  5. Stop this task here (it waits; decide later)                       (s)
  y t r n s or 1-5 · add a note: "y <note>" · v view all · esc later  >
```

An edit shows a diff with line numbers, a new file its first lines, a command the whole argv (never cut) with its working directory and network, an MCP call its arguments. `v` shows everything. What the model wrote is printed with control characters escaped, so it cannot drive your terminal; its explanation is labelled a claim.

- `y` runs the call once; `t` also approves its class for the rest of the task; `r` for every later task of this run (never offered for high-risk calls).
- A note after the key (`y use the 0.1 series`) is passed to the model with the call's result.
- `n` asks for a reason — required — which the model is told (`n no new dependencies` gives it at once).
- `s` stops the task here: the attempt waits, just as it does in an unattended run, for `speccoder task approve` or `task reject`.
- `esc` decides later: nothing changes, the call keeps waiting. In the dashboard the overlay closes and the footer offers it again (`enter`).

The dashboard's overlay takes the same keys, one keystroke each: `tab` adds a note before `y`, `t` or `r`; `n` asks for the reason; `v` shows the whole body, `↑ ↓` scroll it. Decisions made there are recorded `via dashboard`. In the [web interface](@/docs/web.md#when-a-run-needs-you) the call opens as a sheet with the same keys, and calls stopped earlier are approved or rejected from its Report; those decisions are recorded `via web`.

The attempt continues in the same turn, and a "for the task" or "for this run" answer also covers the same class later in that attempt. Take your time: a waiting prompt does not cost the run anything.

## Later

With `--no-prompt`, without a terminal, with `--json`, or when you answer `s`, the attempt **waits** at the call. Nothing is thrown away: its working tree and everything it did so far are kept, and the run ends `needs_approval`.

```sh
speccoder report --approvals                                       # what waits — shown as the prompt shows it — since when, until when
speccoder task approve T003 [--note "…"] [--scope once|task|run]
speccoder task reject T003 --reason "no new dependencies"
speccoder run --task T003                                 # the attempt continues from that call
```

`task approve` and `task reject` need no model or key. The next `run` continues the **same** attempt: model calls and commands that had finished are read back from the journal, not repeated, and the call that runs is exactly the one you were shown. A rejection reaches the model with the call's result, as it does inline.

- `--call <id>` picks one waiting call by its id (or a prefix of it) when there are several; `--all` decides every waiting call of the task, or of the feature, each recorded on its own.
- While one task's attempt waits, the feature's other tasks do not start: they share its worktree. `run` says which call waits and how to decide it.
- A waiting call has a deadline, `[approvals] wait` (default `7d`). When it passes, the call is **denied as expired** — recorded as an expiry, never as anybody's decision — and the attempt continues with the refusal.
- Parallel lanes still stop instead of waiting: their worktree is merged or dropped with the group. Such a stop, and one made before this release, is listed with its saved attempt; deciding it lets the task's next attempt make the call.
- `speccoder report --live` lists what waits, and since when.

## What a decision covers

A decision covers a **class**, not exact bytes — once, for one task, or (inline only) for this run:

| Call | Class |
|---|---|
| a write | the path, with any content |
| a command | its exact argv |
| a command with the network | its exact argv, with the network |
| an MCP tool | the tool |

Decisions travel in each attempt's recorded input — "for this run" grants included — so a replay sees exactly what the live run saw and asks nobody. A run grant ends with the run; a rejection for the task and every policy denial still win over it.

## Who decided

Every decision is a record in the run's journal: **who made it, how speccoder knows that name, through what, when, and what exactly was shown**.

- The name is git's `user.name` in the project (what your commits say), else your operating-system user, else `unknown` — never a guess. `task accept`'s `Accepted-By` uses the same name.
- How it is known: `asserted` for an answer at the terminal, in the dashboard, in the web interface or with `speccoder task approve`/`task reject`, `connected` for one an editor reports over ACP (`via editor Zed`). speccoder authenticates nobody, so it never claims more.
- An approval holds only for the call it was shown: a decision bound to anything else is refused and the call is presented again.
- A call allowed or denied without asking names the **rule** that decided it (``allowed command `cargo test` ``, `workspace write`, a grant for the task), never a person; an expired call says `expired`.

The run report shows it per call (`by`, `operator`, `via`), `speccoder report --approvals` lists it, and strict replay reads every decision back without asking anyone.

## Frozen tests

Tests that existed before an attempt are **frozen**: changing them needs approval, because weakening a test is the easiest way to make a check pass. Adding a test to an existing file is common and harmless, so you can allow additions:

```toml
[tests]
frozen = "additions"   # new lines may be added; changing or removing existing lines still asks
```

## Human controls

```sh
speccoder task accept T004 --reason "done by hand in PR 12"   # tick on your word: shown as attested
speccoder task skip T006 --reason "out of scope"            # runs pass it by; dependents stay blocked
speccoder task skip T006 --undo
speccoder task nudge T005 --note "keep the public API unchanged"   # the next attempts read it
speccoder task nudge T005 --clear
```

An attested task is shown as `attested`, never as `verified`: the report always says whose word a tick rests on.
