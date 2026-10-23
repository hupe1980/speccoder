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

In a foreground run at a terminal, speccoder asks while the attempt waits — one question, with what the call does, why it stopped, what it can reach, what the model claimed, and exactly what each answer covers:

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
  5. Stop this task here (save the attempt, decide later)               (s)
  y t r n s or 1-5 · add a note: "y <note>" · v view all · esc later  >
```

An edit shows a diff with line numbers, a new file its first lines, a command the whole argv (never cut) with its working directory and network, an MCP call its arguments. `v` shows everything. What the model wrote is printed with control characters escaped, so it cannot drive your terminal; its explanation is labelled a claim.

- `y` runs the call once; `t` also approves its class for the rest of the task; `r` for every later task of this run (never offered for high-risk calls).
- A note after the key (`y use the 0.1 series`) is passed to the model with the call's result.
- `n` asks for a reason — required — which the model is told (`n no new dependencies` gives it at once).
- `s` stops the task here: the attempt is saved and the call waits for `speccoder approve`, exactly as an unattended run would.
- `esc` decides later: nothing changes, the prompt waits.

The attempt continues in the same turn, and a "for the task" or "for this run" answer also covers the same class later in that attempt. Take your time: a waiting prompt does not cost the run anything.

## Later

With `--no-prompt`, without a terminal, with `--json` or in a parallel lane, the attempt stops instead and keeps the pending call:

```sh
speccoder approvals                                       # what waits — shown as the prompt shows it — and what was decided
speccoder approve --task T003 [--note "…"]                # allow its class for this task; the note goes to the next attempt
speccoder reject --task T003 --reason "no new dependencies"
speccoder run --task T003                                 # the next attempt may, or is told it may not
```

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

Every decision records **who made it, through what, and when**: an answer at the terminal as your operating-system user (`via terminal`), one in an editor as your user through that editor (`via editor Zed`), and `speccoder approve` / `reject` as your user `via cli`. The run report shows it (`approved by dana (terminal)`), `speccoder approvals` lists it, and the state directory keeps the full history. Without a user name the record says `unknown` — never a guess.

## Frozen tests

Tests that existed before an attempt are **frozen**: changing them needs approval, because weakening a test is the easiest way to make a check pass. Adding a test to an existing file is common and harmless, so you can allow additions:

```toml
[tests]
frozen = "additions"   # new lines may be added; changing or removing existing lines still asks
```

## Human controls

```sh
speccoder accept --task T004 --reason "done by hand in PR 12"   # tick on your word: shown as attested
speccoder skip   --task T006 --reason "out of scope"            # runs pass it by; dependents stay blocked
speccoder skip   --task T006 --undo
speccoder nudge  --task T005 --note "keep the public API unchanged"   # the next attempts read it
speccoder nudge  --task T005 --clear
```

An attested task is shown as `attested`, never as `verified`: the report always says whose word a tick rests on.
