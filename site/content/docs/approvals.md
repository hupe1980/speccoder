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

In a foreground run at a terminal — in the [dashboard](@/docs/running.md#the-dashboard), or as a prompt with `--plain` — speccoder asks while the attempt waits: what the call does, why it stopped, what it can reach, what the model claimed, and what each answer covers.

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

An edit shows a diff with line numbers, a new file its first lines, a command its whole argv with working directory and network, an MCP call its arguments; `v` shows everything. What the model wrote is printed with control characters escaped, and its explanation is labelled a claim.

| Key | Does |
|---|---|
| `y` | Runs the call once |
| `t` | Also approves its class for the rest of the task |
| `r` | Also for every later task of this run — never offered for high-risk calls |
| `n` | Rejects; the reason is required and the model is told it (`n no new dependencies`) |
| `s` | Stops the task: the attempt waits for `task approve` or `task reject`, as in an unattended run |
| `esc` | Decides later: the call keeps waiting; in the dashboard the footer offers it again (`enter`) |

Neither `t` nor `r` is offered when the call's class cannot be shown in full. A note after the key (`y use the 0.1 series`) is passed to the model with the call's result.

The dashboard's overlay takes the same keys, one keystroke each: `tab` adds a note before `y`, `t` or `r`, and `↑ ↓` scroll the body. In the [web interface](@/docs/web.md#when-a-run-needs-you) the call opens as a sheet with the same keys; for a command, `a` (**Always Allow…**) also adds its exact argv to `[commands] allow` (or `[sandbox] network_allow`) in `speccoder.toml`, in force once you commit it. Calls stopped earlier are approved or rejected from the web interface's Report.

The attempt continues in the same turn, and a "for the task" or "for this run" answer also covers the same class later in that attempt. A waiting prompt costs the run nothing.

## Later

With `--no-prompt`, `--json`, without a terminal, or after `s`, the attempt **waits** at the call. Its working tree and everything it did so far are kept, and the run ends `needs_approval`.

```sh
speccoder report --approvals                     # what waits, since and until when
speccoder task approve T003 [--note "…"] [--scope once|task|run]   # default: task
speccoder task reject T003 --reason "no new dependencies"
speccoder run --task T003                        # the attempt continues from that call
```

`task approve` and `task reject` need no model or key. The next `run` continues the **same** attempt: finished model calls and commands are read back from the journal, not repeated, and the call that runs is exactly the one you were shown.

- `--call <id>` picks one waiting call by its id (or a prefix) when there are several; `--all` decides every waiting call of the task, or of the feature, each recorded on its own.
- While one task's attempt waits, the feature's other tasks do not start: they share its worktree.
- A waiting call expires after `[approvals] wait` (default `7d`): it is **denied as expired** — recorded as an expiry, never as anybody's decision — and the attempt continues with the refusal.
- Parallel lanes stop instead of waiting, because their worktree is merged or dropped with the group. The stop is listed with its saved attempt; deciding it lets the task's next attempt make the call.

## What a decision covers

A decision covers a **class**, not exact bytes — once, for the task, or for the run:

| Call | Class |
|---|---|
| a write | the path, with any content |
| a command | its exact argv |
| a command with the network | its exact argv, with the network |
| an MCP tool | the tool |

Decisions travel in each attempt's recorded input, run grants included, so a replay sees what the live run saw and asks nobody. A run grant ends with the run; a rejection for the task and every policy denial win over it.

## Who decided

Every decision is a record in the run's journal: **who made it, how speccoder knows that name, through what, when, and what exactly was shown**.

- The name is git's `user.name` in the project, else your operating-system user, else `unknown`. `task accept`'s `Accepted-By` uses the same name.
- How it is known: `asserted` for an answer at the terminal, in the dashboard, in the web interface or with `task approve`/`task reject`; `connected` for one an editor reports over ACP (`via editor Zed`). speccoder authenticates nobody, so it never claims more.
- An approval holds only for the call it was shown: a decision bound to anything else is refused and the call is presented again.
- A call allowed or denied without asking names the **rule** that decided it (``allowed command `cargo test` ``, `workspace write`, a grant for the task), never a person; an expired call says `expired`.

The run report shows it per call (`by`, `operator`, `via`), `report --approvals` lists it, and strict replay reads every decision back without asking anyone.

## Frozen tests

Tests that existed before an attempt are **frozen**: changing them needs approval, because weakening a test is the easiest way to make a check pass. To let attempts add to existing test files without asking:

```toml
[tests]
frozen = "additions"   # default "ask"; changing or removing existing lines still asks
```

Removing a test, or renaming one, is refused outright, also with approval and also for tests inside a source file (`#[cfg(test)]` in `src/lib.rs`): the model is told to keep it, or to report the task blocked when the test contradicts the spec. An obsolete test is yours to remove, in a commit of your own.

## Human controls

```sh
speccoder task accept T004 --reason "done by hand in PR 12"   # tick on your word: shown as attested
speccoder task skip T006 --reason "out of scope"              # runs pass it by; dependents stay blocked
speccoder task skip T006 --undo
speccoder task nudge T005 --note "keep the public API unchanged"   # the next attempts read it
speccoder task nudge T005 --clear
```

An attested task is shown as `attested`, never as `verified`: the report always says whose word a tick rests on.
