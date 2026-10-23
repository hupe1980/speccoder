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

In a foreground run at a terminal, speccoder asks while the attempt waits:

```text
speccoder needs your approval — 001-slugify/T003 (risk: medium)
  write `Cargo.toml` — `Cargo.toml` is a dependency manifest
  │ - [package] …
  │ + description = "URL slugs" …
  [y] once  [t] for this task  [n] reject  >
```

- `y` runs the call once.
- `t` also approves the call's class for the rest of the task.
- `n` asks for a reason, which the model is told.

The attempt continues in the same turn. Take your time: a waiting prompt does not cost the run anything.

## Later

With `--no-prompt`, without a terminal, with `--json` or in a parallel lane, the attempt stops instead and keeps the pending call:

```sh
speccoder approvals                                       # what waits, and what was decided
speccoder approve --task T003                             # allow its class for this task
speccoder reject --task T003 --reason "no new dependencies"
speccoder run --task T003                                 # the next attempt may, or is told it may not
```

## What a decision covers

A decision covers a **class**, not exact bytes, for one task:

| Call | Class |
|---|---|
| a write | the path, with any content |
| a command | its exact argv |
| a command with the network | its exact argv, with the network |
| an MCP tool | the tool |

Decisions travel in each attempt's recorded input, so a replay sees exactly what the live run saw.

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
